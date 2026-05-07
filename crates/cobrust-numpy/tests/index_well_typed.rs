//! M7.2 well-typed indexing program suite — at least 50 programs accepted.
//!
//! Per ADR-0015 §"M7.2 scope window": ≥ 50 well-typed indexing
//! programs across basic slicing, integer-array indexing, boolean
//! masks, single-int indexing, and `np.where`. Each program is a
//! sequence of cobrust-numpy calls that compute a result and assert
//! on observable invariants (shape, dtype, data).

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

use cobrust_numpy::{
    Array, Dtype, Index, SliceSpec, array_bool, array_f32, array_f64, array_i32, array_i64,
    np_where,
};

fn data_f64(a: &Array) -> Vec<f64> {
    match a {
        Array::Int32(v) => v.iter().map(|&x| f64::from(x)).collect(),
        Array::Int64(v) => v.iter().map(|&x| x as f64).collect(),
        Array::Float32(v) => v.iter().map(|&x| f64::from(x)).collect(),
        Array::Float64(v) => v.iter().copied().collect(),
        Array::Bool(v) => v.iter().map(|&x| f64::from(u8::from(x))).collect(),
    }
}

// ---- Basic slicing — 1-10 ------------------------------------------------

#[test]
fn t01_basic_slice_int32_full() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let v = a.slice(SliceSpec::full()).unwrap();
    assert_eq!(v.shape(), vec![5]);
    assert_eq!(v.dtype(), Dtype::Int32);
}

#[test]
fn t02_basic_slice_int32_range() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let owned = a.slice(SliceSpec::range(1, 4)).unwrap().to_owned();
    assert_eq!(owned.shape(), vec![3]);
    assert_eq!(data_f64(&owned), vec![2.0, 3.0, 4.0]);
}

#[test]
fn t03_basic_slice_int64_step_two() {
    let a = array_i64(&[10, 20, 30, 40, 50, 60], &[6]).unwrap();
    let owned = a.slice(SliceSpec::stepped(0, 6, 2)).unwrap().to_owned();
    assert_eq!(data_f64(&owned), vec![10.0, 30.0, 50.0]);
}

#[test]
fn t04_basic_slice_float64_negative_indices() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0], &[5]).unwrap();
    let owned = a.slice(SliceSpec::range(-3, -1)).unwrap().to_owned();
    assert_eq!(data_f64(&owned), vec![3.0, 4.0]);
}

#[test]
fn t05_basic_slice_float32_step_negative() {
    let a = array_f32(&[1.0, 2.0, 3.0, 4.0, 5.0], &[5]).unwrap();
    let owned = a.slice(SliceSpec::step_only(-1)).unwrap().to_owned();
    assert_eq!(data_f64(&owned), vec![5.0, 4.0, 3.0, 2.0, 1.0]);
}

#[test]
fn t06_basic_slice_clamp_out_of_range_high() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let owned = a.slice(SliceSpec::range(0, 100)).unwrap().to_owned();
    assert_eq!(owned.shape(), vec![3]);
}

#[test]
fn t07_basic_slice_clamp_negative_far_low() {
    let a = array_i32(&[1, 2, 3, 4], &[4]).unwrap();
    let owned = a.slice(SliceSpec::range(-100, 100)).unwrap().to_owned();
    assert_eq!(owned.shape(), vec![4]);
}

#[test]
fn t08_basic_slice_empty_when_start_ge_stop() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let owned = a.slice(SliceSpec::range(3, 1)).unwrap().to_owned();
    assert_eq!(owned.shape(), vec![0]);
}

#[test]
fn t09_basic_slice_2d_first_axis() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6, 7, 8], &[4, 2]).unwrap();
    let owned = a.slice(SliceSpec::range(1, 3)).unwrap().to_owned();
    assert_eq!(owned.shape(), vec![2, 2]);
    assert_eq!(data_f64(&owned), vec![3.0, 4.0, 5.0, 6.0]);
}

#[test]
fn t10_basic_slice_returns_view_dtype_preserves() {
    for d in [
        Dtype::Int32,
        Dtype::Int64,
        Dtype::Float32,
        Dtype::Float64,
        Dtype::Bool,
    ] {
        let a = cobrust_numpy::ones(&[5], d).unwrap();
        let v = a.slice(SliceSpec::range(0, 5)).unwrap();
        assert_eq!(v.dtype(), d);
    }
}

// ---- Single-int indexing — 11-20 ----------------------------------------

#[test]
fn t11_single_index_first_element() {
    let a = array_i32(&[10, 20, 30], &[3]).unwrap();
    let v = a.index_single(0).unwrap();
    assert_eq!(v.shape(), Vec::<usize>::new());
    assert_eq!(v.dtype(), Dtype::Int32);
}

#[test]
fn t12_single_index_negative() {
    let a = array_i32(&[10, 20, 30], &[3]).unwrap();
    let v = a.index_single(-1).unwrap().to_owned();
    assert_eq!(data_f64(&v), vec![30.0]);
}

#[test]
fn t13_single_index_2d_drops_first_axis() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6], &[3, 2]).unwrap();
    let owned = a.index_single(1).unwrap().to_owned();
    assert_eq!(owned.shape(), vec![2]);
    assert_eq!(data_f64(&owned), vec![3.0, 4.0]);
}

#[test]
fn t14_single_index_via_index_get() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let r = a.index_get(&[Index::Single(2)]).unwrap();
    assert_eq!(data_f64(&r), vec![3.0]);
}

#[test]
fn t15_single_index_negative_via_index_get() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let r = a.index_get(&[Index::Single(-2)]).unwrap();
    assert_eq!(data_f64(&r), vec![4.0]);
}

#[test]
fn t16_single_index_dtype_preserved_int64() {
    let a = array_i64(&[100, 200, 300], &[3]).unwrap();
    let v = a.index_single(0).unwrap().to_owned();
    assert_eq!(v.dtype(), Dtype::Int64);
    assert_eq!(data_f64(&v), vec![100.0]);
}

#[test]
fn t17_single_index_float64() {
    let a = array_f64(&[1.5, 2.5, 3.5], &[3]).unwrap();
    let v = a.index_single(1).unwrap().to_owned();
    assert_eq!(data_f64(&v), vec![2.5]);
}

#[test]
fn t18_single_index_bool() {
    let a = array_bool(&[true, false, true], &[3]).unwrap();
    let v = a.index_single(0).unwrap().to_owned();
    assert_eq!(v.dtype(), Dtype::Bool);
}

#[test]
fn t19_single_index_full_negative_range() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    for i in -5..0_i64 {
        let v = a.index_single(i).unwrap().to_owned();
        assert_eq!(data_f64(&v), vec![(5 + i + 1) as f64]);
    }
}

#[test]
fn t20_single_index_all_positive() {
    let a = array_i32(&[10, 20, 30, 40], &[4]).unwrap();
    for i in 0..4_i64 {
        let v = a.index_single(i).unwrap().to_owned();
        assert_eq!(data_f64(&v), vec![10.0 + 10.0 * (i as f64)]);
    }
}

// ---- Integer-array indexing (take) — 21-30 -----------------------------

#[test]
fn t21_take_basic_int32() {
    let a = array_i32(&[10, 20, 30, 40, 50], &[5]).unwrap();
    let r = a.take(&[0, 2, 4]).unwrap();
    assert_eq!(data_f64(&r), vec![10.0, 30.0, 50.0]);
}

#[test]
fn t22_take_negative_indices() {
    let a = array_i32(&[10, 20, 30, 40, 50], &[5]).unwrap();
    let r = a.take(&[-1, -3, -5]).unwrap();
    assert_eq!(data_f64(&r), vec![50.0, 30.0, 10.0]);
}

#[test]
fn t23_take_with_repeats_returns_copy() {
    let a = array_i32(&[10, 20, 30], &[3]).unwrap();
    let r = a.take(&[0, 0, 0, 1, 2]).unwrap();
    assert_eq!(data_f64(&r), vec![10.0, 10.0, 10.0, 20.0, 30.0]);
}

#[test]
fn t24_take_2d_array() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6, 7, 8], &[4, 2]).unwrap();
    let r = a.take(&[0, 3]).unwrap();
    assert_eq!(r.shape(), vec![2, 2]);
    assert_eq!(data_f64(&r), vec![1.0, 2.0, 7.0, 8.0]);
}

#[test]
fn t25_take_empty_indices() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let r = a.take(&[]).unwrap();
    assert_eq!(r.shape(), vec![0]);
}

#[test]
fn t26_take_via_index_get() {
    let a = array_i32(&[10, 20, 30, 40, 50], &[5]).unwrap();
    let r = a.index_get(&[Index::IntArray(vec![0, 4, 2])]).unwrap();
    assert_eq!(data_f64(&r), vec![10.0, 50.0, 30.0]);
}

#[test]
fn t27_take_float64_dtype_preserved() {
    let a = array_f64(&[1.0, 2.5, 3.5, 4.0], &[4]).unwrap();
    let r = a.take(&[1, 3]).unwrap();
    assert_eq!(r.dtype(), Dtype::Float64);
    assert_eq!(data_f64(&r), vec![2.5, 4.0]);
}

#[test]
fn t28_take_bool_dtype_preserved() {
    let a = array_bool(&[true, false, true, false], &[4]).unwrap();
    let r = a.take(&[0, 2]).unwrap();
    assert_eq!(r.dtype(), Dtype::Bool);
    let crate_bool = match &r {
        Array::Bool(b) => b.iter().copied().collect::<Vec<_>>(),
        _ => panic!("expected Bool"),
    };
    assert_eq!(crate_bool, vec![true, true]);
}

#[test]
fn t29_take_int64_dtype_preserved() {
    let a = array_i64(&[100, 200, 300], &[3]).unwrap();
    let r = a.take(&[2, 0]).unwrap();
    assert_eq!(r.dtype(), Dtype::Int64);
    assert_eq!(data_f64(&r), vec![300.0, 100.0]);
}

#[test]
fn t30_take_float32_dtype_preserved() {
    let a = array_f32(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.take(&[2, 1, 0]).unwrap();
    assert_eq!(r.dtype(), Dtype::Float32);
    assert_eq!(data_f64(&r), vec![3.0, 2.0, 1.0]);
}

// ---- Boolean-mask indexing — 31-40 -------------------------------------

#[test]
fn t31_mask_basic_int32() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let m = array_bool(&[true, false, true, false, true], &[5]).unwrap();
    let r = a.mask(&m).unwrap();
    assert_eq!(data_f64(&r), vec![1.0, 3.0, 5.0]);
}

#[test]
fn t32_mask_all_false_returns_empty() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let m = array_bool(&[false, false, false], &[3]).unwrap();
    let r = a.mask(&m).unwrap();
    assert_eq!(r.shape(), vec![0]);
}

#[test]
fn t33_mask_all_true_returns_full_copy() {
    let a = array_i32(&[10, 20, 30], &[3]).unwrap();
    let m = array_bool(&[true, true, true], &[3]).unwrap();
    let r = a.mask(&m).unwrap();
    assert_eq!(r.shape(), vec![3]);
    assert_eq!(data_f64(&r), vec![10.0, 20.0, 30.0]);
}

#[test]
fn t34_mask_2d_flattens() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6], &[2, 3]).unwrap();
    let m = array_bool(&[true, false, true, false, true, false], &[2, 3]).unwrap();
    let r = a.mask(&m).unwrap();
    assert_eq!(r.shape(), vec![3]);
    assert_eq!(data_f64(&r), vec![1.0, 3.0, 5.0]);
}

#[test]
fn t35_mask_via_index_get() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let m = array_bool(&[true, false, true, false, true], &[5]).unwrap();
    let r = a.index_get(&[Index::BoolMask(m)]).unwrap();
    assert_eq!(data_f64(&r), vec![1.0, 3.0, 5.0]);
}

#[test]
fn t36_mask_float64_dtype() {
    let a = array_f64(&[1.5, 2.5, 3.5, 4.5], &[4]).unwrap();
    let m = array_bool(&[true, false, true, true], &[4]).unwrap();
    let r = a.mask(&m).unwrap();
    assert_eq!(r.dtype(), Dtype::Float64);
    assert_eq!(data_f64(&r), vec![1.5, 3.5, 4.5]);
}

#[test]
fn t37_mask_int64_dtype() {
    let a = array_i64(&[100, 200, 300], &[3]).unwrap();
    let m = array_bool(&[false, true, true], &[3]).unwrap();
    let r = a.mask(&m).unwrap();
    assert_eq!(r.dtype(), Dtype::Int64);
}

#[test]
fn t38_mask_via_comparison_pipeline() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let threshold = array_i32(&[3], &[1]).unwrap();
    let mask = a.gt(&threshold).unwrap();
    let r = a.mask(&mask).unwrap();
    assert_eq!(data_f64(&r), vec![4.0, 5.0]);
}

#[test]
fn t39_mask_float32_dtype_preserved() {
    let a = array_f32(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let m = array_bool(&[true, true, false], &[3]).unwrap();
    let r = a.mask(&m).unwrap();
    assert_eq!(r.dtype(), Dtype::Float32);
}

#[test]
fn t40_mask_bool_dtype_preserved() {
    let a = array_bool(&[true, false, true, false], &[4]).unwrap();
    let m = array_bool(&[true, true, false, true], &[4]).unwrap();
    let r = a.mask(&m).unwrap();
    assert_eq!(r.dtype(), Dtype::Bool);
    assert_eq!(r.shape(), vec![3]);
}

// ---- np.where — 41-50 --------------------------------------------------

#[test]
fn t41_where_basic_bool_cond() {
    let cond = array_bool(&[true, false, true], &[3]).unwrap();
    let x = array_i32(&[10, 20, 30], &[3]).unwrap();
    let y = array_i32(&[1, 2, 3], &[3]).unwrap();
    let r = np_where(&cond, &x, &y).unwrap();
    assert_eq!(data_f64(&r), vec![10.0, 2.0, 30.0]);
}

#[test]
fn t42_where_broadcasts_scalar_x() {
    let cond = array_bool(&[true, false, true, false], &[4]).unwrap();
    let x = array_i32(&[42], &[1]).unwrap();
    let y = array_i32(&[1, 2, 3, 4], &[4]).unwrap();
    let r = np_where(&cond, &x, &y).unwrap();
    assert_eq!(data_f64(&r), vec![42.0, 2.0, 42.0, 4.0]);
}

#[test]
fn t43_where_promotes_int_float_to_float64() {
    let cond = array_bool(&[true, false], &[2]).unwrap();
    let x = array_i32(&[1, 2], &[2]).unwrap();
    let y = array_f64(&[10.5, 20.5], &[2]).unwrap();
    let r = np_where(&cond, &x, &y).unwrap();
    assert_eq!(r.dtype(), Dtype::Float64);
    assert_eq!(data_f64(&r), vec![1.0, 20.5]);
}

#[test]
fn t44_where_via_method() {
    let cond = array_bool(&[true, false, true], &[3]).unwrap();
    let x = array_i32(&[10, 20, 30], &[3]).unwrap();
    let y = array_i32(&[1, 2, 3], &[3]).unwrap();
    let r = cond.where_(&x, &y).unwrap();
    assert_eq!(data_f64(&r), vec![10.0, 2.0, 30.0]);
}

#[test]
fn t45_where_2d_arrays() {
    let cond = array_bool(&[true, false, false, true], &[2, 2]).unwrap();
    let x = array_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let y = array_i32(&[10, 20, 30, 40], &[2, 2]).unwrap();
    let r = np_where(&cond, &x, &y).unwrap();
    assert_eq!(r.shape(), vec![2, 2]);
    assert_eq!(data_f64(&r), vec![1.0, 20.0, 30.0, 4.0]);
}

#[test]
fn t46_where_float_dtypes() {
    let cond = array_bool(&[true, false, true], &[3]).unwrap();
    let x = array_f64(&[1.5, 2.5, 3.5], &[3]).unwrap();
    let y = array_f64(&[10.5, 20.5, 30.5], &[3]).unwrap();
    let r = np_where(&cond, &x, &y).unwrap();
    assert_eq!(data_f64(&r), vec![1.5, 20.5, 3.5]);
}

#[test]
fn t47_where_int32_int64_promotes_int64() {
    let cond = array_bool(&[true, false], &[2]).unwrap();
    let x = array_i32(&[1, 2], &[2]).unwrap();
    let y = array_i64(&[10, 20], &[2]).unwrap();
    let r = np_where(&cond, &x, &y).unwrap();
    assert_eq!(r.dtype(), Dtype::Int64);
}

#[test]
fn t48_where_bool_only() {
    let cond = array_bool(&[true, false, true], &[3]).unwrap();
    let x = array_bool(&[true, true, false], &[3]).unwrap();
    let y = array_bool(&[false, false, false], &[3]).unwrap();
    let r = np_where(&cond, &x, &y).unwrap();
    assert_eq!(r.dtype(), Dtype::Bool);
}

#[test]
fn t49_where_with_int_cond_treated_as_truthy() {
    // numpy: non-bool cond is silently cast — non-zero → true.
    let cond = array_i32(&[1, 0, 5], &[3]).unwrap();
    let x = array_i32(&[10, 20, 30], &[3]).unwrap();
    let y = array_i32(&[1, 2, 3], &[3]).unwrap();
    let r = np_where(&cond, &x, &y).unwrap();
    assert_eq!(data_f64(&r), vec![10.0, 2.0, 30.0]);
}

#[test]
fn t50_where_three_way_broadcast() {
    // cond is 1-D, x/y are 2-D — broadcast along axis-0.
    let cond = array_bool(&[true, false], &[2]).unwrap();
    let x = array_i32(&[1, 2, 3, 4, 5, 6], &[3, 2]).unwrap();
    let y = array_i32(&[10, 20, 30, 40, 50, 60], &[3, 2]).unwrap();
    let r = np_where(&cond, &x, &y).unwrap();
    assert_eq!(r.shape(), vec![3, 2]);
    assert_eq!(data_f64(&r), vec![1.0, 20.0, 3.0, 40.0, 5.0, 60.0]);
}

// ---- Extras — combined / multi-axis — 51-55 ----------------------------

#[test]
fn t51_chained_slice_then_take() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], &[10]).unwrap();
    let mid = a.slice(SliceSpec::range(2, 8)).unwrap().to_owned();
    let r = mid.take(&[0, 2, 4]).unwrap();
    assert_eq!(data_f64(&r), vec![3.0, 5.0, 7.0]);
}

#[test]
fn t52_index_get_empty_indices_returns_clone() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let r = a.index_get(&[]).unwrap();
    assert_eq!(data_f64(&r), data_f64(&a));
}

#[test]
fn t53_index_get_chain_slice_then_single() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6], &[6]).unwrap();
    let r = a
        .index_get(&[Index::Slice(SliceSpec::range(2, 5)), Index::Single(0)])
        .unwrap();
    assert_eq!(data_f64(&r), vec![3.0]);
}

#[test]
fn t54_new_axis_inserts_length_1() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let r = a.index_get(&[Index::NewAxis]).unwrap();
    assert_eq!(r.shape(), vec![1, 3]);
}

#[test]
fn t55_view_then_to_owned_round_trip() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
    let v = a.slice(SliceSpec::full()).unwrap();
    let owned = v.to_owned();
    assert_eq!(data_f64(&owned), vec![1.0, 2.0, 3.0, 4.0]);
}
