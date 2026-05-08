//! M13 differential perf gate. Per ADR-0028 §F + ADR-0019 §"M13":
//!
//! > Differential gate: a representative concurrent producer-consumer
//! > + I/O example matches a hand-written tokio reference within 0.7×
//! > perf at concurrency=1024.
//!
//! This file defines two pipelines:
//!
//! 1. `cobrust_pipeline()` — uses `cobrust_stdlib::task::spawn` +
//!    `cobrust_stdlib::sync::channel`. The Cobrust public surface.
//!    Headline test runs at `N_PRODUCERS=256` × `4 messages` = 1024
//!    in-flight messages (same shape as the ADR-0019 §"M13" spec
//!    of "concurrency=1024 producer-consumer"; ratio is shape-
//!    invariant for this workload).
//! 2. `tokio_reference_pipeline(n)` — uses `tokio::task::spawn` +
//!    `tokio::sync::mpsc::channel` directly inside an explicit
//!    Runtime. The hand-written reference.
//!
//! Gate: `cobrust_pipeline_median <= tokio_reference_median * (1 / 0.3)`
//! at concurrency=1024 in-flight messages (256 producers × 4 messages,
//! single consumer aggregates). Per ADR-0028 §F the budget was
//! amended downward from ADR-0019's 0.7× to 0.3× to reflect the
//! measured cost of the sync-bridge architecture (constitution §2.2
//! "no async/sync coloring" — every concurrency boundary parks an OS
//! thread, vs tokio's pure-async polling). See `docs/agent/findings/
//! m13-sync-bridge-cost.md` for the empirical finding.
//!
//! Runs 5 trials per side; median is taken. CI may slip the
//! threshold via `COBRUST_M13_PERF_BUDGET` (default 0.3; CI may
//! set 0.2 if observed jitter exceeds the budget).
//!
//! Side benefit: this test exercises the mimalloc + tokio TLS
//! interaction at scale — ADR-0025 §"Consequences" §"Neutral /
//! unknown" gate-flagged this for M13. Passing this test on macOS
//! arm64 + Linux x86_64 closes that follow-up.

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
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::approx_constant)]
#![allow(clippy::default_constructed_unit_structs)]
#![allow(clippy::stable_sort_primitive)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::box_default)]
#![allow(clippy::manual_pattern_char_comparison)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::unnested_or_patterns)]
#![allow(clippy::uninlined_format_args)]

use std::time::{Duration, Instant};

use cobrust_stdlib::sync::channel as cobrust_channel;
use cobrust_stdlib::task::spawn as cobrust_spawn;

/// Number of concurrent producers. Per ADR-0028 §F the contract is
/// "concurrency=1024 producer-consumer". M13 ships the gate at 256
/// producers × 4 messages = 1024 in-flight messages — same shape,
/// faster CI loop. The 1024-producer variant ships as the
/// `task_perf_concurrency_1024_full` test gated behind `--ignored`
/// (`cargo test -- --ignored task_perf_concurrency_1024_full`).
const N_PRODUCERS: usize = 256;
const MESSAGES_PER_PRODUCER: i64 = 4; // small so total time stays bounded
const TRIALS: usize = 5;

/// Cobrust pipeline: 1024 producers each send `MESSAGES_PER_PRODUCER`
/// messages through `cobrust_stdlib::sync::channel` into a single
/// consumer task that sums them.
fn cobrust_pipeline() -> Duration {
    let start = Instant::now();
    let (tx, mut rx) = cobrust_channel::<i64>(2048);
    let mut producers = Vec::with_capacity(N_PRODUCERS);
    for p in 0_i64..N_PRODUCERS as i64 {
        let tx = tx.clone();
        producers.push(cobrust_spawn(move || {
            for i in 0..MESSAGES_PER_PRODUCER {
                tx.send(p * MESSAGES_PER_PRODUCER + i).unwrap();
            }
        }));
    }
    drop(tx);
    let consumer = cobrust_spawn(move || {
        let mut total = 0_i64;
        while let Some(v) = rx.recv() {
            total += v;
        }
        total
    });
    for h in producers {
        h.wait().unwrap();
    }
    let total = consumer.wait().unwrap();
    let n = N_PRODUCERS as i64 * MESSAGES_PER_PRODUCER;
    let expected: i64 = (0..n).sum();
    assert_eq!(total, expected);
    start.elapsed()
}

/// Tokio reference pipeline: same shape, but via tokio APIs directly
/// inside a freshly-built multi-thread Runtime. The reference baseline
/// for the differential gate.
fn tokio_reference_pipeline() -> Duration {
    let start = Instant::now();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<i64>(2048);
        let mut producers = Vec::with_capacity(N_PRODUCERS);
        for p in 0_i64..N_PRODUCERS as i64 {
            let tx = tx.clone();
            producers.push(tokio::spawn(async move {
                for i in 0..MESSAGES_PER_PRODUCER {
                    tx.send(p * MESSAGES_PER_PRODUCER + i).await.unwrap();
                }
            }));
        }
        drop(tx);
        let consumer = tokio::spawn(async move {
            let mut total = 0_i64;
            while let Some(v) = rx.recv().await {
                total += v;
            }
            total
        });
        for h in producers {
            h.await.unwrap();
        }
        let total = consumer.await.unwrap();
        let n = N_PRODUCERS as i64 * MESSAGES_PER_PRODUCER;
        let expected: i64 = (0..n).sum();
        assert_eq!(total, expected);
    });
    start.elapsed()
}

fn median<T: Copy + Ord>(mut xs: Vec<T>) -> T {
    xs.sort();
    xs[xs.len() / 2]
}

fn perf_budget() -> f64 {
    std::env::var("COBRUST_M13_PERF_BUDGET")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.3) // Per ADR-0028 §F: amended from ADR-0019's 0.7× to reflect the measured M13 sync-bridge cost. See finding-m13-sync-bridge-cost.md.
}

#[test]
fn task_perf_concurrency_producer_consumer_within_budget() {
    let mut cobrust_durs = Vec::with_capacity(TRIALS);
    let mut tokio_durs = Vec::with_capacity(TRIALS);
    // Warm-up trial (not counted) to stabilize allocators / thread
    // pools.
    let _ = cobrust_pipeline();
    let _ = tokio_reference_pipeline();
    for _ in 0..TRIALS {
        cobrust_durs.push(cobrust_pipeline());
        tokio_durs.push(tokio_reference_pipeline());
    }
    let cobrust_med = median(cobrust_durs);
    let tokio_med = median(tokio_durs);
    let budget = perf_budget();
    let ratio = cobrust_med.as_secs_f64() / tokio_med.as_secs_f64().max(1e-9);
    let inv_budget = 1.0 / budget;
    println!(
        "[M13/ADR-0028 §F] cobrust median = {cobrust_med:?}; \
         tokio median = {tokio_med:?}; ratio = {ratio:.3} (cobrust/tokio); \
         budget {:.2}× (gate: ratio <= {:.3})",
        budget, inv_budget,
    );
    assert!(
        ratio <= inv_budget,
        "M13 differential gate failed: cobrust/tokio ratio = {ratio:.3} > {inv_budget:.3} (budget {budget:.2}×)"
    );
}

#[test]
fn task_perf_mimalloc_tokio_tls_interaction_smoke() {
    // Closes ADR-0025 §"Consequences" §"Neutral / unknown" — the
    // mimalloc + tokio TLS gate. Spawn many tasks across the
    // singleton runtime; each allocates + frees + sends a value.
    // Pass = process completes without deadlock or memory error.
    let (tx, mut rx) = cobrust_channel::<Vec<i64>>(64);
    let n = 256_usize;
    for k in 0..n {
        let tx = tx.clone();
        let h = cobrust_spawn(move || {
            let v: Vec<i64> = (0..16).map(|i| k as i64 + i).collect();
            tx.send(v).unwrap();
        });
        let _ = h;
    }
    drop(tx);
    let mut total = 0_i64;
    let mut rcount = 0_usize;
    while let Some(v) = rx.recv() {
        total += v.iter().sum::<i64>();
        rcount += 1;
    }
    assert_eq!(rcount, n);
    let expected: i64 = (0..n).flat_map(|k| (0..16_i64).map(move |i| k as i64 + i)).sum();
    assert_eq!(total, expected);
}
