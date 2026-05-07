// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.0 ndarray foundation per ADR-0013 + M7.1 ufunc methods per ADR-0014.
// see PROVENANCE.toml for the full manifest.

//! Tagged-union `Array` over `ndarray::ArrayD<T>` per ADR-0013 §4.
//!
//! M7.0 ships five variants matching the closed dtype tier; M7.1 adds
//! ufunc methods (binary ops + comparison + element-wise math) per
//! ADR-0014. The variant set itself is closed at M7.0 — adding `Int8`
//! etc. is an ADR-bumpable decision.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::missing_errors_doc)]

use ndarray::ArrayD;

use crate::dtype::Dtype;
use crate::error::NumpyError;
use crate::ufunc;

/// Owned N-dimensional array. Each variant wraps an
/// `ndarray::ArrayD<T>` (heap-allocated dynamic-rank tensor).
///
/// Per ADR-0013 §4 this is a tagged union: pattern-matching dispatch
/// at the public-API boundary, monomorphic `ndarray` algorithms
/// inside each arm. Views (`ArrayView` / `ArrayViewMut`) are deferred
/// to M7.2 indexing.
#[derive(Clone, Debug, PartialEq)]
pub enum Array {
    Int32(ArrayD<i32>),
    Int64(ArrayD<i64>),
    Float32(ArrayD<f32>),
    Float64(ArrayD<f64>),
    Bool(ArrayD<bool>),
}

impl Array {
    /// Dtype of this array.
    #[must_use]
    pub fn dtype(&self) -> Dtype {
        match self {
            Self::Int32(_) => Dtype::Int32,
            Self::Int64(_) => Dtype::Int64,
            Self::Float32(_) => Dtype::Float32,
            Self::Float64(_) => Dtype::Float64,
            Self::Bool(_) => Dtype::Bool,
        }
    }

    /// Shape of this array as a `Vec<usize>` matching numpy's
    /// `arr.shape` tuple.
    #[must_use]
    pub fn shape(&self) -> Vec<usize> {
        match self {
            Self::Int32(a) => a.shape().to_vec(),
            Self::Int64(a) => a.shape().to_vec(),
            Self::Float32(a) => a.shape().to_vec(),
            Self::Float64(a) => a.shape().to_vec(),
            Self::Bool(a) => a.shape().to_vec(),
        }
    }

    /// Number of axes (dimensions). `0` for a scalar; `1` for a
    /// vector; `2` for a matrix; etc.
    #[must_use]
    pub fn ndim(&self) -> usize {
        match self {
            Self::Int32(a) => a.ndim(),
            Self::Int64(a) => a.ndim(),
            Self::Float32(a) => a.ndim(),
            Self::Float64(a) => a.ndim(),
            Self::Bool(a) => a.ndim(),
        }
    }

    /// Total number of elements (product of shape dimensions).
    #[must_use]
    pub fn size(&self) -> usize {
        match self {
            Self::Int32(a) => a.len(),
            Self::Int64(a) => a.len(),
            Self::Float32(a) => a.len(),
            Self::Float64(a) => a.len(),
            Self::Bool(a) => a.len(),
        }
    }

    /// numpy-compatible `repr()` per ADR-0013 §4.
    #[must_use]
    pub fn repr(&self) -> String {
        crate::print::array_repr(self)
    }

    /// Serialise to the `{dtype, shape, data}` JSON shape that the L0
    /// differential gate also produces.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let dtype_name = self.dtype().to_rust_variant_name();
        let shape: Vec<usize> = self.shape();
        let data = match self {
            Self::Int32(a) => {
                serde_json::Value::Array(a.iter().map(|v| serde_json::json!(*v)).collect())
            }
            Self::Int64(a) => {
                serde_json::Value::Array(a.iter().map(|v| serde_json::json!(*v)).collect())
            }
            Self::Float32(a) => {
                serde_json::Value::Array(a.iter().map(|v| serde_json::json!(*v as f64)).collect())
            }
            Self::Float64(a) => {
                serde_json::Value::Array(a.iter().map(|v| serde_json::json!(*v)).collect())
            }
            Self::Bool(a) => {
                serde_json::Value::Array(a.iter().map(|v| serde_json::json!(*v)).collect())
            }
        };
        serde_json::json!({
            "dtype": dtype_name,
            "shape": shape,
            "data": data,
        })
    }

    /// Compute the flat element count for a shape vector. Helper used by
    /// constructors and the L0 harness.
    #[must_use]
    pub fn shape_size(shape: &[usize]) -> usize {
        let mut n: usize = 1;
        for &d in shape {
            n = n.saturating_mul(d);
        }
        n
    }

    // ---- M7.1 binary ops (per ADR-0014 §1) -----------------------------

    /// Element-wise add (`a + b`). Promotes per `result_type`,
    /// broadcasts per numpy rules.
    ///
    /// # Errors
    /// `NumpyError::BroadcastShapeMismatch` if shapes can't broadcast.
    pub fn add(&self, other: &Array) -> Result<Array, NumpyError> {
        ufunc::add(self, other)
    }

    /// Element-wise subtract (`a - b`).
    ///
    /// # Errors
    /// Mirrors `add`.
    pub fn sub(&self, other: &Array) -> Result<Array, NumpyError> {
        ufunc::sub(self, other)
    }

    /// Element-wise multiply (`a * b`).
    ///
    /// # Errors
    /// Mirrors `add`.
    pub fn mul(&self, other: &Array) -> Result<Array, NumpyError> {
        ufunc::mul(self, other)
    }

    /// Element-wise divide (`a / b`). Integer dtypes raise
    /// `IntegerDivisionByZero` on `b == 0`; floats follow IEEE 754.
    ///
    /// # Errors
    /// `NumpyError::IntegerDivisionByZero` (int dtypes only) or
    /// `BroadcastShapeMismatch`.
    pub fn div(&self, other: &Array) -> Result<Array, NumpyError> {
        ufunc::div(self, other)
    }

    /// Element-wise power (`a ** b`).
    ///
    /// # Errors
    /// Mirrors `add`.
    pub fn pow(&self, other: &Array) -> Result<Array, NumpyError> {
        ufunc::pow(self, other)
    }

    // ---- M7.1 comparison ops (always return Dtype::Bool) ---------------

    /// Element-wise equality (`a == b`). Always returns a `Bool`-dtype
    /// array.
    ///
    /// # Errors
    /// `NumpyError::BroadcastShapeMismatch`.
    pub fn eq_(&self, other: &Array) -> Result<Array, NumpyError> {
        ufunc::eq(self, other)
    }

    /// Element-wise inequality (`a != b`).
    ///
    /// # Errors
    /// Mirrors `eq_`.
    pub fn ne_(&self, other: &Array) -> Result<Array, NumpyError> {
        ufunc::ne(self, other)
    }

    /// Element-wise less-than (`a < b`).
    ///
    /// # Errors
    /// Mirrors `eq_`.
    pub fn lt(&self, other: &Array) -> Result<Array, NumpyError> {
        ufunc::lt(self, other)
    }

    /// Element-wise less-than-or-equal (`a <= b`).
    ///
    /// # Errors
    /// Mirrors `eq_`.
    pub fn le(&self, other: &Array) -> Result<Array, NumpyError> {
        ufunc::le(self, other)
    }

    /// Element-wise greater-than (`a > b`).
    ///
    /// # Errors
    /// Mirrors `eq_`.
    pub fn gt(&self, other: &Array) -> Result<Array, NumpyError> {
        ufunc::gt(self, other)
    }

    /// Element-wise greater-than-or-equal (`a >= b`).
    ///
    /// # Errors
    /// Mirrors `eq_`.
    pub fn ge(&self, other: &Array) -> Result<Array, NumpyError> {
        ufunc::ge(self, other)
    }

    // ---- M7.1 unary math ops -------------------------------------------

    /// Element-wise `sin`. Integer inputs promoted to `Float64`.
    ///
    /// # Errors
    /// Currently total — never errors.
    pub fn sin(&self) -> Result<Array, NumpyError> {
        ufunc::sin(self)
    }

    /// Element-wise `cos`. Integer inputs promoted to `Float64`.
    ///
    /// # Errors
    /// Currently total.
    pub fn cos(&self) -> Result<Array, NumpyError> {
        ufunc::cos(self)
    }

    /// Element-wise `exp`. Integer inputs promoted to `Float64`.
    ///
    /// # Errors
    /// Currently total.
    pub fn exp(&self) -> Result<Array, NumpyError> {
        ufunc::exp(self)
    }

    /// Element-wise `log` (natural log, base e). Integer inputs promoted
    /// to `Float64`.
    ///
    /// # Errors
    /// Currently total — `log(<= 0)` returns `NaN` / `-inf` per IEEE 754.
    pub fn log(&self) -> Result<Array, NumpyError> {
        ufunc::log(self)
    }

    /// Element-wise `sqrt`. Integer inputs promoted to `Float64`.
    ///
    /// # Errors
    /// Currently total — `sqrt(< 0)` returns `NaN` per IEEE 754.
    pub fn sqrt(&self) -> Result<Array, NumpyError> {
        ufunc::sqrt(self)
    }
}
