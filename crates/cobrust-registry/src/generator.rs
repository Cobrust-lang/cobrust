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
//!
//! # W4 additions (ADR-0065 §7.4)
//!
//! - [`WheelEntry`] gains `cobrust_abi_version: u32` (default 1) and
//!   `experimental: bool` (SVE wheels are `true`).
//! - [`fetch_sha256sums`] downloads and parses the `SHA256SUMS` release asset,
//!   populating `WheelEntry::sha256` that was `""` in W3.
//! - [`generate_index`] accepts an optional SHA map and marks SVE wheels as
//!   experimental.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Current ABI version stamped into every freshly-generated wheel entry.
///
/// Mirrors `cobrust_pkg::wheel_select::COBRUST_ABI_VERSION`. Both constants
/// must be bumped together when the ABI changes.
pub const GENERATOR_ABI_VERSION: u32 = 1;

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
    /// Asset filename as uploaded (e.g. `cobrust-coil-0.1.0-x86_64-...-v3.tar.gz`).
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
/// `{ triple, cpu_level, sha256, url, size, cobrust_abi_version, experimental }`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WheelEntry {
    /// Target triple (e.g. `x86_64-unknown-linux-gnu`).
    pub triple: String,
    /// CPU level suffix (e.g. `v3`, `neon`, `m1`).
    pub cpu_level: String,
    /// SHA-256 of the wheel archive (lowercase hex). Populated from the
    /// `SHA256SUMS` release asset via [`fetch_sha256sums`] (W4); empty string
    /// only when that asset is absent.
    pub sha256: String,
    /// HTTP(S) download URL.
    pub url: String,
    /// Archive size in bytes.
    pub size: u64,
    /// Numeric ABI version for dependency-closure compatibility (ADR-0065 §6.4).
    /// Default for all v0.4.0+ wheels is `1` ([`GENERATOR_ABI_VERSION`]).
    #[serde(default = "default_abi_version")]
    pub cobrust_abi_version: u32,
    /// If `true` this wheel is experimental and must not be auto-selected
    /// without `--allow-experimental` (ADR-0065 §3.1 / §6.5).
    /// Currently `true` for `cpu_level == "sve"`.
    #[serde(default)]
    pub experimental: bool,
}

fn default_abi_version() -> u32 {
    GENERATOR_ABI_VERSION
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

/// Download and parse the `SHA256SUMS` release asset from the same asset list.
///
/// Looks for an asset named `SHA256SUMS` (exact match) in `assets`; if found,
/// downloads its content and parses each line as `<hex>  <filename>` (the
/// format produced by `sha256sum *.tar.gz > SHA256SUMS`).
///
/// Returns a map from archive filename → lowercase hex SHA-256. Returns an
/// empty map when no `SHA256SUMS` asset is present (graceful degradation:
/// the generator still works without it; wheel entries will have `sha256 = ""`).
///
/// # Errors
/// Returns [`Error::Http`] on transport failure or [`Error::BadStatus`] if the
/// download URL returns non-2xx.
pub fn fetch_sha256sums(assets: &[ReleaseAsset]) -> Result<HashMap<String, String>, Error> {
    let sums_asset = assets.iter().find(|a| a.name == "SHA256SUMS");
    let Some(asset) = sums_asset else {
        return Ok(HashMap::new());
    };

    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!(
            "cobrust-registry-gen/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/Cobrust-lang/cobrust)"
        ))
        .timeout(Duration::from_secs(30))
        .build()?;

    let resp = client.get(&asset.browser_download_url).send()?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::BadStatus {
            status: status.as_u16(),
            url: asset.browser_download_url.clone(),
        });
    }

    let text = resp.text()?;
    Ok(parse_sha256sums_text(&text))
}

/// Parse the text content of a SHA256SUMS file into a filename → hex map.
///
/// Each line format: `<64-hex-chars>  <filename>` (two spaces, as produced by
/// `sha256sum`). Lines that don't match are silently skipped.
#[must_use]
fn parse_sha256sums_text(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let mut parts = line.splitn(2, |c: char| c.is_whitespace());
        let hex = match parts.next() {
            Some(h) if h.len() == 64 => h.to_ascii_lowercase(),
            _ => continue,
        };
        let filename = match parts.next() {
            Some(f) => f.trim().to_owned(),
            None => continue,
        };
        map.insert(filename, hex);
    }
    map
}

/// Parse a release asset filename and extract `(triple, cpu_level)` if it
/// matches the Cobrust wheel naming convention.
///
/// Pattern: `cobrust-<pkg>-<version>-<triple>-<cpu_level>.tar.gz`
///
/// Known cpu_level values (§3.2): `v1`, `v3`, `v4`, `neon`, `sve`, `m1`, `m2`.
///
/// Returns `None` for any asset that does not match (e.g. `SHA256SUMS`,
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
/// `sha256_map` — result of [`fetch_sha256sums`]; if the map contains an
/// entry for the asset filename, the `sha256` field is populated. Pass an
/// empty map when SHA256SUMS is unavailable.
///
/// SVE wheels (`cpu_level == "sve"`) are automatically marked `experimental =
/// true` per ADR-0065 §3.1 / §6.5. All other wheels are `experimental =
/// false`.
#[must_use]
pub fn generate_index<S: std::hash::BuildHasher>(
    pkg: &str,
    version: &str,
    assets: &[ReleaseAsset],
    sha256_map: &HashMap<String, String, S>,
) -> Index {
    let wheels: Vec<WheelEntry> = assets
        .iter()
        .filter_map(|a| {
            let (triple, cpu_level) = parse_wheel_asset(&a.name)?;
            let sha256 = sha256_map.get(&a.name).cloned().unwrap_or_default();
            let experimental = cpu_level == "sve";
            Some(WheelEntry {
                triple,
                cpu_level,
                sha256,
                url: a.browser_download_url.clone(),
                size: a.size,
                cobrust_abi_version: GENERATOR_ABI_VERSION,
                experimental,
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
        assert!(parse_wheel_asset("SHA256SUMS").is_none());
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
            "SHA256SUMS",
            "https://example.com/SHA256SUMS",
            512,
        ));

        let index = generate_index("hello-cb", "0.1.0", &assets, &HashMap::new());

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

    #[test]
    fn generate_index_sve_marked_experimental() {
        let assets = vec![
            make_asset(
                &wheel_name("test-cb", "0.1.0", "aarch64-unknown-linux-gnu", "neon"),
                "https://example.com/neon.tar.gz",
                1024,
            ),
            make_asset(
                &wheel_name("test-cb", "0.1.0", "aarch64-unknown-linux-gnu", "sve"),
                "https://example.com/sve.tar.gz",
                1024,
            ),
        ];
        let index = generate_index("test-cb", "0.1.0", &assets, &HashMap::new());
        let neon = index
            .wheels
            .iter()
            .find(|w| w.cpu_level == "neon")
            .expect("neon wheel must be in index");
        let sve = index
            .wheels
            .iter()
            .find(|w| w.cpu_level == "sve")
            .expect("sve wheel must be in index");
        assert!(!neon.experimental, "neon must NOT be experimental");
        assert!(sve.experimental, "sve MUST be experimental");
    }

    #[test]
    fn generate_index_sha256_map_populated() {
        let asset_name = wheel_name("test-cb", "0.1.0", "x86_64-unknown-linux-gnu", "v3");
        let assets = vec![make_asset(
            &asset_name,
            "https://example.com/v3.tar.gz",
            2048,
        )];
        let expected_sha = "a".repeat(64);
        let mut sha_map = HashMap::new();
        sha_map.insert(asset_name, expected_sha.clone());
        let index = generate_index("test-cb", "0.1.0", &assets, &sha_map);
        assert_eq!(index.wheels[0].sha256, expected_sha);
    }

    #[test]
    fn generate_index_abi_version_stamped() {
        let assets = vec![make_asset(
            &wheel_name("test-cb", "0.1.0", "x86_64-unknown-linux-gnu", "v1"),
            "https://example.com/v1.tar.gz",
            1024,
        )];
        let index = generate_index("test-cb", "0.1.0", &assets, &HashMap::new());
        assert_eq!(index.wheels[0].cobrust_abi_version, GENERATOR_ABI_VERSION);
    }

    #[test]
    fn parse_sha256sums_text_parses_two_entries() {
        let content = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  cobrust-test-0.1.0-x86_64-unknown-linux-gnu-v3.tar.gz\n\
                       deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef  cobrust-test-0.1.0-aarch64-unknown-linux-gnu-neon.tar.gz\n";
        let map = parse_sha256sums_text(content);
        assert_eq!(
            map.get("cobrust-test-0.1.0-x86_64-unknown-linux-gnu-v3.tar.gz"),
            Some(&"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned())
        );
        assert_eq!(
            map.get("cobrust-test-0.1.0-aarch64-unknown-linux-gnu-neon.tar.gz"),
            Some(&"deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_owned())
        );
        assert_eq!(map.len(), 2);
    }
}
