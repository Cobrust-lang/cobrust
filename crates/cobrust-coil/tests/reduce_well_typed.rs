//! M7.3 well-typed reduction programs (per ADR-0016).
//!
//! ≥ 50 well-typed reduction programs covering the M7.3 surface
//! (sum/prod/mean/std/var/min/max/argmin/argmax) across the closed
//! dtype set + axis modes (None / Some(k) / Some(-1)). Every program
//! either succeeds or returns a documented `Err(...)` per ADR-0016.

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
#![allow(clippy::needless_range_loop)]

use coil::{Array, array_bool, array_f32, array_f64, array_i32, array_i64};

fn as_f64(a: &Array) -> f64 {
    match a {
        Array::Float64(arr) => *arr.iter().next().unwrap(),
        Array::Float32(arr) => f64::from(*arr.iter().next().unwrap()),
        Array::Int64(arr) => *arr.iter().next().unwrap() as f64,
        Array::Int32(arr) => f64::from(*arr.iter().next().unwrap()),
        Array::Bool(arr) => f64::from(u8::from(*arr.iter().next().unwrap())),
    }
}

#[test]
fn well_typed_01_sum_int32_all() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let r = a.sum(None).unwrap();
    assert_eq!(as_f64(&r), 6.0);
}

#[test]
fn well_typed_02_sum_int64_all() {
    let a = array_i64(&[10, 20, 30], &[3]).unwrap();
    let r = a.sum(None).unwrap();
    assert_eq!(as_f64(&r), 60.0);
}

#[test]
fn well_typed_03_sum_f32_all() {
    let a = array_f32(&[1.5, 2.5, 3.0], &[3]).unwrap();
    let r = a.sum(None).unwrap();
    assert!((as_f64(&r) - 7.0).abs() < 1e-6);
}

#[test]
fn well_typed_04_sum_f64_all() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.sum(None).unwrap();
    assert_eq!(as_f64(&r), 6.0);
}

#[test]
fn well_typed_05_sum_bool_all() {
    let a = array_bool(&[true, false, true, true], &[4]).unwrap();
    let r = a.sum(None).unwrap();
    assert_eq!(as_f64(&r), 3.0);
}

#[test]
fn well_typed_06_sum_axis_0() {
    let a = array_i64(&[1, 2, 3, 4, 5, 6], &[2, 3]).unwrap();
    let r = a.sum(Some(0)).unwrap();
    assert_eq!(r.shape(), vec![3]);
}

#[test]
fn well_typed_07_sum_axis_1() {
    let a = array_i64(&[1, 2, 3, 4, 5, 6], &[2, 3]).unwrap();
    let r = a.sum(Some(1)).unwrap();
    assert_eq!(r.shape(), vec![2]);
}

#[test]
fn well_typed_08_sum_axis_negative() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
    let r = a.sum(Some(-1)).unwrap();
    assert_eq!(r.shape(), vec![2]);
}

#[test]
fn well_typed_09_prod_int32() {
    let a = array_i32(&[2, 3, 4], &[3]).unwrap();
    let r = a.prod(None).unwrap();
    assert_eq!(as_f64(&r), 24.0);
}

#[test]
fn well_typed_10_prod_f64() {
    let a = array_f64(&[1.5, 2.0, 4.0], &[3]).unwrap();
    let r = a.prod(None).unwrap();
    assert_eq!(as_f64(&r), 12.0);
}

#[test]
fn well_typed_11_prod_axis() {
    let a = array_i64(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let r = a.prod(Some(0)).unwrap();
    assert_eq!(r.shape(), vec![2]);
}

#[test]
fn well_typed_12_prod_empty_is_one() {
    let a = array_i64(&[], &[0]).unwrap();
    let r = a.prod(None).unwrap();
    assert_eq!(as_f64(&r), 1.0);
}

#[test]
fn well_typed_13_mean_int_promotes_f64() {
    let a = array_i64(&[1, 2, 3, 4], &[4]).unwrap();
    let r = a.mean(None).unwrap();
    matches!(r, Array::Float64(_));
    assert_eq!(as_f64(&r), 2.5);
}

#[test]
fn well_typed_14_mean_f32_preserves_f32() {
    let a = array_f32(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.mean(None).unwrap();
    assert!(matches!(r, Array::Float32(_)));
}

#[test]
fn well_typed_15_mean_f64() {
    let a = array_f64(&[10.0, 20.0, 30.0], &[3]).unwrap();
    let r = a.mean(None).unwrap();
    assert_eq!(as_f64(&r), 20.0);
}

#[test]
fn well_typed_16_mean_axis() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let r = a.mean(Some(0)).unwrap();
    assert_eq!(r.shape(), vec![2]);
}

#[test]
fn well_typed_17_var_ddof0() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.var(None, 0).unwrap();
    // mean=2, sq=[1,0,1], var = 2/3
    let v = as_f64(&r);
    assert!((v - 2.0_f64 / 3.0).abs() < 1e-12);
}

#[test]
fn well_typed_18_var_ddof1_bessel() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.var(None, 1).unwrap();
    // mean=2, sq=[1,0,1], var(ddof=1) = 2/2 = 1
    assert!((as_f64(&r) - 1.0).abs() < 1e-12);
}

#[test]
fn well_typed_19_std_ddof0() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.std(None, 0).unwrap();
    let v = as_f64(&r);
    assert!((v - (2.0_f64 / 3.0).sqrt()).abs() < 1e-12);
}

#[test]
fn well_typed_20_min_int32() {
    let a = array_i32(&[3, 1, 2, 4], &[4]).unwrap();
    let r = a.min(None).unwrap();
    assert_eq!(as_f64(&r), 1.0);
}

#[test]
fn well_typed_21_min_int64() {
    let a = array_i64(&[100, -5, 50], &[3]).unwrap();
    let r = a.min(None).unwrap();
    assert_eq!(as_f64(&r), -5.0);
}

#[test]
fn well_typed_22_min_f32() {
    let a = array_f32(&[3.0, 1.0, 2.0], &[3]).unwrap();
    let r = a.min(None).unwrap();
    assert_eq!(as_f64(&r), 1.0);
}

#[test]
fn well_typed_23_min_f64() {
    let a = array_f64(&[3.0, 1.0, 2.0], &[3]).unwrap();
    let r = a.min(None).unwrap();
    assert_eq!(as_f64(&r), 1.0);
}

#[test]
fn well_typed_24_min_bool() {
    let a = array_bool(&[true, false, true], &[3]).unwrap();
    let r = a.min(None).unwrap();
    let Array::Bool(arr) = r else {
        panic!("expected Bool");
    };
    assert!(!arr.iter().next().copied().unwrap());
}

#[test]
fn well_typed_25_max_int32() {
    let a = array_i32(&[3, 1, 2, 4], &[4]).unwrap();
    let r = a.max(None).unwrap();
    assert_eq!(as_f64(&r), 4.0);
}

#[test]
fn well_typed_26_max_int64() {
    let a = array_i64(&[100, -5, 50], &[3]).unwrap();
    let r = a.max(None).unwrap();
    assert_eq!(as_f64(&r), 100.0);
}

#[test]
fn well_typed_27_max_f64() {
    let a = array_f64(&[1.5, 2.5, 0.5], &[3]).unwrap();
    let r = a.max(None).unwrap();
    assert_eq!(as_f64(&r), 2.5);
}

#[test]
fn well_typed_28_max_bool_any_true() {
    let a = array_bool(&[false, false, true], &[3]).unwrap();
    let r = a.max(None).unwrap();
    let Array::Bool(arr) = r else {
        panic!("expected Bool");
    };
    assert!(arr.iter().next().copied().unwrap());
}

#[test]
fn well_typed_29_argmin_first_occurrence() {
    let a = array_i64(&[5, 1, 3, 1, 2], &[5]).unwrap();
    let r = a.argmin(None).unwrap();
    assert_eq!(as_f64(&r), 1.0);
}

#[test]
fn well_typed_30_argmin_int32() {
    let a = array_i32(&[10, 20, 30], &[3]).unwrap();
    let r = a.argmin(None).unwrap();
    assert_eq!(as_f64(&r), 0.0);
}

#[test]
fn well_typed_31_argmin_f64() {
    let a = array_f64(&[3.0, 1.0, 4.0, 1.5], &[4]).unwrap();
    let r = a.argmin(None).unwrap();
    assert_eq!(as_f64(&r), 1.0);
}

#[test]
fn well_typed_32_argmax_first_occurrence() {
    let a = array_i64(&[1, 5, 3, 5, 2], &[5]).unwrap();
    let r = a.argmax(None).unwrap();
    assert_eq!(as_f64(&r), 1.0);
}

#[test]
fn well_typed_33_argmax_f64() {
    let a = array_f64(&[1.0, 2.0, 5.0, 3.0], &[4]).unwrap();
    let r = a.argmax(None).unwrap();
    assert_eq!(as_f64(&r), 2.0);
}

#[test]
fn well_typed_34_argmax_axis() {
    let a = array_i64(&[1, 4, 2, 3, 5, 0], &[2, 3]).unwrap();
    let r = a.argmax(Some(1)).unwrap();
    assert_eq!(r.shape(), vec![2]);
}

#[test]
fn well_typed_35_min_axis_2d() {
    let a = array_f64(&[1.0, 5.0, 2.0, 8.0, 3.0, 4.0], &[2, 3]).unwrap();
    let r = a.min(Some(1)).unwrap();
    assert_eq!(r.shape(), vec![2]);
}

#[test]
fn well_typed_36_max_axis_2d() {
    let a = array_f64(&[1.0, 5.0, 2.0, 8.0, 3.0, 4.0], &[2, 3]).unwrap();
    let r = a.max(Some(0)).unwrap();
    assert_eq!(r.shape(), vec![3]);
}

#[test]
fn well_typed_37_sum_3d_axis_0() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6, 7, 8], &[2, 2, 2]).unwrap();
    let r = a.sum(Some(0)).unwrap();
    assert_eq!(r.shape(), vec![2, 2]);
}

#[test]
fn well_typed_38_sum_3d_axis_2() {
    let a = array_i32(&[1, 2, 3, 4, 5, 6, 7, 8], &[2, 2, 2]).unwrap();
    let r = a.sum(Some(2)).unwrap();
    assert_eq!(r.shape(), vec![2, 2]);
}

#[test]
fn well_typed_39_mean_axis_3d() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 2, 2]).unwrap();
    let r = a.mean(Some(0)).unwrap();
    assert_eq!(r.shape(), vec![2, 2]);
}

#[test]
fn well_typed_40_var_axis() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let r = a.var(Some(0), 0).unwrap();
    assert_eq!(r.shape(), vec![2]);
}

#[test]
fn well_typed_41_std_axis() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let r = a.std(Some(1), 0).unwrap();
    assert_eq!(r.shape(), vec![2]);
}

#[test]
fn well_typed_42_argmin_axis() {
    let a = array_i64(&[1, 4, 2, 3, 5, 0], &[2, 3]).unwrap();
    let r = a.argmin(Some(1)).unwrap();
    assert_eq!(r.shape(), vec![2]);
}

#[test]
fn well_typed_43_sum_negative_axis() {
    let a = array_i64(&[1, 2, 3, 4, 5, 6], &[2, 3]).unwrap();
    let r = a.sum(Some(-1)).unwrap();
    let r_pos = a.sum(Some(1)).unwrap();
    assert_eq!(r.to_json(), r_pos.to_json());
}

#[test]
fn well_typed_44_mean_negative_axis() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
    let r = a.mean(Some(-2)).unwrap();
    let r_pos = a.mean(Some(0)).unwrap();
    assert_eq!(r.to_json(), r_pos.to_json());
}

#[test]
fn well_typed_45_sum_single_element() {
    let a = array_i64(&[42], &[1]).unwrap();
    let r = a.sum(None).unwrap();
    assert_eq!(as_f64(&r), 42.0);
}

#[test]
fn well_typed_46_min_single_element() {
    let a = array_f64(&[2.5], &[1]).unwrap();
    let r = a.min(None).unwrap();
    assert_eq!(as_f64(&r), 2.5);
}

#[test]
fn well_typed_47_argmin_single_element() {
    let a = array_i64(&[42], &[1]).unwrap();
    let r = a.argmin(None).unwrap();
    assert_eq!(as_f64(&r), 0.0);
}

#[test]
fn well_typed_48_var_two_elements() {
    let a = array_f64(&[1.0, 3.0], &[2]).unwrap();
    let r = a.var(None, 0).unwrap();
    assert_eq!(as_f64(&r), 1.0); // mean=2, sq=[1,1], var = 2/2 = 1
}

#[test]
fn well_typed_49_sum_pairwise_large() {
    let v: Vec<f64> = (0..1000).map(|_| 1e-6).collect();
    let a = array_f64(&v, &[1000]).unwrap();
    let r = a.sum(None).unwrap();
    let s = as_f64(&r);
    assert!((s - 1e-3).abs() < 1e-12);
}

#[test]
fn well_typed_50_prod_axis_3d() {
    let a = array_i64(&[1, 2, 3, 4, 5, 6, 7, 8], &[2, 2, 2]).unwrap();
    let r = a.prod(Some(0)).unwrap();
    assert_eq!(r.shape(), vec![2, 2]);
}

#[test]
fn well_typed_51_max_3d_negative_axis() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 2, 2]).unwrap();
    let r = a.max(Some(-1)).unwrap();
    assert_eq!(r.shape(), vec![2, 2]);
}

#[test]
fn well_typed_52_argmax_3d_axis_0() {
    let a = array_i64(&[1, 2, 3, 4, 5, 6, 7, 8], &[2, 2, 2]).unwrap();
    let r = a.argmax(Some(0)).unwrap();
    assert_eq!(r.shape(), vec![2, 2]);
}

#[test]
fn well_typed_53_mean_2d_axis_1() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
    let r = a.mean(Some(1)).unwrap();
    assert_eq!(r.shape(), vec![2]);
}

#[test]
fn well_typed_54_sum_keeps_dtype_int32() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let r = a.sum(None).unwrap();
    assert!(matches!(r, Array::Int32(_)));
}

#[test]
fn well_typed_55_sum_keeps_dtype_int64() {
    let a = array_i64(&[1, 2, 3], &[3]).unwrap();
    let r = a.sum(None).unwrap();
    assert!(matches!(r, Array::Int64(_)));
}
