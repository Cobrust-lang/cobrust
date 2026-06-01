//! Stream W P0 增量 — scalar aggregate reductions (`mean` / `median`
//! / `std` / `var`).
//!
//! These free functions wrap the existing `reduce::*` machinery
//! (`mean / std / var` on `Array`) and add `median` (not in the
//! current `reduce` surface). All four reduce the entire buffer to a
//! single scalar `f64`, matching the LLM-training-data shape of
//! `np.mean(a)`, `np.median(a)`, `np.std(a)`, `np.var(a)` 0-arg
//! invocations per §2.5.
//!
//! The scalar return is intentional: the `.cb`-side `f64` value is
//! `print()`-able and f-string-formattable today, while shaped
//! `Array` returns demand a handle-typed result. Same surface
//! discipline as `coil.print_buffer(b) -> i64` — first proof picks the
//! value shape `.cb` can immediately consume.

// File-level allows mirror reduce.rs (auto-generated brethren). The
// cast / float / impl-doc lints fire on intrinsically-correct numpy
// arithmetic shapes (i32→f64 promotion, f64 NaN comparison, scalar
// extraction). Test-suite unwraps are scoped to `#[cfg(test)]` via the
// inner `mod tests` allow.
//
// `unnecessary_wraps`: every aggregate in this module returns
// `Result<f64, NumpyError>` for a UNIFORM cabi-shim ABI — every
// `__cobrust_coil_<agg>` shim does `<agg>_scalar(a).unwrap_or(NAN)`,
// so a member that is infallible TODAY (`ptp`/`nansum`/`nanmean`/
// `nanstd`/`percentile`, computed directly with no `?`/`Err`) keeps the
// `Result` wrapper so adding a fallible guard later (e.g. an
// integer-overflow check) is non-breaking for the shims. The fallible
// members (`mean`/`std`/`var`, which forward `reduce::*` errors via `?`)
// share the exact signature. Deliberate API-uniformity choice, not
// dead error handling.
#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::manual_midpoint,
    clippy::missing_panics_doc,
    clippy::redundant_closure_for_method_calls,
    clippy::unnecessary_wraps
)]

use crate::array::Array;
use crate::error::{NumpyError, NumpyErrorKind};
use crate::reduce::{mean as reduce_mean, std as reduce_std, var as reduce_var};

/// Convert a 0-d / 1-elem `Array` reduction result to `f64`. Helper
/// shared by every aggregate wrapper below.
fn scalar_to_f64(arr: &Array) -> f64 {
    match arr {
        Array::Float64(a) => a.iter().next().copied().unwrap_or(f64::NAN),
        Array::Float32(a) => a.iter().next().copied().map_or(f64::NAN, f64::from),
        Array::Int64(a) => a.iter().next().copied().map_or(f64::NAN, |v| v as f64),
        Array::Int32(a) => a.iter().next().copied().map_or(f64::NAN, f64::from),
        Array::Bool(a) => a
            .iter()
            .next()
            .copied()
            .map_or(f64::NAN, |v| if v { 1.0 } else { 0.0 }),
    }
}

/// Flatten any-dtype `Array` to an owned `Vec<f64>`, promoting integer /
/// bool lanes to `f64` (numpy's default-float promotion shape on
/// `np.ptp` / `np.percentile` / the `nan*` reducers). Float lanes pass
/// through verbatim (NaN preserved — the `nan*` reducers filter it
/// downstream, `ptp` / `percentile` propagate it).
fn to_f64_vec(a: &Array) -> Vec<f64> {
    match a {
        Array::Float64(x) => x.iter().copied().collect(),
        Array::Float32(x) => x.iter().map(|&v| f64::from(v)).collect(),
        Array::Int32(x) => x.iter().map(|&v| f64::from(v)).collect(),
        Array::Int64(x) => x.iter().map(|&v| v as f64).collect(),
        Array::Bool(x) => x.iter().map(|&v| if v { 1.0 } else { 0.0 }).collect(),
    }
}

/// `coil.mean(a) -> f64` — arithmetic mean of every element.
///
/// numpy semantics: empty input yields `NaN`. Integer / bool inputs
/// promote to `f64` first (matches numpy's default-float promotion
/// behavior on `np.mean`).
///
/// # Errors
///
/// Propagates `NumpyError` from the underlying `reduce::mean`; the
/// reduce-all path is infallible in practice but the wrapper keeps
/// the Result for ABI uniformity with `median`/`std`/`var`.
pub fn mean_scalar(a: &Array) -> Result<f64, NumpyError> {
    let r = reduce_mean(a, None)?;
    Ok(scalar_to_f64(&r))
}

/// `coil.median(a) -> f64` — order statistic.
///
/// numpy semantics: even-length → average of the two middle elements;
/// odd-length → exact middle. Empty input yields `NaN` (numpy raises;
/// we degrade to NaN to keep the C-ABI shim panic-free and match the
/// rest of the aggregate family's empty-input contract).
///
/// # Errors
///
/// Currently infallible — wrapped in `Result` for API uniformity with
/// the other aggregates so adding e.g. integer-overflow guards later
/// is non-breaking.
pub fn median_scalar(a: &Array) -> Result<f64, NumpyError> {
    let n = a.size();
    if n == 0 {
        return Ok(f64::NAN);
    }
    // Promote to f64 + flatten. Integer inputs promote without loss
    // for the i32 lane and with documented precision drift for i64
    // beyond 2^53 (same caveat numpy carries on `np.median(int64)`).
    let mut values: Vec<f64> = match a {
        Array::Float64(x) => x.iter().copied().collect(),
        Array::Float32(x) => x.iter().map(|&v| f64::from(v)).collect(),
        Array::Int32(x) => x.iter().map(|&v| f64::from(v)).collect(),
        Array::Int64(x) => x.iter().map(|&v| v as f64).collect(),
        Array::Bool(x) => x.iter().map(|&v| if v { 1.0 } else { 0.0 }).collect(),
    };
    // total_cmp keeps NaN ordering stable so the median definition is
    // well-defined under f64. If any element is NaN, numpy yields NaN;
    // mirror that by short-circuiting.
    if values.iter().any(|v| v.is_nan()) {
        return Ok(f64::NAN);
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let mid = n / 2;
    if n.is_multiple_of(2) {
        Ok((values[mid - 1] + values[mid]) / 2.0)
    } else {
        Ok(values[mid])
    }
}

/// `coil.std(a) -> f64` — population standard deviation (ddof=0).
///
/// # Errors
///
/// Propagates `NumpyError` from the underlying `reduce::std`.
pub fn std_scalar(a: &Array) -> Result<f64, NumpyError> {
    let r = reduce_std(a, None, 0)?;
    Ok(scalar_to_f64(&r))
}

/// `coil.var(a) -> f64` — population variance (ddof=0).
///
/// # Errors
///
/// Propagates `NumpyError` from the underlying `reduce::var`.
pub fn var_scalar(a: &Array) -> Result<f64, NumpyError> {
    let r = reduce_var(a, None, 0)?;
    Ok(scalar_to_f64(&r))
}

// ---- #145 statistics gap-closure (2026-06-01) ----------------------------
// NaN-aware + spread scalar aggregates extending the mean/median/std/var
// family. All reduce the whole buffer to one `f64` on the proven
// `coil_agg_ty` ABI (`ptp` / `nanmean` / `nansum` / `nanstd`), plus
// `percentile` on a new `(Buffer, f64) -> f64` ABI. Differential-checked
// vs numpy 2.0.2 (`/usr/bin/python3` oracle) — see the unit tests' literal
// oracle values.

/// `coil.ptp(a) -> f64` — peak-to-peak, i.e. `max(a) - min(a)`.
///
/// numpy semantics (`np.ptp`): the range of the data. A single-element
/// input yields `0.0`. Any NaN element propagates to `NaN` (numpy's
/// `max`/`min` are NaN-propagating). Integer / bool inputs promote to
/// `f64` (the scalar return contract; numpy keeps the integer dtype but
/// the *value* is identical). Empty input yields `NaN` (numpy raises on
/// `np.ptp([])`; we degrade to `NaN` to keep the C-ABI shim panic-free,
/// matching the rest of the family's empty-input contract).
///
/// # Errors
///
/// Currently infallible — wrapped in `Result` for API uniformity.
pub fn ptp_scalar(a: &Array) -> Result<f64, NumpyError> {
    if a.size() == 0 {
        return Ok(f64::NAN);
    }
    let values = to_f64_vec(a);
    // NaN-propagating min/max: a single NaN makes the whole range NaN,
    // exactly like numpy's `np.ptp` (which calls the propagating
    // `np.amax` / `np.amin`).
    if values.iter().any(|v| v.is_nan()) {
        return Ok(f64::NAN);
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &v in &values {
        if v < lo {
            lo = v;
        }
        if v > hi {
            hi = v;
        }
    }
    Ok(hi - lo)
}

/// `coil.nansum(a) -> f64` — sum of elements, treating NaN as zero.
///
/// numpy semantics (`np.nansum`): NaN entries are skipped (contribute
/// `0`). An all-NaN input (or empty input) yields `0.0` (numpy returns
/// `0.0` for `np.nansum` of an all-NaN array — it does NOT return NaN).
/// Integer / bool inputs promote to `f64`.
///
/// # Errors
///
/// Currently infallible — wrapped in `Result` for API uniformity.
pub fn nansum_scalar(a: &Array) -> Result<f64, NumpyError> {
    let mut acc = 0.0_f64;
    for v in to_f64_vec(a) {
        if !v.is_nan() {
            acc += v;
        }
    }
    Ok(acc)
}

/// `coil.nanmean(a) -> f64` — arithmetic mean ignoring NaN.
///
/// numpy semantics (`np.nanmean`): the mean over the non-NaN elements
/// only (denominator = count of non-NaN). An all-NaN input (or empty
/// input) yields `NaN` (and numpy emits a RuntimeWarning, which we do
/// not — the value matches). Integer / bool inputs promote to `f64`.
///
/// # Errors
///
/// Currently infallible — wrapped in `Result` for API uniformity.
pub fn nanmean_scalar(a: &Array) -> Result<f64, NumpyError> {
    let mut acc = 0.0_f64;
    let mut count = 0_usize;
    for v in to_f64_vec(a) {
        if !v.is_nan() {
            acc += v;
            count += 1;
        }
    }
    if count == 0 {
        return Ok(f64::NAN);
    }
    Ok(acc / count as f64)
}

/// `coil.nanstd(a) -> f64` — population standard deviation (ddof=0)
/// ignoring NaN.
///
/// numpy semantics (`np.nanstd`): the population std over the non-NaN
/// elements only (mean over non-NaN, variance over non-NaN with the
/// non-NaN count as denominator, then sqrt). An all-NaN input (or empty
/// input) yields `NaN`. A single non-NaN element yields `0.0`. Integer /
/// bool inputs promote to `f64`.
///
/// # Errors
///
/// Currently infallible — wrapped in `Result` for API uniformity.
pub fn nanstd_scalar(a: &Array) -> Result<f64, NumpyError> {
    let finite: Vec<f64> = to_f64_vec(a).into_iter().filter(|v| !v.is_nan()).collect();
    let n = finite.len();
    if n == 0 {
        return Ok(f64::NAN);
    }
    let mean = finite.iter().sum::<f64>() / n as f64;
    let var = finite.iter().map(|&v| (v - mean) * (v - mean)).sum::<f64>() / n as f64;
    Ok(var.sqrt())
}

/// `coil.percentile(a, q) -> f64` — the `q`-th percentile (`q` in
/// `[0, 100]`) using numpy's default `linear` interpolation.
///
/// numpy semantics (`np.percentile(a, q)`, `method="linear"`): sort the
/// data, compute the virtual fractional index `pos = (n - 1) * q / 100`,
/// and linearly interpolate between the two neighbouring order
/// statistics: `lo + frac * (hi - lo)` where `lo = sorted[floor(pos)]`,
/// `hi = sorted[ceil(pos)]`, `frac = pos - floor(pos)`. `q = 0` returns
/// the min, `q = 100` the max. A single-element input returns that
/// element for any `q`. Any NaN element propagates to `NaN` (numpy's
/// plain `percentile` is NaN-propagating; `nanpercentile` is the
/// NaN-skipping variant, deliberately NOT in this batch). Empty input
/// yields `NaN`. `q` is clamped to `[0, 100]` (numpy raises out of
/// range; we clamp to keep the shim panic-free). Integer / bool inputs
/// promote to `f64`.
///
/// # Errors
///
/// Currently infallible — wrapped in `Result` for API uniformity.
pub fn percentile_scalar(a: &Array, q: f64) -> Result<f64, NumpyError> {
    let n = a.size();
    if n == 0 {
        return Ok(f64::NAN);
    }
    let mut values = to_f64_vec(a);
    // NaN-propagating, like numpy's `np.percentile` (the non-`nan` form).
    if values.iter().any(|v| v.is_nan()) || q.is_nan() {
        return Ok(f64::NAN);
    }
    // Clamp out-of-range q (numpy raises; we clamp to stay panic-free).
    let q = q.clamp(0.0, 100.0);
    // total_cmp gives a total order over the (now NaN-free) f64s.
    values.sort_by(|x, y| x.total_cmp(y));
    if n == 1 {
        return Ok(values[0]);
    }
    // Virtual fractional index into the sorted data (`linear` method).
    let pos = (n - 1) as f64 * (q / 100.0);
    let lo_idx = pos.floor() as usize;
    let hi_idx = pos.ceil() as usize;
    let frac = pos - pos.floor();
    let lo = values[lo_idx];
    let hi = values[hi_idx];
    Ok(lo + frac * (hi - lo))
}

/// `coil.split(a, n) -> Buffer` — first-proof split.
///
/// numpy's `np.split(arr, n)` returns a Python list of `n` sub-arrays.
/// The Cobrust handle surface does not yet model list-of-handle (per
/// ADR-0072 §"coil deep operator/index"); the first proof therefore
/// returns the FIRST of the `n` chunks. This is sufficient to prove
/// the chain handles split arithmetic (chunk size = ceil(len/n)) and
/// the buffer-drop discipline.
///
/// # Errors
///
/// `ShapeMismatch` when `n <= 0`.
pub fn split_first_chunk(a: &Array, n: i64) -> Result<Array, NumpyError> {
    if n <= 0 {
        return Err(NumpyError {
            kind: NumpyErrorKind::ShapeMismatch,
            message: format!("split: number of sections {n} must be positive"),
        });
    }
    // SAFETY of `as usize`: we just checked n > 0.
    let n_usize = n as usize;
    let total = a.size();
    if total == 0 {
        // Empty input → empty first chunk.
        return Ok(Array::Float64(
            ndarray::ArrayD::from_shape_vec(ndarray::IxDyn(&[0]), Vec::new()).expect("empty shape"),
        ));
    }
    // numpy split uses floor(total / n) chunks with the remainder
    // distributed across the first `total % n` chunks (`array_split`)
    // OR errors when `total % n != 0` (`split`). We follow
    // `array_split`'s permissive shape (first chunk gets the
    // remainder) so the first proof always succeeds — matches the
    // user-pace "shipping > strict semantics" Stream W intent.
    let base = total / n_usize;
    let rem = total % n_usize;
    let first_len = base + if rem > 0 { 1 } else { 0 };
    let flat: Vec<f64> = match a {
        Array::Float64(x) => x.iter().take(first_len).copied().collect(),
        Array::Float32(x) => x.iter().take(first_len).map(|&v| f64::from(v)).collect(),
        Array::Int32(x) => x.iter().take(first_len).map(|&v| f64::from(v)).collect(),
        Array::Int64(x) => x.iter().take(first_len).map(|&v| v as f64).collect(),
        Array::Bool(x) => x
            .iter()
            .take(first_len)
            .map(|&v| if v { 1.0 } else { 0.0 })
            .collect(),
    };
    Ok(Array::Float64(
        ndarray::ArrayD::from_shape_vec(ndarray::IxDyn(&[first_len]), flat)
            .expect("shape * len match"),
    ))
}

// ---- #145 gap-closure BATCH 5 (2026-06-01) -------------------------------
// Scalar-returning argmin / argmax (→ i64) + any / all (→ bool), the cabi
// helpers the `__cobrust_coil_{argmin,argmax,any,all}` shims call. They
// wrap the no-axis `reduce::{argmin_flat,argmax_flat,any,all}` kernels,
// adapting the return type to the scalar C-ABI shape:
//   - argmin / argmax → `i64` (the flat C-order index). The kernel's
//     empty-input `Err` is PROPAGATED via `?` so the shim can map it to a
//     clean `coil_panic` (numpy raises `ValueError`); a `usize` index is
//     cast to `i64` (always non-negative, well under `i64::MAX`).
//   - any / all → `bool` (infallible; wrapped in `Result` for ABI
//     uniformity with the rest of this module, matching `nansum`/`ptp`).

/// `coil.argmin(a) -> i64` — the FLAT (C-order) index of the first
/// occurrence of the minimum. NaN propagates (its index is returned).
///
/// # Errors
///
/// `ReductionEmptyArray` on an empty input (numpy raises `ValueError`);
/// the C-ABI shim maps this to a clean `coil_panic`.
pub fn argmin_scalar(a: &Array) -> Result<i64, NumpyError> {
    Ok(crate::reduce::argmin_flat(a)? as i64)
}

/// `coil.argmax(a) -> i64` — the FLAT (C-order) index of the first
/// occurrence of the maximum. NaN propagates.
///
/// # Errors
///
/// `ReductionEmptyArray` on an empty input.
pub fn argmax_scalar(a: &Array) -> Result<i64, NumpyError> {
    Ok(crate::reduce::argmax_flat(a)? as i64)
}

/// `coil.any(a) -> bool` — `True` iff ANY element is truthy. `any([]) ==
/// False`. `NaN` is truthy (numpy).
///
/// # Errors
///
/// Currently infallible — wrapped in `Result` for API uniformity with the
/// rest of this module.
pub fn any_scalar(a: &Array) -> Result<bool, NumpyError> {
    Ok(crate::reduce::any(a))
}

/// `coil.all(a) -> bool` — `True` iff ALL elements are truthy. `all([]) ==
/// True` (vacuous truth). `NaN` is truthy (numpy).
///
/// # Errors
///
/// Currently infallible — wrapped in `Result` for API uniformity.
pub fn all_scalar(a: &Array) -> Result<bool, NumpyError> {
    Ok(crate::reduce::all(a))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::constructors::{array_f64, array_i64};

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps || (a.is_nan() && b.is_nan())
    }

    #[test]
    fn mean_basic() {
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
        let m = mean_scalar(&a).unwrap();
        assert!(approx(m, 2.5, 1e-12), "mean got {m}");
    }

    #[test]
    fn mean_integer_promotion() {
        let a = array_i64(&[1, 2, 3, 4, 5], &[5]).unwrap();
        let m = mean_scalar(&a).unwrap();
        assert!(approx(m, 3.0, 1e-12), "mean got {m}");
    }

    #[test]
    fn mean_empty_yields_nan() {
        let a = array_f64(&[], &[0]).unwrap();
        let m = mean_scalar(&a).unwrap();
        assert!(m.is_nan(), "empty mean must be NaN, got {m}");
    }

    #[test]
    fn median_odd_length() {
        let a = array_f64(&[3.0, 1.0, 4.0, 1.0, 5.0], &[5]).unwrap();
        let m = median_scalar(&a).unwrap();
        assert!(approx(m, 3.0, 1e-12), "median got {m}");
    }

    #[test]
    fn median_even_length() {
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
        let m = median_scalar(&a).unwrap();
        assert!(approx(m, 2.5, 1e-12), "median got {m}");
    }

    #[test]
    fn median_with_nan_yields_nan() {
        let a = array_f64(&[1.0, f64::NAN, 3.0], &[3]).unwrap();
        let m = median_scalar(&a).unwrap();
        assert!(m.is_nan(), "median with NaN must be NaN");
    }

    #[test]
    fn std_basic_population() {
        // Population std of [1,2,3,4,5] = sqrt(2.0)
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0], &[5]).unwrap();
        let s = std_scalar(&a).unwrap();
        assert!(approx(s, 2.0_f64.sqrt(), 1e-12), "std got {s}");
    }

    #[test]
    fn var_basic_population() {
        // Population var of [1,2,3,4,5] = 2.0
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0], &[5]).unwrap();
        let v = var_scalar(&a).unwrap();
        assert!(approx(v, 2.0, 1e-12), "var got {v}");
    }

    // ---- #145 statistics gap-closure: ptp / nan* / percentile -----------
    // Every literal below was bit-confirmed against numpy 2.0.2 via the
    // `/usr/bin/python3` oracle (the differential gate's hand-computed
    // shape: assert the cobrust value == the numpy value).

    #[test]
    fn ptp_basic() {
        // np.ptp([3,1,4,1,5,9,2,6]) == 8.0  (max 9 - min 1).
        let a = array_f64(&[3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0], &[8]).unwrap();
        let p = ptp_scalar(&a).unwrap();
        assert!(approx(p, 8.0, 1e-12), "ptp got {p}");
    }

    #[test]
    fn ptp_single_element_is_zero() {
        // np.ptp([7.0]) == 0.0.
        let a = array_f64(&[7.0], &[1]).unwrap();
        let p = ptp_scalar(&a).unwrap();
        assert!(approx(p, 0.0, 1e-12), "ptp single got {p}");
    }

    #[test]
    fn ptp_integer_promotion() {
        // np.ptp([1,2,3,4,5]) == 4 (value-identical under f64).
        let a = array_i64(&[1, 2, 3, 4, 5], &[5]).unwrap();
        let p = ptp_scalar(&a).unwrap();
        assert!(approx(p, 4.0, 1e-12), "ptp int got {p}");
    }

    #[test]
    fn ptp_with_nan_propagates() {
        let a = array_f64(&[1.0, f64::NAN, 3.0], &[3]).unwrap();
        assert!(ptp_scalar(&a).unwrap().is_nan(), "ptp NaN must propagate");
    }

    #[test]
    fn ptp_empty_yields_nan() {
        let a = array_f64(&[], &[0]).unwrap();
        assert!(ptp_scalar(&a).unwrap().is_nan(), "empty ptp must be NaN");
    }

    #[test]
    fn nansum_skips_nan() {
        // np.nansum([1, nan, 3]) == 4.0.
        let a = array_f64(&[1.0, f64::NAN, 3.0], &[3]).unwrap();
        let s = nansum_scalar(&a).unwrap();
        assert!(approx(s, 4.0, 1e-12), "nansum got {s}");
    }

    #[test]
    fn nansum_all_nan_is_zero() {
        // np.nansum([nan, nan]) == 0.0  (NOT NaN).
        let a = array_f64(&[f64::NAN, f64::NAN], &[2]).unwrap();
        let s = nansum_scalar(&a).unwrap();
        assert!(approx(s, 0.0, 1e-12), "nansum all-NaN must be 0.0, got {s}");
    }

    #[test]
    fn nansum_no_nan_equals_sum() {
        // np.nansum([1,2,3,4]) == 10.0.
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
        let s = nansum_scalar(&a).unwrap();
        assert!(approx(s, 10.0, 1e-12), "nansum got {s}");
    }

    #[test]
    fn nanmean_skips_nan() {
        // np.nanmean([1, nan, 3]) == 2.0  (mean of {1,3}).
        let a = array_f64(&[1.0, f64::NAN, 3.0], &[3]).unwrap();
        let m = nanmean_scalar(&a).unwrap();
        assert!(approx(m, 2.0, 1e-12), "nanmean got {m}");
    }

    #[test]
    fn nanmean_all_nan_is_nan() {
        // np.nanmean([nan, nan]) == nan.
        let a = array_f64(&[f64::NAN, f64::NAN], &[2]).unwrap();
        assert!(
            nanmean_scalar(&a).unwrap().is_nan(),
            "nanmean all-NaN must be NaN"
        );
    }

    #[test]
    fn nanmean_no_nan_equals_mean() {
        // np.nanmean([1,2,3,4]) == 2.5.
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
        let m = nanmean_scalar(&a).unwrap();
        assert!(approx(m, 2.5, 1e-12), "nanmean got {m}");
    }

    #[test]
    fn nanstd_skips_nan() {
        // np.nanstd([1, nan, 3]) == 1.0  (population std of {1,3}:
        // mean 2, var ((1)^2+(1)^2)/2 = 1, sqrt = 1).
        let a = array_f64(&[1.0, f64::NAN, 3.0], &[3]).unwrap();
        let s = nanstd_scalar(&a).unwrap();
        assert!(approx(s, 1.0, 1e-12), "nanstd got {s}");
    }

    #[test]
    fn nanstd_no_nan_population() {
        // np.nanstd([1,2,3,4,5]) == sqrt(2.0) (population, ddof=0).
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0], &[5]).unwrap();
        let s = nanstd_scalar(&a).unwrap();
        assert!(approx(s, 2.0_f64.sqrt(), 1e-12), "nanstd got {s}");
    }

    #[test]
    fn nanstd_single_finite_is_zero() {
        let a = array_f64(&[5.0, f64::NAN], &[2]).unwrap();
        let s = nanstd_scalar(&a).unwrap();
        assert!(approx(s, 0.0, 1e-12), "nanstd single finite got {s}");
    }

    #[test]
    fn nanstd_all_nan_is_nan() {
        let a = array_f64(&[f64::NAN, f64::NAN], &[2]).unwrap();
        assert!(
            nanstd_scalar(&a).unwrap().is_nan(),
            "nanstd all-NaN must be NaN"
        );
    }

    #[test]
    fn percentile_quartiles() {
        // np.percentile([1,2,3,4], [0,25,50,75,100]) ==
        //   [1.0, 1.75, 2.5, 3.25, 4.0]  (linear interpolation).
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
        assert!(approx(percentile_scalar(&a, 0.0).unwrap(), 1.0, 1e-12));
        assert!(approx(percentile_scalar(&a, 25.0).unwrap(), 1.75, 1e-12));
        assert!(approx(percentile_scalar(&a, 50.0).unwrap(), 2.5, 1e-12));
        assert!(approx(percentile_scalar(&a, 75.0).unwrap(), 3.25, 1e-12));
        assert!(approx(percentile_scalar(&a, 100.0).unwrap(), 4.0, 1e-12));
    }

    #[test]
    fn percentile_interpolated_midpoint() {
        // np.percentile([3,1,4,1,5], 40) == 2.2  (sorted [1,1,3,4,5],
        // pos = 4*0.40 = 1.6, lo=sorted[1]=1, hi=sorted[2]=3,
        // 1 + 0.6*(3-1) = 2.2).
        let a = array_f64(&[3.0, 1.0, 4.0, 1.0, 5.0], &[5]).unwrap();
        let p = percentile_scalar(&a, 40.0).unwrap();
        assert!(approx(p, 2.2, 1e-12), "percentile got {p}");
    }

    #[test]
    fn percentile_range_zero_to_ten() {
        // np.percentile([0..10], 25) == 2.5 ; ([0..10], 90) == 9.0.
        let a = array_f64(
            &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
            &[11],
        )
        .unwrap();
        assert!(approx(percentile_scalar(&a, 25.0).unwrap(), 2.5, 1e-12));
        assert!(approx(percentile_scalar(&a, 90.0).unwrap(), 9.0, 1e-12));
    }

    #[test]
    fn percentile_median_equiv() {
        // np.percentile(x, 50) == np.median(x) for the same data.
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0], &[5]).unwrap();
        let p50 = percentile_scalar(&a, 50.0).unwrap();
        let med = median_scalar(&a).unwrap();
        assert!(approx(p50, med, 1e-12), "p50={p50} median={med}");
    }

    #[test]
    fn percentile_single_element() {
        // np.percentile([42.0], q) == 42.0 for any q.
        let a = array_f64(&[42.0], &[1]).unwrap();
        assert!(approx(percentile_scalar(&a, 0.0).unwrap(), 42.0, 1e-12));
        assert!(approx(percentile_scalar(&a, 50.0).unwrap(), 42.0, 1e-12));
        assert!(approx(percentile_scalar(&a, 100.0).unwrap(), 42.0, 1e-12));
    }

    #[test]
    fn percentile_integer_promotion() {
        // np.percentile([1,2,3,4], 25) == 1.75 with integer input.
        let a = array_i64(&[1, 2, 3, 4], &[4]).unwrap();
        let p = percentile_scalar(&a, 25.0).unwrap();
        assert!(approx(p, 1.75, 1e-12), "percentile int got {p}");
    }

    #[test]
    fn percentile_with_nan_propagates() {
        let a = array_f64(&[1.0, f64::NAN, 3.0], &[3]).unwrap();
        assert!(
            percentile_scalar(&a, 50.0).unwrap().is_nan(),
            "percentile NaN must propagate"
        );
    }

    #[test]
    fn percentile_empty_yields_nan() {
        let a = array_f64(&[], &[0]).unwrap();
        assert!(
            percentile_scalar(&a, 50.0).unwrap().is_nan(),
            "empty percentile must be NaN"
        );
    }

    #[test]
    fn percentile_out_of_range_q_clamps() {
        // q clamped to [0,100]: q=-10 → q=0 → min; q=200 → q=100 → max.
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
        assert!(approx(percentile_scalar(&a, -10.0).unwrap(), 1.0, 1e-12));
        assert!(approx(percentile_scalar(&a, 200.0).unwrap(), 4.0, 1e-12));
    }

    #[test]
    fn split_first_of_three() {
        // 6 elems split into 3 → first chunk len 2 ({1,2}).
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[6]).unwrap();
        let chunk = split_first_chunk(&a, 3).unwrap();
        assert_eq!(chunk.shape(), vec![2]);
        if let Array::Float64(arr) = &chunk {
            assert_eq!(arr.iter().copied().collect::<Vec<f64>>(), vec![1.0, 2.0]);
        } else {
            panic!("dtype mismatch");
        }
    }

    #[test]
    fn split_uneven_first_gets_remainder() {
        // 7 elems split into 3 → 7/3 = 2 base, rem 1, so first chunk
        // len 3 (matches numpy `array_split`'s remainder-front rule).
        let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0], &[7]).unwrap();
        let chunk = split_first_chunk(&a, 3).unwrap();
        assert_eq!(chunk.shape(), vec![3]);
    }

    #[test]
    fn split_zero_n_errors() {
        let a = array_f64(&[1.0], &[1]).unwrap();
        let r = split_first_chunk(&a, 0);
        assert!(matches!(
            r,
            Err(NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                ..
            })
        ));
    }
}
