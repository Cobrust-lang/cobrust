//! Source backends (ADR-0026 §E "Source resolvers").
//!
//! Three source kinds are in M12 scope:
//!
//! - `Source::Path` — relative or absolute filesystem path.
//! - `Source::Git` — `git clone --depth=1 --branch=<rev>` then fall back
//!   to plain clone + checkout if `--branch` rejects a SHA. The system
//!   `git` CLI is invoked (no `git2` / `libgit2` dep — minimal binary).
//! - `Source::Registry` — at M12 returns `Offline` unless the entry is
//!   already in the cache. Phase F adds HTTP fetch under a separate ADR.
//!
//! All three normalize through `Registry::insert_source_tree` so the
//! cache is the single source of truth for "this is the on-disk
//! installed version" — same content, same hex, regardless of where
//! it came from.

use std::path::{Path, PathBuf};
use std::process::Command;

use semver::VersionReq;

use crate::error::{PkgError, RegistryError, SourceError};
use crate::registry::Registry;

/// Source spec: where to fetch a dependency from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Filesystem path. Resolved relative to the manifest's directory.
    Path { path: PathBuf },
    /// Git URL + revision (SHA, tag, or ref).
    Git { url: String, rev: String },
    /// Registry-mediated.
    Registry { name: String, version: VersionReq },
}

/// Result of resolving + caching a source.
#[derive(Clone, Debug)]
pub struct SourceFetchOutput {
    /// Path under `<registry>/blake3/<hex>/`.
    pub local_path: PathBuf,
    /// `blake3:<hex>` of the canonical tarball.
    pub blake3_hex: String,
    /// `provenance_hash` from `PROVENANCE.toml` if the package carries one
    /// (translated crates do; raw user crates don't).
    pub provenance_hash: Option<String>,
}

impl Source {
    /// Resolve + cache the source. The first call extracts; subsequent
    /// calls hit the content-addressed cache.
    pub fn fetch(
        &self,
        registry: &Registry,
        workspace_root: &Path,
    ) -> Result<SourceFetchOutput, PkgError> {
        match self {
            Self::Path { path } => fetch_path(path, registry, workspace_root),
            Self::Git { url, rev } => fetch_git(url, rev, registry),
            Self::Registry { name, version } => fetch_registry(name, version, registry),
        }
    }
}

fn fetch_path(
    raw: &Path,
    registry: &Registry,
    workspace_root: &Path,
) -> Result<SourceFetchOutput, PkgError> {
    let resolved: PathBuf = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        workspace_root.join(raw)
    };

    if !resolved.exists() {
        return Err(PkgError::Source(SourceError::PathMissing(resolved)));
    }
    if !resolved.is_dir() {
        return Err(PkgError::Source(SourceError::PathNotDirectory(resolved)));
    }

    let entry = registry.insert_source_tree(&resolved)?;
    let provenance_hash = read_provenance_hash(&entry.local_path);
    Ok(SourceFetchOutput {
        local_path: entry.local_path,
        blake3_hex: entry.blake3_hex,
        provenance_hash,
    })
}

fn fetch_git(url: &str, rev: &str, registry: &Registry) -> Result<SourceFetchOutput, PkgError> {
    // M8 security fix: validate that url and rev cannot be mistaken for flags.
    // A branch/tag name like `--upload-pack=evil` would be passed directly to
    // `git clone --branch <rev>`, letting an attacker inject arbitrary git
    // options via a crafted dependency spec. Reject any rev or url that starts
    // with `-`.
    if rev.starts_with('-') {
        return Err(PkgError::Source(SourceError::AdversarialRef(
            rev.to_string(),
        )));
    }
    if url.starts_with('-') {
        return Err(PkgError::Source(SourceError::AdversarialRef(
            url.to_string(),
        )));
    }

    let staging = tempdir_for_git(rev)?;
    let staging_path = staging.path();

    // `--` separates git options from positional arguments. Without it, a URL
    // or rev that looks like a flag (e.g. `--upload-pack=evil`) would be parsed
    // as a git option even after the checks above (e.g. via path components).
    // Try the shallow + branched form first; some refs are SHAs and will
    // fail this. Fall back to plain clone + checkout.
    let shallow = Command::new("git")
        .arg("clone")
        .arg("--depth=1")
        .arg("--branch")
        .arg(rev)
        .arg("--")
        .arg(url)
        .arg(staging_path)
        .status();
    let shallow_ok = matches!(shallow, Ok(s) if s.success());

    if !shallow_ok {
        // Clean any partial state.
        let _ = std::fs::remove_dir_all(staging_path);
        let plain = Command::new("git")
            .arg("clone")
            .arg("--")
            .arg(url)
            .arg(staging_path)
            .status()
            .map_err(|e| PkgError::Source(SourceError::Git(format!("git clone {url}: {e}"))))?;
        if !plain.success() {
            return Err(PkgError::Source(SourceError::Git(format!(
                "git clone {url} failed (exit {plain:?})"
            ))));
        }
        let checkout = Command::new("git")
            .arg("-C")
            .arg(staging_path)
            .arg("checkout")
            .arg("--")
            .arg(rev)
            .status()
            .map_err(|e| PkgError::Source(SourceError::Git(format!("git checkout {rev}: {e}"))))?;
        if !checkout.success() {
            return Err(PkgError::Source(SourceError::Git(format!(
                "git checkout {rev} failed (exit {checkout:?})"
            ))));
        }
    }

    let entry = registry.insert_source_tree(staging_path)?;
    let provenance_hash = read_provenance_hash(&entry.local_path);
    Ok(SourceFetchOutput {
        local_path: entry.local_path,
        blake3_hex: entry.blake3_hex,
        provenance_hash,
    })
}

fn fetch_registry(
    name: &str,
    _version: &VersionReq,
    _registry: &Registry,
) -> Result<SourceFetchOutput, PkgError> {
    // M12 stub: registry sources are always Offline unless the entry
    // is already cached by hash (not by name). Phase F ADR will add
    // HTTP fetch + name→version index.
    Err(PkgError::Registry(RegistryError::Offline {
        name: name.to_string(),
    }))
}

/// Look for `PROVENANCE.toml` under the cache entry root. If present,
/// extract `deterministic_id` per ADR-0007 chain-of-custody.
fn read_provenance_hash(entry_path: &Path) -> Option<String> {
    let p = entry_path.join("PROVENANCE.toml");
    if !p.is_file() {
        return None;
    }
    let contents = std::fs::read_to_string(&p).ok()?;
    // Cheap parse: look for `deterministic_id = "..."`. Avoids a full
    // toml::from_str pull-in here; we trust translated crates' format.
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("deterministic_id") {
            // form: ` = "blake3:..."`
            let after_eq = rest.split_once('=')?.1.trim();
            return after_eq
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .map(str::to_string);
        }
    }
    None
}

/// Pick a temporary directory for git cloning. We deliberately don't
/// use the `tempfile::TempDir` here — git needs a directory that
/// persists across the clone command, and we control deletion explicitly
/// through the registry-insert path (which copies + extracts).
fn tempdir_for_git(rev: &str) -> Result<TempDirGuard, PkgError> {
    let base = std::env::temp_dir().join(format!("cobrust-pkg-git-{rev}-{}", std::process::id()));
    std::fs::create_dir_all(&base)
        .map_err(|e| PkgError::Io(format!("mkdir {}: {e}", base.display())))?;
    Ok(TempDirGuard { path: base })
}

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn make_source(dir: &Path) {
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("cobrust.toml"),
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\ncobrust-version = \"0.0.1\"\n[bin]\nname = \"a\"\npath = \"src/main.cb\"\n",
        )
        .unwrap();
        fs::write(dir.join("src/main.cb"), "fn main() -> i64:\n    return 0\n").unwrap();
    }

    #[test]
    fn path_source_success() {
        let registry_dir = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let dep_dir = workspace.path().join("dep");
        fs::create_dir(&dep_dir).unwrap();
        make_source(&dep_dir);

        let r = Registry::open_at(registry_dir.path()).unwrap();
        let s = Source::Path {
            path: PathBuf::from("dep"),
        };
        let out = s.fetch(&r, workspace.path()).unwrap();
        assert!(out.local_path.is_dir());
        assert!(out.blake3_hex.starts_with("blake3:"));
        assert!(out.provenance_hash.is_none());
    }

    #[test]
    fn path_source_missing() {
        let registry_dir = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let s = Source::Path {
            path: PathBuf::from("nonexistent"),
        };
        let err = s.fetch(&r, workspace.path()).unwrap_err();
        assert!(matches!(err, PkgError::Source(SourceError::PathMissing(_))));
    }

    #[test]
    fn path_source_not_a_directory() {
        let registry_dir = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let file = workspace.path().join("just-a-file.txt");
        fs::write(&file, "x").unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let s = Source::Path {
            path: PathBuf::from("just-a-file.txt"),
        };
        let err = s.fetch(&r, workspace.path()).unwrap_err();
        assert!(matches!(
            err,
            PkgError::Source(SourceError::PathNotDirectory(_))
        ));
    }

    #[test]
    fn path_source_with_provenance() {
        let registry_dir = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let dep_dir = workspace.path().join("dep");
        fs::create_dir(&dep_dir).unwrap();
        make_source(&dep_dir);
        fs::write(
            dep_dir.join("PROVENANCE.toml"),
            "deterministic_id = \"blake3:cafebabe1234\"\n",
        )
        .unwrap();

        let r = Registry::open_at(registry_dir.path()).unwrap();
        let s = Source::Path {
            path: PathBuf::from("dep"),
        };
        let out = s.fetch(&r, workspace.path()).unwrap();
        assert_eq!(out.provenance_hash.as_deref(), Some("blake3:cafebabe1234"));
    }

    #[test]
    fn registry_source_offline_at_m12() {
        let registry_dir = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let s = Source::Registry {
            name: "missing-from-cache".into(),
            version: VersionReq::parse("1.0").unwrap(),
        };
        let err = s.fetch(&r, workspace.path()).unwrap_err();
        assert!(matches!(
            err,
            PkgError::Registry(RegistryError::Offline { .. })
        ));
    }

    #[test]
    #[ignore = "requires network + git"]
    fn git_source_smoke() {
        let registry_dir = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let s = Source::Git {
            url: "https://github.com/octocat/Hello-World".into(),
            rev: "master".into(),
        };
        let _ = s.fetch(&r, workspace.path());
    }

    // ----------------------------------------------------------------
    // M8 adversarial corpus — git ref/url injection via flag-like values
    // ----------------------------------------------------------------

    #[test]
    fn git_adversarial_rev_upload_pack() {
        // A rev starting with `--` must be rejected before any process spawn.
        let registry_dir = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let s = Source::Git {
            url: "https://example.com/repo.git".into(),
            rev: "--upload-pack=evil".into(),
        };
        let err = s.fetch(&r, workspace.path()).unwrap_err();
        assert!(
            matches!(err, PkgError::Source(SourceError::AdversarialRef(_))),
            "expected AdversarialRef, got {err:?}"
        );
    }

    #[test]
    fn git_adversarial_rev_single_dash() {
        let registry_dir = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let s = Source::Git {
            url: "https://example.com/repo.git".into(),
            rev: "-b".into(),
        };
        let err = s.fetch(&r, workspace.path()).unwrap_err();
        assert!(
            matches!(err, PkgError::Source(SourceError::AdversarialRef(_))),
            "expected AdversarialRef, got {err:?}"
        );
    }

    #[test]
    fn git_adversarial_url_flag_like() {
        let registry_dir = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let s = Source::Git {
            url: "--config=core.hookspath=/evil".into(),
            rev: "main".into(),
        };
        let err = s.fetch(&r, workspace.path()).unwrap_err();
        assert!(
            matches!(err, PkgError::Source(SourceError::AdversarialRef(_))),
            "expected AdversarialRef, got {err:?}"
        );
    }
}
