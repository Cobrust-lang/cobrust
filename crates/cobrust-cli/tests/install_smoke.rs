//! Install-path smoke test (T1.3).
//!
//! Gated by `COBRUST_INSTALL_SMOKE=1` — skipped on CI runners that
//! don't have time for a full `cargo install` round-trip.
//!
//! What it validates:
//!
//! 1. `cargo install --path crates/cobrust-cli --root <tmpdir>` succeeds.
//! 2. `<tmpdir>/bin/cobrust --version` prints `cobrust 0.1.0-beta`.
//! 3. `<tmpdir>/bin/cobrust new testpkg` scaffolds a package.
//! 4. `<tmpdir>/bin/cobrust run src/main.cb` (from inside `testpkg/`)
//!    prints `hello, world\n` within 5 seconds.
//!
//! Mark with `#[ignore]` and check `COBRUST_INSTALL_SMOKE` manually so
//! the test is skipped by default on slow CI runners.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

#[test]
#[ignore = "set COBRUST_INSTALL_SMOKE=1 and run with -- --ignored to enable"]
fn install_smoke_end_to_end() {
    if std::env::var("COBRUST_INSTALL_SMOKE").as_deref() != Ok("1") {
        // Extra safety: even if --ignored is passed, respect the env gate.
        eprintln!("install_smoke: COBRUST_INSTALL_SMOKE != 1 — skipping");
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let bin_cobrust = root.join("bin").join(if cfg!(windows) {
        "cobrust.exe"
    } else {
        "cobrust"
    });

    // -----------------------------------------------------------------------
    // 1. cargo install --path crates/cobrust-cli --root <tmpdir>
    // -----------------------------------------------------------------------
    let workspace_root = workspace_root();
    let t0 = Instant::now();

    let status = Command::new("cargo")
        .current_dir(&workspace_root)
        .args([
            "install",
            "--path",
            "crates/cobrust-cli",
            "--root",
            root.to_str().expect("root path must be UTF-8"),
            "--locked",
        ])
        .status()
        .expect("cargo install failed to start");

    assert!(
        status.success(),
        "cargo install failed with status: {status:?}"
    );

    let elapsed = t0.elapsed();
    eprintln!("install_smoke: cargo install completed in {elapsed:.1?}");
    assert!(
        bin_cobrust.exists(),
        "expected binary at {}",
        bin_cobrust.display()
    );

    // -----------------------------------------------------------------------
    // 2. cobrust --version
    // -----------------------------------------------------------------------
    let version_out = Command::new(&bin_cobrust)
        .arg("--version")
        .output()
        .expect("cobrust --version failed");
    let version_str = String::from_utf8_lossy(&version_out.stdout).to_string();
    assert!(
        version_str.contains("cobrust") && version_str.contains("0.1.0-beta"),
        "expected 'cobrust 0.1.0-beta' in --version output, got: {version_str:?}"
    );

    // -----------------------------------------------------------------------
    // 3. cobrust new testpkg
    // -----------------------------------------------------------------------
    let pkg_dir = root.join("testpkg");
    let new_status = Command::new(&bin_cobrust)
        .current_dir(root)
        .args(["new", "testpkg"])
        .status()
        .expect("cobrust new failed");
    assert!(
        new_status.success(),
        "cobrust new exited with {new_status:?}"
    );
    assert!(
        pkg_dir.join("src/main.cb").exists(),
        "expected src/main.cb in scaffolded package"
    );
    assert!(
        pkg_dir.join(".gitignore").exists(),
        "expected .gitignore in scaffolded package"
    );
    assert!(
        pkg_dir.join("README.md").exists(),
        "expected README.md in scaffolded package"
    );

    // -----------------------------------------------------------------------
    // 4. cobrust run src/main.cb — must print "hello, world" within 5 s
    // -----------------------------------------------------------------------
    let t_run = Instant::now();
    let run_out = Command::new(&bin_cobrust)
        .current_dir(&pkg_dir)
        .args(["run", "src/main.cb"])
        .output()
        .expect("cobrust run failed");

    let run_elapsed = t_run.elapsed();
    eprintln!("install_smoke: cobrust run took {run_elapsed:.1?}");
    assert!(
        run_elapsed.as_secs() < 5,
        "cobrust run took more than 5 seconds: {run_elapsed:.1?}"
    );

    let stdout = String::from_utf8_lossy(&run_out.stdout);
    assert!(
        run_out.status.success(),
        "cobrust run exited with {}: stderr={:?}",
        run_out.status,
        String::from_utf8_lossy(&run_out.stderr)
    );
    assert_eq!(
        stdout.trim_end_matches('\n'),
        "hello, world",
        "expected stdout = 'hello, world', got: {stdout:?}"
    );
}

/// Locate the workspace root (two directories above this crate's manifest).
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root is two levels above crates/cobrust-cli")
        .to_path_buf()
}
