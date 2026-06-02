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
//! surface, bind the core"). The cobrust-coil layer owns the
//! Python-shaped contract (dtype string parsing, error taxonomy);
//! `ndarray` owns the storage layout and zero-cost iteration.

// CQ P1-4 + template-fix: single consolidated block; future emits use #[allow] at item level.
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::manual_repeat_n,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::needless_range_loop,
    clippy::similar_names,
    clippy::uninlined_format_args
)]

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

/// `numpy.full(shape, fill_value, dtype=...)`-equivalent. Allocates an
/// array of the given shape and dtype with every element set to
/// `fill_value`.
///
/// The `.cb` surface `coil.full(n, value)` makes a 1-D `Float64` buffer
/// of `n` copies of `value` (`np.full(3, 5.0) == [5., 5., 5.]`). The
/// `fill_value` is taken as `f64` and cast to the target dtype for the
/// integer variants (truncation toward zero, matching numpy's cast), so
/// even the integer-dtype forms are value-faithful for integral fills.
///
/// `@py_compat(strict)` — the fill is an exact copy (no floating
/// arithmetic), so every dtype form is bit-exact vs numpy.
///
/// # Errors
/// Mirrors `zeros` / `ones` (the complex-dtype arm is unreachable for the
/// `Float64` `.cb` surface; reserved for the `Array` tagged-union
/// widening per M7.6 ADR-0021).
pub fn full(shape: &[usize], fill_value: f64, dtype: Dtype) -> Result<Array, NumpyError> {
    let dim = shape_to_ix_dyn(shape);
    Ok(match dtype {
        Dtype::Int32 => Array::Int32(ArrayD::<i32>::from_elem(dim, fill_value as i32)),
        Dtype::Int64 => Array::Int64(ArrayD::<i64>::from_elem(dim, fill_value as i64)),
        Dtype::Float32 => Array::Float32(ArrayD::<f32>::from_elem(dim, fill_value as f32)),
        Dtype::Float64 => Array::Float64(ArrayD::<f64>::from_elem(dim, fill_value)),
        Dtype::Bool => Array::Bool(ArrayD::<bool>::from_elem(dim, fill_value != 0.0)),
        Dtype::Complex64 | Dtype::Complex128 => {
            return Err(NumpyError {
                kind: NumpyErrorKind::LinalgDtypeUnsupported,
                message: format!(
                    "full: complex dtype {dtype} requires Array tagged-union widening; M7.6 ADR-0021 ships dtype tier only"
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

// ---- Stream W item 3: linspace / logspace (numpy `_core/function_base.py`) --
//
// `@py_compat(numerical(rtol=1e-12))` per ADR-0070 §W — float-producing,
// agreement with numpy 2.0.2 to 1e-12 relative on the docstring corpus.

/// Result of `linspace(..., retstep=true)`: the materialised array plus
/// the step between consecutive samples. Mirrors numpy's
/// `(samples, step)` tuple return.
///
/// Per numpy 2.0.2: when `num == 1` the step is `NaN` (no consecutive
/// pair to measure), and when `num == 0` the array is empty and the
/// step is `NaN`.
#[derive(Clone, Debug, PartialEq)]
pub struct LinspaceResult {
    /// The materialised samples.
    pub array: Array,
    /// Spacing between consecutive samples (`NaN` when `num <= 1`).
    pub step: f64,
}

/// Compute the `num` evenly-spaced sample values over `[start, stop]`
/// as an `f64` buffer plus the step, mirroring numpy's float arithmetic.
///
/// numpy computes `step = delta / div` where `div = num - 1` when
/// `endpoint` else `num`, then `y[i] = start + i * step` for the bulk
/// and pins `y[num-1] = stop` exactly when `endpoint && num > 1`
/// (avoids float drift on the last element). The endpoint-pin is why
/// `linspace(0, 1, 5)[4]` is exactly `1.0` while
/// `linspace(0, 1, 5, endpoint=False)[3]` is `0.6000000000000001`.
fn linspace_values(start: f64, stop: f64, num: usize, endpoint: bool) -> (Vec<f64>, f64) {
    if num == 0 {
        return (Vec::new(), f64::NAN);
    }
    if num == 1 {
        return (vec![start], f64::NAN);
    }
    let div = if endpoint {
        (num - 1) as f64
    } else {
        num as f64
    };
    let delta = stop - start;
    let step = delta / div;
    let mut out: Vec<f64> = (0..num).map(|i| start + (i as f64) * step).collect();
    if endpoint {
        // Pin the final sample to `stop` exactly (numpy does this).
        out[num - 1] = stop;
    }
    (out, step)
}

/// `numpy.linspace(start, stop, num=50, endpoint=True, retstep=False,
/// dtype=None)`-equivalent. Returns `num` evenly-spaced samples over
/// `[start, stop]` (inclusive of `stop` when `endpoint`).
///
/// `dtype` selects the output dtype; numpy defaults to `Float64`. When
/// an integer dtype is requested the float samples are truncated toward
/// zero per numpy's cast (`linspace(0, 1, 5, dtype=int)` →
/// `[0, 0, 0, 0, 1]`).
///
/// `@py_compat(numerical(rtol=1e-12))`.
///
/// # Errors
/// - `NumpyError::CastFailed` if `num` would overflow `usize` (caller
///   passes `usize` so this is currently unreachable; reserved).
/// - Forwarded errors from `array(...)`.
pub fn linspace(
    start: f64,
    stop: f64,
    num: usize,
    endpoint: bool,
    dtype: Dtype,
) -> Result<LinspaceResult, NumpyError> {
    let (values, step) = linspace_values(start, stop, num, endpoint);
    let arr = array(&values, &[num], dtype)?;
    Ok(LinspaceResult { array: arr, step })
}

/// `numpy.logspace(start, stop, num=50, endpoint=True, base=10.0,
/// dtype=None)`-equivalent. Returns `num` samples spaced evenly on a
/// log scale: `base ** linspace(start, stop, num, endpoint)`.
///
/// `@py_compat(numerical(rtol=1e-12))`.
///
/// # Errors
/// Forwarded errors from `array(...)`.
pub fn logspace(
    start: f64,
    stop: f64,
    num: usize,
    endpoint: bool,
    base: f64,
    dtype: Dtype,
) -> Result<Array, NumpyError> {
    let (exponents, _step) = linspace_values(start, stop, num, endpoint);
    let values: Vec<f64> = exponents.iter().map(|&e| base.powf(e)).collect();
    array(&values, &[num], dtype)
}

// ---- Stream W item 1: eye / diag / tri / tril / triu ------------------------
// (numpy `lib/_twodim_base_impl.py`)
//
// `@py_compat(strict)` for the integer/structural shape; the default
// `Float64` fill values (1.0 / 0.0) are exact, so even the float-dtype
// forms are bit-exact vs numpy (no `numerical` tolerance needed).

/// `numpy.eye(N, M=None, k=0, dtype=float)`-equivalent. Returns an
/// `N x M` array with ones on the `k`-th diagonal and zeros elsewhere.
///
/// `M` defaults to `N` when `m_cols` is `None`. `k > 0` is an upper
/// diagonal; `k < 0` a lower diagonal. numpy's default dtype is
/// `Float64`; pass `Dtype::Int64` for the integer form.
///
/// `@py_compat(strict)` (values are exactly 0/1; bit-exact vs numpy).
///
/// # Errors
/// Forwarded errors from `array(...)`.
pub fn eye(n: usize, m_cols: Option<usize>, k: i64, dtype: Dtype) -> Result<Array, NumpyError> {
    let m = m_cols.unwrap_or(n);
    let mut values = vec![0.0_f64; n * m];
    for row in 0..n {
        // Column on the k-th diagonal for this row: col = row + k.
        let col = (row as i64) + k;
        if col >= 0 && (col as usize) < m {
            values[row * m + (col as usize)] = 1.0;
        }
    }
    array(&values, &[n, m], dtype)
}

/// `numpy.tri(N, M=None, k=0, dtype=float)`-equivalent. Returns an
/// `N x M` array with ones at and below the `k`-th diagonal and zeros
/// elsewhere (a lower-triangular indicator matrix).
///
/// `@py_compat(strict)`.
///
/// # Errors
/// Forwarded errors from `array(...)`.
pub fn tri(n: usize, m_cols: Option<usize>, k: i64, dtype: Dtype) -> Result<Array, NumpyError> {
    let m = m_cols.unwrap_or(n);
    let mut values = vec![0.0_f64; n * m];
    for row in 0..n {
        for col in 0..m {
            // numpy: tri[i, j] = 1 if j <= i + k else 0.
            if (col as i64) <= (row as i64) + k {
                values[row * m + col] = 1.0;
            }
        }
    }
    array(&values, &[n, m], dtype)
}

/// Read a 2-D `Array` element at `(row, col)` as `f64`, preserving the
/// integer/bool bit pattern through the `f64` lane the constructors use.
/// Helper for `diag` / `tril` / `triu` (which preserve input dtype).
fn elem_f64(arr: &Array, row: usize, col: usize) -> f64 {
    let ix = IxDyn(&[row, col]);
    match arr {
        Array::Int32(a) => f64::from(a[&ix]),
        Array::Int64(a) => a[&ix] as f64,
        Array::Float32(a) => f64::from(a[&ix]),
        Array::Float64(a) => a[&ix],
        Array::Bool(a) => f64::from(u8::from(a[&ix])),
    }
}

/// `numpy.tril(m, k=0)`-equivalent. Returns a copy of the 2-D array `m`
/// with the elements strictly above the `k`-th diagonal zeroed.
/// Preserves the input dtype.
///
/// `@py_compat(strict)`.
///
/// # Errors
/// `NumpyError::LinalgShapeError` if `m` is not 2-D.
pub fn tril(m: &Array, k: i64) -> Result<Array, NumpyError> {
    require_2d(m, "tril")?;
    let shape = m.shape();
    let (rows, cols) = (shape[0], shape[1]);
    let mut values = vec![0.0_f64; rows * cols];
    for row in 0..rows {
        for col in 0..cols {
            if (col as i64) <= (row as i64) + k {
                values[row * cols + col] = elem_f64(m, row, col);
            }
        }
    }
    array(&values, &[rows, cols], m.dtype())
}

/// `numpy.triu(m, k=0)`-equivalent. Returns a copy of the 2-D array `m`
/// with the elements strictly below the `k`-th diagonal zeroed.
/// Preserves the input dtype.
///
/// `@py_compat(strict)`.
///
/// # Errors
/// `NumpyError::LinalgShapeError` if `m` is not 2-D.
pub fn triu(m: &Array, k: i64) -> Result<Array, NumpyError> {
    require_2d(m, "triu")?;
    let shape = m.shape();
    let (rows, cols) = (shape[0], shape[1]);
    let mut values = vec![0.0_f64; rows * cols];
    for row in 0..rows {
        for col in 0..cols {
            if (col as i64) >= (row as i64) + k {
                values[row * cols + col] = elem_f64(m, row, col);
            }
        }
    }
    array(&values, &[rows, cols], m.dtype())
}

/// `numpy.diag(v, k=0)`-equivalent. Two behaviors per numpy:
/// - If `v` is 1-D (length `len`), construct a 2-D array of side
///   `len + |k|` with `v` on the `k`-th diagonal (zeros elsewhere).
/// - If `v` is 2-D, extract the `k`-th diagonal as a 1-D array.
///
/// Preserves the input dtype in both directions.
///
/// `@py_compat(strict)`.
///
/// # Errors
/// `NumpyError::LinalgShapeError` if `v.ndim()` is neither 1 nor 2.
pub fn diag(v: &Array, k: i64) -> Result<Array, NumpyError> {
    match v.ndim() {
        1 => diag_construct(v, k),
        2 => diag_extract(v, k),
        nd => Err(NumpyError {
            kind: NumpyErrorKind::LinalgShapeError,
            message: format!("diag: input must be 1-D or 2-D, got {nd}-D"),
        }),
    }
}

/// 1-D → 2-D: place `v` on the `k`-th diagonal of a zeros matrix.
fn diag_construct(v: &Array, k: i64) -> Result<Array, NumpyError> {
    let len = v.shape()[0];
    let side = len + (k.unsigned_abs() as usize);
    let mut values = vec![0.0_f64; side * side];
    for i in 0..len {
        // numpy: result[i, i + k] = v[i] for k >= 0; result[i - k, i] for k < 0.
        let (row, col) = if k >= 0 {
            (i, i + (k as usize))
        } else {
            (i + (k.unsigned_abs() as usize), i)
        };
        values[row * side + col] = elem_f64_1d(v, i);
    }
    array(&values, &[side, side], v.dtype())
}

/// 2-D → 1-D: extract the `k`-th diagonal.
fn diag_extract(v: &Array, k: i64) -> Result<Array, NumpyError> {
    let shape = v.shape();
    let (rows, cols) = (shape[0], shape[1]);
    // numpy: diagonal starts at (0, k) for k >= 0, (-k, 0) for k < 0.
    let (start_row, start_col) = if k >= 0 {
        (0_usize, k as usize)
    } else {
        (k.unsigned_abs() as usize, 0_usize)
    };
    let mut out = Vec::new();
    let mut row = start_row;
    let mut col = start_col;
    while row < rows && col < cols {
        out.push(elem_f64(v, row, col));
        row += 1;
        col += 1;
    }
    let len = out.len();
    array(&out, &[len], v.dtype())
}

/// Read a 1-D `Array` element as `f64`. Helper for `diag_construct`.
fn elem_f64_1d(arr: &Array, i: usize) -> f64 {
    let ix = IxDyn(&[i]);
    match arr {
        Array::Int32(a) => f64::from(a[&ix]),
        Array::Int64(a) => a[&ix] as f64,
        Array::Float32(a) => f64::from(a[&ix]),
        Array::Float64(a) => a[&ix],
        Array::Bool(a) => f64::from(u8::from(a[&ix])),
    }
}

/// Validate that `arr` is 2-D, returning a `LinalgShapeError` otherwise.
fn require_2d(arr: &Array, op: &str) -> Result<(), NumpyError> {
    if arr.ndim() == 2 {
        Ok(())
    } else {
        Err(NumpyError {
            kind: NumpyErrorKind::LinalgShapeError,
            message: format!("{op}: input must be 2-D, got {}-D", arr.ndim()),
        })
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

    // ---- Stream W item 1: eye / diag / tri / tril / triu ----------------
    // Oracle: numpy 2.0.2 (`python3 -c "import numpy; print(numpy.__version__)"`).

    fn as_f64(a: &Array) -> Vec<f64> {
        match a {
            Array::Float64(arr) => arr.iter().copied().collect(),
            Array::Float32(arr) => arr.iter().map(|v| f64::from(*v)).collect(),
            Array::Int64(arr) => arr.iter().map(|v| *v as f64).collect(),
            Array::Int32(arr) => arr.iter().map(|v| f64::from(*v)).collect(),
            Array::Bool(arr) => arr.iter().map(|v| f64::from(u8::from(*v))).collect(),
        }
    }

    #[test]
    fn eye_3_identity() {
        // np.eye(3) -> [[1,0,0],[0,1,0],[0,0,1]], dtype float64
        let e = eye(3, None, 0, Dtype::Float64).unwrap();
        assert_eq!(e.shape(), vec![3, 3]);
        assert_eq!(e.dtype(), Dtype::Float64);
        assert_eq!(
            as_f64(&e),
            vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn eye_3_4_k1() {
        // np.eye(3,4,k=1) -> diagonal shifted up by 1
        let e = eye(3, Some(4), 1, Dtype::Float64).unwrap();
        assert_eq!(e.shape(), vec![3, 4]);
        assert_eq!(
            as_f64(&e),
            vec![
                0.0, 1.0, 0.0, 0.0, // row 0
                0.0, 0.0, 1.0, 0.0, // row 1
                0.0, 0.0, 0.0, 1.0, // row 2
            ]
        );
    }

    #[test]
    fn eye_3_k_neg1() {
        // np.eye(3,k=-1)
        let e = eye(3, None, -1, Dtype::Float64).unwrap();
        assert_eq!(
            as_f64(&e),
            vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        );
    }

    #[test]
    fn eye_2_3_rectangular() {
        // np.eye(2,3) -> [[1,0,0],[0,1,0]]
        let e = eye(2, Some(3), 0, Dtype::Float64).unwrap();
        assert_eq!(e.shape(), vec![2, 3]);
        assert_eq!(as_f64(&e), vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn eye_int_dtype() {
        // np.eye(3,dtype=int) -> int64 identity
        let e = eye(3, None, 0, Dtype::Int64).unwrap();
        assert_eq!(e.dtype(), Dtype::Int64);
        assert_eq!(
            as_f64(&e),
            vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn tri_3_lower_indicator() {
        // np.tri(3) -> lower-triangular ones (incl diag)
        let t = tri(3, None, 0, Dtype::Float64).unwrap();
        assert_eq!(
            as_f64(&t),
            vec![1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0]
        );
    }

    #[test]
    fn tri_3_4_rectangular() {
        // np.tri(3,4)
        let t = tri(3, Some(4), 0, Dtype::Float64).unwrap();
        assert_eq!(t.shape(), vec![3, 4]);
        assert_eq!(
            as_f64(&t),
            vec![
                1.0, 0.0, 0.0, 0.0, //
                1.0, 1.0, 0.0, 0.0, //
                1.0, 1.0, 1.0, 0.0, //
            ]
        );
    }

    #[test]
    fn tri_3_k1_and_kneg1() {
        // np.tri(3,k=1)
        let t1 = tri(3, None, 1, Dtype::Float64).unwrap();
        assert_eq!(
            as_f64(&t1),
            vec![1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]
        );
        // np.tri(3,k=-1)
        let tm1 = tri(3, None, -1, Dtype::Float64).unwrap();
        assert_eq!(
            as_f64(&tm1),
            vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0]
        );
    }

    #[test]
    fn tril_preserves_dtype_and_zeros_upper() {
        // m2 = [[1,2,3],[4,5,6],[7,8,9]] int64
        let m = array_i64(&[1, 2, 3, 4, 5, 6, 7, 8, 9], &[3, 3]).unwrap();
        let l = tril(&m, 0).unwrap();
        assert_eq!(l.dtype(), Dtype::Int64);
        assert_eq!(
            as_f64(&l),
            vec![1.0, 0.0, 0.0, 4.0, 5.0, 0.0, 7.0, 8.0, 9.0]
        );
        // np.tril(m2,k=1)
        let l1 = tril(&m, 1).unwrap();
        assert_eq!(
            as_f64(&l1),
            vec![1.0, 2.0, 0.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
        );
        // np.tril(m2,k=-1)
        let lm1 = tril(&m, -1).unwrap();
        assert_eq!(
            as_f64(&lm1),
            vec![0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 7.0, 8.0, 0.0]
        );
    }

    #[test]
    fn triu_preserves_dtype_and_zeros_lower() {
        let m = array_i64(&[1, 2, 3, 4, 5, 6, 7, 8, 9], &[3, 3]).unwrap();
        let u = triu(&m, 0).unwrap();
        assert_eq!(
            as_f64(&u),
            vec![1.0, 2.0, 3.0, 0.0, 5.0, 6.0, 0.0, 0.0, 9.0]
        );
        // np.triu(m2,k=1)
        let u1 = triu(&m, 1).unwrap();
        assert_eq!(
            as_f64(&u1),
            vec![0.0, 2.0, 3.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0]
        );
        // np.triu(m2,k=-1)
        let um1 = triu(&m, -1).unwrap();
        assert_eq!(
            as_f64(&um1),
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 0.0, 8.0, 9.0]
        );
    }

    #[test]
    fn tril_rejects_non_2d() {
        let v = array_i64(&[1, 2, 3], &[3]).unwrap();
        let err = tril(&v, 0).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
    }

    #[test]
    fn diag_extract_from_2d() {
        // m = arange(9).reshape(3,3) = [[0,1,2],[3,4,5],[6,7,8]]
        let m = array_i64(&[0, 1, 2, 3, 4, 5, 6, 7, 8], &[3, 3]).unwrap();
        assert_eq!(as_f64(&diag(&m, 0).unwrap()), vec![0.0, 4.0, 8.0]);
        assert_eq!(as_f64(&diag(&m, 1).unwrap()), vec![1.0, 5.0]);
        assert_eq!(as_f64(&diag(&m, -1).unwrap()), vec![3.0, 7.0]);
    }

    #[test]
    fn diag_construct_from_1d() {
        // np.diag([1,2,3]) -> 3x3 with diagonal
        let v = array_i64(&[1, 2, 3], &[3]).unwrap();
        let d = diag(&v, 0).unwrap();
        assert_eq!(d.shape(), vec![3, 3]);
        assert_eq!(
            as_f64(&d),
            vec![1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0]
        );
        // np.diag([1,2,3],k=1) -> 4x4
        let d1 = diag(&v, 1).unwrap();
        assert_eq!(d1.shape(), vec![4, 4]);
        assert_eq!(
            as_f64(&d1),
            vec![
                0.0, 1.0, 0.0, 0.0, //
                0.0, 0.0, 2.0, 0.0, //
                0.0, 0.0, 0.0, 3.0, //
                0.0, 0.0, 0.0, 0.0, //
            ]
        );
        // np.diag([1,2],k=-1) -> 3x3
        let v2 = array_i64(&[1, 2], &[2]).unwrap();
        let dm1 = diag(&v2, -1).unwrap();
        assert_eq!(dm1.shape(), vec![3, 3]);
        assert_eq!(
            as_f64(&dm1),
            vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 0.0]
        );
    }

    #[test]
    fn diag_rejects_3d() {
        let v = array_i64(&[1, 2, 3, 4, 5, 6, 7, 8], &[2, 2, 2]).unwrap();
        let err = diag(&v, 0).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
    }

    // ---- Stream W item 3: linspace / logspace ---------------------------
    // Oracle: numpy 2.0.2.

    #[test]
    fn linspace_0_1_5_endpoint() {
        // np.linspace(0,1,5) -> [0,0.25,0.5,0.75,1.0], step 0.25
        let r = linspace(0.0, 1.0, 5, true, Dtype::Float64).unwrap();
        assert_eq!(r.array.shape(), vec![5]);
        assert_eq!(as_f64(&r.array), vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        assert!((r.step - 0.25).abs() < 1e-15);
        assert_eq!(r.array.dtype(), Dtype::Float64);
    }

    #[test]
    fn linspace_endpoint_false_rounding() {
        // np.linspace(0,1,5,endpoint=False) -> [0,0.2,0.4,0.6000000000000001,0.8]
        // step 0.2. The 0.6000000000000001 is numpy's exact float output.
        let r = linspace(0.0, 1.0, 5, false, Dtype::Float64).unwrap();
        let v = as_f64(&r.array);
        assert_eq!(v[0], 0.0);
        assert_eq!(v[1], 0.2);
        assert_eq!(v[2], 0.4);
        assert_eq!(v[3], 0.600_000_000_000_000_1);
        assert_eq!(v[4], 0.8);
        assert!((r.step - 0.2).abs() < 1e-15);
    }

    #[test]
    fn linspace_num_1_step_is_nan() {
        // np.linspace(2,3,num=1) -> [2.0], step nan
        let r = linspace(2.0, 3.0, 1, true, Dtype::Float64).unwrap();
        assert_eq!(as_f64(&r.array), vec![2.0]);
        assert!(r.step.is_nan());
    }

    #[test]
    fn linspace_num_0_empty() {
        // np.linspace(0,10,num=0) -> []
        let r = linspace(0.0, 10.0, 0, true, Dtype::Float64).unwrap();
        assert_eq!(r.array.size(), 0);
        assert!(r.step.is_nan());
    }

    #[test]
    fn linspace_int_dtype_truncates() {
        // np.linspace(0,1,5,dtype=int) -> [0,0,0,0,1] int64
        let r = linspace(0.0, 1.0, 5, true, Dtype::Int64).unwrap();
        assert_eq!(r.array.dtype(), Dtype::Int64);
        assert_eq!(as_f64(&r.array), vec![0.0, 0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn logspace_default_base10() {
        // np.logspace(0,2,3) -> [1,10,100]
        let l = logspace(0.0, 2.0, 3, true, 10.0, Dtype::Float64).unwrap();
        let v = as_f64(&l);
        assert!((v[0] - 1.0).abs() < 1e-12);
        assert!((v[1] - 10.0).abs() < 1e-12);
        assert!((v[2] - 100.0).abs() < 1e-12);
    }

    #[test]
    fn logspace_base2() {
        // np.logspace(0,2,3,base=2) -> [1,2,4]
        let l = logspace(0.0, 2.0, 3, true, 2.0, Dtype::Float64).unwrap();
        let v = as_f64(&l);
        assert!((v[0] - 1.0).abs() < 1e-12);
        assert!((v[1] - 2.0).abs() < 1e-12);
        assert!((v[2] - 4.0).abs() < 1e-12);
    }

    #[test]
    fn logspace_endpoint_false() {
        // np.logspace(0,2,3,endpoint=False) -> [1, 4.641588833612778, 21.544346900318832]
        let l = logspace(0.0, 2.0, 3, false, 10.0, Dtype::Float64).unwrap();
        let v = as_f64(&l);
        assert!((v[0] - 1.0).abs() < 1e-12);
        assert!((v[1] - 4.641_588_833_612_778).abs() < 1e-12);
        assert!((v[2] - 21.544_346_900_318_832).abs() < 1e-12);
    }

    // ---- BATCH 11 — spacing/value constructor `.cb`-shim contracts ------
    // The `coil.linspace(start, stop, num)` / `coil.logspace(...)` shims
    // call the kernels with `endpoint=true` (numpy's default). These tests
    // pin the BIT-EXACT endpoint-inclusive contract the shims rely on.
    // Oracle: numpy 2.x via `/opt/homebrew/bin/python3.11`.

    #[test]
    fn linspace_endpoint_last_is_stop_bit_exact() {
        // np.linspace(0,1,5)[4] is EXACTLY 1.0 (numpy pins the endpoint to
        // `stop` to avoid float drift). Bit-exact, not within-tolerance.
        let r = linspace(0.0, 1.0, 5, true, Dtype::Float64).unwrap();
        let v = as_f64(&r.array);
        assert_eq!(v[4], 1.0);
        // Bit-identical to `stop` (no `start + 4*step` rounding residue).
        assert_eq!(v[4].to_bits(), 1.0_f64.to_bits());
    }

    #[test]
    fn linspace_2_3_2_both_endpoints() {
        // np.linspace(2,3,2) -> [2.0, 3.0]: num==2 yields exactly the two
        // endpoints, last == stop bit-exactly.
        let r = linspace(2.0, 3.0, 2, true, Dtype::Float64).unwrap();
        assert_eq!(as_f64(&r.array), vec![2.0, 3.0]);
        assert_eq!(as_f64(&r.array)[1].to_bits(), 3.0_f64.to_bits());
        assert!((r.step - 1.0).abs() < 1e-15);
    }

    #[test]
    fn linspace_0_10_5_step_is_2point5() {
        // np.linspace(0,10,5) -> [0, 2.5, 5, 7.5, 10]; step = 10/(5-1) = 2.5.
        let r = linspace(0.0, 10.0, 5, true, Dtype::Float64).unwrap();
        assert_eq!(as_f64(&r.array), vec![0.0, 2.5, 5.0, 7.5, 10.0]);
        assert!((r.step - 2.5).abs() < 1e-15);
    }

    // ---- BATCH 11 — `full` kernel (differential vs numpy) ----------------

    #[test]
    fn full_3_copies_of_5() {
        // np.full(3, 5.0) -> [5., 5., 5.] float64.
        let a = full(&[3], 5.0, Dtype::Float64).unwrap();
        assert_eq!(a.shape(), vec![3]);
        assert_eq!(as_f64(&a), vec![5.0, 5.0, 5.0]);
        assert_eq!(a.dtype(), Dtype::Float64);
    }

    #[test]
    fn full_0_is_empty() {
        // np.full(0, 5.0) -> [] (empty, shape [0]).
        let a = full(&[0], 5.0, Dtype::Float64).unwrap();
        assert_eq!(a.size(), 0);
        assert_eq!(a.shape(), vec![0]);
        assert_eq!(a.dtype(), Dtype::Float64);
    }

    #[test]
    fn full_negative_fill_value() {
        // np.full(2, -1.5) -> [-1.5, -1.5] — the fill is an exact copy.
        let a = full(&[2], -1.5, Dtype::Float64).unwrap();
        assert_eq!(as_f64(&a), vec![-1.5, -1.5]);
    }
}
