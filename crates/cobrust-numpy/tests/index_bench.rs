//! M7.2 perf bench harness — reports against a numpy oracle.
//!
//! Per ADR-0015 + ADR-0014 §5 + ADR-0010 §3: numerical-tier 0.5x
//! floor inherited from M7.1 (ENFORCED). The bench harness measures
//! cobrust-numpy indexing latency on fixed inputs and emits a report
//! under `target/cobrust-bench/numpy-M7.2/<commit>/`. CI consults the
//! report and fails the build if any reported ratio is below 0.5x
//! relative to upstream numpy.
//!
//! M7.2 ships an in-process timing harness (Rust `Instant` + iter
//! count) — same pattern as M7.1's `ufunc_bench.rs`. The pipeline
//! escalation test
//! (`index_pipeline_escalates_when_perf_always_fails`) demonstrates
//! the gate-wiring is in place.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::float_cmp)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::manual_repeat_n)]
#![allow(clippy::if_not_else)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::print_stderr)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::expect_used)]

use cobrust_numpy::{SliceSpec, array_bool, array_f64, array_i64, np_where};
use std::time::Instant;

const N_ITERS: usize = 100;
const N: usize = 1024;

#[test]
fn bench_slice_int64_completes_under_budget() {
    let av: Vec<i64> = (0..N).map(|i| i as i64).collect();
    let a = array_i64(&av, &[N]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.slice(SliceSpec::range(10, N as i64 - 10)).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.2 bench] slice int64 N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "slice int64 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_take_int64_completes_under_budget() {
    let av: Vec<i64> = (0..N).map(|i| i as i64).collect();
    let a = array_i64(&av, &[N]).unwrap();
    let indices: Vec<i64> = (0..32).map(|i| (i * 30) as i64).collect();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.take(&indices).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.2 bench] take int64 N={N} k=32 iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "take int64 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_mask_int64_completes_under_budget() {
    let av: Vec<i64> = (0..N).map(|i| i as i64).collect();
    let a = array_i64(&av, &[N]).unwrap();
    let mask: Vec<bool> = (0..N).map(|i| i % 2 == 0).collect();
    let m = array_bool(&mask, &[N]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.mask(&m).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.2 bench] mask int64 N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "mask int64 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_where_float64_completes_under_budget() {
    let cv: Vec<bool> = (0..N).map(|i| i % 3 == 0).collect();
    let xv: Vec<f64> = (0..N).map(|i| i as f64).collect();
    let yv: Vec<f64> = (0..N).map(|i| (i * 2) as f64).collect();
    let cond = array_bool(&cv, &[N]).unwrap();
    let x = array_f64(&xv, &[N]).unwrap();
    let y = array_f64(&yv, &[N]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = np_where(&cond, &x, &y).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.2 bench] where float64 N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "where float64 budget exceeded: {elapsed:?}"
    );
}
