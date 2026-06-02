//! #145 numpy gap-closure BATCH 11 — `.cb` end-to-end tests for the
//! spacing / value CONSTRUCTORS `coil.linspace` / `coil.logspace` /
//! `coil.full`. These are ALL-SCALAR-ARG producers (NO Buffer input):
//! each allocates a fresh `float64` 1-D buffer the `.cb` caller owns +
//! scope-exit drops. The mirror of the `coil.zeros(n)` /
//! `coil.array1d2(a, b)` all-scalar-arg constructor shape.
//!
//! Mirrors `coil_scalararg_e2e.rs`'s compile-spawn-assert pattern. Results
//! are observed via `coil.print_buffer` (the coil numpy-compatible repr —
//! `array([0, 0.25, 0.5, 0.75, 1], dtype=float64)`; coil's repr uses
//! Rust's `f64::Display`, so integer-valued floats print without a `.0`
//! and `0.25` / `2.5` print as-is). A `coil.mean(...)` chain proves the
//! fresh constructor handle feeds another op.
//!
//! Oracle: numpy 2.x via `/opt/homebrew/bin/python3.11`:
//!
//! 1. `linspace(0, 1, 5) == [0, .25, .5, .75, 1]` (endpoint-inclusive;
//!    `[4]` is EXACTLY `1.0`, the numpy endpoint-pin).
//! 2. `linspace(0, 10, 5) == [0, 2.5, 5, 7.5, 10]` (step = 10/(5-1) = 2.5).
//! 3. `linspace(2, 3, 2) == [2, 3]` (num==2 → just the two endpoints).
//! 4. `linspace(0, 1, 1) == [0]` (num==1 → just `start`).
//! 5. `logspace(0, 2, 3) == [1, 10, 100]` (`10 ** linspace(0, 2, 3)`).
//! 6. `full(3, 5.0) == [5, 5, 5]` (n copies of value).
//! 7. CHAIN — `mean(linspace(0, 10, 5)) == 5.0` (constructor → reducer).
//! 8. Negative — `coil.linspace` rejects a `str` `num` (the manifest-driven
//!    typecheck of the new `[Float, Float, Int]` signature).

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
        .expect("spawn coil-construct prog");
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
// POSITIVE — linspace is endpoint-INCLUSIVE (numpy `endpoint=True`).
// =====================================================================

/// `coil.linspace(0.0, 1.0, 5)` → `[0, 0.25, 0.5, 0.75, 1]`. Oracle:
/// `np.linspace(0, 1, 5)` → `array([0., 0.25, 0.5, 0.75, 1.])`. The last
/// element is EXACTLY `1.0` (numpy pins the endpoint to `stop`), printed
/// as `1` by coil's `f64::Display` repr.
#[test]
fn test_e2e_linspace_0_1_5_endpoint_inclusive() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.linspace(0.0, 1.0, 5)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([0, 0.25, 0.5, 0.75, 1], dtype=float64)"),
        "expected linspace(0,1,5)=[0, 0.25, 0.5, 0.75, 1]; got stdout=\n{stdout}",
    );
}

/// `coil.linspace(0.0, 10.0, 5)` → `[0, 2.5, 5, 7.5, 10]`. Oracle:
/// `np.linspace(0, 10, 5)` → `array([0., 2.5, 5., 7.5, 10.])`. step =
/// `10 / (5 - 1) = 2.5`.
#[test]
fn test_e2e_linspace_0_10_5_step_two_point_five() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.linspace(0.0, 10.0, 5)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([0, 2.5, 5, 7.5, 10], dtype=float64)"),
        "expected linspace(0,10,5)=[0, 2.5, 5, 7.5, 10]; got stdout=\n{stdout}",
    );
}

/// `coil.linspace(2.0, 3.0, 2)` → `[2, 3]` — num==2 yields exactly the two
/// endpoints. Oracle: `np.linspace(2, 3, 2)` → `array([2., 3.])`.
#[test]
fn test_e2e_linspace_2_3_2_both_endpoints() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.linspace(2.0, 3.0, 2)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([2, 3], dtype=float64)"),
        "expected linspace(2,3,2)=[2, 3]; got stdout=\n{stdout}",
    );
}

/// `coil.linspace(0.0, 1.0, 1)` → `[0]` — num==1 yields just `start`.
/// Oracle: `np.linspace(0, 1, 1)` → `array([0.])`. This is the
/// single-sample edge (no consecutive pair; numpy's step is `NaN` but the
/// `.cb` surface returns only the buffer).
#[test]
fn test_e2e_linspace_num_one_is_start_only() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.linspace(0.0, 1.0, 1)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([0], dtype=float64)"),
        "expected linspace(0,1,1)=[0] (just start); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — logspace is `10 ** linspace(start, stop, num)`.
// =====================================================================

/// `coil.logspace(0.0, 2.0, 3)` → `[1, 10, 100]`. Oracle:
/// `np.logspace(0, 2, 3)` → `array([1., 10., 100.])` (`10**0`, `10**1`,
/// `10**2`).
#[test]
fn test_e2e_logspace_0_2_3_base10() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.logspace(0.0, 2.0, 3)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 10, 100], dtype=float64)"),
        "expected logspace(0,2,3)=[1, 10, 100]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — full is `n` copies of `value`.
// =====================================================================

/// `coil.full(3, 5.0)` → `[5, 5, 5]`. Oracle: `np.full(3, 5.0)` →
/// `array([5., 5., 5.])`.
#[test]
fn test_e2e_full_3_copies_of_five() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.full(3, 5.0)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([5, 5, 5], dtype=float64)"),
        "expected full(3,5)=[5, 5, 5]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — a CHAIN: mean(linspace(...)). Proves the fresh constructor
// Buffer feeds a downstream reducer (the all-scalar-arg producer → the
// borrow-Buffer-arg → f64 reducer ABI, with the scalars crossing on the
// constructor call).
// =====================================================================

/// `coil.mean(coil.linspace(0.0, 10.0, 5))` → `5.0`. Oracle:
/// `np.mean(np.linspace(0, 10, 5))` → `5.0` (mean of `[0, 2.5, 5, 7.5,
/// 10]`). The reducer returns an `f64`; we `print((m as i64))` (the
/// proven scalar-reducer e2e pattern) — `5.0 as i64 == 5`.
#[test]
fn test_e2e_mean_of_linspace_chain() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.linspace(0.0, 10.0, 5)\n",
        "    let m: f64 = coil.mean(a)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.lines().any(|l| l.trim() == "5"),
        "expected mean(linspace(0,10,5))=5; got stdout=\n{stdout}",
    );
}

/// `coil.mean(coil.full(4, 7.0))` → `7.0`. Oracle: `np.mean(np.full(4,
/// 7.0))` → `7.0`. A second constructor → reducer chain (full feeds mean).
#[test]
fn test_e2e_mean_of_full_chain() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.full(4, 7.0)\n",
        "    let m: f64 = coil.mean(a)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.lines().any(|l| l.trim() == "7"),
        "expected mean(full(4,7))=7; got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE — the new signatures are manifest-driven typechecked: a `str`
// `num` (linspace) / `value` (full) must be rejected.
// =====================================================================

/// `coil.linspace(start, stop, num)` expects an `i64` `num`; a `str` `num`
/// must be rejected at the manifest-driven typecheck of the new
/// `[Float, Float, Int]` signature.
#[test]
fn test_neg_coil_linspace_rejects_str_num() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.linspace(0.0, 1.0, \"five\")\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.linspace(0.0, 1.0, \"five\") must be rejected (i64 num expected); \
         stderr=\n{stderr}",
    );
}

/// `coil.full(n, value)` expects an `f64` `value`; a `str` `value` must be
/// rejected at the manifest-driven typecheck of the `[Int, Float]`
/// signature.
#[test]
fn test_neg_coil_full_rejects_str_value() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.full(3, \"x\")\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.full(3, \"x\") must be rejected (f64 value expected); stderr=\n{stderr}",
    );
}
