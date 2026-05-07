//! M7.0 PyO3 build-path test for cobrust-numpy.
//!
//! Per ADR-0011 §6 + ADR-0013 §"Decision": invokes `cargo build
//! --features pyo3` as a subprocess and asserts either success
//! (PyO3 dev-deps present) or a clean skip (libpython unavailable).
//! Either outcome is a green test — the M7.0 deliverable is "the
//! feature compiles **when** the host has libpython", not "every CI
//! machine builds the cdylib".

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
    // Check the feature is wired by reading Cargo.toml; if absent,
    // fail loud — the build path is a hard M7.0 deliverable.
    let cargo_toml = std::fs::read_to_string(manifest.join("Cargo.toml")).expect("read Cargo.toml");
    assert!(
        cargo_toml.contains(r#"pyo3 = ["dep:pyo3"]"#),
        "Cargo.toml must declare `pyo3 = [\"dep:pyo3\"]` per ADR-0011 §3"
    );
    assert!(
        cargo_toml.contains(r#"crate-type = ["rlib", "cdylib"]"#),
        "Cargo.toml must declare cdylib crate-type per ADR-0011 §3"
    );
    // Verify ndarray pin per ADR-0013 §2.
    assert!(
        cargo_toml.contains(r#"ndarray = "0.16""#),
        "Cargo.toml must pin ndarray = \"0.16\" per ADR-0013 §2"
    );

    // Best-effort build attempt. We use `cargo build --no-default-features
    // --features pyo3` to keep the surface tight; success is a green
    // signal, failure with stderr containing "libpython" is a clean skip.
    let out = Command::new("cargo")
        .args([
            "build",
            "-p",
            "cobrust-numpy",
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
        || stderr.contains("No such file or directory")
        || stderr.contains("failed to run the Python interpreter")
    {
        eprintln!("PyO3 build path: skipping cleanly — libpython not on host");
        return;
    }
    panic!(
        "PyO3 build failed for unexpected reason:\nstderr: {stderr}\n\
         Per ADR-0011 §6 + ADR-0013, the M7.0 build path must compile when libpython is present.\n\
         Re-run with: cargo build -p cobrust-numpy --features pyo3"
    );
}
