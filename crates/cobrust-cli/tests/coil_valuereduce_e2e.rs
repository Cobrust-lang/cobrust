//! #145 numpy gap-closure BATCH 7 (2026-06-01) — `.cb` end-to-end tests
//! for the VALUE reductions `coil.min(a)` / `coil.max(a)` / `coil.prod(a)`,
//! completing the scalar-reduction family. Each reduces a whole
//! `coil.Buffer` to a single `f64` — the SAME `(Buffer) -> f64` shape
//! `coil.mean` proves (coil's established scalar-reduction convention).
//!
//! Mirrors `coil_scalararg_e2e.rs` / `coil_reduce_e2e.rs`'s
//! compile-spawn-assert pattern. The scalar `f64` result is observed via
//! the polymorphic `print(x)` (ADR-0064) — `__cobrust_println_float`'s
//! Rust `Display`: an integer-valued f64 renders WITHOUT a `.0` (`2.0` →
//! `2`), `f64::NAN` renders as `NaN`, `+inf` as `inf`.
//!
//! Every `.cb` constructor here yields a Float64 buffer, so
//! `min`/`max`/`prod -> f64` is numpy-EXACT (`np.max(f64_array) -> f64`).
//!
//! The asserted values are the numpy 2.4.6 oracle the `aggregates` BATCH-7
//! unit tests carry:
//!
//! 1. Positive — `min([2, 5]) = 2.0` → prints `2`.
//! 2. Positive — `max([2, 5]) = 5.0` → prints `5`.
//! 3. Positive — `prod([2, 3]) = 6.0` → prints `6`.
//! 4. Positive — NaN PROPAGATES: `max([nan, 0.0]) = nan` → prints `NaN`
//!    (the NaN built via `0.0 / [0.0, 2.0]` left-scalar div, IEEE
//!    `0.0/0.0 = NaN`).
//! 5. Positive — `prod([]) = 1.0` (the multiplicative identity, numpy
//!    `np.prod([]) == 1.0`, NOT a trap) → prints `1`.
//! 6. Positive — `prod([1e308, 1e308])` overflows f64 → `+inf` (numpy
//!    parity) → prints `inf`, runs to completion (NOT a trap).
//! 7. Negative — `min(coil.zeros(0))` on an EMPTY buffer ABORTS with a
//!    clean `coil_panic` (numpy `ValueError`): non-zero exit + the
//!    unreachable success marker is absent.
//! 8. Negative — `max(coil.zeros(0))` on an EMPTY buffer — the twin trap.

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
        .expect("spawn coil-valuereduce prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — min / max / prod VALUES (f64 scalar, printed raw).
// =====================================================================

/// `coil.min(coil.array1d2(2.0, 5.0))` → `2.0` → prints `2`. Oracle (numpy
/// 2.x): `np.min([2., 5.]) == 2.0`. The raw-f64 `print` renders the
/// integer-valued result without a `.0`.
#[test]
fn test_e2e_min_value() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 5.0)\n",
        "    let lo: f64 = coil.min(a)\n",
        "    print(lo)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.lines().any(|l| l.trim() == "2"),
        "expected min([2,5])=2; got stdout=\n{stdout}",
    );
}

/// `coil.max(coil.array1d2(2.0, 5.0))` → `5.0` → prints `5`. Oracle:
/// `np.max([2., 5.]) == 5.0`.
#[test]
fn test_e2e_max_value() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 5.0)\n",
        "    let hi: f64 = coil.max(a)\n",
        "    print(hi)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.lines().any(|l| l.trim() == "5"),
        "expected max([2,5])=5; got stdout=\n{stdout}",
    );
}

/// `coil.prod(coil.array1d2(2.0, 3.0))` → `6.0` → prints `6`. Oracle:
/// `np.prod([2., 3.]) == 6.0`.
#[test]
fn test_e2e_prod_value() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 3.0)\n",
        "    let p: f64 = coil.prod(a)\n",
        "    print(p)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.lines().any(|l| l.trim() == "6"),
        "expected prod([2,3])=6; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — NaN PROPAGATES (like coil.mean). `max([nan, 0.0]) = nan`.
// The NaN is built via `0.0 / [0.0, 2.0]` (left-scalar true-division):
// `0.0/0.0 = NaN` (IEEE 754), `0.0/2.0 = 0.0`, so the buffer is
// `[NaN, 0.0]`. `max` must PROPAGATE the NaN (numpy `np.max([nan,0.])`).
// =====================================================================

/// `coil.max(0.0 / coil.array1d2(0.0, 2.0))` = `max([nan, 0.0])` = `nan` →
/// prints `NaN`. Oracle (numpy 2.x): `np.max([np.nan, 0.]) is nan`. Pins
/// that `max` PROPAGATES NaN (does NOT skip it), mirroring `coil.mean`.
#[test]
fn test_e2e_max_propagates_nan() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let d: coil.Buffer = coil.array1d2(0.0, 2.0)\n",
        // 0.0 / [0.0, 2.0] = [0.0/0.0, 0.0/2.0] = [NaN, 0.0]
        "    let b: coil.Buffer = 0.0 / d\n",
        "    let hi: f64 = coil.max(b)\n",
        "    print(hi)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("NaN"),
        "expected max([NaN,0.0])=NaN (NaN PROPAGATES); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `prod([]) = 1.0` (multiplicative identity, numpy parity —
// NOT a trap) and prod-overflow → +inf (numpy parity, runs to completion).
// =====================================================================

/// `coil.prod(coil.zeros(0))` on an EMPTY buffer → `1.0` (the
/// multiplicative identity) → prints `1`, runs to completion (exit 0).
/// Oracle: `np.prod([]) == 1.0`. This is the load-bearing edge —
/// `prod([])` is NOT a trap (unlike `min`/`max([])`), it is the identity.
#[test]
fn test_e2e_prod_empty_is_one() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(0)\n",
        "    let p: f64 = coil.prod(a)\n",
        "    print(p)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(
        ok,
        "prod of an EMPTY buffer must NOT trap (it is the identity 1.0); \
         got non-zero exit. stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert!(
        stdout.lines().any(|l| l.trim() == "1"),
        "expected prod([])=1.0 (multiplicative identity); got stdout=\n{stdout}",
    );
}

/// `coil.prod(coil.array1d2(1e308, 1e308))` overflows f64 → `+inf` (numpy
/// parity) → prints `inf`, runs to completion (exit 0, NOT a trap). Oracle:
/// `np.prod([1e308, 1e308])` → `inf` (a RuntimeWarning, no exception).
#[test]
fn test_e2e_prod_overflow_is_inf() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1e308, 1e308)\n",
        "    let p: f64 = coil.prod(a)\n",
        "    print(p)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(
        ok,
        "prod overflow must NOT trap (IEEE: it saturates to +inf, numpy \
         returns inf with only a RuntimeWarning); got non-zero exit. \
         stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert!(
        stdout.contains("inf"),
        "expected prod([1e308,1e308]) → IEEE `inf`; got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE — EMPTY-input CLEAN TRAP for min / max. numpy RAISES
// `ValueError`; coil cannot raise across the C-ABI, so the shim
// `coil_panic`s (the stdlib `__cobrust_panic`, which diverges) — a CLEAN
// process abort, NEVER a Rust `panic!` unwind across the FFI (UB). We
// assert a NON-ZERO exit (the trap fired) AND that the unreachable success
// marker never printed (a controlled abort, not a wrong-answer-then-exit).
// (prod has NO such trap — `prod([]) == 1.0`, pinned positive above.)
// =====================================================================

/// `coil.min(coil.zeros(0))` on an EMPTY buffer must ABORT with a non-zero
/// exit (the `coil_panic` clean trap), NOT print the unreachable success
/// marker. Mirrors `coil_reduce_e2e.rs`'s `argmin` empty-trap. Oracle:
/// `np.min([])` raises `ValueError`.
#[test]
fn test_e2e_min_empty_clean_trap() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(0)\n",
        "    let lo: f64 = coil.min(a)\n",
        // Unreachable: min of an empty buffer traps above. If the trap did
        // NOT fire (a regression), this marker would print + the binary
        // would exit 0 — both of which the asserts below reject.
        "    print(999)\n",
        "    print((lo as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, _stderr, ok) = run(&exe);
    assert!(
        !ok,
        "min of an EMPTY buffer must ABORT with a non-zero exit (clean \
         coil_panic trap); instead it exited 0 with stdout=\n{stdout}",
    );
    assert!(
        !stdout.contains("999"),
        "the post-min `print(999)` marker must be UNREACHABLE (the trap \
         fires first); found it in stdout=\n{stdout}",
    );
}

/// `coil.max(coil.zeros(0))` on an EMPTY buffer — the twin of the min trap,
/// pinning `max` ALSO traps cleanly. Same non-zero-exit + unreachable-marker
/// assertions. Oracle: `np.max([])` raises `ValueError`.
#[test]
fn test_e2e_max_empty_clean_trap() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(0)\n",
        "    let hi: f64 = coil.max(a)\n",
        "    print(999)\n",
        "    print((hi as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, _stderr, ok) = run(&exe);
    assert!(
        !ok,
        "max of an EMPTY buffer must ABORT with a non-zero exit (clean \
         coil_panic trap); instead it exited 0 with stdout=\n{stdout}",
    );
    assert!(
        !stdout.contains("999"),
        "the post-max `print(999)` marker must be UNREACHABLE; found it in \
         stdout=\n{stdout}",
    );
}
