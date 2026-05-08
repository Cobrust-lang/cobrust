//! M7.0 well-typed program suite — at least 50 programs accepted.
//!
//! Per ADR-0013 §"M7.0 scope window": "≥ 50 well-typed programs
//! accepted". Each "program" is a sequence of cobrust-numpy calls
//! that exercise the M7.0 surface and assert on observable
//! invariants (shape, ndim, size, dtype, repr text, to_json
//! payload). All 50+ entries must compile, run, and pass.
//!
//! These are the positive side of the curated suite; the negative
//! side lives in `tests/ill_typed.rs`.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::approx_constant)]

use cobrust_numpy::{Array, Dtype, NumpyError, arange, array, ones, zeros};

// ---- 1..10 — zeros happy paths -------------------------------------------

#[test]
fn t01_zeros_int32_1d() {
    let a = zeros(&[5], Dtype::Int32).unwrap();
    assert_eq!(a.shape(), vec![5]);
    assert_eq!(a.ndim(), 1);
    assert_eq!(a.size(), 5);
    assert_eq!(a.dtype(), Dtype::Int32);
}

#[test]
fn t02_zeros_int64_2d() {
    let a = zeros(&[3, 4], Dtype::Int64).unwrap();
    assert_eq!(a.shape(), vec![3, 4]);
    assert_eq!(a.ndim(), 2);
    assert_eq!(a.size(), 12);
}

#[test]
fn t03_zeros_float32_3d() {
    let a = zeros(&[2, 3, 4], Dtype::Float32).unwrap();
    assert_eq!(a.shape(), vec![2, 3, 4]);
    assert_eq!(a.ndim(), 3);
    assert_eq!(a.size(), 24);
}

#[test]
fn t04_zeros_float64_2d_square() {
    let a = zeros(&[5, 5], Dtype::Float64).unwrap();
    assert_eq!(a.shape(), vec![5, 5]);
}

#[test]
fn t05_zeros_bool_1d() {
    let a = zeros(&[7], Dtype::Bool).unwrap();
    assert_eq!(a.dtype(), Dtype::Bool);
    let json = a.to_json();
    assert_eq!(
        json["data"],
        serde_json::json!([false, false, false, false, false, false, false])
    );
}

#[test]
fn t06_zeros_empty_shape_is_scalar() {
    let a = zeros(&[], Dtype::Int64).unwrap();
    assert_eq!(a.ndim(), 0);
    assert_eq!(a.size(), 1);
}

#[test]
fn t07_zeros_zero_dim_is_empty() {
    let a = zeros(&[0], Dtype::Float64).unwrap();
    assert_eq!(a.size(), 0);
}

#[test]
fn t08_zeros_2d_with_zero_dim() {
    let a = zeros(&[3, 0], Dtype::Int32).unwrap();
    assert_eq!(a.size(), 0);
    assert_eq!(a.shape(), vec![3, 0]);
}

#[test]
fn t09_zeros_int32_data_all_zero() {
    let a = zeros(&[10], Dtype::Int32).unwrap();
    let json = a.to_json();
    assert_eq!(
        json["data"],
        serde_json::json!([
            0_i32, 0_i32, 0_i32, 0_i32, 0_i32, 0_i32, 0_i32, 0_i32, 0_i32, 0_i32
        ])
    );
}

#[test]
fn t10_zeros_float64_data_all_zero() {
    let a = zeros(&[4], Dtype::Float64).unwrap();
    let json = a.to_json();
    assert_eq!(
        json["data"],
        serde_json::json!([0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64])
    );
}

// ---- 11..20 — ones happy paths -------------------------------------------

#[test]
fn t11_ones_int32_1d() {
    let a = ones(&[3], Dtype::Int32).unwrap();
    assert_eq!(a.to_json()["data"], serde_json::json!([1_i32, 1, 1]));
}

#[test]
fn t12_ones_int64_2d() {
    let a = ones(&[2, 2], Dtype::Int64).unwrap();
    let json = a.to_json();
    assert_eq!(json["data"], serde_json::json!([1_i64, 1, 1, 1]));
}

#[test]
fn t13_ones_float32_1d() {
    let a = ones(&[5], Dtype::Float32).unwrap();
    let json = a.to_json();
    assert_eq!(json["data"], serde_json::json!([1.0, 1.0, 1.0, 1.0, 1.0]));
}

#[test]
fn t14_ones_float64_2d() {
    let a = ones(&[3, 2], Dtype::Float64).unwrap();
    let json = a.to_json();
    assert_eq!(json["shape"], serde_json::json!([3, 2]));
}

#[test]
fn t15_ones_bool() {
    let a = ones(&[4], Dtype::Bool).unwrap();
    let json = a.to_json();
    assert_eq!(json["data"], serde_json::json!([true, true, true, true]));
}

#[test]
fn t16_ones_3d() {
    let a = ones(&[2, 2, 2], Dtype::Int32).unwrap();
    assert_eq!(a.size(), 8);
}

#[test]
fn t17_ones_dtype_preserved() {
    let a = ones(&[3], Dtype::Float32).unwrap();
    assert_eq!(a.dtype(), Dtype::Float32);
}

#[test]
fn t18_ones_repr_contains_dtype() {
    let a = ones(&[3], Dtype::Int64).unwrap();
    let r = a.repr();
    assert!(r.contains("dtype=int64"));
    assert!(r.contains("array("));
}

#[test]
fn t19_ones_repr_contains_data() {
    let a = ones(&[3], Dtype::Int32).unwrap();
    let r = a.repr();
    assert!(r.contains('1'));
}

#[test]
fn t20_ones_zero_size() {
    let a = ones(&[0], Dtype::Float64).unwrap();
    assert_eq!(a.size(), 0);
}

// ---- 21..30 — array(values, shape, dtype) happy paths --------------------

#[test]
fn t21_array_int64_flat() {
    let a = array(&[1.0, 2.0, 3.0, 4.0], &[4], Dtype::Int64).unwrap();
    assert_eq!(a.to_json()["data"], serde_json::json!([1_i64, 2, 3, 4]));
}

#[test]
fn t22_array_int32_2d() {
    let a = array(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3], Dtype::Int32).unwrap();
    assert_eq!(a.to_json()["shape"], serde_json::json!([2, 3]));
}

#[test]
fn t23_array_float64_3d() {
    let v: Vec<f64> = (0..24).map(|x| x as f64).collect();
    let a = array(&v, &[2, 3, 4], Dtype::Float64).unwrap();
    assert_eq!(a.size(), 24);
    assert_eq!(a.ndim(), 3);
}

#[test]
fn t24_array_float32_2d() {
    let a = array(&[0.5, 1.5, 2.5, 3.5], &[2, 2], Dtype::Float32).unwrap();
    let json = a.to_json();
    assert_eq!(json["dtype"], "Float32");
}

#[test]
fn t25_array_bool_truthy() {
    let a = array(&[1.0, 0.0, 1.0, 0.0], &[4], Dtype::Bool).unwrap();
    let json = a.to_json();
    assert_eq!(json["data"], serde_json::json!([true, false, true, false]));
}

#[test]
fn t26_array_int_truncation_negative() {
    let a = array(&[-1.0, -2.0, -3.0], &[3], Dtype::Int64).unwrap();
    assert_eq!(a.to_json()["data"], serde_json::json!([-1_i64, -2, -3]));
}

#[test]
fn t27_array_empty_buffer_zero_dim() {
    let a = array(&[], &[0], Dtype::Float64).unwrap();
    assert_eq!(a.size(), 0);
}

#[test]
fn t28_array_2x0_is_empty() {
    let a = array(&[], &[2, 0], Dtype::Int32).unwrap();
    assert_eq!(a.size(), 0);
    assert_eq!(a.shape(), vec![2, 0]);
}

#[test]
fn t29_array_single_element() {
    let a = array(&[42.0], &[1], Dtype::Int64).unwrap();
    assert_eq!(a.to_json()["data"], serde_json::json!([42_i64]));
}

#[test]
fn t30_array_scalar_shape() {
    let a = array(&[3.14], &[], Dtype::Float64).unwrap();
    assert_eq!(a.size(), 1);
    assert_eq!(a.ndim(), 0);
}

// ---- 31..40 — arange happy paths -----------------------------------------

#[test]
fn t31_arange_int64_basic() {
    let a = arange(0.0, 5.0, 1.0, Dtype::Int64).unwrap();
    assert_eq!(a.to_json()["data"], serde_json::json!([0_i64, 1, 2, 3, 4]));
}

#[test]
fn t32_arange_int_step_2() {
    let a = arange(0.0, 10.0, 2.0, Dtype::Int64).unwrap();
    assert_eq!(a.to_json()["data"], serde_json::json!([0_i64, 2, 4, 6, 8]));
}

#[test]
fn t33_arange_int32_basic() {
    let a = arange(1.0, 4.0, 1.0, Dtype::Int32).unwrap();
    assert_eq!(a.to_json()["data"], serde_json::json!([1_i32, 2, 3]));
}

#[test]
fn t34_arange_float_basic() {
    let a = arange(0.0, 1.0, 0.25, Dtype::Float64).unwrap();
    assert_eq!(
        a.to_json()["data"],
        serde_json::json!([0.0, 0.25, 0.5, 0.75])
    );
}

#[test]
fn t35_arange_negative_step() {
    let a = arange(5.0, 0.0, -1.0, Dtype::Int64).unwrap();
    assert_eq!(a.to_json()["data"], serde_json::json!([5_i64, 4, 3, 2, 1]));
}

#[test]
fn t36_arange_empty_when_step_wrong_sign() {
    let a = arange(0.0, 5.0, -1.0, Dtype::Int64).unwrap();
    assert_eq!(a.size(), 0);
}

#[test]
fn t37_arange_empty_when_start_eq_stop() {
    let a = arange(3.0, 3.0, 1.0, Dtype::Int64).unwrap();
    assert_eq!(a.size(), 0);
}

#[test]
fn t38_arange_float32() {
    let a = arange(0.0, 4.0, 1.0, Dtype::Float32).unwrap();
    assert_eq!(a.dtype(), Dtype::Float32);
    assert_eq!(a.size(), 4);
}

#[test]
fn t39_arange_large_range() {
    let a = arange(0.0, 1000.0, 1.0, Dtype::Int32).unwrap();
    assert_eq!(a.size(), 1000);
}

#[test]
fn t40_arange_ndim_is_1() {
    let a = arange(0.0, 3.0, 1.0, Dtype::Int64).unwrap();
    assert_eq!(a.ndim(), 1);
}

// ---- 41..50 — Dtype + observers ------------------------------------------

#[test]
fn t41_dtype_parse_int32_long() {
    assert_eq!(Dtype::from_python_string("int32").unwrap(), Dtype::Int32);
}

#[test]
fn t42_dtype_parse_int32_alias() {
    assert_eq!(Dtype::from_python_string("i4").unwrap(), Dtype::Int32);
}

#[test]
fn t43_dtype_parse_int64() {
    assert_eq!(Dtype::from_python_string("int64").unwrap(), Dtype::Int64);
    assert_eq!(Dtype::from_python_string("i8").unwrap(), Dtype::Int64);
}

#[test]
fn t44_dtype_parse_float32() {
    assert_eq!(
        Dtype::from_python_string("float32").unwrap(),
        Dtype::Float32
    );
    assert_eq!(Dtype::from_python_string("f4").unwrap(), Dtype::Float32);
}

#[test]
fn t45_dtype_parse_float64() {
    assert_eq!(
        Dtype::from_python_string("float64").unwrap(),
        Dtype::Float64
    );
    assert_eq!(Dtype::from_python_string("f8").unwrap(), Dtype::Float64);
}

#[test]
fn t46_dtype_parse_bool() {
    assert_eq!(Dtype::from_python_string("bool").unwrap(), Dtype::Bool);
    assert_eq!(Dtype::from_python_string("?").unwrap(), Dtype::Bool);
}

#[test]
fn t47_dtype_to_python_string_roundtrip() {
    for s in ["int32", "int64", "float32", "float64", "bool"] {
        let dt = Dtype::from_python_string(s).unwrap();
        assert_eq!(dt.to_python_string(), s);
    }
}

#[test]
fn t48_dtype_item_size() {
    assert_eq!(Dtype::Int32.item_size(), 4);
    assert_eq!(Dtype::Int64.item_size(), 8);
    assert_eq!(Dtype::Float32.item_size(), 4);
    assert_eq!(Dtype::Float64.item_size(), 8);
    assert_eq!(Dtype::Bool.item_size(), 1);
}

#[test]
fn t49_array_to_json_shape_field() {
    let a = zeros(&[3, 4], Dtype::Int32).unwrap();
    let json = a.to_json();
    assert_eq!(json["shape"], serde_json::json!([3, 4]));
}

#[test]
fn t50_array_to_json_dtype_field_matches_variant_name() {
    for (dt, name) in [
        (Dtype::Int32, "Int32"),
        (Dtype::Int64, "Int64"),
        (Dtype::Float32, "Float32"),
        (Dtype::Float64, "Float64"),
        (Dtype::Bool, "Bool"),
    ] {
        let a = zeros(&[1], dt).unwrap();
        let json = a.to_json();
        assert_eq!(json["dtype"], name);
    }
}

// ---- 51..55 — extra coverage (well over 50, padding for safety) ---------

#[test]
fn t51_repr_int_array() {
    let a = arange(0.0, 3.0, 1.0, Dtype::Int64).unwrap();
    let r = a.repr();
    assert!(r.contains("array("));
    assert!(r.contains("dtype=int64"));
    assert!(r.contains("[0, 1, 2]"));
}

#[test]
fn t52_repr_2d_array() {
    let a = ones(&[2, 2], Dtype::Int32).unwrap();
    let r = a.repr();
    // M7.0 cobrust-flavored repr: nested list form.
    assert!(r.contains("[[1, 1], [1, 1]]"));
}

#[test]
fn t53_array_shape_size_helper() {
    assert_eq!(Array::shape_size(&[2, 3, 4]), 24);
    assert_eq!(Array::shape_size(&[]), 1);
    assert_eq!(Array::shape_size(&[0]), 0);
}

#[test]
fn t54_arange_count_helper() {
    assert_eq!(cobrust_numpy::arange_count(0.0, 5.0, 1.0), 5);
    assert_eq!(cobrust_numpy::arange_count(0.0, 5.0, 2.0), 3);
    assert_eq!(cobrust_numpy::arange_count(5.0, 0.0, 1.0), 0);
    assert_eq!(cobrust_numpy::arange_count(0.0, 0.0, 1.0), 0);
}

#[test]
fn t55_no_panic_on_default_dtype_lookup() {
    // Ensures NumpyError carries a non-empty message. M7.6 (ADR-0021)
    // widened the dtype enum to include `Complex128`; pick an
    // unsupported sentinel string to keep this regression test live.
    let err: NumpyError = Dtype::from_python_string("complex32").unwrap_err();
    assert!(!err.message.is_empty());
}
