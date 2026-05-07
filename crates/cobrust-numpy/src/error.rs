// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.0 dtype tier per ADR-0013 §3
// see PROVENANCE.toml for the full manifest.

//! Single error type for cobrust-numpy M7.0.
//!
//! Per constitution §2.2 (Result<T,E> default error path), every
//! fallible cobrust-numpy public-API call returns `Result<_,
//! NumpyError>`. The kind is structured (closed enum) so callers can
//! match on it cleanly rather than parsing the message.

use std::fmt;

/// Single error type for all cobrust-numpy M7.0 operations.
#[derive(Clone, Debug, PartialEq)]
pub struct NumpyError {
    pub kind: NumpyErrorKind,
    pub message: String,
}

/// Closed enum of error categories emitted by cobrust-numpy M7.0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumpyErrorKind {
    /// Python dtype string not in the M7.0 closed set per ADR-0013 §3.
    UnsupportedDtype,
    /// `array(values, shape, dtype)`: `values.len()` does not match
    /// `shape_size(shape)`.
    ShapeMismatch,
    /// Negative dimension supplied to `zeros` / `ones` / `array`.
    NegativeDimension,
    /// `arange(start, stop, step, dtype)` with `step == 0` (matches
    /// numpy's `ZeroDivisionError`).
    ZeroStep,
    /// `arange(...)` invoked with `dtype=bool` (matches numpy's
    /// `TypeError`).
    BoolArangeUnsupported,
    /// Values supplied to `array(...)` could not be cast to the
    /// requested dtype without precision loss in a way that violates
    /// the `@py_compat(strict)` contract.
    CastFailed,
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
        };
        write!(f, "NumpyError({kind_name}): {}", self.message)
    }
}

impl std::error::Error for NumpyError {}
