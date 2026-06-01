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
// see PROVENANCE.toml for the full manifest.

//! Unary transcendental elementwise free functions — the FLOAT-returning
//! `Array -> Array` math surface most-used in real numpy code
//! (`np.exp` / `np.log` / `np.log10` / `np.sqrt` / `np.sin` / `np.cos` /
//! `np.tan`), each returning a fresh owned `Array`.
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
}
