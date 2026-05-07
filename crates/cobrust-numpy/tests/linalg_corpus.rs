//! M7.4 linalg — table-driven correctness tests with hand-computed
//! expected values per ADR-0017 §"In scope".
//!
//! Each entry is a small matrix where the expected output was
//! computed by hand (or via a trusted oracle) and is independent of
//! the cobrust-numpy implementation. Used to catch regressions in
//! the LU / Jacobi / Cholesky kernels that fuzz inputs may smooth
//! over.

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

use cobrust_numpy::{
    Array, EighResult, SvdResult, array_f64, cholesky, det, dot, eigh, inv, matmul, solve, svd,
};

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-9 * b.abs()
}

fn data_of(a: &Array) -> Vec<f64> {
    match a {
        Array::Float64(arr) => arr.iter().copied().collect(),
        Array::Float32(arr) => arr.iter().map(|v| *v as f64).collect(),
        _ => panic!("not float"),
    }
}

fn assert_arr(actual: &Array, expected: &[f64]) {
    let av = data_of(actual);
    assert_eq!(av.len(), expected.len(), "length mismatch");
    for (a, b) in av.iter().zip(expected.iter()) {
        assert!(approx(*a, *b), "expected {b} got {a}");
    }
}

// =========================================================================
// matmul / dot — hand-computed
// =========================================================================

#[test]
fn corpus_matmul_2x3_3x2() {
    let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
    let b = array_f64(&[7., 8., 9., 10., 11., 12.], &[3, 2]).unwrap();
    let c = matmul(&a, &b).unwrap();
    // [[58, 64], [139, 154]]
    assert_arr(&c, &[58., 64., 139., 154.]);
}

#[test]
fn corpus_matmul_3x3_zeros_identity() {
    let a = array_f64(&[0., 1., 2., 3., 4., 5., 6., 7., 8.], &[3, 3]).unwrap();
    let i = array_f64(&[1., 0., 0., 0., 1., 0., 0., 0., 1.], &[3, 3]).unwrap();
    let c = matmul(&a, &i).unwrap();
    assert_arr(&c, &data_of(&a));
}

#[test]
fn corpus_dot_1d_specific() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
    let b = array_f64(&[2.0, 0.0, -1.0, 1.0], &[4]).unwrap();
    let c = dot(&a, &b).unwrap();
    // 1*2 + 2*0 + 3*(-1) + 4*1 = 2 - 3 + 4 = 3
    let v = data_of(&c)[0];
    assert!(approx(v, 3.0));
}

// =========================================================================
// det — hand-computed
// =========================================================================

#[test]
fn corpus_det_2x2_specific() {
    let a = array_f64(&[3., 5., 1., 4.], &[2, 2]).unwrap();
    // det = 3*4 - 5*1 = 7
    let d = det(&a).unwrap();
    assert!(approx(data_of(&d)[0], 7.0));
}

#[test]
fn corpus_det_3x3_specific() {
    // Standard cofactor expansion test:
    // | 1 2 3 |
    // | 0 4 5 | = 1*(4*0-5*1) - 2*(0*0-5*1) + 3*(0*1-4*1)
    // | 1 0 6 |   = 1*(-5) - 2*(-5) + 3*(-4) = -5 + 10 - 12 = -7
    let a = array_f64(&[1., 2., 3., 0., 4., 5., 1., 0., 6.], &[3, 3]).unwrap();
    let d = det(&a).unwrap();
    // numpy: actually det = 1*(4*6 - 5*0) - 2*(0*6 - 5*1) + 3*(0*0 - 4*1)
    //                     = 1*24 - 2*(-5) + 3*(-4)
    //                     = 24 + 10 - 12 = 22
    assert!(approx(data_of(&d)[0], 22.0));
}

#[test]
fn corpus_det_upper_triangular() {
    // det(upper triangular) = product of diagonal
    let a = array_f64(&[2., 1., 3., 0., 5., 7., 0., 0., 4.], &[3, 3]).unwrap();
    let d = det(&a).unwrap();
    assert!(approx(data_of(&d)[0], 40.0));
}

#[test]
fn corpus_det_lower_triangular() {
    // det(lower triangular) = product of diagonal
    let a = array_f64(&[2., 0., 0., 3., 5., 0., 4., 7., 6.], &[3, 3]).unwrap();
    let d = det(&a).unwrap();
    assert!(approx(data_of(&d)[0], 60.0));
}

// =========================================================================
// solve — hand-computed
// =========================================================================

#[test]
fn corpus_solve_2x2_specific() {
    // Solve [[2, 1], [1, 3]] x = [5, 10] → x = [1, 3]
    let a = array_f64(&[2., 1., 1., 3.], &[2, 2]).unwrap();
    let b = array_f64(&[5., 10.], &[2]).unwrap();
    let x = solve(&a, &b).unwrap();
    assert_arr(&x, &[1.0, 3.0]);
}

#[test]
fn corpus_solve_3x3_specific() {
    // Diagonal A = diag(2, 4, 8); b = (4, 16, 32) → x = (2, 4, 4)
    let a = array_f64(&[2., 0., 0., 0., 4., 0., 0., 0., 8.], &[3, 3]).unwrap();
    let b = array_f64(&[4., 16., 32.], &[3]).unwrap();
    let x = solve(&a, &b).unwrap();
    assert_arr(&x, &[2.0, 4.0, 4.0]);
}

#[test]
fn corpus_solve_2d_b() {
    // A = identity 2x2; b = [[1, 2], [3, 4]] → x = b
    let a = array_f64(&[1., 0., 0., 1.], &[2, 2]).unwrap();
    let b = array_f64(&[1., 2., 3., 4.], &[2, 2]).unwrap();
    let x = solve(&a, &b).unwrap();
    assert_arr(&x, &[1., 2., 3., 4.]);
}

// =========================================================================
// inv — hand-computed
// =========================================================================

#[test]
fn corpus_inv_2x2_specific() {
    // A = [[1, 2], [3, 4]] → inv = (1/-2) * [[4, -2], [-3, 1]] = [[-2, 1], [1.5, -0.5]]
    let a = array_f64(&[1., 2., 3., 4.], &[2, 2]).unwrap();
    let ai = inv(&a).unwrap();
    assert_arr(&ai, &[-2.0, 1.0, 1.5, -0.5]);
}

#[test]
fn corpus_inv_3x3_specific_diagonal() {
    let a = array_f64(&[2., 0., 0., 0., 5., 0., 0., 0., 10.], &[3, 3]).unwrap();
    let ai = inv(&a).unwrap();
    assert_arr(&ai, &[0.5, 0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 0.0, 0.1]);
}

// =========================================================================
// cholesky — hand-computed
// =========================================================================

#[test]
fn corpus_cholesky_2x2_specific() {
    // [[4, 12], [12, 37]] → L = [[2, 0], [6, 1]]
    let a = array_f64(&[4., 12., 12., 37.], &[2, 2]).unwrap();
    let l = cholesky(&a).unwrap();
    assert_arr(&l, &[2.0, 0.0, 6.0, 1.0]);
}

#[test]
fn corpus_cholesky_3x3_classic() {
    // Wikipedia example: [[4, 12, -16], [12, 37, -43], [-16, -43, 98]]
    // → L = [[2, 0, 0], [6, 1, 0], [-8, 5, 3]]
    let a = array_f64(&[4., 12., -16., 12., 37., -43., -16., -43., 98.], &[3, 3]).unwrap();
    let l = cholesky(&a).unwrap();
    assert_arr(&l, &[2.0, 0.0, 0.0, 6.0, 1.0, 0.0, -8.0, 5.0, 3.0]);
}

// =========================================================================
// eigh — hand-computed
// =========================================================================

#[test]
fn corpus_eigh_diag_specific() {
    let a = array_f64(&[3., 0., 0., 0., 1., 0., 0., 0., 5.], &[3, 3]).unwrap();
    let EighResult { w, .. } = eigh(&a).unwrap();
    let w_data = data_of(&w);
    // Sorted ascending
    assert!(approx(w_data[0], 1.0));
    assert!(approx(w_data[1], 3.0));
    assert!(approx(w_data[2], 5.0));
}

#[test]
fn corpus_eigh_2x2_classic() {
    // A = [[2, 1], [1, 2]] has eigenvalues 1 and 3.
    let a = array_f64(&[2., 1., 1., 2.], &[2, 2]).unwrap();
    let EighResult { w, .. } = eigh(&a).unwrap();
    let w_data = data_of(&w);
    assert!(approx(w_data[0], 1.0));
    assert!(approx(w_data[1], 3.0));
}

#[test]
fn corpus_eigh_2x2_negative_eigenvalues() {
    // A = [[0, 1], [1, 0]] has eigenvalues +1, -1.
    let a = array_f64(&[0., 1., 1., 0.], &[2, 2]).unwrap();
    let EighResult { w, .. } = eigh(&a).unwrap();
    let w_data = data_of(&w);
    assert!(approx(w_data[0], -1.0));
    assert!(approx(w_data[1], 1.0));
}

// =========================================================================
// svd — verify singular values
// =========================================================================

#[test]
fn corpus_svd_diag_specific() {
    let a = array_f64(&[5., 0., 0., 0., 3., 0., 0., 0., 1.], &[3, 3]).unwrap();
    let SvdResult { s, .. } = svd(&a).unwrap();
    let s_data = data_of(&s);
    // numpy returns in descending order
    assert!(approx(s_data[0], 5.0));
    assert!(approx(s_data[1], 3.0));
    assert!(approx(s_data[2], 1.0));
}

#[test]
fn corpus_svd_2x2_explicit() {
    // A = [[3, 0], [0, 4]] → singular values = [4, 3]
    let a = array_f64(&[3., 0., 0., 4.], &[2, 2]).unwrap();
    let SvdResult { s, .. } = svd(&a).unwrap();
    let s_data = data_of(&s);
    assert!(approx(s_data[0], 4.0));
    assert!(approx(s_data[1], 3.0));
}

// =========================================================================
// Cross-op consistency — corpus
// =========================================================================

#[test]
fn corpus_cross_solve_inv_matmul_consistent() {
    let a = array_f64(&[2., 1., 1., 3.], &[2, 2]).unwrap();
    let b = array_f64(&[4., 7.], &[2]).unwrap();
    let x_solve = solve(&a, &b).unwrap();
    let ai = inv(&a).unwrap();
    let x_inv = matmul(&ai, &b).unwrap();
    assert!(approx(data_of(&x_solve)[0], data_of(&x_inv)[0]));
    assert!(approx(data_of(&x_solve)[1], data_of(&x_inv)[1]));
}

#[test]
fn corpus_cross_det_inv_relation() {
    let a = array_f64(&[1., 2., 3., 4.], &[2, 2]).unwrap();
    let d = det(&a).unwrap();
    let ai = inv(&a).unwrap();
    let di = det(&ai).unwrap();
    // det(A) * det(A⁻¹) = 1
    assert!(approx(data_of(&d)[0] * data_of(&di)[0], 1.0));
}

#[test]
fn corpus_cross_cholesky_l_lt_eq_a() {
    let a = array_f64(&[4., 2., 2., 3.], &[2, 2]).unwrap();
    let l = cholesky(&a).unwrap();
    let l_data = data_of(&l);
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
    let a_data = data_of(&a);
    for k in 0..(n * n) {
        assert!(approx(llt[k], a_data[k]));
    }
}

#[test]
fn corpus_cross_eigh_av_eq_wv() {
    let a = array_f64(&[5., 1., 1., 5.], &[2, 2]).unwrap();
    let EighResult { w, v } = eigh(&a).unwrap();
    let w_data = data_of(&w);
    let v_data = data_of(&v);
    let n = 2;
    for k in 0..n {
        let v_col: Vec<f64> = (0..n).map(|i| v_data[i * n + k]).collect();
        let av: Vec<f64> = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| {
                        let a_data = data_of(&a);
                        a_data[i * n + j] * v_col[j]
                    })
                    .sum()
            })
            .collect();
        for i in 0..n {
            assert!(approx(av[i], w_data[k] * v_col[i]));
        }
    }
}

#[test]
fn corpus_cross_svd_round_trip_3x2() {
    let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[3, 2]).unwrap();
    let SvdResult { u, s, vt } = svd(&a).unwrap();
    let u_data = data_of(&u);
    let s_data = data_of(&s);
    let vt_data = data_of(&vt);
    let m = 3;
    let n = 2;
    let k_min = m.min(n);
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
    let a_data = data_of(&a);
    for i in 0..(m * n) {
        assert!(
            approx(recon[i], a_data[i]),
            "i={i}, expected {} got {}",
            a_data[i],
            recon[i]
        );
    }
}

#[test]
fn corpus_inv_4x4_via_solve_consistency() {
    let a = array_f64(
        &[
            5., 1., 0., 0., 1., 4., 1., 0., 0., 1., 3., 1., 0., 0., 1., 2.,
        ],
        &[4, 4],
    )
    .unwrap();
    let n = 4;
    let mut identity = vec![0.0_f64; n * n];
    for i in 0..n {
        identity[i * n + i] = 1.0;
    }
    let id_arr = array_f64(&identity, &[n, n]).unwrap();
    let inv_via_solve = solve(&a, &id_arr).unwrap();
    let inv_direct = inv(&a).unwrap();
    let v1 = data_of(&inv_via_solve);
    let v2 = data_of(&inv_direct);
    for k in 0..(n * n) {
        assert!(approx(v1[k], v2[k]), "k={k}, {} vs {}", v1[k], v2[k]);
    }
}
