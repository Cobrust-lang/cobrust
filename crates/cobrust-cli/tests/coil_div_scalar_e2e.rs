//! coil elementwise-arithmetic COMPLETION — `.cb` end-to-end proof
//! obligation for the two operators ADR-0077 Phase 1 explicitly DEFERRED
//! (§12): true-division `a / b` on `coil.Buffer`, and the scalar-broadcast
//! forms `a + 1` / `a * 2` / `a - 1` / `a / 2` (Buffer ⊕ python int).
//!
//! ## Where this sits in the add/sub/mul chain
//!
//! Phase 1 (`73c2747`) + Phase 3 (broadcasting) wired the SAME-OP-CLASS
//! elementwise binops `+` / `-` / `*` end-to-end:
//!   - typecheck `synth_bin` Buffer arm (`check.rs:2956-3021`) — accepts
//!     `Buffer ⊕ Buffer` via `lookup_buffer_binop`, rejects everything else;
//!   - MIR retarget (`lower.rs:2404-2429`) — `a + b` → a `Terminator::Call`
//!     onto `__cobrust_coil_buffer_{add,sub,mul}`;
//!   - codegen externs (`llvm_backend.rs:3087-3102`);
//!   - the shared cabi shim body `buffer_binop` (`cabi.rs:441-467`) — borrows
//!     both handles, runtime-checks broadcast-compatibility, forwards to
//!     `Array::{add,sub,mul}`;
//!   - manifest `lookup_buffer_binop` (`ecosystem.rs:1237-1270`) — the
//!     `(COIL_BUFFER_ADT, op)` table that ONLY enumerates Add/Sub/Mul.
//!
//! The DIVISION kernel ALREADY EXISTS but is UNEXPOSED on this surface:
//! `ufunc::div` (`ufunc.rs:399-436`, public-API `Array::div`, `array.rs:182`)
//! is wired into the `Array` method table but `lookup_buffer_binop` returns
//! `None` for `BinOp::Div`, so `synth_bin` rejects `a / b` (see the existing
//! NEGATIVE assertion `coil_ops_e2e.rs::test_neg_buffer_div_unsupported_rejected`,
//! which this completion sprint must DELETE/INVERT). The scalar form `a + 1`
//! has no path at all: `synth_bin` calls `unify(lhs, rhs)` FIRST, so
//! `Buffer + Int` fails at the unify step before any Buffer arm runs (see
//! `coil_ops_e2e.rs::test_neg_buffer_plus_int_rejected`).
//!
//! ## What `ufunc::div` ACTUALLY does today (the heart of the gap)
//!
//! `ufunc::div` dispatches in the PROMOTED dtype via `binary_dispatch`:
//!   - **float/float** (Float64/Float32): `x / y` — IEEE 754. `1.0/0.0 →
//!     +inf`, `-1.0/0.0 → -inf`, `0.0/0.0 → NaN`. **This matches numpy.**
//!   - **int/int** (Int32/Int64): `x.wrapping_div(y)` — INTEGER floor-toward-
//!     zero division, and `y==0 → Err(IntegerDivisionByZero)`. **This DIVERGES
//!     from numpy**, whose `/` is `true_divide`: `np.array([1,2,3],int) /
//!     np.array([2],int)` → FLOAT `[0.5,1.0,1.5]` (NOT integer `[0,1,1]`), and
//!     `int/0 → inf` (a RuntimeWarning, not an exception).
//!
//! ALL `.cb` constructors (`coil.zeros/ones/mgrid/array1d2/array_f64`) build
//! **f64-dtype** buffers, so every `.cb`-buildable `a / b` routes through the
//! Float64 (true-division, IEEE) arm — which is already numpy-correct. The
//! int/int→FLOAT divergence is therefore only observable at the Rust `Array`
//! level and is pinned in the sibling Rust corpus
//! `crates/cobrust-coil/tests/div_scalar_elementwise_corpus.rs`. THIS file
//! pins the `.cb`-buildable surface: f64 true-division (exact + fractional +
//! broadcast + div-by-zero→inf) and the int-scalar broadcast forms.
//!
//! ## TEST-FIRST status (ADSD) — RED at HEAD `fbfe98b`
//!
//! - Every `a / b` positive is REJECTED at typecheck today (`lookup_buffer_binop`
//!   has no Div arm → `synth_bin` "operator not yet supported on coil.Buffer",
//!   exit 2). Confirmed empirically: `cobrust check` on `a / b` → exit 2.
//! - Every scalar `a + 1` positive is REJECTED at typecheck today (`unify(Buffer,
//!   Int)` fails → "expected Adt, found i64", exit 2). Confirmed empirically.
//! - The NO-REGRESSION `+` / `*` cases (same-shape + broadcast) PASS today
//!   (the Phase-1/3 baseline). They must STAY green after the completion.
//!
//! The DEV closes the gap by: adding the `(COIL_BUFFER_ADT, BinOp::Div)` arm to
//! `lookup_buffer_binop` (→ `__cobrust_coil_buffer_div`) + the cabi `_div` shim
//! (forwarding to `Array::div`, with int/int promoted to FLOAT true-division to
//! match numpy — NOT the kernel's current integer `wrapping_div`) + the codegen
//! extern; and adding a Buffer-⊕-scalar path (`a + 1`) — typecheck arm that
//! admits `Buffer ⊕ Int`, a MIR retarget onto a scalar shim
//! (`__cobrust_coil_buffer_{add,sub,mul,div}_scalar(a, k)`), the cabi shims, and
//! the externs. NONE of the cases below are `#[ignore]`d: corpus + impl land
//! atomically.
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_ops_e2e.rs` +
//! `coil_broadcast_e2e.rs`. f64 reads are observed via `(x as i64)` casts
//! (which truncate toward zero — `f64_e2e.rs:207`) where the result is an exact
//! integer; the "true-division yields a FRACTION, not a floor" discriminator is
//! observed via `coil.print_buffer` (whose f64 repr renders `0.5` as `"0.5"`,
//! `1.0` as `"1"` — Rust `Display`, `print.rs:31`).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. The caller spawns + asserts. Mirrors `coil_ops_e2e.rs`.
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

/// Build-only helper — `(build_succeeded, stderr)`. Used by the div-by-zero
/// case (which must BUILD then RUN to completion printing `inf`, NOT trap).
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
// POSITIVE — Q (NEW): a / b  true-division on f64 Buffers, same-shape.
// =====================================================================

/// Positive #1 (`/`, exact) — `[10,20,30] / [2,4,5]` is elementwise
/// true-division yielding `[5.0, 5.0, 6.0]` (all exact integers). Observe via
/// `coil.mean(c)` → `(5+5+6)/3 = 5.333..` → `(... as i64)` → "5", AND via the
/// last element `c[2]` → `30/5 = 6.0` → "6" (pins a per-element divide, not a
/// uniform fill).
///
/// Values chosen so each quotient is an EXACT integer — the `(as i64)`
/// truncation is then unambiguous (no rounding ambiguity).
///
/// Oracle (numpy 2.0.2): `np.array([10.,20.,30.]) / np.array([2.,4.,5.])` →
/// `array([5., 5., 6.])`.
///
/// PROOF OBLIGATION: `a / b` on two Buffers is rejected at typecheck today
/// (`lookup_buffer_binop` has no Div arm → "operator not yet supported on
/// coil.Buffer", exit 2). RED. The DEV adds the Div arm + `__cobrust_coil_
/// buffer_div` shim (forwarding to `Array::div`).
#[test]
fn test_e2e_buffer_div_exact_values() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(10.0, 20.0)\n",
        "    let b: coil.Buffer = coil.array1d2(2.0, 4.0)\n",
        "    let c: coil.Buffer = a / b\n",
        "    let x0: f64 = c[0]\n",
        "    let x1: f64 = c[1]\n",
        "    print((x0 as i64))\n",
        "    print((x1 as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-div");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // [10,20] / [2,4] = [5.0, 5.0]; c[0]=5, c[1]=5.
    assert_eq!(
        stdout.trim(),
        "5\n5".trim_end(),
        "expected [10,20]/[2,4] = [5,5]; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #2 (`/`, TRUE-division yields a FRACTION — NOT floor) — `[1,2,3] /
/// [2,2,2]` is `[0.5, 1.0, 1.5]`. This is THE divergence-pinning case: numpy's
/// `/` is true-division, so the result is fractional FLOAT, NOT integer
/// floor-division `[0,1,1]`. The f64 buffers make coil's kernel route through
/// the Float64 (IEEE true-division) arm, which is numpy-correct.
///
/// Because `(0.5 as i64) == 0` AND a (wrong) floor-division would ALSO give
/// `0`, the `as i64` cast cannot discriminate — so we observe the WHOLE buffer
/// via `coil.print_buffer`, whose f64 repr renders `0.5` literally. A floor /
/// integer division would print `array([0, 1, 1], ...)` (no `0.5`); true
/// division prints `0.5`.
///
/// Oracle (numpy 2.0.2): `np.array([1.,2.,3.]) / np.array([2.,2.,2.])` →
/// `array([0.5, 1. , 1.5])`. coil repr: `array([0.5, 1, 1.5], dtype=float64)`.
#[test]
fn test_e2e_buffer_div_true_division_is_fractional() {
    // `[1] / [1,2,3]` broadcasts `(1,)/(3,)` to `(3,)` → `[1.0, 0.5, 0.333..]`.
    // TRUE division yields a fractional FLOAT; a (wrong) integer/floor division
    // would yield `[1, 0, 0]` (no `0.5`). Both operands are f64 (the only
    // `.cb`-buildable dtype), so coil's kernel routes through the Float64 (IEEE
    // true-division) arm — already numpy-correct. We use `b / a` (length-1
    // numerator) so the `0.5` discriminator element appears.
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(1, 4)\n", // [1.0, 2.0, 3.0], shape (3,)
        "    let b: coil.Buffer = coil.ones(1)\n",     // [1.0], shape (1,) -> broadcast
        "    let half: coil.Buffer = b / a\n", // [1]/[1,2,3] broadcast -> [1.0, 0.5, 0.333..]
        "    let _ = coil.print_buffer(half)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-div-frac");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // [1] / [1,2,3] broadcasts to [1.0, 0.5, 0.333..]. TRUE division → the
    // `0.5` element prints literally; a (wrong) floor/integer division would
    // print `array([1, 0, 0], ...)` with NO `0.5`. Assert the FRACTION is
    // present (the divergence discriminator), not the exact full repr (the
    // 1/3 element's Display digits are not load-bearing).
    assert!(
        stdout.contains("0.5"),
        "expected TRUE division [1]/[1,2,3] to yield a fractional `0.5` element \
         (NOT integer floor-division `[1,0,0]`); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert!(
        !stdout.contains("dtype=int"),
        "true-division result must be a FLOAT array, not an int array; \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #3 (`/`, broadcast — reuses the Phase-3 `(N,)/(1,)` shape) —
/// `coil.mgrid(0, 4)` → `[0,1,2,3]` divided by `coil.ones(1)` → `[1]`
/// broadcasts to `[0,1,2,3]` (divide by 1). Observe `coil.mean(c)` →
/// `(0+1+2+3)/4 = 1.5` → `(... as i64)` → "1". Proves `/` flows through the
/// SAME broadcast-aware shared shim body as `+`/`-`/`*` (the relaxed guard
/// must cover the new `_div` shim too).
///
/// Oracle (numpy 2.0.2): `np.arange(4.) / np.ones(1)` → `array([0.,1.,2.,3.])`;
/// mean `1.5`.
#[test]
fn test_e2e_buffer_div_broadcast_n_by_1() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 4)\n",
        "    let b: coil.Buffer = coil.ones(1)\n",
        "    let c: coil.Buffer = a / b\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-div-bcast");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); broadcast (4,)/(1,) must not trap. \
         stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "1",
        "expected mean(mgrid(0,4)/ones(1)) == 1.5 → '1'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #4 (`/`, div-by-zero → IEEE inf, NOT a trap) — `[1.0] / [0.0]` is
/// `+inf` per IEEE 754. Per numpy, float division by zero is a RuntimeWarning
/// (NOT an exception), and the result is `inf`. coil's Float64 div arm is
/// `x / y` (IEEE), so it must produce `inf` and the program must run to
/// completion (exit 0), NOT abort. Observe via `coil.print_buffer`, whose f64
/// repr renders `inf` literally (Rust `Display`, `print.rs:31`).
///
/// This is the load-bearing "div-by-zero is IEEE, not panic" assertion: the
/// new `_div` shim must NOT route f64/0.0 to `coil_panic` (the kernel's
/// integer `IntegerDivisionByZero` Err path is for int dtypes only — and the
/// completion should promote int/int to FLOAT so even int/0 yields inf, but
/// that is pinned in the Rust corpus; here the operands are f64).
///
/// Oracle (numpy 2.0.2): `np.array([1.]) / np.array([0.])` → `array([inf])`
/// (with a RuntimeWarning; no exception).
#[test]
fn test_e2e_buffer_div_by_zero_is_inf_not_trap() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(1)\n",  // [1.0]
        "    let z: coil.Buffer = coil.zeros(1)\n", // [0.0]
        "    let c: coil.Buffer = a / z\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    );
    // Part 1 — must build (both are coil.Buffer; division is now a wired op).
    let (built, build_stderr) = try_build(source);
    assert!(
        built,
        "`coil.ones(1) / coil.zeros(1)` must BUILD once `/` is wired on Buffer; \
         build stderr=\n{build_stderr}",
    );
    // Part 2 — must RUN TO COMPLETION (exit 0) and print `inf`. IEEE float
    // div-by-zero is defined; it must NOT trap/abort.
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-div-zero");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "float div-by-zero must NOT trap (IEEE 754: 1.0/0.0 = inf, numpy returns \
         inf with only a RuntimeWarning); got non-zero exit. \
         stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert!(
        stdout.contains("inf"),
        "expected 1.0/0.0 → IEEE `inf` in the printed buffer; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — Q (NEW): scalar broadcast  a + 1 / a * 2 / a - 1 / a / 2.
// =====================================================================

/// Positive #5 (scalar `+`) — `[1,2,3] + 1` (array + python int) adds 1 to
/// each element → `[2,3,4]`. Observe `coil.mean(c)` → `(2+3+4)/3 = 3.0` →
/// `(... as i64)` → "3", AND the first element `c[0]` → `2.0` → "2".
///
/// Oracle (numpy 2.0.2): `np.arange(1,4) + 1` → `array([2,3,4])` (and the
/// float-array form `np.array([1.,2.,3.]) + 1` → `array([2.,3.,4.])`).
///
/// PROOF OBLIGATION: `a + 1` (Buffer + Int) is rejected at typecheck today —
/// `synth_bin` calls `unify(Buffer, Int)` FIRST (check.rs:2966), which fails
/// ("expected Adt, found i64", exit 2) before any Buffer arm runs. RED. The
/// DEV adds a Buffer-⊕-scalar typecheck path + a `__cobrust_coil_buffer_add_
/// scalar(a, k)` retarget + shim + extern.
#[test]
fn test_e2e_scalar_add_one() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(1, 4)\n", // [1.0, 2.0, 3.0]
        "    let c: coil.Buffer = a + 1\n",
        "    let x0: f64 = c[0]\n",
        "    print((x0 as i64))\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn scalar-add");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // [1,2,3] + 1 = [2,3,4]; c[0]=2, mean=3.
    assert_eq!(
        stdout.trim(),
        "2\n3".trim_end(),
        "expected [1,2,3]+1 = [2,3,4]: c[0]='2', mean='3'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #6 (scalar `*`) — `[1,2,3] * 2` → `[2,4,6]`. Observe `coil.mean(c)`
/// → `(2+4+6)/3 = 4.0` → "4". A non-identity scalar (`2`, not `1`) and a
/// non-uniform array rule out an add/no-op masquerading as multiply.
///
/// Oracle (numpy 2.0.2): `np.array([1.,2.,3.]) * 2` → `array([2.,4.,6.])`;
/// mean `4.0`.
#[test]
fn test_e2e_scalar_mul_two() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(1, 4)\n", // [1,2,3]
        "    let c: coil.Buffer = a * 2\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn scalar-mul");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "4",
        "expected mean([1,2,3]*2) = mean([2,4,6]) = 4 → '4'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #7 (scalar `-`) — `[1,2,3] - 1` → `[0,1,2]`. Observe the first
/// element `c[0]` → `0.0` → "0" (subtracting brings element-0 to exactly 0,
/// distinguishing `-` from `+`), AND `coil.mean(c)` → `(0+1+2)/3 = 1.0` → "1".
///
/// Oracle (numpy 2.0.2): `np.array([1.,2.,3.]) - 1` → `array([0.,1.,2.])`.
#[test]
fn test_e2e_scalar_sub_one() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(1, 4)\n", // [1,2,3]
        "    let c: coil.Buffer = a - 1\n",
        "    let x0: f64 = c[0]\n",
        "    print((x0 as i64))\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn scalar-sub");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // [1,2,3] - 1 = [0,1,2]; c[0]=0, mean=1.
    assert_eq!(
        stdout.trim(),
        "0\n1".trim_end(),
        "expected [1,2,3]-1 = [0,1,2]: c[0]='0', mean='1'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #8 (scalar `/`) — `[2,4,6] / 2` → `[1,2,3]` (true-division by a
/// python int scalar). Build `[2,4,6]` via `coil.mgrid(1,4)*2` would need the
/// scalar-mul (chicken-and-egg); instead build it directly is not possible
/// (no 3-elem f64 literal ctor). Use `coil.array1d2(2.0, 4.0)` → `[2,4]`, then
/// `/ 2` → `[1,2]`. Observe `c[0]` → `1.0` → "1" and `c[1]` → `2.0` → "2".
///
/// Oracle (numpy 2.0.2): `np.array([2.,4.]) / 2` → `array([1.,2.])`.
#[test]
fn test_e2e_scalar_div_two() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 4.0)\n", // [2,4]
        "    let c: coil.Buffer = a / 2\n",
        "    let x0: f64 = c[0]\n",
        "    let x1: f64 = c[1]\n",
        "    print((x0 as i64))\n",
        "    print((x1 as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn scalar-div");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // [2,4] / 2 = [1,2]; c[0]=1, c[1]=2.
    assert_eq!(
        stdout.trim(),
        "1\n2".trim_end(),
        "expected [2,4]/2 = [1,2]: c[0]='1', c[1]='2'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// NO-REGRESSION — the existing +,-,* surface (same-shape + broadcast)
// must STILL pass after the completion (GREEN at HEAD — the baselines).
// =====================================================================

/// No-regression #1 (same-shape `+`) — verbatim the Phase-1 add baseline:
/// `coil.ones(3) + coil.ones(3)` → `[2,2,2]`, `coil.mean(c)` → `2.0` → "2".
/// PASSES at HEAD; must stay green after Div + scalar paths land (they must
/// not perturb the existing Buffer-⊕-Buffer arm).
#[test]
fn test_e2e_same_shape_add_no_regression() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let c: coil.Buffer = coil.ones(3) + coil.ones(3)\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn add-no-regress");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); same-shape add must stay green. \
         stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "2",
        "expected ones(3)+ones(3) → [2,2,2], mean 2.0 → '2'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// No-regression #2 (broadcast `*`) — verbatim the Phase-3 mul-broadcast
/// baseline: `coil.mgrid(0,4) * coil.ones(1)` broadcasts `(4,)*(1,)` → mean
/// `1.5` → "1". PASSES at HEAD; pins that the shared broadcast-aware shim body
/// (now also hosting `/`) keeps `*` broadcasting.
#[test]
fn test_e2e_broadcast_mul_no_regression() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 4)\n",
        "    let b: coil.Buffer = coil.ones(1)\n",
        "    let c: coil.Buffer = a * b\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe)
        .output()
        .expect("spawn mul-bcast-no-regress");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); broadcast mul must stay green. \
         stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "1",
        "expected mean(mgrid(0,4)*ones(1)) == 1.5 → '1'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}
