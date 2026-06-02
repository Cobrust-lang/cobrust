//! coil PREDICATE ufuncs (`isnan` / `isinf` / `isfinite`) — `.cb`
//! end-to-end proof for the #163 BATCH-12 addition: the 1-arg
//! `Buffer -> Buffer` predicate surface whose RESULT is ALWAYS a
//! BOOL-dtype Buffer (the per-element MASK), REGARDLESS of the input
//! dtype (`np.isnan(x).dtype == bool`) — like the `a < b` comparison
//! (`coil_compare_e2e.rs`), but as a UNARY op.
//!
//! ## The load-bearing semantics (numpy 2.0.2 oracle)
//!
//! - **`isnan(x)`** — element IS NaN. `np.isnan(nan)=True`,
//!   `np.isnan(inf)=False`, `np.isnan(1.0)=False`.
//! - **`isinf(x)`** — element IS +inf OR -inf. BOTH signs are `True`;
//!   `np.isinf(nan)=False`.
//! - **`isfinite(x)`** — element is FINITE (NOT NaN AND NOT inf).
//!   `np.isfinite(1.0)=True`, `np.isfinite(nan)=False`,
//!   `np.isfinite(inf)=False`. The exact complement of `isnan OR isinf`.
//!
//! ## RESULT DTYPE = Bool (the BATCH-12 contract, differs from every
//! prior unary ufunc)
//!
//! Every other 1-arg ufunc on this surface either preserves the dtype
//! (`abs`/`floor`) or promotes int→Float64 (`exp`/`sqrt`). The predicates
//! instead ALWAYS yield a `Dtype::Bool` MASK — so `coil.print_buffer`
//! renders the result as `array([True, False, ...], dtype=bool)`
//! (`print.rs` `format_bool_nested`), NOT a numeric `array([...],
//! dtype=float64)`. That bool-dtype repr IS the load-bearing observable
//! here.
//!
//! ## Building NaN / inf in `.cb` (no nan/inf literal yet)
//!
//! There is no `.cb` NaN / inf literal, so the fixtures are built via
//! IEEE float division (`coil_div_scalar_e2e.rs` proved `a / b` on f64
//! Buffers): `0.0/0.0 → NaN`, `1.0/0.0 → +inf`, `x/1.0 → x` (finite).
//! `coil.array1d2(a, b)` is the only 2-element `.cb` constructor, so each
//! fixture is a length-2 vector mixing one special value + one finite.
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     <op>)` resolves a `Buffer(...) -> Buffer` `EcoSig`, tier `Strict`;
//!   - MIR `try_lower_ecosystem_call` Case-1 (module free fn) → the GENERIC
//!     `emit_ecosystem_call` borrow-arg → fresh-Buffer-return path (the
//!     SAME path `coil.transpose(a)` / `coil.abs(a)` prove; NO BATCH-12-
//!     specific MIR arm, NO `_=>"any"` gap — the bool-dtype result rides
//!     the IDENTICAL opaque-handle return);
//!   - codegen externs (`llvm_backend.rs`) — `(ptr) -> ptr` ≡
//!     `coil_shape_ty`, identical to `transpose`/`abs`;
//!   - the cabi shims (`cabi.rs`) — `__cobrust_coil_<op>` returning a fresh
//!     Boxed BOOL-dtype `Buffer` via the shared `buffer_unary` body (TOTAL
//!     — a predicate never fails, so no `coil_panic` path).
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_round_e2e.rs`.
//! The bool mask is observed via `coil.print_buffer`; the "has any NaN"
//! chain is observed via `coil.any` + the `if b:` idiom.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_round_e2e.rs`.
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
    let out = Command::new(exe)
        .output()
        .expect("spawn coil-predicate prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — `isnan` over a [NaN, finite] buffer → [True, False]. THE
// load-bearing facts: NaN → True, a finite value → False, AND the result
// renders as a BOOL-dtype mask (`dtype=bool`), NOT float64.
// Fixture: `[0.0, 1.0] / [0.0, 1.0]` = `[0.0/0.0, 1.0/1.0]` = `[NaN, 1.0]`.
// Oracle (numpy 2.0.2): `np.isnan([nan, 1.0])` → `array([ True, False])`.
// =====================================================================

/// `coil.isnan([NaN, 1.0])` → `[True, False]` (bool dtype). The NaN is
/// True; the finite `1.0` is False. Pins NaN-detection AND the bool-dtype
/// result repr.
#[test]
fn test_e2e_isnan_nan_and_finite() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let num: coil.Buffer = coil.array1d2(0.0, 1.0)\n",
        "    let den: coil.Buffer = coil.array1d2(0.0, 1.0)\n",
        "    let mixed: coil.Buffer = num / den\n", // [0/0, 1/1] = [NaN, 1.0]
        "    let mask: coil.Buffer = coil.isnan(mixed)\n",
        "    let _ = coil.print_buffer(mask)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([True, False], dtype=bool)"),
        "expected isnan([NaN,1.0])=[True, False] as a BOOL mask; got \
         stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `isinf` over a [+inf, finite] buffer → [True, False]. THE
// load-bearing facts: +inf → True, a finite value → False, bool dtype.
// Fixture: `[1.0, 0.0] / [0.0, 1.0]` = `[1.0/0.0, 0.0/1.0]` = `[+inf, 0.0]`.
// Oracle: `np.isinf([inf, 0.0])` → `array([ True, False])`.
// =====================================================================

/// `coil.isinf([+inf, 0.0])` → `[True, False]` (bool dtype). The +inf is
/// True; the finite `0.0` is False.
#[test]
fn test_e2e_isinf_inf_and_finite() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let num: coil.Buffer = coil.array1d2(1.0, 0.0)\n",
        "    let den: coil.Buffer = coil.array1d2(0.0, 1.0)\n",
        "    let mixed: coil.Buffer = num / den\n", // [1/0, 0/1] = [+inf, 0.0]
        "    let mask: coil.Buffer = coil.isinf(mixed)\n",
        "    let _ = coil.print_buffer(mask)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([True, False], dtype=bool)"),
        "expected isinf([+inf,0.0])=[True, False] as a BOOL mask; got \
         stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `isfinite` over a [NaN, finite] buffer → [False, True]. THE
// load-bearing fact: isfinite is the COMPLEMENT of isnan/isinf — NaN →
// False, a finite value → True (the inverse of the isnan case above).
// Fixture: `[0.0, 1.0] / [0.0, 1.0]` = `[NaN, 1.0]`.
// Oracle: `np.isfinite([nan, 1.0])` → `array([False,  True])`.
// =====================================================================

/// `coil.isfinite([NaN, 1.0])` → `[False, True]` (bool dtype). NaN is NOT
/// finite (False); the finite `1.0` is True. The exact inverse of
/// `test_e2e_isnan_nan_and_finite`'s mask.
#[test]
fn test_e2e_isfinite_nan_and_finite() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let num: coil.Buffer = coil.array1d2(0.0, 1.0)\n",
        "    let den: coil.Buffer = coil.array1d2(0.0, 1.0)\n",
        "    let mixed: coil.Buffer = num / den\n", // [NaN, 1.0]
        "    let mask: coil.Buffer = coil.isfinite(mixed)\n",
        "    let _ = coil.print_buffer(mask)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([False, True], dtype=bool)"),
        "expected isfinite([NaN,1.0])=[False, True] as a BOOL mask; got \
         stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `isfinite` over a FULLY-FINITE buffer → [True, True]. An
// all-finite shape pins that isfinite returns ALL True when nothing is
// NaN / inf (the complement of the mixed case). `coil.ones(1)` /
// `coil.ones(1)` = `[1.0]`; use a 2-element finite vector via array1d2.
// Oracle: `np.isfinite([3.0, -4.0])` → `array([ True,  True])`.
// =====================================================================

/// `coil.isfinite([3.0, -4.0])` → `[True, True]`. Pins isfinite=all-True
/// when every element is finite (no false positive from the predicate).
#[test]
fn test_e2e_isfinite_all_finite() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(3.0, -4.0)\n",
        "    let mask: coil.Buffer = coil.isfinite(a)\n",
        "    let _ = coil.print_buffer(mask)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([True, True], dtype=bool)"),
        "expected isfinite([3.0,-4.0])=[True, True]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — CHAIN `coil.any(coil.isnan(a))` → "does the buffer have ANY
// NaN?". Proves (1) the bool MASK from `isnan` feeds the `any` reduction
// (a bool-dtype Buffer is a valid `any` input), (2) the fresh mask handle
// is consumed + drop-scheduled (no leak / double-free), (3) the boolean
// answer routes through the `-> bool` `any` wiring. `[NaN, 1.0]` HAS a
// NaN → any(isnan) = True → prints `1`. A finite-only buffer HAS NO NaN →
// any(isnan) = False → prints `0`.
// Oracle: `np.any(np.isnan([nan,1.0]))==True`,
//         `np.any(np.isnan([2.0,3.0]))==False`.
// =====================================================================

/// `coil.any(coil.isnan([NaN, 1.0]))` is True (the buffer HAS a NaN) →
/// `1`; `coil.any(coil.isnan([2.0, 3.0]))` is False (no NaN) → `0`. The
/// canonical "is this buffer NaN-clean?" idiom.
#[test]
fn test_e2e_chain_any_isnan_has_nan() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        // buffer WITH a NaN: [0/0, 1/1] = [NaN, 1.0]
        "    let n: coil.Buffer = coil.array1d2(0.0, 1.0)\n",
        "    let d: coil.Buffer = coil.array1d2(0.0, 1.0)\n",
        "    let has_nan_buf: coil.Buffer = n / d\n",
        "    let mask: coil.Buffer = coil.isnan(has_nan_buf)\n",
        "    let any_nan: bool = coil.any(&mask)\n",
        "    if any_nan:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        // buffer with NO NaN: [2.0, 3.0]
        "    let clean: coil.Buffer = coil.array1d2(2.0, 3.0)\n",
        "    let mask2: coil.Buffer = coil.isnan(clean)\n",
        "    let any_nan2: bool = coil.any(&mask2)\n",
        "    if any_nan2:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "1\n0".trim_end(),
        "expected any(isnan([NaN,1.0]))=True (->1), \
         any(isnan([2.0,3.0]))=False (->0); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}
