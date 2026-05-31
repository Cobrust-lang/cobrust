//! #157 faer SPIKE — layout-correctness differential gate
//! (`docs/agent/strategy/faer-adoption-survey.md` §6 RISK #4).
//!
//! GATED on `--features coil-faer`: when the feature is ON, `coil::matmul`'s
//! f64 2-D·2-D arm routes through faer's column-major `Mat` GEMM instead of
//! ndarray's row-major `Array2::dot`. The single biggest correctness risk in
//! that swap is a ROW-MAJOR ↔ COLUMN-MAJOR transpose bug in the marshalling.
//!
//! This test pins it by asserting the faer-backed `coil::matmul` equals an
//! INDEPENDENT reference — `ndarray::Array2::dot` (the exact pre-spike
//! backend) — within a tight f64 tolerance, on RECTANGULAR / non-symmetric
//! matrices. Rectangularity is load-bearing: a square *symmetric* product
//! would pass even if one operand were silently transposed, so it could not
//! catch the layout footgun. `3x4 @ 4x2` (non-square, non-symmetric) and a
//! larger `64x64` with an asymmetric ramp WILL diverge under any transpose
//! bug. A `3x4 @ 4x2` also structurally cannot be computed transposed
//! (`4x3 @ 2x4` is not even conformable), so a layout bug there is a panic or
//! a wrong-shape result, not a silent wrong value.
//!
//! When the feature is OFF this whole file compiles to nothing (the single
//! `#[cfg(feature = "coil-faer")]` test) — the default build is untouched.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::float_cmp)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

#[cfg(feature = "coil-faer")]
mod faer_layout {
    use coil::{Array, array_f64, matmul};
    use ndarray::Array2;

    /// Largest absolute element-wise difference between a `coil` matmul
    /// result `(rows x cols)` and an `ndarray` reference. Both are read in
    /// ROW-MAJOR logical order, so this comparison itself is layout-correct.
    fn max_abs_diff(got: &Array, reference: &Array2<f64>, rows: usize, cols: usize) -> f64 {
        let data = match got {
            Array::Float64(arr) => arr,
            other => panic!("expected Float64 matmul result, got {other:?}"),
        };
        assert_eq!(
            data.shape(),
            &[rows, cols],
            "faer matmul produced the wrong SHAPE — a layout/transpose bug"
        );
        let flat: Vec<f64> = data.iter().copied().collect();
        let mut worst = 0.0_f64;
        for i in 0..rows {
            for j in 0..cols {
                let diff = (flat[i * cols + j] - reference[(i, j)]).abs();
                if diff > worst {
                    worst = diff;
                }
            }
        }
        worst
    }

    /// One case: build `a (m x k)` and `b (k x n)` from row-major data, run
    /// the faer-backed `coil::matmul`, and compare to `ndarray`'s `.dot`.
    fn assert_faer_matches_ndarray(
        a_row_major: &[f64],
        m: usize,
        k: usize,
        b_row_major: &[f64],
        n: usize,
    ) {
        assert_eq!(a_row_major.len(), m * k);
        assert_eq!(b_row_major.len(), k * n);

        // coil::matmul == faer GEMM under `--features coil-faer`.
        let a = array_f64(a_row_major, &[m, k]).unwrap();
        let b = array_f64(b_row_major, &[k, n]).unwrap();
        let got = matmul(&a, &b).expect("conformable matmul");

        // Independent reference: ndarray's row-major Array2::dot — the EXACT
        // backend the spike replaces. (Both consume the same row-major data.)
        let a_nd = Array2::from_shape_vec((m, k), a_row_major.to_vec()).unwrap();
        let b_nd = Array2::from_shape_vec((k, n), b_row_major.to_vec()).unwrap();
        let reference = a_nd.dot(&b_nd);

        // Tight tolerance: same GEMM math, only summation-order / rounding may
        // differ → expect parity to ~1e-10 relative on these well-scaled
        // inputs. (The M7.4 numpy gate is rtol=1e-6; this is far tighter
        // because we compare two Rust f64 GEMMs, not vs numpy.)
        let worst = max_abs_diff(&got, &reference, m, n);
        let ref_max = reference.iter().fold(0.0_f64, |acc, &v| acc.max(v.abs()));
        let tol = 1e-9 * ref_max.max(1.0);
        assert!(
            worst <= tol,
            "faer GEMM diverges from ndarray .dot on a {m}x{k} @ {k}x{n}: \
             max|diff| = {worst:e} > tol {tol:e}. A row/col-major transpose \
             bug in the faer marshalling is the prime suspect."
        );
    }

    #[test]
    fn faer_matmul_matches_ndarray_3x4_times_4x2() {
        // 3x4 @ 4x2 -> 3x2. Non-square + non-symmetric: the canonical
        // layout-bug catcher (the transposed shapes 4x3 @ 2x4 are not even
        // conformable, so a transpose bug cannot silently "work").
        // Deliberately asymmetric, non-trivial values (no 0/1 ramps that
        // could mask a swap).
        let a: Vec<f64> = vec![
            1.5, -2.0, 3.25, 0.5, // row 0
            -4.0, 5.5, -6.0, 7.0, // row 1
            8.0, -9.5, 10.0, -11.25, // row 2
        ];
        let b: Vec<f64> = vec![
            2.0, -1.0, // row 0
            0.5, 3.0, // row 1
            -4.0, 2.5, // row 2
            6.0, -0.75, // row 3
        ];
        assert_faer_matches_ndarray(&a, 3, 4, &b, 2);
    }

    #[test]
    fn faer_matmul_matches_ndarray_2x3_times_3x5() {
        // A second rectangular shape with a different aspect ratio, so the
        // marshalling is exercised on both "wide" and "tall" operands.
        let a: Vec<f64> = (0..6).map(|i| (i as f64) * 0.5 - 1.0).collect();
        let b: Vec<f64> = (0..15).map(|i| 2.0 - (i as f64) * 0.25).collect();
        assert_faer_matches_ndarray(&a, 2, 3, &b, 5);
    }

    #[test]
    fn faer_matmul_matches_ndarray_64x64_asymmetric() {
        // Larger N=64. The matrix is deliberately NON-symmetric: entry (i,j)
        // depends on (i, j) asymmetrically (`i*1.0 - j*0.7 + ...`), so the
        // product a@b differs from (a@b)^T and from a^T@b — any transpose in
        // the faer round-trip changes the numbers well beyond tol.
        let n = 64usize;
        let a: Vec<f64> = (0..n * n)
            .map(|idx| {
                let i = (idx / n) as f64;
                let j = (idx % n) as f64;
                i * 1.0 - j * 0.7 + ((i * 31.0 + j * 17.0) % 13.0) * 0.01
            })
            .collect();
        let b: Vec<f64> = (0..n * n)
            .map(|idx| {
                let i = (idx / n) as f64;
                let j = (idx % n) as f64;
                j * 0.3 - i * 0.9 + ((i * 7.0 + j * 23.0) % 11.0) * 0.02
            })
            .collect();
        assert_faer_matches_ndarray(&a, n, n, &b, n);
    }
}
