// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.1 ufuncs per ADR-0014 §3 + M7.6 complex extension per ADR-0021 §4.

//! Type-promotion table for cobrust-coil ufuncs (per ADR-0014 §3 +
//! ADR-0021 §4).
//!
//! Implements `result_type(a, b)` for the seven-dtype tier
//! (M7.0 5 dtypes + M7.6 Complex64/Complex128) per NumPy 2.x NEP 50
//! (https://numpy.org/neps/nep-0050-scalar-promotion.html).
//! Hand-coded match table — explicit, auditable, fast.
//!
//! Per ADR-0021 §4: complex is the "top of the lattice". Notable
//! NEP 50 corners:
//!   - `Complex128 + anything` → `Complex128`.
//!   - `Complex64 + Float64 / Int64 / Int32` → `Complex128` (mantissa
//!     wider than `f32`).
//!   - `Complex64 + Float32 / Bool` → `Complex64`.
//!   - `Complex64 + Complex64` → `Complex64`.
//!
//! Comparison ops always return `Dtype::Bool` for non-complex; for
//! complex `lt/le/gt/ge` raise `ComplexNotOrderable`. That is **not**
//! done through `result_type` — callers in `ufunc.rs` route
//! comparisons to a separate path. `result_type` reflects
//! arithmetic-op promotion only.

// CQ P1-4 + template-fix: single consolidated block; future emits use #[allow] at item level.
#![allow(
    clippy::match_same_arms,
    clippy::missing_panics_doc,
    clippy::must_use_candidate
)]

use crate::dtype::Dtype;

/// Compute the promoted dtype for an arithmetic ufunc on operands of
/// dtypes `a` and `b`.
///
/// Per ADR-0014 §3 + ADR-0021 §4. The decision tree:
/// - Both complex → wider precision wins.
/// - One complex, other real → upgrade to complex of width sufficient
///   for the real mantissa (`Complex64 + i64` → `Complex128`).
/// - Same dtype on both sides → preserve.
/// - `Bool` is "smaller" than every numeric dtype.
/// - `Int32 + Float32 → Float64` (NEP 50: i32 mantissa doesn't fit in f32).
/// - `Int64 + Float* → Float64` (i64 mantissa doesn't fit in f32).
/// - `Float32 + Float64 → Float64` (standard width-up).
#[must_use]
pub fn result_type(a: Dtype, b: Dtype) -> Dtype {
    use Dtype::{Bool, Complex64, Complex128, Float32, Float64, Int32, Int64};
    match (a, b) {
        // ---- Complex128 row (always Complex128) ----
        (Complex128, _) | (_, Complex128) => Complex128,

        // ---- Complex64 row ----
        (Complex64, Complex64) => Complex64,
        // f32/bool stays in single precision complex.
        (Complex64, Float32) | (Float32, Complex64) => Complex64,
        (Complex64, Bool) | (Bool, Complex64) => Complex64,
        // f64/i32/i64 widens to double precision complex.
        (Complex64, Float64) | (Float64, Complex64) => Complex128,
        (Complex64, Int32) | (Int32, Complex64) => Complex128,
        (Complex64, Int64) | (Int64, Complex64) => Complex128,

        // ---- Bool row (M7.1 inherited) ----
        (Bool, Bool) => Bool,
        (Bool, Int32) | (Int32, Bool) => Int32,
        (Bool, Int64) | (Int64, Bool) => Int64,
        (Bool, Float32) | (Float32, Bool) => Float32,
        (Bool, Float64) | (Float64, Bool) => Float64,

        // ---- Int rows (M7.1 inherited) ----
        (Int32, Int32) => Int32,
        (Int32, Int64) | (Int64, Int32) => Int64,
        (Int32, Float32) | (Float32, Int32) => Float64,
        (Int32, Float64) | (Float64, Int32) => Float64,
        (Int64, Int64) => Int64,
        (Int64, Float32) | (Float32, Int64) => Float64,
        (Int64, Float64) | (Float64, Int64) => Float64,

        // ---- Float rows (M7.1 inherited) ----
        (Float32, Float32) => Float32,
        (Float32, Float64) | (Float64, Float32) => Float64,
        (Float64, Float64) => Float64,
    }
}

/// Promote integer dtypes to `Float64` for unary math ops
/// (`sin / cos / exp / log / sqrt`). Float dtypes are preserved
/// (sin on f32 stays f32 to match numpy). Complex dtypes are
/// preserved at their precision tier.
#[must_use]
pub fn unary_math_dtype(input: Dtype) -> Dtype {
    match input {
        Dtype::Bool | Dtype::Int32 | Dtype::Int64 => Dtype::Float64,
        Dtype::Float32 => Dtype::Float32,
        Dtype::Float64 => Dtype::Float64,
        Dtype::Complex64 => Dtype::Complex64,
        Dtype::Complex128 => Dtype::Complex128,
    }
}

/// Compute the result dtype for NumPy **true division** (`/`, the
/// `true_divide` ufunc) on operands of dtypes `a` and `b`.
///
/// Per NumPy: `/` ALWAYS yields a floating result — integer / boolean
/// operands are promoted to `Float64` BEFORE the division, so
/// `int / int → float64` (NOT integer floor-division) and `int / 0 →
/// IEEE inf` (a RuntimeWarning, never an exception). This DIVERGES from
/// the dtype-preserving [`result_type`] used by `+`/`-`/`*` (where
/// `int + int → int`). The rule:
/// - integer / boolean operands → promoted to `Float64` first;
/// - `float32 / float32 → float32` (single precision preserved);
/// - any `float64` (or mixed float32/float64) → `float64`;
/// - complex tiers are preserved at their precision (`result_type`'s
///   complex lattice on the float-promoted operands).
///
/// Implementation: map each integer/bool dtype to its floating promotion
/// (`Float64`) then defer to [`result_type`] on the promoted pair (which
/// already encodes the float-width + complex lattice).
#[must_use]
pub fn true_div_dtype(a: Dtype, b: Dtype) -> Dtype {
    result_type(to_floating(a), to_floating(b))
}

/// Promote an integer / boolean dtype to its NumPy true-division
/// floating counterpart (`Float64`); float + complex dtypes pass
/// through unchanged. The helper for [`true_div_dtype`].
fn to_floating(d: Dtype) -> Dtype {
    match d {
        Dtype::Bool | Dtype::Int32 | Dtype::Int64 => Dtype::Float64,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::cast_possible_truncation)]
    #![allow(clippy::cast_possible_wrap)]
    #![allow(clippy::cast_precision_loss)]
    #![allow(clippy::cast_sign_loss)]
    #![allow(clippy::format_push_string)]
    #![allow(clippy::let_unit_value)]
    #![allow(clippy::ignored_unit_patterns)]
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    #![allow(clippy::float_cmp)]
    #![allow(clippy::imprecise_flops)]
    #![allow(clippy::suboptimal_flops)]
    #![allow(clippy::similar_names)]
    #![allow(clippy::approx_constant)]
    #![allow(clippy::uninlined_format_args)]
    use super::*;
    use Dtype::{Bool, Complex64, Complex128, Float32, Float64, Int32, Int64};

    #[test]
    fn same_dtype_preserved() {
        for d in [Bool, Int32, Int64, Float32, Float64, Complex64, Complex128] {
            assert_eq!(result_type(d, d), d);
        }
    }

    #[test]
    fn nep50_int32_float32_widens_to_float64() {
        assert_eq!(result_type(Int32, Float32), Float64);
        assert_eq!(result_type(Float32, Int32), Float64);
    }

    #[test]
    fn float_width_up() {
        assert_eq!(result_type(Float32, Float64), Float64);
        assert_eq!(result_type(Float64, Float32), Float64);
    }

    #[test]
    fn bool_smaller_than_numeric() {
        assert_eq!(result_type(Bool, Int32), Int32);
        assert_eq!(result_type(Bool, Float64), Float64);
    }

    #[test]
    fn int_widening() {
        assert_eq!(result_type(Int32, Int64), Int64);
        assert_eq!(result_type(Int64, Int32), Int64);
    }

    #[test]
    fn unary_int_to_f64() {
        assert_eq!(unary_math_dtype(Int32), Float64);
        assert_eq!(unary_math_dtype(Int64), Float64);
        assert_eq!(unary_math_dtype(Bool), Float64);
    }

    #[test]
    fn unary_float_preserves() {
        assert_eq!(unary_math_dtype(Float32), Float32);
        assert_eq!(unary_math_dtype(Float64), Float64);
    }

    // ---- M7.6 complex tests (per ADR-0021 §4) -------------------------------

    #[test]
    fn complex128_dominates_anything() {
        for d in [Bool, Int32, Int64, Float32, Float64, Complex64] {
            assert_eq!(result_type(Complex128, d), Complex128);
            assert_eq!(result_type(d, Complex128), Complex128);
        }
        assert_eq!(result_type(Complex128, Complex128), Complex128);
    }

    #[test]
    fn complex64_with_low_precision_real_stays_c64() {
        assert_eq!(result_type(Complex64, Float32), Complex64);
        assert_eq!(result_type(Complex64, Bool), Complex64);
        assert_eq!(result_type(Complex64, Complex64), Complex64);
    }

    #[test]
    fn complex64_with_high_precision_real_widens_to_c128() {
        assert_eq!(result_type(Complex64, Float64), Complex128);
        assert_eq!(result_type(Complex64, Int32), Complex128);
        assert_eq!(result_type(Complex64, Int64), Complex128);
    }

    #[test]
    fn complex_promotion_is_symmetric() {
        for a in [Bool, Int32, Int64, Float32, Float64, Complex64, Complex128] {
            for b in [Bool, Int32, Int64, Float32, Float64, Complex64, Complex128] {
                assert_eq!(
                    result_type(a, b),
                    result_type(b, a),
                    "promotion must be symmetric: {:?} + {:?}",
                    a,
                    b
                );
            }
        }
    }

    #[test]
    fn unary_complex_preserves_precision() {
        assert_eq!(unary_math_dtype(Complex64), Complex64);
        assert_eq!(unary_math_dtype(Complex128), Complex128);
    }

    #[test]
    fn dtype_is_complex_helper() {
        assert!(Complex64.is_complex());
        assert!(Complex128.is_complex());
        for d in [Bool, Int32, Int64, Float32, Float64] {
            assert!(!d.is_complex());
        }
    }

    #[test]
    fn dtype_is_floating_helper() {
        for d in [Float32, Float64, Complex64, Complex128] {
            assert!(d.is_floating());
        }
        for d in [Bool, Int32, Int64] {
            assert!(!d.is_floating());
        }
    }
}
