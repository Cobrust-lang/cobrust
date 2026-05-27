//! M7.3 ill-typed reduction programs (per ADR-0016).
//!
//! ≥ 50 ill-typed reduction programs covering documented failure
//! paths: empty-array min/max/argmin/argmax (ReductionEmptyArray),
//! axis out of bounds (IndexError), and ddof-related NaN cases.

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
#![allow(clippy::excessive_precision)]
#![allow(clippy::similar_names)]

use coil::{Array, NumpyErrorKind, array_bool, array_f32, array_f64, array_i32, array_i64};

#[test]
fn ill_typed_01_min_empty_int32() {
    let a = array_i32(&[], &[0]).unwrap();
    let err = a.min(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_02_min_empty_int64() {
    let a = array_i64(&[], &[0]).unwrap();
    let err = a.min(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_03_min_empty_f32() {
    let a = array_f32(&[], &[0]).unwrap();
    let err = a.min(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_04_min_empty_f64() {
    let a = array_f64(&[], &[0]).unwrap();
    let err = a.min(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_05_min_empty_bool() {
    let a = array_bool(&[], &[0]).unwrap();
    let err = a.min(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_06_max_empty_int32() {
    let a = array_i32(&[], &[0]).unwrap();
    let err = a.max(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_07_max_empty_int64() {
    let a = array_i64(&[], &[0]).unwrap();
    let err = a.max(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_08_max_empty_f32() {
    let a = array_f32(&[], &[0]).unwrap();
    let err = a.max(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_09_max_empty_f64() {
    let a = array_f64(&[], &[0]).unwrap();
    let err = a.max(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_10_argmin_empty() {
    let a = array_i32(&[], &[0]).unwrap();
    let err = a.argmin(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_11_argmin_empty_f64() {
    let a = array_f64(&[], &[0]).unwrap();
    let err = a.argmin(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_12_argmax_empty() {
    let a = array_i64(&[], &[0]).unwrap();
    let err = a.argmax(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_13_argmax_empty_f64() {
    let a = array_f64(&[], &[0]).unwrap();
    let err = a.argmax(None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_14_sum_axis_5_on_1d() {
    let a = array_i64(&[1, 2, 3], &[3]).unwrap();
    let err = a.sum(Some(5)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_15_sum_axis_neg_5_on_1d() {
    let a = array_i64(&[1, 2, 3], &[3]).unwrap();
    let err = a.sum(Some(-5)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_16_min_axis_oob() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = a.min(Some(10)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_17_max_axis_oob() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = a.max(Some(10)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_18_mean_axis_oob() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = a.mean(Some(10)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_19_var_axis_oob() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = a.var(Some(2), 0).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_20_std_axis_oob() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = a.std(Some(7), 0).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_21_argmin_axis_oob() {
    let a = array_i64(&[1, 2, 3], &[3]).unwrap();
    let err = a.argmin(Some(5)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_22_argmax_axis_oob() {
    let a = array_i64(&[1, 2, 3], &[3]).unwrap();
    let err = a.argmax(Some(-7)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_23_min_axis_zero_lane() {
    // 0x3 — axis-0 is empty
    let a = array_f64(&[], &[0, 3]).unwrap();
    let err = a.min(Some(0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_24_max_axis_zero_lane() {
    let a = array_f64(&[], &[3, 0]).unwrap();
    let err = a.max(Some(1)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_25_argmin_axis_zero_lane() {
    let a = array_i64(&[], &[0, 3]).unwrap();
    let err = a.argmin(Some(0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_26_argmax_axis_zero_lane() {
    let a = array_i64(&[], &[3, 0]).unwrap();
    let err = a.argmax(Some(1)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_27_var_ddof_eq_n_yields_nan() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.var(None, 3).unwrap();
    let Array::Float64(arr) = r else {
        panic!("expected Float64");
    };
    assert!(arr.iter().next().unwrap().is_nan());
}

#[test]
fn ill_typed_28_var_ddof_gt_n_yields_nan() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.var(None, 100).unwrap();
    let Array::Float64(arr) = r else {
        panic!("expected Float64");
    };
    assert!(arr.iter().next().unwrap().is_nan());
}

#[test]
fn ill_typed_29_std_ddof_gt_n_yields_nan() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.std(None, 7).unwrap();
    let Array::Float64(arr) = r else {
        panic!("expected Float64");
    };
    assert!(arr.iter().next().unwrap().is_nan());
}

#[test]
fn ill_typed_30_var_empty_ddof0_nan() {
    let a = array_f64(&[], &[0]).unwrap();
    let r = a.var(None, 0).unwrap();
    let Array::Float64(arr) = r else {
        panic!("expected Float64");
    };
    assert!(arr.iter().next().unwrap().is_nan());
}

#[test]
fn ill_typed_31_std_empty_nan() {
    let a = array_f64(&[], &[0]).unwrap();
    let r = a.std(None, 0).unwrap();
    let Array::Float64(arr) = r else {
        panic!("expected Float64");
    };
    assert!(arr.iter().next().unwrap().is_nan());
}

#[test]
fn ill_typed_32_mean_empty_nan() {
    let a = array_f64(&[], &[0]).unwrap();
    let r = a.mean(None).unwrap();
    let Array::Float64(arr) = r else {
        panic!("expected Float64");
    };
    assert!(arr.iter().next().unwrap().is_nan());
}

#[test]
fn ill_typed_33_mean_empty_int_nan() {
    let a = array_i64(&[], &[0]).unwrap();
    let r = a.mean(None).unwrap();
    let Array::Float64(arr) = r else {
        panic!("expected Float64 promoted");
    };
    assert!(arr.iter().next().unwrap().is_nan());
}

#[test]
fn ill_typed_34_argmin_axis_oob_3d() {
    let a = array_i64(&[1, 2, 3, 4, 5, 6, 7, 8], &[2, 2, 2]).unwrap();
    let err = a.argmin(Some(5)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_35_argmax_axis_oob_3d() {
    let a = array_i64(&[1, 2, 3, 4, 5, 6, 7, 8], &[2, 2, 2]).unwrap();
    let err = a.argmax(Some(-7)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_36_sum_zero_dim_array_axis() {
    // 0-dim array — axis=0 is OOB
    let a = coil::array(&[42.0], &[], coil::Dtype::Float64).unwrap();
    let err = a.sum(Some(0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_37_min_zero_dim_array_axis() {
    let a = coil::array(&[1.0], &[], coil::Dtype::Float64).unwrap();
    let err = a.min(Some(0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_38_argmax_zero_dim_array_axis() {
    let a = coil::array(&[1.0], &[], coil::Dtype::Float64).unwrap();
    let err = a.argmax(Some(0)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_39_var_2d_empty_lane() {
    let a = array_f64(&[], &[0, 3]).unwrap();
    let r = a.var(Some(0), 0).unwrap();
    let Array::Float64(arr) = r else {
        panic!("Float64");
    };
    assert!(arr.iter().all(|v| v.is_nan()));
}

#[test]
fn ill_typed_40_min_negative_axis_too_negative() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let err = a.min(Some(-3)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_41_max_negative_axis_too_negative() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let err = a.max(Some(-3)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_42_sum_negative_axis_too_negative() {
    let a = array_i64(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let err = a.sum(Some(-5)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_43_var_axis_oob_2d() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let err = a.var(Some(2), 0).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_44_std_axis_oob_2d() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let err = a.std(Some(2), 0).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_45_mean_axis_oob_3d() {
    let a = array_f64(&[1.0; 8], &[2, 2, 2]).unwrap();
    let err = a.mean(Some(3)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_46_min_empty_axis_neg_oob() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = a.min(Some(-2)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_47_argmin_axis_neg_oob() {
    let a = array_i64(&[1, 2, 3], &[3]).unwrap();
    let err = a.argmin(Some(-2)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_48_argmax_axis_neg_oob() {
    let a = array_i64(&[1, 2, 3], &[3]).unwrap();
    let err = a.argmax(Some(-2)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}

#[test]
fn ill_typed_49_min_2d_empty_lane_axis1() {
    let a = array_f64(&[], &[3, 0]).unwrap();
    let err = a.min(Some(1)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_50_argmin_2d_empty_lane_axis1() {
    let a = array_i64(&[], &[3, 0]).unwrap();
    let err = a.argmin(Some(1)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ReductionEmptyArray);
}

#[test]
fn ill_typed_51_max_axis_negative_oob_3d() {
    let a = array_f64(&[1.0; 8], &[2, 2, 2]).unwrap();
    let err = a.max(Some(-4)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IndexError);
}
