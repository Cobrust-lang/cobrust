// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.1 broadcasting per ADR-0014 §2.
// see PROVENANCE.toml for the full manifest.

//! Numpy-exact broadcasting shape rules per ADR-0014 §2.
//!
//! Cite https://numpy.org/doc/stable/user/basics.broadcasting.html.
//!
//! Rules:
//!   1. Right-align the two shape vectors; pad the shorter on the
//!      LEFT with 1s.
//!   2. For each axis: if `a_k == b_k` OR `a_k == 1` OR `b_k == 1`,
//!      the broadcast axis is `max(a_k, b_k)`. Otherwise raise
//!      `BroadcastShapeMismatch`.
//!   3. Empty shape `()` (scalar) broadcasts against any shape (each
//!      axis treated as 1).
//!
//! `broadcast_shape` returns the broadcast result-shape; the actual
//! broadcast view is produced by callers in `ufunc.rs` via
//! `ndarray::ArrayBase::broadcast(target_shape)`.

// CQ P1-4 + template-fix: single consolidated block; future emits use #[allow] at item level.
#![allow(clippy::missing_errors_doc, clippy::uninlined_format_args)]

use crate::error::{NumpyError, NumpyErrorKind};

/// Compute the numpy-exact broadcast shape of two input shapes.
///
/// # Errors
/// `NumpyError::BroadcastShapeMismatch` if the two shapes are not
/// broadcast-compatible per the rules above.
pub fn broadcast_shape(a: &[usize], b: &[usize]) -> Result<Vec<usize>, NumpyError> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        // Right-align: index from the right.
        let a_dim = if k < a.len() { a[a.len() - 1 - k] } else { 1 };
        let b_dim = if k < b.len() { b[b.len() - 1 - k] } else { 1 };
        let dim = if a_dim == b_dim {
            a_dim
        } else if a_dim == 1 {
            b_dim
        } else if b_dim == 1 {
            a_dim
        } else {
            return Err(NumpyError {
                kind: NumpyErrorKind::BroadcastShapeMismatch,
                message: format!(
                    "operands could not be broadcast together with shapes {a:?} {b:?}"
                ),
            });
        };
        out.push(dim);
    }
    out.reverse();
    Ok(out)
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
    fn equal_shapes() {
        assert_eq!(broadcast_shape(&[3, 4], &[3, 4]).unwrap(), vec![3, 4]);
    }

    #[test]
    fn scalar_broadcasts_to_anything() {
        assert_eq!(broadcast_shape(&[], &[3, 4]).unwrap(), vec![3, 4]);
        assert_eq!(broadcast_shape(&[3, 4], &[]).unwrap(), vec![3, 4]);
        assert_eq!(broadcast_shape(&[], &[]).unwrap(), Vec::<usize>::new());
    }

    #[test]
    fn size_one_axis_expands() {
        assert_eq!(broadcast_shape(&[3, 1], &[1, 4]).unwrap(), vec![3, 4]);
        assert_eq!(broadcast_shape(&[1, 4], &[3, 1]).unwrap(), vec![3, 4]);
    }

    #[test]
    fn shorter_left_pads_with_ones() {
        // [4] vs [3, 4] → [3, 4] (the [4] is treated as [1, 4]).
        assert_eq!(broadcast_shape(&[4], &[3, 4]).unwrap(), vec![3, 4]);
        assert_eq!(broadcast_shape(&[3, 4], &[4]).unwrap(), vec![3, 4]);
    }

    #[test]
    fn higher_dim_pads_correctly() {
        // [5, 1, 4] vs [3, 4] → [5, 3, 4].
        assert_eq!(broadcast_shape(&[5, 1, 4], &[3, 4]).unwrap(), vec![5, 3, 4]);
    }

    #[test]
    fn mismatched_axes_error() {
        let err = broadcast_shape(&[3, 5], &[3, 4]).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
    }

    #[test]
    fn non_one_non_equal_inner_axis_errors() {
        let err = broadcast_shape(&[2, 3], &[5, 3]).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
    }

    #[test]
    fn long_short_mix() {
        // [8, 1, 6, 1] vs [7, 1, 5] → [8, 7, 6, 5].
        assert_eq!(
            broadcast_shape(&[8, 1, 6, 1], &[7, 1, 5]).unwrap(),
            vec![8, 7, 6, 5]
        );
    }
}
