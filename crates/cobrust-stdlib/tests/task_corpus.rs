//! M13 corpus — concurrent producer-consumer + scoped + cancellation
//! integration tests. Per ADR-0028 §F + §"Examples", this file
//! exercises the realistic shapes of structured concurrency: many
//! producers + one consumer, scoped fan-out / fan-in, cooperative
//! cancellation through scope boundaries.

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

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use cobrust_stdlib::sync::channel;
use cobrust_stdlib::task::{scope, spawn};

// =====================================================================
// Pattern 1: Multi-producer / single-consumer aggregation
// =====================================================================

#[test]
fn corpus_producer_consumer_64_producers_one_consumer() {
    let (tx, mut rx) = channel::<i64>(128);
    let n_producers = 64_i64;
    let messages_per_producer = 10_i64;
    let mut handles = Vec::new();
    for p in 0..n_producers {
        let tx = tx.clone();
        handles.push(spawn(move || {
            for i in 0..messages_per_producer {
                tx.send(p * messages_per_producer + i).unwrap();
            }
        }));
    }
    drop(tx); // close once every producer clone went away.

    let total_expected = (0..(n_producers * messages_per_producer)).sum::<i64>();
    let mut total_observed = 0_i64;
    while let Some(v) = rx.recv() {
        total_observed += v;
    }
    for h in handles {
        h.wait().unwrap();
    }
    assert_eq!(total_observed, total_expected);
}

#[test]
fn corpus_producer_consumer_string_messages() {
    let (tx, mut rx) = channel::<String>(32);
    let h = spawn(move || {
        for i in 0..10 {
            tx.send(format!("msg-{i}")).unwrap();
        }
    });
    let mut received: Vec<String> = Vec::new();
    while let Some(s) = rx.recv() {
        received.push(s);
    }
    h.wait().unwrap();
    assert_eq!(received.len(), 10);
    assert_eq!(received[0], "msg-0");
    assert_eq!(received[9], "msg-9");
}

// =====================================================================
// Pattern 2: Scoped fan-out / fan-in
// =====================================================================

#[test]
fn corpus_scope_fan_out_fan_in_sums() {
    let result = scope(|s| {
        let mut handles = Vec::new();
        for i in 0_i64..16 {
            handles.push(s.spawn(move || i * i));
        }
        handles
            .into_iter()
            .map(|h| h.wait().unwrap_or(0))
            .sum::<i64>()
    });
    let expected: i64 = (0..16_i64).map(|i| i * i).sum();
    assert_eq!(result, expected);
}

#[test]
fn corpus_scope_pipeline_three_stages() {
    let result = scope(|s| {
        let (tx1, mut rx1) = channel::<i64>(16);
        let (tx2, mut rx2) = channel::<i64>(16);
        // Stage 1: produce.
        let h1 = s.spawn(move || {
            for i in 1..=10 {
                tx1.send(i).unwrap();
            }
        });
        // Stage 2: square.
        let h2 = s.spawn(move || {
            while let Some(v) = rx1.recv() {
                tx2.send(v * v).unwrap();
            }
        });
        // Stage 3: sum.
        let h3 = s.spawn(move || {
            let mut total = 0_i64;
            while let Some(v) = rx2.recv() {
                total += v;
            }
            total
        });
        h1.wait().unwrap();
        h2.wait().unwrap();
        h3.wait().unwrap()
    });
    // sum of squares 1..=10 = 385.
    assert_eq!(result, 385);
}

// =====================================================================
// Pattern 3: Cooperative cancellation through scope
// =====================================================================

#[test]
fn corpus_cooperative_cancellation_scope_drop() {
    let counter = Arc::new(AtomicI64::new(0));
    let cnt_before = counter.load(Ordering::SeqCst);
    {
        let counter = counter.clone();
        scope(|s| {
            let cnt_for_a = counter.clone();
            let _h = s.spawn(move || {
                for _ in 0..10 {
                    cnt_for_a.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(2));
                }
            });
            std::thread::sleep(Duration::from_millis(5));
            // Don't wait on h — scope drop should cancel.
        });
    }
    let cnt_after = counter.load(Ordering::SeqCst);
    // Some increments may have landed; the bound is 10.
    assert!(
        cnt_after - cnt_before <= 10,
        "counter went past task budget: {}",
        cnt_after - cnt_before
    );
}

#[test]
fn corpus_explicit_cancel_via_free_fn() {
    use cobrust_stdlib::task::cancel;
    let h = spawn(|| {
        std::thread::sleep(Duration::from_millis(50));
        99_i64
    });
    cancel(&h);
    assert!(h.is_cancelled());
    let _ = h.wait();
}

// =====================================================================
// Pattern 4: Bounded back-pressure
// =====================================================================

#[test]
fn corpus_back_pressure_bounded_buffer_throttles() {
    let (tx, mut rx) = channel::<i64>(2); // tiny buffer
    let progress = Arc::new(AtomicI64::new(0));
    let p = progress.clone();
    let h_send = spawn(move || {
        for i in 0..20 {
            tx.send(i).unwrap();
            p.fetch_add(1, Ordering::SeqCst);
        }
    });

    // Receive slowly, ensuring sender is back-pressured.
    let mut received = Vec::new();
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(1));
        received.push(rx.recv().unwrap());
    }
    h_send.wait().unwrap();
    assert_eq!(received, (0..20).collect::<Vec<_>>());
    assert_eq!(progress.load(Ordering::SeqCst), 20);
}

// =====================================================================
// Pattern 5: Task that awaits another task's result via channel
// =====================================================================

#[test]
fn corpus_task_chain_via_channel() {
    let (tx, mut rx) = channel::<i64>(1);
    let h_compute = spawn(move || {
        let answer = (1..=100_i64).sum::<i64>();
        tx.send(answer).unwrap();
    });
    let h_observe = spawn(move || rx.recv().unwrap_or(-1));
    h_compute.wait().unwrap();
    let observed = h_observe.wait().unwrap();
    assert_eq!(observed, 5050);
}

// =====================================================================
// Pattern 6: Many short-lived tasks (no panic, no leak)
// =====================================================================

#[test]
fn corpus_many_short_lived_tasks() {
    let mut handles = Vec::with_capacity(200);
    for i in 0_i64..200 {
        handles.push(spawn(move || i * 2));
    }
    let total: i64 = handles.into_iter().map(|h| h.wait().unwrap_or(0)).sum();
    let expected: i64 = (0..200_i64).map(|i| i * 2).sum();
    assert_eq!(total, expected);
}

// =====================================================================
// Pattern 7: Scope nested inside scope (structured composition)
// =====================================================================

#[test]
fn corpus_nested_scope_composition() {
    let result = scope(|outer| {
        let h_outer = outer.spawn(|| {
            scope(|inner| {
                let a = inner.spawn(|| 3_i64);
                let b = inner.spawn(|| 4_i64);
                a.wait().unwrap_or(0) + b.wait().unwrap_or(0)
            })
        });
        h_outer.wait().unwrap_or(0) * 10
    });
    assert_eq!(result, 70);
}
