//! Index generator — GitHub Releases asset scan → `wheels.json` per ADR-0065 §3.4.
//!
//! Three public stages:
//! 1. [`fetch_release_assets`] — query the GitHub Releases API for a tagged release.
//! 2. [`parse_wheel_asset`] — regex-style parse of an asset filename into
//!    `(triple, cpu_level)`.
//! 3. [`generate_index`] — assemble a typed [`Index`] from the asset list.
//! 4. [`write_index_json`] — serialize the index to disk.
//!
//! GitHub is used as the static CDN for release assets per §3.4. The
//! `GITHUB_TOKEN` env var is read for authenticated requests (higher rate
//! limit); the generator still works unauthenticated for public repos.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from the generator pipeline.
#[derive(Debug, Error)]
pub enum Error {
    /// HTTP transport or GitHub API error.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// GitHub API returned a non-2xx status.
    #[error(
        "GitHub API returned status {status} for {url}\n  suggestion: check the version tag exists and the repo is public (or set GITHUB_TOKEN)"
    )]
    BadStatus {
        /// HTTP status code returned.
        status: u16,
        /// URL that returned the bad status.
        url: String,
    },
    /// JSON deserialization of the GitHub API response failed.
    #[error("GitHub API JSON parse error: {0}")]
    Parse(#[from] serde_json::Error),
    /// Filesystem I/O failure writing index JSON.
    #[error("io error writing {path}: {source}")]
    Io {
        /// Destination path being written.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

// ── GitHub Releases API types ────────────────────────────────────────────────

/// A GitHub Releases API asset entry (subset of fields we consume).
#[derive(Clone, Debug, Deserialize)]
pub struct ReleaseAsset {
    /// Asset filename as uploaded (e.g. `cobrust-numpy-0.1.0-x86_64-...-v3.tar.gz`).
    pub name: String,
    /// Direct download URL (`browser_download_url` in the API response).
    pub browser_download_url: String,
    /// Asset size in bytes (`size` in the API response).
    pub size: u64,
}

/// Subset of the GitHub Releases API release object.
#[derive(Debug, Deserialize)]
struct GhRelease {
    assets: Vec<GhAsset>,
}

/// Raw GitHub asset entry from the API JSON.
#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

// ── Index wire types ─────────────────────────────────────────────────────────

/// One wheel entry in the generated `wheels.json`.
///
/// Maps to the §3.4 JSON shape:
/// `{ triple, cpu_level, sha256, url, size }`.
/// `sha256` is the hex digest of the wheel archive. When generating from the
/// GitHub Releases API (which does not expose SHA-256 in the asset metadata),
/// the field is set to the empty string `""` as a placeholder; the CI
/// post-processing step that calls this generator must patch the SHA values
/// after downloading the assets. This behaviour is documented as a known gap
/// in ADR-0065 §7.3 — W4 will add SHA computation to the release pipeline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WheelEntry {
    /// Target triple (e.g. `x86_64-unknown-linux-gnu`).
    pub triple: String,
    /// CPU level suffix (e.g. `v3`, `neon`, `m2`).
    pub cpu_level: String,
    /// SHA-256 of the wheel archive (lowercase hex). May be `""` if not yet
    /// computed (see struct-level doc).
    pub sha256: String,
    /// HTTP(S) download URL.
    pub url: String,
    /// Archive size in bytes.
    pub size: u64,
}

/// The canonical `wheels.json` index for one package version.
///
/// Serializes to the §3.4 shape:
/// `{ name, version, wheels: [...] }`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Index {
    /// Package name (e.g. `numpy-cb`).
    pub name: String,
    /// Package version string (e.g. `0.1.0`).
    pub version: String,
    /// Wheel entries — one per (triple, cpu_level) variant.
    pub wheels: Vec<WheelEntry>,
}

// ── Implementation ───────────────────────────────────────────────────────────

/// Fetch the release asset list from the GitHub Releases API for a given `repo`
/// and `version` tag.
///
/// `repo` is the `owner/name` form (e.g. `Cobrust-lang/cobrust`).
/// `version` is the semver string WITHOUT the `v` prefix (e.g. `0.4.0`);
/// the function prepends `v` to form the tag.
///
/// Reads `GITHUB_TOKEN` from the environment if present; falls back to
/// unauthenticated (60 req/h rate limit on public repos).
///
/// # Errors
/// Returns [`Error::Http`] on transport failure, [`Error::BadStatus`] if the
/// API returns a non-2xx status, or [`Error::Parse`] if the JSON cannot be
/// deserialized.
pub fn fetch_release_assets(repo: &str, version: &str) -> Result<Vec<ReleaseAsset>, Error> {
    let tag = format!("v{version}");
    let url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");

    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(concat!(
            "cobrust-registry-gen/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/Cobrust-lang/cobrust)"
        ))
        .timeout(Duration::from_secs(30))
        .build()?
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }

    let resp = builder.send()?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::BadStatus {
            status: status.as_u16(),
            url,
        });
    }

    let release: GhRelease = resp.json()?;
    let assets = release
        .assets
        .into_iter()
        .map(|a| ReleaseAsset {
            name: a.name,
            browser_download_url: a.browser_download_url,
            size: a.size,
        })
        .collect();
    Ok(assets)
}

/// Parse a release asset filename and extract `(triple, cpu_level)` if it
/// matches the Cobrust wheel naming convention.
///
/// Pattern: `cobrust-<pkg>-<version>-<triple>-<cpu_level>.tar.gz`
///
/// Known cpu_level values (§3.2): `v1`, `v3`, `v4`, `neon`, `sve`, `m1`, `m2`.
///
/// Returns `None` for any asset that does not match (e.g. `sha256sums.txt`,
/// source tarballs, or compiler binary packages that don't carry a cpu_level).
///
/// # Design note
/// We do not use the `regex` crate to keep the dependency tree minimal; the
/// pattern is simple enough for manual parsing.
#[must_use]
pub fn parse_wheel_asset(asset_name: &str) -> Option<(String, String)> {
    // Known cpu_level suffixes (longest match first to avoid ambiguity).
    // The cpu_level is always the LAST dash-separated segment.
    // Declared at top of function to avoid clippy::items_after_statements.
    const CPU_LEVELS: &[&str] = &["neon", "sve", "m1", "m2", "v1", "v3", "v4"];

    // Must start with "cobrust-" and end with ".tar.gz"
    let stem = asset_name
        .strip_prefix("cobrust-")?
        .strip_suffix(".tar.gz")?;

    // Split off the last segment.
    let (prefix, cpu_level_raw) = stem.rsplit_once('-')?;
    // Validate it's a known cpu_level.
    if !CPU_LEVELS.contains(&cpu_level_raw) {
        return None;
    }

    // prefix is now: <pkg>-<version>-<triple_possibly_with_dashes>
    // The triple is everything after the first two dash-segments (pkg + version).
    // pkg and version do not contain dashes in our convention; triple does
    // (e.g. `x86_64-unknown-linux-gnu`).
    //
    // Split: first dash = end of pkg, second dash = end of version.
    let after_pkg = prefix.splitn(3, '-').nth(2)?; // <version>-<triple>
    let (_, triple) = after_pkg.split_once('-')?; // <triple>

    // Minimal sanity: triple must contain at least one more dash.
    if !triple.contains('-') {
        return None;
    }

    Some((triple.to_owned(), cpu_level_raw.to_owned()))
}

/// Assemble a typed [`Index`] from a list of GitHub release assets.
///
/// Filters to only the assets that parse as valid wheel filenames (via
/// [`parse_wheel_asset`]). Non-wheel assets are silently skipped.
///
/// `sha256` is left empty (`""`) for each entry because the GitHub Releases
/// API does not expose SHA-256 in the asset metadata at this call site.
/// See [`WheelEntry::sha256`] for the documented gap.
#[must_use]
pub fn generate_index(pkg: &str, version: &str, assets: &[ReleaseAsset]) -> Index {
    let wheels: Vec<WheelEntry> = assets
        .iter()
        .filter_map(|a| {
            let (triple, cpu_level) = parse_wheel_asset(&a.name)?;
            Some(WheelEntry {
                triple,
                cpu_level,
                sha256: String::new(),
                url: a.browser_download_url.clone(),
                size: a.size,
            })
        })
        .collect();

    Index {
        name: pkg.to_owned(),
        version: version.to_owned(),
        wheels,
    }
}

/// Serialize `index` to JSON and write to `path`.
///
/// Creates parent directories if absent. Overwrites any existing file at
/// `path`.
///
/// # Errors
/// Returns [`Error::Parse`] if JSON serialization fails (in practice this is
/// infallible for well-typed inputs), or [`Error::Io`] on filesystem failure.
pub fn write_index_json(index: &Index, path: &Path) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let json = serde_json::to_string_pretty(index)?;
    std::fs::write(path, json).map_err(|e| Error::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_asset(name: &str, url: &str, size: u64) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_owned(),
            browser_download_url: url.to_owned(),
            size,
        }
    }

    /// Helper: build the canonical wheel asset name.
    fn wheel_name(pkg: &str, ver: &str, triple: &str, cpu: &str) -> String {
        format!("cobrust-{pkg}-{ver}-{triple}-{cpu}.tar.gz")
    }

    #[test]
    fn parse_wheel_asset_x86_64_gnu_v3() {
        let name = wheel_name("numpy-cb", "0.1.0", "x86_64-unknown-linux-gnu", "v3");
        let result = parse_wheel_asset(&name);
        assert_eq!(
            result,
            Some(("x86_64-unknown-linux-gnu".to_owned(), "v3".to_owned()))
        );
    }

    #[test]
    fn parse_wheel_asset_unrelated_skips() {
        // Non-wheel assets must return None.
        assert!(parse_wheel_asset("sha256sums.txt").is_none());
        assert!(parse_wheel_asset("cobrust-0.4.0-x86_64-unknown-linux-gnu.tar.gz").is_none());
        assert!(parse_wheel_asset("README.md").is_none());
    }

    #[test]
    fn generate_index_round_trip() {
        // 9 wheel assets: gnu×3 + musl×2 + linux-aarch64×2 + apple-darwin×2 = 9, plus 1 non-wheel.
        let triples = [
            ("x86_64-unknown-linux-gnu", &["v1", "v3", "v4"][..]),
            ("x86_64-unknown-linux-musl", &["v1", "v3"][..]),
            ("aarch64-unknown-linux-gnu", &["neon", "sve"][..]),
            ("aarch64-apple-darwin", &["m1", "m2"][..]),
        ];
        let mut assets: Vec<ReleaseAsset> = Vec::new();
        for (triple, levels) in &triples {
            for cpu in *levels {
                assets.push(make_asset(
                    &wheel_name("hello-cb", "0.1.0", triple, cpu),
                    &format!("https://example.com/{triple}-{cpu}.tar.gz"),
                    1024,
                ));
            }
        }
        // Add one non-wheel asset that should be skipped.
        assets.push(make_asset(
            "sha256sums.txt",
            "https://example.com/sha256sums.txt",
            512,
        ));

        let index = generate_index("hello-cb", "0.1.0", &assets);

        // 9 wheel assets, 1 non-wheel skipped → 9 wheel entries.
        assert_eq!(
            index.wheels.len(),
            9,
            "expected 9 wheel entries, got {}",
            index.wheels.len()
        );
        assert_eq!(index.name, "hello-cb");
        assert_eq!(index.version, "0.1.0");

        // Round-trip through JSON.
        let json = serde_json::to_string(&index).expect("serialize");
        let back: Index = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(index, back);
    }
}
