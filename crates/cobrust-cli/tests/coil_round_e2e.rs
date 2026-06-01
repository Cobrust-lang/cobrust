//! coil unary ROUNDING / SIGN ufuncs (`abs` / `floor` / `ceil` / `round` /
//! `trunc` / `square` / `sign`) — `.cb` end-to-end proof for the #145
//! BATCH-4 addition: the DTYPE-PRESERVING 1-arg Buffer -> Buffer surface,
//! wired EXACTLY like the BATCH-3 transcendentals (`coil_ufunc_e2e.rs`) and
//! BATCH-2 reshape ops — borrow-Buffer-arg → fresh-Buffer-return, NOT the
//! scalar-return stats.
//!
//! ## The load-bearing semantics (the two numpy-exact nuances)
//!
//! - **`round` = round-half-to-EVEN (banker's rounding)**: `np.round(0.5)=0`,
//!   `np.round(1.5)=2`, `np.round(2.5)=2`, `np.round(-0.5)=-0`. coil uses
//!   Rust `f64::round_ties_even`, NOT `f64::round` (which is
//!   half-away-from-zero — `0.5->1` — WRONG vs numpy). The `-0.5 -> -0.0`
//!   value renders as `-0` in the coil repr (numpy prints `-0.` too).
//! - **`sign(0)=0` and `sign(NaN)=NaN`**: `np.sign(0.0)=0.0`,
//!   `np.sign(-0.0)=0.0`. coil uses an explicit branch, NOT `f64::signum`
//!   (which returns `+1.0` for `0.0`). `sign(x>0)=1`, `sign(x<0)=-1`.
//!
//! ## DTYPE PRESERVATION (the BATCH-4 contract, differs from BATCH-3)
//!
//! These KEEP the input dtype (int->int, f32->f32, f64->f64) — they do NOT
//! promote int -> Float64 like the transcendentals. `floor`/`ceil`/`round`/
//! `trunc` are NO-OPS on integer input. Every `.cb` coil constructor
//! (`array1d2` / `array2x2` / `array2x3` / `mgrid` / `ones` / `zeros`)
//! yields a `Float64` buffer, so these ops return `Float64` here and the
//! integer-valued results print WITHOUT a `.0` suffix (`array([1, 2],
//! dtype=float64)`). (There is no int-DTYPE `.cb` constructor yet — the
//! int->int preservation + int no-op contracts are exhaustively pinned in
//! the `elementwise.rs` Rust unit tests; here we prove the float-DTYPE
//! end-to-end value contract those rules ultimately serve, including the
//! two correctness nuances above.)
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     <op>)` resolves the `Buffer(...) -> Buffer` `EcoSig`;
//!   - MIR `try_lower_ecosystem_call` Case-1 (module free fn) → the GENERIC
//!     `emit_ecosystem_call` borrow-arg → fresh-Buffer-return path (the
//!     SAME path `coil.transpose(a)` / `coil.exp(a)` prove; NO BATCH-4-
//!     specific MIR arm, NO `_=>"any"` gap — the 1-Buffer-arg shape is
//!     identical);
//!   - codegen externs (`llvm_backend.rs`) — `(ptr) -> ptr` ≡
//!     `coil_shape_ty`, identical to `transpose`/`exp`;
//!   - the cabi shims (`cabi.rs`) — `__cobrust_coil_<op>` returning a fresh
//!     Boxed `Buffer` via the shared `buffer_unary` body (TOTAL — no
//!     `coil_panic` path; never unwinds across the C-ABI).
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_ufunc_e2e.rs`.
//! Results are observed via `coil.print_buffer`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_ufunc_e2e.rs`.
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
    let out = Command::new(exe).output().expect("spawn coil-round prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — `round` is round-half-to-EVEN (banker's rounding). THE #1
// correctness nuance: `0.5 -> 0`, `1.5 -> 2`, `2.5 -> 2`, `-0.5 -> -0`.
// (Rust `f64::round` would give `0.5 -> 1`, `2.5 -> 3` — WRONG vs numpy.)
// =====================================================================

/// `coil.round(array2x2(0.5, 1.5, 2.5, -0.5))` → `[[0, 2], [2, -0]]`
/// (Float64). Oracle (numpy 2.x): `np.round([[0.5,1.5],[2.5,-0.5]])` →
/// `array([[ 0.,  2.], [ 2., -0.]])` — banker's rounding (`0.5` and `2.5`
/// both round to the nearest EVEN integer `0` / `2`, not up). The `-0.5`
/// rounds to `-0.0`, which the coil repr prints as `-0`.
#[test]
fn test_e2e_round_bankers_half_to_even() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(0.5, 1.5, 2.5, -0.5)\n",
        "    let r: coil.Buffer = coil.round(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[0, 2], [2, -0]]") && stdout.contains("dtype=float64"),
        "expected banker's round [[0,2],[2,-0]] (0.5->0, 2.5->2, NOT \
         half-away-from-zero); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `sign` over NEGATIVE / ZERO / POSITIVE. THE #2 correctness
// nuance: `sign(0.0)=0` (NOT +1 as `f64::signum` would give).
// =====================================================================

/// `coil.sign(array2x2(-2.5, 0.0, 3.0, -7.0))` → `[[-1, 0], [1, -1]]`
/// (Float64). Oracle: `np.sign([[-2.5,0.],[3.,-7.]])` →
/// `array([[-1.,  0.], [ 1., -1.]])`. The `sign(0.0)=0` case is the
/// load-bearing one — `f64::signum(0.0)` is `+1.0`, WRONG vs numpy.
#[test]
fn test_e2e_sign_neg_zero_pos() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(-2.5, 0.0, 3.0, -7.0)\n",
        "    let s: coil.Buffer = coil.sign(a)\n",
        "    let _ = coil.print_buffer(s)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[-1, 0], [1, -1]]") && stdout.contains("dtype=float64"),
        "expected sign [[-1,0],[1,-1]] (sign(0.0)=0, NOT +1); got \
         stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `abs` over NEGATIVES. `abs(-1.5)=1.5`, `abs(2.5)=2.5`.
// =====================================================================

/// `coil.abs(array1d2(-1.5, 2.5))` → `[1.5, 2.5]` (Float64). Oracle:
/// `np.abs([-1.5, 2.5])` → `array([1.5, 2.5])`.
#[test]
fn test_e2e_abs_negatives() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(-1.5, 2.5)\n",
        "    let r: coil.Buffer = coil.abs(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1.5, 2.5], dtype=float64)"),
        "expected abs([-1.5,2.5])=[1.5, 2.5]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `floor` / `ceil` / `trunc` distinct behavior on negatives.
// `floor(-1.5)=-2` (down), `ceil(-1.5)=-1` (up), `trunc(-1.5)=-1` (toward
// zero — same as ceil for negatives, differs from floor).
// =====================================================================

/// `coil.floor(array1d2(-1.5, 1.5))` → `[-2, 1]` (Float64). Oracle:
/// `np.floor([-1.5, 1.5])` → `array([-2.,  1.])`.
#[test]
fn test_e2e_floor_negatives() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(-1.5, 1.5)\n",
        "    let r: coil.Buffer = coil.floor(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([-2, 1], dtype=float64)"),
        "expected floor([-1.5,1.5])=[-2, 1]; got stdout=\n{stdout}",
    );
}

/// `coil.ceil(array1d2(-1.5, 1.5))` → `[-1, 2]` (Float64). Oracle:
/// `np.ceil([-1.5, 1.5])` → `array([-1.,  2.])` — UP, contrast floor.
#[test]
fn test_e2e_ceil_negatives() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(-1.5, 1.5)\n",
        "    let r: coil.Buffer = coil.ceil(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([-1, 2], dtype=float64)"),
        "expected ceil([-1.5,1.5])=[-1, 2]; got stdout=\n{stdout}",
    );
}

/// `coil.trunc(array1d2(-1.7, 1.7))` → `[-1, 1]` (Float64, toward zero).
/// Oracle: `np.trunc([-1.7, 1.7])` → `array([-1.,  1.])`.
#[test]
fn test_e2e_trunc_toward_zero() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(-1.7, 1.7)\n",
        "    let r: coil.Buffer = coil.trunc(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([-1, 1], dtype=float64)"),
        "expected trunc([-1.7,1.7])=[-1, 1] (toward zero); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `square` over negatives + a 2x2 shape (shape preserved).
// =====================================================================

/// `coil.square(array2x2(2.0, -3.0, 0.0, 4.0))` → `[[4, 9], [0, 16]]`
/// (Float64, shape `(2,2)` preserved). Oracle: `np.square([[2,-3],[0,4]])`
/// → `[[4., 9.], [0., 16.]]`.
#[test]
fn test_e2e_square_2x2() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(2.0, -3.0, 0.0, 4.0)\n",
        "    let r: coil.Buffer = coil.square(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[4, 9], [0, 16]]") && stdout.contains("dtype=float64"),
        "expected square [[4,9],[0,16]] (2x2 float64); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — CHAIN `abs(floor(a))`: proves the fresh-Buffer return of the
// inner op feeds the next op AND that the inner temporary is dropped
// exactly once at scope exit (no leak / no double-free).
// =====================================================================

/// `coil.abs(coil.floor(array1d2(-1.5, 2.5)))` →
/// `abs([-2, 2])` = `[2, 2]` (Float64).
///
/// The intermediate `coil.floor(...)` Buffer is a fresh handle consumed by
/// `coil.abs`; both the intermediate and the final are drop-scheduled by
/// the `.cb` scope. Oracle: `np.abs(np.floor([-1.5, 2.5]))` →
/// `array([2., 2.])`.
#[test]
fn test_e2e_chain_abs_of_floor() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(-1.5, 2.5)\n",
        "    let r: coil.Buffer = coil.abs(coil.floor(a))\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([2, 2], dtype=float64)"),
        "expected abs(floor([-1.5,2.5]))=[2, 2]; got stdout=\n{stdout}",
    );
}
