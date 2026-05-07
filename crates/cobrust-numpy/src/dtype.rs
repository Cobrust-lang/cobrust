// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.0 dtype tier per ADR-0013 §3
// see PROVENANCE.toml for the full manifest.

//! Closed dtype enum for the M7.0 ndarray foundation (ADR-0013 §3).
//!
//! Maps Python dtype strings (long form + type-char form) to the
//! Rust types that back `Array` variants. The enum is closed at five
//! variants for M7.0; widening is an explicit ADR bump (M7.1+ will
//! add `int8` / `int16` / `uint*` / `float16` / `complex*` etc.).

use crate::error::{NumpyError, NumpyErrorKind};

/// Closed enum of dtypes the M7.0 ndarray foundation supports.
///
/// Per ADR-0013 §3 the closed set is `Int32 / Int64 / Float32 /
/// Float64 / Bool`. M7.1+ may widen via a follow-up ADR; until then,
/// any unrecognised Python dtype string raises
/// `NumpyError::UnsupportedDtype`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dtype {
    /// `numpy.int32` — exact 32-bit signed integer (`i4` shorthand).
    Int32,
    /// `numpy.int64` — exact 64-bit signed integer (`i8` shorthand).
    /// M7.0 default integer dtype on 64-bit hosts (matches upstream
    /// numpy's default integer dtype).
    Int64,
    /// `numpy.float32` — single-precision IEEE 754 (`f4` shorthand).
    Float32,
    /// `numpy.float64` — double-precision IEEE 754 (`f8` shorthand).
    /// M7.0 default float dtype (matches upstream numpy).
    Float64,
    /// `numpy.bool_` — 1-byte boolean (`?` shorthand). Note: this is
    /// numpy's 1-byte form, not Rust's bit-packed `bool` of `bitvec`.
    Bool,
}

impl Dtype {
    /// Parse a Python dtype string (long form or type-char form) to a
    /// `Dtype`.
    ///
    /// # Errors
    /// Returns `NumpyError::UnsupportedDtype` for any string outside
    /// the M7.0 closed set per ADR-0013 §3.
    pub fn from_python_string(s: &str) -> Result<Self, NumpyError> {
        match s {
            "int32" | "i4" => Ok(Self::Int32),
            "int64" | "i8" => Ok(Self::Int64),
            "float32" | "f4" => Ok(Self::Float32),
            "float64" | "f8" => Ok(Self::Float64),
            "bool" | "?" => Ok(Self::Bool),
            other => Err(NumpyError {
                kind: NumpyErrorKind::UnsupportedDtype,
                message: format!(
                    "unsupported dtype string: {other:?}; M7.0 supports \
                     int32 / int64 / float32 / float64 / bool per ADR-0013"
                ),
            }),
        }
    }

    /// Map a `Dtype` back to its canonical Python long-form string.
    /// Used by `Array::repr` and `Array::to_json`.
    #[must_use]
    pub fn to_python_string(self) -> &'static str {
        match self {
            Self::Int32 => "int32",
            Self::Int64 => "int64",
            Self::Float32 => "float32",
            Self::Float64 => "float64",
            Self::Bool => "bool",
        }
    }

    /// Map a `Dtype` to its canonical Rust variant name (the same
    /// name the `Array` enum uses). Used by the `to_json` serialiser
    /// so the L0 differential gate can compare bytewise against the
    /// Python reference.
    #[must_use]
    pub fn to_rust_variant_name(self) -> &'static str {
        match self {
            Self::Int32 => "Int32",
            Self::Int64 => "Int64",
            Self::Float32 => "Float32",
            Self::Float64 => "Float64",
            Self::Bool => "Bool",
        }
    }

    /// Bytes per element for this dtype.
    #[must_use]
    pub fn item_size(self) -> usize {
        match self {
            Self::Int32 | Self::Float32 => 4,
            Self::Int64 | Self::Float64 => 8,
            Self::Bool => 1,
        }
    }
}

impl core::fmt::Display for Dtype {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.to_python_string())
    }
}
