//! Resolver integration smoke tests — ADR-0026 §D.

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

use cobrust_pkg::{
    Manifest, MaxCompatibleStrategy, PkgError, Registry, RegistryError, ResolutionError, Resolver,
};

fn write(p: &Path, contents: &str) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, contents).unwrap();
}

fn write_minimal_dep(dir: &Path, name: &str, version: &str, deps: &[(&str, &str)]) {
    let mut s = format!(
        "[package]\nname = \"{name}\"\nversion = \"{version}\"\ncobrust-version = \"0.0.1\"\n",
    );
    if !deps.is_empty() {
        s.push_str("\n[dependencies]\n");
        for (n, v) in deps {
            s.push_str(&format!("{n} = {v}\n"));
        }
    }
    s.push_str(&format!(
        "\n[bin]\nname = \"{name}\"\npath = \"src/main.cb\"\n"
    ));
    write(&dir.join("cobrust.toml"), &s);
    write(
        &dir.join("src/main.cb"),
        "fn main() -> i64:\n    return 0\n",
    );
}

#[test]
fn empty_dep_graph() {
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    write_minimal_dep(workspace.path(), "lone", "0.1.0", &[]);
    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let r = Registry::open_at(registry.path()).unwrap();
    let res = Resolver::new(MaxCompatibleStrategy)
        .resolve(&m, workspace.path(), &r)
        .unwrap();
    assert_eq!(res.root.name, "lone");
    assert!(res.packages.is_empty());
}

#[test]
fn single_path_dep() {
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    let dep = workspace.path().join("dep");
    fs::create_dir(&dep).unwrap();
    write_minimal_dep(&dep, "dep", "0.5.0", &[]);
    write_minimal_dep(
        workspace.path(),
        "main",
        "0.1.0",
        &[("dep", "{ path = \"dep\" }")],
    );

    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let r = Registry::open_at(registry.path()).unwrap();
    let res = Resolver::new(MaxCompatibleStrategy)
        .resolve(&m, workspace.path(), &r)
        .unwrap();
    assert_eq!(res.packages.len(), 1);
    let dep_pkg = &res.packages["dep"];
    assert_eq!(dep_pkg.version.to_string(), "0.5.0");
    assert!(dep_pkg.hash.starts_with("blake3:"));
    assert!(dep_pkg.local_path.is_dir());
}

#[test]
fn multiple_path_deps_at_root() {
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    for n in ["dep_a", "dep_b", "dep_c"] {
        let d = workspace.path().join(n);
        fs::create_dir(&d).unwrap();
        write_minimal_dep(&d, n, "1.0.0", &[]);
    }
    write_minimal_dep(
        workspace.path(),
        "main",
        "0.1.0",
        &[
            ("dep_a", "{ path = \"dep_a\" }"),
            ("dep_b", "{ path = \"dep_b\" }"),
            ("dep_c", "{ path = \"dep_c\" }"),
        ],
    );

    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let r = Registry::open_at(registry.path()).unwrap();
    let res = Resolver::new(MaxCompatibleStrategy)
        .resolve(&m, workspace.path(), &r)
        .unwrap();
    assert_eq!(res.packages.len(), 3);
}

#[test]
fn transitive_dep() {
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();

    let dep_a = workspace.path().join("dep_a");
    fs::create_dir(&dep_a).unwrap();
    let nested_b = dep_a.join("dep_b");
    fs::create_dir(&nested_b).unwrap();
    write_minimal_dep(&nested_b, "dep_b", "2.0.0", &[]);
    write_minimal_dep(
        &dep_a,
        "dep_a",
        "1.0.0",
        &[("dep_b", "{ path = \"dep_b\" }")],
    );
    write_minimal_dep(
        workspace.path(),
        "main",
        "0.1.0",
        &[("dep_a", "{ path = \"dep_a\" }")],
    );

    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let r = Registry::open_at(registry.path()).unwrap();
    let res = Resolver::new(MaxCompatibleStrategy)
        .resolve(&m, workspace.path(), &r)
        .unwrap();
    assert_eq!(res.packages.len(), 2);
    assert!(res.packages.contains_key("dep_a"));
    assert!(res.packages.contains_key("dep_b"));
    // dep_a's `dependency_names` should include "dep_b".
    assert_eq!(res.packages["dep_a"].dependency_names, vec!["dep_b"]);
}

#[test]
fn missing_path_dep_errors() {
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    write_minimal_dep(
        workspace.path(),
        "main",
        "0.1.0",
        &[("missing", "{ path = \"nope\" }")],
    );
    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let r = Registry::open_at(registry.path()).unwrap();
    let err = Resolver::new(MaxCompatibleStrategy)
        .resolve(&m, workspace.path(), &r)
        .unwrap_err();
    assert!(matches!(
        err,
        PkgError::Source(cobrust_pkg::SourceError::PathMissing(_))
    ));
}

#[test]
fn registry_offline_at_m12() {
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    write_minimal_dep(
        workspace.path(),
        "main",
        "0.1.0",
        &[("missing-from-cache", "\"1.0\"")],
    );
    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let r = Registry::open_at(registry.path()).unwrap();
    let err = Resolver::new(MaxCompatibleStrategy)
        .resolve(&m, workspace.path(), &r)
        .unwrap_err();
    assert!(matches!(
        err,
        PkgError::Registry(RegistryError::Offline { .. })
    ));
}

#[test]
fn cycle_detection_self_reference() {
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    // dep_a depends on dep_a (via path "."). The resolver should detect
    // and surface a Cycle.
    let dep_a = workspace.path().join("dep_a");
    fs::create_dir(&dep_a).unwrap();
    // Self-reference: dep_a's manifest declares a dep on dep_a via path "."
    // (which points back to dep_a's own root).
    let mut s = String::new();
    s.push_str("[package]\nname = \"dep_a\"\nversion = \"1.0.0\"\ncobrust-version = \"0.0.1\"\n\n");
    s.push_str("[dependencies]\ndep_a = { path = \".\" }\n\n");
    s.push_str("[bin]\nname = \"dep_a\"\npath = \"src/main.cb\"\n");
    write(&dep_a.join("cobrust.toml"), &s);
    write(
        &dep_a.join("src/main.cb"),
        "fn main() -> i64:\n    return 0\n",
    );

    write_minimal_dep(
        workspace.path(),
        "main",
        "0.1.0",
        &[("dep_a", "{ path = \"dep_a\" }")],
    );
    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let r = Registry::open_at(registry.path()).unwrap();
    let err = Resolver::new(MaxCompatibleStrategy)
        .resolve(&m, workspace.path(), &r)
        .unwrap_err();
    assert!(matches!(
        err,
        PkgError::Resolution(ResolutionError::Cycle { .. })
    ));
}

#[test]
fn resolution_is_deterministic_under_workspace_repeat() {
    // Resolving twice with identical inputs gives identical resolutions.
    let workspace = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    let dep = workspace.path().join("dep");
    fs::create_dir(&dep).unwrap();
    write_minimal_dep(&dep, "dep", "0.5.0", &[]);
    write_minimal_dep(
        workspace.path(),
        "main",
        "0.1.0",
        &[("dep", "{ path = \"dep\" }")],
    );

    let m =
        Manifest::parse_str(&fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap())
            .unwrap();
    let r = Registry::open_at(registry.path()).unwrap();
    let r1 = Resolver::new(MaxCompatibleStrategy)
        .resolve(&m, workspace.path(), &r)
        .unwrap();
    let r2 = Resolver::new(MaxCompatibleStrategy)
        .resolve(&m, workspace.path(), &r)
        .unwrap();
    assert_eq!(r1.packages["dep"].hash, r2.packages["dep"].hash);
    assert_eq!(r1.packages["dep"].version, r2.packages["dep"].version);
}
