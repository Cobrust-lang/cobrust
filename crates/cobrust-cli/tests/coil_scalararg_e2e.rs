//! #145 SCALAR-ARG ufunc gap-closure BATCH 6 (2026-06-01) — `.cb`
//! end-to-end tests for `coil.clip(a, lo, hi)` + `coil.power(a, p)`, the
//! FIRST Buffer-RETURNING coil ops to take EXTRA f64 SCALAR args beside the
//! handle (`clip` is the FIRST with TWO trailing f64 bounds; `power` shares
//! `coil.percentile`'s `(Buffer, f64)` arg shape but RETURNS a fresh
//! Buffer).
//!
//! Mirrors `coil_round_e2e.rs`'s compile-spawn-assert pattern. Results are
//! observed via `coil.print_buffer` (the coil numpy-compatible repr —
//! `array([2, 7], dtype=float64)`). Every `.cb` constructor here makes a
//! `float64` buffer, so integer-valued results print without a `.0`.
//!
//! The asserted values are the SAME numpy 2.4.6 oracle values the
//! `coil::elementwise` BATCH-6 unit tests carry (the differential gate's
//! hand-computed shape):
//!
//! 1. Positive — `clip([1, 9], 2, 7) = [2, 7]` (clamp, dtype-preserving;
//!    here a float64 buffer so the repr is `[2, 7]`).
//! 2. Positive — `clip([1, 9], 7, 2) = [2, 2]` (lo > hi → the UPPER bound
//!    wins, numpy `minimum(maximum(a, lo), hi)`).
//! 3. Positive — `power([2, 3], 2.0) = [4, 9]` (square; the `(Buffer, f64)
//!    -> Buffer` shim).
//! 4. Positive — `power([4, 9], 0.5) = [2, 3]` (`x ** 0.5 == sqrt(x)`).
//! 5. Positive — `power([2, 3], 0) = [1, 1]` (`x ** 0 == 1`, even `0 ** 0`).
//! 6. Positive — `clip(power([1, 4], 2.0), 2, 9) = clip([1, 16], 2, 9) =
//!    [2, 9]` — a CHAIN proving a fresh Buffer feeds the next scalar-arg op.
//! 7. Negative — `coil.clip` rejects a `str` bound (the manifest-driven
//!    typecheck of the new `(Buffer, f64, f64)` signature).
//! 8. Negative — `coil.power` rejects a `str` exponent (the `(Buffer, f64)`
//!    signature).

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

fn run(exe: &PathBuf) -> (String, String, bool) {
    let out = Command::new(exe)
        .output()
        .expect("spawn coil-scalararg prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
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

// =====================================================================
// POSITIVE — `clip` clamps to [lo, hi], dtype-preserving.
// =====================================================================

/// `coil.clip(array1d2(1.0, 9.0), 2.0, 7.0)` → `[2, 7]` (Float64). Oracle
/// (numpy 2.x): `np.clip([1., 9.], 2., 7.)` → `array([2., 7.])`. The `1`
/// clamps UP to the lower bound `2`, the `9` clamps DOWN to the upper bound
/// `7`. The repr prints integer-valued floats without a `.0`.
#[test]
fn test_e2e_clip_clamps_to_bounds() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 9.0)\n",
        "    let r: coil.Buffer = coil.clip(a, 2.0, 7.0)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([2, 7], dtype=float64)"),
        "expected clip([1,9],2,7)=[2, 7]; got stdout=\n{stdout}",
    );
}

/// `coil.clip(array1d2(1.0, 9.0), 7.0, 2.0)` with `lo > hi` → `[2, 2]`. The
/// UPPER bound wins (numpy is `minimum(maximum(a, lo), hi)`). Oracle:
/// `np.clip([1., 9.], 7., 2.)` → `array([2., 2.])`. This is the load-bearing
/// edge — Rust's `f64::clamp` would PANIC on `lo > hi`; the shim uses
/// `max(lo).min(hi)` instead.
#[test]
fn test_e2e_clip_lo_gt_hi_clamps_to_hi() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 9.0)\n",
        "    let r: coil.Buffer = coil.clip(a, 7.0, 2.0)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([2, 2], dtype=float64)"),
        "expected clip([1,9],7,2)=[2, 2] (UPPER bound wins on lo>hi); got \
         stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `power` raises to the p-th power (float-promoting).
// =====================================================================

/// `coil.power(array1d2(2.0, 3.0), 2.0)` → `[4, 9]` (square). Oracle:
/// `np.power([2., 3.], 2.0)` → `array([4., 9.])`.
#[test]
fn test_e2e_power_square() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 3.0)\n",
        "    let r: coil.Buffer = coil.power(a, 2.0)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([4, 9], dtype=float64)"),
        "expected power([2,3],2)=[4, 9]; got stdout=\n{stdout}",
    );
}

/// `coil.power(array1d2(4.0, 9.0), 0.5)` → `[2, 3]` (`x ** 0.5 == sqrt(x)`).
/// Oracle: `np.power([4., 9.], 0.5)` → `array([2., 3.])`.
#[test]
fn test_e2e_power_half_is_sqrt() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(4.0, 9.0)\n",
        "    let r: coil.Buffer = coil.power(a, 0.5)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([2, 3], dtype=float64)"),
        "expected power([4,9],0.5)=sqrt=[2, 3]; got stdout=\n{stdout}",
    );
}

/// `coil.power(array1d2(2.0, 3.0), 0.0)` → `[1, 1]` (`x ** 0 == 1`). Oracle:
/// `np.power([2., 3.], 0.0)` → `array([1., 1.])`. (numpy's `0 ** 0 == 1`
/// too — the f64::powf identity.)
#[test]
fn test_e2e_power_zero_exponent_is_one() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 3.0)\n",
        "    let r: coil.Buffer = coil.power(a, 0.0)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 1], dtype=float64)"),
        "expected power([2,3],0)=[1, 1]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — a CHAIN: clip(power(a, p), lo, hi). Proves a fresh Buffer
// from one scalar-arg op feeds the next (the borrow-arg → fresh-return
// value-handle ABI, with the f64 scalars crossing on each call).
// =====================================================================

/// `coil.clip(coil.power(array1d2(1.0, 4.0), 2.0), 2.0, 9.0)` →
/// `clip([1, 16], 2, 9) = [2, 9]`. Oracle: `np.clip(np.power([1.,4.], 2.0),
/// 2., 9.)` → `array([2., 9.])`. The `power` result `[1, 16]` then clamps:
/// `1` UP to `2`, `16` DOWN to `9`.
#[test]
fn test_e2e_clip_of_power_chain() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 4.0)\n",
        "    let sq: coil.Buffer = coil.power(a, 2.0)\n",
        "    let r: coil.Buffer = coil.clip(sq, 2.0, 9.0)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([2, 9], dtype=float64)"),
        "expected clip(power([1,4],2),2,9)=clip([1,16],2,9)=[2, 9]; got \
         stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE — the new signatures are manifest-driven typechecked: a `str`
// bound / exponent must be rejected (the f64 scalar args are not strings).
// =====================================================================

/// `coil.clip(a, lo, hi)` expects `f64` bounds; a `str` lower bound must be
/// rejected at the manifest-driven typecheck of the new 3-arg signature.
#[test]
fn test_neg_coil_clip_rejects_str_bound() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 9.0)\n",
        "    let r: coil.Buffer = coil.clip(a, \"lo\", 7.0)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.clip(a, \"lo\", 7.0) must be rejected (f64 bound expected); \
         stderr=\n{stderr}",
    );
}

/// `coil.power(a, p)` expects an `f64` exponent; a `str` exponent must be
/// rejected at the manifest-driven typecheck of the 2-arg signature.
#[test]
fn test_neg_coil_power_rejects_str_exponent() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 3.0)\n",
        "    let r: coil.Buffer = coil.power(a, \"two\")\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.power(a, \"two\") must be rejected (f64 exponent expected); \
         stderr=\n{stderr}",
    );
}
