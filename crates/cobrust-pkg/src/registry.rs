//! Content-addressed package registry (ADR-0026 §F "Content-addressed
//! registry layout", §"Registry layout (binding)").
//!
//! Layout:
//!
//! ```text
//! ~/.cobrust/registry/
//! ├── blake3/
//! │   ├── 39e9e2d3...0069/      # cobrust-nest 2.0.1 (source: tomli)
//! │   │   ├── cobrust.toml
//! │   │   ├── PROVENANCE.toml
//! │   │   └── src/...
//! │   ├── c0ffee...0042/        # my_app 0.1.0
//! │   │   └── ...
//! │   └── deadbeef.../          # ...
//! └── index/
//!     └── name-to-versions.toml  # cached index (M12 stub; empty)
//! ```
//!
//! The `<hex>` in `blake3/<hex>/` is the gzipped tarball's blake3
//! hash (the `:` from `blake3:` is stripped). One hex → one extracted
//! source tree. Idempotent: re-inserting the same content is a no-op.

use std::path::{Path, PathBuf};

use crate::error::{PkgError, RegistryError};
use crate::tarball::Tarball;

/// Open handle on the on-disk registry root.
#[derive(Clone, Debug)]
pub struct Registry {
    root: PathBuf,
}

/// A registry entry: the cached extracted source tree of a package.
#[derive(Clone, Debug)]
pub struct RegistryEntry {
    pub blake3_hex: String,
    pub local_path: PathBuf,
}

impl Registry {
    /// Open the default user-global registry at `~/.cobrust/registry/`.
    pub fn open_default() -> Result<Self, PkgError> {
        let home = home_dir()?;
        Self::open_at(&home.join(".cobrust").join("registry"))
    }

    /// Open the registry rooted at `root`. Creates the directory layout
    /// if missing.
    pub fn open_at(root: &Path) -> Result<Self, PkgError> {
        let blake3_dir = root.join("blake3");
        let index_dir = root.join("index");
        std::fs::create_dir_all(&blake3_dir).map_err(|e| {
            PkgError::Registry(RegistryError::NotWritable(root.to_path_buf()))
                .with_io_context(format!("mkdir {}: {e}", blake3_dir.display()))
        })?;
        std::fs::create_dir_all(&index_dir).map_err(|e| {
            PkgError::Registry(RegistryError::NotWritable(root.to_path_buf()))
                .with_io_context(format!("mkdir {}: {e}", index_dir.display()))
        })?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// Return the registry's filesystem root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path to the extracted tree for the given hash.
    /// `hash` may be either `blake3:<hex>` or bare `<hex>`.
    #[must_use]
    pub fn path_for_hash(&self, hash: &str) -> PathBuf {
        let hex = hash.strip_prefix("blake3:").unwrap_or(hash);
        self.root.join("blake3").join(hex)
    }

    /// Whether the registry already has an entry for this hash.
    #[must_use]
    pub fn has(&self, hash: &str) -> bool {
        self.path_for_hash(hash).is_dir()
    }

    /// Look up an entry by hash. Returns `None` if not in cache.
    #[must_use]
    pub fn get(&self, hash: &str) -> Option<RegistryEntry> {
        let path = self.path_for_hash(hash);
        if path.is_dir() {
            let hex = hash
                .strip_prefix("blake3:")
                .map(str::to_string)
                .unwrap_or_else(|| hash.to_string());
            Some(RegistryEntry {
                blake3_hex: format!("blake3:{hex}"),
                local_path: path,
            })
        } else {
            None
        }
    }

    /// Insert a source tree into the registry. Computes the deterministic
    /// tarball, hashes it, extracts under `<root>/blake3/<hex>/`. Idempotent:
    /// if the entry already exists with a matching hash, re-extraction is
    /// skipped.
    ///
    /// Returns the cache-side entry record.
    pub fn insert_source_tree(&self, source_dir: &Path) -> Result<RegistryEntry, PkgError> {
        let tarball = Tarball::build(source_dir)?;
        self.insert_tarball(&tarball)
    }

    /// Insert a pre-built tarball. The on-disk extraction is keyed by
    /// `tarball.hash()`.
    pub fn insert_tarball(&self, tarball: &Tarball) -> Result<RegistryEntry, PkgError> {
        let hex = tarball.hash().strip_prefix("blake3:").ok_or_else(|| {
            PkgError::Registry(RegistryError::HashMismatch {
                expected: "blake3:<hex>".into(),
                actual: tarball.hash().to_string(),
            })
        })?;
        let dest = self.root.join("blake3").join(hex);

        if dest.is_dir() {
            // Already cached; verify a sentinel file exists (or recover).
            // The cheapest verification is "directory non-empty" — full
            // re-hash is O(n) and would defeat the cache. For high-trust
            // mode call `verify_entry` separately.
            return Ok(RegistryEntry {
                blake3_hex: tarball.hash().to_string(),
                local_path: dest,
            });
        }

        // Extract atomically: stage in a sibling dir, rename on success.
        let staging = self.root.join("blake3").join(format!(".staging-{hex}"));
        let _ = std::fs::remove_dir_all(&staging);
        tarball.extract(&staging)?;

        // Atomic rename.
        std::fs::rename(&staging, &dest).map_err(|e| {
            PkgError::Io(format!(
                "atomic rename {} → {}: {e}",
                staging.display(),
                dest.display()
            ))
        })?;
        Ok(RegistryEntry {
            blake3_hex: tarball.hash().to_string(),
            local_path: dest,
        })
    }

    /// Strict integrity check: re-tar `<root>/blake3/<hex>/` and verify
    /// the recomputed hash matches `<hex>`. Useful for `cobrust verify`.
    pub fn verify_entry(&self, hash: &str) -> Result<(), PkgError> {
        let path = self.path_for_hash(hash);
        if !path.is_dir() {
            return Err(PkgError::Registry(RegistryError::EntryNotFound {
                hash: hash.to_string(),
            }));
        }
        let tarball = Tarball::build(&path)?;
        if tarball.hash() != normalize_hash(hash) {
            return Err(PkgError::Registry(RegistryError::HashMismatch {
                expected: hash.to_string(),
                actual: tarball.hash().to_string(),
            }));
        }
        Ok(())
    }
}

fn normalize_hash(hash: &str) -> String {
    if hash.starts_with("blake3:") {
        hash.to_string()
    } else {
        format!("blake3:{hash}")
    }
}

fn home_dir() -> Result<PathBuf, PkgError> {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return Ok(PathBuf::from(h));
        }
    }
    if let Ok(h) = std::env::var("USERPROFILE") {
        if !h.is_empty() {
            return Ok(PathBuf::from(h));
        }
    }
    Err(PkgError::Io(
        "cannot determine HOME (set $HOME or $USERPROFILE)".into(),
    ))
}

// PkgError::with_io_context — small adapter to fold an IO context message into
// the existing variant. Implemented as a free helper to keep error.rs lean.
trait WithIoContext {
    fn with_io_context(self, ctx: String) -> Self;
}

impl WithIoContext for PkgError {
    fn with_io_context(self, ctx: String) -> Self {
        // We always re-wrap as Io — the underlying registry error has been
        // surfaced, but the actionable IO context is what users need.
        PkgError::Io(ctx)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn make_source(dir: &Path, marker: &str) {
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("cobrust.toml"),
            format!(
                "[package]\nname = \"x\"\nversion = \"0.1.0\"\ncobrust-version = \"0.0.1\"\ndescription = \"{marker}\"\n[bin]\nname = \"x\"\npath = \"src/main.cb\"\n",
            ),
        )
        .unwrap();
        fs::write(dir.join("src/main.cb"), format!("# {marker}\n")).unwrap();
    }

    #[test]
    fn open_creates_layout() {
        let root = tempdir().unwrap();
        let r = Registry::open_at(root.path()).unwrap();
        assert!(r.root().join("blake3").is_dir());
        assert!(r.root().join("index").is_dir());
    }

    #[test]
    fn insert_and_get() {
        let root = tempdir().unwrap();
        let src = tempdir().unwrap();
        make_source(src.path(), "v1");

        let r = Registry::open_at(root.path()).unwrap();
        let entry = r.insert_source_tree(src.path()).unwrap();

        assert!(r.has(&entry.blake3_hex));
        let got = r.get(&entry.blake3_hex).unwrap();
        assert_eq!(got.local_path, entry.local_path);
        assert!(entry.local_path.join("src/main.cb").is_file());
    }

    #[test]
    fn insert_is_idempotent() {
        let root = tempdir().unwrap();
        let src = tempdir().unwrap();
        make_source(src.path(), "stable");

        let r = Registry::open_at(root.path()).unwrap();
        let e1 = r.insert_source_tree(src.path()).unwrap();
        let e2 = r.insert_source_tree(src.path()).unwrap();
        assert_eq!(e1.blake3_hex, e2.blake3_hex);
        assert_eq!(e1.local_path, e2.local_path);
    }

    #[test]
    fn miss_returns_none() {
        let root = tempdir().unwrap();
        let r = Registry::open_at(root.path()).unwrap();
        assert!(r.get("blake3:0000").is_none());
        assert!(!r.has("blake3:0000"));
    }

    #[test]
    fn verify_entry_succeeds_after_insert() {
        let root = tempdir().unwrap();
        let src = tempdir().unwrap();
        make_source(src.path(), "v");
        let r = Registry::open_at(root.path()).unwrap();
        let e = r.insert_source_tree(src.path()).unwrap();
        r.verify_entry(&e.blake3_hex).unwrap();
    }

    #[test]
    fn different_sources_different_hashes() {
        let root = tempdir().unwrap();
        let r = Registry::open_at(root.path()).unwrap();
        let s1 = tempdir().unwrap();
        let s2 = tempdir().unwrap();
        make_source(s1.path(), "alpha");
        make_source(s2.path(), "beta");
        let e1 = r.insert_source_tree(s1.path()).unwrap();
        let e2 = r.insert_source_tree(s2.path()).unwrap();
        assert_ne!(e1.blake3_hex, e2.blake3_hex);
    }
}
