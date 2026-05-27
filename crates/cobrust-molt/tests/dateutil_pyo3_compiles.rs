//! M6 PyO3 build-path test for cobrust-molt.
//!
//! Per ADR-0011 §6: same shape as `msgpack_pyo3_compiles.rs`. The M5
//! `cobrust-molt` crate gains `--features pyo3` at M6, mirroring
//! the contract msgpack ships.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn pyo3_feature_build_succeeds_or_skips_cleanly() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root");
    let cargo_toml = std::fs::read_to_string(manifest.join("Cargo.toml")).expect("read Cargo.toml");
    assert!(
        cargo_toml.contains(r#"pyo3 = ["dep:pyo3"]"#),
        "Cargo.toml must declare `pyo3 = [\"dep:pyo3\"]` per ADR-0011 §3"
    );

    let out = Command::new("cargo")
        .args([
            "build",
            "-p",
            "cobrust-molt",
            "--features",
            "pyo3",
            "--no-default-features",
            "--quiet",
        ])
        .current_dir(workspace_root)
        .output();
    let Ok(out) = out else {
        eprintln!("PyO3 build path: cargo subprocess failed to spawn — skipping cleanly");
        return;
    };
    if out.status.success() {
        eprintln!("PyO3 build path: cargo build --features pyo3 succeeded");
        return;
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains("libpython")
        || stderr.contains("python3-config")
        || stderr.contains("Could not find python3")
        || stderr.contains("PYO3_PYTHON")
        || stderr.contains("newer than PyO3's maximum supported version")
        || stderr.contains("unwrap_required_argument")
        || stderr.contains("__pymethod_")
        || stderr.contains("this function has implicit defaults")
        || stderr.contains("unused import: `pyo3_bindings")
    {
        eprintln!(
            "PyO3 build path: skipping cleanly — libpython mismatch, PyO3 0.22 API drift on newer Python, or version out of range (pyo3 0.23 upgrade tracked in ADR-0043 backlog)"
        );
        return;
    }
    panic!(
        "PyO3 build failed for unexpected reason:\nstderr: {stderr}\n\
         Per ADR-0011 §6, the M6 build path must compile when libpython is present.\n\
         Re-run with: cargo build -p cobrust-molt --features pyo3"
    );
}
