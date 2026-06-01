// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: #145 numpy gap-closure BATCH 2 — array MANIPULATION ops
//   (transpose / flatten / ravel / concatenate / vstack / hstack), the
//   Buffer-RETURNING combine + reshape surface (mirror the `@` matmul
//   Buffer-return wiring of ADR-0077, NOT the scalar-return stats).
// see PROVENANCE.toml for the full manifest.

//! Array-manipulation free functions — the `Array -> Array` combine +
//! reshape surface most-used in real numpy code (`np.transpose` /
//! `np.flatten` / `np.ravel` / `np.concatenate` / `np.vstack` /
//! `np.hstack`), each returning a fresh owned `Array`.
//!
//! ## Why these six (the bounded #145 BATCH-2 choice)
//!
//! Per the LLM-training-data-overlap rule (§2.5) these are the array-
//! manipulation ops an LLM reaches for first. The cut line is the ARITY
//! CONTRACT: only the 1-arg (`transpose` / `flatten` / `ravel`) and the
//! 2-array (`concatenate(a, b)` / `vstack(a, b)` / `hstack(a, b)`) forms
//! ship here — they wire through the EXISTING borrow-Buffer-args →
//! fresh-Buffer-return ecosystem path (`emit_ecosystem_call`, proven by
//! `coil.linalg.solve(a, b)`'s 2-Buffer-arg → Buffer path). The N-array
//! `np.concatenate([a, b, c, ...])` and the shape-tuple `np.reshape(a,
//! (m, n))` forms are DEFERRED: they need `list[Buffer]` / tuple
//! marshalling that does not exist yet (a follow-up once that lands).
//!
//! ## numpy-exact semantics (the load-bearing contract)
//!
//! - `transpose(a)` — reverse ALL axes (`a.T`). A 1-D array is returned
//!   UNCHANGED (numpy: `np.array([1,2,3]).T` is still `(3,)`); a 2-D
//!   `(m, n)` becomes `(n, m)`. Dtype + values preserved.
//! - `flatten(a)` / `ravel(a)` — collapse to a 1-D C-order (row-major)
//!   copy. Both return the SAME values; numpy's `ravel` returns a VIEW
//!   when possible and `flatten` always copies, but the VALUES are
//!   identical, so the Semantic tier holds (Cobrust returns an owned
//!   copy for both — the handle ABI has no view-aliasing-into-parent
//!   surface).
//! - `concatenate(a, b)` — join along axis 0 (the default `np.concatenate`
//!   axis). The two arrays must have the SAME rank and matching sizes on
//!   every axis EXCEPT axis 0; a mismatch is a `ShapeMismatch`
//!   (numpy's `ValueError`).
//! - `vstack(a, b)` — stack row-wise. A 1-D `(n,)` operand is first
//!   promoted to `(1, n)` (numpy's `atleast_2d` behavior), THEN both are
//!   concatenated along axis 0. So `vstack((n,), (n,)) -> (2, n)` and
//!   `vstack((r,c), (s,c)) -> (r+s, c)`.
//! - `hstack(a, b)` — stack column-wise. For 1-D operands it concatenates
//!   along axis 0 (`hstack((p,), (q,)) -> (p+q,)`); for ≥2-D operands it
//!   concatenates along axis 1 (`hstack((r,c1), (r,c2)) -> (r, c1+c2)`).
//!
//! ## Dtype contract (the §2.5-honest minimal surface)
//!
//! The 1-arg ops (`transpose` / `flatten` / `ravel`) are dtype-generic:
//! they preserve the input variant across all five dtypes. The 2-array
//! combine ops (`concatenate` / `vstack` / `hstack`) require the two
//! operands to share a dtype and raise `ShapeMismatch` otherwise. numpy
//! PROMOTES a mixed-dtype pair to a common dtype; we keep the clean
//! equal-dtype contract because (a) every `.cb` Buffer constructor today
//! emits `Float64` (so the common path is always `f64`+`f64`), and (b) a
//! silent cross-dtype promotion is exactly the kind of implicit coercion
//! §2.2 forbids. A mixed-dtype promoting form is a tracked follow-up.

// File-level allows mirror the other auto-generated coil modules. The
// cast / wrap lints fire on intrinsically-correct shape arithmetic.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions
)]

use ndarray::{ArrayD, Axis, IxDyn};

use crate::array::Array;
use crate::error::{NumpyError, NumpyErrorKind};

/// `np.transpose(a)` / `a.T` — reverse every axis. A 1-D (or 0-D) array
/// is returned unchanged (numpy semantics); a 2-D `(m, n)` becomes
/// `(n, m)`. Dtype + values preserved. The returned array is a fresh
/// owned C-standard-layout copy (`reversed_axes` produces an F-layout
/// view; we materialise it owned so the handle owns a contiguous buffer).
#[must_use]
pub fn transpose(a: &Array) -> Array {
    match a {
        Array::Int32(arr) => Array::Int32(owned_c(&arr.t())),
        Array::Int64(arr) => Array::Int64(owned_c(&arr.t())),
        Array::Float32(arr) => Array::Float32(owned_c(&arr.t())),
        Array::Float64(arr) => Array::Float64(owned_c(&arr.t())),
        Array::Bool(arr) => Array::Bool(owned_c(&arr.t())),
    }
}

/// `a.flatten()` — collapse to a 1-D C-order (row-major) copy. Always a
/// fresh owned buffer. Dtype preserved.
#[must_use]
pub fn flatten(a: &Array) -> Array {
    match a {
        Array::Int32(arr) => Array::Int32(flatten_c(arr)),
        Array::Int64(arr) => Array::Int64(flatten_c(arr)),
        Array::Float32(arr) => Array::Float32(flatten_c(arr)),
        Array::Float64(arr) => Array::Float64(flatten_c(arr)),
        Array::Bool(arr) => Array::Bool(flatten_c(arr)),
    }
}

/// `np.ravel(a)` — collapse to a 1-D C-order copy. numpy's `ravel`
/// returns a view when the memory is already contiguous; the Cobrust
/// handle ABI has no view-into-parent surface, so we return an owned
/// copy. The VALUES are identical to numpy's `ravel`, hence Semantic
/// tier. Delegates to [`flatten`] (same value contract).
#[must_use]
pub fn ravel(a: &Array) -> Array {
    flatten(a)
}

/// `np.concatenate((a, b))` along axis 0 (the default axis). The two
/// arrays must have the SAME rank and matching sizes on every axis
/// except axis 0.
///
/// # Errors
///
/// `ShapeMismatch` (numpy's `ValueError`) when the operands have
/// different ranks, mismatched non-axis-0 dimensions, OR different
/// dtypes (the equal-dtype contract — see the module docs).
pub fn concatenate(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    concat_axis(a, b, 0)
}

/// `np.vstack((a, b))` — stack row-wise. A 1-D `(n,)` operand is first
/// promoted to `(1, n)`, then both are concatenated along axis 0:
/// `vstack((n,), (n,)) -> (2, n)`; `vstack((r,c), (s,c)) -> (r+s, c)`.
///
/// # Errors
///
/// `ShapeMismatch` when the (post-`atleast_2d`) operands have mismatched
/// column counts or different dtypes.
pub fn vstack(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    let a2 = atleast_2d_row(a);
    let b2 = atleast_2d_row(b);
    concat_axis(&a2, &b2, 0)
}

/// `np.hstack((a, b))` — stack column-wise. For 1-D operands concatenate
/// along axis 0 (`hstack((p,), (q,)) -> (p+q,)`); for ≥2-D operands
/// concatenate along axis 1 (`hstack((r,c1), (r,c2)) -> (r, c1+c2)`).
///
/// # Errors
///
/// `ShapeMismatch` when the operands have mismatched non-axis-1
/// dimensions (e.g. differing row counts for 2-D inputs) or different
/// dtypes.
pub fn hstack(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    // numpy: 1-D inputs concat along axis 0; ≥2-D along axis 1. The axis
    // is chosen from the FIRST operand's rank (numpy uses the common
    // ndim; mismatched ranks fall through to `concat_axis`'s rank guard).
    let axis = usize::from(a.ndim() >= 2);
    concat_axis(a, b, axis)
}

// ---- internals -----------------------------------------------------------

/// Materialise an `ArrayView` as an owned C-standard-layout `ArrayD<T>`.
/// `reversed_axes` / `t()` yield an F-layout view; `.to_owned()` on a
/// non-contiguous view already produces a C-standard-layout owned array
/// in ndarray 0.15, so the handle's buffer is contiguous + row-major.
/// Takes the view by shared reference (it is only read).
fn owned_c<T: Clone>(view: &ndarray::ArrayViewD<'_, T>) -> ArrayD<T> {
    view.to_owned()
}

/// 1-D C-order copy of an `ArrayD<T>` (the `flatten` / `ravel` body).
/// `iter()` walks in C (row-major) logical order regardless of the
/// physical layout, so this is the numpy `flatten('C')` value sequence.
fn flatten_c<T: Clone>(arr: &ArrayD<T>) -> ArrayD<T> {
    let flat: Vec<T> = arr.iter().cloned().collect();
    let len = flat.len();
    // from_shape_vec on a 1-D shape with a matching-length vec is
    // infallible; the explicit shape is `[len]`.
    ArrayD::from_shape_vec(IxDyn(&[len]), flat)
        .expect("1-D from_shape_vec with matching length is infallible")
}

/// Promote a 1-D `(n,)` array to a `(1, n)` row (numpy's `atleast_2d`
/// behavior for `vstack`). Arrays of rank ≥ 2 are returned as a clone
/// (numpy leaves them unchanged); a 0-D scalar becomes `(1, 1)`.
fn atleast_2d_row(a: &Array) -> Array {
    match a.ndim() {
        0 => reshape_to(a, &[1, 1]),
        1 => {
            let n = a.shape()[0];
            reshape_to(a, &[1, n])
        }
        _ => a.clone(),
    }
}

/// Reshape `a` to `shape` (a total-size-preserving owned C-order copy).
/// Used only by [`atleast_2d_row`] with a shape whose product equals
/// `a.size()`, so the reshape is always valid.
fn reshape_to(a: &Array, shape: &[usize]) -> Array {
    fn go<T: Clone>(arr: &ArrayD<T>, shape: &[usize]) -> ArrayD<T> {
        let flat: Vec<T> = arr.iter().cloned().collect();
        ArrayD::from_shape_vec(IxDyn(shape), flat)
            .expect("reshape_to: caller guarantees size-preserving shape")
    }
    match a {
        Array::Int32(arr) => Array::Int32(go(arr, shape)),
        Array::Int64(arr) => Array::Int64(go(arr, shape)),
        Array::Float32(arr) => Array::Float32(go(arr, shape)),
        Array::Float64(arr) => Array::Float64(go(arr, shape)),
        Array::Bool(arr) => Array::Bool(go(arr, shape)),
    }
}

/// Concatenate two same-dtype, same-rank arrays along `axis`. The shared
/// body of `concatenate` / `vstack` / `hstack`.
///
/// # Errors
///
/// `ShapeMismatch` (numpy's `ValueError`) on a dtype mismatch, a rank
/// mismatch, an out-of-range `axis`, or a non-axis dimension mismatch
/// (the latter is surfaced by `ndarray::concatenate`'s own `ShapeError`).
fn concat_axis(a: &Array, b: &Array, axis: usize) -> Result<Array, NumpyError> {
    // Dtype contract — equal-dtype only (see module docs). A mismatch is
    // numpy's eventual promotion point; we raise instead (no silent
    // coercion, §2.2).
    if a.dtype() != b.dtype() {
        return Err(NumpyError {
            kind: NumpyErrorKind::ShapeMismatch,
            message: format!(
                "concatenate: dtype mismatch {:?} vs {:?} (equal-dtype contract; \
                 cross-dtype promotion is a tracked follow-up)",
                a.dtype(),
                b.dtype()
            ),
        });
    }
    // Rank guard (numpy: "all the input array dimensions must match").
    if a.ndim() != b.ndim() {
        return Err(NumpyError {
            kind: NumpyErrorKind::ShapeMismatch,
            message: format!(
                "concatenate: dimension mismatch — operand ranks {} and {} differ",
                a.ndim(),
                b.ndim()
            ),
        });
    }
    if axis >= a.ndim() {
        return Err(NumpyError {
            kind: NumpyErrorKind::ShapeMismatch,
            message: format!(
                "concatenate: axis {} out of bounds for {}-D operands",
                axis,
                a.ndim()
            ),
        });
    }
    macro_rules! concat_variant {
        ($va:expr, $vb:expr, $ctor:path) => {{
            // `ndarray::concatenate` enforces the non-axis dimensions match
            // and returns a fresh owned C-order array; a mismatch is a
            // `ShapeError` we remap to numpy's `ValueError` shape.
            let views = [$va.view(), $vb.view()];
            ndarray::concatenate(Axis(axis), &views)
                .map($ctor)
                .map_err(|e| NumpyError {
                    kind: NumpyErrorKind::ShapeMismatch,
                    message: format!(
                        "concatenate: all the input array dimensions except for the \
                         concatenation axis must match exactly ({e})"
                    ),
                })
        }};
    }
    match (a, b) {
        (Array::Int32(x), Array::Int32(y)) => concat_variant!(x, y, Array::Int32),
        (Array::Int64(x), Array::Int64(y)) => concat_variant!(x, y, Array::Int64),
        (Array::Float32(x), Array::Float32(y)) => concat_variant!(x, y, Array::Float32),
        (Array::Float64(x), Array::Float64(y)) => concat_variant!(x, y, Array::Float64),
        (Array::Bool(x), Array::Bool(y)) => concat_variant!(x, y, Array::Bool),
        // Unreachable: the dtype-equality guard above already returned on
        // any mismatched pair.
        _ => Err(NumpyError {
            kind: NumpyErrorKind::ShapeMismatch,
            message: "concatenate: dtype mismatch".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]
    use super::*;
    use crate::constructors::{array_f64, array_i64};

    // ---- differential helpers ----
    // Oracle values captured from numpy 2.x via the allowed
    // `/opt/homebrew/bin/python3.11` interpreter (numpy 2.4.6); the
    // transpose / flatten / concat / stack semantics are identical to the
    // coil-provenance numpy 2.0.2.

    fn f64_vals(a: &Array) -> Vec<f64> {
        match a {
            Array::Float64(arr) => arr.iter().copied().collect(),
            _ => panic!("expected Float64"),
        }
    }

    #[test]
    fn transpose_2x3_to_3x2() {
        // np.array([[1,2,3],[4,5,6]]).T -> [[1,4],[2,5],[3,6]], shape (3,2).
        let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
        let t = transpose(&a);
        assert_eq!(t.shape(), vec![3, 2]);
        assert_eq!(f64_vals(&t), vec![1., 4., 2., 5., 3., 6.]);
        assert_eq!(t.dtype(), a.dtype());
    }

    #[test]
    fn transpose_1d_unchanged() {
        // np.array([1,2,3]).T is still (3,) with the same values.
        let a = array_f64(&[1., 2., 3.], &[3]).unwrap();
        let t = transpose(&a);
        assert_eq!(t.shape(), vec![3]);
        assert_eq!(f64_vals(&t), vec![1., 2., 3.]);
    }

    #[test]
    fn transpose_preserves_int_dtype() {
        let a = array_i64(&[1, 2, 3, 4], &[2, 2]).unwrap();
        let t = transpose(&a);
        assert_eq!(t.dtype(), crate::dtype::Dtype::Int64);
        match t {
            Array::Int64(arr) => {
                assert_eq!(arr.iter().copied().collect::<Vec<_>>(), vec![1, 3, 2, 4]);
            }
            _ => panic!("dtype not preserved"),
        }
    }

    #[test]
    fn flatten_2x3_c_order() {
        // np.array([[1,2,3],[4,5,6]]).flatten() -> [1,2,3,4,5,6] (C-order).
        let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
        let f = flatten(&a);
        assert_eq!(f.shape(), vec![6]);
        assert_eq!(f64_vals(&f), vec![1., 2., 3., 4., 5., 6.]);
    }

    #[test]
    fn flatten_of_transpose_is_f_order_values() {
        // (a.T).flatten() walks the TRANSPOSED logical order:
        // a.T = [[1,4],[2,5],[3,6]] -> flatten -> [1,4,2,5,3,6].
        let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
        let f = flatten(&transpose(&a));
        assert_eq!(f64_vals(&f), vec![1., 4., 2., 5., 3., 6.]);
    }

    #[test]
    fn ravel_matches_flatten() {
        let a = array_f64(&[7., 8., 9., 10.], &[2, 2]).unwrap();
        assert_eq!(f64_vals(&ravel(&a)), f64_vals(&flatten(&a)));
    }

    #[test]
    fn flatten_empty() {
        let a = array_f64(&[], &[0]).unwrap();
        let f = flatten(&a);
        assert_eq!(f.shape(), vec![0]);
        assert!(f64_vals(&f).is_empty());
    }

    #[test]
    fn concatenate_2x3_axis0_to_4x3() {
        // np.concatenate([a,b]) along axis 0 -> (4,3).
        let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
        let b = array_f64(&[7., 8., 9., 10., 11., 12.], &[2, 3]).unwrap();
        let c = concatenate(&a, &b).unwrap();
        assert_eq!(c.shape(), vec![4, 3]);
        assert_eq!(
            f64_vals(&c),
            vec![1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12.]
        );
    }

    #[test]
    fn concatenate_1d() {
        // np.concatenate([[1,2,3],[4,5,6]]) -> [1,2,3,4,5,6] (6,).
        let a = array_f64(&[1., 2., 3.], &[3]).unwrap();
        let b = array_f64(&[4., 5., 6.], &[3]).unwrap();
        let c = concatenate(&a, &b).unwrap();
        assert_eq!(c.shape(), vec![6]);
        assert_eq!(f64_vals(&c), vec![1., 2., 3., 4., 5., 6.]);
    }

    #[test]
    fn concatenate_nonconformable_is_err() {
        // (2,3) concat (3,2) along axis 0: non-axis dim 3 != 2 -> ValueError.
        let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
        let b = array_f64(&[1., 2., 3., 4., 5., 6.], &[3, 2]).unwrap();
        let err = concatenate(&a, &b).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
    }

    #[test]
    fn concatenate_rank_mismatch_is_err() {
        let a = array_f64(&[1., 2., 3.], &[3]).unwrap();
        let b = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
        let err = concatenate(&a, &b).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
    }

    #[test]
    fn vstack_2x3_to_4x3() {
        let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
        let b = array_f64(&[7., 8., 9., 10., 11., 12.], &[2, 3]).unwrap();
        let v = vstack(&a, &b).unwrap();
        assert_eq!(v.shape(), vec![4, 3]);
        assert_eq!(
            f64_vals(&v),
            vec![1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12.]
        );
    }

    #[test]
    fn vstack_1d_promotes_to_2x3() {
        // np.vstack([[1,2,3],[4,5,6]]) -> [[1,2,3],[4,5,6]], shape (2,3).
        let a = array_f64(&[1., 2., 3.], &[3]).unwrap();
        let b = array_f64(&[4., 5., 6.], &[3]).unwrap();
        let v = vstack(&a, &b).unwrap();
        assert_eq!(v.shape(), vec![2, 3]);
        assert_eq!(f64_vals(&v), vec![1., 2., 3., 4., 5., 6.]);
    }

    #[test]
    fn hstack_2x3_to_2x6() {
        // np.hstack([(2,3),(2,3)]) -> (2,6): row-wise interleave of columns.
        let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
        let b = array_f64(&[7., 8., 9., 10., 11., 12.], &[2, 3]).unwrap();
        let h = hstack(&a, &b).unwrap();
        assert_eq!(h.shape(), vec![2, 6]);
        // row 0: [1,2,3,7,8,9], row 1: [4,5,6,10,11,12].
        assert_eq!(
            f64_vals(&h),
            vec![1., 2., 3., 7., 8., 9., 4., 5., 6., 10., 11., 12.]
        );
    }

    #[test]
    fn hstack_1d_concats_axis0() {
        // np.hstack([[1,2,3],[4,5,6]]) -> [1,2,3,4,5,6] (6,).
        let a = array_f64(&[1., 2., 3.], &[3]).unwrap();
        let b = array_f64(&[4., 5., 6.], &[3]).unwrap();
        let h = hstack(&a, &b).unwrap();
        assert_eq!(h.shape(), vec![6]);
        assert_eq!(f64_vals(&h), vec![1., 2., 3., 4., 5., 6.]);
    }

    #[test]
    fn hstack_2d_row_mismatch_is_err() {
        // (2,3) hstack (3,3): differing row counts (2 != 3) on axis 0 -> err.
        let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
        let b = array_f64(&[1., 2., 3., 4., 5., 6., 7., 8., 9.], &[3, 3]).unwrap();
        let err = hstack(&a, &b).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
    }

    #[test]
    fn concatenate_dtype_mismatch_is_err() {
        let a = array_f64(&[1., 2.], &[2]).unwrap();
        let b = array_i64(&[1, 2], &[2]).unwrap();
        let err = concatenate(&a, &b).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
    }
}
