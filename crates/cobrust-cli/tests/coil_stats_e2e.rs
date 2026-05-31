//! #145 statistics gap-closure (2026-06-01) — `.cb` end-to-end tests for
//! the 5 new scalar aggregates added to `coil` (`ptp` / `nansum` /
//! `nanmean` / `nanstd` + `percentile`).
//!
//! Mirrors `coil_p0_e2e.rs`'s compile-spawn-assert pattern. The values
//! asserted below are the SAME numpy 2.0.2 oracle values the
//! `coil::aggregates` unit tests carry (the differential gate's
//! hand-computed shape). Four tests:
//!
//! 1. Positive — `mgrid + ptp + nansum + percentile` (matches
//!    `examples/coil_stats/main.cb`). Asserts truncated integer parts via
//!    `(x as i64)` print (avoids f-string precision drift).
//! 2. Positive — `array1d2` (explicit data incl. an out-of-NaN value) +
//!    `nanmean` + `nanstd` exercises the NaN-aware reducers from `.cb`.
//! 3. Positive — `percentile(a, 25.0)` interpolates (the `(Buffer, f64)
//!    -> f64` 2-arg shim path) — `np.percentile([1,2,3,4], 25) = 1.75`.
//! 4. Negative — `coil.percentile` rejects a `str` quantile argument
//!    (the manifest-driven typecheck of the new 2-arg signature).

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

/// Positive #1 — `coil.mgrid(0, 5)` → range `[0,1,2,3,4]`, then `ptp`
/// (= 4), `nansum` (= 10), `percentile(_, 50)` (= 2.0, the median). All
/// three reductions borrow the same handle via `&a` (ADR-0052a explicit
/// shared borrow — `coil.Buffer` is non-Copy and a bare `a` would consume
/// the handle, blocking the later reductions). Printed `i64`-cast:
/// `4`, `10`, `2`.
#[test]
fn test_e2e_coil_stats_ptp_nansum_percentile() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 5)\n",
        "    let r: f64 = coil.ptp(&a)\n",
        "    let s: f64 = coil.nansum(&a)\n",
        "    let p: f64 = coil.percentile(&a, 50.0)\n",
        "    print((r as i64))\n",
        "    print((s as i64))\n",
        "    print((p as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn stats example");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "4\n10\n2".trim_end(),
        "expected '4\\n10\\n2'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #2 — the NaN-aware reducers from `.cb`. `coil.array1d2(2.0,
/// 4.0)` builds the explicit 2-element buffer `[2.0, 4.0]`. `nanmean` =
/// (2+4)/2 = 3.0; `nanstd` (population, no NaN here) = 1.0 (mean 3,
/// var ((1)^2+(1)^2)/2 = 1, sqrt = 1). Printed `i64`-cast: `3`, `1`.
/// (A `.cb`-constructible NaN literal is not yet on the surface, so the
/// NaN-SKIPPING behavior is covered by the Rust unit + cabi tests; this
/// proves the reducers are callable + correct on real data from `.cb`.)
#[test]
fn test_e2e_coil_stats_nanmean_nanstd() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 4.0)\n",
        "    let m: f64 = coil.nanmean(&a)\n",
        "    let s: f64 = coil.nanstd(&a)\n",
        "    print((m as i64))\n",
        "    print((s as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn stats example");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "3\n1".trim_end(),
        "expected '3\\n1'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #3 — `percentile` interpolation via the `(Buffer, f64) ->
/// f64` shim. `coil.array2x2(1,2,3,4)` flattens to `[1,2,3,4]`;
/// `percentile(_, 25.0)` = 1.75 (linear interp: pos = 3*0.25 = 0.75,
/// 1 + 0.75*(2-1) = 1.75). Printed scaled-by-100 then `i64`-cast (=175)
/// so the fractional part is asserted without f-string precision drift.
#[test]
fn test_e2e_coil_stats_percentile_interpolates() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let p: f64 = coil.percentile(&a, 25.0)\n",
        "    let scaled: f64 = p * 100.0\n",
        "    print((scaled as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn stats example");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "175",
        "expected '175' (1.75 * 100); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Negative — `coil.percentile(a, q)` expects an `f64` quantile; a `str`
/// second argument must be rejected at the manifest-driven typecheck of
/// the new 2-arg signature (mirrors the P0 negative corpus).
#[test]
fn test_neg_coil_percentile_rejects_str_quantile() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 5)\n",
        "    let p: f64 = coil.percentile(&a, \"half\")\n",
        "    print((p as i64))\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.percentile(&a, \"half\") must be rejected (f64 expected); stderr=\n{stderr}",
    );
}
