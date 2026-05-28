//! ADR-0076 Phase 1 — crate-type shape regression. Mirrors
//! `cobrust-hood/tests/click_pyo3_compiles.rs` but cobrust-dora has no
//! `pyo3` feature in Phase 1 (deferred per ADR-0076 §5 Phase 1 "no PyO3
//! reverse-binding"), so this test ONLY asserts the Cargo.toml declares
//! the expected `crate-type = ["rlib", "cdylib", "staticlib"]` shape.
//! When Phase 2/3 adds a PyO3 wrapper, this test grows the
//! `--features pyo3` build branch and the cargo-toml `pyo3 = ["dep:pyo3"]`
//! assertion verbatim from hood's pattern.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;

#[test]
fn crate_type_matches_ecosystem_module_shape() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cargo_toml = std::fs::read_to_string(manifest.join("Cargo.toml")).expect("read Cargo.toml");
    // ADR-0076 Phase 1 second proof — `staticlib` produces `libdora.a`
    // the `cobrust build` per-import link consumes. Both literal forms
    // are accepted so a future PyO3 graduation (Phase 2/3) lands without
    // breaking this gate.
    assert!(
        cargo_toml.contains(r#"crate-type = ["rlib", "cdylib", "staticlib"]"#)
            || cargo_toml.contains(r#"crate-type = ["rlib", "staticlib"]"#),
        "Cargo.toml must declare staticlib crate-type per ADR-0076 Phase 1"
    );
}
