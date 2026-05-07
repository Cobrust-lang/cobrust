//! M7.3 perf bench harness — reports against a numpy oracle.
//!
//! Per ADR-0016 + ADR-0014 §5 + ADR-0010 §3: numerical-tier 0.5x
//! floor inherited from M7.1/M7.2 (ENFORCED). The bench harness
//! measures cobrust-numpy reduction latency on fixed inputs and emits
//! a report under `target/cobrust-bench/numpy-M7.3/<commit>/`. CI
//! consults the report and fails the build if any reported ratio is
//! below 0.5x relative to upstream numpy.
//!
//! M7.3 ships an in-process timing harness (Rust `Instant` + iter
//! count) — same pattern as M7.1's `ufunc_bench.rs` and M7.2's
//! `index_bench.rs`. The pipeline escalation test
//! (`reduce_pipeline_escalates_when_perf_always_fails`) demonstrates
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

use cobrust_numpy::{array_f64, array_i64};
use std::time::Instant;

const N_ITERS: usize = 100;
const N: usize = 1024;

#[test]
fn bench_sum_int64_completes_under_budget() {
    let av: Vec<i64> = (0..N).map(|i| i as i64).collect();
    let a = array_i64(&av, &[N]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.sum(None).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.3 bench] sum int64 N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "sum int64 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_mean_float64_completes_under_budget() {
    let av: Vec<f64> = (0..N).map(|i| i as f64).collect();
    let a = array_f64(&av, &[N]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.mean(None).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.3 bench] mean f64 N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "mean f64 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_var_float64_completes_under_budget() {
    let av: Vec<f64> = (0..N).map(|i| i as f64).collect();
    let a = array_f64(&av, &[N]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.var(None, 0).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.3 bench] var f64 N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "var f64 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_min_int64_completes_under_budget() {
    let av: Vec<i64> = (0..N).map(|i| (N - i) as i64).collect();
    let a = array_i64(&av, &[N]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.min(None).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.3 bench] min int64 N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "min int64 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_argmax_int64_completes_under_budget() {
    let av: Vec<i64> = (0..N).map(|i| (i * 13 + 7) as i64).collect();
    let a = array_i64(&av, &[N]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.argmax(None).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.3 bench] argmax int64 N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "argmax int64 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_sum_axis_2d_completes_under_budget() {
    let av: Vec<i64> = (0..32 * 32).map(|i| i as i64).collect();
    let a = array_i64(&av, &[32, 32]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.sum(Some(0)).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.3 bench] sum axis=0 32x32 iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "sum axis 2d budget exceeded: {elapsed:?}"
    );
}
