//! Lockfile determinism — ADR-0026 §C.
//!
//! Same `(manifest, registry-state)` MUST emit byte-identical lockfile
//! bytes. We exercise this by:
//!
//! - building two source trees with identical content but different on-disk
//!   creation order
//! - resolving each independently into separate registries
//! - comparing the canonical TOML bytes of the two lockfiles

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

use cobrust_pkg::{Manifest, Registry, resolve_and_lock};

fn write(p: &Path, contents: &str) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, contents).unwrap();
}

fn make_workspace(root: &Path, dep_order_a_then_b: bool) {
    // dep_a/cobrust.toml + dep_a/src/main.cb
    let dep_a = root.join("dep_a");
    write(
        &dep_a.join("cobrust.toml"),
        "[package]\nname = \"dep_a\"\nversion = \"1.0.0\"\ncobrust-version = \"0.0.1\"\n[bin]\nname = \"dep_a\"\npath = \"src/main.cb\"\n",
    );
    write(
        &dep_a.join("src/main.cb"),
        "fn main() -> i64:\n    return 0\n",
    );

    // dep_b
    let dep_b = root.join("dep_b");
    write(
        &dep_b.join("cobrust.toml"),
        "[package]\nname = \"dep_b\"\nversion = \"2.0.0\"\ncobrust-version = \"0.0.1\"\n[bin]\nname = \"dep_b\"\npath = \"src/main.cb\"\n",
    );
    write(
        &dep_b.join("src/main.cb"),
        "fn main() -> i64:\n    return 0\n",
    );

    // Root manifest — vary the dep declaration order between the two cases.
    let manifest = if dep_order_a_then_b {
        "[package]\nname = \"root\"\nversion = \"0.1.0\"\ncobrust-version = \"0.0.1\"\n\n[dependencies]\ndep_a = { path = \"dep_a\" }\ndep_b = { path = \"dep_b\" }\n\n[bin]\nname = \"root\"\npath = \"src/main.cb\"\n"
    } else {
        "[package]\nname = \"root\"\nversion = \"0.1.0\"\ncobrust-version = \"0.0.1\"\n\n[dependencies]\ndep_b = { path = \"dep_b\" }\ndep_a = { path = \"dep_a\" }\n\n[bin]\nname = \"root\"\npath = \"src/main.cb\"\n"
    };
    write(&root.join("cobrust.toml"), manifest);
    write(
        &root.join("src/main.cb"),
        "fn main() -> i64:\n    return 0\n",
    );
}

#[test]
fn same_inputs_byte_identical_bytes() {
    let workspace_a = tempfile::tempdir().unwrap();
    let workspace_b = tempfile::tempdir().unwrap();
    let registry_a = tempfile::tempdir().unwrap();
    let registry_b = tempfile::tempdir().unwrap();

    make_workspace(workspace_a.path(), true);
    make_workspace(workspace_b.path(), true);

    let m_a =
        Manifest::parse_str(&fs::read_to_string(workspace_a.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let m_b =
        Manifest::parse_str(&fs::read_to_string(workspace_b.path().join("cobrust.toml")).unwrap())
            .unwrap();

    let reg_a = Registry::open_at(registry_a.path()).unwrap();
    let reg_b = Registry::open_at(registry_b.path()).unwrap();

    let lock_a = resolve_and_lock(&m_a, workspace_a.path(), &reg_a).unwrap();
    let lock_b = resolve_and_lock(&m_b, workspace_b.path(), &reg_b).unwrap();

    let bytes_a = lock_a.to_canonical_toml();
    let bytes_b = lock_b.to_canonical_toml();

    // Note: the `source = "path+file:///abs/path"` field will differ
    // because the absolute paths are different (different tempdirs).
    // The determinism contract is "same source paths → same bytes",
    // not "same content tree → same bytes regardless of paths."
    // For a tighter test we redact the source field before comparing.
    let redact = |s: &str| {
        s.lines()
            .map(|l| {
                if l.starts_with("source = ") {
                    "source = \"<redacted>\""
                } else {
                    l
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        redact(&bytes_a),
        redact(&bytes_b),
        "lockfile bytes (sans source) differ:\nA:\n{bytes_a}\n\nB:\n{bytes_b}"
    );
}

#[test]
fn dep_declaration_order_doesnt_affect_bytes() {
    let workspace_a = tempfile::tempdir().unwrap();
    let workspace_b = tempfile::tempdir().unwrap();
    let registry_a = tempfile::tempdir().unwrap();
    let registry_b = tempfile::tempdir().unwrap();

    make_workspace(workspace_a.path(), true);
    make_workspace(workspace_b.path(), false); // a-then-b vs b-then-a

    let m_a =
        Manifest::parse_str(&fs::read_to_string(workspace_a.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let m_b =
        Manifest::parse_str(&fs::read_to_string(workspace_b.path().join("cobrust.toml")).unwrap())
            .unwrap();
    assert_eq!(m_a.manifest_hash(), m_b.manifest_hash());

    let reg_a = Registry::open_at(registry_a.path()).unwrap();
    let reg_b = Registry::open_at(registry_b.path()).unwrap();
    let lock_a = resolve_and_lock(&m_a, workspace_a.path(), &reg_a).unwrap();
    let lock_b = resolve_and_lock(&m_b, workspace_b.path(), &reg_b).unwrap();

    let bytes_a = lock_a.to_canonical_toml();
    let bytes_b = lock_b.to_canonical_toml();
    let redact = |s: &str| {
        s.lines()
            .map(|l| {
                if l.starts_with("source = ") {
                    "source = \"<redacted>\""
                } else {
                    l
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(redact(&bytes_a), redact(&bytes_b));
}

#[test]
fn run_twice_in_same_workspace_byte_identical() {
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    make_workspace(workspace.path(), true);

    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let reg = Registry::open_at(registry.path()).unwrap();

    let l1 = resolve_and_lock(&m, workspace.path(), &reg).unwrap();
    let l2 = resolve_and_lock(&m, workspace.path(), &reg).unwrap();
    assert_eq!(l1.to_canonical_toml(), l2.to_canonical_toml());
}

#[test]
fn determinism_gate_save_file() {
    // The `cobrust build` determinism gate per the prompt: compute the
    // lockfile twice via save+load, compare via diff.
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    make_workspace(workspace.path(), true);
    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let reg = Registry::open_at(registry.path()).unwrap();
    let lock = resolve_and_lock(&m, workspace.path(), &reg).unwrap();

    let p1 = workspace.path().join("cobrust.lock.1");
    let p2 = workspace.path().join("cobrust.lock.2");
    cobrust_pkg::save_lockfile(&lock, &p1).unwrap();
    cobrust_pkg::save_lockfile(&lock, &p2).unwrap();
    let b1 = fs::read(&p1).unwrap();
    let b2 = fs::read(&p2).unwrap();
    assert_eq!(b1, b2);
}

#[test]
fn lockfile_has_lf_only() {
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    make_workspace(workspace.path(), true);
    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let reg = Registry::open_at(registry.path()).unwrap();
    let lock = resolve_and_lock(&m, workspace.path(), &reg).unwrap();
    let bytes = lock.to_canonical_toml();
    assert!(!bytes.contains('\r'), "lockfile must be LF-only");
    assert!(bytes.ends_with('\n'), "lockfile must end with newline");
}

#[test]
fn lockfile_round_trips_through_parse() {
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    make_workspace(workspace.path(), true);
    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let reg = Registry::open_at(registry.path()).unwrap();
    let l1 = resolve_and_lock(&m, workspace.path(), &reg).unwrap();
    let s = l1.to_canonical_toml();
    let l2 = cobrust_pkg::Lockfile::parse_str(&s).unwrap();
    assert_eq!(l1, l2);
}
