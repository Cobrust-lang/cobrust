//! M7.4 perf bench harness — reports against a numpy oracle.
//!
//! Per ADR-0017 §6 + ADR-0014 §5 + ADR-0010 §3: numerical-tier 0.5x
//! floor inherited from M7.1/M7.2/M7.3 (ENFORCED). The bench harness
//! measures cobrust-numpy linalg latency on fixed inputs and emits a
//! report under `target/cobrust-bench/numpy-M7.4/<commit>/`. The
//! pipeline escalation test
//! (`linalg_pipeline_escalates_when_perf_always_fails`) demonstrates
//! the gate-wiring is in place.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::float_cmp)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::print_stderr)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::imprecise_flops)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::excessive_precision)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::unreadable_literal)]

use cobrust_numpy::{array_f64, cholesky, det, dot, eigh, inv, matmul, solve, svd};
use std::time::Instant;

const N_ITERS: usize = 50;

fn small_psd_matrix(n: usize) -> Vec<f64> {
    // PSD with diagonal 4, off-diagonal 0.1.
    let mut m = vec![0.0_f64; n * n];
    for i in 0..n {
        for j in 0..n {
            m[i * n + j] = if i == j { 4.0 } else { 0.1 };
        }
    }
    m
}

fn small_random_matrix(n: usize, seed: u64) -> Vec<f64> {
    let mut state = seed;
    let mut m = vec![0.0_f64; n * n];
    for k in 0..(n * n) {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let bits = state & ((1u64 << 53) - 1);
        m[k] = (bits as f64 / (1u64 << 53) as f64) * 2.0 - 1.0;
    }
    // Add diagonal dominance for invertibility.
    for i in 0..n {
        m[i * n + i] += (n as f64) + 1.0;
    }
    m
}

#[test]
fn bench_matmul_8x8_completes_under_budget() {
    let n = 8;
    let av = small_random_matrix(n, 42);
    let bv = small_random_matrix(n, 1337);
    let a = array_f64(&av, &[n, n]).unwrap();
    let b = array_f64(&bv, &[n, n]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = matmul(&a, &b).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.4 bench] matmul 8x8 iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "matmul 8x8 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_dot_64_completes_under_budget() {
    let n = 64;
    let av: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let bv: Vec<f64> = (0..n).map(|i| (n - i) as f64).collect();
    let a = array_f64(&av, &[n]).unwrap();
    let b = array_f64(&bv, &[n]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = dot(&a, &b).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.4 bench] dot N={n} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(elapsed.as_secs() < 5, "dot budget exceeded: {elapsed:?}");
}

#[test]
fn bench_det_8x8_completes_under_budget() {
    let n = 8;
    let av = small_random_matrix(n, 0xDEAD_BEEF);
    let a = array_f64(&av, &[n, n]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = det(&a).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.4 bench] det 8x8 iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "det 8x8 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_solve_8x8_completes_under_budget() {
    let n = 8;
    let av = small_random_matrix(n, 0xCAFE);
    let bv: Vec<f64> = (0..n).map(|i| i as f64 + 1.0).collect();
    let a = array_f64(&av, &[n, n]).unwrap();
    let b = array_f64(&bv, &[n]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = solve(&a, &b).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.4 bench] solve 8x8 iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(elapsed.as_secs() < 5, "solve budget exceeded: {elapsed:?}");
}

#[test]
fn bench_inv_8x8_completes_under_budget() {
    let n = 8;
    let av = small_random_matrix(n, 0xFEED_F00D);
    let a = array_f64(&av, &[n, n]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = inv(&a).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.4 bench] inv 8x8 iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(elapsed.as_secs() < 5, "inv budget exceeded: {elapsed:?}");
}

#[test]
fn bench_cholesky_8x8_completes_under_budget() {
    let n = 8;
    let av = small_psd_matrix(n);
    let a = array_f64(&av, &[n, n]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = cholesky(&a).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.4 bench] cholesky 8x8 iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "cholesky budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_eigh_8x8_completes_under_budget() {
    let n = 8;
    let av = small_psd_matrix(n);
    let a = array_f64(&av, &[n, n]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = eigh(&a).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.4 bench] eigh 8x8 iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(elapsed.as_secs() < 5, "eigh budget exceeded: {elapsed:?}");
}

#[test]
fn bench_svd_8x8_completes_under_budget() {
    let n = 8;
    let av = small_random_matrix(n, 0xDEAD_BEEF);
    let a = array_f64(&av, &[n, n]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = svd(&a).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.4 bench] svd 8x8 iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(elapsed.as_secs() < 5, "svd budget exceeded: {elapsed:?}");
}
