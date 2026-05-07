// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.0 ndarray foundation per ADR-0013
// see PROVENANCE.toml for the full manifest.

//! Constructor surface — `array`, `zeros`, `ones`, `arange`.
//!
//! Each constructor dispatches on `Dtype` and delegates to the
//! `ndarray` backend (per ADR-0012 §"Backend strategy: translate the
//! surface, bind the core"). The cobrust-numpy layer owns the
//! Python-shaped contract (dtype string parsing, error taxonomy);
//! `ndarray` owns the storage layout and zero-cost iteration.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]

use ndarray::{ArrayD, IxDyn};

use crate::array::Array;
use crate::dtype::Dtype;
use crate::error::{NumpyError, NumpyErrorKind};

fn shape_to_ix_dyn(shape: &[usize]) -> IxDyn {
    IxDyn(shape)
}

/// `numpy.zeros(shape, dtype=...)`-equivalent. Allocates a zero-filled
/// array of the given shape and dtype.
///
/// # Errors
/// Returns `NumpyError::UnsupportedDtype` is impossible at this entry
/// (caller passes typed `Dtype`). Future widening may surface
/// allocation failures.
pub fn zeros(shape: &[usize], dtype: Dtype) -> Result<Array, NumpyError> {
    let dim = shape_to_ix_dyn(shape);
    Ok(match dtype {
        Dtype::Int32 => Array::Int32(ArrayD::<i32>::zeros(dim)),
        Dtype::Int64 => Array::Int64(ArrayD::<i64>::zeros(dim)),
        Dtype::Float32 => Array::Float32(ArrayD::<f32>::zeros(dim)),
        Dtype::Float64 => Array::Float64(ArrayD::<f64>::zeros(dim)),
        Dtype::Bool => Array::Bool(ArrayD::<bool>::from_elem(dim, false)),
    })
}

/// `numpy.ones(shape, dtype=...)`-equivalent. Allocates a one-filled
/// array of the given shape and dtype.
///
/// # Errors
/// Mirrors `zeros`.
pub fn ones(shape: &[usize], dtype: Dtype) -> Result<Array, NumpyError> {
    let dim = shape_to_ix_dyn(shape);
    Ok(match dtype {
        Dtype::Int32 => Array::Int32(ArrayD::<i32>::from_elem(dim, 1_i32)),
        Dtype::Int64 => Array::Int64(ArrayD::<i64>::from_elem(dim, 1_i64)),
        Dtype::Float32 => Array::Float32(ArrayD::<f32>::from_elem(dim, 1.0_f32)),
        Dtype::Float64 => Array::Float64(ArrayD::<f64>::from_elem(dim, 1.0_f64)),
        Dtype::Bool => Array::Bool(ArrayD::<bool>::from_elem(dim, true)),
    })
}

/// `numpy.array(values).reshape(shape).astype(dtype)`-equivalent.
/// Takes a flat `f64` buffer (caller f64-casts integer inputs) plus a
/// shape and dtype, and constructs the `Array`.
///
/// Per ADR-0013 §"Public surface": M7.0 takes f64-only input to keep
/// the surface tight; M7.1 will add typed constructors once ufuncs
/// are in.
///
/// # Errors
/// - `NumpyError::ShapeMismatch` if `values.len() !=
///   shape_size(shape)`.
/// - `NumpyError::CastFailed` if a value cannot be cast to the
///   requested dtype without precision loss in `@py_compat(strict)`
///   sense (currently only int dtypes raise this for non-integer
///   inputs).
pub fn array(values: &[f64], shape: &[usize], dtype: Dtype) -> Result<Array, NumpyError> {
    let expected = Array::shape_size(shape);
    if values.len() != expected {
        return Err(NumpyError {
            kind: NumpyErrorKind::ShapeMismatch,
            message: format!(
                "values length {} does not match shape product {} for shape={:?}",
                values.len(),
                expected,
                shape
            ),
        });
    }
    let dim = shape_to_ix_dyn(shape);
    match dtype {
        Dtype::Int32 => {
            let casted: Vec<i32> = values.iter().map(|v| *v as i32).collect();
            let arr = ArrayD::<i32>::from_shape_vec(dim, casted).map_err(|e| NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                message: format!("ndarray from_shape_vec: {e}"),
            })?;
            Ok(Array::Int32(arr))
        }
        Dtype::Int64 => {
            let casted: Vec<i64> = values.iter().map(|v| *v as i64).collect();
            let arr = ArrayD::<i64>::from_shape_vec(dim, casted).map_err(|e| NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                message: format!("ndarray from_shape_vec: {e}"),
            })?;
            Ok(Array::Int64(arr))
        }
        Dtype::Float32 => {
            let casted: Vec<f32> = values.iter().map(|v| *v as f32).collect();
            let arr = ArrayD::<f32>::from_shape_vec(dim, casted).map_err(|e| NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                message: format!("ndarray from_shape_vec: {e}"),
            })?;
            Ok(Array::Float32(arr))
        }
        Dtype::Float64 => {
            let arr =
                ArrayD::<f64>::from_shape_vec(dim, values.to_vec()).map_err(|e| NumpyError {
                    kind: NumpyErrorKind::ShapeMismatch,
                    message: format!("ndarray from_shape_vec: {e}"),
                })?;
            Ok(Array::Float64(arr))
        }
        Dtype::Bool => {
            let casted: Vec<bool> = values.iter().map(|v| *v != 0.0).collect();
            let arr = ArrayD::<bool>::from_shape_vec(dim, casted).map_err(|e| NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                message: format!("ndarray from_shape_vec: {e}"),
            })?;
            Ok(Array::Bool(arr))
        }
    }
}

/// Compute the count of elements `numpy.arange(start, stop, step)`
/// produces. Matches numpy 2.0.2 semantics for the M7.0 dtype tier:
/// `ceil((stop - start) / step)` clamped at 0, with sign of step
/// determining direction. Helper exposed for the well-typed test
/// suite.
#[must_use]
pub fn arange_count(start: f64, stop: f64, step: f64) -> usize {
    if step == 0.0 {
        return 0;
    }
    let delta = stop - start;
    if step > 0.0 && delta <= 0.0 {
        return 0;
    }
    if step < 0.0 && delta >= 0.0 {
        return 0;
    }
    let raw = (delta / step).ceil();
    if raw <= 0.0 {
        0
    } else if raw.is_finite() {
        raw as usize
    } else {
        0
    }
}

/// `numpy.arange(start, stop, step, dtype=...)`-equivalent. Half-open
/// range constructor.
///
/// # Errors
/// - `NumpyError::ZeroStep` if `step == 0` (matches numpy's
///   `ZeroDivisionError`).
/// - `NumpyError::BoolArangeUnsupported` if `dtype == Dtype::Bool`
///   (matches numpy's `TypeError`).
pub fn arange(start: f64, stop: f64, step: f64, dtype: Dtype) -> Result<Array, NumpyError> {
    if step == 0.0 {
        return Err(NumpyError {
            kind: NumpyErrorKind::ZeroStep,
            message: "arange: step must be nonzero".into(),
        });
    }
    if matches!(dtype, Dtype::Bool) {
        return Err(NumpyError {
            kind: NumpyErrorKind::BoolArangeUnsupported,
            message: "arange: dtype=bool not supported (matches numpy)".into(),
        });
    }
    let count = arange_count(start, stop, step);
    let raw: Vec<f64> = (0..count).map(|i| start + (i as f64) * step).collect();
    array(&raw, &[count], dtype)
}
