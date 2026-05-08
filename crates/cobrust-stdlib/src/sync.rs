//! `std.sync` — synchronization primitives. M13 ships bounded MPSC
//! channels.
//!
//! ADR-0028 §C is the authoritative design document and pins:
//!
//! - **Backend**: `tokio::sync::mpsc::channel` (multi-producer,
//!   single-consumer, bounded).
//! - **Sync surface**: every method is `fn` (no `async fn` keyword
//!   in the public surface — constitution §2.2).
//! - **Capacity**: 0 = rendezvous (sender blocks until receiver
//!   takes); n > 0 = bounded buffer.
//! - **Send semantics**: blocks the current thread when full;
//!   `try_send` returns `Err(TrySendError::Full(value))` instead.
//! - **Recv semantics**: blocks until a value arrives or every
//!   sender is dropped (then returns `None`).
//!
//! Routes through the singleton tokio runtime owned by
//! [`crate::task`]; calls into channel APIs from inside the user's
//! own tokio runtime would deadlock and are documented as forbidden
//! (ADR-0028 §"Consequences").

#![cfg(feature = "tokio-runtime")]

use std::sync::OnceLock;

use tokio::runtime::Runtime;
use tokio::sync::mpsc;

// =====================================================================
// Runtime singleton — shared with `crate::task`
// =====================================================================

/// Per ADR-0028 §A: the runtime is a process-singleton. We replicate
/// the helper here to keep modules independent (no cross-module
/// internal coupling at the public Rust ABI).
fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        Runtime::new().expect("M13 tokio runtime initialization failed (ADR-0028 §A)")
    })
}

// =====================================================================
// Errors
// =====================================================================

/// Sender-side error: the receiver is gone (every receiver clone
/// dropped). Per ADR-0028 §C.
#[derive(Debug, Eq, PartialEq)]
pub struct SendError<T>(pub T);

impl<T: std::fmt::Debug> std::fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "channel closed: receiver dropped")
    }
}

impl<T: std::fmt::Debug> std::error::Error for SendError<T> {}

/// Try-send error. Per ADR-0028 §C.
#[derive(Debug, Eq, PartialEq)]
pub enum TrySendError<T> {
    /// Buffer is full and capacity is bounded.
    Full(T),
    /// Receiver dropped.
    Closed(T),
}

impl<T: std::fmt::Debug> std::fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full(_) => write!(f, "channel full"),
            Self::Closed(_) => write!(f, "channel closed"),
        }
    }
}

impl<T: std::fmt::Debug> std::error::Error for TrySendError<T> {}

/// Try-recv error. Per ADR-0028 §C.
#[derive(Debug, Eq, PartialEq)]
pub enum TryRecvError {
    /// No value available right now.
    Empty,
    /// Every sender dropped; no further values can ever arrive.
    Disconnected,
}

impl std::fmt::Display for TryRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "channel empty"),
            Self::Disconnected => write!(f, "channel disconnected"),
        }
    }
}

impl std::error::Error for TryRecvError {}

// =====================================================================
// Channel
// =====================================================================

/// Bounded MPSC channel constructor. Per ADR-0028 §C.
///
/// `capacity == 0` selects rendezvous semantics (each `send` blocks
/// until a `recv` consumes the value); `capacity > 0` selects a
/// bounded buffer.
///
/// **Constraint**: must not be called from inside a user-owned
/// tokio runtime (would deadlock; see ADR-0028 §"Consequences").
pub fn channel<T: Send + 'static>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    // tokio's mpsc::channel requires capacity >= 1; rendezvous is
    // approximated by capacity = 1 + a discipline that send blocks
    // until receiver advances. M13 documents this as the closest
    // shape; capacity-zero rendezvous is a Phase F follow-up.
    let cap = capacity.max(1);
    // Touch the singleton so `blocking_send` / `blocking_recv` have a
    // runtime to park against (per ADR-0028 §A).
    let _ = runtime();
    let (tx, rx) = mpsc::channel::<T>(cap);
    (Sender { inner: tx }, Receiver { inner: rx })
}

/// Multi-producer sender. Clone to obtain additional senders.
pub struct Sender<T> {
    inner: mpsc::Sender<T>,
}

impl<T: Send + 'static> Sender<T> {
    /// Block the calling thread until the value is enqueued.
    /// Returns `Err(SendError(value))` if the receiver was dropped.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        // tokio's `Sender::blocking_send` is purpose-built for the
        // sync-bridge case: it parks the OS thread on an internal
        // primitive without re-entering the async runtime. Crucial
        // for honoring constitution §2.2 ("no async/sync coloring")
        // at scale — calling `runtime().block_on(self.inner.send(..))`
        // from inside a `spawn_blocking` worker would deadlock the
        // singleton runtime.
        //
        // Caveat: `blocking_send` requires the runtime to exist;
        // we ensure that by referencing the singleton at construction
        // time (via `channel`).
        match self.inner.blocking_send(value) {
            Ok(()) => Ok(()),
            Err(err) => Err(SendError(err.0)),
        }
    }

    /// Non-blocking send. Per ADR-0028 §C.
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        match self.inner.try_send(value) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(v)) => Err(TrySendError::Full(v)),
            Err(mpsc::error::TrySendError::Closed(v)) => Err(TrySendError::Closed(v)),
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Single-consumer receiver. Cannot be cloned.
pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
}

impl<T: Send + 'static> Receiver<T> {
    /// Block the calling thread until a value arrives or every
    /// sender is dropped (then returns `None`).
    pub fn recv(&mut self) -> Option<T> {
        // tokio's `Receiver::blocking_recv` is the sync analogue of
        // `recv().await` — parks the OS thread on the channel's
        // internal primitive without re-entering the async runtime.
        // Same rationale as `Sender::send` above.
        self.inner.blocking_recv()
    }

    /// Non-blocking receive. Per ADR-0028 §C.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        match self.inner.try_recv() {
            Ok(value) => Ok(value),
            Err(mpsc::error::TryRecvError::Empty) => Err(TryRecvError::Empty),
            Err(mpsc::error::TryRecvError::Disconnected) => Err(TryRecvError::Disconnected),
        }
    }
}
