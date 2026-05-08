//! Deterministic tarball helper (ADR-0026 §G "Tarball determinism").
//!
//! When `Registry::insert` ingests a source tree, it must produce a
//! byte-identical tar.gz given identical input. The recipe (binding):
//!
//! - Walk the source tree depth-first; sort each directory's entries
//!   lexically by file name.
//! - Zero `mtime`, `uid`, `gid`, `uname`, `gname` on every header.
//! - Canonical permission bits: `0o644` for files, `0o755` for dirs.
//! - Drop symlinks unconditionally (M12 has no use case; future ADR
//!   may lift).
//! - Skip `target/`, `cobrust.lock`, `.git/`, `.cobrust/` (build
//!   outputs that aren't part of the source identity).
//!
//! The tarball's `blake3` hash IS the package's content-addressed
//! identity. Same source tree → same hex.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::{PkgError, RegistryError};

/// A tarball ready for hashing + extraction.
#[derive(Clone)]
pub struct Tarball {
    bytes: Vec<u8>,
    blake3_hex: String,
}

impl Tarball {
    /// Build a deterministic tar.gz from `dir` (recursively).
    pub fn build(dir: &Path) -> Result<Self, PkgError> {
        if !dir.is_dir() {
            return Err(PkgError::Io(format!(
                "tarball source `{}` is not a directory",
                dir.display()
            )));
        }

        // Collect entries: (relative-path, kind, contents-bytes-if-file).
        let mut entries: Vec<TarEntry> = Vec::new();
        collect_entries(dir, Path::new(""), &mut entries)?;
        entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

        // Build the tar.
        let mut tar_bytes: Vec<u8> = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            builder.mode(tar::HeaderMode::Deterministic);
            for ent in &entries {
                let mut header = tar::Header::new_gnu();
                header.set_mtime(0);
                header.set_uid(0);
                header.set_gid(0);
                header
                    .set_username("")
                    .map_err(|e| PkgError::Io(format!("tar set_username: {e}")))?;
                header
                    .set_groupname("")
                    .map_err(|e| PkgError::Io(format!("tar set_groupname: {e}")))?;
                header.set_path(&ent.rel_path).map_err(|e| {
                    PkgError::Io(format!("tar set_path `{}`: {e}", ent.rel_path.display()))
                })?;
                match &ent.kind {
                    TarKind::Dir => {
                        header.set_entry_type(tar::EntryType::Directory);
                        header.set_mode(0o755);
                        header.set_size(0);
                        header.set_cksum();
                        builder
                            .append(&header, std::io::empty())
                            .map_err(|e| PkgError::Io(format!("tar append dir: {e}")))?;
                    }
                    TarKind::File(bytes) => {
                        header.set_entry_type(tar::EntryType::Regular);
                        header.set_mode(0o644);
                        header.set_size(bytes.len() as u64);
                        header.set_cksum();
                        builder
                            .append(&header, &bytes[..])
                            .map_err(|e| PkgError::Io(format!("tar append file: {e}")))?;
                    }
                }
            }
            builder
                .finish()
                .map_err(|e| PkgError::Io(format!("tar finish: {e}")))?;
        }

        // Gzip with deterministic options: no mtime, no filename, no comment,
        // and a fixed compression level.
        let mut gz_bytes: Vec<u8> = Vec::new();
        {
            let mut encoder =
                flate2::write::GzEncoder::new(&mut gz_bytes, flate2::Compression::new(6));
            encoder
                .write_all(&tar_bytes)
                .map_err(|e| PkgError::Io(format!("gz write: {e}")))?;
            encoder
                .finish()
                .map_err(|e| PkgError::Io(format!("gz finish: {e}")))?;
        }

        let h = blake3::hash(&gz_bytes);
        Ok(Self {
            bytes: gz_bytes,
            blake3_hex: format!("blake3:{}", h.to_hex()),
        })
    }

    /// `blake3:<hex>` of the gzipped tarball bytes.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.blake3_hex
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Extract the tarball into `dest`. Creates `dest` if missing.
    pub fn extract(&self, dest: &Path) -> Result<(), PkgError> {
        std::fs::create_dir_all(dest)
            .map_err(|e| PkgError::Io(format!("mkdir {}: {e}", dest.display())))?;
        let decoder = flate2::read::GzDecoder::new(&self.bytes[..]);
        let mut archive = tar::Archive::new(decoder);
        archive
            .unpack(dest)
            .map_err(|e| PkgError::Io(format!("untar into {}: {e}", dest.display())))?;
        Ok(())
    }

    /// Re-construct from raw gzipped bytes (used by registry to verify
    /// previously-cached entries).
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let h = blake3::hash(&bytes);
        Self {
            blake3_hex: format!("blake3:{}", h.to_hex()),
            bytes,
        }
    }

    /// Verify a recorded `blake3:<hex>` matches this tarball's hash.
    pub fn verify_hash(&self, expected: &str) -> Result<(), PkgError> {
        if expected == self.blake3_hex {
            Ok(())
        } else {
            Err(PkgError::Registry(RegistryError::HashMismatch {
                expected: expected.to_string(),
                actual: self.blake3_hex.clone(),
            }))
        }
    }
}

impl std::fmt::Debug for Tarball {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tarball")
            .field("hash", &self.blake3_hex)
            .field("bytes_len", &self.bytes.len())
            .finish()
    }
}

#[derive(Debug)]
struct TarEntry {
    rel_path: PathBuf,
    kind: TarKind,
}

#[derive(Debug)]
enum TarKind {
    Dir,
    File(Vec<u8>),
}

/// Names that should NEVER appear in a deterministic source tarball.
fn is_excluded(name: &str) -> bool {
    matches!(name, "target" | ".git" | ".cobrust" | "cobrust.lock")
}

fn collect_entries(
    root: &Path,
    rel_prefix: &Path,
    out: &mut Vec<TarEntry>,
) -> Result<(), PkgError> {
    let mut sorted_children: BTreeSet<String> = BTreeSet::new();
    let read_dir = std::fs::read_dir(root)
        .map_err(|e| PkgError::Io(format!("read_dir {}: {e}", root.display())))?;
    for entry in read_dir {
        let entry = entry.map_err(|e| PkgError::Io(format!("dir entry: {e}")))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| PkgError::Io("non-UTF8 file name".into()))?;
        if is_excluded(&name) {
            continue;
        }
        sorted_children.insert(name);
    }

    for name in sorted_children {
        let abs = root.join(&name);
        let rel = rel_prefix.join(&name);
        let metadata = std::fs::symlink_metadata(&abs)
            .map_err(|e| PkgError::Io(format!("metadata {}: {e}", abs.display())))?;
        if metadata.file_type().is_symlink() {
            // ADR-0026: drop symlinks.
            continue;
        }
        if metadata.is_dir() {
            out.push(TarEntry {
                rel_path: rel.clone(),
                kind: TarKind::Dir,
            });
            collect_entries(&abs, &rel, out)?;
        } else if metadata.is_file() {
            let mut f = std::fs::File::open(&abs)
                .map_err(|e| PkgError::Io(format!("open {}: {e}", abs.display())))?;
            let mut buf = Vec::with_capacity(metadata.len() as usize);
            f.read_to_end(&mut buf)
                .map_err(|e| PkgError::Io(format!("read {}: {e}", abs.display())))?;
            out.push(TarEntry {
                rel_path: rel,
                kind: TarKind::File(buf),
            });
        }
        // Other file types (sockets, devices) silently skipped.
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn deterministic_same_inputs_same_hash() {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();

        for dir in [a.path(), b.path()] {
            fs::create_dir(dir.join("src")).unwrap();
            fs::write(dir.join("src/main.cb"), "fn main() -> i64:\n    return 0\n").unwrap();
            fs::write(
                dir.join("cobrust.toml"),
                "[package]\nname = \"x\"\nversion = \"0.1.0\"\ncobrust-version = \"0.0.1\"\n[bin]\nname = \"x\"\npath = \"src/main.cb\"\n",
            )
            .unwrap();
        }

        let ta = Tarball::build(a.path()).unwrap();
        let tb = Tarball::build(b.path()).unwrap();
        assert_eq!(ta.hash(), tb.hash(), "same input → same hash");
        assert_eq!(ta.bytes(), tb.bytes(), "same input → byte-identical");
    }

    #[test]
    fn excludes_target_and_lock() {
        let a = tempdir().unwrap();
        fs::create_dir(a.path().join("src")).unwrap();
        fs::write(a.path().join("src/main.cb"), "x").unwrap();
        // Add things that must be excluded.
        fs::create_dir(a.path().join("target")).unwrap();
        fs::write(a.path().join("target/junk.o"), "junk").unwrap();
        fs::write(a.path().join("cobrust.lock"), "lock-junk").unwrap();
        fs::create_dir(a.path().join(".git")).unwrap();
        fs::write(a.path().join(".git/HEAD"), "ref").unwrap();

        let ta = Tarball::build(a.path()).unwrap();
        let extract_dir = tempdir().unwrap();
        ta.extract(extract_dir.path()).unwrap();
        assert!(extract_dir.path().join("src/main.cb").is_file());
        assert!(!extract_dir.path().join("target").exists());
        assert!(!extract_dir.path().join("cobrust.lock").exists());
        assert!(!extract_dir.path().join(".git").exists());
    }

    #[test]
    fn round_trip_extract() {
        let a = tempdir().unwrap();
        fs::create_dir(a.path().join("src")).unwrap();
        fs::write(
            a.path().join("src/main.cb"),
            "fn main() -> i64:\n    return 0\n",
        )
        .unwrap();
        let ta = Tarball::build(a.path()).unwrap();

        let extract = tempdir().unwrap();
        ta.extract(extract.path()).unwrap();
        let content = fs::read_to_string(extract.path().join("src/main.cb")).unwrap();
        assert_eq!(content, "fn main() -> i64:\n    return 0\n");
    }

    #[test]
    fn hash_changes_on_file_edit() {
        let a = tempdir().unwrap();
        fs::create_dir(a.path().join("src")).unwrap();
        fs::write(a.path().join("src/main.cb"), "v1").unwrap();
        let h1 = Tarball::build(a.path()).unwrap().hash().to_string();
        fs::write(a.path().join("src/main.cb"), "v2").unwrap();
        let h2 = Tarball::build(a.path()).unwrap().hash().to_string();
        assert_ne!(h1, h2);
    }

    #[test]
    fn from_bytes_round_trip_hash() {
        let a = tempdir().unwrap();
        fs::create_dir(a.path().join("src")).unwrap();
        fs::write(a.path().join("src/main.cb"), "x").unwrap();
        let ta = Tarball::build(a.path()).unwrap();
        let raw = ta.bytes().to_vec();
        let h = ta.hash().to_string();

        let tb = Tarball::from_bytes(raw);
        assert_eq!(tb.hash(), h);
    }

    #[test]
    fn verify_hash_succeeds_and_fails() {
        let a = tempdir().unwrap();
        fs::create_dir(a.path().join("src")).unwrap();
        fs::write(a.path().join("src/main.cb"), "x").unwrap();
        let ta = Tarball::build(a.path()).unwrap();
        let h = ta.hash().to_string();
        assert!(ta.verify_hash(&h).is_ok());
        assert!(ta.verify_hash("blake3:0000").is_err());
    }
}
