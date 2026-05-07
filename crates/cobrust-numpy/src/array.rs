// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.0 ndarray foundation per ADR-0013
// see PROVENANCE.toml for the full manifest.

//! Tagged-union `Array` over `ndarray::ArrayD<T>` per ADR-0013 §4.
//!
//! M7.0 ships five variants matching the closed dtype tier; later
//! sub-milestones (M7.1 ufuncs / M7.2 indexing / M7.3 reductions /
//! M7.4 linalg / M7.5 random) will add operations on top, but the
//! variant set itself is closed at M7.0 — adding `Int8` etc. is an
//! ADR-bumpable decision.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

use ndarray::ArrayD;

use crate::dtype::Dtype;

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

    /// numpy-compatible `repr()` per ADR-0013 §4 ("informationally
    /// equivalent" — see `corpus/numpy/M7.0/upstream/array_core.py
    /// ::array_repr`). Produces output of the form
    /// `array([flat_data], dtype=int32)` without numpy's
    /// column-aligned multi-line layout (the differential gate uses
    /// `to_json`, not repr text).
    #[must_use]
    pub fn repr(&self) -> String {
        crate::print::array_repr(self)
    }

    /// Serialise to the `{dtype, shape, data}` JSON shape that the L0
    /// differential gate (`corpus/numpy/M7.0/harness/h_array.py`)
    /// also produces. Bytewise comparison of these payloads is the
    /// M7.0 behavior gate per ADR-0013 §5.
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

    /// Compute the flat element count for a shape vector.
    /// Helper used by constructors; exposed for the well-typed test
    /// suite and the L0 harness.
    ///
    /// # Errors
    /// Returns `NumpyError::NegativeDimension` if any dim is negative
    /// (the input type is `usize` so this is enforced by the type
    /// system; the `i64`-input variant lives on the constructors).
    #[must_use]
    pub fn shape_size(shape: &[usize]) -> usize {
        let mut n: usize = 1;
        for &d in shape {
            n = n.saturating_mul(d);
        }
        n
    }
}
