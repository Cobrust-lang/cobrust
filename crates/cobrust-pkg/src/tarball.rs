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
use std::path::{Component, Path, PathBuf};

use crate::error::{PkgError, RegistryError, TarballError};

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
    ///
    /// # Security (M9)
    ///
    /// Before extracting, every entry path is validated:
    /// - Absolute paths are rejected.
    /// - Any path component equal to `..` is rejected.
    /// - Symlink entries are rejected (Cobrust tarballs never contain
    ///   symlinks; any symlink in an externally-sourced tarball is
    ///   treated as adversarial).
    ///
    /// This prevents a "zip-slip" / "symlink escape" attack where a
    /// crafted tarball writes outside the extraction directory.
    pub fn extract(&self, dest: &Path) -> Result<(), PkgError> {
        std::fs::create_dir_all(dest)
            .map_err(|e| PkgError::Io(format!("mkdir {}: {e}", dest.display())))?;

        // --- Pass 1: validate every entry before touching the filesystem. ---
        {
            let decoder = flate2::read::GzDecoder::new(&self.bytes[..]);
            let mut archive = tar::Archive::new(decoder);
            for entry in archive
                .entries()
                .map_err(|e| PkgError::Tarball(TarballError::ReadEntries(e.to_string())))?
            {
                let entry = entry
                    .map_err(|e| PkgError::Tarball(TarballError::ReadEntries(e.to_string())))?;
                let header = entry.header();

                // Reject symlinks and hard-links.
                let entry_type = header.entry_type();
                if entry_type.is_symlink() || entry_type.is_hard_link() {
                    let path = entry
                        .path()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_else(|_| "<non-utf8>".into());
                    return Err(PkgError::Tarball(TarballError::SymlinkEntry(path)));
                }

                // Reject absolute paths and `..` components.
                let raw_path = entry
                    .path()
                    .map_err(|e| PkgError::Tarball(TarballError::BadEntryPath(e.to_string())))?;
                validate_entry_path(&raw_path)?;
            }
        }

        // --- Pass 2: extract (all entries are safe). ---
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

/// (M9) Validate that a tarball entry path cannot escape the extraction root.
///
/// Rejects:
/// - Absolute paths (`/etc/passwd`, `C:\Windows\…`)
/// - Any `..` component in the path
fn validate_entry_path(path: &Path) -> Result<(), PkgError> {
    if path.is_absolute() {
        return Err(PkgError::Tarball(TarballError::PathEscape(
            path.to_string_lossy().into_owned(),
        )));
    }
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(PkgError::Tarball(TarballError::PathEscape(
                path.to_string_lossy().into_owned(),
            )));
        }
    }
    Ok(())
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

    // ----------------------------------------------------------------
    // M9 adversarial corpus — tarball path-traversal + symlink attacks
    // ----------------------------------------------------------------

    /// Build a gzipped tarball at the raw byte level with a single entry
    /// at `entry_path` (as a regular file with content `"x"`).
    ///
    /// We write the POSIX tar header manually rather than via `tar::Builder`
    /// because `tar::Header::set_path` refuses `..` components — exactly what
    /// we need to craft adversarial fixtures.  The format is a 512-byte POSIX
    /// header block followed by 512-byte data block(s) then two 512-byte EOF
    /// blocks of NUL.
    fn build_raw_tarball_with_path(entry_path: &str) -> Vec<u8> {
        let mut header = [0u8; 512];
        // name field: bytes 0..100
        let name_bytes = entry_path.as_bytes();
        let name_len = name_bytes.len().min(100);
        header[..name_len].copy_from_slice(&name_bytes[..name_len]);
        // mode: bytes 100..108
        header[100..107].copy_from_slice(b"0000644");
        // uid/gid: bytes 108..116 / 116..124
        header[108..115].copy_from_slice(b"0000000");
        header[116..123].copy_from_slice(b"0000000");
        // size: 1 byte content "x", bytes 124..136
        header[124..135].copy_from_slice(b"00000000001");
        // mtime: bytes 136..148
        header[136..147].copy_from_slice(b"00000000000");
        // checksum placeholder: 8 spaces bytes 148..156
        header[148..156].copy_from_slice(b"        ");
        // typeflag: bytes 156 — '0' = regular file
        header[156] = b'0';
        // magic: bytes 257..263 "ustar\0"
        header[257..263].copy_from_slice(b"ustar\0");
        // version: bytes 263..265 "00"
        header[263..265].copy_from_slice(b"00");
        // Compute checksum (sum of all bytes as unsigned, stored in field 148..156).
        let sum: u32 = header.iter().map(|&b| u32::from(b)).sum();
        // Write octal checksum (6 digits + NUL + space).
        let cksum = format!("{sum:06o}\0 ");
        header[148..156].copy_from_slice(cksum.as_bytes());

        let mut tar_bytes: Vec<u8> = Vec::new();
        tar_bytes.extend_from_slice(&header);
        // Data block for "x" (padded to 512 bytes).
        let mut data_block = [0u8; 512];
        data_block[0] = b'x';
        tar_bytes.extend_from_slice(&data_block);
        // Two EOF blocks.
        tar_bytes.extend_from_slice(&[0u8; 512]);
        tar_bytes.extend_from_slice(&[0u8; 512]);

        let mut gz_bytes: Vec<u8> = Vec::new();
        let mut encoder = flate2::write::GzEncoder::new(&mut gz_bytes, flate2::Compression::new(6));
        encoder.write_all(&tar_bytes).unwrap();
        encoder.finish().unwrap();
        gz_bytes
    }

    /// Build a gzipped tarball with a symlink entry pointing at `link_target`.
    fn build_raw_tarball_with_symlink(link_name: &str, link_target: &str) -> Vec<u8> {
        let mut tar_bytes: Vec<u8> = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_mode(0o777);
            header.set_size(0);
            header.set_mtime(0);
            header.set_uid(0);
            header.set_gid(0);
            header.set_path(link_name).unwrap();
            header.set_link_name(link_target).unwrap();
            header.set_cksum();
            builder.append(&header, std::io::empty()).unwrap();
            builder.finish().unwrap();
        }
        let mut gz_bytes: Vec<u8> = Vec::new();
        let mut encoder = flate2::write::GzEncoder::new(&mut gz_bytes, flate2::Compression::new(6));
        encoder.write_all(&tar_bytes).unwrap();
        encoder.finish().unwrap();
        gz_bytes
    }

    #[test]
    fn adversarial_path_traversal_dotdot() {
        // `../etc/passwd` must be rejected before extraction.
        let raw = build_raw_tarball_with_path("../etc/passwd");
        let tb = Tarball::from_bytes(raw);
        let dest = tempdir().unwrap();
        let err = tb.extract(dest.path()).unwrap_err();
        assert!(
            matches!(err, PkgError::Tarball(TarballError::PathEscape(_))),
            "expected PathEscape, got {err:?}"
        );
        // Confirm the file was NOT written.
        assert!(!dest.path().join("etc/passwd").exists());
    }

    #[test]
    fn adversarial_path_traversal_nested_dotdot() {
        // `safe/../../etc/secret` contains a `..` component after safe prefix.
        let raw = build_raw_tarball_with_path("safe/../../etc/secret");
        let tb = Tarball::from_bytes(raw);
        let dest = tempdir().unwrap();
        let err = tb.extract(dest.path()).unwrap_err();
        assert!(
            matches!(err, PkgError::Tarball(TarballError::PathEscape(_))),
            "expected PathEscape, got {err:?}"
        );
    }

    #[test]
    fn adversarial_symlink_escape() {
        // A symlink `link -> ../../../../tmp/evil` must be rejected.
        let raw = build_raw_tarball_with_symlink("link", "../../../../tmp/evil");
        let tb = Tarball::from_bytes(raw);
        let dest = tempdir().unwrap();
        let err = tb.extract(dest.path()).unwrap_err();
        assert!(
            matches!(err, PkgError::Tarball(TarballError::SymlinkEntry(_))),
            "expected SymlinkEntry, got {err:?}"
        );
        assert!(!dest.path().join("link").exists());
    }

    #[test]
    fn adversarial_symlink_same_dir() {
        // Even a symlink within the archive (not escaping) is rejected —
        // Cobrust tarballs never contain symlinks.
        let raw = build_raw_tarball_with_symlink("a_link", "a_target");
        let tb = Tarball::from_bytes(raw);
        let dest = tempdir().unwrap();
        let err = tb.extract(dest.path()).unwrap_err();
        assert!(
            matches!(err, PkgError::Tarball(TarballError::SymlinkEntry(_))),
            "expected SymlinkEntry, got {err:?}"
        );
    }

    #[test]
    fn safe_tarball_still_extracts() {
        // Sanity-check: a well-formed tarball with only normal files extracts OK.
        let a = tempdir().unwrap();
        fs::create_dir(a.path().join("src")).unwrap();
        fs::write(a.path().join("src/main.cb"), "fn main():\n    pass\n").unwrap();
        let ta = Tarball::build(a.path()).unwrap();
        let dest = tempdir().unwrap();
        ta.extract(dest.path()).unwrap();
        assert!(dest.path().join("src/main.cb").is_file());
    }
}
