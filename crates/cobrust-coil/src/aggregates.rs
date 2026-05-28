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
    clippy::redundant_closure_for_method_calls
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
