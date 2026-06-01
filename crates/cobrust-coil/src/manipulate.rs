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

// =====================================================================
// #145 numpy gap-closure BATCH 9 (2026-06-01) — the FLAT search / order
// surface (`sort` / `argsort` / `unique` / `flatnonzero`), each a 1-arg
// `Array -> Array` op that FLATTENS to C-order first (numpy's no-axis
// default) and returns a fresh owned 1-D `Array`. They wire through the
// SAME borrow-Buffer-arg → fresh-Buffer-return value-handle ABI as the
// reshape ops above (`transpose` / `flatten` / `ravel`), riding the
// shared `cabi::buffer_unary` body + the `coil_shape_ty` `(ptr) -> ptr`
// extern — NO new MIR arm.
//
// ## Return-dtype contract (the load-bearing split)
//
// - `sort` / `unique` PRESERVE the input dtype (numpy: `np.sort` /
//   `np.unique` return the same dtype as the input).
// - `argsort` / `flatnonzero` produce an `Int64` `Array` (the indices),
//   REGARDLESS of input dtype (numpy: both return `intp`, an `int64` on
//   the 64-bit targets the AOT backend supports).
//
// ## NaN semantics (numpy-exact, confirmed via `/opt/homebrew/bin/
// python3.11 -c 'import numpy'`, numpy 2.4.6)
//
// - `sort`: ascending; for floats ALL `NaN` sort to the END regardless
//   of sign-bit (`np.sort([nan,1,-nan,2]) == [1,2,nan,nan]`). We CANNOT
//   use `f64::total_cmp` directly (it orders `NaN` by sign-bit, placing
//   `-NaN` FIRST), so the float arm partitions non-NaN (sorted ascending
//   via `total_cmp` on the finite/inf subset) ++ the `NaN`s (kept in
//   input order). Int / bool arms are a plain ascending sort.
// - `argsort`: STABLE (the reproducible, deterministic choice — numpy's
//   default quicksort happens to agree on the test cases, but a stable
//   sort PINS the tie-break for equal keys + the `NaN`-index tail). The
//   `NaN`-bearing indices go last, in input order (consistent with
//   `sort`'s `NaN` tail).
// - `unique`: sorted ascending unique; numpy 1.21+ collapses MULTIPLE
//   `NaN` to ONE trailing `NaN` (`np.unique([nan,nan,1,nan,2]) ==
//   [1,2,nan]`). We replicate exactly: dedupe the non-NaN values, then
//   append a single `NaN` iff the input contained any.
// - `flatnonzero`: C-order flat indices where `a != 0`; for floats the
//   predicate is `a != 0.0`, so `NaN` (being `!= 0.0`) IS included as
//   nonzero (`np.flatnonzero([0.,nan,0.]) == [1]`).
// =====================================================================

/// `np.sort(a)` — return a fresh ASCENDING-sorted 1-D copy (numpy's
/// no-axis default flattens to C-order first). DTYPE-PRESERVING. For
/// floats, ALL `NaN` sort to the END regardless of sign-bit (numpy:
/// `np.sort([3.,nan,1.]) == [1.,3.,nan]`); int / bool arms are a plain
/// ascending sort. The result is always 1-D (a 2-D input is flattened).
#[must_use]
pub fn sort(a: &Array) -> Array {
    match a {
        Array::Int32(arr) => Array::Int32(sorted_total(arr)),
        Array::Int64(arr) => Array::Int64(sorted_total(arr)),
        Array::Float32(arr) => Array::Float32(sorted_float_nan_last(arr)),
        Array::Float64(arr) => Array::Float64(sorted_float_nan_last(arr)),
        Array::Bool(arr) => Array::Bool(sorted_total(arr)),
    }
}

/// `np.argsort(a)` — return the `Int64` indices that would sort `a`
/// (ascending, over the C-order FLATTENED array). STABLE (the
/// deterministic, reproducible tie-break). For floats, the `NaN`-bearing
/// indices go LAST in input order (consistent with [`sort`]'s `NaN`
/// tail). Result dtype is ALWAYS `Int64` (numpy `intp`). numpy:
/// `np.argsort([3.,1.,2.]) == [1,2,0]`.
#[must_use]
pub fn argsort(a: &Array) -> Array {
    let idx: Vec<i64> = match a {
        Array::Int32(arr) => argsort_total(&arr.iter().copied().collect::<Vec<_>>()),
        Array::Int64(arr) => argsort_total(&arr.iter().copied().collect::<Vec<_>>()),
        Array::Bool(arr) => argsort_total(&arr.iter().copied().collect::<Vec<_>>()),
        Array::Float32(arr) => argsort_float_nan_last(&arr.iter().copied().collect::<Vec<_>>()),
        Array::Float64(arr) => argsort_float_nan_last(&arr.iter().copied().collect::<Vec<_>>()),
    };
    int64_1d(idx)
}

/// `np.unique(a)` — return the SORTED unique values as a fresh 1-D copy
/// (over the C-order FLATTENED array). DTYPE-PRESERVING. For floats,
/// numpy 1.21+ collapses MULTIPLE `NaN` to ONE trailing `NaN`
/// (`np.unique([nan,1.,nan]) == [1.,nan]`); we replicate exactly. numpy:
/// `np.unique([3,1,2,1,3]) == [1,2,3]`.
#[must_use]
pub fn unique(a: &Array) -> Array {
    match a {
        Array::Int32(arr) => Array::Int32(unique_total(arr)),
        Array::Int64(arr) => Array::Int64(unique_total(arr)),
        Array::Float32(arr) => Array::Float32(unique_float_nan_collapse(arr)),
        Array::Float64(arr) => Array::Float64(unique_float_nan_collapse(arr)),
        Array::Bool(arr) => Array::Bool(unique_total(arr)),
    }
}

/// `np.flatnonzero(a)` — return the `Int64` flat C-order indices where
/// `a != 0`. Result dtype is ALWAYS `Int64` (numpy `intp`). For floats
/// the predicate is `a != 0.0`, so `NaN` (being `!= 0.0`) IS counted as
/// nonzero (`np.flatnonzero([0.,nan,0.]) == [1]`). numpy:
/// `np.flatnonzero([0,5,0,2]) == [1,3]`; a 2-D input flattens C-order
/// first.
#[must_use]
pub fn flatnonzero(a: &Array) -> Array {
    let idx: Vec<i64> = match a {
        Array::Int32(arr) => nonzero_idx(arr.iter(), |&v| v != 0),
        Array::Int64(arr) => nonzero_idx(arr.iter(), |&v| v != 0),
        Array::Float32(arr) => nonzero_idx(arr.iter(), |&v| v != 0.0),
        Array::Float64(arr) => nonzero_idx(arr.iter(), |&v| v != 0.0),
        Array::Bool(arr) => nonzero_idx(arr.iter(), |&v| v),
    };
    int64_1d(idx)
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

/// `np.where(cond, a, b)` — the THREE-argument elementwise conditional
/// select: `result[i] = cond[i] truthy ? a[i] : b[i]`. This is the 3-arg
/// `np.where` form (NOT the 1-arg `np.where(cond)` index form, which
/// returns variable-length index arrays — a tracked follow-up).
///
/// ## numpy-exact semantics (the load-bearing contract)
///
/// - `cond` truthiness: a `Bool`-dtype `cond` uses the bool value
///   directly (the clean case — the result of a `coil.Buffer` comparison
///   `a < b` per ADR-0077 is a `Bool`-dtype Buffer); a numeric `cond`
///   treats any NONZERO element as true (numpy: `0`/`0.0` false, every
///   other value — incl. `NaN` — true). The `to_bool` helper mirrors the
///   M7.2 `index::np_where` `to_bool_array` cast exactly.
/// - Result dtype = `a`'s dtype (== `b`'s — see the dtype contract). The
///   selected VALUES are copied verbatim, so a `NaN` in `a`/`b` FLOWS
///   THROUGH as a value (it is never inspected, only selected).
///
/// ## Shape contract (the §2.5-honest minimal surface)
///
/// All three operands must have the SAME shape (`cond.shape() ==
/// a.shape() == b.shape()`). numpy BROADCASTS `cond`/`a`/`b` to a common
/// shape; we keep the clean equal-shape contract for this batch — a
/// non-conformable triple raises `ShapeMismatch` (numpy's `ValueError`).
/// Broadcasting is a tracked follow-up (the existing M7.2 `index::
/// np_where` already broadcasts; this `manipulate` entry is the
/// equal-shape ecosystem-surface form that wires through the C-ABI).
///
/// ## Dtype contract
///
/// `a` and `b` must share a dtype (the result dtype). numpy PROMOTES a
/// mixed `a`/`b` pair to a common dtype; we raise `ShapeMismatch` on a
/// mismatch — the SAME equal-dtype rule `concatenate` uses (no silent
/// cross-dtype coercion, §2.2). A mixed-dtype promoting form is a tracked
/// follow-up. `cond` may be ANY dtype (its truthiness is read; it does
/// not participate in the result dtype) — typically `Bool` from `a < b`.
///
/// # Errors
///
/// `ShapeMismatch` (numpy's `ValueError`) when the three operands do not
/// all share one shape, OR when `a` and `b` have different dtypes.
pub fn where_select(cond: &Array, a: &Array, b: &Array) -> Result<Array, NumpyError> {
    // Shape contract — all three must agree (equal-shape; broadcasting is a
    // tracked follow-up). numpy raises `ValueError` on a non-conformable
    // triple; we raise `ShapeMismatch`.
    let cs = cond.shape();
    let as_ = a.shape();
    let bs = b.shape();
    if cs != as_ || as_ != bs {
        return Err(NumpyError {
            kind: NumpyErrorKind::ShapeMismatch,
            message: format!(
                "where: all three operands must share one shape (equal-shape \
                 contract; broadcasting is a tracked follow-up) — cond {cs:?}, \
                 a {as_:?}, b {bs:?} differ"
            ),
        });
    }
    // Dtype contract — `a` and `b` must match (the result dtype). numpy's
    // eventual promotion point; we raise instead (no silent coercion, §2.2).
    if a.dtype() != b.dtype() {
        return Err(NumpyError {
            kind: NumpyErrorKind::ShapeMismatch,
            message: format!(
                "where: dtype mismatch {:?} vs {:?} between `a` and `b` \
                 (equal-dtype contract; cross-dtype promotion is a tracked \
                 follow-up)",
                a.dtype(),
                b.dtype()
            ),
        });
    }
    // `cond` → a bool mask. A `Bool`-dtype cond uses the value directly;
    // a numeric cond is truthy on any nonzero element (numpy parity). The
    // shapes already match, so the mask aligns element-for-element with
    // `a` / `b` in C-order. Mirrors `index::np_where`'s `to_bool_array`.
    let mask = where_to_bool(cond);
    // dtype(a) == dtype(b) is enforced above, so every match arm pairs
    // like-with-like; the `select` macro `Zip`s the (already-same-shape)
    // mask + a + b and copies the selected element verbatim (NaN flows
    // through as a VALUE — it is selected, never inspected).
    macro_rules! select {
        ($va:expr, $vb:expr, $ctor:path, $zero:expr) => {{
            let mut out = ArrayD::from_elem($va.raw_dim(), $zero);
            ndarray::Zip::from(&mut out)
                .and(&mask)
                .and($va)
                .and($vb)
                .for_each(|o, &c, &x, &y| {
                    *o = if c { x } else { y };
                });
            $ctor(out)
        }};
    }
    Ok(match (a, b) {
        (Array::Int32(x), Array::Int32(y)) => select!(x, y, Array::Int32, 0_i32),
        (Array::Int64(x), Array::Int64(y)) => select!(x, y, Array::Int64, 0_i64),
        (Array::Float32(x), Array::Float32(y)) => select!(x, y, Array::Float32, 0.0_f32),
        (Array::Float64(x), Array::Float64(y)) => select!(x, y, Array::Float64, 0.0_f64),
        (Array::Bool(x), Array::Bool(y)) => select!(x, y, Array::Bool, false),
        // Unreachable: the dtype-equality guard above already returned on
        // any mismatched (a, b) pair.
        _ => {
            return Err(NumpyError {
                kind: NumpyErrorKind::ShapeMismatch,
                message: "where: dtype mismatch between `a` and `b`".to_string(),
            });
        }
    })
}

/// Cast any numeric / bool `cond` to a `bool` mask: a `Bool`-dtype array
/// uses its value directly; a numeric array is truthy on any NONZERO
/// element (numpy: `0`/`0.0` false, every other value incl. `NaN` true).
/// Mirrors `index::to_bool_array` (the M7.2 `np_where` cast) so the two
/// `where` surfaces read `cond` truthiness identically.
fn where_to_bool(cond: &Array) -> ArrayD<bool> {
    match cond {
        Array::Int32(c) => c.mapv(|v| v != 0),
        Array::Int64(c) => c.mapv(|v| v != 0),
        Array::Float32(c) => c.mapv(|v| v != 0.0),
        Array::Float64(c) => c.mapv(|v| v != 0.0),
        Array::Bool(c) => c.clone(),
    }
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

// ---- BATCH-9 sort / argsort / unique / flatnonzero internals ----------

/// Wrap a `Vec<i64>` (already in result order) as a fresh owned 1-D
/// `Array::Int64`. The shared back-end of [`argsort`] / [`flatnonzero`]
/// (both produce `Int64` indices regardless of input dtype).
fn int64_1d(idx: Vec<i64>) -> Array {
    let len = idx.len();
    Array::Int64(
        ArrayD::from_shape_vec(IxDyn(&[len]), idx)
            .expect("1-D from_shape_vec with matching length is infallible"),
    )
}

/// Ascending sort of a totally-ordered (`Ord`) `ArrayD<T>` → a fresh 1-D
/// `ArrayD<T>`. The int / bool [`sort`] arm; `T::cmp` is a total order so
/// `sort` (a stable merge sort) suffices and is deterministic.
fn sorted_total<T: Clone + Ord>(arr: &ArrayD<T>) -> ArrayD<T> {
    let mut flat: Vec<T> = arr.iter().cloned().collect();
    flat.sort();
    let len = flat.len();
    ArrayD::from_shape_vec(IxDyn(&[len]), flat)
        .expect("1-D from_shape_vec with matching length is infallible")
}

/// Ascending sort of a float `ArrayD<T>` with ALL `NaN` placed LAST
/// (numpy `np.sort` float semantics), regardless of `NaN` sign-bit. The
/// finite/inf subset is sorted via `total_cmp` (a total order on
/// non-NaN floats that agrees with `<`); the `NaN`s are appended in
/// input order. `f64::total_cmp` alone is WRONG here (it orders `NaN` by
/// sign-bit, placing `-NaN` first), so the partition is mandatory.
fn sorted_float_nan_last<T>(arr: &ArrayD<T>) -> ArrayD<T>
where
    T: Float,
{
    let (mut finite, nans): (Vec<T>, Vec<T>) = arr.iter().copied().partition(|v| !v.is_nan());
    finite.sort_by(T::total_cmp);
    finite.extend(nans);
    let len = finite.len();
    ArrayD::from_shape_vec(IxDyn(&[len]), finite)
        .expect("1-D from_shape_vec with matching length is infallible")
}

/// STABLE argsort of a totally-ordered (`Ord`) slice → the `Int64`
/// indices that would sort it ascending. The int / bool [`argsort`] arm.
/// `sort_by` is a stable sort, so equal keys keep their input order (the
/// deterministic, reproducible tie-break).
fn argsort_total<T: Ord>(flat: &[T]) -> Vec<i64> {
    let mut idx: Vec<usize> = (0..flat.len()).collect();
    idx.sort_by(|&i, &j| flat[i].cmp(&flat[j]));
    idx.into_iter().map(|i| i as i64).collect()
}

/// STABLE argsort of a float slice with the `NaN`-bearing indices LAST
/// (in input order), consistent with [`sorted_float_nan_last`]. The
/// non-NaN indices are stably sorted by `total_cmp`; the `NaN` indices
/// are appended in input order. The float [`argsort`] arm.
fn argsort_float_nan_last<T>(flat: &[T]) -> Vec<i64>
where
    T: Float,
{
    let (mut finite, nans): (Vec<usize>, Vec<usize>) =
        (0..flat.len()).partition(|&i| !flat[i].is_nan());
    finite.sort_by(|&i, &j| flat[i].total_cmp(&flat[j]));
    finite.extend(nans);
    finite.into_iter().map(|i| i as i64).collect()
}

/// Sorted-unique of a totally-ordered (`Ord`) `ArrayD<T>` → a fresh 1-D
/// `ArrayD<T>`. The int / bool [`unique`] arm: sort then `dedup` (which
/// removes CONSECUTIVE duplicates — correct after a sort).
fn unique_total<T: Clone + Ord>(arr: &ArrayD<T>) -> ArrayD<T> {
    let mut flat: Vec<T> = arr.iter().cloned().collect();
    flat.sort();
    flat.dedup();
    let len = flat.len();
    ArrayD::from_shape_vec(IxDyn(&[len]), flat)
        .expect("1-D from_shape_vec with matching length is infallible")
}

/// Sorted-unique of a float `ArrayD<T>` with the numpy 1.21+ `NaN`-
/// collapse rule: MULTIPLE `NaN` collapse to ONE trailing `NaN`
/// (`np.unique([nan,nan,1.,nan,2.]) == [1.,nan]`). The non-NaN values
/// are sorted + deduped (via `total_cmp`, a total order on non-NaN
/// floats); a SINGLE trailing `NaN` is appended iff the input held any.
fn unique_float_nan_collapse<T>(arr: &ArrayD<T>) -> ArrayD<T>
where
    T: Float,
{
    let (mut finite, nans): (Vec<T>, Vec<T>) = arr.iter().copied().partition(|v| !v.is_nan());
    finite.sort_by(T::total_cmp);
    finite.dedup();
    if let Some(&nan) = nans.first() {
        // Collapse every NaN to the FIRST one (numpy keeps one trailing
        // NaN; any NaN bit-pattern renders as `NaN`).
        finite.push(nan);
    }
    let len = finite.len();
    ArrayD::from_shape_vec(IxDyn(&[len]), finite)
        .expect("1-D from_shape_vec with matching length is infallible")
}

/// Flat C-order indices of the elements satisfying `is_nonzero`. The
/// shared body of every [`flatnonzero`] arm (`iter()` walks C-order, so
/// the enumerated positions ARE the numpy flat indices). Returns `Int64`
/// indices.
fn nonzero_idx<'a, T, I, P>(iter: I, is_nonzero: P) -> Vec<i64>
where
    T: 'a,
    I: Iterator<Item = &'a T>,
    P: Fn(&T) -> bool,
{
    iter.enumerate()
        .filter_map(|(i, v)| if is_nonzero(v) { Some(i as i64) } else { None })
        .collect()
}

/// Minimal float abstraction over the two float dtypes (`f32` / `f64`)
/// — just the `is_nan` predicate + the `total_cmp` total order the
/// BATCH-9 float arms need (`PartialEq` for the `unique` `dedup`).
/// Implemented for the two `ndarray` float element types; deliberately
/// tiny (NOT a `num-traits` dependency — the no-new-dep constraint).
trait Float: Copy + PartialEq {
    fn is_nan(self) -> bool;
    fn total_cmp(&self, other: &Self) -> std::cmp::Ordering;
}

impl Float for f32 {
    fn is_nan(self) -> bool {
        f32::is_nan(self)
    }
    fn total_cmp(&self, other: &Self) -> std::cmp::Ordering {
        f32::total_cmp(self, other)
    }
}

impl Float for f64 {
    fn is_nan(self) -> bool {
        f64::is_nan(self)
    }
    fn total_cmp(&self, other: &Self) -> std::cmp::Ordering {
        f64::total_cmp(self, other)
    }
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

    // ---- where_select (3-arg np.where) ----
    // Oracle values captured from numpy 2.x via the allowed
    // `/opt/homebrew/bin/python3.11` interpreter (numpy 2.4.6).

    use crate::constructors::array_bool;

    #[test]
    fn where_bool_cond_selects_elementwise() {
        // np.where([True,False,True],[1,2,3],[4,5,6]) -> [1,5,3].
        let cond = array_bool(&[true, false, true], &[3]).unwrap();
        let a = array_f64(&[1., 2., 3.], &[3]).unwrap();
        let b = array_f64(&[4., 5., 6.], &[3]).unwrap();
        let r = where_select(&cond, &a, &b).unwrap();
        assert_eq!(r.shape(), vec![3]);
        assert_eq!(f64_vals(&r), vec![1., 5., 3.]);
        assert_eq!(r.dtype(), a.dtype());
    }

    #[test]
    fn where_all_true_returns_a() {
        // np.where([True,True,True], a, b) -> a (every lane picks a).
        let cond = array_bool(&[true, true, true], &[3]).unwrap();
        let a = array_f64(&[1., 2., 3.], &[3]).unwrap();
        let b = array_f64(&[7., 8., 9.], &[3]).unwrap();
        let r = where_select(&cond, &a, &b).unwrap();
        assert_eq!(f64_vals(&r), vec![1., 2., 3.]);
    }

    #[test]
    fn where_all_false_returns_b() {
        // np.where([False,False,False], a, b) -> b (every lane picks b).
        let cond = array_bool(&[false, false, false], &[3]).unwrap();
        let a = array_f64(&[1., 2., 3.], &[3]).unwrap();
        let b = array_f64(&[7., 8., 9.], &[3]).unwrap();
        let r = where_select(&cond, &a, &b).unwrap();
        assert_eq!(f64_vals(&r), vec![7., 8., 9.]);
    }

    #[test]
    fn where_numeric_cond_nonzero_is_true() {
        // A numeric (non-bool) cond: nonzero is true (numpy parity).
        // np.where([0.,1.,0.],[1,2,3],[4,5,6]) -> [4,2,6].
        let cond = array_f64(&[0., 1., 0.], &[3]).unwrap();
        let a = array_f64(&[1., 2., 3.], &[3]).unwrap();
        let b = array_f64(&[4., 5., 6.], &[3]).unwrap();
        let r = where_select(&cond, &a, &b).unwrap();
        assert_eq!(f64_vals(&r), vec![4., 2., 6.]);
    }

    #[test]
    fn where_nan_flows_through_as_value() {
        // NaN in a/b is SELECTED as a value (never inspected for truthiness).
        // np.where([True,False],[NaN,2.],[5.,NaN]) -> [NaN, NaN].
        let cond = array_bool(&[true, false], &[2]).unwrap();
        let a = array_f64(&[f64::NAN, 2.], &[2]).unwrap();
        let b = array_f64(&[5., f64::NAN], &[2]).unwrap();
        let r = where_select(&cond, &a, &b).unwrap();
        match r {
            Array::Float64(arr) => {
                let v: Vec<f64> = arr.iter().copied().collect();
                assert!(v[0].is_nan(), "lane 0 picks a[0]=NaN; got {}", v[0]);
                assert!(v[1].is_nan(), "lane 1 picks b[1]=NaN; got {}", v[1]);
            }
            _ => panic!("expected Float64 result"),
        }
    }

    #[test]
    fn where_2d_same_shape() {
        // 3-arg where on a (2,2): mask picks per element, C-order.
        // np.where([[T,F],[F,T]],[[1,2],[3,4]],[[5,6],[7,8]]) -> [[1,6],[7,4]].
        let cond = array_bool(&[true, false, false, true], &[2, 2]).unwrap();
        let a = array_f64(&[1., 2., 3., 4.], &[2, 2]).unwrap();
        let b = array_f64(&[5., 6., 7., 8.], &[2, 2]).unwrap();
        let r = where_select(&cond, &a, &b).unwrap();
        assert_eq!(r.shape(), vec![2, 2]);
        assert_eq!(f64_vals(&r), vec![1., 6., 7., 4.]);
    }

    #[test]
    fn where_preserves_int_dtype() {
        let cond = array_bool(&[true, false], &[2]).unwrap();
        let a = array_i64(&[10, 20], &[2]).unwrap();
        let b = array_i64(&[30, 40], &[2]).unwrap();
        let r = where_select(&cond, &a, &b).unwrap();
        assert_eq!(r.dtype(), crate::dtype::Dtype::Int64);
        match r {
            Array::Int64(arr) => {
                assert_eq!(arr.iter().copied().collect::<Vec<_>>(), vec![10, 40]);
            }
            _ => panic!("dtype not preserved"),
        }
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn where_cond_built_from_comparison() {
        // Prove the bool-mask integration: cond = (a < b) is a Bool-dtype
        // Array (ADR-0077), fed straight into where_select.
        // a=[1,5], b=[3,2] -> a<b=[True,False]; where(cond,[10,20],[30,40])
        // -> [10, 40].
        let a = array_f64(&[1., 5.], &[2]).unwrap();
        let b = array_f64(&[3., 2.], &[2]).unwrap();
        let cond = a.lt(&b).unwrap();
        assert_eq!(cond.dtype(), crate::dtype::Dtype::Bool);
        let x = array_f64(&[10., 20.], &[2]).unwrap();
        let y = array_f64(&[30., 40.], &[2]).unwrap();
        let r = where_select(&cond, &x, &y).unwrap();
        assert_eq!(f64_vals(&r), vec![10., 40.]);
    }

    #[test]
    fn where_nonconformable_shape_is_err() {
        // cond (3,) vs a/b (2,) -> ShapeMismatch (equal-shape contract).
        let cond = array_bool(&[true, false, true], &[3]).unwrap();
        let a = array_f64(&[1., 2.], &[2]).unwrap();
        let b = array_f64(&[3., 4.], &[2]).unwrap();
        let err = where_select(&cond, &a, &b).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
    }

    #[test]
    fn where_a_b_shape_mismatch_is_err() {
        // a (2,) vs b (3,) -> ShapeMismatch even with a matching-len cond
        // on one side (all three must agree).
        let cond = array_bool(&[true, false], &[2]).unwrap();
        let a = array_f64(&[1., 2.], &[2]).unwrap();
        let b = array_f64(&[3., 4., 5.], &[3]).unwrap();
        let err = where_select(&cond, &a, &b).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
    }

    #[test]
    fn where_a_b_dtype_mismatch_is_err() {
        // a f64 vs b i64 -> ShapeMismatch (equal-dtype contract).
        let cond = array_bool(&[true, false], &[2]).unwrap();
        let a = array_f64(&[1., 2.], &[2]).unwrap();
        let b = array_i64(&[3, 4], &[2]).unwrap();
        let err = where_select(&cond, &a, &b).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
    }

    // ---- BATCH 9: sort / argsort / unique / flatnonzero ----
    // Oracle values captured from numpy 2.x via the allowed
    // `/opt/homebrew/bin/python3.11` interpreter (numpy 2.4.6).
    use crate::constructors::{array_f32, array_i32};

    fn i64_vals(a: &Array) -> Vec<i64> {
        match a {
            Array::Int64(arr) => arr.iter().copied().collect(),
            _ => panic!("expected Int64"),
        }
    }

    // ---- sort ----

    #[test]
    fn sort_f64_ascending() {
        // np.sort([3.,1.,2.]) == [1.,2.,3.] (1-D, float64 preserved).
        let a = array_f64(&[3., 1., 2.], &[3]).unwrap();
        let s = sort(&a);
        assert_eq!(s.dtype(), crate::dtype::Dtype::Float64);
        assert_eq!(s.shape(), vec![3]);
        assert_eq!(f64_vals(&s), vec![1., 2., 3.]);
    }

    #[test]
    fn sort_2d_flattens_then_sorts() {
        // np.sort flattens the no-axis default: np.sort([[3,1],[2,0]],
        // axis=None) == [0,1,2,3].
        let a = array_f64(&[3., 1., 2., 0.], &[2, 2]).unwrap();
        let s = sort(&a);
        assert_eq!(s.shape(), vec![4]);
        assert_eq!(f64_vals(&s), vec![0., 1., 2., 3.]);
    }

    #[test]
    fn sort_f64_nan_last() {
        // np.sort([3.,nan,1.]) == [1.,3.,nan] (NaN sorts to the END).
        let a = array_f64(&[3., f64::NAN, 1.], &[3]).unwrap();
        match sort(&a) {
            Array::Float64(arr) => {
                let v: Vec<f64> = arr.iter().copied().collect();
                assert_eq!(v[0], 1.);
                assert_eq!(v[1], 3.);
                assert!(v[2].is_nan(), "NaN must be last; got {}", v[2]);
            }
            _ => panic!("dtype not preserved"),
        }
    }

    #[test]
    fn sort_signed_nan_all_last() {
        // np.sort([nan,1.,-nan,2.]) == [1.,2.,nan,nan] — BOTH NaN last
        // regardless of sign-bit (the total_cmp-alone trap this guards).
        let neg_nan = f64::from_bits(f64::NAN.to_bits() | (1 << 63));
        assert!(neg_nan.is_nan() && neg_nan.is_sign_negative());
        let a = array_f64(&[f64::NAN, 1., neg_nan, 2.], &[4]).unwrap();
        match sort(&a) {
            Array::Float64(arr) => {
                let v: Vec<f64> = arr.iter().copied().collect();
                assert_eq!(&v[0..2], &[1., 2.]);
                assert!(v[2].is_nan() && v[3].is_nan(), "both NaN last; got {v:?}");
            }
            _ => panic!("dtype not preserved"),
        }
    }

    #[test]
    fn sort_preserves_int_dtype() {
        // np.sort([3,1,2]) == [1,2,3] (int64 preserved).
        let a = array_i64(&[3, 1, 2], &[3]).unwrap();
        let s = sort(&a);
        assert_eq!(s.dtype(), crate::dtype::Dtype::Int64);
        assert_eq!(i64_vals(&s), vec![1, 2, 3]);
    }

    #[test]
    fn sort_preserves_f32_dtype() {
        // np.sort(float32([3,1,2])).dtype == float32.
        let a = array_f32(&[3., 1., 2.], &[3]).unwrap();
        let s = sort(&a);
        assert_eq!(s.dtype(), crate::dtype::Dtype::Float32);
        match s {
            Array::Float32(arr) => {
                assert_eq!(arr.iter().copied().collect::<Vec<_>>(), vec![1., 2., 3.]);
            }
            _ => panic!("dtype not preserved"),
        }
    }

    #[test]
    fn sort_int32_dtype() {
        let a = array_i32(&[5, 2, 8, 1], &[4]).unwrap();
        let s = sort(&a);
        assert_eq!(s.dtype(), crate::dtype::Dtype::Int32);
        match s {
            Array::Int32(arr) => {
                assert_eq!(arr.iter().copied().collect::<Vec<_>>(), vec![1, 2, 5, 8]);
            }
            _ => panic!("dtype not preserved"),
        }
    }

    #[test]
    fn sort_empty() {
        let a = array_f64(&[], &[0]).unwrap();
        let s = sort(&a);
        assert_eq!(s.shape(), vec![0]);
        assert!(f64_vals(&s).is_empty());
    }

    // ---- argsort ----

    #[test]
    fn argsort_basic_i64_indices() {
        // np.argsort([3.,1.,2.]) == [1,2,0]; result dtype int64.
        let a = array_f64(&[3., 1., 2.], &[3]).unwrap();
        let s = argsort(&a);
        assert_eq!(s.dtype(), crate::dtype::Dtype::Int64);
        assert_eq!(i64_vals(&s), vec![1, 2, 0]);
    }

    #[test]
    fn argsort_stable_on_duplicate_keys() {
        // np.argsort([2,1,2,1,3]) == [1,3,0,2,4]: the two 1s keep input
        // order (idx 1 before 3) and the two 2s keep input order (idx 0
        // before 2) — PROVES the stable tie-break.
        let a = array_i64(&[2, 1, 2, 1, 3], &[5]).unwrap();
        let s = argsort(&a);
        assert_eq!(i64_vals(&s), vec![1, 3, 0, 2, 4]);
    }

    #[test]
    fn argsort_nan_indices_last() {
        // np.argsort([5.,nan,3.,nan,1.]) == [4,2,0,1,3]: non-NaN sorted
        // (1<3<5 -> idx 4,2,0), then NaN indices in input order (1,3).
        let a = array_f64(&[5., f64::NAN, 3., f64::NAN, 1.], &[5]).unwrap();
        let s = argsort(&a);
        assert_eq!(i64_vals(&s), vec![4, 2, 0, 1, 3]);
    }

    #[test]
    fn argsort_int_dtype_input_still_i64_indices() {
        // The index dtype is ALWAYS int64, even for an int32 input.
        let a = array_i32(&[30, 10, 20], &[3]).unwrap();
        let s = argsort(&a);
        assert_eq!(s.dtype(), crate::dtype::Dtype::Int64);
        assert_eq!(i64_vals(&s), vec![1, 2, 0]);
    }

    // ---- unique ----

    #[test]
    fn unique_int_sorted_dedupe() {
        // np.unique([3,1,2,1,3]) == [1,2,3] (int64 preserved).
        let a = array_i64(&[3, 1, 2, 1, 3], &[5]).unwrap();
        let u = unique(&a);
        assert_eq!(u.dtype(), crate::dtype::Dtype::Int64);
        assert_eq!(i64_vals(&u), vec![1, 2, 3]);
    }

    #[test]
    fn unique_f64_preserves_dtype() {
        // np.unique([3.,1.,2.,1.,3.]) == [1.,2.,3.] (float64 preserved).
        let a = array_f64(&[3., 1., 2., 1., 3.], &[5]).unwrap();
        let u = unique(&a);
        assert_eq!(u.dtype(), crate::dtype::Dtype::Float64);
        assert_eq!(f64_vals(&u), vec![1., 2., 3.]);
    }

    #[test]
    fn unique_nan_collapses_to_one_trailing() {
        // np.unique([nan,1.,nan]) == [1.,nan] (one trailing NaN).
        let a = array_f64(&[f64::NAN, 1., f64::NAN], &[3]).unwrap();
        match unique(&a) {
            Array::Float64(arr) => {
                let v: Vec<f64> = arr.iter().copied().collect();
                assert_eq!(v.len(), 2, "one value + one NaN; got {v:?}");
                assert_eq!(v[0], 1.);
                assert!(v[1].is_nan(), "single trailing NaN; got {}", v[1]);
            }
            _ => panic!("dtype not preserved"),
        }
    }

    #[test]
    fn unique_multi_nan_collapses_to_one() {
        // np.unique([nan,nan,1.,nan,2.]) == [1.,2.,nan] (one NaN despite 3).
        let a = array_f64(&[f64::NAN, f64::NAN, 1., f64::NAN, 2.], &[5]).unwrap();
        match unique(&a) {
            Array::Float64(arr) => {
                let v: Vec<f64> = arr.iter().copied().collect();
                assert_eq!(v.len(), 3, "two values + one NaN; got {v:?}");
                assert_eq!(&v[0..2], &[1., 2.]);
                assert!(v[2].is_nan(), "single trailing NaN; got {}", v[2]);
            }
            _ => panic!("dtype not preserved"),
        }
    }

    #[test]
    fn unique_2d_flattens() {
        // np.unique([[3,1],[2,1]]) == [1,2,3] (flattens C-order first).
        let a = array_i64(&[3, 1, 2, 1], &[2, 2]).unwrap();
        let u = unique(&a);
        assert_eq!(u.shape(), vec![3]);
        assert_eq!(i64_vals(&u), vec![1, 2, 3]);
    }

    // ---- flatnonzero ----

    #[test]
    fn flatnonzero_int_indices() {
        // np.flatnonzero([0,5,0,2]) == [1,3]; result dtype int64.
        let a = array_i64(&[0, 5, 0, 2], &[4]).unwrap();
        let nz = flatnonzero(&a);
        assert_eq!(nz.dtype(), crate::dtype::Dtype::Int64);
        assert_eq!(i64_vals(&nz), vec![1, 3]);
    }

    #[test]
    fn flatnonzero_nan_is_nonzero() {
        // np.flatnonzero([0.,nan,0.]) == [1] (NaN is != 0.0 -> included).
        let a = array_f64(&[0., f64::NAN, 0.], &[3]).unwrap();
        let nz = flatnonzero(&a);
        assert_eq!(i64_vals(&nz), vec![1]);
    }

    #[test]
    fn flatnonzero_2d_flattens_c_order() {
        // np.flatnonzero([[0,5],[0,2]]) == [1,3] (C-order flat indices).
        let a = array_i64(&[0, 5, 0, 2], &[2, 2]).unwrap();
        let nz = flatnonzero(&a);
        assert_eq!(i64_vals(&nz), vec![1, 3]);
    }

    #[test]
    fn flatnonzero_float_input_still_i64_indices() {
        // f64 input -> int64 index dtype (the index dtype never tracks input).
        let a = array_f64(&[1., 0., 3.], &[3]).unwrap();
        let nz = flatnonzero(&a);
        assert_eq!(nz.dtype(), crate::dtype::Dtype::Int64);
        assert_eq!(i64_vals(&nz), vec![0, 2]);
    }

    #[test]
    fn flatnonzero_all_zero_is_empty() {
        let a = array_i64(&[0, 0, 0], &[3]).unwrap();
        let nz = flatnonzero(&a);
        assert_eq!(nz.dtype(), crate::dtype::Dtype::Int64);
        assert!(i64_vals(&nz).is_empty());
    }

    // ---- chain: sort(unique(a)) ----

    #[test]
    fn chain_sort_of_unique() {
        // unique already returns sorted, so sort(unique([3,1,2,1,3]))
        // == [1,2,3] — proves the result Array feeds the next op.
        let a = array_f64(&[3., 1., 2., 1., 3.], &[5]).unwrap();
        let r = sort(&unique(&a));
        assert_eq!(f64_vals(&r), vec![1., 2., 3.]);
    }
}
