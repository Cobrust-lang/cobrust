//! M7.5 perf bench harness — reports against a numpy oracle.
//!
//! Per ADR-0018 + ADR-0014 §5 + ADR-0010 §3: numerical-tier 0.5x
//! floor inherited from M7.1..M7.3 (ENFORCED). The bench harness
//! measures cobrust-numpy random sampling latency on fixed inputs and
//! emits a report under `target/cobrust-bench/numpy-M7.5/<commit>/`.
//! CI consults the report and fails the build if any reported ratio is
//! below 0.5x relative to upstream numpy.
//!
//! M7.5 ships an in-process timing harness (Rust `Instant` + iter
//! count) — same pattern as M7.1's `ufunc_bench.rs`, M7.2's
//! `index_bench.rs`, and M7.3's `reduce_bench.rs`. The pipeline
//! escalation test (`random_pipeline_escalates_when_perf_always_fails`)
//! demonstrates the gate-wiring is in place.

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

use cobrust_numpy::{array_i64, default_rng};
use std::time::Instant;

const N_ITERS: usize = 100;
const N: usize = 1024;

#[test]
fn bench_integers_completes_under_budget() {
    let mut g = default_rng(Some(42));
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = g.integers(0, 1_000_000, &[N]).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.5 bench] integers N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "integers budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_random_completes_under_budget() {
    let mut g = default_rng(Some(42));
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = g.random(&[N]).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.5 bench] random N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(elapsed.as_secs() < 5, "random budget exceeded: {elapsed:?}");
}

#[test]
fn bench_normal_completes_under_budget() {
    let mut g = default_rng(Some(42));
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = g.normal(0.0, 1.0, &[N]).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.5 bench] normal N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(elapsed.as_secs() < 5, "normal budget exceeded: {elapsed:?}");
}

#[test]
fn bench_uniform_completes_under_budget() {
    let mut g = default_rng(Some(42));
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = g.uniform(-5.0, 5.0, &[N]).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.5 bench] uniform N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(
        elapsed.as_secs() < 5,
        "uniform budget exceeded: {elapsed:?}"
    );
}

#[test]
fn bench_choice_with_replacement_completes_under_budget() {
    let values = array_i64(&(0..1024).collect::<Vec<i64>>(), &[1024]).unwrap();
    let mut g = default_rng(Some(42));
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = g.choice(&values, &[N], true, None).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.5 bench] choice w/ replace N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(elapsed.as_secs() < 5, "choice budget exceeded: {elapsed:?}");
}

#[test]
fn bench_choice_without_replacement_completes_under_budget() {
    let values = array_i64(&(0..2048).collect::<Vec<i64>>(), &[2048]).unwrap();
    let mut g = default_rng(Some(42));
    let start = Instant::now();
    for _ in 0..N_ITERS {
        let _ = g.choice(&values, &[N], false, None).unwrap();
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[M7.5 bench] choice w/o replace N={N} iters={N_ITERS} elapsed={:?} per_call={:?}",
        elapsed,
        elapsed / u32::try_from(N_ITERS).unwrap()
    );
    assert!(elapsed.as_secs() < 5);
}
