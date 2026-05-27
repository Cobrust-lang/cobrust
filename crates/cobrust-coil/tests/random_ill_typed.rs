//! M7.5 ill-typed random programs (per ADR-0018).
//!
//! ≥ 50 ill-typed random programs covering documented failure paths:
//!   - InvalidIntegerRange (low >= high)
//!   - InvalidDistributionParams (scale <= 0, low >= high, non-finite)
//!   - InvalidProbabilities (p does not sum to 1, length mismatch, negative)
//!   - EmptyChoicePopulation (values.size() == 0)
//!   - InvalidDistributionParams (replace=false, size > values.size())

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
#![allow(clippy::imprecise_flops)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::excessive_precision)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::print_stderr)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::if_not_else)]
#![allow(clippy::unusual_byte_groupings)]

use coil::{NumpyErrorKind, array_f32, array_f64, array_i32, array_i64, default_rng};

#[test]
fn ill_01_integers_low_eq_high() {
    let mut g = default_rng(Some(42));
    let err = g.integers(5, 5, &[10]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidIntegerRange);
}

#[test]
fn ill_02_integers_low_gt_high() {
    let mut g = default_rng(Some(42));
    let err = g.integers(10, 5, &[10]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidIntegerRange);
}

#[test]
fn ill_03_integers_low_max_high_min() {
    let mut g = default_rng(Some(42));
    let err = g.integers(i64::MAX, i64::MIN, &[10]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidIntegerRange);
}

#[test]
fn ill_04_normal_zero_scale() {
    let mut g = default_rng(Some(42));
    let err = g.normal(0.0, 0.0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_05_normal_negative_scale() {
    let mut g = default_rng(Some(42));
    let err = g.normal(0.0, -1.0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_06_normal_nan_scale() {
    let mut g = default_rng(Some(42));
    let err = g.normal(0.0, f64::NAN, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_07_normal_inf_scale() {
    let mut g = default_rng(Some(42));
    let err = g.normal(0.0, f64::INFINITY, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_08_normal_nan_loc() {
    let mut g = default_rng(Some(42));
    let err = g.normal(f64::NAN, 1.0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_09_normal_inf_loc() {
    let mut g = default_rng(Some(42));
    let err = g.normal(f64::INFINITY, 1.0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_10_uniform_low_eq_high() {
    let mut g = default_rng(Some(42));
    let err = g.uniform(5.0, 5.0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_11_uniform_low_gt_high() {
    let mut g = default_rng(Some(42));
    let err = g.uniform(10.0, 5.0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_12_uniform_nan_low() {
    let mut g = default_rng(Some(42));
    let err = g.uniform(f64::NAN, 1.0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_13_uniform_nan_high() {
    let mut g = default_rng(Some(42));
    let err = g.uniform(0.0, f64::NAN, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_14_uniform_inf_low() {
    let mut g = default_rng(Some(42));
    let err = g.uniform(f64::NEG_INFINITY, 1.0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_15_uniform_inf_high() {
    let mut g = default_rng(Some(42));
    let err = g.uniform(0.0, f64::INFINITY, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_16_choice_empty_int64() {
    let v = array_i64(&[], &[0]).unwrap();
    let mut g = default_rng(Some(42));
    let err = g.choice(&v, &[3], true, None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::EmptyChoicePopulation);
}

#[test]
fn ill_17_choice_empty_int32() {
    let v = array_i32(&[], &[0]).unwrap();
    let mut g = default_rng(Some(42));
    let err = g.choice(&v, &[3], true, None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::EmptyChoicePopulation);
}

#[test]
fn ill_18_choice_empty_float64() {
    let v = array_f64(&[], &[0]).unwrap();
    let mut g = default_rng(Some(42));
    let err = g.choice(&v, &[3], true, None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::EmptyChoicePopulation);
}

#[test]
fn ill_19_choice_empty_float32() {
    let v = array_f32(&[], &[0]).unwrap();
    let mut g = default_rng(Some(42));
    let err = g.choice(&v, &[3], true, None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::EmptyChoicePopulation);
}

#[test]
fn ill_20_choice_p_length_short() {
    let v = array_i64(&[1, 2, 3], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.5, 0.5];
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
}

#[test]
fn ill_21_choice_p_length_long() {
    let v = array_i64(&[1, 2], &[2]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.3, 0.3, 0.4];
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
}

#[test]
fn ill_22_choice_p_sum_too_low() {
    let v = array_i64(&[1, 2, 3], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.1, 0.2, 0.3]; // sums to 0.6
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
}

#[test]
fn ill_23_choice_p_sum_too_high() {
    let v = array_i64(&[1, 2, 3], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.5, 0.5, 0.5]; // sums to 1.5
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
}

#[test]
fn ill_24_choice_p_negative() {
    let v = array_i64(&[1, 2, 3], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![-0.1, 0.6, 0.5]; // negative entry
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
}

#[test]
fn ill_25_choice_p_nan() {
    let v = array_i64(&[1, 2, 3], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![f64::NAN, 0.5, 0.5];
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
}

#[test]
fn ill_26_choice_p_inf() {
    let v = array_i64(&[1, 2, 3], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![f64::INFINITY, 0.0, 0.0];
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
}

#[test]
fn ill_27_choice_replace_false_too_many() {
    let v = array_i64(&[10, 20, 30], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let err = g.choice(&v, &[5], false, None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_28_choice_replace_false_with_2d_too_many() {
    let v = array_i64(&[1, 2, 3], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let err = g.choice(&v, &[2, 2], false, None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_29_normal_neg_inf_loc() {
    let mut g = default_rng(Some(42));
    let err = g.normal(f64::NEG_INFINITY, 1.0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_30_normal_neg_zero_scale() {
    let mut g = default_rng(Some(42));
    let err = g.normal(0.0, -0.0, &[3]).unwrap_err(); // -0.0 considered <= 0.0
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_31_uniform_low_above_high_negative() {
    let mut g = default_rng(Some(42));
    let err = g.uniform(-1.0, -2.0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_32_choice_empty_with_p() {
    let v = array_i64(&[], &[0]).unwrap();
    let mut g = default_rng(Some(42));
    let p: Vec<f64> = Vec::new();
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    // Empty population is checked before p validation.
    assert_eq!(err.kind, NumpyErrorKind::EmptyChoicePopulation);
}

#[test]
fn ill_33_integers_negative_low_eq_high() {
    let mut g = default_rng(Some(42));
    let err = g.integers(-5, -5, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidIntegerRange);
}

#[test]
fn ill_34_choice_p_with_empty_array_priority() {
    // EmptyChoicePopulation must take priority over invalid p.
    let v = array_i64(&[], &[0]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.5, 0.5];
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::EmptyChoicePopulation);
}

#[test]
fn ill_35_choice_replace_false_too_many_singleton() {
    let v = array_i64(&[7], &[1]).unwrap();
    let mut g = default_rng(Some(42));
    let err = g.choice(&v, &[2], false, None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_36_normal_negative_inf_scale() {
    let mut g = default_rng(Some(42));
    let err = g.normal(0.0, f64::NEG_INFINITY, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_37_uniform_neg_inf_low_pos_inf_high() {
    let mut g = default_rng(Some(42));
    let err = g
        .uniform(f64::NEG_INFINITY, f64::INFINITY, &[3])
        .unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_38_choice_p_sum_close_below() {
    let v = array_i64(&[1, 2], &[2]).unwrap();
    let mut g = default_rng(Some(42));
    // 0.5 + 0.499999 = 0.999999 — exceeds 1e-8 tolerance.
    let p = vec![0.5, 0.499_999];
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
}

#[test]
fn ill_39_choice_p_sum_close_above() {
    let v = array_i64(&[1, 2], &[2]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.5, 0.500_001]; // sum 1.000001 — beyond 1e-8 tol
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
}

#[test]
fn ill_40_integers_zero_zero() {
    let mut g = default_rng(Some(42));
    let err = g.integers(0, 0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidIntegerRange);
}

#[test]
fn ill_41_normal_subnormal_zero() {
    let mut g = default_rng(Some(42));
    // Smallest positive scale should still be valid; 0.0 is invalid.
    let err = g.normal(0.0, 0.0_f64, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_42_uniform_low_zero_high_zero() {
    let mut g = default_rng(Some(42));
    let err = g.uniform(0.0, 0.0, &[3]).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_43_choice_replace_false_too_many_3d() {
    let v = array_i64(&[1, 2, 3, 4], &[4]).unwrap();
    let mut g = default_rng(Some(42));
    let err = g.choice(&v, &[2, 2, 2], false, None).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
}

#[test]
fn ill_44_choice_p_all_zero() {
    let v = array_i64(&[1, 2, 3], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.0, 0.0, 0.0];
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
}

#[test]
fn ill_45_choice_p_two_negative() {
    let v = array_i64(&[1, 2, 3], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![-0.5, -0.5, 2.0];
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
}

#[test]
fn ill_46_normal_message_mentions_scale() {
    let mut g = default_rng(Some(42));
    let err = g.normal(0.0, -1.0, &[3]).unwrap_err();
    assert!(err.message.contains("scale"));
}

#[test]
fn ill_47_uniform_message_mentions_low_high() {
    let mut g = default_rng(Some(42));
    let err = g.uniform(10.0, 5.0, &[3]).unwrap_err();
    assert!(err.message.contains("low"));
}

#[test]
fn ill_48_integers_message_mentions_low_high() {
    let mut g = default_rng(Some(42));
    let err = g.integers(10, 5, &[3]).unwrap_err();
    assert!(err.message.contains("low") && err.message.contains("high"));
}

#[test]
fn ill_49_choice_message_mentions_pop() {
    let v = array_i64(&[], &[0]).unwrap();
    let mut g = default_rng(Some(42));
    let err = g.choice(&v, &[3], true, None).unwrap_err();
    assert!(err.message.contains("non-empty"));
}

#[test]
fn ill_50_choice_p_message_mentions_sum() {
    let v = array_i64(&[1, 2], &[2]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.1, 0.2];
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert!(err.message.contains("sum"));
}

#[test]
fn ill_51_choice_p_message_mentions_length() {
    let v = array_i64(&[1, 2, 3], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.5, 0.5];
    let err = g.choice(&v, &[3], true, Some(&p)).unwrap_err();
    assert!(err.message.contains("length") || err.message.contains("len"));
}
