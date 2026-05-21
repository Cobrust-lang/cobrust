// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.2 indexing per ADR-0015 §2 (view ownership model).
// see PROVENANCE.toml for the full manifest.

//! Array views — `ArrayView<'a>` and `ArrayViewMut<'a>` per ADR-0015 §2.
//!
//! Per constitution §2.2 (no `dyn`): closed enums (5 variants each, one
//! per dtype) wrapping `ndarray::ArrayViewD<'a, T>` /
//! `ndarray::ArrayViewMutD<'a, T>`. Per ADR-0015 §2: lifetime parameter
//! `'a` ties the view to the parent `Array`'s borrow; the Rust borrow
//! checker enforces that while a `ArrayViewMut<'a>` is alive, no other
//! reference to the parent is allowed.
//!
//! Views are produced by basic slicing (`Array::slice`) and single-int
//! indexing (`Array::index_axis`). Advanced indexing (integer-array,
//! boolean mask, `np.where`) returns owned `Array` per the view-vs-copy
//! contract documented in ADR-0015 §3.

// CQ P1-4: consolidated from 9 separate inner attrs; translator-template fix deferred per F37.
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::module_name_repetitions
)]

use ndarray::{ArrayViewD, ArrayViewMutD};

use crate::array::Array;
use crate::dtype::Dtype;

/// Immutable view over an `Array`'s storage. Lifetime `'a` is bound
/// to the parent `Array`'s borrow per ADR-0015 §2.
///
/// Five variants matching the closed dtype tier — constitution §2.2
/// (no `dyn`) is satisfied: dispatch is by enum match, not trait
/// object.
#[derive(Debug)]
pub enum ArrayView<'a> {
    Int32(ArrayViewD<'a, i32>),
    Int64(ArrayViewD<'a, i64>),
    Float32(ArrayViewD<'a, f32>),
    Float64(ArrayViewD<'a, f64>),
    Bool(ArrayViewD<'a, bool>),
}

impl ArrayView<'_> {
    /// Dtype of this view.
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

    /// Shape of this view as a `Vec<usize>`.
    #[must_use]
    pub fn shape(&self) -> Vec<usize> {
        match self {
            Self::Int32(v) => v.shape().to_vec(),
            Self::Int64(v) => v.shape().to_vec(),
            Self::Float32(v) => v.shape().to_vec(),
            Self::Float64(v) => v.shape().to_vec(),
            Self::Bool(v) => v.shape().to_vec(),
        }
    }

    /// Number of axes.
    #[must_use]
    pub fn ndim(&self) -> usize {
        match self {
            Self::Int32(v) => v.ndim(),
            Self::Int64(v) => v.ndim(),
            Self::Float32(v) => v.ndim(),
            Self::Float64(v) => v.ndim(),
            Self::Bool(v) => v.ndim(),
        }
    }

    /// Total element count (product of shape dimensions).
    #[must_use]
    pub fn size(&self) -> usize {
        match self {
            Self::Int32(v) => v.len(),
            Self::Int64(v) => v.len(),
            Self::Float32(v) => v.len(),
            Self::Float64(v) => v.len(),
            Self::Bool(v) => v.len(),
        }
    }

    /// Materialise this view into an owned `Array` (copies the
    /// elements). Used by `np_where` and friends to convert a view to
    /// the owned representation.
    #[must_use]
    pub fn to_owned(&self) -> Array {
        match self {
            Self::Int32(v) => Array::Int32(v.to_owned()),
            Self::Int64(v) => Array::Int64(v.to_owned()),
            Self::Float32(v) => Array::Float32(v.to_owned()),
            Self::Float64(v) => Array::Float64(v.to_owned()),
            Self::Bool(v) => Array::Bool(v.to_owned()),
        }
    }
}

/// Mutable view over an `Array`'s storage. Lifetime `'a` is bound to
/// the parent `Array`'s exclusive borrow per ADR-0015 §2.
///
/// Five variants matching the closed dtype tier. While an
/// `ArrayViewMut<'a>` is alive, the Rust borrow checker prohibits any
/// other reference to the parent (mut or const).
#[derive(Debug)]
pub enum ArrayViewMut<'a> {
    Int32(ArrayViewMutD<'a, i32>),
    Int64(ArrayViewMutD<'a, i64>),
    Float32(ArrayViewMutD<'a, f32>),
    Float64(ArrayViewMutD<'a, f64>),
    Bool(ArrayViewMutD<'a, bool>),
}

impl ArrayViewMut<'_> {
    /// Dtype of this view.
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

    /// Shape of this view.
    #[must_use]
    pub fn shape(&self) -> Vec<usize> {
        match self {
            Self::Int32(v) => v.shape().to_vec(),
            Self::Int64(v) => v.shape().to_vec(),
            Self::Float32(v) => v.shape().to_vec(),
            Self::Float64(v) => v.shape().to_vec(),
            Self::Bool(v) => v.shape().to_vec(),
        }
    }

    /// Number of axes.
    #[must_use]
    pub fn ndim(&self) -> usize {
        match self {
            Self::Int32(v) => v.ndim(),
            Self::Int64(v) => v.ndim(),
            Self::Float32(v) => v.ndim(),
            Self::Float64(v) => v.ndim(),
            Self::Bool(v) => v.ndim(),
        }
    }

    /// Total element count.
    #[must_use]
    pub fn size(&self) -> usize {
        match self {
            Self::Int32(v) => v.len(),
            Self::Int64(v) => v.len(),
            Self::Float32(v) => v.len(),
            Self::Float64(v) => v.len(),
            Self::Bool(v) => v.len(),
        }
    }

    /// Fill every element of the view with the f64 value `v`, casting
    /// to the view's dtype. Used by tests to demonstrate
    /// mutate-through-view semantics (per ADR-0015 §"View ownership
    /// model"); a basic-slice view aliases the parent's storage, so
    /// mutation through the view is observable on the parent.
    pub fn fill_f64(&mut self, v: f64) {
        match self {
            Self::Int32(view) => view.fill(v as i32),
            Self::Int64(view) => view.fill(v as i64),
            Self::Float32(view) => view.fill(v as f32),
            Self::Float64(view) => view.fill(v),
            Self::Bool(view) => view.fill(v != 0.0),
        }
    }

    /// Materialise this mut view into an owned `Array` (copies the
    /// elements). Useful when the caller wants to take ownership
    /// without holding the mut borrow.
    #[must_use]
    pub fn to_owned(&self) -> Array {
        match self {
            Self::Int32(v) => Array::Int32(v.to_owned()),
            Self::Int64(v) => Array::Int64(v.to_owned()),
            Self::Float32(v) => Array::Float32(v.to_owned()),
            Self::Float64(v) => Array::Float64(v.to_owned()),
            Self::Bool(v) => Array::Bool(v.to_owned()),
        }
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
    #![allow(clippy::similar_names)]
    use crate::index::SliceSpec;
    use crate::{array_f64, array_i32};

    #[test]
    fn view_dtype_observable() {
        let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
        let v = a.slice(SliceSpec::range(1, 4)).unwrap();
        assert_eq!(v.dtype(), crate::Dtype::Int32);
        assert_eq!(v.shape(), vec![3]);
        assert_eq!(v.size(), 3);
        assert_eq!(v.ndim(), 1);
    }

    #[test]
    fn view_to_owned_round_trip() {
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
        let v = a.slice(SliceSpec::range(1, 3)).unwrap();
        let owned = v.to_owned();
        assert_eq!(owned.shape(), vec![2]);
    }

    #[test]
    fn mut_view_fill_propagates_to_parent() {
        let mut a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0], &[5]).unwrap();
        {
            let mut v = a.slice_mut(SliceSpec::range(1, 4)).unwrap();
            v.fill_f64(99.0);
        }
        let crate::Array::Float64(arr) = &a else {
            panic!("expected Float64");
        };
        assert_eq!(arr.as_slice().unwrap(), &[1.0, 99.0, 99.0, 99.0, 5.0]);
    }
}
