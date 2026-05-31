// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.3 reductions per ADR-0016.
// see PROVENANCE.toml for the full manifest.

//! Reduction surface — `sum / prod / mean / std / var / min / max /
//! argmin / argmax` per ADR-0016.
//!
//! Per ADR-0016 §1 the surface is closed at 9 reductions. Per ADR-0016
//! §2 axis semantics are `axis: Option<i64>` (None reduces all; Some(k)
//! reduces along axis k; negative-axis aware). Per ADR-0016 §3 float
//! sum/mean/std/var uses pairwise summation (chunk size 8) to match
//! numpy's accuracy floor — O(log n × eps) instead of naive O(n × eps).
//! Per ADR-0016 §4 std/var carry a `ddof: u32` parameter (default 0).
//! Per ADR-0016 §5 empty-array behavior matches numpy: identity for
//! sum/prod, NaN for mean/std/var, `ReductionEmptyArray` error for
//! min/max/argmin/argmax.
//!
//! Constitution §2.2 (no `dyn`) is satisfied: every dispatch arm is on
//! a closed enum variant. Constitution §5.3 (efficient): inner loops
//! are auto-vectorisable.

// CQ P1-4 + template-fix: all file-level allows consolidated into one block.
// Future translator emits should use #[allow] at item level; file-level retained
// here because reduce.rs is auto-generated and items are too numerous to annotate
// individually without a regen step.
#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::excessive_precision,
    clippy::explicit_iter_loop,
    clippy::float_cmp,
    clippy::if_not_else,
    clippy::imprecise_flops,
    clippy::map_unwrap_or,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unnecessary_wraps
)]

use ndarray::{ArrayD, Axis, IxDyn};

use crate::array::Array;
// use crate::dtype::Dtype;  // unused at M7.3 — kept for symmetry
use crate::error::{NumpyError, NumpyErrorKind};

// ---- Pairwise summation (per ADR-0016 §3) -------------------------------

/// Pairwise summation matching numpy's algorithm. Chunk size 8: leaves
/// of size <= 8 sum naively; recursive bisection above. Suppresses the
/// floating-point error from naive O(n × eps) to O(log n × eps).
#[must_use]
pub fn pairwise_sum_f64(values: &[f64]) -> f64 {
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n <= 8 {
        let mut s = 0.0_f64;
        for v in values {
            s += *v;
        }
        return s;
    }
    let mid = n / 2;
    pairwise_sum_f64(&values[..mid]) + pairwise_sum_f64(&values[mid..])
}

/// Pairwise summation for `f32`. Same algorithm; separate fn to avoid
/// wide intermediate accumulation that would mask precision the user
/// expects from `f32` arithmetic.
#[must_use]
pub fn pairwise_sum_f32(values: &[f32]) -> f32 {
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n <= 8 {
        let mut s = 0.0_f32;
        for v in values {
            s += *v;
        }
        return s;
    }
    let mid = n / 2;
    pairwise_sum_f32(&values[..mid]) + pairwise_sum_f32(&values[mid..])
}

// ---- Axis normalisation -------------------------------------------------

/// Normalise an `Option<i64>` axis index. `None` indicates reduce-all.
/// Negative values normalise mod ndim. Out-of-bounds raises
/// `IndexError`.
fn normalize_axis(axis: Option<i64>, ndim: usize) -> Result<Option<usize>, NumpyError> {
    let Some(mut a) = axis else {
        return Ok(None);
    };
    let n = ndim as i64;
    if a < 0 {
        a += n;
    }
    if n == 0 || a < 0 || a >= n {
        return Err(NumpyError {
            kind: NumpyErrorKind::IndexError,
            message: format!("axis {axis:?} is out of bounds for array of dimension {ndim}"),
        });
    }
    Ok(Some(a as usize))
}

// ---- Dtype-collapse helpers ---------------------------------------------

/// Collapse a 1-element 1-D `Vec<T>` of result data into a 0-d Array.
fn make_scalar_f64(v: f64) -> Array {
    let arr: ArrayD<f64> =
        ArrayD::from_shape_vec(IxDyn(&[]), vec![v]).expect("0-d shape always succeeds");
    Array::Float64(arr)
}

fn make_scalar_f32(v: f32) -> Array {
    let arr: ArrayD<f32> =
        ArrayD::from_shape_vec(IxDyn(&[]), vec![v]).expect("0-d shape always succeeds");
    Array::Float32(arr)
}

fn make_scalar_i64(v: i64) -> Array {
    let arr: ArrayD<i64> =
        ArrayD::from_shape_vec(IxDyn(&[]), vec![v]).expect("0-d shape always succeeds");
    Array::Int64(arr)
}

fn make_scalar_i32(v: i32) -> Array {
    let arr: ArrayD<i32> =
        ArrayD::from_shape_vec(IxDyn(&[]), vec![v]).expect("0-d shape always succeeds");
    Array::Int32(arr)
}

fn from_vec_f64(data: Vec<f64>, shape: Vec<usize>) -> Array {
    Array::Float64(ArrayD::from_shape_vec(IxDyn(&shape), data).expect("shape * len match"))
}

fn from_vec_f32(data: Vec<f32>, shape: Vec<usize>) -> Array {
    Array::Float32(ArrayD::from_shape_vec(IxDyn(&shape), data).expect("shape * len match"))
}

fn from_vec_i64(data: Vec<i64>, shape: Vec<usize>) -> Array {
    Array::Int64(ArrayD::from_shape_vec(IxDyn(&shape), data).expect("shape * len match"))
}

fn from_vec_i32(data: Vec<i32>, shape: Vec<usize>) -> Array {
    Array::Int32(ArrayD::from_shape_vec(IxDyn(&shape), data).expect("shape * len match"))
}

/// Output shape after dropping `axis` from `shape`.
fn drop_axis(shape: &[usize], axis: usize) -> Vec<usize> {
    let mut out = Vec::with_capacity(shape.len().saturating_sub(1));
    for (i, &d) in shape.iter().enumerate() {
        if i != axis {
            out.push(d);
        }
    }
    out
}

// ---- sum -----------------------------------------------------------------

/// `np.sum(arr, axis=...)`. Pairwise summation for floats per ADR-0016
/// §3; integer reductions use Rust's wrapping_add.
pub fn sum(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError> {
    let shape = arr.shape();
    let ax = normalize_axis(axis, shape.len())?;
    match ax {
        None => sum_all(arr),
        Some(k) => sum_axis(arr, k),
    }
}

fn sum_all(arr: &Array) -> Result<Array, NumpyError> {
    Ok(match arr {
        Array::Int32(a) => {
            let mut s: i32 = 0;
            for &v in a.iter() {
                s = s.wrapping_add(v);
            }
            make_scalar_i32(s)
        }
        Array::Int64(a) => {
            let mut s: i64 = 0;
            for &v in a.iter() {
                s = s.wrapping_add(v);
            }
            make_scalar_i64(s)
        }
        Array::Float32(a) => {
            // Contiguous fast path (no intermediate Vec): sum directly over
            // the backing slice when the layout is standard-contiguous;
            // `as_slice()` is `None` for non-contiguous views, where we fall
            // back to the collect. Same `pairwise_sum_f32` over the same
            // elements either way — behaviour is identical.
            match a.as_slice() {
                Some(s) => make_scalar_f32(pairwise_sum_f32(s)),
                None => {
                    let v: Vec<f32> = a.iter().copied().collect();
                    make_scalar_f32(pairwise_sum_f32(&v))
                }
            }
        }
        Array::Float64(a) => match a.as_slice() {
            Some(s) => make_scalar_f64(pairwise_sum_f64(s)),
            None => {
                let v: Vec<f64> = a.iter().copied().collect();
                make_scalar_f64(pairwise_sum_f64(&v))
            }
        },
        Array::Bool(a) => {
            // numpy: sum(bool) → int64 count of true.
            let mut s: i64 = 0;
            for &v in a.iter() {
                if v {
                    s += 1;
                }
            }
            make_scalar_i64(s)
        }
    })
}

fn sum_axis(arr: &Array, axis: usize) -> Result<Array, NumpyError> {
    let in_shape = arr.shape();
    let out_shape = drop_axis(&in_shape, axis);
    Ok(match arr {
        Array::Int32(a) => {
            let r = a.fold_axis(Axis(axis), 0_i32, |&acc, &v| acc.wrapping_add(v));
            from_vec_i32(r.iter().copied().collect(), out_shape)
        }
        Array::Int64(a) => {
            let r = a.fold_axis(Axis(axis), 0_i64, |&acc, &v| acc.wrapping_add(v));
            from_vec_i64(r.iter().copied().collect(), out_shape)
        }
        Array::Float32(a) => {
            // Pairwise summation per group.
            let mut out: Vec<f32> = Vec::new();
            for lane in a.axis_iter(Axis(axis)) {
                let v: Vec<f32> = lane.iter().copied().collect();
                out.push(pairwise_sum_f32(&v));
            }
            // axis_iter yields lanes orthogonal to axis; we built them in
            // out-shape iteration order. We need to remap into out_shape
            // ordering. The simplest correct approach: fold over each group.
            drop(out);
            let mut data: Vec<f32> = Vec::new();
            // For pairwise correctness we collect along the reduced axis.
            // Use a manual gather: walk every output index in row-major
            // order, gather along axis, sum pairwise, push.
            gather_then_sum_f32(a, axis, &out_shape, &mut data);
            from_vec_f32(data, out_shape)
        }
        Array::Float64(a) => {
            let mut data: Vec<f64> = Vec::new();
            gather_then_sum_f64(a, axis, &out_shape, &mut data);
            from_vec_f64(data, out_shape)
        }
        Array::Bool(a) => {
            // Count true per lane → Int64.
            let r = a.fold_axis(Axis(axis), 0_i64, |&acc, &v| acc + if v { 1 } else { 0 });
            from_vec_i64(r.iter().copied().collect(), out_shape)
        }
    })
}

// ---- gather_then_sum helpers (pairwise per group) -----------------------

/// Walk the output positions and gather along the reduction axis,
/// applying pairwise summation per group. Mirrors numpy's per-axis
/// pairwise behavior.
fn gather_then_sum_f64(a: &ArrayD<f64>, axis: usize, out_shape: &[usize], out: &mut Vec<f64>) {
    let in_shape = a.shape().to_vec();
    let axis_len = in_shape[axis];
    walk_out_positions(out_shape, &mut |out_multi| {
        let mut group: Vec<f64> = Vec::with_capacity(axis_len);
        for j in 0..axis_len {
            let mut full = Vec::with_capacity(in_shape.len());
            let mut k = 0;
            for ax_i in 0..in_shape.len() {
                if ax_i == axis {
                    full.push(j);
                } else {
                    full.push(out_multi[k]);
                    k += 1;
                }
            }
            group.push(a[IxDyn(&full)]);
        }
        out.push(pairwise_sum_f64(&group));
    });
}

fn gather_then_sum_f32(a: &ArrayD<f32>, axis: usize, out_shape: &[usize], out: &mut Vec<f32>) {
    let in_shape = a.shape().to_vec();
    let axis_len = in_shape[axis];
    walk_out_positions(out_shape, &mut |out_multi| {
        let mut group: Vec<f32> = Vec::with_capacity(axis_len);
        for j in 0..axis_len {
            let mut full = Vec::with_capacity(in_shape.len());
            let mut k = 0;
            for ax_i in 0..in_shape.len() {
                if ax_i == axis {
                    full.push(j);
                } else {
                    full.push(out_multi[k]);
                    k += 1;
                }
            }
            group.push(a[IxDyn(&full)]);
        }
        out.push(pairwise_sum_f32(&group));
    });
}

/// Walk every multi-index in row-major order over `shape`, calling
/// `visit` per position. Empty shape yields a single () position.
fn walk_out_positions<F>(shape: &[usize], visit: &mut F)
where
    F: FnMut(&[usize]),
{
    if shape.is_empty() {
        visit(&[]);
        return;
    }
    let mut idx = vec![0_usize; shape.len()];
    loop {
        visit(&idx);
        // Increment row-major — last axis fastest.
        let mut k = shape.len();
        loop {
            if k == 0 {
                return;
            }
            k -= 1;
            idx[k] += 1;
            if idx[k] < shape[k] {
                break;
            }
            idx[k] = 0;
        }
    }
}

// ---- prod ----------------------------------------------------------------

pub fn prod(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError> {
    let shape = arr.shape();
    let ax = normalize_axis(axis, shape.len())?;
    match ax {
        None => prod_all(arr),
        Some(k) => prod_axis(arr, k),
    }
}

fn prod_all(arr: &Array) -> Result<Array, NumpyError> {
    Ok(match arr {
        Array::Int32(a) => {
            let mut p: i32 = 1;
            for &v in a.iter() {
                p = p.wrapping_mul(v);
            }
            make_scalar_i32(p)
        }
        Array::Int64(a) => {
            let mut p: i64 = 1;
            for &v in a.iter() {
                p = p.wrapping_mul(v);
            }
            make_scalar_i64(p)
        }
        Array::Float32(a) => {
            let mut p: f32 = 1.0;
            for &v in a.iter() {
                p *= v;
            }
            make_scalar_f32(p)
        }
        Array::Float64(a) => {
            let mut p: f64 = 1.0;
            for &v in a.iter() {
                p *= v;
            }
            make_scalar_f64(p)
        }
        Array::Bool(a) => {
            // numpy: prod(bool) → int64 (1 if all true, else 0). Per
            // numpy, bool prod is treated as int multiplication.
            let mut p: i64 = 1;
            for &v in a.iter() {
                p *= if v { 1 } else { 0 };
            }
            make_scalar_i64(p)
        }
    })
}

fn prod_axis(arr: &Array, axis: usize) -> Result<Array, NumpyError> {
    let in_shape = arr.shape();
    let out_shape = drop_axis(&in_shape, axis);
    Ok(match arr {
        Array::Int32(a) => {
            let r = a.fold_axis(Axis(axis), 1_i32, |&acc, &v| acc.wrapping_mul(v));
            from_vec_i32(r.iter().copied().collect(), out_shape)
        }
        Array::Int64(a) => {
            let r = a.fold_axis(Axis(axis), 1_i64, |&acc, &v| acc.wrapping_mul(v));
            from_vec_i64(r.iter().copied().collect(), out_shape)
        }
        Array::Float32(a) => {
            let r = a.fold_axis(Axis(axis), 1.0_f32, |&acc, &v| acc * v);
            from_vec_f32(r.iter().copied().collect(), out_shape)
        }
        Array::Float64(a) => {
            let r = a.fold_axis(Axis(axis), 1.0_f64, |&acc, &v| acc * v);
            from_vec_f64(r.iter().copied().collect(), out_shape)
        }
        Array::Bool(a) => {
            let r = a.fold_axis(Axis(axis), 1_i64, |&acc, &v| acc * if v { 1 } else { 0 });
            from_vec_i64(r.iter().copied().collect(), out_shape)
        }
    })
}

// ---- mean ----------------------------------------------------------------

pub fn mean(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError> {
    let shape = arr.shape();
    let ax = normalize_axis(axis, shape.len())?;
    match ax {
        None => mean_all(arr),
        Some(k) => mean_axis(arr, k),
    }
}

fn mean_all(arr: &Array) -> Result<Array, NumpyError> {
    let n = arr.size();
    Ok(match arr {
        Array::Float32(a) => {
            if n == 0 {
                return Ok(make_scalar_f32(f32::NAN));
            }
            // Contiguous fast path (no intermediate Vec): pairwise-sum the
            // backing slice directly when standard-contiguous; fall back to
            // the collect for non-contiguous views (`as_slice()` → `None`).
            // Identical `pairwise_sum_f32` / `n` either way.
            match a.as_slice() {
                Some(s) => make_scalar_f32(pairwise_sum_f32(s) / n as f32),
                None => {
                    let v: Vec<f32> = a.iter().copied().collect();
                    make_scalar_f32(pairwise_sum_f32(&v) / n as f32)
                }
            }
        }
        Array::Float64(a) => {
            if n == 0 {
                return Ok(make_scalar_f64(f64::NAN));
            }
            match a.as_slice() {
                Some(s) => make_scalar_f64(pairwise_sum_f64(s) / n as f64),
                None => {
                    let v: Vec<f64> = a.iter().copied().collect();
                    make_scalar_f64(pairwise_sum_f64(&v) / n as f64)
                }
            }
        }
        // int / bool inputs promote to Float64.
        _ => {
            if n == 0 {
                return Ok(make_scalar_f64(f64::NAN));
            }
            let mut v: Vec<f64> = Vec::with_capacity(n);
            match arr {
                Array::Int32(a) => v.extend(a.iter().map(|&x| x as f64)),
                Array::Int64(a) => v.extend(a.iter().map(|&x| x as f64)),
                Array::Bool(a) => v.extend(a.iter().map(|&x| if x { 1.0 } else { 0.0 })),
                _ => unreachable!(),
            }
            make_scalar_f64(pairwise_sum_f64(&v) / n as f64)
        }
    })
}

fn mean_axis(arr: &Array, axis: usize) -> Result<Array, NumpyError> {
    let in_shape = arr.shape();
    let out_shape = drop_axis(&in_shape, axis);
    let axis_len = in_shape[axis];
    Ok(match arr {
        Array::Float32(a) => {
            let mut data: Vec<f32> = Vec::new();
            gather_then_sum_f32(a, axis, &out_shape, &mut data);
            if axis_len == 0 {
                for d in data.iter_mut() {
                    *d = f32::NAN;
                }
            } else {
                let denom = axis_len as f32;
                for d in data.iter_mut() {
                    *d /= denom;
                }
            }
            from_vec_f32(data, out_shape)
        }
        Array::Float64(a) => {
            let mut data: Vec<f64> = Vec::new();
            gather_then_sum_f64(a, axis, &out_shape, &mut data);
            if axis_len == 0 {
                for d in data.iter_mut() {
                    *d = f64::NAN;
                }
            } else {
                let denom = axis_len as f64;
                for d in data.iter_mut() {
                    *d /= denom;
                }
            }
            from_vec_f64(data, out_shape)
        }
        // int / bool — promote to f64 first.
        _ => {
            let promoted = promote_to_f64(arr);
            mean_axis(&promoted, axis)?
        }
    })
}

fn promote_to_f64(arr: &Array) -> Array {
    match arr {
        Array::Int32(a) => Array::Float64(a.mapv(|v| v as f64)),
        Array::Int64(a) => Array::Float64(a.mapv(|v| v as f64)),
        Array::Float32(a) => Array::Float64(a.mapv(|v| v as f64)),
        Array::Float64(a) => Array::Float64(a.clone()),
        Array::Bool(a) => Array::Float64(a.mapv(|v| if v { 1.0 } else { 0.0 })),
    }
}

// ---- var / std -----------------------------------------------------------

pub fn var(arr: &Array, axis: Option<i64>, ddof: u32) -> Result<Array, NumpyError> {
    let shape = arr.shape();
    let ax = normalize_axis(axis, shape.len())?;
    match ax {
        None => var_all(arr, ddof),
        Some(k) => var_axis(arr, k, ddof),
    }
}

pub fn std(arr: &Array, axis: Option<i64>, ddof: u32) -> Result<Array, NumpyError> {
    let v = var(arr, axis, ddof)?;
    Ok(sqrt_array(&v))
}

fn sqrt_array(arr: &Array) -> Array {
    match arr {
        Array::Float32(a) => {
            Array::Float32(a.mapv(|v| if v.is_nan() { f32::NAN } else { v.sqrt() }))
        }
        Array::Float64(a) => {
            Array::Float64(a.mapv(|v| if v.is_nan() { f64::NAN } else { v.sqrt() }))
        }
        _ => unreachable!("var always returns float"),
    }
}

fn var_all(arr: &Array, ddof: u32) -> Result<Array, NumpyError> {
    let n = arr.size();
    let denom = (n as i64) - (ddof as i64);
    Ok(match arr {
        Array::Float32(a) => {
            if denom <= 0 {
                return Ok(make_scalar_f32(f32::NAN));
            }
            let v: Vec<f32> = a.iter().copied().collect();
            let m = pairwise_sum_f32(&v) / n as f32;
            let sq: Vec<f32> = v.iter().map(|x| (*x - m) * (*x - m)).collect();
            make_scalar_f32(pairwise_sum_f32(&sq) / denom as f32)
        }
        Array::Float64(a) => {
            if denom <= 0 {
                return Ok(make_scalar_f64(f64::NAN));
            }
            let v: Vec<f64> = a.iter().copied().collect();
            let m = pairwise_sum_f64(&v) / n as f64;
            let sq: Vec<f64> = v.iter().map(|x| (*x - m) * (*x - m)).collect();
            make_scalar_f64(pairwise_sum_f64(&sq) / denom as f64)
        }
        // Promote int/bool to f64.
        _ => var_all(&promote_to_f64(arr), ddof)?,
    })
}

fn var_axis(arr: &Array, axis: usize, ddof: u32) -> Result<Array, NumpyError> {
    let in_shape = arr.shape();
    let out_shape = drop_axis(&in_shape, axis);
    let axis_len = in_shape[axis];
    let denom = (axis_len as i64) - (ddof as i64);
    Ok(match arr {
        Array::Float32(a) => {
            let mut out: Vec<f32> = Vec::new();
            walk_out_positions(&out_shape, &mut |out_multi| {
                let mut group: Vec<f32> = Vec::with_capacity(axis_len);
                for j in 0..axis_len {
                    let mut full = Vec::with_capacity(in_shape.len());
                    let mut k = 0;
                    for ax_i in 0..in_shape.len() {
                        if ax_i == axis {
                            full.push(j);
                        } else {
                            full.push(out_multi[k]);
                            k += 1;
                        }
                    }
                    group.push(a[IxDyn(&full)]);
                }
                if denom <= 0 {
                    out.push(f32::NAN);
                } else {
                    let m = pairwise_sum_f32(&group) / axis_len as f32;
                    let sq: Vec<f32> = group.iter().map(|x| (*x - m) * (*x - m)).collect();
                    out.push(pairwise_sum_f32(&sq) / denom as f32);
                }
            });
            from_vec_f32(out, out_shape)
        }
        Array::Float64(a) => {
            let mut out: Vec<f64> = Vec::new();
            walk_out_positions(&out_shape, &mut |out_multi| {
                let mut group: Vec<f64> = Vec::with_capacity(axis_len);
                for j in 0..axis_len {
                    let mut full = Vec::with_capacity(in_shape.len());
                    let mut k = 0;
                    for ax_i in 0..in_shape.len() {
                        if ax_i == axis {
                            full.push(j);
                        } else {
                            full.push(out_multi[k]);
                            k += 1;
                        }
                    }
                    group.push(a[IxDyn(&full)]);
                }
                if denom <= 0 {
                    out.push(f64::NAN);
                } else {
                    let m = pairwise_sum_f64(&group) / axis_len as f64;
                    let sq: Vec<f64> = group.iter().map(|x| (*x - m) * (*x - m)).collect();
                    out.push(pairwise_sum_f64(&sq) / denom as f64);
                }
            });
            from_vec_f64(out, out_shape)
        }
        _ => var_axis(&promote_to_f64(arr), axis, ddof)?,
    })
}

// ---- min / max -----------------------------------------------------------

pub fn min(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError> {
    let shape = arr.shape();
    let ax = normalize_axis(axis, shape.len())?;
    match ax {
        None => min_all(arr),
        Some(k) => min_axis(arr, k),
    }
}

pub fn max(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError> {
    let shape = arr.shape();
    let ax = normalize_axis(axis, shape.len())?;
    match ax {
        None => max_all(arr),
        Some(k) => max_axis(arr, k),
    }
}

fn min_all(arr: &Array) -> Result<Array, NumpyError> {
    if arr.size() == 0 {
        return Err(empty_reduction_error("min"));
    }
    Ok(match arr {
        Array::Int32(a) => {
            let mut m = i32::MAX;
            for &v in a.iter() {
                if v < m {
                    m = v;
                }
            }
            make_scalar_i32(m)
        }
        Array::Int64(a) => {
            let mut m = i64::MAX;
            for &v in a.iter() {
                if v < m {
                    m = v;
                }
            }
            make_scalar_i64(m)
        }
        Array::Float32(a) => {
            // numpy: min propagates NaN — if any element is NaN, result is NaN.
            let mut m: f32 = f32::INFINITY;
            for &v in a.iter() {
                if v.is_nan() {
                    m = f32::NAN;
                    break;
                }
                if v < m {
                    m = v;
                }
            }
            make_scalar_f32(m)
        }
        Array::Float64(a) => {
            let mut m: f64 = f64::INFINITY;
            for &v in a.iter() {
                if v.is_nan() {
                    m = f64::NAN;
                    break;
                }
                if v < m {
                    m = v;
                }
            }
            make_scalar_f64(m)
        }
        Array::Bool(a) => {
            // numpy: min(bool) → bool (false if any false; else true).
            let mut all_true = true;
            for &v in a.iter() {
                if !v {
                    all_true = false;
                    break;
                }
            }
            Array::Bool(ArrayD::from_shape_vec(IxDyn(&[]), vec![all_true]).expect("0-d shape"))
        }
    })
}

fn max_all(arr: &Array) -> Result<Array, NumpyError> {
    if arr.size() == 0 {
        return Err(empty_reduction_error("max"));
    }
    Ok(match arr {
        Array::Int32(a) => {
            let mut m = i32::MIN;
            for &v in a.iter() {
                if v > m {
                    m = v;
                }
            }
            make_scalar_i32(m)
        }
        Array::Int64(a) => {
            let mut m = i64::MIN;
            for &v in a.iter() {
                if v > m {
                    m = v;
                }
            }
            make_scalar_i64(m)
        }
        Array::Float32(a) => {
            let mut m: f32 = f32::NEG_INFINITY;
            for &v in a.iter() {
                if v.is_nan() {
                    m = f32::NAN;
                    break;
                }
                if v > m {
                    m = v;
                }
            }
            make_scalar_f32(m)
        }
        Array::Float64(a) => {
            let mut m: f64 = f64::NEG_INFINITY;
            for &v in a.iter() {
                if v.is_nan() {
                    m = f64::NAN;
                    break;
                }
                if v > m {
                    m = v;
                }
            }
            make_scalar_f64(m)
        }
        Array::Bool(a) => {
            let mut any_true = false;
            for &v in a.iter() {
                if v {
                    any_true = true;
                    break;
                }
            }
            Array::Bool(ArrayD::from_shape_vec(IxDyn(&[]), vec![any_true]).expect("0-d shape"))
        }
    })
}

fn min_axis(arr: &Array, axis: usize) -> Result<Array, NumpyError> {
    let in_shape = arr.shape();
    let axis_len = in_shape[axis];
    if axis_len == 0 {
        return Err(empty_reduction_error("min"));
    }
    let out_shape = drop_axis(&in_shape, axis);
    Ok(match arr {
        Array::Int32(a) => {
            let r = a.fold_axis(
                Axis(axis),
                i32::MAX,
                |&acc, &v| if v < acc { v } else { acc },
            );
            from_vec_i32(r.iter().copied().collect(), out_shape)
        }
        Array::Int64(a) => {
            let r = a.fold_axis(
                Axis(axis),
                i64::MAX,
                |&acc, &v| if v < acc { v } else { acc },
            );
            from_vec_i64(r.iter().copied().collect(), out_shape)
        }
        Array::Float32(a) => {
            let r = a.fold_axis(Axis(axis), f32::INFINITY, |&acc, &v| {
                if acc.is_nan() || v.is_nan() {
                    f32::NAN
                } else if v < acc {
                    v
                } else {
                    acc
                }
            });
            from_vec_f32(r.iter().copied().collect(), out_shape)
        }
        Array::Float64(a) => {
            let r = a.fold_axis(Axis(axis), f64::INFINITY, |&acc, &v| {
                if acc.is_nan() || v.is_nan() {
                    f64::NAN
                } else if v < acc {
                    v
                } else {
                    acc
                }
            });
            from_vec_f64(r.iter().copied().collect(), out_shape)
        }
        Array::Bool(a) => {
            let r = a.fold_axis(Axis(axis), true, |&acc, &v| acc && v);
            Array::Bool(
                ArrayD::from_shape_vec(IxDyn(&out_shape), r.iter().copied().collect())
                    .expect("shape * len"),
            )
        }
    })
}

fn max_axis(arr: &Array, axis: usize) -> Result<Array, NumpyError> {
    let in_shape = arr.shape();
    let axis_len = in_shape[axis];
    if axis_len == 0 {
        return Err(empty_reduction_error("max"));
    }
    let out_shape = drop_axis(&in_shape, axis);
    Ok(match arr {
        Array::Int32(a) => {
            let r = a.fold_axis(
                Axis(axis),
                i32::MIN,
                |&acc, &v| if v > acc { v } else { acc },
            );
            from_vec_i32(r.iter().copied().collect(), out_shape)
        }
        Array::Int64(a) => {
            let r = a.fold_axis(
                Axis(axis),
                i64::MIN,
                |&acc, &v| if v > acc { v } else { acc },
            );
            from_vec_i64(r.iter().copied().collect(), out_shape)
        }
        Array::Float32(a) => {
            let r = a.fold_axis(Axis(axis), f32::NEG_INFINITY, |&acc, &v| {
                if acc.is_nan() || v.is_nan() {
                    f32::NAN
                } else if v > acc {
                    v
                } else {
                    acc
                }
            });
            from_vec_f32(r.iter().copied().collect(), out_shape)
        }
        Array::Float64(a) => {
            let r = a.fold_axis(Axis(axis), f64::NEG_INFINITY, |&acc, &v| {
                if acc.is_nan() || v.is_nan() {
                    f64::NAN
                } else if v > acc {
                    v
                } else {
                    acc
                }
            });
            from_vec_f64(r.iter().copied().collect(), out_shape)
        }
        Array::Bool(a) => {
            let r = a.fold_axis(Axis(axis), false, |&acc, &v| acc || v);
            Array::Bool(
                ArrayD::from_shape_vec(IxDyn(&out_shape), r.iter().copied().collect())
                    .expect("shape * len"),
            )
        }
    })
}

// ---- argmin / argmax -----------------------------------------------------

pub fn argmin(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError> {
    let shape = arr.shape();
    let ax = normalize_axis(axis, shape.len())?;
    match ax {
        None => argmin_all(arr),
        Some(k) => argmin_axis(arr, k),
    }
}

pub fn argmax(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError> {
    let shape = arr.shape();
    let ax = normalize_axis(axis, shape.len())?;
    match ax {
        None => argmax_all(arr),
        Some(k) => argmax_axis(arr, k),
    }
}

fn argmin_all(arr: &Array) -> Result<Array, NumpyError> {
    if arr.size() == 0 {
        return Err(empty_reduction_error("argmin"));
    }
    let i = match arr {
        Array::Int32(a) => arg_extreme_iter_i32(a.iter().copied(), false),
        Array::Int64(a) => arg_extreme_iter_i64(a.iter().copied(), false),
        Array::Float32(a) => arg_extreme_iter_f32(a.iter().copied(), false),
        Array::Float64(a) => arg_extreme_iter_f64(a.iter().copied(), false),
        Array::Bool(a) => arg_extreme_iter_bool(a.iter().copied(), false),
    };
    Ok(make_scalar_i64(i as i64))
}

fn argmax_all(arr: &Array) -> Result<Array, NumpyError> {
    if arr.size() == 0 {
        return Err(empty_reduction_error("argmax"));
    }
    let i = match arr {
        Array::Int32(a) => arg_extreme_iter_i32(a.iter().copied(), true),
        Array::Int64(a) => arg_extreme_iter_i64(a.iter().copied(), true),
        Array::Float32(a) => arg_extreme_iter_f32(a.iter().copied(), true),
        Array::Float64(a) => arg_extreme_iter_f64(a.iter().copied(), true),
        Array::Bool(a) => arg_extreme_iter_bool(a.iter().copied(), true),
    };
    Ok(make_scalar_i64(i as i64))
}

fn arg_extreme_iter_i32<I: Iterator<Item = i32>>(it: I, want_max: bool) -> usize {
    let mut best_i: usize = 0;
    let mut best_v: Option<i32> = None;
    for (i, v) in it.enumerate() {
        match best_v {
            None => {
                best_v = Some(v);
                best_i = i;
            }
            Some(bv) => {
                let take = if want_max { v > bv } else { v < bv };
                if take {
                    best_v = Some(v);
                    best_i = i;
                }
            }
        }
    }
    best_i
}

fn arg_extreme_iter_i64<I: Iterator<Item = i64>>(it: I, want_max: bool) -> usize {
    let mut best_i: usize = 0;
    let mut best_v: Option<i64> = None;
    for (i, v) in it.enumerate() {
        match best_v {
            None => {
                best_v = Some(v);
                best_i = i;
            }
            Some(bv) => {
                let take = if want_max { v > bv } else { v < bv };
                if take {
                    best_v = Some(v);
                    best_i = i;
                }
            }
        }
    }
    best_i
}

fn arg_extreme_iter_f32<I: Iterator<Item = f32>>(it: I, want_max: bool) -> usize {
    let mut best_i: usize = 0;
    let mut best_v: Option<f32> = None;
    for (i, v) in it.enumerate() {
        // numpy: NaN is treated as the maximum value for argmin/argmax
        // ordering — first NaN wins.
        if v.is_nan() {
            // For both argmin and argmax: first NaN is returned by numpy.
            return i;
        }
        match best_v {
            None => {
                best_v = Some(v);
                best_i = i;
            }
            Some(bv) => {
                let take = if want_max { v > bv } else { v < bv };
                if take {
                    best_v = Some(v);
                    best_i = i;
                }
            }
        }
    }
    best_i
}

fn arg_extreme_iter_f64<I: Iterator<Item = f64>>(it: I, want_max: bool) -> usize {
    let mut best_i: usize = 0;
    let mut best_v: Option<f64> = None;
    for (i, v) in it.enumerate() {
        if v.is_nan() {
            return i;
        }
        match best_v {
            None => {
                best_v = Some(v);
                best_i = i;
            }
            Some(bv) => {
                let take = if want_max { v > bv } else { v < bv };
                if take {
                    best_v = Some(v);
                    best_i = i;
                }
            }
        }
    }
    best_i
}

fn arg_extreme_iter_bool<I: Iterator<Item = bool>>(it: I, want_max: bool) -> usize {
    let mut best_i: usize = 0;
    let mut best_v: Option<bool> = None;
    for (i, v) in it.enumerate() {
        let val_i = i32::from(v);
        match best_v {
            None => {
                best_v = Some(v);
                best_i = i;
            }
            Some(bv) => {
                let bv_i = i32::from(bv);
                let take = if want_max { val_i > bv_i } else { val_i < bv_i };
                if take {
                    best_v = Some(v);
                    best_i = i;
                }
            }
        }
    }
    best_i
}

fn argmin_axis(arr: &Array, axis: usize) -> Result<Array, NumpyError> {
    arg_axis_impl(arr, axis, false)
}

fn argmax_axis(arr: &Array, axis: usize) -> Result<Array, NumpyError> {
    arg_axis_impl(arr, axis, true)
}

fn arg_axis_impl(arr: &Array, axis: usize, want_max: bool) -> Result<Array, NumpyError> {
    let in_shape = arr.shape();
    let axis_len = in_shape[axis];
    let kind = if want_max { "argmax" } else { "argmin" };
    if axis_len == 0 {
        return Err(empty_reduction_error(kind));
    }
    let out_shape = drop_axis(&in_shape, axis);
    let mut out: Vec<i64> = Vec::new();
    walk_out_positions(&out_shape, &mut |out_multi| {
        let pick = match arr {
            Array::Int32(a) => arg_extreme_iter_i32(
                (0..axis_len).map(|j| {
                    let mut full = Vec::with_capacity(in_shape.len());
                    let mut k = 0;
                    for ax_i in 0..in_shape.len() {
                        if ax_i == axis {
                            full.push(j);
                        } else {
                            full.push(out_multi[k]);
                            k += 1;
                        }
                    }
                    a[IxDyn(&full)]
                }),
                want_max,
            ),
            Array::Int64(a) => arg_extreme_iter_i64(
                (0..axis_len).map(|j| {
                    let mut full = Vec::with_capacity(in_shape.len());
                    let mut k = 0;
                    for ax_i in 0..in_shape.len() {
                        if ax_i == axis {
                            full.push(j);
                        } else {
                            full.push(out_multi[k]);
                            k += 1;
                        }
                    }
                    a[IxDyn(&full)]
                }),
                want_max,
            ),
            Array::Float32(a) => arg_extreme_iter_f32(
                (0..axis_len).map(|j| {
                    let mut full = Vec::with_capacity(in_shape.len());
                    let mut k = 0;
                    for ax_i in 0..in_shape.len() {
                        if ax_i == axis {
                            full.push(j);
                        } else {
                            full.push(out_multi[k]);
                            k += 1;
                        }
                    }
                    a[IxDyn(&full)]
                }),
                want_max,
            ),
            Array::Float64(a) => arg_extreme_iter_f64(
                (0..axis_len).map(|j| {
                    let mut full = Vec::with_capacity(in_shape.len());
                    let mut k = 0;
                    for ax_i in 0..in_shape.len() {
                        if ax_i == axis {
                            full.push(j);
                        } else {
                            full.push(out_multi[k]);
                            k += 1;
                        }
                    }
                    a[IxDyn(&full)]
                }),
                want_max,
            ),
            Array::Bool(a) => arg_extreme_iter_bool(
                (0..axis_len).map(|j| {
                    let mut full = Vec::with_capacity(in_shape.len());
                    let mut k = 0;
                    for ax_i in 0..in_shape.len() {
                        if ax_i == axis {
                            full.push(j);
                        } else {
                            full.push(out_multi[k]);
                            k += 1;
                        }
                    }
                    a[IxDyn(&full)]
                }),
                want_max,
            ),
        };
        out.push(pick as i64);
    });
    Ok(from_vec_i64(out, out_shape))
}

// ---- Error helpers -------------------------------------------------------

fn empty_reduction_error(op: &str) -> NumpyError {
    NumpyError {
        kind: NumpyErrorKind::ReductionEmptyArray,
        message: format!("zero-size array to reduction operation {op}"),
    }
}

// ---- Tests ---------------------------------------------------------------

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
    #![allow(clippy::imprecise_flops)]
    #![allow(clippy::suboptimal_flops)]
    #![allow(clippy::excessive_precision)]
    use super::*;
    use crate::{array_bool, array_f64, array_i32, array_i64};

    #[test]
    fn pairwise_sum_empty() {
        assert_eq!(pairwise_sum_f64(&[]), 0.0);
        assert_eq!(pairwise_sum_f32(&[]), 0.0);
    }

    #[test]
    fn pairwise_sum_small() {
        let v: Vec<f64> = (1..=5).map(|x| x as f64).collect();
        assert_eq!(pairwise_sum_f64(&v), 15.0);
    }

    #[test]
    fn pairwise_sum_million_tiny_floats_matches_numpy_floor() {
        // Per ADR-0016 §"Pairwise summation accuracy" / reduce_corpus
        // test: sum 10^6 floats of magnitude 1e-9. Naive accumulator
        // accumulates O(n*eps) error; pairwise floor matches numpy.
        let v: Vec<f64> = (0..1_000_000).map(|_| 1e-9_f64).collect();
        let s = pairwise_sum_f64(&v);
        let expected = 1e-3_f64;
        let rel_err = (s - expected).abs() / expected;
        assert!(
            rel_err < 1e-12,
            "pairwise relative error {rel_err} too high"
        );
    }

    #[test]
    fn sum_all_int64() {
        let a = array_i64(&[1, 2, 3, 4], &[4]).unwrap();
        let r = sum(&a, None).unwrap();
        let Array::Int64(arr) = r else {
            panic!("expected Int64");
        };
        assert_eq!(arr[IxDyn(&[])], 10);
    }

    #[test]
    fn sum_all_f64_contiguous_via_as_slice() {
        // Pins the F74 collect-elimination WIN path directly: a standard
        // (row-major) contiguous f64 array has `as_slice() == Some`, so
        // `sum_all` pairwise-sums the slice directly (the fast path), NOT the
        // non-contiguous collect fallback. Guards the WIN'd arm against a
        // future refactor — the audit flagged it as covered only transitively
        // (via mean_all + the bench SAME_VALUE_GUARD) before this test.
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0], &[5]).unwrap();
        let r = sum(&a, None).unwrap();
        let Array::Float64(arr) = r else {
            panic!("expected Float64");
        };
        assert_eq!(arr[IxDyn(&[])], 15.0);
    }

    #[test]
    fn sum_axis0_2x3() {
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
        let r = sum(&a, Some(0)).unwrap();
        let Array::Float64(arr) = r else {
            panic!("expected Float64");
        };
        assert_eq!(arr.shape(), &[3]);
        assert_eq!(
            arr.iter().copied().collect::<Vec<f64>>(),
            vec![5.0, 7.0, 9.0]
        );
    }

    #[test]
    fn min_empty_errs() {
        let a = array_i64(&[], &[0]).unwrap();
        let err = min(&a, None).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
    }

    #[test]
    fn argmin_first_occurrence() {
        let a = array_i64(&[3, 1, 2, 1, 5], &[5]).unwrap();
        let r = argmin(&a, None).unwrap();
        let Array::Int64(arr) = r else {
            panic!("expected Int64");
        };
        assert_eq!(arr[IxDyn(&[])], 1); // first occurrence of min
    }

    #[test]
    fn mean_empty_is_nan() {
        let a = array_f64(&[], &[0]).unwrap();
        let r = mean(&a, None).unwrap();
        let Array::Float64(arr) = r else {
            panic!("expected Float64");
        };
        assert!(arr[IxDyn(&[])].is_nan());
    }

    #[test]
    fn var_ddof_clamp_to_nan() {
        let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
        let r = var(&a, None, 5).unwrap(); // ddof > N
        let Array::Float64(arr) = r else {
            panic!("expected Float64");
        };
        assert!(arr[IxDyn(&[])].is_nan());
    }

    #[test]
    fn axis_negative_normalizes() {
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
        let r = sum(&a, Some(-1)).unwrap();
        let Array::Float64(arr) = r else {
            panic!("expected Float64");
        };
        assert_eq!(arr.shape(), &[2]);
    }

    #[test]
    fn axis_out_of_bounds_errs() {
        let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
        let err = sum(&a, Some(5)).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::IndexError);
    }

    #[test]
    fn sum_bool_returns_int64_count() {
        let a = array_bool(&[true, false, true, true], &[4]).unwrap();
        let r = sum(&a, None).unwrap();
        let Array::Int64(arr) = r else {
            panic!("expected Int64");
        };
        assert_eq!(arr[IxDyn(&[])], 3);
    }

    #[test]
    fn prod_empty_is_one() {
        let a = array_i64(&[], &[0]).unwrap();
        let r = prod(&a, None).unwrap();
        let Array::Int64(arr) = r else {
            panic!("expected Int64");
        };
        assert_eq!(arr[IxDyn(&[])], 1);
    }

    #[test]
    fn sum_empty_is_zero() {
        let a = array_i32(&[], &[0]).unwrap();
        let r = sum(&a, None).unwrap();
        let Array::Int32(arr) = r else {
            panic!("expected Int32");
        };
        assert_eq!(arr[IxDyn(&[])], 0);
    }
}
