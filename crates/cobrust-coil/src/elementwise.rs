// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy 2.4.6)
// scope: #145 numpy gap-closure BATCH 3 — the unary TRANSCENDENTAL
//   elementwise ufunc family (`exp` / `log` (natural ln) / `log10` /
//   `sqrt` / `sin` / `cos` / `tan`, plus the trivial same-dtype-rule
//   `exp2` / `log2` / `cbrt` / `sinh` / `cosh` / `tanh`), the
//   1-arg Buffer -> Buffer FLOAT-returning surface (mirrors the
//   `transpose` / `flatten` / `ravel` 1-arg reshape wiring of BATCH 2,
//   commit 3900d5f — the SAME borrow-Buffer-arg → fresh-Buffer-return
//   value-handle ABI, NOT the scalar-return stats).
//   + #145 BATCH 4 — the unary ROUNDING / SIGN elementwise ufunc family
//   (`abs` / `floor` / `ceil` / `round` / `trunc` / `square` / `sign`),
//   the SAME 1-arg Buffer -> Buffer shape but DTYPE-PRESERVING (int->int,
//   f32->f32, f64->f64; floor/ceil/round/trunc are int no-ops). `round`
//   is round-half-to-EVEN (banker's); `sign(0)=0` + `sign(NaN)=NaN`.
// see PROVENANCE.toml for the full manifest.

//! Unary transcendental elementwise free functions — the FLOAT-returning
//! `Array -> Array` math surface most-used in real numpy code
//! (`np.exp` / `np.log` / `np.log10` / `np.sqrt` / `np.sin` / `np.cos` /
//! `np.tan`), each returning a fresh owned `Array`.
//!
//! BATCH-4 (#145) extends this module with the DTYPE-PRESERVING rounding/sign
//! family (`abs`/`floor`/`ceil`/`round`/`trunc`/`square`/`sign`) — see the
//! BATCH-4 section banner below. Unlike the transcendental family described
//! here those do NOT float-promote (`int -> int`, `f32 -> f32`); `round` uses
//! banker's (round-half-to-even) rounding and `sign(0)=0` / `sign(NaN)=NaN`.
//!
//! ## Why these (the bounded #145 BATCH-3 choice)
//!
//! Per the LLM-training-data-overlap rule (§2.5) these are the unary
//! math ufuncs an LLM reaches for first. The cut line is the ARITY +
//! RETURN CONTRACT: only the 1-arg, FLOAT-returning forms ship here —
//! they wire through the EXISTING borrow-Buffer-arg → fresh-Buffer-return
//! ecosystem path (the SAME path `coil.transpose(a)` / `coil.flatten(a)`
//! prove), so codegen needs ZERO new arms (the flat `__cobrust_coil_*`
//! recognizer + `coil_shape_ty` `(ptr) -> ptr` extern shape already
//! covers them). Reductions that return a scalar (`np.sum` of `exp`),
//! the 2-arg `np.logaddexp`, and the inverse-trig family
//! (`arcsin`/`arctan2`) are DEFERRED follow-ups.
//!
//! ## numpy-exact DTYPE PROMOTION (the load-bearing contract)
//!
//! These are all FLOAT-RETURNING. Per numpy + [`unary_math_dtype`]:
//!
//! - **integer input** (any int dtype) PROMOTES to `Float64`:
//!   `exp(int_array) -> Float64 Buffer` (`np.exp(np.int64([0,1,2])).dtype
//!   == float64`).
//! - **`Float32` input** STAYS `Float32`: `sqrt(f32) -> Float32 Buffer`.
//! - **`Float64` input** STAYS `Float64`.
//! - **`Bool` input** PROMOTES to `Float64`. (numpy promotes `bool` to
//!   `float16` for these ufuncs — `np.exp(np.bool_(True)).dtype ==
//!   float16` — but the coil `Array` tagged-union has NO `float16`
//!   variant, so coil pins `bool -> Float64`. The VALUES are identical
//!   — `True=1.0`/`False=0.0` so `exp(True)=e`, `sqrt(False)=0` — only
//!   the dtype TIER differs (`Float64` vs numpy's `Float16`). This is a
//!   `Semantic`-tier, value-faithful divergence consistent with the
//!   existing [`unary_math_dtype`] contract — see the coil PROVENANCE
//!   manifest.)
//!
//! ## NaN / inf EDGE CASES (VALUES, not errors)
//!
//! These kernels are total: a domain-error input yields an IEEE-754
//! special VALUE (numpy emits a RuntimeWarning but the array value is
//! the same), NEVER a trap / error. The Rust `f64::ln` / `f64::sqrt` /
//! etc. emit bit-identical IEEE-754 results:
//!
//! - `log(0) -> -inf`, `log(-1) -> NaN`;
//! - `log10(0) -> -inf`, `log10(-1) -> NaN`;
//! - `log2(0) -> -inf`;
//! - `sqrt(-1) -> NaN`;
//! - `exp(710) -> +inf` (overflow).
//!
//! Because there is no conformability concept for a unary op, the cabi
//! shim ALWAYS returns a fresh `Buffer` — no `coil_panic` path exists.

// File-level allows mirror the other auto-generated coil modules. The
// cast lints fire on the intrinsically-correct int/bool -> f64 numpy
// promotion (`unary_math_dtype`), and `mapv` closures read as `suboptimal
// _flops` / `imprecise_flops` to clippy though they are the exact
// IEEE-754 libm calls numpy itself dispatches.
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::imprecise_flops,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    // The BATCH-4 `unary_value` / `unary_round_family` helpers carry one
    // closure parameter per dtype (`op_i32` / `op_i64` / `op_f32` /
    // `op_f64`) — the dtype suffix IS the clearest possible name (it names
    // exactly which `Array` arm the kernel serves), so the `similar_names`
    // lint on the `op_*` family is a false positive here (same rationale as
    // `promote.rs`'s test-module allow).
    clippy::similar_names,
    clippy::suboptimal_flops
)]

use ndarray::ArrayD;

use crate::array::Array;
use crate::dtype::Dtype;
use crate::promote::unary_math_dtype;

/// Apply a unary float kernel elementwise with numpy dtype promotion.
///
/// The promoted dtype is [`unary_math_dtype`] of the input dtype: int /
/// bool inputs promote to `Float64`, `Float32` stays `Float32`, `Float64`
/// stays `Float64`. The matching monomorphic kernel (`op_f32` for the
/// `Float32`-promoted path, `op_f64` otherwise) is `mapv`'d over a fresh
/// owned `ArrayD<T>`, so the result is a contiguous C-order copy the
/// handle owns. Total — never errors (a domain-error element yields an
/// IEEE-754 special value, never a trap).
fn unary_float(arr: &Array, op_f32: impl Fn(f32) -> f32, op_f64: impl Fn(f64) -> f64) -> Array {
    match unary_math_dtype(arr.dtype()) {
        // `Float32` is the ONLY non-`Float64` promotion target: a
        // `Float32` input stays single-precision (numpy `sqrt(f32) -> f32`).
        Dtype::Float32 => Array::Float32(as_f32(arr).mapv(op_f32)),
        // Every other input (`Float64`, and the int / bool inputs that
        // `unary_math_dtype` promotes to `Float64`) runs the `f64` kernel.
        _ => Array::Float64(as_f64(arr).mapv(op_f64)),
    }
}

/// Cast any `Array` variant to an owned `ArrayD<f64>` (the numpy
/// `astype(float64)` promotion for the int / bool / f64 unary-math
/// inputs). `Float32 -> f64` is included for completeness but is never
/// reached on the `Float32` path (that takes [`as_f32`]).
fn as_f64(arr: &Array) -> ArrayD<f64> {
    match arr {
        Array::Int32(a) => a.mapv(f64::from),
        Array::Int64(a) => a.mapv(|v| v as f64),
        Array::Float32(a) => a.mapv(f64::from),
        Array::Float64(a) => a.clone(),
        Array::Bool(a) => a.mapv(|v| f64::from(u8::from(v))),
    }
}

/// Cast a `Float32` `Array` to an owned `ArrayD<f32>`. Only the
/// `Float32`-input path of [`unary_float`] calls this; the other variants
/// are unreachable there (they promote to `Float64`) but are handled
/// total-ly (cast to `f32`) to keep the helper standalone.
fn as_f32(arr: &Array) -> ArrayD<f32> {
    match arr {
        Array::Int32(a) => a.mapv(|v| v as f32),
        Array::Int64(a) => a.mapv(|v| v as f32),
        Array::Float32(a) => a.clone(),
        Array::Float64(a) => a.mapv(|v| v as f32),
        Array::Bool(a) => a.mapv(|v| f32::from(u8::from(v))),
    }
}

// ---- the 7 CORE transcendental ufuncs ------------------------------------

/// `np.exp(a)` — `e**x` elementwise. Int / bool -> `Float64`, `Float32`
/// stays `Float32`. `exp(710) -> +inf` (overflow, IEEE-754). Total.
#[must_use]
pub fn exp(a: &Array) -> Array {
    unary_float(a, f32::exp, f64::exp)
}

/// `np.log(a)` — NATURAL log (base e), elementwise. Int / bool ->
/// `Float64`, `Float32` stays `Float32`. `log(0) -> -inf`, `log(-1) ->
/// NaN` (IEEE-754 domain values, NOT errors). Total.
#[must_use]
pub fn log(a: &Array) -> Array {
    unary_float(a, f32::ln, f64::ln)
}

/// `np.log10(a)` — base-10 log, elementwise. Int / bool -> `Float64`,
/// `Float32` stays `Float32`. `log10(0) -> -inf`, `log10(-1) -> NaN`.
/// Total.
#[must_use]
pub fn log10(a: &Array) -> Array {
    unary_float(a, f32::log10, f64::log10)
}

/// `np.sqrt(a)` — square root, elementwise. Int / bool -> `Float64`,
/// `Float32` stays `Float32`. `sqrt(-1) -> NaN` (IEEE-754 domain value).
/// Total.
#[must_use]
pub fn sqrt(a: &Array) -> Array {
    unary_float(a, f32::sqrt, f64::sqrt)
}

/// `np.sin(a)` — sine (radians), elementwise. Int / bool -> `Float64`,
/// `Float32` stays `Float32`. Total.
#[must_use]
pub fn sin(a: &Array) -> Array {
    unary_float(a, f32::sin, f64::sin)
}

/// `np.cos(a)` — cosine (radians), elementwise. Int / bool -> `Float64`,
/// `Float32` stays `Float32`. Total.
#[must_use]
pub fn cos(a: &Array) -> Array {
    unary_float(a, f32::cos, f64::cos)
}

/// `np.tan(a)` — tangent (radians), elementwise. Int / bool -> `Float64`,
/// `Float32` stays `Float32`. Total.
#[must_use]
pub fn tan(a: &Array) -> Array {
    unary_float(a, f32::tan, f64::tan)
}

// ---- the 6 OPTIONAL same-dtype-rule transcendental ufuncs ----------------
// Each follows the IDENTICAL `unary_float` int->f64 / f32->f32 / f64->f64
// promotion as the 7 core ops (numpy-confirmed: `np.exp2(f32).dtype ==
// float32`, `np.cbrt(int64).dtype == float64`), so they are trivial,
// zero-risk additions.

/// `np.exp2(a)` — `2**x` elementwise. Same dtype rule as [`exp`]. Total.
#[must_use]
pub fn exp2(a: &Array) -> Array {
    unary_float(a, f32::exp2, f64::exp2)
}

/// `np.log2(a)` — base-2 log, elementwise. Same dtype rule as [`log`].
/// `log2(0) -> -inf`, `log2(-1) -> NaN`. Total.
#[must_use]
pub fn log2(a: &Array) -> Array {
    unary_float(a, f32::log2, f64::log2)
}

/// `np.cbrt(a)` — cube root, elementwise. Same dtype rule as [`sqrt`].
/// Unlike `sqrt`, `cbrt` is defined for negatives (`cbrt(-8) -> -2`).
/// Total.
#[must_use]
pub fn cbrt(a: &Array) -> Array {
    unary_float(a, f32::cbrt, f64::cbrt)
}

/// `np.sinh(a)` — hyperbolic sine, elementwise. Same dtype rule as
/// [`sin`]. Total.
#[must_use]
pub fn sinh(a: &Array) -> Array {
    unary_float(a, f32::sinh, f64::sinh)
}

/// `np.cosh(a)` — hyperbolic cosine, elementwise. Same dtype rule as
/// [`cos`]. Total.
#[must_use]
pub fn cosh(a: &Array) -> Array {
    unary_float(a, f32::cosh, f64::cosh)
}

/// `np.tanh(a)` — hyperbolic tangent, elementwise. Same dtype rule as
/// [`tan`]. Total.
#[must_use]
pub fn tanh(a: &Array) -> Array {
    unary_float(a, f32::tanh, f64::tanh)
}

// =========================================================================
// #145 numpy gap-closure BATCH 4 — the unary ROUNDING / SIGN elementwise
// ufunc family (`abs` / `floor` / `ceil` / `round` / `trunc` / `square` /
// `sign`). Same 1-arg `Array -> Array` shape as the transcendentals above,
// but a DIFFERENT dtype contract: these are **DTYPE-PRESERVING** (numpy 2.x
// confirmed), NOT float-promoting.
//
// ## numpy-exact DTYPE PRESERVATION (the load-bearing contract)
//
// Unlike the float-returning [`unary_float`] family, these KEEP the input
// dtype: `int64 -> int64`, `float32 -> float32`, `float64 -> float64`
// (`np.abs(np.int64([...])).dtype == int64`, `np.round(np.float32([...]))
// .dtype == float32`). They DO NOT promote int -> float.
//
// ## INTEGER no-op (the #1 dtype subtlety)
//
// `floor` / `ceil` / `round` / `trunc` are NO-OPS on integer input —
// numpy 2.x `np.floor(int_array)` returns the int array UNCHANGED (an
// integer is already "rounded"). For an int / bool `Array` these four ops
// return the input as-is (clone via [`unary_round_family`]'s int arm); they
// transform only the `Float32` / `Float64` variants. `abs` / `square` /
// `sign` DO apply per-element to integers (`abs(-3)=3`, `square(2)=4`,
// `sign(-5)=-1`), via [`unary_value`].
//
// ## TWO numpy-exact correctness nuances pinned in tests
//
// 1. **`round` = round-half-to-EVEN** (banker's rounding), via Rust
//    [`f64::round_ties_even`] / [`f32::round_ties_even`] — NOT `f64::round`
//    (which is half-AWAY-from-zero: `round(0.5)=1`, WRONG vs numpy). numpy
//    `np.round`: `0.5 -> 0`, `1.5 -> 2`, `2.5 -> 2`, `-0.5 -> -0`.
// 2. **`sign(0)=0` and `sign(NaN)=NaN`**, via an explicit branch — NOT
//    Rust [`f64::signum`] (which returns `+1.0` for `0.0` and propagates
//    the sign bit for `NaN`, WRONG vs numpy). numpy `np.sign`:
//    `0.0 -> 0.0`, `-0.0 -> 0.0`, `nan -> nan`, `x>0 -> 1`, `x<0 -> -1`.
//
// ## BOOL input (coil's documented Semantic-tier divergence)
//
// numpy DIVERGES per op on bool input — `np.round(bool) -> float16`,
// `np.square(bool) -> int8`, `np.sign(bool) -> ERROR`, while
// `np.abs(bool)` / `np.floor(bool)` stay `bool`. coil's `Array`
// tagged-union has NO `float16` / `int8` variant and the unary surface is
// TOTAL (no trap path), so coil pins a single uniform, value-faithful rule:
// **every op returns the `Bool` `Array` UNCHANGED on bool input** (bool is
// already 0/1, the fixed point of all seven ops — `round(True)=1=True`,
// `square(True)=1=True`, `sign(True)=1=True`, `abs(False)=0=False`). The
// VALUES match what each op would mean on the 0/1 numeric; only the dtype
// TIER differs from numpy's per-op promotion (`Bool` vs `float16` / `int8`)
// and `sign(bool)` does NOT raise (coil's unary kernels never trap). This
// is a `Semantic`-tier divergence consistent with the BATCH-3 `bool ->
// Float64` choice — see the coil PROVENANCE manifest.

/// Apply a `round`-family kernel (`floor` / `ceil` / `round` / `trunc`)
/// elementwise, **preserving dtype**. Integer / bool inputs are returned
/// UNCHANGED (numpy 2.x no-op: an integer is already rounded); only the
/// `Float32` / `Float64` variants run the kernel. Total — never errors.
fn unary_round_family(
    arr: &Array,
    op_f32: impl Fn(f32) -> f32,
    op_f64: impl Fn(f64) -> f64,
) -> Array {
    match arr {
        // numpy 2.x: floor/ceil/round/trunc on an int / bool array is a
        // no-op (dtype preserved, values unchanged) — return as-is.
        Array::Int32(_) | Array::Int64(_) | Array::Bool(_) => arr.clone(),
        Array::Float32(a) => Array::Float32(a.mapv(op_f32)),
        Array::Float64(a) => Array::Float64(a.mapv(op_f64)),
    }
}

/// Apply a per-element VALUE kernel (`abs` / `square` / `sign`)
/// elementwise, **preserving dtype**. Every variant transforms: the int
/// kernels (`op_i32` / `op_i64`) run on `Int32` / `Int64`, the float
/// kernels (`op_f32` / `op_f64`) on `Float32` / `Float64`, and a `Bool`
/// array is returned UNCHANGED (coil's value-faithful Semantic divergence:
/// bool is the 0/1 fixed point of `abs`/`square`/`sign`). Total.
fn unary_value(
    arr: &Array,
    op_i32: impl Fn(i32) -> i32,
    op_i64: impl Fn(i64) -> i64,
    op_f32: impl Fn(f32) -> f32,
    op_f64: impl Fn(f64) -> f64,
) -> Array {
    match arr {
        Array::Int32(a) => Array::Int32(a.mapv(op_i32)),
        Array::Int64(a) => Array::Int64(a.mapv(op_i64)),
        Array::Float32(a) => Array::Float32(a.mapv(op_f32)),
        Array::Float64(a) => Array::Float64(a.mapv(op_f64)),
        // Bool: 0/1 fixed point of abs/square/sign — return unchanged
        // (coil's documented Semantic-tier divergence; numpy would emit
        // bool / int8 / raise, but the VALUE is identical).
        Array::Bool(_) => arr.clone(),
    }
}

/// numpy-exact `sign` for an IEEE float: `x>0 -> 1`, `x<0 -> -1`,
/// `x==0 -> 0` (so `+0.0` and `-0.0` both map to `0.0`), `NaN -> NaN`.
/// NOT Rust [`f64::signum`] (which returns `+1.0` for `0.0` and propagates
/// the sign bit for `NaN`). The `is_nan` branch is first so `NaN`
/// short-circuits before the comparisons (which are all `false` for `NaN`).
fn sign_f64(x: f64) -> f64 {
    if x.is_nan() {
        f64::NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        // `x == 0.0` covers both `+0.0` and `-0.0` (they compare equal).
        0.0
    }
}

/// `f32` companion of [`sign_f64`] — identical numpy-exact semantics.
fn sign_f32(x: f32) -> f32 {
    if x.is_nan() {
        f32::NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// numpy-exact `sign` for a signed integer: `-1` / `0` / `1`.
/// `(x>0) - (x<0)` is the branch-free numpy formula; written explicitly
/// here for clarity. `i32` companion is [`sign_i32`].
fn sign_i64(x: i64) -> i64 {
    (x > 0) as i64 - (x < 0) as i64
}

/// `i32` companion of [`sign_i64`].
fn sign_i32(x: i32) -> i32 {
    (x > 0) as i32 - (x < 0) as i32
}

/// `np.abs(a)` — absolute value, elementwise. **Dtype-preserving**:
/// `abs(int)->int`, `abs(f32)->f32`, `abs(f64)->f64`. `abs(bool)` returns
/// the bool array unchanged (numpy: `bool`; values match). Total.
///
/// `i32::MIN` / `i64::MIN` wrap to themselves under `wrapping_abs` (the
/// numpy two's-complement behavior — `np.abs(np.int64(-2**63))` is the
/// negative `-2**63`, NOT a panic); the float kernels use IEEE `abs`
/// (`abs(-0.0)=0.0`, `abs(NaN)=NaN`).
#[must_use]
pub fn abs(a: &Array) -> Array {
    unary_value(a, i32::wrapping_abs, i64::wrapping_abs, f32::abs, f64::abs)
}

/// `np.floor(a)` — largest integer `<= x`, elementwise. **Dtype-
/// preserving** float-rounding; a NO-OP on integer / bool input (numpy
/// 2.x returns the int array unchanged). `floor(-1.5) -> -2`,
/// `floor(1.5) -> 1`. `floor(NaN)=NaN`, `floor(±inf)=±inf`. Total.
#[must_use]
pub fn floor(a: &Array) -> Array {
    unary_round_family(a, f32::floor, f64::floor)
}

/// `np.ceil(a)` — smallest integer `>= x`, elementwise. **Dtype-
/// preserving**; a NO-OP on integer / bool input. `ceil(-1.5) -> -1`,
/// `ceil(1.5) -> 2`. `ceil(NaN)=NaN`, `ceil(±inf)=±inf`. Total.
#[must_use]
pub fn ceil(a: &Array) -> Array {
    unary_round_family(a, f32::ceil, f64::ceil)
}

/// `np.round(a)` — round to the nearest integer with **round-half-to-EVEN**
/// (banker's rounding), elementwise. **Dtype-preserving**; a NO-OP on
/// integer / bool input. Uses Rust [`f64::round_ties_even`] /
/// [`f32::round_ties_even`] — NOT `round` (half-away-from-zero). numpy
/// `np.round`: `0.5 -> 0`, `1.5 -> 2`, `2.5 -> 2`, `3.5 -> 4`,
/// `-0.5 -> -0`. `round(NaN)=NaN`. Total.
#[must_use]
pub fn round(a: &Array) -> Array {
    unary_round_family(a, f32::round_ties_even, f64::round_ties_even)
}

/// `np.trunc(a)` — truncate toward zero (drop the fractional part),
/// elementwise. **Dtype-preserving**; a NO-OP on integer / bool input.
/// `trunc(-1.7) -> -1`, `trunc(1.7) -> 1` (toward zero, UNLIKE `floor`).
/// `trunc(NaN)=NaN`, `trunc(±inf)=±inf`. Total.
#[must_use]
pub fn trunc(a: &Array) -> Array {
    unary_round_family(a, f32::trunc, f64::trunc)
}

/// `np.square(a)` — `x * x` elementwise. **Dtype-preserving**:
/// `square(int)->int` (`square(2)=4`, integer wrapping on overflow per
/// numpy two's-complement), `square(f32)->f32`, `square(f64)->f64`.
/// `square(bool)` returns the bool array unchanged (numpy: `int8` `[1,0]`;
/// VALUE `True*True=True`, `False*False=False` matches). Total.
#[must_use]
pub fn square(a: &Array) -> Array {
    unary_value(
        a,
        |x: i32| x.wrapping_mul(x),
        |x: i64| x.wrapping_mul(x),
        |x| x * x,
        |x| x * x,
    )
}

/// `np.sign(a)` — `-1` / `0` / `1` indicating the sign, elementwise.
/// **Dtype-preserving**: `sign(int)->int`, `sign(f32)->f32`,
/// `sign(f64)->f64`. numpy-exact special cases (via [`sign_f64`] /
/// [`sign_f32`]): `sign(0.0)=0.0`, `sign(-0.0)=0.0`, `sign(NaN)=NaN`.
/// `sign(bool)` returns the bool array unchanged (numpy RAISES on bool;
/// coil's unary kernels are TOTAL — `sign(True)=1=True`,
/// `sign(False)=0=False`, a documented Semantic divergence). Total.
#[must_use]
pub fn sign(a: &Array) -> Array {
    unary_value(a, sign_i32, sign_i64, sign_f32, sign_f64)
}

#[cfg(test)]
mod tests {
    // Differential-vs-numpy unit tests. Oracle values captured from
    // numpy 2.4.6 via `/opt/homebrew/bin/python3.11 -c 'import numpy'`;
    // the transcendental ufunc semantics are identical to the
    // coil-provenance numpy 2.0.2. Approx comparison (rtol) for
    // transcendental values; EXACT for inf; `.is_nan()` for NaN.
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::float_cmp)]
    #![allow(clippy::approx_constant)]
    #![allow(clippy::cast_possible_truncation)]
    use super::*;
    use crate::constructors::{array_bool, array_f32, array_f64, array_i32, array_i64};

    // ---- differential helpers ----

    /// Relative-tolerance comparison for f64 transcendental values
    /// (rtol 1e-12). Exact 0.0 compares exact.
    fn approx_f64(got: f64, want: f64, rtol: f64) -> bool {
        if want == 0.0 {
            got.abs() <= rtol
        } else {
            ((got - want) / want).abs() <= rtol
        }
    }

    fn approx_f32(got: f32, want: f32, rtol: f32) -> bool {
        if want == 0.0 {
            got.abs() <= rtol
        } else {
            ((got - want) / want).abs() <= rtol
        }
    }

    fn f64_vals(a: &Array) -> Vec<f64> {
        match a {
            Array::Float64(arr) => arr.iter().copied().collect(),
            _ => panic!("expected Float64, got {:?}", a.dtype()),
        }
    }

    fn f32_vals(a: &Array) -> Vec<f32> {
        match a {
            Array::Float32(arr) => arr.iter().copied().collect(),
            _ => panic!("expected Float32, got {:?}", a.dtype()),
        }
    }

    // ---- exp (f64 + dtype) ----

    #[test]
    fn exp_f64_values_and_dtype() {
        // np.exp([0,1,2]) -> [1, 2.718281828459045, 7.38905609893065], f64.
        let a = array_f64(&[0.0, 1.0, 2.0], &[3]).unwrap();
        let r = exp(&a);
        assert_eq!(r.dtype(), Dtype::Float64);
        let v = f64_vals(&r);
        assert!(approx_f64(v[0], 1.0, 1e-12));
        assert!(approx_f64(v[1], 2.718_281_828_459_045, 1e-12));
        assert!(approx_f64(v[2], 7.389_056_098_930_65, 1e-12));
    }

    #[test]
    fn exp_overflow_is_plus_inf() {
        // np.exp(710) -> inf (overflow). EXACT special value, not error.
        let a = array_f64(&[710.0], &[1]).unwrap();
        let v = f64_vals(&exp(&a));
        assert!(v[0].is_infinite() && v[0] > 0.0);
    }

    // ---- log (natural ln) + NaN/inf edges ----

    #[test]
    fn log_natural_values() {
        // np.log([1, e, e^2]) -> [0, 1, 2].
        let a = array_f64(
            &[1.0, std::f64::consts::E, std::f64::consts::E.powi(2)],
            &[3],
        )
        .unwrap();
        let v = f64_vals(&log(&a));
        assert!(approx_f64(v[0], 0.0, 1e-12));
        assert!(approx_f64(v[1], 1.0, 1e-12));
        assert!(approx_f64(v[2], 2.0, 1e-12));
    }

    #[test]
    fn log_zero_is_neg_inf_log_neg_is_nan() {
        // np.log(0) -> -inf (EXACT); np.log(-1) -> nan.
        let a = array_f64(&[0.0, -1.0], &[2]).unwrap();
        let v = f64_vals(&log(&a));
        assert!(v[0].is_infinite() && v[0] < 0.0);
        assert!(v[1].is_nan());
    }

    // ---- log10 + edges ----

    #[test]
    fn log10_values_and_edges() {
        // np.log10([1,10,100]) -> [0,1,2]; log10(0) -> -inf; log10(-1) -> nan.
        let a = array_f64(&[1.0, 10.0, 100.0], &[3]).unwrap();
        let v = f64_vals(&log10(&a));
        assert!(approx_f64(v[0], 0.0, 1e-12));
        assert!(approx_f64(v[1], 1.0, 1e-12));
        assert!(approx_f64(v[2], 2.0, 1e-12));
        let edges = f64_vals(&log10(&array_f64(&[0.0, -1.0], &[2]).unwrap()));
        assert!(edges[0].is_infinite() && edges[0] < 0.0);
        assert!(edges[1].is_nan());
    }

    // ---- sqrt + NaN edge ----

    #[test]
    fn sqrt_values_and_neg_is_nan() {
        // np.sqrt([0,1,4,9]) -> [0,1,2,3]; sqrt(-1) -> nan.
        let a = array_f64(&[0.0, 1.0, 4.0, 9.0], &[4]).unwrap();
        let v = f64_vals(&sqrt(&a));
        assert_eq!(v, vec![0.0, 1.0, 2.0, 3.0]);
        let neg = f64_vals(&sqrt(&array_f64(&[-1.0], &[1]).unwrap()));
        assert!(neg[0].is_nan());
    }

    // ---- sin / cos / tan ----

    #[test]
    fn sin_cos_tan_values() {
        // np.sin([0, pi/2, pi]) -> [0, 1, ~0]; np.cos -> [1, ~0, -1];
        // np.tan([0, pi/4]) -> [0, ~1].
        let pi = std::f64::consts::PI;
        let s = f64_vals(&sin(&array_f64(&[0.0, pi / 2.0, pi], &[3]).unwrap()));
        assert!(approx_f64(s[0], 0.0, 1e-12));
        assert!(approx_f64(s[1], 1.0, 1e-12));
        // sin(pi) is ~1.2e-16, not exactly 0 — within abs tol.
        assert!(s[2].abs() < 1e-12);
        let c = f64_vals(&cos(&array_f64(&[0.0, pi / 2.0, pi], &[3]).unwrap()));
        assert!(approx_f64(c[0], 1.0, 1e-12));
        assert!(c[1].abs() < 1e-12);
        assert!(approx_f64(c[2], -1.0, 1e-12));
        let t = f64_vals(&tan(&array_f64(&[0.0, pi / 4.0], &[2]).unwrap()));
        assert!(approx_f64(t[0], 0.0, 1e-12));
        assert!(approx_f64(t[1], 1.0, 1e-12));
    }

    // ---- int -> f64 promotion (the #1 correctness nuance) ----

    #[test]
    fn exp_int64_promotes_to_f64() {
        // np.exp(np.int64([0,1,2,3])).dtype == float64.
        let a = array_i64(&[0, 1, 2, 3], &[4]).unwrap();
        let r = exp(&a);
        assert_eq!(r.dtype(), Dtype::Float64);
        let v = f64_vals(&r);
        assert!(approx_f64(v[3], 20.085_536_923_187_668, 1e-12));
    }

    #[test]
    fn sqrt_int64_promotes_to_f64() {
        // np.sqrt(np.int64([0,1,4,9])) -> [0,1,2,3] f64.
        let a = array_i64(&[0, 1, 4, 9], &[4]).unwrap();
        let r = sqrt(&a);
        assert_eq!(r.dtype(), Dtype::Float64);
        assert_eq!(f64_vals(&r), vec![0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn log10_int32_promotes_to_f64() {
        // np.log10(np.int32([1,10,100])).dtype == float64 -> [0,1,2].
        let a = array_i32(&[1, 10, 100], &[3]).unwrap();
        let r = log10(&a);
        assert_eq!(r.dtype(), Dtype::Float64);
        let v = f64_vals(&r);
        assert!(approx_f64(v[0], 0.0, 1e-12));
        assert!(approx_f64(v[1], 1.0, 1e-12));
        assert!(approx_f64(v[2], 2.0, 1e-12));
    }

    // ---- f32 stays f32 ----

    #[test]
    fn sqrt_f32_stays_f32() {
        // np.sqrt(np.float32([0,1,4])).dtype == float32 -> [0,1,2].
        let a = array_f32(&[0.0, 1.0, 4.0], &[3]).unwrap();
        let r = sqrt(&a);
        assert_eq!(r.dtype(), Dtype::Float32);
        assert_eq!(f32_vals(&r), vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn exp_f32_stays_f32() {
        // np.exp(np.float32([0,1,2])).dtype == float32. f32 libm precision
        // diverges from f64 at the ~1e-6 tier (numpy: 2.7182817, 7.389056).
        let a = array_f32(&[0.0, 1.0, 2.0], &[3]).unwrap();
        let r = exp(&a);
        assert_eq!(r.dtype(), Dtype::Float32);
        let v = f32_vals(&r);
        assert!(approx_f32(v[0], 1.0, 1e-6));
        assert!(approx_f32(v[1], 2.718_281_7, 1e-6));
        assert!(approx_f32(v[2], 7.389_056, 1e-6));
    }

    // ---- bool -> f64 (coil divergence from numpy's float16; values match) ----

    #[test]
    fn exp_bool_promotes_to_f64_values_match() {
        // numpy `np.exp(bool)` is float16; coil has no f16 -> f64. VALUES
        // match: exp(True)=e, exp(False)=1.
        let a = array_bool(&[true, false], &[2]).unwrap();
        let r = exp(&a);
        assert_eq!(r.dtype(), Dtype::Float64);
        let v = f64_vals(&r);
        assert!(approx_f64(v[0], std::f64::consts::E, 1e-12));
        assert!(approx_f64(v[1], 1.0, 1e-12));
    }

    #[test]
    fn sqrt_bool_promotes_to_f64() {
        // sqrt(True)=1, sqrt(False)=0. coil f64 (numpy f16; values match).
        let a = array_bool(&[true, false], &[2]).unwrap();
        let r = sqrt(&a);
        assert_eq!(r.dtype(), Dtype::Float64);
        assert_eq!(f64_vals(&r), vec![1.0, 0.0]);
    }

    // ---- shape preservation ----

    #[test]
    fn unary_preserves_shape() {
        let a = array_f64(&[1.0, 4.0, 9.0, 16.0], &[2, 2]).unwrap();
        assert_eq!(sqrt(&a).shape(), vec![2, 2]);
        assert_eq!(exp(&a).shape(), vec![2, 2]);
        assert_eq!(log(&a).shape(), vec![2, 2]);
    }

    // ---- chain (proves a fresh Array feeds the next op) ----

    #[test]
    fn chain_sqrt_of_exp() {
        // sqrt(exp([0,2])) = [sqrt(1), sqrt(e^2)] = [1, e].
        let a = array_f64(&[0.0, 2.0], &[2]).unwrap();
        let v = f64_vals(&sqrt(&exp(&a)));
        assert!(approx_f64(v[0], 1.0, 1e-12));
        assert!(approx_f64(v[1], std::f64::consts::E, 1e-12));
    }

    // ---- optional ops: same dtype rule + values ----

    #[test]
    fn exp2_log2_values_and_dtype() {
        // np.exp2([0,1,2,10]) -> [1,2,4,1024]; np.log2([1,2,8]) -> [0,1,3].
        let r = exp2(&array_f64(&[0.0, 1.0, 2.0, 10.0], &[4]).unwrap());
        assert_eq!(r.dtype(), Dtype::Float64);
        assert_eq!(f64_vals(&r), vec![1.0, 2.0, 4.0, 1024.0]);
        let l = f64_vals(&log2(&array_f64(&[1.0, 2.0, 8.0], &[3]).unwrap()));
        assert!(approx_f64(l[0], 0.0, 1e-12));
        assert!(approx_f64(l[1], 1.0, 1e-12));
        assert!(approx_f64(l[2], 3.0, 1e-12));
    }

    #[test]
    fn cbrt_handles_negatives() {
        // np.cbrt([8,27,-8]) -> [2,3,-2] (cbrt IS defined for negatives).
        let v = f64_vals(&cbrt(&array_f64(&[8.0, 27.0, -8.0], &[3]).unwrap()));
        assert!(approx_f64(v[0], 2.0, 1e-12));
        assert!(approx_f64(v[1], 3.0, 1e-12));
        assert!(approx_f64(v[2], -2.0, 1e-12));
    }

    #[test]
    fn hyperbolics_values_and_promotion() {
        // np.sinh([0,1]) -> [0, 1.1752...]; cosh -> [1, 1.5430...];
        // tanh -> [0, 0.7615...].
        let s = f64_vals(&sinh(&array_f64(&[0.0, 1.0], &[2]).unwrap()));
        assert!(approx_f64(s[0], 0.0, 1e-12));
        assert!(approx_f64(s[1], 1.175_201_193_643_801_4, 1e-12));
        let c = f64_vals(&cosh(&array_f64(&[0.0, 1.0], &[2]).unwrap()));
        assert!(approx_f64(c[0], 1.0, 1e-12));
        assert!(approx_f64(c[1], 1.543_080_634_815_243_7, 1e-12));
        let t = f64_vals(&tanh(&array_f64(&[0.0, 1.0], &[2]).unwrap()));
        assert!(approx_f64(t[0], 0.0, 1e-12));
        assert!(approx_f64(t[1], 0.761_594_155_955_764_9, 1e-12));
        // int -> f64 promotion holds for the optionals too.
        assert_eq!(
            cbrt(&array_i64(&[8, 27], &[2]).unwrap()).dtype(),
            Dtype::Float64
        );
        // f32 stays f32 for the optionals too.
        assert_eq!(
            exp2(&array_f32(&[1.0, 2.0], &[2]).unwrap()).dtype(),
            Dtype::Float32
        );
    }

    // =====================================================================
    // BATCH 4 — the DTYPE-PRESERVING rounding / sign family
    // (`abs`/`floor`/`ceil`/`round`/`trunc`/`square`/`sign`). Differential-
    // vs-numpy 2.4.6 (oracle values captured via
    // `/opt/homebrew/bin/python3.11 -c 'import numpy'`). These assert on
    // `.dtype()` (the preserving contract) in ADDITION to values. NaN via
    // `.is_nan()`, never `assert_eq!(x, NAN)`.
    // =====================================================================

    /// Extract `Vec<i64>` from an `Int64` array (else panic with the dtype).
    fn i64_vals(a: &Array) -> Vec<i64> {
        match a {
            Array::Int64(arr) => arr.iter().copied().collect(),
            _ => panic!("expected Int64, got {:?}", a.dtype()),
        }
    }

    /// Extract `Vec<i32>` from an `Int32` array (else panic with the dtype).
    fn i32_vals(a: &Array) -> Vec<i32> {
        match a {
            Array::Int32(arr) => arr.iter().copied().collect(),
            _ => panic!("expected Int32, got {:?}", a.dtype()),
        }
    }

    /// Extract `Vec<bool>` from a `Bool` array (else panic with the dtype).
    fn bool_vals(a: &Array) -> Vec<bool> {
        match a {
            Array::Bool(arr) => arr.iter().copied().collect(),
            _ => panic!("expected Bool, got {:?}", a.dtype()),
        }
    }

    // ---- abs: dtype-preserving + negative inputs ----

    #[test]
    fn abs_int64_preserves_dtype_and_values() {
        // np.abs(np.int64([-1,-2,3])) -> [1,2,3], dtype int64 (PRESERVED).
        let a = array_i64(&[-1, -2, 3, 0], &[4]).unwrap();
        let r = abs(&a);
        assert_eq!(r.dtype(), Dtype::Int64);
        assert_eq!(i64_vals(&r), vec![1, 2, 3, 0]);
    }

    #[test]
    fn abs_int32_preserves_dtype() {
        // np.abs(np.int32([-5,5])) -> [5,5], dtype int32.
        let a = array_i32(&[-5, 5], &[2]).unwrap();
        let r = abs(&a);
        assert_eq!(r.dtype(), Dtype::Int32);
        assert_eq!(i32_vals(&r), vec![5, 5]);
    }

    #[test]
    fn abs_f64_and_f32_preserve_dtype() {
        // np.abs([-1.5,2.5]) -> [1.5,2.5] f64; f32 stays f32.
        let r64 = abs(&array_f64(&[-1.5, 2.5, -0.0], &[3]).unwrap());
        assert_eq!(r64.dtype(), Dtype::Float64);
        assert_eq!(f64_vals(&r64), vec![1.5, 2.5, 0.0]);
        let r32 = abs(&array_f32(&[-1.5, 2.5], &[2]).unwrap());
        assert_eq!(r32.dtype(), Dtype::Float32);
        assert_eq!(f32_vals(&r32), vec![1.5, 2.5]);
    }

    #[test]
    fn abs_nan_stays_nan() {
        // np.abs(nan) -> nan.
        let r = abs(&array_f64(&[f64::NAN], &[1]).unwrap());
        assert!(f64_vals(&r)[0].is_nan());
    }

    // ---- floor / ceil / trunc: INTEGER no-op (the #1 dtype subtlety) ----

    #[test]
    fn round_family_int64_is_noop_dtype_preserved() {
        // numpy 2.x: np.floor/ceil/round/trunc(int64([1,2,3])) -> [1,2,3]
        // UNCHANGED, dtype int64 (NOT promoted to float).
        let a = array_i64(&[1, 2, 3], &[3]).unwrap();
        for op in [floor, ceil, round, trunc] {
            let r = op(&a);
            assert_eq!(r.dtype(), Dtype::Int64, "int no-op must preserve int64");
            assert_eq!(i64_vals(&r), vec![1, 2, 3]);
        }
    }

    #[test]
    fn round_family_int32_is_noop() {
        let a = array_i32(&[-7, 0, 7], &[3]).unwrap();
        for op in [floor, ceil, round, trunc] {
            let r = op(&a);
            assert_eq!(r.dtype(), Dtype::Int32);
            assert_eq!(i32_vals(&r), vec![-7, 0, 7]);
        }
    }

    // ---- floor / ceil / trunc: float values + dtype-preserving ----

    #[test]
    fn floor_f64_values_and_dtype() {
        // np.floor([-1.5,1.5,2.7,-0.1]) -> [-2,1,2,-1] f64.
        let r = floor(&array_f64(&[-1.5, 1.5, 2.7, -0.1], &[4]).unwrap());
        assert_eq!(r.dtype(), Dtype::Float64);
        assert_eq!(f64_vals(&r), vec![-2.0, 1.0, 2.0, -1.0]);
    }

    #[test]
    fn ceil_f64_values_and_dtype() {
        // np.ceil([-1.5,1.5,2.3,0.1]) -> [-1,2,3,1] f64.
        let r = ceil(&array_f64(&[-1.5, 1.5, 2.3, 0.1], &[4]).unwrap());
        assert_eq!(r.dtype(), Dtype::Float64);
        assert_eq!(f64_vals(&r), vec![-1.0, 2.0, 3.0, 1.0]);
    }

    #[test]
    fn trunc_f64_toward_zero_unlike_floor() {
        // np.trunc([-1.7,1.7,-0.9,0.9]) -> [-1,1,-0,0] (toward zero).
        let r = trunc(&array_f64(&[-1.7, 1.7, -0.9, 0.9], &[4]).unwrap());
        assert_eq!(r.dtype(), Dtype::Float64);
        assert_eq!(f64_vals(&r), vec![-1.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn floor_f32_stays_f32() {
        let r = floor(&array_f32(&[-1.5, 2.7], &[2]).unwrap());
        assert_eq!(r.dtype(), Dtype::Float32);
        assert_eq!(f32_vals(&r), vec![-2.0, 2.0]);
    }

    #[test]
    fn floor_ceil_nan_inf_edges() {
        // floor/ceil/trunc(NaN)=NaN; floor(-inf)=-inf; ceil(+inf)=+inf.
        let nan = floor(&array_f64(&[f64::NAN], &[1]).unwrap());
        assert!(f64_vals(&nan)[0].is_nan());
        let neg_inf = floor(&array_f64(&[f64::NEG_INFINITY], &[1]).unwrap());
        assert!(f64_vals(&neg_inf)[0].is_infinite() && f64_vals(&neg_inf)[0] < 0.0);
        let pos_inf = ceil(&array_f64(&[f64::INFINITY], &[1]).unwrap());
        assert!(f64_vals(&pos_inf)[0].is_infinite() && f64_vals(&pos_inf)[0] > 0.0);
    }

    // ---- round: round-half-to-EVEN (the #2 correctness nuance) ----

    #[test]
    fn round_is_half_to_even_bankers() {
        // numpy np.round: 0.5->0, 1.5->2, 2.5->2, 3.5->4, -0.5->-0, -1.5->-2.
        // (Rust f64::round would give 0.5->1, 2.5->3 — WRONG vs numpy.)
        let r = round(&array_f64(&[0.5, 1.5, 2.5, 3.5, -0.5, -1.5], &[6]).unwrap());
        assert_eq!(r.dtype(), Dtype::Float64);
        // -0.5 rounds to -0.0 which compares == 0.0; the value contract is
        // satisfied by the f64 list equality (numpy prints -0. too).
        assert_eq!(f64_vals(&r), vec![0.0, 2.0, 2.0, 4.0, 0.0, -2.0]);
    }

    #[test]
    fn round_half_to_even_f32() {
        // f32 banker's rounding too: 0.5->0, 1.5->2, 2.5->2.
        let r = round(&array_f32(&[0.5, 1.5, 2.5], &[3]).unwrap());
        assert_eq!(r.dtype(), Dtype::Float32);
        assert_eq!(f32_vals(&r), vec![0.0, 2.0, 2.0]);
    }

    #[test]
    fn round_non_half_values() {
        // np.round([2.3,2.7,-2.3,-2.7]) -> [2,3,-2,-3].
        let r = round(&array_f64(&[2.3, 2.7, -2.3, -2.7], &[4]).unwrap());
        assert_eq!(f64_vals(&r), vec![2.0, 3.0, -2.0, -3.0]);
    }

    #[test]
    fn round_nan_stays_nan() {
        let r = round(&array_f64(&[f64::NAN], &[1]).unwrap());
        assert!(f64_vals(&r)[0].is_nan());
    }

    // ---- square: dtype-preserving + negative inputs ----

    #[test]
    fn square_int64_values_and_dtype() {
        // np.square(np.int64([2,-3,0,4])) -> [4,9,0,16], dtype int64.
        let r = square(&array_i64(&[2, -3, 0, 4], &[4]).unwrap());
        assert_eq!(r.dtype(), Dtype::Int64);
        assert_eq!(i64_vals(&r), vec![4, 9, 0, 16]);
    }

    #[test]
    fn square_f64_and_f32_values_and_dtype() {
        // np.square([1.5,-2.0,0.0]) -> [2.25,4,0] f64; f32 stays f32.
        let r64 = square(&array_f64(&[1.5, -2.0, 0.0], &[3]).unwrap());
        assert_eq!(r64.dtype(), Dtype::Float64);
        assert_eq!(f64_vals(&r64), vec![2.25, 4.0, 0.0]);
        let r32 = square(&array_f32(&[1.5, -2.0], &[2]).unwrap());
        assert_eq!(r32.dtype(), Dtype::Float32);
        assert_eq!(f32_vals(&r32), vec![2.25, 4.0]);
    }

    // ---- sign: 0/-0/NaN + +/- + dtype-preserving (the #2 nuance) ----

    #[test]
    fn sign_f64_zero_and_nan_and_signs() {
        // numpy np.sign([-2.5,0.0,-0.0,3.0,nan]) -> [-1,0,0,1,nan].
        // (Rust f64::signum would give +1 for 0.0 and propagate NaN sign
        // bit — WRONG vs numpy. Explicit branch pins it.)
        let r = sign(&array_f64(&[-2.5, 0.0, -0.0, 3.0, f64::NAN], &[5]).unwrap());
        assert_eq!(r.dtype(), Dtype::Float64);
        let v = f64_vals(&r);
        assert_eq!(v[0], -1.0);
        // sign(0.0) == 0.0 (NOT +1.0 as f64::signum would give).
        assert_eq!(v[1], 0.0);
        // sign(-0.0) == 0.0 too.
        assert_eq!(v[2], 0.0);
        assert_eq!(v[3], 1.0);
        // sign(NaN).is_nan() (never assert_eq! against NaN).
        assert!(v[4].is_nan());
    }

    #[test]
    fn sign_f32_zero_and_nan() {
        let r = sign(&array_f32(&[-2.5, 0.0, 3.0, f32::NAN], &[4]).unwrap());
        assert_eq!(r.dtype(), Dtype::Float32);
        let v = f32_vals(&r);
        assert_eq!(v[0], -1.0);
        assert_eq!(v[1], 0.0);
        assert_eq!(v[2], 1.0);
        assert!(v[3].is_nan());
    }

    #[test]
    fn sign_int64_values_and_dtype() {
        // np.sign(np.int64([-5,0,7])) -> [-1,0,1], dtype int64.
        let r = sign(&array_i64(&[-5, 0, 7], &[3]).unwrap());
        assert_eq!(r.dtype(), Dtype::Int64);
        assert_eq!(i64_vals(&r), vec![-1, 0, 1]);
    }

    #[test]
    fn sign_int32_values_and_dtype() {
        let r = sign(&array_i32(&[-5, 0, 7], &[3]).unwrap());
        assert_eq!(r.dtype(), Dtype::Int32);
        assert_eq!(i32_vals(&r), vec![-1, 0, 1]);
    }

    // ---- bool input: coil's documented Semantic divergence (unchanged) ----

    #[test]
    fn bool_input_returns_unchanged_for_all_ops() {
        // coil pins every BATCH-4 op to return the Bool array UNCHANGED
        // (value-faithful: True/False is the 0/1 fixed point). numpy would
        // promote round->float16 / square->int8 / raise on sign(bool); coil
        // keeps Bool (Semantic-tier divergence; VALUES match).
        let a = array_bool(&[true, false, true], &[3]).unwrap();
        for op in [abs, floor, ceil, round, trunc, square, sign] {
            let r = op(&a);
            assert_eq!(r.dtype(), Dtype::Bool, "bool input must stay Bool in coil");
            assert_eq!(bool_vals(&r), vec![true, false, true]);
        }
    }

    // ---- shape preservation ----

    #[test]
    fn batch4_preserves_shape() {
        let a = array_f64(&[-1.5, 2.5, -3.5, 4.5], &[2, 2]).unwrap();
        assert_eq!(abs(&a).shape(), vec![2, 2]);
        assert_eq!(floor(&a).shape(), vec![2, 2]);
        assert_eq!(round(&a).shape(), vec![2, 2]);
        assert_eq!(square(&a).shape(), vec![2, 2]);
        assert_eq!(sign(&a).shape(), vec![2, 2]);
    }

    // ---- chain (proves a fresh Array feeds the next op) ----

    #[test]
    fn chain_abs_of_floor() {
        // abs(floor([-1.5, 2.5, -0.5])) = abs([-2, 2, -1]) = [2, 2, 1].
        let a = array_f64(&[-1.5, 2.5, -0.5], &[3]).unwrap();
        let r = abs(&floor(&a));
        assert_eq!(r.dtype(), Dtype::Float64);
        assert_eq!(f64_vals(&r), vec![2.0, 2.0, 1.0]);
    }

    #[test]
    fn chain_sign_of_square_is_nonneg() {
        // square always >= 0, so sign(square(x)) is 0 (x==0) or 1.
        // sign(square([-3.0, 0.0, 2.0])) = sign([9,0,4]) = [1,0,1].
        let a = array_f64(&[-3.0, 0.0, 2.0], &[3]).unwrap();
        let r = sign(&square(&a));
        assert_eq!(f64_vals(&r), vec![1.0, 0.0, 1.0]);
    }
}
