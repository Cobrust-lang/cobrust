//! M7.2 ill-typed indexing program suite — at least 50 programs rejected.
//!
//! Per ADR-0015 §"M7.2 scope window": ≥ 50 programs that exercise the
//! error paths — out-of-bounds single/int-array indices,
//! shape-mismatched bool masks, non-bool dtype on `mask`, zero-step
//! slices, indexing a 0-d array, etc. Each test asserts that the
//! cobrust-coil call returns a typed `NumpyError` with the right
//! `kind` per ADR-0015 §4.

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

use coil::{
    Dtype, Index, NumpyErrorKind, SliceSpec, array_bool, array_f32, array_f64, array_i32,
    array_i64, np_where, ones, zeros,
};

// ---- 0-d / cannot index — 1-5 -------------------------------------------

#[test]
fn t01_slice_zero_d_array_errors() {
    let a = array_i32(&[42], &[]).unwrap();
    let err = a.slice(SliceSpec::full()).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn t02_single_zero_d_array_errors() {
    let a = array_i32(&[42], &[]).unwrap();
    let err = a.index_single(0).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn t03_take_zero_d_array_errors() {
    let a = array_i32(&[42], &[]).unwrap();
    let err = a.take(&[0]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn t04_slice_mut_zero_d_errors() {
    let mut a = array_i32(&[42], &[]).unwrap();
    let err = a.slice_mut(SliceSpec::full()).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn t05_index_get_too_many_indices_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    // After Single(0) we have a 0-d array; another Single is invalid.
    let err = a
        .index_get(&[Index::Single(0), Index::Single(0)])
        .unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

// ---- Out-of-bounds single index — 6-15 ----------------------------------

#[test]
fn t06_single_above_length_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.index_single(3).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t07_single_far_above_length_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.index_single(100).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t08_single_below_negative_length_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.index_single(-4).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t09_single_far_below_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.index_single(-100).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t10_single_index_get_oob_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.index_get(&[Index::Single(5)]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t11_single_index_dtype_int64() {
    let a = array_i64(&[1, 2, 3], &[3]).unwrap();
    let err = a.index_single(10).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t12_single_index_dtype_float64() {
    let a = array_f64(&[1.0, 2.0], &[2]).unwrap();
    let err = a.index_single(2).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t13_single_index_dtype_float32() {
    let a = array_f32(&[1.0, 2.0], &[2]).unwrap();
    let err = a.index_single(-3).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t14_single_index_dtype_bool() {
    let a = array_bool(&[true, false], &[2]).unwrap();
    let err = a.index_single(2).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t15_empty_array_single_index_errors() {
    let a = zeros(&[0], Dtype::Int32).unwrap();
    let err = a.index_single(0).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

// ---- Out-of-bounds in take (integer-array indexing) — 16-25 -------------

#[test]
fn t16_take_index_above_length_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.take(&[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t17_take_index_far_above_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.take(&[100]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t18_take_index_below_negative_length_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.take(&[-4]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t19_take_one_oob_in_list_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.take(&[0, 1, 5]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t20_take_first_oob_in_list_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.take(&[10, 1, 2]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t21_take_via_index_get_oob_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.index_get(&[Index::IntArray(vec![5])]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t22_take_oob_int64() {
    let a = array_i64(&[1, 2, 3], &[3]).unwrap();
    let err = a.take(&[100]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t23_take_oob_float64() {
    let a = array_f64(&[1.0, 2.0], &[2]).unwrap();
    let err = a.take(&[5]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t24_take_oob_bool() {
    let a = array_bool(&[true, false], &[2]).unwrap();
    let err = a.take(&[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t25_take_oob_negative_far() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a.take(&[-100]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

// ---- Bool-mask shape mismatch — 26-35 ----------------------------------

#[test]
fn t26_mask_too_few_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let m = array_bool(&[true, false], &[2]).unwrap();
    let err = a.mask(&m).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BoolMaskShapeMismatch);
}

#[test]
fn t27_mask_too_many_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let m = array_bool(&[true, false, true, false], &[4]).unwrap();
    let err = a.mask(&m).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BoolMaskShapeMismatch);
}

#[test]
fn t28_mask_2d_shape_differs_errors() {
    let a = array_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let m = array_bool(&[true, false, true, false], &[4]).unwrap();
    let err = a.mask(&m).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BoolMaskShapeMismatch);
}

#[test]
fn t29_mask_different_dim_errors() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6], &[2, 3]).unwrap();
    let m = array_bool(&[true, false, true], &[3]).unwrap();
    let err = a.mask(&m).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BoolMaskShapeMismatch);
}

#[test]
fn t30_mask_same_size_different_shape_errors() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6], &[2, 3]).unwrap();
    let m = array_bool(&[true, false, true, false, true, false], &[3, 2]).unwrap();
    let err = a.mask(&m).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BoolMaskShapeMismatch);
}

#[test]
fn t31_mask_via_index_get_shape_mismatch_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let m = array_bool(&[true, false], &[2]).unwrap();
    let err = a.index_get(&[Index::BoolMask(m)]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BoolMaskShapeMismatch);
}

#[test]
fn t32_mask_int_dtype_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let m = array_i32(&[1, 0, 1], &[3]).unwrap();
    let err = a.mask(&m).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexDtypeNotInteger);
}

#[test]
fn t33_mask_float_dtype_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let m = array_f64(&[1.0, 0.0, 1.0], &[3]).unwrap();
    let err = a.mask(&m).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexDtypeNotInteger);
}

#[test]
fn t34_mask_int64_dtype_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let m = array_i64(&[1, 0, 1], &[3]).unwrap();
    let err = a.mask(&m).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexDtypeNotInteger);
}

#[test]
fn t35_mask_float32_dtype_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let m = array_f32(&[1.0, 0.0, 1.0], &[3]).unwrap();
    let err = a.mask(&m).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexDtypeNotInteger);
}

// ---- Slice errors — 36-45 ---------------------------------------------

#[test]
fn t36_slice_zero_step_errors() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let err = a.slice(SliceSpec::stepped(0, 5, 0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn t37_slice_zero_step_only_errors() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let err = a.slice(SliceSpec::step_only(0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn t38_slice_mut_zero_step_errors() {
    let mut a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let err = a.slice_mut(SliceSpec::step_only(0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn t39_slice_int64_zero_step_errors() {
    let a = array_i64(&[1, 2, 3], &[3]).unwrap();
    let err = a.slice(SliceSpec::step_only(0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn t40_slice_float64_zero_step_errors() {
    let a = array_f64(&[1.0], &[1]).unwrap();
    let err = a.slice(SliceSpec::step_only(0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn t41_slice_via_index_get_zero_step_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a
        .index_get(&[Index::Slice(SliceSpec::step_only(0))])
        .unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn t42_slice_zero_step_float32_errors() {
    let a = array_f32(&[1.0], &[1]).unwrap();
    let err = a.slice(SliceSpec::step_only(0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn t43_slice_zero_step_bool_errors() {
    let a = array_bool(&[true, false], &[2]).unwrap();
    let err = a.slice(SliceSpec::step_only(0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn t44_slice_zero_step_with_full_bounds_errors() {
    let a = array_i32(&[1, 2], &[2]).unwrap();
    let err = a.slice(SliceSpec::stepped(0, 2, 0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn t45_slice_via_index_get_zero_step_int_array_chain_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a
        .index_get(&[
            Index::Slice(SliceSpec::full()),
            Index::Slice(SliceSpec::step_only(0)),
        ])
        .unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

// ---- Where / broadcasting errors — 46-55 -------------------------------

#[test]
fn t46_where_incompatible_shapes_errors() {
    let cond = array_bool(&[true, false, true], &[3]).unwrap();
    let x = array_i32(&[1, 2], &[2]).unwrap();
    let y = array_i32(&[10, 20, 30], &[3]).unwrap();
    let err = np_where(&cond, &x, &y).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t47_where_x_y_shape_mismatch_errors() {
    let cond = array_bool(&[true, false], &[2]).unwrap();
    let x = array_i32(&[1, 2], &[2]).unwrap();
    let y = array_i32(&[10, 20, 30], &[3]).unwrap();
    let err = np_where(&cond, &x, &y).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t48_where_2d_incompat_errors() {
    let cond = array_bool(&[true, false, true, false], &[2, 2]).unwrap();
    let x = array_i32(&[1, 2, 3, 4, 5, 6], &[2, 3]).unwrap();
    let y = array_i32(&[10, 20, 30, 40], &[2, 2]).unwrap();
    let err = np_where(&cond, &x, &y).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t49_take_oob_2d_errors() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6], &[3, 2]).unwrap();
    let err = a.take(&[5]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t50_mask_empty_shape_mismatch_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let m = array_bool(&[], &[0]).unwrap();
    let err = a.mask(&m).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BoolMaskShapeMismatch);
}

#[test]
fn t51_single_index_zero_length_array() {
    let a = ones(&[0], Dtype::Int32).unwrap();
    let err = a.index_single(-1).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t52_take_against_zero_length_array_with_index() {
    let a = ones(&[0], Dtype::Int32).unwrap();
    let err = a.take(&[0]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::OutOfBoundsIndex);
}

#[test]
fn t53_mask_int32_dtype_dispatch_errors_first() {
    // When both shape and dtype are wrong, the dtype check fires first.
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let m = array_i32(&[1, 0, 1, 1], &[4]).unwrap();
    let err = a.mask(&m).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexDtypeNotInteger);
}

#[test]
fn t54_too_many_indices_after_chain_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let err = a
        .index_get(&[
            Index::Slice(SliceSpec::range(0, 1)),
            Index::Single(0),
            Index::Single(0),
        ])
        .unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn t55_slice_zero_step_2d_errors() {
    let a = array_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let err = a.slice(SliceSpec::stepped(0, 2, 0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}
