//! Stream W P0 增量 — `mgrid` / `ogrid` (1-D first proof).
//!
//! numpy's `mgrid[start:stop]` / `ogrid[start:stop]` 1-D forms produce
//! the integer sequence `[start, start+1, ..., stop-1]` as a dense
//! `f64` buffer (numpy default dtype on a step-1 range — there is no
//! step-aware dtype-preservation issue at the 1-D first proof).
//!
//! Multi-dimensional `mgrid[:, :]` (the n-D index-tuple form) wants an
//! `IndexExpr` surface on top of the Buffer handle which is explicitly
//! deferred per ADR-0072 §"coil deep operator/index". The 1-D form is
//! sufficient to unblock numpy's most common `mgrid` pattern (the
//! linear-axis case) and is the proven scope for Stream W's gap-
//! closure sprint.
//!
//! Per CLAUDE.md §2.5 + §5.3, the implementation composes the existing
//! `constructors::arange` path so the underlying `ndarray::ArrayD<f64>`
//! allocation + buffer drop discipline is identical to `zeros` / `ones`
//! / `eye`. No new storage shape is invented; the buffer handle ABI is
//! reused as-is.

#![allow(clippy::cast_precision_loss)]

use crate::array::Array;
use crate::constructors::arange;
use crate::dtype::Dtype;
use crate::error::NumpyError;

/// `coil.mgrid(start, stop) -> Buffer` 1-D form.
///
/// Returns the f64 buffer `[start, start+1, ..., stop-1]`. When
/// `stop <= start` an empty buffer is returned (matches numpy's empty-
/// slice convention).
///
/// # Errors
///
/// Propagates `NumpyError::ZeroStep` from the underlying `arange` if a
/// future widening introduces a step-zero path; currently unreachable
/// (we always pass step=1).
pub fn mgrid_1d(start: i64, stop: i64) -> Result<Array, NumpyError> {
    // Reuse arange's count + materialisation discipline; numpy's
    // `mgrid[s:e]` is `arange(s, e)` for the 1-D linear case.
    let start_f = start as f64;
    let stop_f = stop as f64;
    if stop_f <= start_f {
        // Empty 1-D buffer — same shape numpy yields for an inverted /
        // empty index slice.
        return arange(0.0, 0.0, 1.0, Dtype::Float64);
    }
    arange(start_f, stop_f, 1.0, Dtype::Float64)
}

/// `coil.ogrid(start, stop) -> Buffer` 1-D form.
///
/// For the 1-D case numpy's `ogrid` and `mgrid` yield the same
/// sequence (the difference shows only in n-D where `ogrid` returns
/// "open" 1-D sub-axes vs `mgrid`'s dense product grid). The 1-D
/// first proof therefore intentionally aliases `mgrid_1d`.
///
/// # Errors
///
/// Same as `mgrid_1d`.
pub fn ogrid_1d(start: i64, stop: i64) -> Result<Array, NumpyError> {
    mgrid_1d(start, stop)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn mgrid_basic_range() {
        let a = mgrid_1d(0, 5).expect("mgrid 0..5");
        assert_eq!(a.shape(), vec![5]);
        assert_eq!(a.dtype(), Dtype::Float64);
        if let Array::Float64(arr) = &a {
            let v: Vec<f64> = arr.iter().copied().collect();
            assert_eq!(v, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
        } else {
            panic!("mgrid_1d must yield Float64");
        }
    }

    #[test]
    fn mgrid_negative_start() {
        let a = mgrid_1d(-3, 2).expect("mgrid -3..2");
        if let Array::Float64(arr) = &a {
            let v: Vec<f64> = arr.iter().copied().collect();
            assert_eq!(v, vec![-3.0, -2.0, -1.0, 0.0, 1.0]);
        } else {
            panic!("dtype mismatch");
        }
    }

    #[test]
    fn mgrid_empty_range() {
        let a = mgrid_1d(5, 5).expect("mgrid empty");
        assert_eq!(a.size(), 0);
    }

    #[test]
    fn mgrid_inverted_range_yields_empty() {
        let a = mgrid_1d(10, 5).expect("mgrid inverted");
        assert_eq!(a.size(), 0);
    }

    #[test]
    fn ogrid_aliases_mgrid_in_1d() {
        let m = mgrid_1d(0, 5).expect("mgrid");
        let o = ogrid_1d(0, 5).expect("ogrid");
        assert_eq!(m.shape(), o.shape());
        assert_eq!(m.dtype(), o.dtype());
    }
}
