//! M7.3 reduction corpus — table-driven correctness against
//! hand-computed expected values.
//!
//! Per ADR-0016 §"M7.3 scope window": demonstrates each reduction's
//! numerical agreement on a curated set of inputs without relying on
//! the upstream numpy oracle (used for stability when CI hosts lack
//! Python/numpy).

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

use cobrust_numpy::{Array, array_bool, array_f32, array_f64, array_i32, array_i64};

fn iter_f64(a: &Array) -> Vec<f64> {
    match a {
        Array::Float64(arr) => arr.iter().copied().collect(),
        Array::Float32(arr) => arr.iter().map(|v| f64::from(*v)).collect(),
        Array::Int64(arr) => arr.iter().map(|v| *v as f64).collect(),
        Array::Int32(arr) => arr.iter().map(|v| f64::from(*v)).collect(),
        Array::Bool(arr) => arr.iter().map(|v| f64::from(u8::from(*v))).collect(),
    }
}

fn approx_eq(a: f64, b: f64) -> bool {
    if a.is_nan() && b.is_nan() {
        return true;
    }
    let diff = (a - b).abs();
    diff < 1e-10 || (a != 0.0 && (diff / a.abs()) < 1e-9)
}

fn approx_eq_vec(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| approx_eq(*x, *y))
}

#[test]
fn corpus_sum_2x3_axis_0() {
    let a = array_i64(&[1, 2, 3, 4, 5, 6], &[2, 3]).unwrap();
    let r = a.sum(Some(0)).unwrap();
    assert_eq!(iter_f64(&r), vec![5.0, 7.0, 9.0]);
}

#[test]
fn corpus_sum_2x3_axis_1() {
    let a = array_i64(&[1, 2, 3, 4, 5, 6], &[2, 3]).unwrap();
    let r = a.sum(Some(1)).unwrap();
    assert_eq!(iter_f64(&r), vec![6.0, 15.0]);
}

#[test]
fn corpus_prod_2x2_axis_0() {
    let a = array_i64(&[2, 3, 4, 5], &[2, 2]).unwrap();
    let r = a.prod(Some(0)).unwrap();
    assert_eq!(iter_f64(&r), vec![8.0, 15.0]);
}

#[test]
fn corpus_mean_2x3_axis_0() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
    let r = a.mean(Some(0)).unwrap();
    assert!(approx_eq_vec(&iter_f64(&r), &[2.5, 3.5, 4.5]));
}

#[test]
fn corpus_mean_2x3_axis_1() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
    let r = a.mean(Some(1)).unwrap();
    assert!(approx_eq_vec(&iter_f64(&r), &[2.0, 5.0]));
}

#[test]
fn corpus_var_3_ddof_0() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.var(None, 0).unwrap();
    let expected = 2.0_f64 / 3.0;
    assert!(approx_eq(iter_f64(&r)[0], expected));
}

#[test]
fn corpus_var_3_ddof_1() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.var(None, 1).unwrap();
    assert!(approx_eq(iter_f64(&r)[0], 1.0));
}

#[test]
fn corpus_std_3_ddof_0() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.std(None, 0).unwrap();
    let expected = (2.0_f64 / 3.0).sqrt();
    assert!(approx_eq(iter_f64(&r)[0], expected));
}

#[test]
fn corpus_min_2x3_axis_0() {
    let a = array_f64(&[3.0, 1.0, 4.0, 0.5, 2.0, 5.0], &[2, 3]).unwrap();
    let r = a.min(Some(0)).unwrap();
    assert_eq!(iter_f64(&r), vec![0.5, 1.0, 4.0]);
}

#[test]
fn corpus_max_2x3_axis_1() {
    let a = array_f64(&[3.0, 1.0, 4.0, 0.5, 2.0, 5.0], &[2, 3]).unwrap();
    let r = a.max(Some(1)).unwrap();
    assert_eq!(iter_f64(&r), vec![4.0, 5.0]);
}

#[test]
fn corpus_argmin_2x3_axis_0() {
    let a = array_i64(&[3, 1, 4, 0, 2, 5], &[2, 3]).unwrap();
    let r = a.argmin(Some(0)).unwrap();
    assert_eq!(iter_f64(&r), vec![1.0, 0.0, 0.0]);
}

#[test]
fn corpus_argmax_2x3_axis_1() {
    let a = array_i64(&[3, 1, 4, 0, 2, 5], &[2, 3]).unwrap();
    let r = a.argmax(Some(1)).unwrap();
    assert_eq!(iter_f64(&r), vec![2.0, 2.0]);
}

#[test]
fn corpus_pairwise_million_floats_within_numpy_floor() {
    // 10^6 floats of magnitude 1e-9; sum should be 1e-3 within rtol=1e-12.
    let v: Vec<f64> = (0..1_000_000).map(|_| 1e-9).collect();
    let a = array_f64(&v, &[1_000_000]).unwrap();
    let r = a.sum(None).unwrap();
    let expected = 1e-3;
    let s = iter_f64(&r)[0];
    let rel_err = (s - expected).abs() / expected;
    assert!(
        rel_err < 1e-12,
        "pairwise sum precision: got {s}, expected {expected}, rel_err {rel_err}"
    );
}

#[test]
fn corpus_pairwise_alternating_signs() {
    // Alternating +1 / -1 sums to 0 even with 10000 elements.
    let v: Vec<f64> = (0..10_000)
        .map(|i| if i % 2 == 0 { 1.0_f64 } else { -1.0 })
        .collect();
    let a = array_f64(&v, &[10_000]).unwrap();
    let r = a.sum(None).unwrap();
    assert_eq!(iter_f64(&r)[0], 0.0);
}

#[test]
fn corpus_sum_3d_axis_0() {
    let a = array_i64(&[1, 2, 3, 4, 5, 6, 7, 8], &[2, 2, 2]).unwrap();
    let r = a.sum(Some(0)).unwrap();
    // shape [2,2]; values: arr[0][i][j] + arr[1][i][j] for each (i,j)
    // arr[0] = [[1,2],[3,4]]; arr[1] = [[5,6],[7,8]]
    // sum = [[6,8],[10,12]]
    assert_eq!(iter_f64(&r), vec![6.0, 8.0, 10.0, 12.0]);
}

#[test]
fn corpus_sum_3d_axis_1() {
    let a = array_i64(&[1, 2, 3, 4, 5, 6, 7, 8], &[2, 2, 2]).unwrap();
    let r = a.sum(Some(1)).unwrap();
    // shape [2,2]; arr[k][0][j]+arr[k][1][j]
    // arr[0]: [[1,2],[3,4]] → sum axis 1: [4,6]
    // arr[1]: [[5,6],[7,8]] → sum axis 1: [12,14]
    assert_eq!(iter_f64(&r), vec![4.0, 6.0, 12.0, 14.0]);
}

#[test]
fn corpus_mean_promotion_int_to_float() {
    let a = array_i64(&[1, 2, 3, 4], &[4]).unwrap();
    let r = a.mean(None).unwrap();
    assert!(matches!(r, Array::Float64(_)));
    assert_eq!(iter_f64(&r)[0], 2.5);
}

#[test]
fn corpus_mean_f32_preserves_f32() {
    let a = array_f32(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let r = a.mean(None).unwrap();
    assert!(matches!(r, Array::Float32(_)));
}

#[test]
fn corpus_argmin_first_occurrence_2d() {
    // First occurrence per row.
    let a = array_i64(&[2, 1, 1, 4, 3, 3], &[2, 3]).unwrap();
    let r = a.argmin(Some(1)).unwrap();
    assert_eq!(iter_f64(&r), vec![1.0, 1.0]); // first 1 in row 0 at col 1; first 3 in row 1 at col 1
}

// (Replaced by corpus_argmax_first_occurrence_2d_correct below.)

#[test]
fn corpus_argmax_first_occurrence_2d_correct() {
    let a = array_i64(&[1, 2, 2, 4, 1, 4], &[2, 3]).unwrap();
    let r = a.argmax(Some(1)).unwrap();
    let result = iter_f64(&r);
    // arr[0]=[1,2,2]→argmax=1; arr[1]=[4,1,4]→argmax=0
    assert_eq!(result, vec![1.0, 0.0]);
}

#[test]
fn corpus_min_propagates_nan() {
    let a = array_f64(&[1.0, f64::NAN, 3.0], &[3]).unwrap();
    let r = a.min(None).unwrap();
    assert!(iter_f64(&r)[0].is_nan());
}

#[test]
fn corpus_max_propagates_nan() {
    let a = array_f64(&[1.0, f64::NAN, 3.0], &[3]).unwrap();
    let r = a.max(None).unwrap();
    assert!(iter_f64(&r)[0].is_nan());
}

#[test]
fn corpus_sum_int_overflow_wraps() {
    // Per ADR-0014/0016: integer overflow wraps (matches numpy's
    // default).
    let a = array_i32(&[i32::MAX, 1, 0], &[3]).unwrap();
    let r = a.sum(None).unwrap();
    // i32::MAX.wrapping_add(1).wrapping_add(0) = i32::MIN
    assert_eq!(iter_f64(&r)[0], f64::from(i32::MIN));
}

#[test]
fn corpus_prod_bool_all_true() {
    let a = array_bool(&[true, true, true], &[3]).unwrap();
    let r = a.prod(None).unwrap();
    let Array::Int64(arr) = r else {
        panic!("prod(bool) -> Int64");
    };
    assert_eq!(*arr.iter().next().unwrap(), 1);
}

#[test]
fn corpus_prod_bool_with_false() {
    let a = array_bool(&[true, false, true], &[3]).unwrap();
    let r = a.prod(None).unwrap();
    let Array::Int64(arr) = r else {
        panic!("prod(bool) -> Int64");
    };
    assert_eq!(*arr.iter().next().unwrap(), 0);
}
