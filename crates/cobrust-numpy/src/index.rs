// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.2 indexing per ADR-0015.
// see PROVENANCE.toml for the full manifest.

//! Indexing surface — `Index` enum, `SliceSpec`, `index_get`,
//! `np_where`, plus the `Array::slice / take / mask / index_get /
//! where_` methods (defined in `array.rs` but routed here).
//!
//! Per ADR-0015 §1: closed `Index` enum at the public API (no `dyn`
//! per constitution §2.2). Per ADR-0015 §3: view-vs-copy rules match
//! numpy's documented contract — basic slicing produces a view via
//! `Array::slice` returning `ArrayView<'a>`; integer-array
//! (`Array::take`), boolean mask (`Array::mask`), and `np_where` all
//! produce copies.
//!
//! Per ADR-0015 §4: out-of-bounds + shape-mismatched mask + non-int
//! index dtype all raise typed errors (`OutOfBoundsIndex`,
//! `BoolMaskShapeMismatch`, `IndexDtypeNotInteger`). Per ADR-0015 §5:
//! negative-index normalisation matches numpy; slice bounds clamp
//! (not error) on out-of-range; zero step → `ZeroStep`.

// CQ P1-4 + template-fix: all file-level allows consolidated into one block.
// Future translator emits should use #[allow] at item level; file-level retained
// here because index.rs is auto-generated and items are too numerous to annotate
// individually without a regen step.
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::if_not_else,
    clippy::map_unwrap_or,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unnecessary_wraps
)]

use ndarray::{ArrayD, IxDyn};

use crate::array::Array;
use crate::broadcast::broadcast_shape;
use crate::dtype::Dtype;
use crate::error::{NumpyError, NumpyErrorKind};
use crate::view::ArrayView;

// ---- SliceSpec -----------------------------------------------------------

/// Numpy `slice(start, stop, step)` triple. `None` values use numpy
/// defaults (start=0, stop=len, step=1 with sign-aware reverse).
///
/// Per ADR-0015 §1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SliceSpec {
    pub start: Option<i64>,
    pub stop: Option<i64>,
    pub step: Option<i64>,
}

impl SliceSpec {
    /// `[:]` — full slice.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            start: None,
            stop: None,
            step: None,
        }
    }

    /// `[start:]` — from `start` to end.
    #[must_use]
    pub const fn from_start(start: i64) -> Self {
        Self {
            start: Some(start),
            stop: None,
            step: None,
        }
    }

    /// `[:stop]` — from beginning to `stop` (exclusive).
    #[must_use]
    pub const fn to_stop(stop: i64) -> Self {
        Self {
            start: None,
            stop: Some(stop),
            step: None,
        }
    }

    /// `[start:stop]`.
    #[must_use]
    pub const fn range(start: i64, stop: i64) -> Self {
        Self {
            start: Some(start),
            stop: Some(stop),
            step: None,
        }
    }

    /// `[start:stop:step]`.
    #[must_use]
    pub const fn stepped(start: i64, stop: i64, step: i64) -> Self {
        Self {
            start: Some(start),
            stop: Some(stop),
            step: Some(step),
        }
    }

    /// `[::step]` — every Nth element.
    #[must_use]
    pub const fn step_only(step: i64) -> Self {
        Self {
            start: None,
            stop: None,
            step: Some(step),
        }
    }
}

// ---- Index taxonomy -----------------------------------------------------

/// Closed indexing-kind taxonomy per ADR-0015 §1.
///
/// Per constitution §2.2 (no `dyn`): pattern-matchable on the
/// view-vs-copy decision. Variants:
/// - `Single(i64)` — `a[i]`; negative-index aware, drops first axis (view).
/// - `Slice(SliceSpec)` — `a[start:stop:step]`; basic slicing (view).
/// - `IntArray(Vec<i64>)` — `a[[0, 2, 5]]`; advanced indexing → copy.
/// - `BoolMask(Array)` — `a[a > 0]`; mask must be `Bool`-dtype, copy.
/// - `NewAxis` — `a[np.newaxis]`; inserts a length-1 axis.
#[derive(Clone, Debug, PartialEq)]
pub enum Index {
    /// `a[i]` — single integer index on the first axis.
    Single(i64),
    /// `a[start:stop:step]` — basic slicing on the first axis.
    Slice(SliceSpec),
    /// `a[[0, 2, 5]]` — integer-array indexing on the first axis.
    /// Always copies per ADR-0015 §3.
    IntArray(Vec<i64>),
    /// `a[mask]` — boolean-mask indexing. Mask must be `Bool`-dtype
    /// and have the same shape as `a`. Always copies per ADR-0015 §3.
    BoolMask(Array),
    /// `a[np.newaxis]` — inserts a length-1 axis (no data movement).
    NewAxis,
}

// ---- Helpers ------------------------------------------------------------

/// Normalise a negative single-int index per ADR-0015 §5.
/// Returns the non-negative index or `OutOfBoundsIndex`.
pub(crate) fn normalize_single(idx: i64, length: i64) -> Result<i64, NumpyError> {
    let norm = if idx < 0 { idx + length } else { idx };
    if norm < 0 || norm >= length {
        return Err(NumpyError {
            kind: NumpyErrorKind::OutOfBoundsIndex,
            message: format!("index {idx} is out of bounds for axis with length {length}"),
        });
    }
    Ok(norm)
}

/// Resolve numpy-exact slice bounds per ADR-0015 §5.
///
/// Returns `(begin, end, step)` with `step != 0`. Out-of-range bounds
/// are clamped to the valid range (matches numpy slice semantics).
/// `step == 0` raises `ZeroStep` (matches numpy `ValueError`).
pub(crate) fn resolve_slice(
    start: Option<i64>,
    stop: Option<i64>,
    step: Option<i64>,
    length: i64,
) -> Result<(i64, i64, i64), NumpyError> {
    let st = step.unwrap_or(1);
    if st == 0 {
        return Err(NumpyError {
            kind: NumpyErrorKind::ZeroStep,
            message: "slice step cannot be zero".into(),
        });
    }

    let (begin, end) = if st > 0 {
        // Default start = 0, stop = length.
        let mut s = start.unwrap_or(0);
        let mut e = stop.unwrap_or(length);
        if s < 0 {
            s += length;
        }
        if e < 0 {
            e += length;
        }
        s = s.max(0).min(length);
        e = e.max(0).min(length);
        if e < s {
            e = s;
        }
        (s, e)
    } else {
        // step < 0: walk down. Default start = length-1, stop = -length-1.
        let mut s = start.unwrap_or(length - 1);
        let mut e = stop.unwrap_or(-length - 1);
        if s < 0 && start.is_some() {
            s += length;
        }
        if e < 0 && stop.is_some() {
            e += length;
        }
        s = s.max(-1).min(length - 1);
        if e < -1 {
            e = -1;
        } else if e > length {
            e = length;
        }
        if e > s {
            e = s;
        }
        (s, e)
    };
    Ok((begin, end, st))
}

/// Count elements produced by a normalised slice. Helper exposed for
/// the well-typed test suite (mirrors `index_core.slice_count`).
#[must_use]
#[allow(dead_code)]
pub(crate) fn slice_count(begin: i64, end: i64, step: i64) -> usize {
    if step > 0 {
        if end <= begin {
            0
        } else {
            ((end - begin + step - 1) / step) as usize
        }
    } else if end >= begin {
        0
    } else {
        ((begin - end + (-step) - 1) / (-step)) as usize
    }
}

// ---- View constructors --------------------------------------------------

/// Public re-export of the numpy→ndarray slice translation, used by
/// `Array::slice_mut`. Otherwise crate-private.
pub(crate) fn to_nd_slice_pub(begin: i64, end: i64, step: i64, length: i64) -> ndarray::Slice {
    to_nd_slice(begin, end, step, length)
}

/// Convert numpy-style `(begin, end, step)` to `ndarray::Slice` per
/// the upstream `ndarray::Slice` contract:
///   - For positive step: indices `begin..end` with stride `step`.
///   - For negative step: numpy semantics walk down from `begin`
///     toward `end` (exclusive). ndarray::Slice with a negative step
///     uses `start..end` with `start <= end` (always positive
///     bounds) and reverses inside, so we must translate.
fn to_nd_slice(begin: i64, end: i64, step: i64, length: i64) -> ndarray::Slice {
    if step > 0 {
        ndarray::Slice {
            start: begin as isize,
            end: Some(end as isize),
            step: step as isize,
        }
    } else {
        // numpy walks from `begin` down to `end` (exclusive). To map
        // to ndarray::Slice with positive bounds + negative step:
        //   - The set of selected indices is {begin, begin+step,
        //     begin+2*step, ...} all > end.
        //   - The min selected index is `begin + (n-1)*step` where
        //     `n = (begin - end - 1) / (-step) + 1`.
        //   - ndarray's negative-step Slice is `start..end` where
        //     `start = lo`, `end = hi+1`, and reversed iteration runs
        //     `hi, hi-step, ...` toward `lo`.
        let n = if end >= begin {
            0
        } else {
            (begin - end + (-step) - 1) / (-step)
        };
        let _ = length;
        if n == 0 {
            // Empty slice: produce an empty positive-step view.
            return ndarray::Slice {
                start: 0,
                end: Some(0),
                step: 1,
            };
        }
        let hi = begin; // top of the walk (inclusive)
        let lo = begin + (n - 1) * step; // bottom of the walk (inclusive, lo <= hi)
        ndarray::Slice {
            start: lo as isize,
            end: Some((hi + 1) as isize),
            step: step as isize,
        }
    }
}

/// Build an `ArrayView<'a>` from a basic-slice spec on the first axis.
///
/// Per ADR-0015 §3 this returns a view (does not copy). Backed by
/// `ndarray::ArrayBase::slice_axis` which produces an `ArrayViewD`.
pub(crate) fn slice_view(arr: &Array, spec: SliceSpec) -> Result<ArrayView<'_>, NumpyError> {
    if arr.ndim() == 0 {
        return Err(NumpyError {
            kind: NumpyErrorKind::IndexError,
            message: "cannot slice a 0-d array".into(),
        });
    }
    let length = arr.shape()[0] as i64;
    let (begin, end, step) = resolve_slice(spec.start, spec.stop, spec.step, length)?;
    let nd_slice = to_nd_slice(begin, end, step, length);
    Ok(match arr {
        Array::Int32(a) => ArrayView::Int32(a.slice_axis(ndarray::Axis(0), nd_slice)),
        Array::Int64(a) => ArrayView::Int64(a.slice_axis(ndarray::Axis(0), nd_slice)),
        Array::Float32(a) => ArrayView::Float32(a.slice_axis(ndarray::Axis(0), nd_slice)),
        Array::Float64(a) => ArrayView::Float64(a.slice_axis(ndarray::Axis(0), nd_slice)),
        Array::Bool(a) => ArrayView::Bool(a.slice_axis(ndarray::Axis(0), nd_slice)),
    })
}

/// Build a single-int view (drops the first axis) per ADR-0015 §3.
pub(crate) fn single_view(arr: &Array, idx: i64) -> Result<ArrayView<'_>, NumpyError> {
    if arr.ndim() == 0 {
        return Err(NumpyError {
            kind: NumpyErrorKind::IndexError,
            message: "cannot index a 0-d array".into(),
        });
    }
    let length = arr.shape()[0] as i64;
    let i = normalize_single(idx, length)? as usize;
    Ok(match arr {
        Array::Int32(a) => ArrayView::Int32(a.index_axis(ndarray::Axis(0), i)),
        Array::Int64(a) => ArrayView::Int64(a.index_axis(ndarray::Axis(0), i)),
        Array::Float32(a) => ArrayView::Float32(a.index_axis(ndarray::Axis(0), i)),
        Array::Float64(a) => ArrayView::Float64(a.index_axis(ndarray::Axis(0), i)),
        Array::Bool(a) => ArrayView::Bool(a.index_axis(ndarray::Axis(0), i)),
    })
}

// ---- take (integer-array indexing → copy) -------------------------------

/// Integer-array indexing on the first axis. Always returns a copy
/// per ADR-0015 §3.
pub(crate) fn take_impl(arr: &Array, indices: &[i64]) -> Result<Array, NumpyError> {
    if arr.ndim() == 0 {
        return Err(NumpyError {
            kind: NumpyErrorKind::IndexError,
            message: "cannot take from a 0-d array".into(),
        });
    }
    let length = arr.shape()[0] as i64;
    let mut norm: Vec<usize> = Vec::with_capacity(indices.len());
    for &idx in indices {
        norm.push(normalize_single(idx, length)? as usize);
    }

    let mut out_shape: Vec<usize> = vec![indices.len()];
    out_shape.extend(arr.shape().iter().skip(1));
    let inner: usize = arr.shape().iter().skip(1).product();

    Ok(match arr {
        Array::Int32(a) => {
            let flat = a.as_slice().expect("contiguous");
            let mut out = Vec::<i32>::with_capacity(indices.len() * inner);
            for &i in &norm {
                let base = i * inner;
                out.extend_from_slice(&flat[base..base + inner]);
            }
            Array::Int32(ArrayD::<i32>::from_shape_vec(IxDyn(&out_shape), out).map_err(shape_err)?)
        }
        Array::Int64(a) => {
            let flat = a.as_slice().expect("contiguous");
            let mut out = Vec::<i64>::with_capacity(indices.len() * inner);
            for &i in &norm {
                let base = i * inner;
                out.extend_from_slice(&flat[base..base + inner]);
            }
            Array::Int64(ArrayD::<i64>::from_shape_vec(IxDyn(&out_shape), out).map_err(shape_err)?)
        }
        Array::Float32(a) => {
            let flat = a.as_slice().expect("contiguous");
            let mut out = Vec::<f32>::with_capacity(indices.len() * inner);
            for &i in &norm {
                let base = i * inner;
                out.extend_from_slice(&flat[base..base + inner]);
            }
            Array::Float32(
                ArrayD::<f32>::from_shape_vec(IxDyn(&out_shape), out).map_err(shape_err)?,
            )
        }
        Array::Float64(a) => {
            let flat = a.as_slice().expect("contiguous");
            let mut out = Vec::<f64>::with_capacity(indices.len() * inner);
            for &i in &norm {
                let base = i * inner;
                out.extend_from_slice(&flat[base..base + inner]);
            }
            Array::Float64(
                ArrayD::<f64>::from_shape_vec(IxDyn(&out_shape), out).map_err(shape_err)?,
            )
        }
        Array::Bool(a) => {
            let flat = a.as_slice().expect("contiguous");
            let mut out = Vec::<bool>::with_capacity(indices.len() * inner);
            for &i in &norm {
                let base = i * inner;
                out.extend_from_slice(&flat[base..base + inner]);
            }
            Array::Bool(ArrayD::<bool>::from_shape_vec(IxDyn(&out_shape), out).map_err(shape_err)?)
        }
    })
}

// ---- mask (boolean-mask indexing → 1-D copy) ----------------------------

/// Boolean-mask indexing per ADR-0015 §3. Mask must be `Bool`-dtype
/// and have the same shape as `arr`. Returns a 1-D copy of selected
/// elements (matches numpy).
pub(crate) fn mask_impl(arr: &Array, mask: &Array) -> Result<Array, NumpyError> {
    if mask.dtype() != Dtype::Bool {
        return Err(NumpyError {
            kind: NumpyErrorKind::IndexDtypeNotInteger,
            message: format!("boolean index requires Dtype::Bool; got {:?}", mask.dtype()),
        });
    }
    if arr.shape() != mask.shape() {
        return Err(NumpyError {
            kind: NumpyErrorKind::BoolMaskShapeMismatch,
            message: format!(
                "boolean index shape mismatch: a={:?} mask={:?}",
                arr.shape(),
                mask.shape()
            ),
        });
    }
    let Array::Bool(mask_arr) = mask else {
        unreachable!("dtype check above");
    };
    let mask_flat = mask_arr.as_slice().expect("contiguous");

    Ok(match arr {
        Array::Int32(a) => {
            let flat = a.as_slice().expect("contiguous");
            let out: Vec<i32> = flat
                .iter()
                .zip(mask_flat.iter())
                .filter_map(|(v, &keep)| if keep { Some(*v) } else { None })
                .collect();
            let n = out.len();
            Array::Int32(ArrayD::<i32>::from_shape_vec(IxDyn(&[n]), out).map_err(shape_err)?)
        }
        Array::Int64(a) => {
            let flat = a.as_slice().expect("contiguous");
            let out: Vec<i64> = flat
                .iter()
                .zip(mask_flat.iter())
                .filter_map(|(v, &keep)| if keep { Some(*v) } else { None })
                .collect();
            let n = out.len();
            Array::Int64(ArrayD::<i64>::from_shape_vec(IxDyn(&[n]), out).map_err(shape_err)?)
        }
        Array::Float32(a) => {
            let flat = a.as_slice().expect("contiguous");
            let out: Vec<f32> = flat
                .iter()
                .zip(mask_flat.iter())
                .filter_map(|(v, &keep)| if keep { Some(*v) } else { None })
                .collect();
            let n = out.len();
            Array::Float32(ArrayD::<f32>::from_shape_vec(IxDyn(&[n]), out).map_err(shape_err)?)
        }
        Array::Float64(a) => {
            let flat = a.as_slice().expect("contiguous");
            let out: Vec<f64> = flat
                .iter()
                .zip(mask_flat.iter())
                .filter_map(|(v, &keep)| if keep { Some(*v) } else { None })
                .collect();
            let n = out.len();
            Array::Float64(ArrayD::<f64>::from_shape_vec(IxDyn(&[n]), out).map_err(shape_err)?)
        }
        Array::Bool(a) => {
            let flat = a.as_slice().expect("contiguous");
            let out: Vec<bool> = flat
                .iter()
                .zip(mask_flat.iter())
                .filter_map(|(v, &keep)| if keep { Some(*v) } else { None })
                .collect();
            let n = out.len();
            Array::Bool(ArrayD::<bool>::from_shape_vec(IxDyn(&[n]), out).map_err(shape_err)?)
        }
    })
}

// ---- index_get top-level ------------------------------------------------

/// Multi-axis indexing dispatcher per ADR-0015 §1.
///
/// M7.2 ships single-axis cases via per-`Index`-variant routing.
/// Multi-axis chains are materialised iteratively: each axis is
/// applied in order on the leading axis (per-axis policy). For
/// combinations with advanced indexing, the result is always copied
/// per ADR-0015 §3. ADR-0015 §"M7.2 scope window" notes that
/// multi-axis tuple-of-mixed-kind indexing where some axes are views
/// and others copies is M7.x deferred.
pub fn index_get(arr: &Array, indices: &[Index]) -> Result<Array, NumpyError> {
    if indices.is_empty() {
        return Ok(arr.clone());
    }
    let head = &indices[0];
    let rest = &indices[1..];
    let stepped = match head {
        Index::Single(i) => single_view(arr, *i)?.to_owned(),
        Index::Slice(spec) => slice_view(arr, *spec)?.to_owned(),
        Index::IntArray(idx) => take_impl(arr, idx)?,
        Index::BoolMask(mask) => mask_impl(arr, mask)?,
        Index::NewAxis => insert_axis(arr, 0)?,
    };
    // When the result is 0-d but more indices remain, it is an error
    // — there is no axis left to index.
    if !rest.is_empty() && stepped.ndim() == 0 {
        return Err(NumpyError {
            kind: NumpyErrorKind::IndexError,
            message: format!(
                "too many indices for array; remaining {} indices after 0-d reduction",
                rest.len()
            ),
        });
    }
    if rest.is_empty() {
        Ok(stepped)
    } else {
        index_get(&stepped, rest)
    }
}

/// Insert a length-1 axis at `axis` (matches `np.expand_dims`).
fn insert_axis(arr: &Array, axis: usize) -> Result<Array, NumpyError> {
    Ok(match arr {
        Array::Int32(a) => Array::Int32(a.clone().insert_axis(ndarray::Axis(axis))),
        Array::Int64(a) => Array::Int64(a.clone().insert_axis(ndarray::Axis(axis))),
        Array::Float32(a) => Array::Float32(a.clone().insert_axis(ndarray::Axis(axis))),
        Array::Float64(a) => Array::Float64(a.clone().insert_axis(ndarray::Axis(axis))),
        Array::Bool(a) => Array::Bool(a.clone().insert_axis(ndarray::Axis(axis))),
    })
}

// ---- np_where (ternary selection → copy) --------------------------------

/// Cast helper: any numeric/bool dtype → bool element-wise.
fn to_bool_array(arr: &Array) -> ArrayD<bool> {
    match arr {
        Array::Int32(a) => a.mapv(|v| v != 0),
        Array::Int64(a) => a.mapv(|v| v != 0),
        Array::Float32(a) => a.mapv(|v| v != 0.0),
        Array::Float64(a) => a.mapv(|v| v != 0.0),
        Array::Bool(a) => a.clone(),
    }
}

/// `np.where(cond, x, y)` — element-wise selection with broadcasting.
///
/// Per ADR-0015 §"Public surface (M7.2 additions)" + ADR-0014 §2:
/// broadcasts cond/x/y per numpy rules; output dtype is
/// `result_type(x.dtype(), y.dtype())`. Always materialises (copy).
///
/// `cond` is silently cast to bool (matches numpy); non-bool dtypes
/// are interpreted as truthy != 0.
pub fn np_where(cond: &Array, x: &Array, y: &Array) -> Result<Array, NumpyError> {
    // Compute the broadcast shape across all three.
    let cx_shape = broadcast_shape(&cond.shape(), &x.shape())?;
    let target_shape = broadcast_shape(&cx_shape, &y.shape())?;
    let target_ix = IxDyn(&target_shape);

    // Promote x/y per the result_type table; cast cond to bool.
    let promoted = crate::promote::result_type(x.dtype(), y.dtype());
    let x_cast = cast_to(x, promoted);
    let y_cast = cast_to(y, promoted);
    let cond_bool = to_bool_array(cond);

    let cond_b = broadcast_owned_bool(&cond_bool, &target_shape);

    Ok(match (x_cast, y_cast) {
        (Array::Int32(xv), Array::Int32(yv)) => {
            let xv_b = broadcast_owned(&xv, &target_shape);
            let yv_b = broadcast_owned(&yv, &target_shape);
            let mut out = ArrayD::<i32>::zeros(target_ix);
            ndarray::Zip::from(&mut out)
                .and(&cond_b)
                .and(&xv_b)
                .and(&yv_b)
                .for_each(|o, &c, &x, &y| {
                    *o = if c { x } else { y };
                });
            Array::Int32(out)
        }
        (Array::Int64(xv), Array::Int64(yv)) => {
            let xv_b = broadcast_owned(&xv, &target_shape);
            let yv_b = broadcast_owned(&yv, &target_shape);
            let mut out = ArrayD::<i64>::zeros(target_ix);
            ndarray::Zip::from(&mut out)
                .and(&cond_b)
                .and(&xv_b)
                .and(&yv_b)
                .for_each(|o, &c, &x, &y| {
                    *o = if c { x } else { y };
                });
            Array::Int64(out)
        }
        (Array::Float32(xv), Array::Float32(yv)) => {
            let xv_b = broadcast_owned(&xv, &target_shape);
            let yv_b = broadcast_owned(&yv, &target_shape);
            let mut out = ArrayD::<f32>::zeros(target_ix);
            ndarray::Zip::from(&mut out)
                .and(&cond_b)
                .and(&xv_b)
                .and(&yv_b)
                .for_each(|o, &c, &x, &y| {
                    *o = if c { x } else { y };
                });
            Array::Float32(out)
        }
        (Array::Float64(xv), Array::Float64(yv)) => {
            let xv_b = broadcast_owned(&xv, &target_shape);
            let yv_b = broadcast_owned(&yv, &target_shape);
            let mut out = ArrayD::<f64>::zeros(target_ix);
            ndarray::Zip::from(&mut out)
                .and(&cond_b)
                .and(&xv_b)
                .and(&yv_b)
                .for_each(|o, &c, &x, &y| {
                    *o = if c { x } else { y };
                });
            Array::Float64(out)
        }
        (Array::Bool(xv), Array::Bool(yv)) => {
            let xv_b = broadcast_owned_bool(&xv, &target_shape);
            let yv_b = broadcast_owned_bool(&yv, &target_shape);
            let mut out = ArrayD::<bool>::from_elem(target_ix, false);
            ndarray::Zip::from(&mut out)
                .and(&cond_b)
                .and(&xv_b)
                .and(&yv_b)
                .for_each(|o, &c, &x, &y| {
                    *o = if c { x } else { y };
                });
            Array::Bool(out)
        }
        _ => unreachable!("cast_to must produce matching variants"),
    })
}

// ---- Internal helpers ---------------------------------------------------

fn shape_err(e: ndarray::ShapeError) -> NumpyError {
    NumpyError {
        kind: NumpyErrorKind::ShapeMismatch,
        message: format!("ndarray from_shape_vec: {e}"),
    }
}

fn cast_to(arr: &Array, target: Dtype) -> Array {
    match target {
        Dtype::Int32 => Array::Int32(match arr {
            Array::Int32(a) => a.clone(),
            Array::Int64(a) => a.mapv(|v| v as i32),
            Array::Float32(a) => a.mapv(|v| v as i32),
            Array::Float64(a) => a.mapv(|v| v as i32),
            Array::Bool(a) => a.mapv(i32::from),
        }),
        Dtype::Int64 => Array::Int64(match arr {
            Array::Int32(a) => a.mapv(i64::from),
            Array::Int64(a) => a.clone(),
            Array::Float32(a) => a.mapv(|v| v as i64),
            Array::Float64(a) => a.mapv(|v| v as i64),
            Array::Bool(a) => a.mapv(i64::from),
        }),
        Dtype::Float32 => Array::Float32(match arr {
            Array::Int32(a) => a.mapv(|v| v as f32),
            Array::Int64(a) => a.mapv(|v| v as f32),
            Array::Float32(a) => a.clone(),
            Array::Float64(a) => a.mapv(|v| v as f32),
            Array::Bool(a) => a.mapv(|v| f32::from(u8::from(v))),
        }),
        Dtype::Float64 => Array::Float64(match arr {
            Array::Int32(a) => a.mapv(f64::from),
            Array::Int64(a) => a.mapv(|v| v as f64),
            Array::Float32(a) => a.mapv(f64::from),
            Array::Float64(a) => a.clone(),
            Array::Bool(a) => a.mapv(|v| f64::from(u8::from(v))),
        }),
        Dtype::Bool => Array::Bool(match arr {
            Array::Int32(a) => a.mapv(|v| v != 0),
            Array::Int64(a) => a.mapv(|v| v != 0),
            Array::Float32(a) => a.mapv(|v| v != 0.0),
            Array::Float64(a) => a.mapv(|v| v != 0.0),
            Array::Bool(a) => a.clone(),
        }),
        Dtype::Complex64 | Dtype::Complex128 => {
            // Per ADR-0021 §3 the M7.6 sub-milestone widens `Dtype` to seven
            // variants but defers the `Array` tagged-union widening (and
            // therefore complex-cast routing) to a follow-up sprint.
            // Reaching this arm at M7.6 means a caller passed a complex
            // target_dtype to a code path that did not pre-validate it;
            // every consumer in the M7.6 surface filters complex via
            // `Dtype::is_complex` before calling `cast_to`.
            unreachable!(
                "index::cast_to: target Complex dtype routed through real-only path;                  callers must filter via Dtype::is_complex before reaching here                  (M7.6 ADR-0021 §3)"
            );
        }
    }
}

fn broadcast_owned<T: Clone>(arr: &ArrayD<T>, target: &[usize]) -> ArrayD<T> {
    arr.broadcast(IxDyn(target))
        .map(|view| view.to_owned())
        .unwrap_or_else(|| arr.clone())
}

fn broadcast_owned_bool(arr: &ArrayD<bool>, target: &[usize]) -> ArrayD<bool> {
    arr.broadcast(IxDyn(target))
        .map(|view| view.to_owned())
        .unwrap_or_else(|| arr.clone())
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
    #![allow(clippy::approx_constant)]
    #![allow(clippy::uninlined_format_args)]
    use super::*;

    #[test]
    fn slice_count_positive_step() {
        assert_eq!(slice_count(0, 5, 1), 5);
        assert_eq!(slice_count(0, 10, 2), 5);
        assert_eq!(slice_count(0, 9, 2), 5);
    }

    #[test]
    fn slice_count_negative_step() {
        assert_eq!(slice_count(4, -1, -1), 5);
        assert_eq!(slice_count(9, -1, -2), 5);
    }

    #[test]
    fn resolve_slice_zero_step_errors() {
        let err = resolve_slice(None, None, Some(0), 10).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
    }

    #[test]
    fn normalize_negative() {
        assert_eq!(normalize_single(-1, 10).unwrap(), 9);
        assert_eq!(normalize_single(-10, 10).unwrap(), 0);
    }

    #[test]
    fn normalize_out_of_range_errors() {
        assert_eq!(
            normalize_single(10, 10).unwrap_err().kind,
            NumpyErrorKind::OutOfBoundsIndex
        );
        assert_eq!(
            normalize_single(-11, 10).unwrap_err().kind,
            NumpyErrorKind::OutOfBoundsIndex
        );
    }
}
