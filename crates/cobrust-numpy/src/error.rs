// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.0 dtype tier per ADR-0013 §3 + M7.1 ufuncs per ADR-0014 + M7.2 indexing per ADR-0015.
// see PROVENANCE.toml for the full manifest.

//! Single error type for cobrust-numpy.
//!
//! Per constitution §2.2 (Result<T,E> default error path), every
//! fallible cobrust-numpy public-API call returns `Result<_,
//! NumpyError>`. The kind is structured (closed enum) so callers can
//! match on it cleanly rather than parsing the message.
//!
//! M7.0 (per ADR-0013) shipped six variants. M7.1 (per ADR-0014 §4)
//! added three more. M7.2 (per ADR-0015 §4) adds four more for the
//! indexing surface.

#![allow(clippy::uninlined_format_args)]

use std::fmt;

/// Single error type for all cobrust-numpy operations.
#[derive(Clone, Debug, PartialEq)]
pub struct NumpyError {
    pub kind: NumpyErrorKind,
    pub message: String,
}

/// Closed enum of error categories emitted by cobrust-numpy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumpyErrorKind {
    // ---- M7.0 (per ADR-0013) ----
    /// Python dtype string not in the M7.0 closed set per ADR-0013 §3.
    UnsupportedDtype,
    /// `array(values, shape, dtype)`: `values.len()` does not match
    /// `shape_size(shape)`.
    ShapeMismatch,
    /// Negative dimension supplied to `zeros` / `ones` / `array`.
    NegativeDimension,
    /// `arange(start, stop, step, dtype)` with `step == 0` (matches
    /// numpy's `ZeroDivisionError`). Reused by M7.2 `slice` for
    /// `step == 0` per ADR-0015 §5.
    ZeroStep,
    /// `arange(...)` invoked with `dtype=bool` (matches numpy's
    /// `TypeError`).
    BoolArangeUnsupported,
    /// Values supplied to `array(...)` could not be cast to the
    /// requested dtype without precision loss in a way that violates
    /// the `@py_compat(strict)` contract.
    CastFailed,

    // ---- M7.1 (per ADR-0014 §4) ----
    /// Integer-dtype `Array::div` with a divisor element equal to 0.
    /// Matches numpy's `ZeroDivisionError` outcome (operation fails);
    /// shape of failure is Cobrust-native (`Result::Err`) per
    /// constitution §2.2.
    IntegerDivisionByZero,
    /// Two arrays' shapes cannot be broadcast together per the numpy
    /// rules (right-aligned, size-1-expand, equal-or-mismatch). See
    /// ADR-0014 §2 + https://numpy.org/doc/stable/user/basics.broadcasting.html.
    BroadcastShapeMismatch,
    /// `result_type(a, b)` could not produce a valid promoted dtype.
    /// Reserved for future widening; the current 5-dtype tier table
    /// is total, so this is not raised by the M7.1 closed set — kept
    /// to keep the surface stable across M7.x.
    TypePromotionFailure,

    // ---- M7.2 (per ADR-0015 §4) ----
    /// Umbrella for indexing errors not covered by more specific
    /// variants below — for example, applying multi-axis `index_get`
    /// with more axes than the array has.
    IndexError,
    /// Single-int or int-array index out of `[-len, len)`. Matches
    /// numpy's `IndexError`.
    OutOfBoundsIndex,
    /// Boolean mask passed to `mask` has shape != self.shape().
    /// Matches numpy's `IndexError`.
    BoolMaskShapeMismatch,
    /// Index array passed to `take` / `Index::IntArray` is not
    /// integer-dtype (must be `Int32` or `Int64`).
    IndexDtypeNotInteger,
}

impl fmt::Display for NumpyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind_name = match self.kind {
            NumpyErrorKind::UnsupportedDtype => "unsupported_dtype",
            NumpyErrorKind::ShapeMismatch => "shape_mismatch",
            NumpyErrorKind::NegativeDimension => "negative_dimension",
            NumpyErrorKind::ZeroStep => "zero_step",
            NumpyErrorKind::BoolArangeUnsupported => "bool_arange_unsupported",
            NumpyErrorKind::CastFailed => "cast_failed",
            NumpyErrorKind::IntegerDivisionByZero => "integer_division_by_zero",
            NumpyErrorKind::BroadcastShapeMismatch => "broadcast_shape_mismatch",
            NumpyErrorKind::TypePromotionFailure => "type_promotion_failure",
            NumpyErrorKind::IndexError => "index_error",
            NumpyErrorKind::OutOfBoundsIndex => "out_of_bounds_index",
            NumpyErrorKind::BoolMaskShapeMismatch => "bool_mask_shape_mismatch",
            NumpyErrorKind::IndexDtypeNotInteger => "index_dtype_not_integer",
        };
        write!(f, "NumpyError({kind_name}): {}", self.message)
    }
}

impl std::error::Error for NumpyError {}
