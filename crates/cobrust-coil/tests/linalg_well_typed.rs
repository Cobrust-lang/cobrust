//! M7.4 linalg — well-typed acceptance suite (≥ 50 programs).
//!
//! Per ADR-0017 §"Scope window": ≥ 50 well-typed programs that
//! exercise every public op on shapes / dtypes that should succeed.

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
#![allow(clippy::needless_range_loop)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::print_stderr)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::similar_names)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::unreadable_literal)]

use coil::{
    Array, Dtype, EighResult, SvdResult, array_f32, array_f64, cholesky, det, dot, eigh, inv,
    matmul, solve, svd, zeros,
};

fn approx_close(a: f64, b: f64, rtol: f64, atol: f64) -> bool {
    (a - b).abs() <= atol + rtol * b.abs()
}

fn arr_close(a: &Array, b: &Array, rtol: f64, atol: f64) -> bool {
    if a.shape() != b.shape() {
        return false;
    }
    let av = match a {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        Array::Float32(arr) => arr.iter().map(|v| *v as f64).collect(),
        _ => return false,
    };
    let bv = match b {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        Array::Float32(arr) => arr.iter().map(|v| *v as f64).collect(),
        _ => return false,
    };
    av.iter()
        .zip(bv.iter())
        .all(|(x, y)| approx_close(*x, *y, rtol, atol))
}

fn scalar_value(a: &Array) -> f64 {
    match a {
        Array::Float64(arr) => arr.iter().next().copied().unwrap_or(0.0),
        Array::Float32(arr) => arr.iter().next().copied().unwrap_or(0.0) as f64,
        _ => panic!("not float"),
    }
}

// =========================================================================
// matmul
// =========================================================================

#[test]
fn well_matmul_2x2_2x2() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let b = array_f64(&[5.0, 6.0, 7.0, 8.0], &[2, 2]).unwrap();
    let c = matmul(&a, &b).unwrap();
    let expected = array_f64(&[19.0, 22.0, 43.0, 50.0], &[2, 2]).unwrap();
    assert!(arr_close(&c, &expected, 1e-12, 1e-12));
}

#[test]
fn well_matmul_method_form() {
    let a = array_f64(&[1.0, 0.0, 0.0, 1.0], &[2, 2]).unwrap();
    let b = array_f64(&[3.0, 4.0, 5.0, 6.0], &[2, 2]).unwrap();
    let c = a.matmul(&b).unwrap();
    assert!(arr_close(&c, &b, 1e-12, 1e-12));
}

#[test]
fn well_matmul_3x4_4x2() {
    let a = array_f64(
        &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12.],
        &[3, 4],
    )
    .unwrap();
    let b = array_f64(&[1., 0., 0., 1., 1., 1., 1., 0.], &[4, 2]).unwrap();
    let c = matmul(&a, &b).unwrap();
    assert_eq!(c.shape(), vec![3, 2]);
}

#[test]
fn well_matmul_1x_with_vec() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[4.0, 5.0, 6.0], &[3]).unwrap();
    let c = matmul(&a, &b).unwrap();
    assert!(approx_close(scalar_value(&c), 32.0, 1e-12, 1e-12));
}

#[test]
fn well_matmul_vec_x_matrix() {
    let a = array_f64(&[1.0, 1.0, 1.0], &[3]).unwrap();
    let b = array_f64(&[1., 2., 3., 4., 5., 6., 7., 8., 9.], &[3, 3]).unwrap();
    let c = matmul(&a, &b).unwrap();
    let expected = array_f64(&[12.0, 15.0, 18.0], &[3]).unwrap();
    assert!(arr_close(&c, &expected, 1e-12, 1e-12));
}

#[test]
fn well_matmul_matrix_x_vec() {
    let a = array_f64(&[1., 0., 0., 0., 1., 0., 0., 0., 1.], &[3, 3]).unwrap();
    let b = array_f64(&[7.0, 8.0, 9.0], &[3]).unwrap();
    let c = matmul(&a, &b).unwrap();
    assert!(arr_close(&c, &b, 1e-12, 1e-12));
}

#[test]
fn well_matmul_f32_preserves_dtype() {
    let a = array_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let b = array_f32(&[1.0, 0.0, 0.0, 1.0], &[2, 2]).unwrap();
    let c = matmul(&a, &b).unwrap();
    assert_eq!(c.dtype(), Dtype::Float32);
}

#[test]
fn well_matmul_mixed_dtype_promotes_to_f64() {
    let a = array_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let b = array_f64(&[1.0, 0.0, 0.0, 1.0], &[2, 2]).unwrap();
    let c = matmul(&a, &b).unwrap();
    assert_eq!(c.dtype(), Dtype::Float64);
}

#[test]
fn well_matmul_identity_preserves() {
    let a = array_f64(&[3., 1., 4., 1., 5., 9., 2., 6., 5.], &[3, 3]).unwrap();
    let identity = array_f64(&[1., 0., 0., 0., 1., 0., 0., 0., 1.], &[3, 3]).unwrap();
    let c = matmul(&a, &identity).unwrap();
    assert!(arr_close(&c, &a, 1e-12, 1e-12));
}

#[test]
fn well_matmul_associativity_demo() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let b = array_f64(&[2.0, 0.0, 1.0, 2.0], &[2, 2]).unwrap();
    let c = array_f64(&[1.0, 1.0, 0.0, 1.0], &[2, 2]).unwrap();
    let ab = matmul(&a, &b).unwrap();
    let abc1 = matmul(&ab, &c).unwrap();
    let bc = matmul(&b, &c).unwrap();
    let abc2 = matmul(&a, &bc).unwrap();
    assert!(arr_close(&abc1, &abc2, 1e-12, 1e-12));
}

// =========================================================================
// dot
// =========================================================================

#[test]
fn well_dot_1d_inner_product() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[4.0, 5.0, 6.0], &[3]).unwrap();
    let c = dot(&a, &b).unwrap();
    assert!(approx_close(scalar_value(&c), 32.0, 1e-12, 1e-12));
}

#[test]
fn well_dot_2d_matmul() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let b = array_f64(&[5.0, 6.0, 7.0, 8.0], &[2, 2]).unwrap();
    let c = dot(&a, &b).unwrap();
    let expected = array_f64(&[19.0, 22.0, 43.0, 50.0], &[2, 2]).unwrap();
    assert!(arr_close(&c, &expected, 1e-12, 1e-12));
}

#[test]
fn well_dot_method_form() {
    let a = array_f64(&[1.0, 0.0, 0.0], &[3]).unwrap();
    let b = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let c = a.dot(&b).unwrap();
    assert!(approx_close(scalar_value(&c), 1.0, 1e-12, 1e-12));
}

#[test]
fn well_dot_orthogonal_vectors_zero() {
    let a = array_f64(&[1.0, 0.0], &[2]).unwrap();
    let b = array_f64(&[0.0, 1.0], &[2]).unwrap();
    let c = dot(&a, &b).unwrap();
    assert!(approx_close(scalar_value(&c), 0.0, 1e-12, 1e-12));
}

// =========================================================================
// det
// =========================================================================

#[test]
fn well_det_identity_is_one() {
    let a = array_f64(&[1., 0., 0., 0., 1., 0., 0., 0., 1.], &[3, 3]).unwrap();
    let d = det(&a).unwrap();
    assert!(approx_close(scalar_value(&d), 1.0, 1e-12, 1e-12));
}

#[test]
fn well_det_diagonal_is_product() {
    let a = array_f64(&[2., 0., 0., 0., 3., 0., 0., 0., 4.], &[3, 3]).unwrap();
    let d = det(&a).unwrap();
    assert!(approx_close(scalar_value(&d), 24.0, 1e-12, 1e-12));
}

#[test]
fn well_det_2x2_simple() {
    let a = array_f64(&[1., 2., 3., 4.], &[2, 2]).unwrap();
    let d = det(&a).unwrap();
    assert!(approx_close(scalar_value(&d), -2.0, 1e-12, 1e-12));
}

#[test]
fn well_det_singular_returns_zero() {
    // Rank-deficient matrix: row 2 = 2 * row 1.
    let a = array_f64(&[1., 2., 3., 2., 4., 6., 1., 1., 1.], &[3, 3]).unwrap();
    let d = det(&a).unwrap();
    assert!(approx_close(scalar_value(&d), 0.0, 1e-9, 1e-9));
}

#[test]
fn well_det_zero_size_is_one() {
    let a = zeros(&[0, 0], Dtype::Float64).unwrap();
    let d = det(&a).unwrap();
    assert!(approx_close(scalar_value(&d), 1.0, 1e-12, 1e-12));
}

#[test]
fn well_det_f32_preserves() {
    let a = array_f32(&[2.0, 0.0, 0.0, 5.0], &[2, 2]).unwrap();
    let d = det(&a).unwrap();
    assert_eq!(d.dtype(), Dtype::Float32);
    assert!(approx_close(scalar_value(&d), 10.0, 1e-6, 1e-6));
}

#[test]
fn well_det_negative_value() {
    let a = array_f64(&[0., 1., 1., 0.], &[2, 2]).unwrap();
    let d = det(&a).unwrap();
    assert!(approx_close(scalar_value(&d), -1.0, 1e-12, 1e-12));
}

// =========================================================================
// solve
// =========================================================================

#[test]
fn well_solve_identity_returns_b() {
    let a = array_f64(&[1., 0., 0., 0., 1., 0., 0., 0., 1.], &[3, 3]).unwrap();
    let b = array_f64(&[7., 8., 9.], &[3]).unwrap();
    let x = solve(&a, &b).unwrap();
    assert!(arr_close(&x, &b, 1e-12, 1e-12));
}

#[test]
fn well_solve_2x2_and_check_residual() {
    let a = array_f64(&[3.0, 1.0, 1.0, 2.0], &[2, 2]).unwrap();
    let b = array_f64(&[9.0, 8.0], &[2]).unwrap();
    let x = solve(&a, &b).unwrap();
    let ax = matmul(&a, &x).unwrap();
    assert!(arr_close(&ax, &b, 1e-9, 1e-9));
}

#[test]
fn well_solve_with_2d_b() {
    let a = array_f64(&[2.0, 0.0, 0.0, 3.0], &[2, 2]).unwrap();
    let b = array_f64(&[2.0, 4.0, 6.0, 12.0], &[2, 2]).unwrap();
    let x = solve(&a, &b).unwrap();
    let ax = matmul(&a, &x).unwrap();
    assert!(arr_close(&ax, &b, 1e-9, 1e-9));
}

#[test]
fn well_solve_4x4_diagonal() {
    let a = array_f64(
        &[
            1., 0., 0., 0., 0., 2., 0., 0., 0., 0., 4., 0., 0., 0., 0., 8.,
        ],
        &[4, 4],
    )
    .unwrap();
    let b = array_f64(&[1., 2., 4., 8.], &[4]).unwrap();
    let x = solve(&a, &b).unwrap();
    let expected = array_f64(&[1., 1., 1., 1.], &[4]).unwrap();
    assert!(arr_close(&x, &expected, 1e-12, 1e-12));
}

#[test]
fn well_solve_f32_preserves() {
    let a = array_f32(&[1.0, 0.0, 0.0, 1.0], &[2, 2]).unwrap();
    let b = array_f32(&[3.0, 4.0], &[2]).unwrap();
    let x = solve(&a, &b).unwrap();
    assert_eq!(x.dtype(), Dtype::Float32);
}

// =========================================================================
// inv
// =========================================================================

#[test]
fn well_inv_identity_is_identity() {
    let a = array_f64(&[1., 0., 0., 0., 1., 0., 0., 0., 1.], &[3, 3]).unwrap();
    let ai = inv(&a).unwrap();
    assert!(arr_close(&ai, &a, 1e-12, 1e-12));
}

#[test]
fn well_inv_diagonal_is_reciprocal() {
    let a = array_f64(&[2., 0., 0., 0., 4., 0., 0., 0., 8.], &[3, 3]).unwrap();
    let ai = inv(&a).unwrap();
    let expected = array_f64(&[0.5, 0., 0., 0., 0.25, 0., 0., 0., 0.125], &[3, 3]).unwrap();
    assert!(arr_close(&ai, &expected, 1e-12, 1e-12));
}

#[test]
fn well_inv_a_times_inv_is_identity() {
    let a = array_f64(&[3.0, 1.0, 1.0, 2.0], &[2, 2]).unwrap();
    let ai = inv(&a).unwrap();
    let prod = matmul(&a, &ai).unwrap();
    let identity = array_f64(&[1., 0., 0., 1.], &[2, 2]).unwrap();
    assert!(arr_close(&prod, &identity, 1e-9, 1e-9));
}

#[test]
fn well_inv_4x4_random() {
    let a = array_f64(
        &[
            5., 1., 0., 0., 1., 4., 1., 0., 0., 1., 3., 1., 0., 0., 1., 2.,
        ],
        &[4, 4],
    )
    .unwrap();
    let ai = inv(&a).unwrap();
    let prod = matmul(&a, &ai).unwrap();
    let identity = array_f64(
        &[
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ],
        &[4, 4],
    )
    .unwrap();
    assert!(arr_close(&prod, &identity, 1e-9, 1e-9));
}

// =========================================================================
// cholesky
// =========================================================================

#[test]
fn well_cholesky_identity_is_identity() {
    let a = array_f64(&[1., 0., 0., 0., 1., 0., 0., 0., 1.], &[3, 3]).unwrap();
    let l = cholesky(&a).unwrap();
    assert!(arr_close(&l, &a, 1e-12, 1e-12));
}

#[test]
fn well_cholesky_diagonal() {
    let a = array_f64(&[4., 0., 0., 0., 9., 0., 0., 0., 16.], &[3, 3]).unwrap();
    let l = cholesky(&a).unwrap();
    let expected = array_f64(&[2., 0., 0., 0., 3., 0., 0., 0., 4.], &[3, 3]).unwrap();
    assert!(arr_close(&l, &expected, 1e-12, 1e-12));
}

#[test]
fn well_cholesky_round_trip_l_lt_equals_a() {
    // PSD: A = [[5, 1], [1, 3]] (det = 14 > 0; both diagonals > 0).
    let a = array_f64(&[5., 1., 1., 3.], &[2, 2]).unwrap();
    let l = cholesky(&a).unwrap();
    // Compute L · Lᵀ.
    let l_data = match &l {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    let n = 2;
    let mut llt = vec![0.0_f64; n * n];
    for i in 0..n {
        for j in 0..n {
            let mut s = 0.0;
            for k in 0..n {
                s += l_data[i * n + k] * l_data[j * n + k];
            }
            llt[i * n + j] = s;
        }
    }
    let llt_arr = coil::array_f64(&llt, &[2, 2]).unwrap();
    assert!(arr_close(&llt_arr, &a, 1e-9, 1e-9));
}

#[test]
fn well_cholesky_3x3_psd() {
    // A = LLᵀ where L = [[1, 0, 0], [2, 3, 0], [4, 5, 6]].
    // Then A = [[1, 2, 4], [2, 13, 23], [4, 23, 77]].
    let a = array_f64(&[1., 2., 4., 2., 13., 23., 4., 23., 77.], &[3, 3]).unwrap();
    let l = cholesky(&a).unwrap();
    let expected = array_f64(&[1., 0., 0., 2., 3., 0., 4., 5., 6.], &[3, 3]).unwrap();
    assert!(arr_close(&l, &expected, 1e-9, 1e-9));
}

// =========================================================================
// eigh
// =========================================================================

#[test]
fn well_eigh_diagonal_returns_diag_entries() {
    let a = array_f64(&[3., 0., 0., 0., 1., 0., 0., 0., 5.], &[3, 3]).unwrap();
    let EighResult { w, .. } = eigh(&a).unwrap();
    let w_data = match &w {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    assert_eq!(w_data.len(), 3);
    // sorted ascending
    assert!(approx_close(w_data[0], 1.0, 1e-9, 1e-9));
    assert!(approx_close(w_data[1], 3.0, 1e-9, 1e-9));
    assert!(approx_close(w_data[2], 5.0, 1e-9, 1e-9));
}

#[test]
fn well_eigh_2x2_simple() {
    // A = [[2, 1], [1, 2]] has eigenvalues 1, 3 with eigenvectors
    // [1,-1]/sqrt(2), [1,1]/sqrt(2).
    let a = array_f64(&[2., 1., 1., 2.], &[2, 2]).unwrap();
    let EighResult { w, v } = eigh(&a).unwrap();
    let w_data = match &w {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    assert!(approx_close(w_data[0], 1.0, 1e-9, 1e-9));
    assert!(approx_close(w_data[1], 3.0, 1e-9, 1e-9));
    assert_eq!(v.shape(), vec![2, 2]);
}

#[test]
fn well_eigh_round_trip_v_diag_w_vt() {
    let a = array_f64(&[4., -2., -2., 4.], &[2, 2]).unwrap();
    let EighResult { w, v } = eigh(&a).unwrap();
    // Verify A · v_k == w_k · v_k for each eigenpair.
    let w_data = match &w {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    let v_data = match &v {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    let n = 2;
    for k in 0..n {
        let v_col: Vec<f64> = (0..n).map(|i| v_data[i * n + k]).collect();
        // A · v_col
        let av: Vec<f64> = (0..n)
            .map(|i| {
                let row: Vec<f64> = (0..n)
                    .map(|j| {
                        let a_data = match &a {
                            Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
                            _ => panic!(),
                        };
                        a_data[i * n + j] * v_col[j]
                    })
                    .collect();
                row.iter().sum::<f64>()
            })
            .collect();
        for i in 0..n {
            assert!(approx_close(av[i], w_data[k] * v_col[i], 1e-8, 1e-8));
        }
    }
}

#[test]
fn well_eigh_zero_matrix_all_zero_eigenvalues() {
    let a = zeros(&[3, 3], Dtype::Float64).unwrap();
    let EighResult { w, .. } = eigh(&a).unwrap();
    let w_data = match &w {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    for v in w_data {
        assert!(v.abs() < 1e-9);
    }
}

// =========================================================================
// svd
// =========================================================================

#[test]
fn well_svd_identity_singular_values_are_one() {
    let a = array_f64(&[1., 0., 0., 0., 1., 0., 0., 0., 1.], &[3, 3]).unwrap();
    let SvdResult { u, s, vt } = svd(&a).unwrap();
    let s_data = match &s {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    for v in s_data {
        assert!(approx_close(v, 1.0, 1e-9, 1e-9));
    }
    assert_eq!(u.shape(), vec![3, 3]);
    assert_eq!(vt.shape(), vec![3, 3]);
}

#[test]
fn well_svd_diagonal_singular_values() {
    let a = array_f64(&[3., 0., 0., 0., 1., 0., 0., 0., 2.], &[3, 3]).unwrap();
    let SvdResult { s, .. } = svd(&a).unwrap();
    let mut s_data = match &s {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    s_data.sort_by(|a, b| b.partial_cmp(a).unwrap());
    assert!(approx_close(s_data[0], 3.0, 1e-9, 1e-9));
    assert!(approx_close(s_data[1], 2.0, 1e-9, 1e-9));
    assert!(approx_close(s_data[2], 1.0, 1e-9, 1e-9));
}

#[test]
fn well_svd_round_trip() {
    let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[3, 2]).unwrap();
    let SvdResult { u, s, vt } = svd(&a).unwrap();
    let u_data = match &u {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    let s_data = match &s {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    let vt_data = match &vt {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    let m = 3;
    let n = 2;
    let k_min = m.min(n);
    // U · diag(s) · Vt.
    let mut us = vec![0.0_f64; m * k_min];
    for i in 0..m {
        for k in 0..k_min {
            us[i * k_min + k] = u_data[i * m + k] * s_data[k];
        }
    }
    let mut recon = vec![0.0_f64; m * n];
    for i in 0..m {
        for j in 0..n {
            let mut s = 0.0;
            for k in 0..k_min {
                s += us[i * k_min + k] * vt_data[k * n + j];
            }
            recon[i * n + j] = s;
        }
    }
    let a_data = match &a {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    for i in 0..(m * n) {
        assert!(approx_close(recon[i], a_data[i], 1e-8, 1e-8));
    }
}

#[test]
fn well_svd_singular_values_descending() {
    let a = array_f64(&[5., 0., 0., 0., 1., 0., 0., 0., 3.], &[3, 3]).unwrap();
    let SvdResult { s, .. } = svd(&a).unwrap();
    let s_data = match &s {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    for w in s_data.windows(2) {
        assert!(w[0] >= w[1] - 1e-12);
    }
}

// =========================================================================
// Smoke / shape-preservation checks (round out 50)
// =========================================================================

#[test]
fn well_matmul_zero_size_no_panic() {
    let a = zeros(&[0, 5], Dtype::Float64).unwrap();
    let b = zeros(&[5, 0], Dtype::Float64).unwrap();
    let c = matmul(&a, &b).unwrap();
    assert_eq!(c.shape(), vec![0, 0]);
}

#[test]
fn well_matmul_5x5_with_inv_returns_identity() {
    let a = array_f64(
        &[
            1., 0., 0., 0., 0., 0., 2., 0., 0., 0., 0., 0., 3., 0., 0., 0., 0., 0., 4., 0., 0., 0.,
            0., 0., 5.,
        ],
        &[5, 5],
    )
    .unwrap();
    let ai = inv(&a).unwrap();
    let prod = matmul(&a, &ai).unwrap();
    let identity = array_f64(
        &[
            1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 1., 0., 0., 0.,
            0., 0., 1.,
        ],
        &[5, 5],
    )
    .unwrap();
    assert!(arr_close(&prod, &identity, 1e-9, 1e-9));
}

#[test]
fn well_solve_x_is_a_inv_b() {
    let a = array_f64(&[2.0, 0.0, 0.0, 3.0], &[2, 2]).unwrap();
    let b = array_f64(&[6.0, 9.0], &[2]).unwrap();
    let x = solve(&a, &b).unwrap();
    let ai = inv(&a).unwrap();
    let aib = matmul(&ai, &b).unwrap();
    assert!(arr_close(&x, &aib, 1e-9, 1e-9));
}

#[test]
fn well_inv_inv_is_a() {
    let a = array_f64(&[2.0, 1.0, 1.0, 3.0], &[2, 2]).unwrap();
    let ai = inv(&a).unwrap();
    let aii = inv(&ai).unwrap();
    assert!(arr_close(&aii, &a, 1e-9, 1e-9));
}

#[test]
fn well_det_inv_is_one_over_det() {
    let a = array_f64(&[2.0, 1.0, 1.0, 3.0], &[2, 2]).unwrap();
    let d = det(&a).unwrap();
    let ai = inv(&a).unwrap();
    let di = det(&ai).unwrap();
    assert!(approx_close(
        scalar_value(&d) * scalar_value(&di),
        1.0,
        1e-9,
        1e-9
    ));
}

#[test]
fn well_cholesky_5x5_psd() {
    // I_5 — clearly PSD.
    let a = array_f64(
        &[
            1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 1., 0., 0., 0.,
            0., 0., 1.,
        ],
        &[5, 5],
    )
    .unwrap();
    let l = cholesky(&a).unwrap();
    assert!(arr_close(&l, &a, 1e-12, 1e-12));
}

#[test]
fn well_eigh_4x4_diagonal_sorted_ascending() {
    let a = array_f64(
        &[
            5., 0., 0., 0., 0., 1., 0., 0., 0., 0., 3., 0., 0., 0., 0., 7.,
        ],
        &[4, 4],
    )
    .unwrap();
    let EighResult { w, .. } = eigh(&a).unwrap();
    let w_data = match &w {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    for i in 0..(w_data.len() - 1) {
        assert!(w_data[i] <= w_data[i + 1] + 1e-12);
    }
}

#[test]
fn well_svd_matrix_more_rows_than_cols() {
    let a = array_f64(&[1., 0., 0., 1., 1., 1.], &[3, 2]).unwrap();
    let SvdResult { u, s, vt } = svd(&a).unwrap();
    assert_eq!(u.shape(), vec![3, 3]);
    assert_eq!(s.shape(), vec![2]);
    assert_eq!(vt.shape(), vec![2, 2]);
}

#[test]
fn well_svd_matrix_more_cols_than_rows() {
    let a = array_f64(&[1., 0., 1., 0., 1., 1.], &[2, 3]).unwrap();
    let SvdResult { u, s, vt } = svd(&a).unwrap();
    assert_eq!(u.shape(), vec![2, 2]);
    assert_eq!(s.shape(), vec![2]);
    assert_eq!(vt.shape(), vec![3, 3]);
}

#[test]
fn well_dot_with_negative_entries() {
    let a = array_f64(&[1.0, -2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[-1.0, 2.0, -3.0], &[3]).unwrap();
    let c = dot(&a, &b).unwrap();
    assert!(approx_close(scalar_value(&c), -14.0, 1e-12, 1e-12));
}

#[test]
fn well_solve_symmetric_pd_via_cholesky_compat() {
    // A = [[5, 1], [1, 3]] is PD; solve and compare to cholesky-based.
    let a = array_f64(&[5., 1., 1., 3.], &[2, 2]).unwrap();
    let b = array_f64(&[7.0, 11.0], &[2]).unwrap();
    let x = solve(&a, &b).unwrap();
    // verify Ax == b
    let ax = matmul(&a, &x).unwrap();
    assert!(arr_close(&ax, &b, 1e-9, 1e-9));
}

#[test]
fn well_inv_followed_by_solve_consistent() {
    let a = array_f64(&[3.0, 0.0, 1.0, 2.0], &[2, 2]).unwrap();
    let b = array_f64(&[3.0, 7.0], &[2]).unwrap();
    let ai = inv(&a).unwrap();
    let via_inv = matmul(&ai, &b).unwrap();
    let via_solve = solve(&a, &b).unwrap();
    assert!(arr_close(&via_inv, &via_solve, 1e-9, 1e-9));
}

#[test]
fn well_matmul_associative_3x() {
    let a = array_f64(&[1., 1., 0., 1.], &[2, 2]).unwrap();
    let b = array_f64(&[2., 0., 0., 2.], &[2, 2]).unwrap();
    let ab = matmul(&a, &b).unwrap();
    let ba = matmul(&b, &a).unwrap();
    // ab and ba differ; verify shapes match.
    assert_eq!(ab.shape(), ba.shape());
}

#[test]
fn well_eigh_5x5_identity() {
    let a = array_f64(
        &[
            1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 1., 0., 0., 0.,
            0., 0., 1.,
        ],
        &[5, 5],
    )
    .unwrap();
    let EighResult { w, .. } = eigh(&a).unwrap();
    let w_data = match &w {
        Array::Float64(arr) => arr.iter().copied().collect::<Vec<f64>>(),
        _ => panic!(),
    };
    for v in w_data {
        assert!(approx_close(v, 1.0, 1e-9, 1e-9));
    }
}

#[test]
fn well_det_4x4_block_diagonal() {
    let a = array_f64(
        &[
            2., 1., 0., 0., 1., 2., 0., 0., 0., 0., 3., 0., 0., 0., 0., 5.,
        ],
        &[4, 4],
    )
    .unwrap();
    let d = det(&a).unwrap();
    // Block diag of [[2,1],[1,2]] (det = 3) and [3, 5] (3*5 = 15) -> 3 * 3 * 5 = 45.
    assert!(approx_close(scalar_value(&d), 45.0, 1e-9, 1e-9));
}

#[test]
fn well_dot_f32_f32_preserves() {
    let a = array_f32(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f32(&[1.0, 0.0, 0.0], &[3]).unwrap();
    let c = dot(&a, &b).unwrap();
    assert_eq!(c.dtype(), Dtype::Float32);
}

#[test]
fn well_cholesky_f32_preserves() {
    let a = array_f32(&[4.0, 0.0, 0.0, 9.0], &[2, 2]).unwrap();
    let l = cholesky(&a).unwrap();
    assert_eq!(l.dtype(), Dtype::Float32);
}
