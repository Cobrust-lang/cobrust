// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.0 dtype tier per ADR-0013 §3 + M7.6 Complex variants per ADR-0021 §3.
// see PROVENANCE.toml for the full manifest.

//! Closed dtype enum for the M7.0 ndarray foundation (ADR-0013 §3) +
//! M7.6 Complex widening (ADR-0021 §3).
//!
//! Maps Python dtype strings (long form + type-char form) to the
//! Rust types that back `Array` variants. The enum was closed at five
//! variants for M7.0; M7.6 widens to seven by adding `Complex64` and
//! `Complex128` (per ADR-0021 §3). Further widening is an explicit
//! ADR bump (M7.7+ may add `Int8` / `UInt32` / `Float16` etc.).

use crate::error::{NumpyError, NumpyErrorKind};

/// Closed enum of dtypes the cobrust-numpy crate supports.
///
/// Per ADR-0013 §3 the M7.0 closed set was `Int32 / Int64 / Float32 /
/// Float64 / Bool`. M7.6 (per ADR-0021 §3) widens to seven by adding
/// `Complex64` and `Complex128`. Any unrecognised Python dtype string
/// raises `NumpyError::UnsupportedDtype`.
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
    /// `numpy.complex64` — `(f32, f32)` IEEE 754 real + imaginary
    /// (`c8` shorthand). Per ADR-0021 §3. Storage is
    /// `num_complex::Complex<f32>`; total size 8 bytes.
    Complex64,
    /// `numpy.complex128` — `(f64, f64)` IEEE 754 real + imaginary
    /// (`c16` shorthand). Per ADR-0021 §3. Storage is
    /// `num_complex::Complex<f64>`; total size 16 bytes.
    Complex128,
}

impl Dtype {
    /// Parse a Python dtype string (long form or type-char form) to a
    /// `Dtype`.
    ///
    /// # Errors
    /// Returns `NumpyError::UnsupportedDtype` for any string outside
    /// the seven-variant closed set per ADR-0013 §3 + ADR-0021 §3.
    pub fn from_python_string(s: &str) -> Result<Self, NumpyError> {
        match s {
            "int32" | "i4" => Ok(Self::Int32),
            "int64" | "i8" => Ok(Self::Int64),
            "float32" | "f4" => Ok(Self::Float32),
            "float64" | "f8" => Ok(Self::Float64),
            "bool" | "?" => Ok(Self::Bool),
            "complex64" | "c8" => Ok(Self::Complex64),
            "complex128" | "c16" => Ok(Self::Complex128),
            other => Err(NumpyError {
                kind: NumpyErrorKind::UnsupportedDtype,
                message: format!(
                    "unsupported dtype string: {other:?}; cobrust-numpy supports \
                     int32 / int64 / float32 / float64 / bool / complex64 / complex128 \
                     per ADR-0013 + ADR-0021"
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
            Self::Complex64 => "complex64",
            Self::Complex128 => "complex128",
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
            Self::Complex64 => "Complex64",
            Self::Complex128 => "Complex128",
        }
    }

    /// Bytes per element for this dtype.
    ///
    /// Per ADR-0021 §3: `Complex64` is 8 bytes (two `f32`),
    /// `Complex128` is 16 bytes (two `f64`).
    #[must_use]
    pub fn item_size(self) -> usize {
        match self {
            Self::Int32 | Self::Float32 => 4,
            Self::Int64 | Self::Float64 | Self::Complex64 => 8,
            Self::Bool => 1,
            Self::Complex128 => 16,
        }
    }

    /// Returns `true` when this dtype is a complex variant
    /// (`Complex64 / Complex128`). Used by ufunc/linalg routing per
    /// ADR-0021 §5 + §6 to decide between real and complex code paths.
    #[must_use]
    pub fn is_complex(self) -> bool {
        matches!(self, Self::Complex64 | Self::Complex128)
    }

    /// Returns `true` when this dtype is float-or-complex (i.e. not
    /// integer or bool). Convenience for unary-math routing.
    #[must_use]
    pub fn is_floating(self) -> bool {
        matches!(
            self,
            Self::Float32 | Self::Float64 | Self::Complex64 | Self::Complex128
        )
    }
}

impl core::fmt::Display for Dtype {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.to_python_string())
    }
}
