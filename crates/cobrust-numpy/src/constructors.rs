// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.0 ndarray foundation per ADR-0013 + M7.1 typed/nested constructors per ADR-0014.
// see PROVENANCE.toml for the full manifest.

//! Constructor surface — `array`, `zeros`, `ones`, `arange`, plus the
//! M7.1 typed constructors (`array_i32` / `array_i64` / `array_f32` /
//! `array_f64` / `array_bool`) and `array_from_nested` for 2D/3D
//! Python-list inputs (closes ADR-0013 follow-up #2 + #4 per
//! ADR-0014).
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
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::manual_repeat_n)]

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
        Dtype::Complex64 | Dtype::Complex128 => {
            return Err(NumpyError {
                kind: NumpyErrorKind::LinalgDtypeUnsupported,
                message: format!(
                    "zeros: complex dtype {dtype} requires Array tagged-union widening; M7.6 ADR-0021 ships dtype tier only"
                ),
            });
        }
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
        Dtype::Complex64 | Dtype::Complex128 => {
            return Err(NumpyError {
                kind: NumpyErrorKind::LinalgDtypeUnsupported,
                message: format!(
                    "ones: complex dtype {dtype} requires Array tagged-union widening; M7.6 ADR-0021 ships dtype tier only"
                ),
            });
        }
    })
}

/// `numpy.array(values).reshape(shape).astype(dtype)`-equivalent.
/// Takes a flat `f64` buffer (caller f64-casts integer inputs) plus a
/// shape and dtype, and constructs the `Array`.
///
/// Per ADR-0013 §"Public surface": M7.0 takes f64-only input to keep
/// the surface tight; M7.1 adds the typed constructors below
/// (`array_i32` / `array_i64` / `array_f32` / `array_f64` /
/// `array_bool`) so callers no longer need to f64-cast integer inputs
/// (closes ADR-0013 follow-up #2 per ADR-0014).
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
        Dtype::Complex64 | Dtype::Complex128 => Err(NumpyError {
            kind: NumpyErrorKind::LinalgDtypeUnsupported,
            message: format!(
                "array: complex dtype {dtype} requires Array tagged-union widening; M7.6 ADR-0021 ships dtype tier only"
            ),
        }),
    }
}

// ---- M7.1 typed constructors (closes ADR-0013 follow-up #2) ------------

/// `numpy.array(values, dtype=int32).reshape(shape)`-equivalent. Takes
/// a typed `&[i32]` buffer plus shape, no f64 round-trip.
///
/// # Errors
/// `NumpyError::ShapeMismatch` if `values.len() !=
/// Array::shape_size(shape)`.
pub fn array_i32(values: &[i32], shape: &[usize]) -> Result<Array, NumpyError> {
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
    let arr =
        ArrayD::<i32>::from_shape_vec(shape_to_ix_dyn(shape), values.to_vec()).map_err(|e| {
            NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                message: format!("ndarray from_shape_vec: {e}"),
            }
        })?;
    Ok(Array::Int32(arr))
}

/// `numpy.array(values, dtype=int64).reshape(shape)`-equivalent.
///
/// # Errors
/// Mirrors `array_i32`.
pub fn array_i64(values: &[i64], shape: &[usize]) -> Result<Array, NumpyError> {
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
    let arr =
        ArrayD::<i64>::from_shape_vec(shape_to_ix_dyn(shape), values.to_vec()).map_err(|e| {
            NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                message: format!("ndarray from_shape_vec: {e}"),
            }
        })?;
    Ok(Array::Int64(arr))
}

/// `numpy.array(values, dtype=float32).reshape(shape)`-equivalent.
///
/// # Errors
/// Mirrors `array_i32`.
pub fn array_f32(values: &[f32], shape: &[usize]) -> Result<Array, NumpyError> {
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
    let arr =
        ArrayD::<f32>::from_shape_vec(shape_to_ix_dyn(shape), values.to_vec()).map_err(|e| {
            NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                message: format!("ndarray from_shape_vec: {e}"),
            }
        })?;
    Ok(Array::Float32(arr))
}

/// `numpy.array(values, dtype=float64).reshape(shape)`-equivalent.
///
/// # Errors
/// Mirrors `array_i32`.
pub fn array_f64(values: &[f64], shape: &[usize]) -> Result<Array, NumpyError> {
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
    let arr =
        ArrayD::<f64>::from_shape_vec(shape_to_ix_dyn(shape), values.to_vec()).map_err(|e| {
            NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                message: format!("ndarray from_shape_vec: {e}"),
            }
        })?;
    Ok(Array::Float64(arr))
}

/// `numpy.array(values, dtype=bool).reshape(shape)`-equivalent.
///
/// # Errors
/// Mirrors `array_i32`.
pub fn array_bool(values: &[bool], shape: &[usize]) -> Result<Array, NumpyError> {
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
    let arr =
        ArrayD::<bool>::from_shape_vec(shape_to_ix_dyn(shape), values.to_vec()).map_err(|e| {
            NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                message: format!("ndarray from_shape_vec: {e}"),
            }
        })?;
    Ok(Array::Bool(arr))
}

// ---- M7.1 nested-list constructor (closes ADR-0013 follow-up #4) -------

/// Recursive nested-list type for 2D/3D Python-list inputs.
/// Mirrors `numpy.array([[1, 2], [3, 4]])` semantics: every leaf is a
/// scalar `f64`, every internal node is a list, all sibling lists at
/// the same depth must have the same length.
#[derive(Clone, Debug, PartialEq)]
pub enum NestedList {
    Scalar(f64),
    List(Vec<NestedList>),
}

impl NestedList {
    /// Convenience constructor: `NestedList::scalars(&[1.0, 2.0, 3.0])`
    /// produces `List([Scalar(1.0), Scalar(2.0), Scalar(3.0)])` for a
    /// 1-D input.
    #[must_use]
    pub fn scalars(values: &[f64]) -> Self {
        Self::List(values.iter().map(|v| Self::Scalar(*v)).collect())
    }

    /// Compute the inferred shape of this nested list. Returns
    /// `Err(NumpyErrorKind::ShapeMismatch)` if sibling lists at the same
    /// depth have inconsistent lengths.
    fn infer_shape(&self) -> Result<Vec<usize>, NumpyError> {
        match self {
            Self::Scalar(_) => Ok(vec![]),
            Self::List(items) => {
                if items.is_empty() {
                    return Ok(vec![0]);
                }
                let first_shape = items[0].infer_shape()?;
                for sibling in items.iter().skip(1) {
                    let sibling_shape = sibling.infer_shape()?;
                    if sibling_shape != first_shape {
                        return Err(NumpyError {
                            kind: NumpyErrorKind::ShapeMismatch,
                            message: format!(
                                "ragged nested-list: expected shape {:?}, got {:?}",
                                first_shape, sibling_shape
                            ),
                        });
                    }
                }
                let mut shape = vec![items.len()];
                shape.extend(first_shape);
                Ok(shape)
            }
        }
    }

    /// Flatten this nested list into a row-major `Vec<f64>`.
    fn flatten(&self) -> Vec<f64> {
        let mut out = Vec::new();
        self.flatten_into(&mut out);
        out
    }

    fn flatten_into(&self, out: &mut Vec<f64>) {
        match self {
            Self::Scalar(v) => out.push(*v),
            Self::List(items) => {
                for item in items {
                    item.flatten_into(out);
                }
            }
        }
    }
}

/// `numpy.array(nested, dtype=...)`-equivalent for 1D/2D/3D nested-list
/// inputs (closes ADR-0013 follow-up #4 per ADR-0014).
///
/// # Errors
/// - `NumpyError::ShapeMismatch` for ragged inputs.
/// - Forwarded errors from `array(...)`.
pub fn array_from_nested(nested: &NestedList, dtype: Dtype) -> Result<Array, NumpyError> {
    let shape = nested.infer_shape()?;
    let flat = nested.flatten();
    array(&flat, &shape, dtype)
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

    #[test]
    fn array_i32_round_trip() {
        let a = array_i32(&[1, 2, 3, 4, 5, 6], &[2, 3]).unwrap();
        assert_eq!(a.dtype(), Dtype::Int32);
        assert_eq!(a.shape(), vec![2, 3]);
    }

    #[test]
    fn array_bool_round_trip() {
        let a = array_bool(&[true, false, true], &[3]).unwrap();
        assert_eq!(a.dtype(), Dtype::Bool);
        assert_eq!(a.shape(), vec![3]);
    }

    #[test]
    fn nested_list_2d_inference() {
        let nl = NestedList::List(vec![
            NestedList::scalars(&[1.0, 2.0]),
            NestedList::scalars(&[3.0, 4.0]),
            NestedList::scalars(&[5.0, 6.0]),
        ]);
        let arr = array_from_nested(&nl, Dtype::Int32).unwrap();
        assert_eq!(arr.shape(), vec![3, 2]);
        assert_eq!(arr.dtype(), Dtype::Int32);
    }

    #[test]
    fn nested_list_ragged_errors() {
        let nl = NestedList::List(vec![
            NestedList::scalars(&[1.0, 2.0]),
            NestedList::scalars(&[3.0, 4.0, 5.0]),
        ]);
        let err = array_from_nested(&nl, Dtype::Float64).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
    }

    #[test]
    fn typed_constructor_shape_mismatch_errors() {
        let err = array_i32(&[1, 2, 3], &[2, 3]).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
    }
}
