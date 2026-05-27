//! M7.1 well-typed ufunc program suite — at least 50 programs accepted.
//!
//! Per ADR-0014: ≥ 50 well-typed programs that exercise binary ops
//! across the 5-dtype tier + element-wise math on float dtypes.
//! Each "program" is a sequence of cobrust-coil calls that compute
//! a result and assert on observable invariants (shape, dtype, data).

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

use coil::{
    Array, Dtype, array_bool, array_f32, array_f64, array_i32, array_i64, broadcast_shape,
    result_type,
};

// ---- Helper to extract data as Vec<f64> for assertions ------------------

fn data_f64(a: &Array) -> Vec<f64> {
    match a {
        Array::Int32(v) => v.iter().map(|&x| f64::from(x)).collect(),
        Array::Int64(v) => v.iter().map(|&x| x as f64).collect(),
        Array::Float32(v) => v.iter().map(|&x| f64::from(x)).collect(),
        Array::Float64(v) => v.iter().copied().collect(),
        Array::Bool(v) => v.iter().map(|&x| f64::from(u8::from(x))).collect(),
    }
}

// ---- 1..10 — add across dtypes ------------------------------------------

#[test]
fn t01_add_int32_int32() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_i32(&[10, 20, 30], &[3]).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Int32);
    assert_eq!(data_f64(&c), vec![11.0, 22.0, 33.0]);
}

#[test]
fn t02_add_int32_int64() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_i64(&[10, 20, 30], &[3]).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Int64);
}

#[test]
fn t03_add_int32_float32_promotes_f64() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_f32(&[0.5, 1.5, 2.5], &[3]).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Float64);
}

#[test]
fn t04_add_float32_float64_promotes_f64() {
    let a = array_f32(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[0.5, 1.5, 2.5], &[3]).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Float64);
}

#[test]
fn t05_add_bool_int32_promotes_int32() {
    let a = array_bool(&[true, false, true], &[3]).unwrap();
    let b = array_i32(&[10, 20, 30], &[3]).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Int32);
    assert_eq!(data_f64(&c), vec![11.0, 20.0, 31.0]);
}

#[test]
fn t06_add_2d_2d_same_shape() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let b = array_f64(&[10.0, 20.0, 30.0, 40.0], &[2, 2]).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.shape(), vec![2, 2]);
    assert_eq!(data_f64(&c), vec![11.0, 22.0, 33.0, 44.0]);
}

#[test]
fn t07_add_3d_3d_same_shape() {
    let a = array_f64(&[1.0; 8], &[2, 2, 2]).unwrap();
    let b = array_f64(&[2.0; 8], &[2, 2, 2]).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.shape(), vec![2, 2, 2]);
    assert_eq!(data_f64(&c), vec![3.0; 8]);
}

#[test]
fn t08_add_broadcast_row_to_matrix() {
    // [3, 4] + [4] → [3, 4]
    let a = array_f64(
        &[
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        ],
        &[3, 4],
    )
    .unwrap();
    let b = array_f64(&[100.0, 200.0, 300.0, 400.0], &[4]).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.shape(), vec![3, 4]);
}

#[test]
fn t09_add_broadcast_col_to_matrix() {
    // [3, 1] + [1, 4] → [3, 4]
    let a = array_f64(&[1.0, 2.0, 3.0], &[3, 1]).unwrap();
    let b = array_f64(&[10.0, 20.0, 30.0, 40.0], &[1, 4]).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.shape(), vec![3, 4]);
}

#[test]
fn t10_add_scalar_to_vector() {
    // [] + [3] → [3]
    let a = array_f64(&[5.0], &[]).unwrap();
    let b = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.shape(), vec![3]);
    assert_eq!(data_f64(&c), vec![6.0, 7.0, 8.0]);
}

// ---- 11..20 — sub / mul / div / pow -----------------------------------

#[test]
fn t11_sub_float64_float64() {
    let a = array_f64(&[10.0, 20.0, 30.0], &[3]).unwrap();
    let b = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let c = a.sub(&b).unwrap();
    assert_eq!(data_f64(&c), vec![9.0, 18.0, 27.0]);
}

#[test]
fn t12_mul_int64_int64() {
    let a = array_i64(&[2, 3, 4], &[3]).unwrap();
    let b = array_i64(&[5, 6, 7], &[3]).unwrap();
    let c = a.mul(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Int64);
    assert_eq!(data_f64(&c), vec![10.0, 18.0, 28.0]);
}

#[test]
fn t13_div_float_returns_float() {
    let a = array_f64(&[10.0, 20.0, 30.0], &[3]).unwrap();
    let b = array_f64(&[2.0, 4.0, 5.0], &[3]).unwrap();
    let c = a.div(&b).unwrap();
    assert_eq!(data_f64(&c), vec![5.0, 5.0, 6.0]);
}

#[test]
fn t14_div_int_int_returns_int() {
    let a = array_i32(&[10, 20, 30], &[3]).unwrap();
    let b = array_i32(&[3, 4, 7], &[3]).unwrap();
    let c = a.div(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Int32);
    // Integer floor div: 10/3=3, 20/4=5, 30/7=4
    assert_eq!(data_f64(&c), vec![3.0, 5.0, 4.0]);
}

#[test]
fn t15_pow_float_float() {
    let a = array_f64(&[2.0, 3.0, 4.0], &[3]).unwrap();
    let b = array_f64(&[2.0, 2.0, 2.0], &[3]).unwrap();
    let c = a.pow(&b).unwrap();
    assert_eq!(data_f64(&c), vec![4.0, 9.0, 16.0]);
}

#[test]
fn t16_pow_int_int_zero_squared_is_one() {
    let a = array_i32(&[0, 1, 2], &[3]).unwrap();
    let b = array_i32(&[0, 0, 0], &[3]).unwrap();
    let c = a.pow(&b).unwrap();
    assert_eq!(data_f64(&c), vec![1.0, 1.0, 1.0]);
}

#[test]
fn t17_pow_int_negative_truncates_to_zero() {
    let a = array_i32(&[2, 3, 4], &[3]).unwrap();
    let b = array_i32(&[-1, -2, -3], &[3]).unwrap();
    let c = a.pow(&b).unwrap();
    assert_eq!(data_f64(&c), vec![0.0, 0.0, 0.0]);
}

#[test]
fn t18_sub_broadcast_scalar() {
    let a = array_f64(&[10.0, 20.0, 30.0], &[3]).unwrap();
    let b = array_f64(&[5.0], &[]).unwrap();
    let c = a.sub(&b).unwrap();
    assert_eq!(data_f64(&c), vec![5.0, 15.0, 25.0]);
}

#[test]
fn t19_mul_broadcast_2d() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let b = array_f64(&[10.0, 20.0], &[2]).unwrap();
    let c = a.mul(&b).unwrap();
    assert_eq!(c.shape(), vec![2, 2]);
    assert_eq!(data_f64(&c), vec![10.0, 40.0, 30.0, 80.0]);
}

#[test]
fn t20_div_2d_by_1d() {
    let a = array_f64(&[10.0, 20.0, 30.0, 40.0], &[2, 2]).unwrap();
    let b = array_f64(&[2.0, 4.0], &[2]).unwrap();
    let c = a.div(&b).unwrap();
    assert_eq!(c.shape(), vec![2, 2]);
}

// ---- 21..30 — comparison ufuncs always return Bool -------------------

#[test]
fn t21_eq_int32_int32() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_i32(&[1, 4, 3], &[3]).unwrap();
    let c = a.eq_(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Bool);
    assert_eq!(data_f64(&c), vec![1.0, 0.0, 1.0]);
}

#[test]
fn t22_ne_returns_bool() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[1.0, 5.0, 3.0], &[3]).unwrap();
    let c = a.ne_(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Bool);
    assert_eq!(data_f64(&c), vec![0.0, 1.0, 0.0]);
}

#[test]
fn t23_lt_returns_bool() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_i32(&[2, 2, 2], &[3]).unwrap();
    let c = a.lt(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Bool);
    assert_eq!(data_f64(&c), vec![1.0, 0.0, 0.0]);
}

#[test]
fn t24_le_returns_bool() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[2.0, 2.0, 2.0], &[3]).unwrap();
    let c = a.le(&b).unwrap();
    assert_eq!(data_f64(&c), vec![1.0, 1.0, 0.0]);
}

#[test]
fn t25_gt_returns_bool() {
    let a = array_i64(&[3, 2, 1], &[3]).unwrap();
    let b = array_i64(&[2, 2, 2], &[3]).unwrap();
    let c = a.gt(&b).unwrap();
    assert_eq!(data_f64(&c), vec![1.0, 0.0, 0.0]);
}

#[test]
fn t26_ge_returns_bool() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[2.0, 2.0, 2.0], &[3]).unwrap();
    let c = a.ge(&b).unwrap();
    assert_eq!(data_f64(&c), vec![0.0, 1.0, 1.0]);
}

#[test]
fn t27_eq_promotes_for_comparison() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let c = a.eq_(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Bool);
    assert_eq!(data_f64(&c), vec![1.0, 1.0, 1.0]);
}

#[test]
fn t28_lt_2d_broadcast() {
    let a = array_f64(&[1.0, 5.0, 2.0, 3.0], &[2, 2]).unwrap();
    let b = array_f64(&[2.5], &[]).unwrap();
    let c = a.lt(&b).unwrap();
    assert_eq!(c.shape(), vec![2, 2]);
    assert_eq!(data_f64(&c), vec![1.0, 0.0, 1.0, 0.0]);
}

#[test]
fn t29_eq_bool_bool() {
    let a = array_bool(&[true, false, true], &[3]).unwrap();
    let b = array_bool(&[true, true, false], &[3]).unwrap();
    let c = a.eq_(&b).unwrap();
    assert_eq!(c.dtype(), Dtype::Bool);
    assert_eq!(data_f64(&c), vec![1.0, 0.0, 0.0]);
}

#[test]
fn t30_ne_int_int() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_i32(&[3, 2, 1], &[3]).unwrap();
    let c = a.ne_(&b).unwrap();
    assert_eq!(data_f64(&c), vec![1.0, 0.0, 1.0]);
}

// ---- 31..40 — element-wise math ---------------------------------------

#[test]
fn t31_sin_float64_preserves_f64() {
    let a = array_f64(&[0.0, std::f64::consts::PI], &[2]).unwrap();
    let c = a.sin().unwrap();
    assert_eq!(c.dtype(), Dtype::Float64);
    let v = data_f64(&c);
    assert!((v[0] - 0.0).abs() < 1e-10);
    assert!((v[1] - 0.0).abs() < 1e-10);
}

#[test]
fn t32_cos_float64_preserves_f64() {
    let a = array_f64(&[0.0, std::f64::consts::PI], &[2]).unwrap();
    let c = a.cos().unwrap();
    let v = data_f64(&c);
    assert!((v[0] - 1.0).abs() < 1e-10);
    assert!((v[1] - (-1.0)).abs() < 1e-10);
}

#[test]
fn t33_exp_float64_preserves_f64() {
    let a = array_f64(&[0.0, 1.0], &[2]).unwrap();
    let c = a.exp().unwrap();
    let v = data_f64(&c);
    assert!((v[0] - 1.0).abs() < 1e-10);
    assert!((v[1] - std::f64::consts::E).abs() < 1e-10);
}

#[test]
fn t34_log_float64_preserves_f64() {
    let a = array_f64(&[1.0, std::f64::consts::E], &[2]).unwrap();
    let c = a.log().unwrap();
    let v = data_f64(&c);
    assert!((v[0] - 0.0).abs() < 1e-10);
    assert!((v[1] - 1.0).abs() < 1e-10);
}

#[test]
fn t35_sqrt_float64_preserves_f64() {
    let a = array_f64(&[1.0, 4.0, 9.0], &[3]).unwrap();
    let c = a.sqrt().unwrap();
    assert_eq!(data_f64(&c), vec![1.0, 2.0, 3.0]);
}

#[test]
fn t36_sin_int_promotes_to_f64() {
    let a = array_i32(&[0, 1, 2], &[3]).unwrap();
    let c = a.sin().unwrap();
    assert_eq!(c.dtype(), Dtype::Float64);
}

#[test]
fn t37_cos_float32_preserves_f32() {
    let a = array_f32(&[0.0_f32, 1.0_f32], &[2]).unwrap();
    let c = a.cos().unwrap();
    assert_eq!(c.dtype(), Dtype::Float32);
}

#[test]
fn t38_log_zero_yields_neg_inf() {
    let a = array_f64(&[0.0], &[1]).unwrap();
    let c = a.log().unwrap();
    let v = data_f64(&c);
    assert!(v[0].is_infinite() && v[0] < 0.0);
}

#[test]
fn t39_sqrt_negative_yields_nan() {
    let a = array_f64(&[-1.0, -4.0], &[2]).unwrap();
    let c = a.sqrt().unwrap();
    let v = data_f64(&c);
    assert!(v[0].is_nan() && v[1].is_nan());
}

#[test]
fn t40_exp_2d_array_preserves_shape() {
    let a = array_f64(&[0.0, 1.0, 2.0, 3.0], &[2, 2]).unwrap();
    let c = a.exp().unwrap();
    assert_eq!(c.shape(), vec![2, 2]);
}

// ---- 41..50 — promotion + broadcast helper functions -----------------

#[test]
fn t41_result_type_int32_int32() {
    assert_eq!(result_type(Dtype::Int32, Dtype::Int32), Dtype::Int32);
}

#[test]
fn t42_result_type_int32_float64() {
    assert_eq!(result_type(Dtype::Int32, Dtype::Float64), Dtype::Float64);
}

#[test]
fn t43_result_type_bool_float32() {
    assert_eq!(result_type(Dtype::Bool, Dtype::Float32), Dtype::Float32);
}

#[test]
fn t44_broadcast_shape_equal() {
    assert_eq!(broadcast_shape(&[3, 4], &[3, 4]).unwrap(), vec![3, 4]);
}

#[test]
fn t45_broadcast_shape_size_one_axis() {
    assert_eq!(broadcast_shape(&[3, 1], &[1, 4]).unwrap(), vec![3, 4]);
}

#[test]
fn t46_broadcast_shape_pad_left() {
    assert_eq!(broadcast_shape(&[4], &[3, 4]).unwrap(), vec![3, 4]);
}

#[test]
fn t47_broadcast_shape_higher_dim() {
    assert_eq!(broadcast_shape(&[5, 1, 4], &[3, 4]).unwrap(), vec![5, 3, 4]);
}

#[test]
fn t48_chained_ufuncs_compute_squared_distance() {
    // (a - b) ** 2 element-wise.
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[0.0, 0.0, 0.0], &[3]).unwrap();
    let two = array_f64(&[2.0], &[]).unwrap();
    let diff = a.sub(&b).unwrap();
    let sq = diff.pow(&two).unwrap();
    assert_eq!(data_f64(&sq), vec![1.0, 4.0, 9.0]);
}

#[test]
fn t49_pythagorean_via_ufuncs() {
    // sqrt(a*a + b*b)
    let a = array_f64(&[3.0, 5.0], &[2]).unwrap();
    let b = array_f64(&[4.0, 12.0], &[2]).unwrap();
    let aa = a.mul(&a).unwrap();
    let bb = b.mul(&b).unwrap();
    let sum = aa.add(&bb).unwrap();
    let h = sum.sqrt().unwrap();
    let v = data_f64(&h);
    assert!((v[0] - 5.0).abs() < 1e-10);
    assert!((v[1] - 13.0).abs() < 1e-10);
}

#[test]
fn t50_array_from_nested_2d_then_add() {
    use coil::{NestedList, array_from_nested};
    let nl = NestedList::List(vec![
        NestedList::scalars(&[1.0, 2.0]),
        NestedList::scalars(&[3.0, 4.0]),
    ]);
    let a = array_from_nested(&nl, Dtype::Float64).unwrap();
    let b = array_from_nested(&nl, Dtype::Float64).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.shape(), vec![2, 2]);
    assert_eq!(data_f64(&c), vec![2.0, 4.0, 6.0, 8.0]);
}
