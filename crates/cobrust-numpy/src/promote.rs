// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.1 ufuncs per ADR-0014 §3.
// see PROVENANCE.toml for the full manifest.

//! Type-promotion table for cobrust-numpy ufuncs (per ADR-0014 §3).
//!
//! Implements `result_type(a, b)` for the M7.0 5-dtype tier per
//! NumPy 2.x NEP 50 (https://numpy.org/neps/nep-0050-scalar-promotion.html).
//! Hand-coded 25-entry table — explicit, auditable, fast.
//!
//! Comparison ops always return `Dtype::Bool`; that is **not** done
//! through `result_type` — callers in `ufunc.rs` route comparisons
//! to a separate path. `result_type` reflects arithmetic-op
//! promotion only.

#![allow(clippy::match_same_arms)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]

use crate::dtype::Dtype;

/// Compute the promoted dtype for an arithmetic ufunc on operands of
/// dtypes `a` and `b`.
///
/// Per ADR-0014 §3:
/// - Same dtype on both sides → preserve.
/// - `Bool` is "smaller" than every numeric dtype.
/// - `Int32 + Float32 → Float64` (NEP 50: i32 mantissa doesn't fit in f32).
/// - `Int64 + Float* → Float64` (i64 mantissa doesn't fit in f32).
/// - `Float32 + Float64 → Float64` (standard width-up).
#[must_use]
pub fn result_type(a: Dtype, b: Dtype) -> Dtype {
    use Dtype::{Bool, Float32, Float64, Int32, Int64};
    match (a, b) {
        // Bool row
        (Bool, Bool) => Bool,
        (Bool, Int32) | (Int32, Bool) => Int32,
        (Bool, Int64) | (Int64, Bool) => Int64,
        (Bool, Float32) | (Float32, Bool) => Float32,
        (Bool, Float64) | (Float64, Bool) => Float64,
        // Int rows
        (Int32, Int32) => Int32,
        (Int32, Int64) | (Int64, Int32) => Int64,
        (Int32, Float32) | (Float32, Int32) => Float64,
        (Int32, Float64) | (Float64, Int32) => Float64,
        (Int64, Int64) => Int64,
        (Int64, Float32) | (Float32, Int64) => Float64,
        (Int64, Float64) | (Float64, Int64) => Float64,
        // Float rows
        (Float32, Float32) => Float32,
        (Float32, Float64) | (Float64, Float32) => Float64,
        (Float64, Float64) => Float64,
    }
}

/// Promote integer dtypes to `Float64` for unary math ops
/// (`sin / cos / exp / log / sqrt`). Float dtypes are preserved
/// (sin on f32 stays f32 to match numpy).
#[must_use]
pub fn unary_math_dtype(input: Dtype) -> Dtype {
    match input {
        Dtype::Bool | Dtype::Int32 | Dtype::Int64 => Dtype::Float64,
        Dtype::Float32 => Dtype::Float32,
        Dtype::Float64 => Dtype::Float64,
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
    use Dtype::{Bool, Float32, Float64, Int32, Int64};

    #[test]
    fn same_dtype_preserved() {
        for d in [Bool, Int32, Int64, Float32, Float64] {
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
}
