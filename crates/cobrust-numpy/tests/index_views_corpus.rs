//! M7.2 view-vs-copy semantics — table-driven assertions per ADR-0015 §3.
//!
//! Tests in this file demonstrate:
//!   - **Basic slicing produces a view.** Mutating through the view
//!     is observable on the parent (`mut_view_mutates_parent`).
//!   - **Advanced indexing produces a copy.** Mutating the result is
//!     not observable on the parent (`take_returns_independent_copy`,
//!     `mask_returns_independent_copy`).
//!   - **np.where always copies.** Mutating the result is not
//!     observable on either input.
//!   - **Single-int indexing produces a view** with one fewer axis.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::approx_constant)]
#![allow(clippy::imprecise_flops)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::manual_repeat_n)]
#![allow(clippy::if_not_else)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::print_stderr)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::needless_lifetimes)]

use cobrust_numpy::{Array, SliceSpec, array_bool, array_f64, array_i32, array_i64, np_where};

fn data_i32(a: &Array) -> Vec<i32> {
    match a {
        Array::Int32(v) => v.iter().copied().collect(),
        _ => panic!("expected Int32 dtype"),
    }
}

fn data_i64(a: &Array) -> Vec<i64> {
    match a {
        Array::Int64(v) => v.iter().copied().collect(),
        _ => panic!("expected Int64 dtype"),
    }
}

fn data_f64(a: &Array) -> Vec<f64> {
    match a {
        Array::Float64(v) => v.iter().copied().collect(),
        _ => panic!("expected Float64 dtype"),
    }
}

// ---- Basic slicing → VIEW (mutate-through-view propagates) ----------

#[test]
fn basic_slice_mut_view_mutates_parent_int32() {
    let mut a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    {
        let mut v = a.slice_mut(SliceSpec::range(1, 4)).unwrap();
        v.fill_f64(99.0);
    }
    assert_eq!(data_i32(&a), vec![1, 99, 99, 99, 5]);
}

#[test]
fn basic_slice_mut_view_mutates_parent_int64() {
    let mut a = array_i64(&[10, 20, 30, 40], &[4]).unwrap();
    {
        let mut v = a.slice_mut(SliceSpec::range(0, 2)).unwrap();
        v.fill_f64(0.0);
    }
    assert_eq!(data_i64(&a), vec![0, 0, 30, 40]);
}

#[test]
fn basic_slice_mut_view_mutates_parent_float64() {
    let mut a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0], &[5]).unwrap();
    {
        let mut v = a.slice_mut(SliceSpec::stepped(0, 5, 2)).unwrap();
        v.fill_f64(7.0);
    }
    // Indices 0, 2, 4 mutated; 1, 3 untouched.
    assert_eq!(data_f64(&a), vec![7.0, 2.0, 7.0, 4.0, 7.0]);
}

#[test]
fn basic_slice_view_observes_mutation_in_parent() {
    // Mutate the parent first, then take a view: view should reflect it.
    let mut a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    if let Array::Int32(arr) = &mut a {
        arr[1] = 100;
    }
    let v = a.slice(SliceSpec::range(0, 3)).unwrap();
    let owned = v.to_owned();
    assert_eq!(data_i32(&owned), vec![1, 100, 3]);
}

#[test]
fn basic_slice_2d_mut_view_mutates_parent() {
    let mut a = array_i32(&[1, 2, 3, 4, 5, 6, 7, 8], &[4, 2]).unwrap();
    {
        let mut v = a.slice_mut(SliceSpec::range(1, 3)).unwrap();
        v.fill_f64(0.0);
    }
    // First row (1, 2) and last row (7, 8) preserved; middle two zeroed.
    assert_eq!(data_i32(&a), vec![1, 2, 0, 0, 0, 0, 7, 8]);
}

// ---- Advanced indexing (take) → COPY (independent of parent) -------

#[test]
fn take_returns_independent_copy() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let mut taken = a.take(&[0, 2, 4]).unwrap();
    if let Array::Int32(arr) = &mut taken {
        arr[0] = 99;
    }
    // Parent untouched.
    assert_eq!(data_i32(&a), vec![1, 2, 3, 4, 5]);
    assert_eq!(data_i32(&taken), vec![99, 3, 5]);
}

#[test]
fn take_repeated_indices_produces_independent_elements() {
    let a = array_i64(&[10, 20, 30], &[3]).unwrap();
    let mut taken = a.take(&[1, 1, 1]).unwrap();
    if let Array::Int64(arr) = &mut taken {
        arr[0] = 0;
    }
    // Other "1, 1" entries should be unaffected (they're already
    // copies; mutation on element 0 is local).
    assert_eq!(data_i64(&a), vec![10, 20, 30]);
    assert_eq!(data_i64(&taken), vec![0, 20, 20]);
}

// ---- Advanced indexing (mask) → COPY (independent of parent) -------

#[test]
fn mask_returns_independent_copy() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let m = array_bool(&[true, false, true, false, true], &[5]).unwrap();
    let mut masked = a.mask(&m).unwrap();
    if let Array::Int32(arr) = &mut masked {
        arr[0] = 99;
    }
    assert_eq!(data_i32(&a), vec![1, 2, 3, 4, 5]);
    assert_eq!(data_i32(&masked), vec![99, 3, 5]);
}

#[test]
fn mask_2d_returns_1d_copy() {
    let a = array_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let m = array_bool(&[true, false, true, true], &[2, 2]).unwrap();
    let masked = a.mask(&m).unwrap();
    assert_eq!(masked.shape(), vec![3]);
    assert_eq!(data_i32(&masked), vec![1, 3, 4]);
    // Parent shape unchanged.
    assert_eq!(a.shape(), vec![2, 2]);
}

// ---- np.where → COPY (independent of all three operands) -----------

#[test]
fn np_where_returns_copy_independent_of_x() {
    let cond = array_bool(&[true, false, true], &[3]).unwrap();
    let x = array_i32(&[10, 20, 30], &[3]).unwrap();
    let y = array_i32(&[1, 2, 3], &[3]).unwrap();
    let mut out = np_where(&cond, &x, &y).unwrap();
    if let Array::Int32(arr) = &mut out {
        arr[0] = 99;
    }
    assert_eq!(data_i32(&x), vec![10, 20, 30]);
    assert_eq!(data_i32(&y), vec![1, 2, 3]);
    assert_eq!(data_i32(&out), vec![99, 2, 30]);
}

// ---- Single-int → VIEW (sub-shape view; clone for ownership) -------

#[test]
fn single_int_view_drops_first_axis() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6], &[3, 2]).unwrap();
    let v = a.index_single(1).unwrap();
    assert_eq!(v.shape(), vec![2]);
    let owned = v.to_owned();
    assert_eq!(data_i32(&owned), vec![3, 4]);
}

// ---- Combination: chain of views and copies preserves rules --------

#[test]
fn chain_slice_then_take_preserves_copy_semantics() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6], &[6]).unwrap();
    let mid = a.slice(SliceSpec::range(1, 5)).unwrap().to_owned();
    let mut taken = mid.take(&[0, 2]).unwrap();
    if let Array::Int32(arr) = &mut taken {
        arr[0] = 0;
    }
    // Parent untouched; mid (intermediate copy) untouched.
    assert_eq!(data_i32(&a), vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(data_i32(&mid), vec![2, 3, 4, 5]);
    assert_eq!(data_i32(&taken), vec![0, 4]);
}

#[test]
fn slice_view_then_to_owned_is_independent() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let owned = a.slice(SliceSpec::range(1, 4)).unwrap().to_owned();
    // owned is now independent.
    let mut owned2 = owned.clone();
    if let Array::Int32(arr) = &mut owned2 {
        arr[0] = 99;
    }
    // Original `owned` and parent `a` untouched.
    assert_eq!(data_i32(&owned), vec![2, 3, 4]);
    assert_eq!(data_i32(&a), vec![1, 2, 3, 4, 5]);
}

// ---- View dtype preservation table ---------------------------------

#[test]
fn view_dtype_preservation_across_dtypes() {
    let cases: Vec<(cobrust_numpy::Dtype, Vec<f64>)> = vec![
        (cobrust_numpy::Dtype::Int32, vec![1.0, 2.0, 3.0]),
        (cobrust_numpy::Dtype::Int64, vec![1.0, 2.0, 3.0]),
        (cobrust_numpy::Dtype::Float32, vec![1.0, 2.0, 3.0]),
        (cobrust_numpy::Dtype::Float64, vec![1.0, 2.0, 3.0]),
        (cobrust_numpy::Dtype::Bool, vec![1.0, 0.0, 1.0]),
    ];
    for (dtype, vals) in cases {
        let a = cobrust_numpy::array(&vals, &[3], dtype).unwrap();
        let v = a.slice(SliceSpec::full()).unwrap();
        assert_eq!(v.dtype(), dtype);
        assert_eq!(v.shape(), vec![3]);
    }
}
