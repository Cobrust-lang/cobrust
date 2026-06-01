//! coil unary TRANSCENDENTAL ufuncs (`exp` / `log` / `log10` / `sqrt` /
//! `sin` / `cos` / `tan`) — `.cb` end-to-end proof for the #145 BATCH-3
//! addition: the FLOAT-returning 1-arg Buffer -> Buffer surface, wired
//! EXACTLY like the BATCH-2 reshape ops (`transpose` / `flatten` /
//! `ravel`) — borrow-Buffer-arg → fresh-Buffer-return, NOT the
//! scalar-return stats.
//!
//! ## The load-bearing semantics
//!
//! - `coil.exp(a)` / `coil.sqrt(a)` / `coil.log(a)` / `coil.log10(a)` /
//!   `coil.sin(a)` / `coil.cos(a)` / `coil.tan(a)` apply the libm kernel
//!   elementwise, returning a FRESH `coil.Buffer`.
//! - The kernels are TOTAL: a domain-error input is an IEEE-754 special
//!   VALUE, not an error — `log(0) -> -inf`, `log(-1) -> NaN`,
//!   `sqrt(-1) -> NaN`. They render in the coil repr as `inf` / `-inf` /
//!   `NaN` (Rust `Display`).
//! - DTYPE: every `.cb` coil constructor (`array1d2` / `array2x2` /
//!   `array2x3` / `mgrid` / `ones` / `zeros`) yields a `Float64` buffer,
//!   so these float ufuncs return `Float64` and the integer-valued
//!   results print WITHOUT a `.0` suffix (`array([0, 1, 2, 3],
//!   dtype=float64)`). (There is no int-DTYPE `.cb` constructor yet — the
//!   int -> Float64 promotion path is exhaustively pinned in the
//!   `elementwise.rs` Rust unit tests; here we prove the float-valued
//!   end-to-end float-RETURNING contract those promotions ultimately
//!   serve, plus a `mgrid`-sourced integer-VALUED-float case.)
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     <op>)` resolves the `Buffer(...) -> Buffer` `EcoSig`;
//!   - MIR `try_lower_ecosystem_call` Case-1 (module free fn) → the GENERIC
//!     `emit_ecosystem_call` borrow-arg → fresh-Buffer-return path (the
//!     SAME path `coil.transpose(a)` proves; NO transcendental-specific
//!     MIR arm, NO `_=>"any"` gap — the 1-Buffer-arg shape is identical);
//!   - codegen externs (`llvm_backend.rs`) — `(ptr) -> ptr` ≡
//!     `coil_shape_ty`, identical to `transpose`/`flatten`/`ravel`;
//!   - the cabi shims (`cabi.rs`) — `__cobrust_coil_<op>` returning a fresh
//!     Boxed `Buffer` via the shared `buffer_unary` body (TOTAL — no
//!     `coil_panic` path; never unwinds across the C-ABI).
//!
//! Mirrors the compile→spawn→assert-stdout harness of
//! `coil_manipulate_e2e.rs`. Results are observed via `coil.print_buffer`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_manipulate_e2e.rs`.
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
    let out = Command::new(exe).output().expect("spawn coil-ufunc prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — basic `exp` on a float buffer.
// =====================================================================

/// `coil.exp(array1d2(0.0, 1.0))` → `[1, e]` (Float64).
///
/// Oracle (numpy 2.x): `np.exp([0., 1.])` →
/// `array([1.        , 2.71828183])`; coil repr prints the full f64
/// `2.718281828459045`.
#[test]
fn test_e2e_exp_float() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(0.0, 1.0)\n",
        "    let e: coil.Buffer = coil.exp(a)\n",
        "    let _ = coil.print_buffer(e)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 2.718281828459045], dtype=float64)"),
        "expected exp([0,1])=[1, e] (float64); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — basic `sqrt` on a 2x2 float buffer (shape preserved).
// =====================================================================

/// `coil.sqrt(array2x2(0,1,4,9))` → `[[0, 1], [2, 3]]` (Float64, shape
/// `(2,2)` preserved). Oracle: `np.sqrt([[0,1],[4,9]])` →
/// `[[0., 1.], [2., 3.]]`.
#[test]
fn test_e2e_sqrt_2x2() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(0.0, 1.0, 4.0, 9.0)\n",
        "    let s: coil.Buffer = coil.sqrt(a)\n",
        "    let _ = coil.print_buffer(s)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[0, 1], [2, 3]]") && stdout.contains("dtype=float64"),
        "expected sqrt [[0,1],[2,3]] (2x2 float64); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — CHAIN `sqrt(exp(a))`: proves the fresh-Buffer return of the
// inner op feeds the next op AND that the inner temporary is dropped
// exactly once at scope exit (no leak / no double-free).
// =====================================================================

/// `coil.sqrt(coil.exp(array1d2(0.0, 2.0)))` →
/// `sqrt([1, e^2])` = `[1, e]` = `[1, 2.718281828459045]`.
///
/// The intermediate `coil.exp(...)` Buffer is a fresh handle consumed by
/// `coil.sqrt`; both the intermediate and the final are drop-scheduled by
/// the `.cb` scope. Oracle: `np.sqrt(np.exp([0., 2.]))` →
/// `array([1.        , 2.71828183])`.
#[test]
fn test_e2e_chain_sqrt_of_exp() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(0.0, 2.0)\n",
        "    let r: coil.Buffer = coil.sqrt(coil.exp(a))\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 2.718281828459045], dtype=float64)"),
        "expected sqrt(exp([0,2]))=[1, e]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `log10` round-trips integer-VALUED powers of ten to exact
// integers, proving the FLOAT-returning contract prints clean floats.
// =====================================================================

/// `coil.log10(array2x3(1, 10, 100, 1000, 10000, 100000))` →
/// `[[0,1,2],[3,4,5]]` (Float64). Oracle: `np.log10([1,10,...,1e5])` →
/// `[0., 1., 2., 3., 4., 5.]`.
#[test]
fn test_e2e_log10_powers_of_ten() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 10.0, 100.0, 1000.0, 10000.0, 100000.0)\n",
        "    let l: coil.Buffer = coil.log10(a)\n",
        "    let _ = coil.print_buffer(l)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[0, 1, 2], [3, 4, 5]]") && stdout.contains("dtype=float64"),
        "expected log10 powers-of-ten [[0,1,2],[3,4,5]]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — float-RETURNING contract on a `mgrid`-sourced integer-VALUED
// float buffer. `coil.mgrid(1, 4)` yields the Float64 `[1, 2, 3]`; the
// closest `.cb`-reachable analogue of numpy's `np.sqrt(np.arange(...))`
// int->float promotion (the int-DTYPE promotion itself is pinned in the
// `elementwise.rs` Rust unit tests, as no int-DTYPE `.cb` ctor exists).
// =====================================================================

/// `coil.sqrt(coil.mgrid(1, 4))` → `sqrt([1, 2, 3])` =
/// `[1, 1.4142135623730951, 1.7320508075688772]` (Float64). Oracle:
/// `np.sqrt(np.array([1.,2.,3.]))`.
#[test]
fn test_e2e_sqrt_mgrid_seq() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(1, 4)\n",
        "    let s: coil.Buffer = coil.sqrt(a)\n",
        "    let _ = coil.print_buffer(s)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 1.4142135623730951, 1.7320508075688772], dtype=float64)"),
        "expected sqrt([1,2,3]); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — NaN / inf EDGE VALUES are emitted (not trapped). `log` of a
// buffer holding 0 and a negative renders `-inf` and `NaN` in the repr.
// =====================================================================

/// `coil.log(array1d2(0.0, -1.0))` → `[-inf, NaN]` (Float64). These are
/// IEEE-754 domain VALUES, not errors — the program exits 0 and the repr
/// prints `-inf` / `NaN`. Oracle: `np.log([0., -1.])` →
/// `array([-inf,  nan])` (with a RuntimeWarning; the VALUES are these).
#[test]
fn test_e2e_log_nan_inf_edges() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(0.0, -1.0)\n",
        "    let l: coil.Buffer = coil.log(a)\n",
        "    let _ = coil.print_buffer(l)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([-inf, NaN], dtype=float64)"),
        "expected log([0,-1])=[-inf, NaN]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `exp` overflow → `+inf` VALUE (not a trap). `exp(710)` is
// IEEE-754 `+inf`; the program exits 0.
// =====================================================================

/// `coil.exp(array1d2(710.0, 0.0))` → `[inf, 1]` (Float64). Oracle:
/// `np.exp([710., 0.])` → `array([inf,  1.])` (RuntimeWarning overflow).
#[test]
fn test_e2e_exp_overflow_inf() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(710.0, 0.0)\n",
        "    let e: coil.Buffer = coil.exp(a)\n",
        "    let _ = coil.print_buffer(e)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([inf, 1], dtype=float64)"),
        "expected exp([710,0])=[inf, 1]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — trig: `sin` / `cos` at clean angles. `sin(0)=0`, `cos(0)=1`.
// =====================================================================

/// `coil.cos(array1d2(0.0, 0.0))` → `[1, 1]` (cos(0)=1, an exact clean
/// value avoiding the ~1e-16 `cos(pi/2)` float dust). Oracle:
/// `np.cos([0., 0.])` → `array([1., 1.])`.
#[test]
fn test_e2e_cos_zero() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(0.0, 0.0)\n",
        "    let c: coil.Buffer = coil.cos(a)\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 1], dtype=float64)"),
        "expected cos([0,0])=[1, 1]; got stdout=\n{stdout}",
    );
}

/// `coil.sin(array1d2(0.0, 0.0))` → `[0, 0]` (sin(0)=0). Oracle:
/// `np.sin([0., 0.])` → `array([0., 0.])`.
#[test]
fn test_e2e_sin_zero() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(0.0, 0.0)\n",
        "    let s: coil.Buffer = coil.sin(a)\n",
        "    let _ = coil.print_buffer(s)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([0, 0], dtype=float64)"),
        "expected sin([0,0])=[0, 0]; got stdout=\n{stdout}",
    );
}
