//! Stream W P0 增量 (2026-05-29) — `.cb` end-to-end tests for the 8
//! new free functions added to `coil` (`mgrid` / `ogrid` /
//! `broadcast_to` / `split` + `mean` / `median` / `std` / `var`).
//!
//! Mirrors `coil_hello_e2e.rs`'s compile-spawn-assert pattern. Three
//! tests:
//!
//! 1. Positive — `mgrid + mean + std` (matches `examples/coil_p0/main.cb`).
//!    Asserts the truncated integer parts via `(x as i64)` print
//!    (avoids f-string precision drift).
//! 2. Positive — `mgrid + broadcast_to + median` exercises a second
//!    handle round-trip + the order-statistic reducer.
//! 3. Negative — `coil.mgrid` rejects a `str` argument (mirrors the
//!    first proof's negative corpus).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

fn compile_source(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let build = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}\nstderr: {}",
        build.status,
        String::from_utf8_lossy(&build.stderr)
    );
    (dir, exe)
}

fn try_build(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Positive #1 — `coil.mgrid(0, 10)` then `mean` + `std` printed as
/// `i64`-cast. `mean(0..10) = 4.5` → `(4.5 as i64)` → "4". `std(0..10)
/// = sqrt(8.25) ≈ 2.872` → "2". The two reductions both borrow the
/// same handle via `&a` (ADR-0052a explicit shared borrow — required
/// because `coil.Buffer` is non-Copy and a bare `a` argument would
/// consume the handle, blocking the second reduction).
#[test]
fn test_e2e_coil_p0_mgrid_mean_std() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 10)\n",
        "    let m: f64 = coil.mean(&a)\n",
        "    let s: f64 = coil.std(&a)\n",
        "    print((m as i64))\n",
        "    print((s as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn p0 example");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "4\n2".trim_end(),
        "expected '4\\n2'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #2 — `coil.mgrid` + `broadcast_to` + `median`. Broadcasts
/// the 5-elem range `[0..5)` to 10 elems (tile: 0,1,2,3,4,0,1,2,3,4)
/// then takes the median → sorted middle of [0,0,1,1,2,2,3,3,4,4] is
/// (2 + 2) / 2 = 2.0 → `(2.0 as i64)` → "2".
#[test]
fn test_e2e_coil_p0_broadcast_then_median() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 5)\n",
        "    let b: coil.Buffer = coil.broadcast_to(a, 10)\n",
        "    let m: f64 = coil.median(b)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn p0 example");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "2",
        "expected '2'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Negative — `coil.mgrid` expects two `i64` args; a `str` first
/// argument must be rejected at the manifest-driven typecheck.
#[test]
fn test_neg_coil_mgrid_rejects_str_argument() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(\"zero\", 10)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.mgrid(\"zero\", 10) must be rejected (i64 expected); stderr=\n{stderr}",
    );
}
