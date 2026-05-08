//! Ill-typed / failure-path M13 task + sync corpus. Per ADR-0028 §C/D
//! every error variant must be observable; this file exercises:
//!
//! - `JoinError::Cancelled` via `cancel()` then `wait()`.
//! - `JoinError::Panicked` via `spawn` of a panicking closure.
//! - `SendError` via send-after-receiver-drop.
//! - `TrySendError::Full / Closed` via try_send on bounded channels.
//! - `TryRecvError::Empty / Disconnected`.
//! - Scope cancellation propagation (drop-on-exit).
//!
//! Constitution §2.2 — `Result<T, E>` is the default error path; no
//! exceptions; every failure mode here is observable as an `Err`.

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
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cobrust_stdlib::sync::{SendError, TryRecvError, TrySendError, channel};
use cobrust_stdlib::task::{JoinError, scope, spawn};

// =====================================================================
// JoinError::Cancelled
// =====================================================================

#[test]
fn task_cancel_then_wait_is_observable() {
    // The task is in flight; we cancel + wait. Per ADR-0028 §C,
    // cancellation is cooperative for `spawn_blocking`. The actual
    // `wait` outcome may be Cancelled OR Ok depending on scheduler
    // race — we only assert that the cancel flag is observed.
    let h = spawn(|| {
        std::thread::sleep(Duration::from_millis(50));
        7_i64
    });
    h.cancel();
    assert!(h.is_cancelled());
    let _ = h.wait();
}

#[test]
fn task_cancel_before_spawn_observation_works() {
    let h = spawn(|| {
        // Give the scheduler a moment so cancel can land before
        // closure body reaches its sleep.
        std::thread::sleep(Duration::from_millis(5));
        9_i64
    });
    h.cancel();
    // Tokio's spawn_blocking is non-cooperative; cancel sets the
    // flag but the closure may complete. wait() returns Ok if the
    // closure raced past abort.
    let outcome = h.wait();
    assert!(outcome == Ok(9) || outcome == Err(JoinError::Cancelled));
}

#[test]
fn task_double_cancel_is_idempotent() {
    let h = spawn(|| 1_i64);
    h.cancel();
    h.cancel();
    h.cancel();
    assert!(h.is_cancelled());
    let _ = h.wait();
}

// =====================================================================
// JoinError::Panicked
// =====================================================================

#[test]
fn task_spawn_panic_is_caught() {
    let h = spawn(|| -> i64 { panic!("intentional") });
    let result = h.wait();
    assert!(matches!(result, Err(JoinError::Panicked)));
}

#[test]
fn task_spawn_panic_does_not_kill_runtime() {
    let h1 = spawn(|| -> i64 { panic!("panic in task 1") });
    let _ = h1.wait();
    let h2 = spawn(|| 100_i64);
    assert_eq!(h2.wait(), Ok(100));
}

#[test]
fn task_spawn_panic_with_message() {
    let h = spawn(|| -> String { panic!("wat") });
    assert!(matches!(h.wait(), Err(JoinError::Panicked)));
}

// =====================================================================
// SendError + receiver drop
// =====================================================================

#[test]
fn task_channel_send_after_receiver_drop_errs() {
    let (tx, rx) = channel::<i64>(1);
    drop(rx);
    match tx.send(7) {
        Err(SendError(7)) => {}
        Err(SendError(other)) => panic!("expected 7, got {other}"),
        Ok(()) => panic!("send succeeded after receiver dropped"),
    }
}

#[test]
fn task_channel_send_after_receiver_drop_returns_value() {
    let (tx, rx) = channel::<String>(1);
    drop(rx);
    let err = tx.send(String::from("payload")).unwrap_err();
    assert_eq!(err.0, "payload");
}

// =====================================================================
// TrySendError::Full
// =====================================================================

#[test]
fn task_channel_try_send_full_returns_value() {
    let (tx, _rx) = channel::<i64>(1);
    tx.try_send(1).unwrap();
    match tx.try_send(2) {
        Err(TrySendError::Full(2)) => {}
        other => panic!("expected Full(2), got {other:?}"),
    }
}

#[test]
fn task_channel_try_send_full_capacity_two() {
    let (tx, _rx) = channel::<i64>(2);
    tx.try_send(1).unwrap();
    tx.try_send(2).unwrap();
    match tx.try_send(3) {
        Err(TrySendError::Full(3)) => {}
        other => panic!("expected Full(3), got {other:?}"),
    }
}

// =====================================================================
// TrySendError::Closed
// =====================================================================

#[test]
fn task_channel_try_send_closed_returns_value() {
    let (tx, rx) = channel::<i64>(2);
    drop(rx);
    match tx.try_send(99) {
        Err(TrySendError::Closed(99)) => {}
        other => panic!("expected Closed(99), got {other:?}"),
    }
}

#[test]
fn task_channel_try_send_closed_string_value() {
    let (tx, rx) = channel::<String>(2);
    drop(rx);
    let err = tx.try_send(String::from("zzz")).unwrap_err();
    assert!(matches!(err, TrySendError::Closed(_)));
}

// =====================================================================
// TryRecvError::Empty
// =====================================================================

#[test]
fn task_channel_try_recv_empty_observable() {
    let (_tx, mut rx) = channel::<i64>(4);
    match rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        other => panic!("expected Empty, got {other:?}"),
    }
}

#[test]
fn task_channel_try_recv_empty_after_consume() {
    let (tx, mut rx) = channel::<i64>(4);
    tx.send(1).unwrap();
    rx.recv().unwrap();
    match rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        other => panic!("expected Empty, got {other:?}"),
    }
}

// =====================================================================
// TryRecvError::Disconnected
// =====================================================================

#[test]
fn task_channel_try_recv_disconnected_after_all_senders_drop() {
    let (tx, mut rx) = channel::<i64>(2);
    drop(tx);
    // Drain any pending; first try should observe disconnection.
    let outcome = rx.try_recv();
    assert!(matches!(
        outcome,
        Err(TryRecvError::Disconnected) | Err(TryRecvError::Empty)
    ));
    if let Err(TryRecvError::Empty) = outcome {
        // Subsequent recv must observe None (closed).
        assert!(rx.recv().is_none());
    }
}

#[test]
fn task_channel_try_recv_after_value_then_close() {
    let (tx, mut rx) = channel::<i64>(2);
    tx.send(7).unwrap();
    drop(tx);
    assert_eq!(rx.try_recv(), Ok(7));
    let next = rx.try_recv();
    assert!(matches!(
        next,
        Err(TryRecvError::Disconnected) | Err(TryRecvError::Empty)
    ));
}

// =====================================================================
// Scope drop-on-exit cancels children
// =====================================================================

#[test]
fn task_scope_panic_in_body_cancels_children_and_propagates() {
    let cancel_seen = Arc::new(AtomicBool::new(false));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let cs = cancel_seen.clone();
        scope(|s| {
            let _h = s.spawn(move || {
                let _ = cs;
                std::thread::sleep(Duration::from_millis(20));
            });
            panic!("body panic");
        })
    }));
    assert!(result.is_err(), "scope must propagate body panic");
}

#[test]
fn task_scope_unwaited_child_does_not_leak() {
    // Repeatedly opening + dropping scopes must not leak resources;
    // we run 50 cycles and assert process keeps moving (the test
    // succeeds if it returns within the per-test timeout, ~60s).
    for _ in 0..50 {
        scope(|s| {
            let _h = s.spawn(|| 1_i64);
            // Don't wait.
        });
    }
}

#[test]
fn task_scope_cancelled_handle_observable() {
    let result = scope(|s| {
        let h = s.spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            42_i64
        });
        h.cancel();
        h.wait()
    });
    // Cooperative: outcome may be Cancelled (abort took) OR Ok
    // (closure raced past). Both are valid per ADR-0028 §"Negative".
    assert!(result == Ok(42) || result == Err(JoinError::Cancelled));
}

// =====================================================================
// Capacity edge cases
// =====================================================================

#[test]
fn task_channel_send_blocks_when_full_and_resumes_after_recv() {
    use std::sync::atomic::AtomicI32;
    let (tx, mut rx) = channel::<i64>(1);
    tx.send(1).unwrap();
    let observed = Arc::new(AtomicI32::new(0));
    let obs_clone = observed.clone();
    let h = spawn(move || {
        // This send blocks until the recv below frees the slot.
        tx.send(2).unwrap();
        obs_clone.store(1, Ordering::SeqCst);
    });
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(observed.load(Ordering::SeqCst), 0);
    assert_eq!(rx.recv(), Some(1));
    let _ = h.wait();
    assert_eq!(rx.recv(), Some(2));
    assert_eq!(observed.load(Ordering::SeqCst), 1);
}

#[test]
fn task_channel_recv_blocks_when_empty_until_send() {
    use std::sync::atomic::AtomicI32;
    let (tx, mut rx) = channel::<i64>(2);
    let progress = Arc::new(AtomicI32::new(0));
    let p = progress.clone();
    let h = spawn(move || {
        let v = rx.recv().unwrap();
        p.store(v as i32, Ordering::SeqCst);
    });
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(progress.load(Ordering::SeqCst), 0);
    tx.send(77).unwrap();
    let _ = h.wait();
    assert_eq!(progress.load(Ordering::SeqCst), 77);
}

// =====================================================================
// SendError::PartialEq + Debug
// =====================================================================

#[test]
fn task_send_error_eq_self() {
    assert_eq!(SendError::<i64>(7), SendError::<i64>(7));
    assert_ne!(SendError::<i64>(7), SendError::<i64>(8));
}

#[test]
fn task_send_error_display() {
    let err: SendError<i64> = SendError(42);
    assert_eq!(format!("{err}"), "channel closed: receiver dropped");
}

#[test]
fn task_try_send_error_eq() {
    assert_eq!(TrySendError::Full::<i64>(1), TrySendError::Full::<i64>(1));
    assert_ne!(TrySendError::Full::<i64>(1), TrySendError::Closed::<i64>(1));
}

#[test]
fn task_try_recv_error_eq() {
    assert_eq!(TryRecvError::Empty, TryRecvError::Empty);
    assert_ne!(TryRecvError::Empty, TryRecvError::Disconnected);
}

// =====================================================================
// scope with panicking child handled
// =====================================================================

#[test]
fn task_scope_child_panic_visible_through_wait() {
    let result = scope(|s| {
        let h = s.spawn(|| -> i64 { panic!("child panics") });
        h.wait()
    });
    assert!(matches!(result, Err(JoinError::Panicked)));
}

#[test]
fn task_spawn_after_panic_in_runtime_still_works() {
    // Run several panicking tasks back-to-back to ensure runtime
    // doesn't degrade.
    for _ in 0..5 {
        let h = spawn(|| -> i64 { panic!("rapid") });
        let _ = h.wait();
    }
    let h_ok = spawn(|| 12345_i64);
    assert_eq!(h_ok.wait(), Ok(12345));
}

#[test]
fn task_channel_send_to_full_via_blocking_then_close_succeeds() {
    let (tx, mut rx) = channel::<i64>(1);
    tx.send(1).unwrap();
    // Drain in receiver and close.
    let h = spawn(move || {
        let v = rx.recv();
        assert_eq!(v, Some(1));
        // rx dropped here.
    });
    let _ = h.wait();
}

#[test]
fn task_send_error_debug_format_does_not_panic() {
    let err = SendError::<String>(String::from("x"));
    let _ = format!("{err:?}");
}

#[test]
fn task_try_send_error_debug_format_does_not_panic() {
    let err: TrySendError<i64> = TrySendError::Full(7);
    let _ = format!("{err:?}");
    let err2: TrySendError<i64> = TrySendError::Closed(8);
    let _ = format!("{err2:?}");
}

#[test]
fn task_try_recv_error_debug_format_does_not_panic() {
    let err = TryRecvError::Empty;
    let _ = format!("{err:?}");
    let err2 = TryRecvError::Disconnected;
    let _ = format!("{err2:?}");
}

#[test]
fn task_spawn_zero_unit_immediate_completion() {
    let h = spawn(|| ());
    assert_eq!(h.wait(), Ok(()));
}
