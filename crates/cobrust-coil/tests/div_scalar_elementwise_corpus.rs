//! coil elementwise-arithmetic COMPLETION — failing TEST-FIRST corpus for
//! the DIVISION (`a / b`) + scalar-broadcast (`a ⊕ k`) gap, at the Rust
//! `Array` / C-ABI level (exact-shape + exact-value oracles the `.cb`
//! constructors cannot express).
//!
//! ## Companion to the `.cb` E2E
//!
//! The sibling `.cb` E2E (`crates/cobrust-cli/tests/coil_div_scalar_e2e.rs`)
//! pins the `.cb`-buildable surface (f64 true-division + int-scalar broadcast).
//! THIS Rust corpus pins what `.cb` cannot reach:
//!   1. the **int/int → FLOAT true-division** divergence — the HEART of the gap
//!      (all `.cb` ctors are f64-only, so a `.cb` `a / b` always routes through
//!      the already-numpy-correct Float64 arm and CANNOT expose the int
//!      divergence);
//!   2. **div-by-zero → IEEE inf/nan** exact-value oracles (not just "doesn't
//!      trap");
//!   3. exact-shape/value oracles for the existing `+`/`*` no-regression
//!      anchors via the C-ABI shims (mirrors `broadcast_elementwise_corpus.rs`).
//!
//! ## What `ufunc::div` / `Array::div` ACTUALLY does today (the divergence)
//!
//! `ufunc::div` (`crates/cobrust-coil/src/ufunc.rs:399-436`; public-API
//! `Array::div`, `array.rs:182`) dispatches in the PROMOTED dtype via
//! `binary_dispatch`:
//!   - **float/float** (Float64/Float32): `x / y` — IEEE 754. `1.0/0.0 → +inf`,
//!     `-1.0/0.0 → -inf`, `0.0/0.0 → NaN`. **Matches numpy.**
//!   - **int/int** (Int32/Int64): `x.wrapping_div(y)` — INTEGER floor-toward-
//!     zero division, result stays the INTEGER promoted dtype; `y==0 →
//!     Err(NumpyErrorKind::IntegerDivisionByZero)`. **DIVERGES from numpy.**
//!
//! NumPy's `/` is `true_divide`: `np.array([1,2,3]) / np.array([2])` →
//! FLOAT `array([0.5, 1. , 1.5])` (NOT integer `array([0, 1, 1])`), and
//! `np.array([1]) / np.array([0])` → `array([inf])` (a RuntimeWarning, NOT an
//! exception). So `Array::div` on two int arrays is NOT a faithful `/`:
//! it yields integer floor-division and raises on int/0.
//!
//! ## What the completion must do (the contract these tests pin)
//!
//! The new `__cobrust_coil_buffer_div` C-ABI shim must implement numpy `/`
//! (true_divide): promote BOTH operands to FLOAT before dividing, so
//! int/int → FLOAT and int/0 → IEEE inf (NOT the kernel's integer
//! `wrapping_div` / `IntegerDivisionByZero`). The kernel already has the
//! IEEE-correct Float64 arm; the shim must route int operands into it (e.g.
//! cast to f64 first, or call a `true_divide`-flavored kernel). The
//! divergence-pinning tests below assert the FLOAT, numpy-faithful result —
//! so they are RED against today's integer-div `Array::div` and turn green
//! once the completion routes division through the float arm.
//!
//! ## RED at HEAD `fbfe98b`
//!
//! - `int_div_int_yields_float_true_division` — RED: `Array::div` on two
//!   Int64 arrays returns an Int64 array (integer floor-div), so the
//!   `assert dtype == Float64` + `assert values == [0.5,1.0,1.5]` fails.
//! - `int_div_by_zero_yields_inf_not_error` — RED: `Array::div` on int/0
//!   returns `Err(IntegerDivisionByZero)`, so the `assert Ok + inf` fails.
//! - The f64 `+`/`*` shim anchors (`*_still_works`) PASS at HEAD (baselines).
//! - `float_div_already_ieee_*` (kernel cross-check) PASSES at HEAD — the
//!   evidence the Float64 arm is already correct, so the completion is a
//!   shim-level promotion, not new numerical code.
//!
//! NONE are `#[ignore]`d — they are the contract the DEV turns green (corpus +
//! impl land atomically). The C-ABI add/mul shims abort the test binary on a
//! `coil_panic` (`__cobrust_panic` is `-> !`); the anchors here use only
//! broadcast-compatible / equal shapes so they never reach that path.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::undocumented_unsafe_blocks)]
#![allow(clippy::missing_panics_doc)]
// Opaque-handle ABI round-trip casts (`*mut u8 <-> *mut Array`). The pointers
// all originate from `Box::into_raw` of the SAME target type (the `into_handle`
// helper), so the alignment-narrowing lint is a false positive — mirrors the
// production cabi allow (`cabi.rs` §53-66) + the broadcast corpus sibling.
#![allow(clippy::cast_ptr_alignment)]
#![allow(clippy::cast_sign_loss)]

use coil::Array;
use coil::Dtype;
use coil::cabi::{
    __cobrust_coil_buffer_add, __cobrust_coil_buffer_drop, __cobrust_coil_buffer_mul,
};
use coil::{array_f64, array_i64};

// =====================================================================
// Stdlib ABI stub. The cabi shims reference `__cobrust_panic`
// (link-resolved from libcobrust_stdlib.a only at `.cb`-link time, NOT into
// this integration-test binary). Provide a `panic!`ing stub so the binary
// links; the anchors below use broadcast-compatible shapes so they never
// reach it. (Mirrors `broadcast_elementwise_corpus.rs`.)
// =====================================================================

#[unsafe(no_mangle)]
extern "C" fn __cobrust_panic(ptr: *const u8, len: usize) -> ! {
    // SAFETY: the coil_panic helper passes a valid UTF-8 `&str`'s (ptr,len).
    let msg = unsafe { std::slice::from_raw_parts(ptr, len) };
    panic!(
        "__cobrust_panic (test stub): {}",
        String::from_utf8_lossy(msg)
    );
}

// =====================================================================
// Helpers (mirror `broadcast_elementwise_corpus.rs`).
// =====================================================================

/// Box an `Array` as an opaque `Buffer` handle.
fn into_handle(arr: Array) -> *mut u8 {
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// Borrow a `Buffer` handle back as `&Array`.
///
/// # Safety
/// `h` must be a live handle from `into_handle` / a cabi constructor.
unsafe fn borrow_handle<'a>(h: *mut u8) -> &'a Array {
    unsafe { &*h.cast::<Array>() }
}

/// Materialise an f64 `Buffer` result's elements row-major.
///
/// # Safety
/// `h` must be a live `Buffer` handle holding an f64 array.
unsafe fn read_values_f64(h: *mut u8) -> Vec<f64> {
    let arr = unsafe { borrow_handle(h) };
    match arr {
        Array::Float64(a) => a.iter().copied().collect(),
        other => panic!("expected Float64 result, got {:?}", other.dtype()),
    }
}

/// Extract an `Array`'s elements as an f64 row-major `Vec` regardless of
/// dtype (int dtypes are promoted) — lets a divergence assertion compare a
/// (wrong) integer result against the expected float values, so the failure
/// message shows the integer truncation explicitly.
fn array_values_as_f64(arr: &Array) -> Vec<f64> {
    match arr {
        Array::Float64(a) => a.iter().copied().collect(),
        Array::Float32(a) => a.iter().map(|&v| f64::from(v)).collect(),
        Array::Int64(a) => a.iter().map(|&v| v as f64).collect(),
        Array::Int32(a) => a.iter().map(|&v| f64::from(v)).collect(),
        Array::Bool(a) => a.iter().map(|&v| if v { 1.0 } else { 0.0 }).collect(),
    }
}

// =====================================================================
// DIVERGENCE — int/int → FLOAT true-division (the HEART of the gap).
// RED at HEAD: Array::div does INTEGER floor-division on int/int.
// =====================================================================

/// `np.array([1,2,3]) / np.array([2])` → FLOAT `[0.5, 1.0, 1.5]` (true
/// division, NumPy `true_divide`). NOT integer floor-division `[0,1,1]`.
///
/// Oracle (numpy 2.0.2):
/// ```python
/// np.array([1,2,3]) / np.array([2])   # array([0.5, 1. , 1.5]); dtype float64
/// ```
///
/// PROOF OBLIGATION (divergence pin): the integer-dtype-preserving
/// `Array::div` (numpy `//` floor-division, with int/0 → `Err`) computes
/// `x.wrapping_div(y)` → `[0, 1, 1]` as an **Int64** array — the
/// divergence from numpy `/`. The completion adds the numpy-`/` operator
/// surface `Array::true_div` (wired into `__cobrust_coil_buffer_div`),
/// which promotes operands to FLOAT (numpy `/` is true_divide), so the
/// result is the Float64 `[0.5, 1.0, 1.5]`.
///
/// (Anchored on `Array::true_div` — the numpy-`/` true-division method the
/// `__cobrust_coil_buffer_div` shim forwards to — rather than the
/// integer-floor `Array::div` (kept as numpy `//`) or the C-ABI shim
/// itself, so the corpus COMPILES + asserts the FLOAT contract directly at
/// the kernel level. `Array::div` stays the established int-floor surface
/// pinned by `ufunc_well_typed::t14` / `ufunc_ill_typed`; `true_div` is the
/// new numpy-`/` surface. The shim satisfies the SAME contract: int/int →
/// numpy-faithful FLOAT true-division.)
#[test]
fn int_div_int_yields_float_true_division() {
    let a = array_i64(&[1, 2, 3], &[3]).unwrap();
    let b = array_i64(&[2], &[1]).unwrap(); // (1,) broadcasts against (3,)
    let c = a
        .true_div(&b)
        .expect("int/int true-div must not error for nonzero divisor");

    assert_eq!(
        c.dtype(),
        Dtype::Float64,
        "numpy `/` is true_divide: int/int must yield a FLOAT array, not an \
         integer one (got dtype {:?}; the kernel's integer floor-div is the \
         divergence the completion must fix)",
        c.dtype(),
    );
    assert_eq!(c.shape(), vec![3], "(3,)/(1,) must broadcast to shape (3,)",);
    assert_eq!(
        array_values_as_f64(&c),
        vec![0.5, 1.0, 1.5],
        "true division [1,2,3]/[2] must be [0.5,1.0,1.5] (NOT integer floor \
         [0,1,1]); got {:?}",
        array_values_as_f64(&c),
    );
}

/// A second int/int case with a divisor that does NOT divide evenly, so the
/// floor-vs-true divergence is unmissable: `[7,3] / [2,2]` → true `[3.5,1.5]`
/// vs floor `[3,1]`.
///
/// Oracle (numpy 2.0.2): `np.array([7,3]) / np.array([2,2])` →
/// `array([3.5, 1.5])`.
///
/// RED at HEAD: the int-floor `Array::div` → Int64 `[3, 1]`; the new
/// `Array::true_div` (numpy `/`) → Float64 `[3.5, 1.5]`.
#[test]
fn int_div_int_nonexact_is_true_division() {
    let a = array_i64(&[7, 3], &[2]).unwrap();
    let b = array_i64(&[2, 2], &[2]).unwrap();
    let c = a.true_div(&b).expect("int/int true-div nonzero divisor");

    assert_eq!(
        c.dtype(),
        Dtype::Float64,
        "int/int true-division must yield FLOAT; got {:?}",
        c.dtype(),
    );
    assert_eq!(
        array_values_as_f64(&c),
        vec![3.5, 1.5],
        "true division [7,3]/[2,2] must be [3.5,1.5] (NOT floor [3,1]); got {:?}",
        array_values_as_f64(&c),
    );
}

/// `np.array([1]) / np.array([0])` (int operands) → `array([inf])` — IEEE,
/// a RuntimeWarning, NOT a Python exception. numpy's `/` promotes to float
/// FIRST, so even int/0 is `inf`, never a raise.
///
/// Oracle (numpy 2.0.2):
/// ```python
/// np.array([1]) / np.array([0])    # RuntimeWarning; array([inf])
/// np.array([0]) / np.array([0])    # RuntimeWarning; array([nan])
/// ```
///
/// PROOF OBLIGATION (divergence pin): the int-floor `Array::div` on int/0
/// returns `Err(NumpyErrorKind::IntegerDivisionByZero)` — the divergence.
/// The completion's `Array::true_div` (the `_div` shim's kernel), by
/// promoting to float, must yield `inf` (and `0/0 → NaN`) and NOT
/// error/abort.
#[test]
fn int_div_by_zero_yields_inf_not_error() {
    let a = array_i64(&[1, 0], &[2]).unwrap();
    let b = array_i64(&[0, 0], &[2]).unwrap();
    let c = a
        .true_div(&b)
        .expect("numpy `/` promotes to float: int/0 must be IEEE inf/nan, NOT an error");

    assert_eq!(
        c.dtype(),
        Dtype::Float64,
        "int/0 under numpy `/` must yield a FLOAT array (true_divide promotes); got {:?}",
        c.dtype(),
    );
    let v = array_values_as_f64(&c);
    assert!(
        v[0].is_infinite() && v[0] > 0.0,
        "1/0 must be +inf (IEEE; numpy RuntimeWarning, not a raise); got {}",
        v[0],
    );
    assert!(v[1].is_nan(), "0/0 must be NaN (IEEE); got {}", v[1]);
}

// =====================================================================
// f64 true-division — VALUE oracles (the dtype `.cb` builds; documents the
// numpy-correct path the shim already routes for floats). The same-shape
// f64 case PASSES via the kernel today; once `__cobrust_coil_buffer_div`
// exists these become the shim-level anchors (the DEV may retarget them).
// =====================================================================

/// f64 same-shape true-division exact values: `[10,20,30]/[2,4,5] = [5,5,6]`.
///
/// Oracle (numpy 2.0.2): `np.array([10.,20.,30.]) / np.array([2.,4.,5.])` →
/// `array([5., 5., 6.])`. (PASSES at HEAD via the kernel — the float arm is
/// already correct; pins the value contract the shim must preserve.)
#[test]
fn float_div_same_shape_exact_values() {
    let a = array_f64(&[10.0, 20.0, 30.0], &[3]).unwrap();
    let b = array_f64(&[2.0, 4.0, 5.0], &[3]).unwrap();
    let c = a.div(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Float64);
    assert_eq!(c.shape(), vec![3]);
    assert_eq!(read_values_via_owned(&c), vec![5.0, 5.0, 6.0]);
}

/// f64 broadcast true-division `(3,)/(1,)`: `[1,2,3]/[2] = [0.5,1.0,1.5]`.
/// Same VALUES as the int case above, but f64 operands → already FLOAT, so
/// this PASSES at HEAD via the kernel. The contrast with
/// `int_div_int_yields_float_true_division` (identical values, int operands,
/// RED) is the whole divergence in two tests.
///
/// Oracle (numpy 2.0.2): `np.array([1.,2.,3.]) / np.array([2.])` →
/// `array([0.5, 1. , 1.5])`.
#[test]
fn float_div_broadcast_3_by_1_is_fractional() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[2.0], &[1]).unwrap();
    let c = a.div(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Float64);
    assert_eq!(c.shape(), vec![3]);
    assert_eq!(read_values_via_owned(&c), vec![0.5, 1.0, 1.5]);
}

/// f64 div-by-zero is IEEE (the kernel's float arm): `[1,-1,0]/[0,0,0]` →
/// `[+inf, -inf, NaN]`. PASSES at HEAD (this is the behavior the completion
/// preserves for floats and EXTENDS to ints via promotion).
///
/// Oracle (numpy 2.0.2): `np.array([1.,-1.,0.]) / np.array([0.,0.,0.])` →
/// `array([inf, -inf, nan])` (RuntimeWarning, no exception).
#[test]
fn float_div_by_zero_already_ieee() {
    let a = array_f64(&[1.0, -1.0, 0.0], &[3]).unwrap();
    let b = array_f64(&[0.0, 0.0, 0.0], &[3]).unwrap();
    let c = a
        .div(&b)
        .expect("float div-by-zero is IEEE, never an error");
    let v = read_values_via_owned(&c);
    assert!(
        v[0].is_infinite() && v[0] > 0.0,
        "1.0/0.0 = +inf; got {}",
        v[0]
    );
    assert!(
        v[1].is_infinite() && v[1] < 0.0,
        "-1.0/0.0 = -inf; got {}",
        v[1]
    );
    assert!(v[2].is_nan(), "0.0/0.0 = NaN; got {}", v[2]);
}

/// Read an owned `Array`'s f64 values (panics if not Float64 — used by the
/// float-arm value oracles that MUST be Float64).
fn read_values_via_owned(arr: &Array) -> Vec<f64> {
    match arr {
        Array::Float64(a) => a.iter().copied().collect(),
        other => panic!("expected Float64, got {:?}", other.dtype()),
    }
}

// =====================================================================
// SCALAR-BROADCAST contract — `a ⊕ k` value oracles (the kernel route the
// scalar shim must produce). At the Array level, `a + 1` is modeled as
// `a + array([1.0])` (a length-1 broadcast — numpy's `array + scalar` is
// exactly a (1,)-broadcast). The completion's `__cobrust_coil_buffer_<op>_
// scalar(a, k)` shim must produce these results. Anchored on `Array::{add,
// sub,mul,div}` with a length-1 RHS (compiles at HEAD; the float arm is
// numpy-correct, so these PASS at the kernel level — they pin the VALUES the
// scalar shim must reproduce once it exists).
// =====================================================================

/// `[1,2,3] + 1` → `[2,3,4]` (scalar add). Modeled as `a + [1.0]`.
///
/// Oracle (numpy 2.0.2): `np.array([1.,2.,3.]) + 1` → `array([2.,3.,4.])`.
#[test]
fn scalar_add_one_values() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let k = array_f64(&[1.0], &[1]).unwrap(); // scalar-as-(1,)
    let c = a.add(&k).unwrap();
    assert_eq!(read_values_via_owned(&c), vec![2.0, 3.0, 4.0]);
}

/// `[1,2,3] * 2` → `[2,4,6]` (scalar mul). Modeled as `a * [2.0]`.
///
/// Oracle (numpy 2.0.2): `np.array([1.,2.,3.]) * 2` → `array([2.,4.,6.])`.
#[test]
fn scalar_mul_two_values() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let k = array_f64(&[2.0], &[1]).unwrap();
    let c = a.mul(&k).unwrap();
    assert_eq!(read_values_via_owned(&c), vec![2.0, 4.0, 6.0]);
}

/// `[1,2,3] - 1` → `[0,1,2]` (scalar sub). Modeled as `a - [1.0]`.
///
/// Oracle (numpy 2.0.2): `np.array([1.,2.,3.]) - 1` → `array([0.,1.,2.])`.
#[test]
fn scalar_sub_one_values() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let k = array_f64(&[1.0], &[1]).unwrap();
    let c = a.sub(&k).unwrap();
    assert_eq!(read_values_via_owned(&c), vec![0.0, 1.0, 2.0]);
}

/// `[2,4,6] / 2` → `[1,2,3]` (scalar true-div). Modeled as `a / [2.0]`.
///
/// Oracle (numpy 2.0.2): `np.array([2.,4.,6.]) / 2` → `array([1.,2.,3.])`.
#[test]
fn scalar_div_two_values() {
    let a = array_f64(&[2.0, 4.0, 6.0], &[3]).unwrap();
    let k = array_f64(&[2.0], &[1]).unwrap();
    let c = a.div(&k).unwrap();
    assert_eq!(read_values_via_owned(&c), vec![1.0, 2.0, 3.0]);
}

// =====================================================================
// NO-REGRESSION — the existing +,-,* C-ABI shims, exact-shape/value via
// the opaque-handle round-trip. PASS at HEAD (the baselines). Must stay
// green after the Div + scalar paths land.
// =====================================================================

/// `[1,2,3] + [10,20,30]` via the REAL `__cobrust_coil_buffer_add` shim →
/// `[11,22,33]`. Exercises the exact handle round-trip the `.cb` chain uses.
/// PASSES at HEAD.
///
/// Oracle (numpy 2.0.2): `np.array([1.,2.,3.]) + np.array([10.,20.,30.])` →
/// `array([11.,22.,33.])`.
#[test]
fn shim_add_same_shape_still_works() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[10.0, 20.0, 30.0], &[3]).unwrap();
    let ha = into_handle(a);
    let hb = into_handle(b);
    // SAFETY: live handles; equal-shape add — no guard fires.
    let hc = unsafe { __cobrust_coil_buffer_add(ha, hb) };
    let values = unsafe { read_values_f64(hc) };
    assert_eq!(
        values,
        vec![11.0, 22.0, 33.0],
        "[1,2,3]+[10,20,30] via the add shim must equal numpy's elementwise sum",
    );
    unsafe {
        __cobrust_coil_buffer_drop(ha);
        __cobrust_coil_buffer_drop(hb);
        __cobrust_coil_buffer_drop(hc);
    }
}

/// `[1,2,3] * [10,20,30]` via the REAL `__cobrust_coil_buffer_mul` shim →
/// `[10,40,90]`. PASSES at HEAD. Pins the mul shim through the round-trip.
///
/// Oracle (numpy 2.0.2): `np.array([1.,2.,3.]) * np.array([10.,20.,30.])` →
/// `array([10.,40.,90.])`.
#[test]
fn shim_mul_same_shape_still_works() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[10.0, 20.0, 30.0], &[3]).unwrap();
    let ha = into_handle(a);
    let hb = into_handle(b);
    // SAFETY: live handles; equal-shape mul — no guard fires.
    let hc = unsafe { __cobrust_coil_buffer_mul(ha, hb) };
    let values = unsafe { read_values_f64(hc) };
    assert_eq!(
        values,
        vec![10.0, 40.0, 90.0],
        "[1,2,3]*[10,20,30] via the mul shim must equal numpy's elementwise product",
    );
    unsafe {
        __cobrust_coil_buffer_drop(ha);
        __cobrust_coil_buffer_drop(hb);
        __cobrust_coil_buffer_drop(hc);
    }
}

/// `[1,2,3] * [10]` BROADCAST via the REAL mul shim → `[10,20,30]`. PASSES at
/// HEAD (Phase-3 broadcasting). Pins that the no-regression broadcast path
/// stays green when the shared shim body also hosts `/`.
///
/// Oracle (numpy 2.0.2): `np.array([1.,2.,3.]) * np.array([10.])` →
/// `array([10.,20.,30.])`.
#[test]
fn shim_mul_broadcast_3_by_1_still_works() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[10.0], &[1]).unwrap();
    let ha = into_handle(a);
    let hb = into_handle(b);
    // SAFETY: live handles; (3,)*(1,) broadcasts — Phase-3 guard lets it through.
    let hc = unsafe { __cobrust_coil_buffer_mul(ha, hb) };
    let values = unsafe { read_values_f64(hc) };
    assert_eq!(
        values,
        vec![10.0, 20.0, 30.0],
        "[1,2,3]*[10] must broadcast to numpy's [10,20,30]",
    );
    unsafe {
        __cobrust_coil_buffer_drop(ha);
        __cobrust_coil_buffer_drop(hb);
        __cobrust_coil_buffer_drop(hc);
    }
}
