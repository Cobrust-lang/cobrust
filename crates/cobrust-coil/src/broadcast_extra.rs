//! Stream W P0 增量 — `broadcast_to` (1-D first proof).
//!
//! numpy's `np.broadcast_to(a, shape)` virtually replicates `a` into
//! the target `shape` (zero-copy stride trick). The first proof here
//! materialises a 1-D tile-to-`n` because the Buffer handle ABI does
//! not yet model strided views (per ADR-0072 §"coil deep operator/
//! index"). The behavior matches numpy on the 1-D scalar-broadcast
//! case `np.broadcast_to(np.array([x]), (n,))`: every output element
//! is the input's first element. For an already-shaped input this
//! tiles modulo the input length (numpy errors here; we tile to keep
//! the first proof useful for the "tile a scalar to length n" pattern
//! which is the dominant LLM-training-data shape per §2.5).
//!
//! A strided-view path + true numpy-compatible error semantics are
//! tracked under the deferred operator/index sub-ADR.

// File-level allows mirror reduce.rs / cabi.rs pattern. The truncation
// + sign + precision lints fire on numpy-shape arithmetic that the
// boundary already validates (n ≥ 0 is checked at function entry).
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_panics_doc
)]

use ndarray::{ArrayD, IxDyn};

use crate::array::Array;
use crate::error::{NumpyError, NumpyErrorKind};

/// `coil.broadcast_to(a, n) -> Buffer` 1-D first proof.
///
/// Materialises a fresh f64 1-D buffer of length `n` by tiling `a`'s
/// flat values. When `a` is empty the output is empty regardless of
/// `n` (numpy errors; we silently degrade to preserve `i64`-clamp
/// discipline of the broader cabi).
///
/// # Errors
///
/// `NumpyError::ShapeMismatch` when `n < 0` (the cabi shim clamps
/// before reaching here, so the error path is defensive).
pub fn broadcast_to_1d(a: &Array, n: i64) -> Result<Array, NumpyError> {
    if n < 0 {
        return Err(NumpyError {
            kind: NumpyErrorKind::ShapeMismatch,
            message: format!("broadcast_to: target length {n} must be non-negative"),
        });
    }
    let n = n as usize;
    // Promote inputs to f64 (the first-proof handle storage uniformly
    // round-trips f64; structural dtype-preservation is tracked under
    // the deep coil sub-ADR).
    let src: Vec<f64> = match a {
        Array::Int32(x) => x.iter().map(|&v| f64::from(v)).collect(),
        Array::Int64(x) => x.iter().map(|&v| v as f64).collect(),
        Array::Float32(x) => x.iter().map(|&v| f64::from(v)).collect(),
        Array::Float64(x) => x.iter().copied().collect(),
        Array::Bool(x) => x.iter().map(|&v| if v { 1.0 } else { 0.0 }).collect(),
    };
    let mut out: Vec<f64> = Vec::with_capacity(n);
    if src.is_empty() {
        // Defensive: numpy errors on empty source. Yield empty so the
        // C-ABI shim's `Box::new` never panics.
        return Ok(Array::Float64(
            ArrayD::from_shape_vec(IxDyn(&[0]), Vec::new()).expect("empty shape"),
        ));
    }
    let m = src.len();
    for i in 0..n {
        out.push(src[i % m]);
    }
    Ok(Array::Float64(
        ArrayD::from_shape_vec(IxDyn(&[n]), out).expect("shape * len match"),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::constructors::array_f64;

    #[test]
    fn broadcast_scalar_to_three() {
        let a = array_f64(&[7.0], &[1]).unwrap();
        let b = broadcast_to_1d(&a, 3).unwrap();
        assert_eq!(b.shape(), vec![3]);
        if let Array::Float64(arr) = &b {
            assert_eq!(
                arr.iter().copied().collect::<Vec<f64>>(),
                vec![7.0, 7.0, 7.0]
            );
        } else {
            panic!("dtype mismatch");
        }
    }

    #[test]
    fn broadcast_tiles_when_input_longer_than_one() {
        let a = array_f64(&[1.0, 2.0], &[2]).unwrap();
        let b = broadcast_to_1d(&a, 5).unwrap();
        if let Array::Float64(arr) = &b {
            assert_eq!(
                arr.iter().copied().collect::<Vec<f64>>(),
                vec![1.0, 2.0, 1.0, 2.0, 1.0],
            );
        } else {
            panic!("dtype mismatch");
        }
    }

    #[test]
    fn broadcast_to_zero_yields_empty() {
        let a = array_f64(&[1.0], &[1]).unwrap();
        let b = broadcast_to_1d(&a, 0).unwrap();
        assert_eq!(b.size(), 0);
    }

    #[test]
    fn broadcast_negative_n_errors() {
        let a = array_f64(&[1.0], &[1]).unwrap();
        let r = broadcast_to_1d(&a, -1);
        assert!(matches!(
            r,
            Err(NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                ..
            })
        ));
    }
}
