//! Content-addressed registry cache — ADR-0026 §F.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]

use std::fs;
use std::path::Path;

use cobrust_pkg::{Registry, Tarball};

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
fn registry_creates_layout() {
    let root = tempfile::tempdir().unwrap();
    let r = Registry::open_at(root.path()).unwrap();
    assert!(r.root().join("blake3").is_dir());
    assert!(r.root().join("index").is_dir());
}

#[test]
fn blake3_hash_is_blake3_prefixed() {
    let src = tempfile::tempdir().unwrap();
    make_source(src.path(), "v");
    let t = Tarball::build(src.path()).unwrap();
    assert!(t.hash().starts_with("blake3:"));
    let hex_part = t.hash().strip_prefix("blake3:").unwrap();
    assert_eq!(hex_part.len(), 64, "blake3 hex must be 64 chars");
    assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn cache_miss_then_hit() {
    let registry = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    make_source(src.path(), "v1");
    let r = Registry::open_at(registry.path()).unwrap();

    // Compute hash without inserting; expect miss.
    let t = Tarball::build(src.path()).unwrap();
    assert!(!r.has(t.hash()));

    // Insert; expect hit.
    let entry = r.insert_source_tree(src.path()).unwrap();
    assert!(r.has(&entry.blake3_hex));
    assert_eq!(entry.blake3_hex, t.hash().to_string());
}

#[test]
fn idempotent_insert() {
    let registry = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    make_source(src.path(), "stable");
    let r = Registry::open_at(registry.path()).unwrap();

    let e1 = r.insert_source_tree(src.path()).unwrap();
    let e2 = r.insert_source_tree(src.path()).unwrap();
    assert_eq!(e1.blake3_hex, e2.blake3_hex);
    assert_eq!(e1.local_path, e2.local_path);
}

#[test]
fn hash_changes_with_content() {
    let registry = tempfile::tempdir().unwrap();
    let r = Registry::open_at(registry.path()).unwrap();
    let s1 = tempfile::tempdir().unwrap();
    let s2 = tempfile::tempdir().unwrap();
    make_source(s1.path(), "alpha");
    make_source(s2.path(), "beta");
    let e1 = r.insert_source_tree(s1.path()).unwrap();
    let e2 = r.insert_source_tree(s2.path()).unwrap();
    assert_ne!(e1.blake3_hex, e2.blake3_hex);
}

#[test]
fn registry_path_layout_blake3_hex() {
    // <root>/blake3/<64-hex-chars>/cobrust.toml
    let registry = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    make_source(src.path(), "v");
    let r = Registry::open_at(registry.path()).unwrap();
    let entry = r.insert_source_tree(src.path()).unwrap();

    let p = entry.local_path.clone();
    let parent = p.parent().unwrap();
    assert_eq!(parent.file_name().unwrap().to_str().unwrap(), "blake3");
    let hex = p.file_name().unwrap().to_str().unwrap();
    assert_eq!(hex.len(), 64);
}

#[test]
fn verify_entry_matches_blake3() {
    let registry = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    make_source(src.path(), "v");
    let r = Registry::open_at(registry.path()).unwrap();
    let entry = r.insert_source_tree(src.path()).unwrap();
    r.verify_entry(&entry.blake3_hex).unwrap();
}

#[test]
fn verify_entry_missing_is_error() {
    let registry = tempfile::tempdir().unwrap();
    let r = Registry::open_at(registry.path()).unwrap();
    let err = r
        .verify_entry("blake3:0000000000000000000000000000000000000000000000000000000000000000")
        .unwrap_err();
    assert!(matches!(
        err,
        cobrust_pkg::PkgError::Registry(cobrust_pkg::RegistryError::EntryNotFound { .. })
    ));
}

#[test]
fn tarball_round_trip_via_registry() {
    let registry = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    make_source(src.path(), "rt");
    let r = Registry::open_at(registry.path()).unwrap();
    let entry = r.insert_source_tree(src.path()).unwrap();
    // Source tree present in cache.
    assert!(entry.local_path.join("src/main.cb").is_file());
    assert!(entry.local_path.join("cobrust.toml").is_file());
}

#[test]
fn get_returns_some_when_cached() {
    let registry = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    make_source(src.path(), "g");
    let r = Registry::open_at(registry.path()).unwrap();
    let entry = r.insert_source_tree(src.path()).unwrap();

    let got = r.get(&entry.blake3_hex).unwrap();
    assert_eq!(got.local_path, entry.local_path);
    assert_eq!(got.blake3_hex, entry.blake3_hex);
}

#[test]
fn get_returns_none_when_missing() {
    let registry = tempfile::tempdir().unwrap();
    let r = Registry::open_at(registry.path()).unwrap();
    assert!(
        r.get("blake3:0000000000000000000000000000000000000000000000000000000000000000")
            .is_none()
    );
}
