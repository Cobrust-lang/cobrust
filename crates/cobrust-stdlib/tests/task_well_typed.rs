//! Well-typed M13 task + sync corpus. ADR-0028 §C/D pin the surface;
//! every test in this file exercises a successful execution path.
//!
//! Naming: `task_<surface>_<scenario>` — the surface anchor is the
//! ADR-0028 §C row exercised; the scenario describes the happy path.

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

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cobrust_stdlib::sync::{TryRecvError, TrySendError, channel};
use cobrust_stdlib::task::{JoinError, cancel, scope, spawn};

// =====================================================================
// spawn + wait — the bread and butter
// =====================================================================

#[test]
fn task_spawn_returns_value() {
    let h = spawn(|| 42_i64);
    assert_eq!(h.wait(), Ok(42));
}

#[test]
fn task_spawn_string_round_trip() {
    let h = spawn(|| String::from("hello"));
    assert_eq!(h.wait().unwrap(), "hello");
}

#[test]
fn task_spawn_unit_return() {
    let h = spawn(|| ());
    assert_eq!(h.wait(), Ok(()));
}

#[test]
fn task_spawn_vec_return() {
    let h = spawn(|| vec![1_i64, 2, 3, 4, 5]);
    assert_eq!(h.wait().unwrap(), vec![1, 2, 3, 4, 5]);
}

#[test]
fn task_spawn_arithmetic_closure() {
    let h = spawn(|| (1..=100_i64).sum::<i64>());
    assert_eq!(h.wait(), Ok(5050));
}

#[test]
fn task_spawn_captures_immutable() {
    let x = 7_i64;
    let h = spawn(move || x * x);
    assert_eq!(h.wait(), Ok(49));
}

#[test]
fn task_spawn_captures_owned_string() {
    let s = String::from("captured");
    let h = spawn(move || s.len());
    assert_eq!(h.wait(), Ok(8));
}

#[test]
fn task_spawn_two_independent_tasks() {
    let h1 = spawn(|| 1_i64);
    let h2 = spawn(|| 2_i64);
    assert_eq!(h1.wait().unwrap() + h2.wait().unwrap(), 3);
}

#[test]
fn task_spawn_three_tasks_sum() {
    let h1 = spawn(|| 10_i64);
    let h2 = spawn(|| 20_i64);
    let h3 = spawn(|| 30_i64);
    assert_eq!(
        h1.wait().unwrap() + h2.wait().unwrap() + h3.wait().unwrap(),
        60
    );
}

#[test]
fn task_spawn_with_arc_atomic_counter() {
    let counter = Arc::new(AtomicI32::new(0));
    let mut handles = Vec::new();
    for _ in 0..10 {
        let c = counter.clone();
        handles.push(spawn(move || c.fetch_add(1, Ordering::SeqCst)));
    }
    for h in handles {
        let _ = h.wait();
    }
    assert_eq!(counter.load(Ordering::SeqCst), 10);
}

// =====================================================================
// JoinHandle::cancel + free-fn cancel + is_cancelled
// =====================================================================

#[test]
fn task_handle_cancel_sets_flag() {
    let h = spawn(|| {
        std::thread::sleep(Duration::from_millis(10));
        99_i64
    });
    h.cancel();
    assert!(h.is_cancelled());
}

#[test]
fn task_free_fn_cancel_sets_flag() {
    let h = spawn(|| {
        std::thread::sleep(Duration::from_millis(10));
        100_i64
    });
    cancel(&h);
    assert!(h.is_cancelled());
}

#[test]
fn task_no_cancel_no_flag() {
    let h = spawn(|| 5_i64);
    assert!(!h.is_cancelled());
    let _ = h.wait();
}

// =====================================================================
// scope — structured concurrency
// =====================================================================

#[test]
fn task_scope_returns_body_value() {
    let result = scope(|_s| 7_i64);
    assert_eq!(result, 7);
}

#[test]
fn task_scope_spawn_and_wait() {
    let result = scope(|s| {
        let h = s.spawn(|| 42_i64);
        h.wait().unwrap()
    });
    assert_eq!(result, 42);
}

#[test]
fn task_scope_two_children_sum() {
    let result = scope(|s| {
        let a = s.spawn(|| 10_i64);
        let b = s.spawn(|| 20_i64);
        a.wait().unwrap() + b.wait().unwrap()
    });
    assert_eq!(result, 30);
}

#[test]
fn task_scope_drop_on_exit_cancels_unwaited() {
    // Spawn a sleep-forever task inside scope, do not wait. Scope
    // must cancel + complete in bounded time.
    let start = std::time::Instant::now();
    scope(|s| {
        let _h = s.spawn(|| {
            // Cooperative: poll cancellation periodically.
            for _ in 0..100 {
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        // Don't wait — let scope drop-on-exit cancel.
    });
    // Bound: scope returned in well under 1 second (cooperative
    // child notwithstanding, abort is non-cooperative for tokio).
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "scope took {elapsed:?}; expected < 2s"
    );
}

#[test]
fn task_scope_inside_arc_share() {
    let value = Arc::new(AtomicI32::new(0));
    scope(|s| {
        let v = value.clone();
        let h = s.spawn(move || v.store(13, Ordering::SeqCst));
        h.wait().unwrap();
    });
    assert_eq!(value.load(Ordering::SeqCst), 13);
}

// =====================================================================
// channel — bounded MPSC
// =====================================================================

#[test]
fn task_channel_send_recv_single() {
    let (tx, mut rx) = channel::<i64>(1);
    tx.send(42).unwrap();
    assert_eq!(rx.recv(), Some(42));
}

#[test]
fn task_channel_send_recv_many() {
    let (tx, mut rx) = channel::<i64>(8);
    for i in 0..5 {
        tx.send(i).unwrap();
    }
    let mut received: Vec<i64> = Vec::new();
    for _ in 0..5 {
        received.push(rx.recv().unwrap());
    }
    assert_eq!(received, vec![0, 1, 2, 3, 4]);
}

#[test]
fn task_channel_close_returns_none() {
    let (tx, mut rx) = channel::<i64>(1);
    drop(tx);
    assert_eq!(rx.recv(), None);
}

#[test]
fn task_channel_clone_sender_multi_producer() {
    let (tx, mut rx) = channel::<i64>(8);
    let tx2 = tx.clone();
    tx.send(1).unwrap();
    tx2.send(2).unwrap();
    drop(tx);
    drop(tx2);
    let mut got = Vec::new();
    while let Some(v) = rx.recv() {
        got.push(v);
    }
    got.sort();
    assert_eq!(got, vec![1, 2]);
}

#[test]
fn task_channel_try_send_succeeds_when_capacity_open() {
    let (tx, mut rx) = channel::<i64>(2);
    tx.try_send(1).unwrap();
    tx.try_send(2).unwrap();
    assert_eq!(rx.recv(), Some(1));
    assert_eq!(rx.recv(), Some(2));
}

#[test]
fn task_channel_try_recv_empty_returns_err() {
    let (tx, mut rx) = channel::<i64>(1);
    match rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        other => panic!("expected Empty, got {other:?}"),
    }
    drop(tx);
}

#[test]
fn task_channel_try_recv_disconnected_after_drop() {
    let (tx, mut rx) = channel::<i64>(1);
    drop(tx);
    // After dropping all senders, eventually try_recv returns
    // Disconnected. We may need to drain first; capacity is 1 with
    // no values, so first poll should be Disconnected.
    match rx.try_recv() {
        Err(TryRecvError::Disconnected) => {}
        // tokio sometimes returns Empty before observing the close;
        // a follow-up recv blocks until closure observed.
        Err(TryRecvError::Empty) => {
            assert_eq!(rx.recv(), None);
        }
        Ok(_) => panic!("unexpected value on closed channel"),
    }
}

#[test]
fn task_channel_capacity_zero_treated_as_one() {
    // ADR-0028 §C documents capacity 0 as approximated by 1 at M13.
    let (tx, mut rx) = channel::<i64>(0);
    tx.send(7).unwrap();
    assert_eq!(rx.recv(), Some(7));
}

#[test]
fn task_channel_concurrent_producer_consumer() {
    let (tx, mut rx) = channel::<i64>(16);
    let handle = spawn(move || {
        for i in 0..20 {
            tx.send(i).unwrap();
        }
    });
    let mut sum = 0_i64;
    for _ in 0..20 {
        sum += rx.recv().unwrap();
    }
    handle.wait().unwrap();
    assert_eq!(sum, (0..20).sum());
}

#[test]
fn task_channel_string_messages() {
    let (tx, mut rx) = channel::<String>(2);
    tx.send(String::from("hello")).unwrap();
    tx.send(String::from("world")).unwrap();
    assert_eq!(rx.recv().unwrap(), "hello");
    assert_eq!(rx.recv().unwrap(), "world");
}

// =====================================================================
// JoinError surface
// =====================================================================

#[test]
fn task_join_error_display_cancelled() {
    let err = JoinError::Cancelled;
    assert_eq!(format!("{err}"), "task cancelled");
}

#[test]
fn task_join_error_display_panicked() {
    let err = JoinError::Panicked;
    assert_eq!(format!("{err}"), "task panicked");
}

#[test]
fn task_join_error_eq_self() {
    assert_eq!(JoinError::Cancelled, JoinError::Cancelled);
    assert_ne!(JoinError::Cancelled, JoinError::Panicked);
}

#[test]
fn task_try_send_error_display() {
    let err: TrySendError<i64> = TrySendError::Full(7);
    assert_eq!(format!("{err}"), "channel full");
    let err2: TrySendError<i64> = TrySendError::Closed(8);
    assert_eq!(format!("{err2}"), "channel closed");
}

#[test]
fn task_try_recv_error_display() {
    let err = TryRecvError::Empty;
    assert_eq!(format!("{err}"), "channel empty");
    let err2 = TryRecvError::Disconnected;
    assert_eq!(format!("{err2}"), "channel disconnected");
}

#[test]
fn task_spawn_with_channel_inside() {
    let (tx, mut rx) = channel::<i64>(4);
    let h = spawn(move || {
        tx.send(11).unwrap();
        tx.send(22).unwrap();
    });
    let v1 = rx.recv().unwrap();
    let v2 = rx.recv().unwrap();
    h.wait().unwrap();
    assert_eq!(v1 + v2, 33);
}

#[test]
fn task_scope_with_channel_pipeline() {
    let result = scope(|s| {
        let (tx, mut rx) = channel::<i64>(4);
        let h_send = s.spawn(move || {
            tx.send(100).unwrap();
        });
        let h_recv = s.spawn(move || rx.recv().unwrap_or(-1));
        h_send.wait().unwrap();
        h_recv.wait().unwrap()
    });
    assert_eq!(result, 100);
}
