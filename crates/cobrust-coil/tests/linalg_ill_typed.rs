//! M7.4 linalg — ill-typed rejection suite (≥ 50 programs).
//!
//! Per ADR-0017 §"Scope window": ≥ 50 ill-typed programs that
//! return `NumpyError` rather than wrong-shaped output. Covers:
//!
//! - Singular matrices on `inv` / `solve` → `SingularMatrix`.
//! - Non-square `det / inv / solve / eigh / cholesky` → `LinalgShapeError`.
//! - matmul shape mismatch → `LinalgShapeError`.
//! - Non-PSD matrices on `cholesky` → `NotPositiveDefinite`.
//! - Non-symmetric input on `eigh` → `LinalgShapeError`.
//! - Non-float dtypes on every op → `LinalgDtypeUnsupported`.

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
    Dtype, NumpyErrorKind, arange, array_bool, array_f32, array_f64, array_i32, array_i64,
    cholesky, det, dot, eigh, inv, matmul, ones, solve, svd, zeros,
};

// =========================================================================
// Singular matrices → SingularMatrix
// =========================================================================

#[test]
fn ill_inv_singular_zero_matrix() {
    let a = zeros(&[3, 3], Dtype::Float64).unwrap();
    let err = inv(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::SingularMatrix);
}

#[test]
fn ill_inv_singular_rank_deficient() {
    let a = array_f64(&[1., 2., 3., 2., 4., 6., 1., 1., 1.], &[3, 3]).unwrap();
    let err = inv(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::SingularMatrix);
}

#[test]
fn ill_solve_singular_a() {
    let a = array_f64(&[1., 2., 2., 4.], &[2, 2]).unwrap();
    let b = array_f64(&[1.0, 2.0], &[2]).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::SingularMatrix);
}

#[test]
fn ill_inv_singular_2x2() {
    let a = array_f64(&[1., 1., 1., 1.], &[2, 2]).unwrap();
    let err = inv(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::SingularMatrix);
}

#[test]
fn ill_solve_singular_with_2d_b() {
    let a = array_f64(&[0., 1., 0., 1.], &[2, 2]).unwrap();
    let b = array_f64(&[1., 0., 0., 1.], &[2, 2]).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::SingularMatrix);
}

// =========================================================================
// Non-square shapes → LinalgShapeError
// =========================================================================

#[test]
fn ill_det_non_square_2x3() {
    let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
    let err = det(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_det_rank_1_input() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = det(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_inv_non_square() {
    let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[3, 2]).unwrap();
    let err = inv(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_solve_non_square_a() {
    let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
    let b = array_f64(&[7., 8.], &[2]).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_solve_b_wrong_length() {
    let a = array_f64(&[1., 0., 0., 1.], &[2, 2]).unwrap();
    let b = array_f64(&[1., 2., 3.], &[3]).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_solve_b_rank_3() {
    let a = array_f64(&[1., 0., 0., 1.], &[2, 2]).unwrap();
    let b = ones(&[2, 2, 2], Dtype::Float64).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_eigh_non_square() {
    let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[2, 3]).unwrap();
    let err = eigh(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_eigh_rank_1() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = eigh(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_cholesky_non_square() {
    let a = array_f64(&[1., 2., 3., 4., 5., 6.], &[3, 2]).unwrap();
    let err = cholesky(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_cholesky_rank_1() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = cholesky(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_svd_rank_1() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = svd(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_svd_rank_3() {
    let a = ones(&[2, 2, 2], Dtype::Float64).unwrap();
    let err = svd(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

// =========================================================================
// matmul shape mismatch → LinalgShapeError
// =========================================================================

#[test]
fn ill_matmul_shape_mismatch_2d_2d() {
    let a = array_f64(&[1., 2., 3., 4.], &[2, 2]).unwrap();
    let b = array_f64(&[1., 2., 3., 4., 5., 6.], &[3, 2]).unwrap();
    let err = matmul(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_matmul_1d_1d_length_mismatch() {
    let a = array_f64(&[1.0, 2.0], &[2]).unwrap();
    let b = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = matmul(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_matmul_1d_2d_mismatch() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[1., 2., 3., 4.], &[2, 2]).unwrap();
    let err = matmul(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_matmul_2d_1d_mismatch() {
    let a = array_f64(&[1., 2., 3., 4.], &[2, 2]).unwrap();
    let b = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = matmul(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_matmul_rank_3_unsupported() {
    let a = ones(&[2, 2, 2], Dtype::Float64).unwrap();
    let b = ones(&[2, 2, 2], Dtype::Float64).unwrap();
    let err = matmul(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_dot_shape_mismatch() {
    let a = array_f64(&[1.0, 2.0], &[2]).unwrap();
    let b = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let err = dot(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

// =========================================================================
// Cholesky non-PSD → NotPositiveDefinite
// =========================================================================

#[test]
fn ill_cholesky_non_psd_negative_diagonal() {
    let a = array_f64(&[-1., 0., 0., 1.], &[2, 2]).unwrap();
    let err = cholesky(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::NotPositiveDefinite);
}

#[test]
fn ill_cholesky_zero_diagonal_first() {
    let a = array_f64(&[0., 0., 0., 1.], &[2, 2]).unwrap();
    let err = cholesky(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::NotPositiveDefinite);
}

#[test]
fn ill_cholesky_non_psd_indefinite() {
    // Indefinite (eigenvalues +1, -1).
    let a = array_f64(&[0., 1., 1., 0.], &[2, 2]).unwrap();
    let err = cholesky(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::NotPositiveDefinite);
}

#[test]
fn ill_cholesky_3x3_non_psd() {
    let a = array_f64(&[1., 2., 0., 2., 1., 0., 0., 0., 1.], &[3, 3]).unwrap();
    let err = cholesky(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::NotPositiveDefinite);
}

// =========================================================================
// eigh non-symmetric → LinalgShapeError
// =========================================================================

#[test]
fn ill_eigh_non_symmetric() {
    let a = array_f64(&[1., 2., 3., 4.], &[2, 2]).unwrap();
    let err = eigh(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_eigh_non_symmetric_3x3() {
    let a = array_f64(&[1., 0., 1., 0., 2., 0., 0., 0., 3.], &[3, 3]).unwrap();
    // Upper [0][2] = 1 but lower [2][0] = 0; asymmetric.
    let err = eigh(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

// =========================================================================
// Non-float dtypes → LinalgDtypeUnsupported
// =========================================================================

#[test]
fn ill_matmul_int_dtype() {
    let a = array_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let b = array_i32(&[1, 0, 0, 1], &[2, 2]).unwrap();
    let err = matmul(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_matmul_int64_dtype() {
    let a = array_i64(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let b = array_i64(&[1, 0, 0, 1], &[2, 2]).unwrap();
    let err = matmul(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_dot_bool_dtype() {
    let a = array_bool(&[true, false], &[2]).unwrap();
    let b = array_bool(&[true, true], &[2]).unwrap();
    let err = dot(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_det_int_dtype() {
    let a = array_i32(&[1, 0, 0, 1], &[2, 2]).unwrap();
    let err = det(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_det_bool_dtype() {
    let a = array_bool(&[true, false, false, true], &[2, 2]).unwrap();
    let err = det(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_solve_int_dtype() {
    let a = array_i32(&[1, 0, 0, 1], &[2, 2]).unwrap();
    let b = array_i32(&[1, 2], &[2]).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_solve_bool_a() {
    let a = array_bool(&[true, false, false, true], &[2, 2]).unwrap();
    let b = array_f64(&[1., 2.], &[2]).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_inv_int_dtype() {
    let a = array_i32(&[1, 0, 0, 1], &[2, 2]).unwrap();
    let err = inv(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_inv_int64_dtype() {
    let a = array_i64(&[1, 0, 0, 1], &[2, 2]).unwrap();
    let err = inv(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_inv_bool_dtype() {
    let a = array_bool(&[true, false, false, true], &[2, 2]).unwrap();
    let err = inv(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_svd_int_dtype() {
    let a = array_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let err = svd(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_svd_bool_dtype() {
    let a = array_bool(&[true, false, false, true], &[2, 2]).unwrap();
    let err = svd(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_eigh_int_dtype() {
    let a = array_i32(&[1, 0, 0, 1], &[2, 2]).unwrap();
    let err = eigh(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_eigh_bool_dtype() {
    let a = array_bool(&[true, false, false, true], &[2, 2]).unwrap();
    let err = eigh(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_cholesky_int_dtype() {
    let a = array_i32(&[1, 0, 0, 1], &[2, 2]).unwrap();
    let err = cholesky(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_cholesky_bool_dtype() {
    let a = array_bool(&[true, false, false, true], &[2, 2]).unwrap();
    let err = cholesky(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

// =========================================================================
// Mixed bad-shape cases
// =========================================================================

#[test]
fn ill_matmul_2d_with_arange_int() {
    let a = arange(0.0, 4.0, 1.0, Dtype::Int32).unwrap();
    let b = arange(0.0, 4.0, 1.0, Dtype::Int32).unwrap();
    let err = matmul(&a, &b).unwrap_err();
    // Either dtype unsupported or shape (rank-1 1D x 1D length 4 OK; but int dtype reject).
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_solve_2d_a_int_dtype() {
    let a = array_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let b = array_f64(&[1., 2.], &[2]).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_solve_b_int_dtype() {
    let a = array_f64(&[1., 0., 0., 1.], &[2, 2]).unwrap();
    let b = array_i32(&[1, 2], &[2]).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_matmul_mixed_int_float_rejected() {
    let a = array_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let b = array_f64(&[1., 0., 0., 1.], &[2, 2]).unwrap();
    let err = matmul(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_dot_int_int() {
    let a = array_i32(&[1, 2, 3], &[3]).unwrap();
    let b = array_i32(&[4, 5, 6], &[3]).unwrap();
    let err = dot(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ill_eigh_5x5_non_symmetric() {
    // Diagonal-ish but with ONE asymmetric off-diagonal.
    let a = array_f64(
        &[
            1., 0., 0., 0., 0., 0., 2., 0., 0., 5., 0., 0., 3., 0., 0., 0., 0., 0., 4., 0., 0., 0.,
            0., 0., 5.,
        ],
        &[5, 5],
    )
    .unwrap();
    // [1][4] = 5 but [4][1] = 0 — non-symmetric.
    let err = eigh(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_inv_zero_size_singular() {
    // Empty 0x0 — det = 1 by convention; but inv of 0x0 should also be 0x0.
    // We test a 1x1 zero matrix.
    let a = array_f64(&[0.0], &[1, 1]).unwrap();
    let err = inv(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::SingularMatrix);
}

#[test]
fn ill_solve_zero_a_with_nonzero_b() {
    let a = array_f64(&[0., 0., 0., 0.], &[2, 2]).unwrap();
    let b = array_f64(&[1., 2.], &[2]).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::SingularMatrix);
}

#[test]
fn ill_matmul_3d_2d_unsupported() {
    let a = ones(&[2, 3, 3], Dtype::Float64).unwrap();
    let b = ones(&[3, 3], Dtype::Float64).unwrap();
    let err = matmul(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_inv_rank_3_unsupported() {
    let a = ones(&[2, 2, 2], Dtype::Float64).unwrap();
    let err = inv(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_det_rank_3_unsupported() {
    let a = ones(&[2, 2, 2], Dtype::Float64).unwrap();
    let err = det(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_cholesky_rank_3_unsupported() {
    let a = ones(&[2, 2, 2], Dtype::Float64).unwrap();
    let err = cholesky(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_eigh_rank_3_unsupported() {
    let a = ones(&[2, 2, 2], Dtype::Float64).unwrap();
    let err = eigh(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_solve_a_rank_3_unsupported() {
    let a = ones(&[2, 2, 2], Dtype::Float64).unwrap();
    let b = array_f64(&[1., 2.], &[2]).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::LinalgShapeError);
}

#[test]
fn ill_cholesky_negative_3x3() {
    let a = array_f64(&[-1., 0., 0., 0., -1., 0., 0., 0., -1.], &[3, 3]).unwrap();
    let err = cholesky(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::NotPositiveDefinite);
}

#[test]
fn ill_inv_f32_singular() {
    let a = array_f32(&[1.0, 1.0, 1.0, 1.0], &[2, 2]).unwrap();
    let err = inv(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::SingularMatrix);
}

#[test]
fn ill_solve_f32_singular() {
    let a = array_f32(&[0.0, 0.0, 0.0, 0.0], &[2, 2]).unwrap();
    let b = array_f32(&[1.0, 2.0], &[2]).unwrap();
    let err = solve(&a, &b).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::SingularMatrix);
}

#[test]
fn ill_cholesky_f32_non_psd() {
    let a = array_f32(&[-1.0, 0.0, 0.0, 1.0], &[2, 2]).unwrap();
    let err = cholesky(&a).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::NotPositiveDefinite);
}
