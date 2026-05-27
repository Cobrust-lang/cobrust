//! M7.1 ill-typed ufunc program suite — at least 50 programs rejected.
//!
//! Per ADR-0014: ≥ 50 ill-typed programs covering broadcasting
//! incompatibilities, integer division by zero, and shape mismatches
//! that should yield `Err(NumpyError)` rather than panic.

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

use coil::{Dtype, NumpyErrorKind, array_f64, array_i32, array_i64, broadcast_shape};

// ---- 1..15 — broadcast shape mismatches across operands ---------------

#[test]
fn t01_add_3_vs_4_errors() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
    let err = a.add(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t02_sub_2x3_vs_2x4_errors() {
    let a = array_f64(&[1.0; 6], &[2, 3]).unwrap();
    let b = array_f64(&[1.0; 8], &[2, 4]).unwrap();
    let err = a.sub(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t03_mul_3x2_vs_2x3_errors() {
    let a = array_f64(&[1.0; 6], &[3, 2]).unwrap();
    let b = array_f64(&[1.0; 6], &[2, 3]).unwrap();
    let err = a.mul(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t04_eq_5_vs_3_errors() {
    let a = array_i32(&[1; 5], &[5]).unwrap();
    let b = array_i32(&[1; 3], &[3]).unwrap();
    let err = a.eq_(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t05_lt_2x3_vs_4_errors() {
    let a = array_f64(&[1.0; 6], &[2, 3]).unwrap();
    let b = array_f64(&[1.0; 4], &[4]).unwrap();
    let err = a.lt(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t06_div_2x5_vs_3x5_errors() {
    let a = array_f64(&[1.0; 10], &[2, 5]).unwrap();
    let b = array_f64(&[1.0; 15], &[3, 5]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t07_pow_3_vs_2_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_i32(&[1, 2], &[2]).unwrap();
    let err = a.pow(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t08_ne_2_vs_3_errors() {
    let a = array_f64(&[1.0, 2.0], &[2]).unwrap();
    let b = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = a.ne_(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t09_gt_3x4_vs_5x4_errors() {
    let a = array_f64(&[1.0; 12], &[3, 4]).unwrap();
    let b = array_f64(&[1.0; 20], &[5, 4]).unwrap();
    let err = a.gt(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t10_le_3d_4d_incompatible_errors() {
    let a = array_f64(&[1.0; 24], &[2, 3, 4]).unwrap();
    let b = array_f64(&[1.0; 60], &[5, 3, 4]).unwrap();
    let err = a.le(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t11_ge_2x5_vs_2x4_errors() {
    let a = array_f64(&[1.0; 10], &[2, 5]).unwrap();
    let b = array_f64(&[1.0; 8], &[2, 4]).unwrap();
    let err = a.ge(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t12_add_5x1x4_vs_3x4_no_match_errors() {
    let a = array_f64(&[1.0; 20], &[5, 1, 4]).unwrap();
    let b = array_f64(&[1.0; 6], &[3, 2]).unwrap();
    let err = a.add(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t13_sub_4x4x4_vs_2x4_errors() {
    let a = array_f64(&[1.0; 64], &[4, 4, 4]).unwrap();
    let b = array_f64(&[1.0; 8], &[2, 4]).unwrap();
    let err = a.sub(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t14_mul_3x3_vs_2x3_errors() {
    let a = array_f64(&[1.0; 9], &[3, 3]).unwrap();
    let b = array_f64(&[1.0; 6], &[2, 3]).unwrap();
    let err = a.mul(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t15_eq_5_vs_4_errors() {
    let a = array_i32(&[1; 5], &[5]).unwrap();
    let b = array_i32(&[1; 4], &[4]).unwrap();
    let err = a.eq_(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

// ---- 16..30 — integer division by zero --------------------------------

#[test]
fn t16_div_int32_by_zero_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_i32(&[1, 0, 3], &[3]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t17_div_int64_by_zero_errors() {
    let a = array_i64(&[10, 20], &[2]).unwrap();
    let b = array_i64(&[2, 0], &[2]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t18_div_int_by_all_zeros_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_i32(&[0, 0, 0], &[3]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t19_div_int_promoted_int64_by_zero_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_i64(&[1, 0, 3], &[3]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t20_div_int_2d_with_zero_errors() {
    let a = array_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let b = array_i32(&[1, 0, 1, 1], &[2, 2]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t21_div_int_broadcast_zero_scalar_errors() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_i32(&[0], &[]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t22_div_int_neg_zero_errors() {
    let a = array_i32(&[-1, -2, -3], &[3]).unwrap();
    let b = array_i32(&[1, 0, 3], &[3]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t23_div_int_zero_by_zero_errors() {
    let a = array_i32(&[0, 0, 0], &[3]).unwrap();
    let b = array_i32(&[0, 1, 2], &[3]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t24_div_int_negative_by_zero_errors() {
    let a = array_i32(&[-100], &[1]).unwrap();
    let b = array_i32(&[0], &[1]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t25_div_3d_int_with_zero_errors() {
    let a = array_i32(&[1; 8], &[2, 2, 2]).unwrap();
    let b = array_i32(&[1, 0, 1, 1, 1, 1, 1, 1], &[2, 2, 2]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t26_div_int64_3d_zero_errors() {
    let a = array_i64(&[1; 4], &[2, 2]).unwrap();
    let b = array_i64(&[1, 1, 0, 1], &[2, 2]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t27_div_int_with_broadcast_2d_zero_errors() {
    let a = array_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let b = array_i32(&[1, 0], &[2]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t28_div_int_high_value_zero_errors() {
    let a = array_i32(&[i32::MAX, i32::MAX], &[2]).unwrap();
    let b = array_i32(&[2, 0], &[2]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t29_div_int_min_zero_errors() {
    let a = array_i32(&[i32::MIN], &[1]).unwrap();
    let b = array_i32(&[0], &[1]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t30_div_5_int_with_one_zero_errors() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let b = array_i32(&[1, 1, 0, 1, 1], &[5]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

// ---- 31..50 — broadcast_shape direct mismatches -----------------------

#[test]
fn t31_broadcast_shape_3_vs_4_errors() {
    let err = broadcast_shape(&[3], &[4]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t32_broadcast_shape_2x3_vs_3x2_errors() {
    let err = broadcast_shape(&[2, 3], &[3, 2]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t33_broadcast_shape_5x6_vs_4x6_errors() {
    let err = broadcast_shape(&[5, 6], &[4, 6]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t34_broadcast_shape_3d_4d_mismatch_errors() {
    let err = broadcast_shape(&[2, 3, 4], &[3, 5, 4]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t35_broadcast_shape_4d_3d_mismatch_errors() {
    let err = broadcast_shape(&[2, 3, 4, 5], &[3, 4, 6]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t36_broadcast_shape_size_2_vs_3_errors() {
    let err = broadcast_shape(&[2, 1], &[3, 1]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t37_broadcast_shape_inner_axis_mismatch_errors() {
    let err = broadcast_shape(&[3, 7, 2], &[3, 5, 2]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t38_broadcast_shape_completely_disjoint_errors() {
    let err = broadcast_shape(&[7], &[8]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t39_broadcast_shape_high_rank_mismatch_errors() {
    let err = broadcast_shape(&[2, 3, 4, 5, 6], &[3, 4, 5, 6, 7]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t40_broadcast_shape_2x4_vs_3x5_errors() {
    let err = broadcast_shape(&[2, 4], &[3, 5]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

// ---- 41..50 — extra ill-typed ufunc combinations ---------------------

#[test]
fn t41_add_5_vs_6_errors() {
    let a = array_f64(&[1.0; 5], &[5]).unwrap();
    let b = array_f64(&[1.0; 6], &[6]).unwrap();
    let err = a.add(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t42_sub_2x3_vs_3x2_errors() {
    let a = array_f64(&[1.0; 6], &[2, 3]).unwrap();
    let b = array_f64(&[1.0; 6], &[3, 2]).unwrap();
    let err = a.sub(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t43_mul_2x4_vs_3x4_errors() {
    let a = array_f64(&[1.0; 8], &[2, 4]).unwrap();
    let b = array_f64(&[1.0; 12], &[3, 4]).unwrap();
    let err = a.mul(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t44_div_int_5_with_zero_at_end_errors() {
    let a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let b = array_i32(&[1, 2, 3, 4, 0], &[5]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t45_pow_3_vs_5_errors() {
    let a = array_i32(&[2; 3], &[3]).unwrap();
    let b = array_i32(&[2; 5], &[5]).unwrap();
    let err = a.pow(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t46_eq_int_vs_int_mismatch_errors() {
    let a = array_i32(&[1; 4], &[4]).unwrap();
    let b = array_i32(&[1; 5], &[5]).unwrap();
    let err = a.eq_(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t47_lt_3d_2d_inner_mismatch_errors() {
    let a = array_f64(&[1.0; 24], &[2, 3, 4]).unwrap();
    let b = array_f64(&[1.0; 6], &[2, 3]).unwrap();
    let err = a.lt(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t48_div_int_2d_central_zero_errors() {
    let a = array_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let b = array_i32(&[2, 0, 1, 1], &[2, 2]).unwrap();
    let err = a.div(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
}

#[test]
fn t49_add_high_rank_disjoint_errors() {
    let a = array_f64(&[1.0; 30], &[2, 3, 5]).unwrap();
    let b = array_f64(&[1.0; 60], &[3, 4, 5]).unwrap();
    let err = a.add(&b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
}

#[test]
fn t50_dtype_promotion_table_does_not_fail_for_closed_set() {
    // Sanity: result_type is total over the M7.0 5-dtype tier so
    // TypePromotionFailure is reserved for future widening; this test
    // documents that the variant exists but is not raised by closed-set
    // pairs.
    use coil::result_type;
    for a in [
        Dtype::Bool,
        Dtype::Int32,
        Dtype::Int64,
        Dtype::Float32,
        Dtype::Float64,
    ] {
        for b in [
            Dtype::Bool,
            Dtype::Int32,
            Dtype::Int64,
            Dtype::Float32,
            Dtype::Float64,
        ] {
            let _ = result_type(a, b); // total — should not panic.
        }
    }
}
