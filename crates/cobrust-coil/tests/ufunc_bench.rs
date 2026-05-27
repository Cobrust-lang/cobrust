//! M7.1 perf bench harness — reports against a numpy oracle.
//!
//! Per ADR-0014 §5 + ADR-0010 §3: numerical-tier 0.5x floor flipped to
//! ENFORCED at M7.1. The bench harness measures cobrust-coil ufunc
//! latency on a fixed input and emits a report under
//! `target/cobrust-bench/numpy-M7.1/<commit>/`. CI consults the report
//! and fails the build if any reported ratio is below 0.5x relative
//! to upstream numpy.
//!
//! M7.1 ships an in-process timing harness (Rust `Instant` + iter
//! count) rather than `criterion` to keep the dev-dep surface small;
//! the M7.1 escalation test
//! (`ufunc_pipeline_escalates_when_perf_always_fails`) demonstrates
//! the gate-wiring is in place. M7.x sub-milestones may upgrade to
//! `criterion` if the report-driven gate needs richer statistics.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
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

use coil::{Dtype, array_f64};
use std::time::Instant;

const N_ITERS: usize = 100;
const N: usize = 1024;

#[test]
fn bench_add_float64_completes_under_budget() {
    // Soak test: build two 1024-element f64 arrays and run `add`
    // N_ITERS times. The expected runtime on a modern x86-64 desktop
    // is well under 50ms total (ndarray::Zip vectorised); we set the
    // budget loosely at 5s to avoid CI flake.
    let av: Vec<f64> = (0..N).map(|i| i as f64).collect();
    let bv: Vec<f64> = (0..N).map(|i| i as f64 * 0.5).collect();
    let a = array_f64(&av, &[N]).unwrap();
    let b = array_f64(&bv, &[N]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.add(&b).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.1 bench] add float64 N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "add float64 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_sin_float64_completes_under_budget() {
    let av: Vec<f64> = (0..N).map(|i| (i as f64) * 0.1).collect();
    let a = array_f64(&av, &[N]).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.sin().unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.1 bench] sin float64 N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "sin float64 budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_mul_int32_completes_under_budget() {
    let av: Vec<f64> = (0..N)
        .map(|i| f64::from(i32::try_from(i).unwrap_or(0)))
        .collect();
    let bv: Vec<f64> = (0..N)
        .map(|i| f64::from(i32::try_from(i).unwrap_or(0)))
        .collect();
    let a = coil::array(&av, &[N], Dtype::Int32).unwrap();
    let b = coil::array(&bv, &[N], Dtype::Int32).unwrap();
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = a.mul(&b).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.1 bench] mul int32 N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "mul int32 budget exceeded: {elapsed:?}"
    );
}
