// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.0 ndarray foundation per ADR-0013 + M7.1 ufunc methods per ADR-0014
//   + M7.2 indexing per ADR-0015 + M7.3 reductions per ADR-0016 + M7.4 linalg per ADR-0017.
// see PROVENANCE.toml for the full manifest.

//! Tagged-union `Array` over `ndarray::ArrayD<T>` per ADR-0013 §4.
//!
//! M7.0 ships five variants matching the closed dtype tier; M7.1 adds
//! ufunc methods (binary ops + comparison + element-wise math) per
//! ADR-0014. The variant set itself is closed at M7.0 — adding `Int8`
//! etc. is an ADR-bumpable decision.

// CQ P1-4: consolidated from 5 separate inner attrs; translator-template fix deferred per F37.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::missing_errors_doc
)]

use ndarray::{ArrayD, ArrayViewD, ArrayViewMutD};

use crate::dtype::Dtype;
use crate::error::{NumpyError, NumpyErrorKind};
use crate::index::{self, Index, SliceSpec};
use crate::linalg;
use crate::reduce;
use crate::ufunc;
use crate::view::{ArrayView, ArrayViewMut};

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

    // ---- M7.2 indexing surface (per ADR-0015) --------------------------

    /// Basic slicing on the first axis (`a[start:stop:step]`). Returns a
    /// **view** per ADR-0015 §3 — does not copy.
    ///
    /// # Errors
    /// - `NumpyError::IndexError` if `self` is 0-d.
    /// - `NumpyError::ZeroStep` if `spec.step == Some(0)`.
    pub fn slice(&self, spec: SliceSpec) -> Result<ArrayView<'_>, NumpyError> {
        index::slice_view(self, spec)
    }

    /// Mutable basic-slice view. Mutating through the view is
    /// observable on the parent (per ADR-0015 §"View ownership model").
    ///
    /// # Errors
    /// Mirrors `slice`.
    pub fn slice_mut(&mut self, spec: SliceSpec) -> Result<ArrayViewMut<'_>, NumpyError> {
        if self.ndim() == 0 {
            return Err(NumpyError {
                kind: NumpyErrorKind::IndexError,
                message: "cannot slice a 0-d array".into(),
            });
        }
        let length = self.shape()[0] as i64;
        let (begin, end, step) = index::resolve_slice(spec.start, spec.stop, spec.step, length)?;
        let nd_slice = index::to_nd_slice_pub(begin, end, step, length);
        Ok(match self {
            Self::Int32(a) => {
                let v: ArrayViewMutD<'_, i32> = a.slice_axis_mut(ndarray::Axis(0), nd_slice);
                ArrayViewMut::Int32(v)
            }
            Self::Int64(a) => {
                let v: ArrayViewMutD<'_, i64> = a.slice_axis_mut(ndarray::Axis(0), nd_slice);
                ArrayViewMut::Int64(v)
            }
            Self::Float32(a) => {
                let v: ArrayViewMutD<'_, f32> = a.slice_axis_mut(ndarray::Axis(0), nd_slice);
                ArrayViewMut::Float32(v)
            }
            Self::Float64(a) => {
                let v: ArrayViewMutD<'_, f64> = a.slice_axis_mut(ndarray::Axis(0), nd_slice);
                ArrayViewMut::Float64(v)
            }
            Self::Bool(a) => {
                let v: ArrayViewMutD<'_, bool> = a.slice_axis_mut(ndarray::Axis(0), nd_slice);
                ArrayViewMut::Bool(v)
            }
        })
    }

    /// Single-int indexing on the first axis (`a[i]`). Returns a
    /// **view** with one fewer axis per ADR-0015 §3.
    ///
    /// # Errors
    /// - `NumpyError::IndexError` if `self` is 0-d.
    /// - `NumpyError::OutOfBoundsIndex` if `i` is outside `[-len, len)`.
    pub fn index_single(&self, i: i64) -> Result<ArrayView<'_>, NumpyError> {
        index::single_view(self, i)
    }

    /// Integer-array indexing on the first axis (`a[[i0, i1, ...]]`).
    /// Always returns a **copy** per ADR-0015 §3.
    ///
    /// # Errors
    /// - `NumpyError::IndexError` if `self` is 0-d.
    /// - `NumpyError::OutOfBoundsIndex` if any `i` is outside `[-len,
    ///   len)`.
    pub fn take(&self, indices: &[i64]) -> Result<Array, NumpyError> {
        index::take_impl(self, indices)
    }

    /// Boolean-mask indexing (`a[mask]`). Returns a 1-D **copy** per
    /// ADR-0015 §3.
    ///
    /// # Errors
    /// - `NumpyError::IndexDtypeNotInteger` if `mask.dtype() !=
    ///   Dtype::Bool` (the "not integer" name is shared with int-array
    ///   dtype validation; the mask-dtype check pre-empts it).
    /// - `NumpyError::BoolMaskShapeMismatch` if `mask.shape() !=
    ///   self.shape()`.
    pub fn mask(&self, mask: &Array) -> Result<Array, NumpyError> {
        index::mask_impl(self, mask)
    }

    /// Multi-axis indexing (`a[i, :, [0, 2, 5]]` — but M7.2 only
    /// supports per-axis chains; the result is always materialised).
    /// Per ADR-0015 §1.
    ///
    /// # Errors
    /// Forwarded from the per-axis dispatch.
    pub fn index_get(&self, indices: &[Index]) -> Result<Array, NumpyError> {
        index::index_get(self, indices)
    }

    /// Convenience for `np.where(self, x, y)` — element-wise selection
    /// using `self` as the condition mask. Per ADR-0015 §"Public surface".
    ///
    /// # Errors
    /// Forwarded from `np_where`.
    pub fn where_(&self, x: &Array, y: &Array) -> Result<Array, NumpyError> {
        index::np_where(self, x, y)
    }

    // ---- M7.3 reductions (per ADR-0016) ---------------------------------

    /// Sum over `axis` (or all axes when `axis=None`). Pairwise
    /// summation for floats per ADR-0016 §3; integer reductions wrap.
    /// Bool inputs return `Int64` count of `true`.
    ///
    /// # Errors
    /// `NumpyError::IndexError` if `axis` is out of bounds.
    pub fn sum(&self, axis: Option<i64>) -> Result<Array, NumpyError> {
        reduce::sum(self, axis)
    }

    /// Product over `axis`. Multiplicative identity 1 for empty arrays.
    ///
    /// # Errors
    /// Mirrors `sum`.
    pub fn prod(&self, axis: Option<i64>) -> Result<Array, NumpyError> {
        reduce::prod(self, axis)
    }

    /// Arithmetic mean over `axis`. Empty-array → NaN per numpy. Float
    /// dtypes preserve width (`f32` → `f32`, `f64` → `f64`); int / bool
    /// promote to `Float64`.
    ///
    /// # Errors
    /// Mirrors `sum`.
    pub fn mean(&self, axis: Option<i64>) -> Result<Array, NumpyError> {
        reduce::mean(self, axis)
    }

    /// Standard deviation over `axis`. `ddof` is the divisor offset:
    /// `denom = N - ddof`; `denom <= 0` → NaN. Default behavior:
    /// `ddof=0` (population). Use `ddof=1` for sample (Bessel).
    ///
    /// # Errors
    /// Mirrors `sum`.
    pub fn std(&self, axis: Option<i64>, ddof: u32) -> Result<Array, NumpyError> {
        reduce::std(self, axis, ddof)
    }

    /// Variance over `axis`. See `std` for `ddof` semantics.
    ///
    /// # Errors
    /// Mirrors `sum`.
    pub fn var(&self, axis: Option<i64>, ddof: u32) -> Result<Array, NumpyError> {
        reduce::var(self, axis, ddof)
    }

    /// Minimum over `axis`. NaN propagates per IEEE 754 (any NaN in the
    /// reduction lane → NaN result). Empty-array →
    /// `NumpyError::ReductionEmptyArray`.
    ///
    /// # Errors
    /// `NumpyError::ReductionEmptyArray` if the array (or reduced lane)
    /// is empty; `NumpyError::IndexError` if `axis` is out of bounds.
    pub fn min(&self, axis: Option<i64>) -> Result<Array, NumpyError> {
        reduce::min(self, axis)
    }

    /// Maximum over `axis`. Mirrors `min`.
    ///
    /// # Errors
    /// Mirrors `min`.
    pub fn max(&self, axis: Option<i64>) -> Result<Array, NumpyError> {
        reduce::max(self, axis)
    }

    /// Index of minimum over `axis`. First-occurrence tie-breaking per
    /// numpy. Result is `Int64` (matches numpy's `intp` on 64-bit hosts).
    /// NaN inputs return the index of the first NaN encountered.
    ///
    /// # Errors
    /// Mirrors `min`.
    pub fn argmin(&self, axis: Option<i64>) -> Result<Array, NumpyError> {
        reduce::argmin(self, axis)
    }

    /// Index of maximum over `axis`. Mirrors `argmin` semantics.
    ///
    /// # Errors
    /// Mirrors `min`.
    pub fn argmax(&self, axis: Option<i64>) -> Result<Array, NumpyError> {
        reduce::argmax(self, axis)
    }

    // ---- M7.4 linalg surface (per ADR-0017) -----------------------------

    /// Matrix multiplication. Defers to `linalg::matmul` per ADR-0017.
    /// Float-only; mixed-dtype promotes to `Float64`.
    ///
    /// # Errors
    /// `NumpyError::LinalgShapeError` on shape mismatch;
    /// `NumpyError::LinalgDtypeUnsupported` on int / bool inputs.
    pub fn matmul(&self, other: &Array) -> Result<Array, NumpyError> {
        linalg::matmul(self, other)
    }

    /// Dot product. 1-D × 1-D → scalar; 2-D × 2-D → matmul. Per
    /// ADR-0017 §1, M7.4 defers `dot` to `matmul`.
    ///
    /// # Errors
    /// Mirrors `matmul`.
    pub fn dot(&self, other: &Array) -> Result<Array, NumpyError> {
        linalg::dot(self, other)
    }

    /// Borrow this Array as an `ArrayView<'_>`. Useful for callers that
    /// want a view-shaped representation without re-deriving via slice.
    #[must_use]
    pub fn as_view(&self) -> ArrayView<'_> {
        match self {
            Self::Int32(a) => ArrayView::Int32(<ArrayViewD<'_, i32>>::from(a)),
            Self::Int64(a) => ArrayView::Int64(<ArrayViewD<'_, i64>>::from(a)),
            Self::Float32(a) => ArrayView::Float32(<ArrayViewD<'_, f32>>::from(a)),
            Self::Float64(a) => ArrayView::Float64(<ArrayViewD<'_, f64>>::from(a)),
            Self::Bool(a) => ArrayView::Bool(<ArrayViewD<'_, bool>>::from(a)),
        }
    }
}
