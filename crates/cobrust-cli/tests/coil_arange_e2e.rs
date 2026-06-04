//! `coil.arange(n)` — `.cb` end-to-end proof for the #numpy BATCH-20
//! addition: the FINAL core numpy constructor. VERY HIGH-USE — LLMs write
//! `np.arange(n)` constantly and it was the last core constructor MISSING
//! from the `.cb` surface. These tests compile → link → spawn REAL binaries
//! and assert that `arange(n)` produces a 1-D `Int64` buffer `[0, 1, ...,
//! n-1]` whose `print_buffer` repr shows `dtype=int64` + the exact values,
//! proving int-dtype range construction from `.cb` works END-TO-END.
//!
//! ## The shape — an all-scalar-arg `(i64) -> Buffer` producer
//!
//! `arange` takes NO Buffer input — just the scalar `n` — so it MIRRORS
//! `coil.zeros(n)` (the `[Ty::Int] -> Buffer` arg shape), lowering via the
//! GENERIC `try_lower_ecosystem_call` `[Int] -> Buffer` path with ZERO new
//! MIR. The ONLY difference from zeros/ones: the result is an `Int64` buffer
//! (`np.arange(<int>)` is `int64`-dtype on a 64-bit host, so a Float64
//! result would DIVERGE from numpy — the e2e asserts `dtype=int64`).
//!
//! ## The load-bearing semantics (numpy 2.4.6 confirmed via python3.11)
//!
//! - `coil.arange(5) == array([0, 1, 2, 3, 4], dtype=int64)` (C-order,
//!   0-based, `stop`-EXCLUSIVE — `5` itself is NOT present).
//! - `coil.arange(0) == array([], dtype=int64)` (empty).
//! - `coil.arange(-3) == array([], dtype=int64)` — a NEGATIVE `n` gives an
//!   EMPTY int64 buffer, NOT an error/panic (the binary exits ZERO).
//! - Only the 1-arg `arange(stop)` form ships (the fixed-arity ecosystem
//!   signature); `arange(start, stop[, step])` is a documented deferral.
//! - A NON-Int arg (e.g. `arange("x")`) is a COMPILE-TIME reject (the §2.5
//!   compile-time-catch path — `unify_call_arg` against `Ty::Int`).
//!
//! The CHAINS prove the fresh `Int64` buffer is a first-class drop-scheduled
//! handle that feeds downstream ops:
//! - `reshape(arange(6), 2, 3) == [[0, 1, 2], [3, 4, 5]]` (still int64).
//! - `astype(arange(5), "float64") == [0, 1, 2, 3, 4]` float64.
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_astype_e2e`.
//! Results are observed via `coil.print_buffer`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative doc comments read as "lazy" list items to clippy; they
// are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_astype_e2e.rs`.
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

/// Spawn a compiled program; return `(stdout, stderr, success)`.
fn run(exe: &PathBuf) -> (String, String, bool) {
    let out = Command::new(exe).output().expect("spawn coil-arange prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Compile-only helper for negative typecheck cases — returns (success?,
/// stderr).
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
// POSITIVE — the core proof: `arange(5)` is an INT64 buffer [0,1,2,3,4].
// The `dtype=int64` assertion fails any Float64 mutation; the exact-value
// assertion fails any off-by-one (a `stop`-INCLUSIVE bug would append `5`).
// =====================================================================

/// `coil.arange(5)` → `array([0, 1, 2, 3, 4], dtype=int64)`. THIS is int64
/// range construction from `.cb`, end-to-end.
///
/// Oracle (numpy 2.x): `np.arange(5) == [0, 1, 2, 3, 4]`, dtype `int64`.
#[test]
fn test_e2e_arange_5_is_0_to_4_int64() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.arange(5)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("dtype=int64"),
        "expected an int64-dtype Buffer (float64 would diverge from numpy); got stdout=\n{stdout}",
    );
    assert!(
        stdout.contains("[0, 1, 2, 3, 4]"),
        "expected stop-EXCLUSIVE [0, 1, 2, 3, 4] (a stop-inclusive bug appends 5); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `arange(1)` is the single-element edge `[0]` (int64).
// =====================================================================

/// `coil.arange(1)` → `array([0], dtype=int64)` (one element, `0`).
///
/// Oracle (numpy 2.x): `np.arange(1) == [0]`.
#[test]
fn test_e2e_arange_1_is_single_zero() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.arange(1)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([0], dtype=int64)"),
        "expected single-element int64 [0]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// CHAIN — the int64 result feeds `reshape`. `reshape(arange(6), 2, 3)` →
// `[[0, 1, 2], [3, 4, 5]]` (still int64). Proves arange's fresh Buffer is
// a first-class drop-scheduled handle a downstream op consumes (both
// temporaries drop).
// =====================================================================

/// `coil.reshape(coil.arange(6), 2, 3)` → `array([[0, 1, 2], [3, 4, 5]],
/// dtype=int64)`. The int64 dtype survives reshape; arange feeds reshape.
///
/// Oracle (numpy 2.x): `np.arange(6).reshape(2, 3) ==
/// [[0, 1, 2], [3, 4, 5]]`.
#[test]
fn test_e2e_reshape_of_arange_6() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.arange(6)\n",
        "    let r: coil.Buffer = coil.reshape(a, 2, 3)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[0, 1, 2], [3, 4, 5]]") && stdout.contains("dtype=int64"),
        "expected reshape∘arange (2,3) int64 [[0,1,2],[3,4,5]]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// CHAIN — the int64 result feeds `astype`. `astype(arange(5), "float64")`
// → `array([0, 1, 2, 3, 4], dtype=float64)`. Proves arange composes with
// the dtype-conversion op (int64 → float64).
// =====================================================================

/// `coil.astype(coil.arange(5), "float64")` → `array([0, 1, 2, 3, 4],
/// dtype=float64)` (the int64 range widened to float64).
///
/// Oracle (numpy 2.x): `np.arange(5).astype('float64')` is float64
/// `[0., 1., 2., 3., 4.]`.
#[test]
fn test_e2e_astype_of_arange_to_float64() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.arange(5)\n",
        "    let r: coil.Buffer = coil.astype(a, \"float64\")\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[0, 1, 2, 3, 4]") && stdout.contains("dtype=float64"),
        "expected astype∘arange float64 [0, 1, 2, 3, 4]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `arange(0)` is the EMPTY int64 buffer. A non-erroring zero
// case (the binary exits ZERO; the repr shows the empty dtype).
// =====================================================================

/// `coil.arange(0)` → `array([], dtype=int64)` (empty). The binary exits
/// ZERO (no error path).
///
/// Oracle (numpy 2.x): `np.arange(0) == array([], dtype=int64)`.
#[test]
fn test_e2e_arange_0_is_empty() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.arange(0)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([], dtype=int64)"),
        "expected empty int64 buffer; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `arange(-3)` is EMPTY, NOT a trap. A NEGATIVE `n` is
// valid-empty (matching numpy); the binary exits ZERO (proving arange has
// NO error path — unlike astype's unknown-dtype trap).
// =====================================================================

/// `coil.arange(-3)` → `array([], dtype=int64)`. A NEGATIVE `n` yields an
/// EMPTY buffer and the binary exits ZERO — NOT a runtime trap.
///
/// Oracle (numpy 2.x): `np.arange(-3) == array([], dtype=int64)`.
#[test]
fn test_e2e_arange_negative_is_empty_not_trap() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.arange(-3)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(
        ok,
        "negative arange must EXIT ZERO (empty, not a trap); stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert!(
        stdout.contains("array([], dtype=int64)"),
        "expected empty int64 buffer from a negative n; got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE (TYPECHECK) — a non-Int arg is rejected at the manifest-driven
// signature check. `coil.arange`'s param is `i64`, NOT `str`; the
// typechecker catches `arange("x")` BEFORE codegen (the §2.5 compile-time-
// catch path — `unify_call_arg` against `Ty::Int`). arange has no runtime
// error path, so this compile-time reject is the only negative gate.
// =====================================================================

/// `coil.arange("x")` is rejected (int expected for `n`). A type error,
/// NOT a runtime trap — surfaced at the type-check arm.
#[test]
fn test_neg_arange_rejects_str_arg() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.arange(\"x\")\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.arange(\"x\") must be rejected (int expected for n); stderr=\n{stderr}"
    );
}
