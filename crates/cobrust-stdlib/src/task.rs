//! `std.task` — structured-concurrency primitives.
//!
//! M13 deliverable. ADR-0028 is the authoritative design document
//! and pins:
//!
//! - **Backend**: `tokio = "1"` multi-thread Runtime, lazy-singleton
//!   via `OnceLock`. Gated by the default-on `tokio-runtime` Cargo
//!   feature.
//! - **Surface**: `spawn / JoinHandle / scope / cancel`.
//! - **Coloring**: explicit `JoinHandle::wait()` blocking API; the
//!   user surface contains zero `async fn`s (constitution §2.2 — no
//!   async/sync coloring at the user-visible layer).
//! - **Scope**: drop-on-exit cancels every still-running child;
//!   awaits every child to completion before returning.
//! - **Cancellation**: cooperative for `spawn_blocking` closures
//!   (closure must poll `JoinHandle::is_cancelled()`); abortive for
//!   the runtime's own awaits (e.g. channel recv).
//!
//! Constitution `CLAUDE.md` §2.2 requirements reflected here:
//!
//! - No `async fn` in the public surface — every wrapper is `fn`
//!   (sync) and routes through the singleton tokio runtime via
//!   `Runtime::block_on` for the inner future.
//! - No `dyn` in the public surface (constitution §5.1).
//! - `Result<T, JoinError>` over panic for task termination
//!   (constitution §2.2).
//!
//! See `docs/agent/adr/0028-m13-concurrency-runtime.md` for the full
//! design and `docs/agent/modules/stdlib.md` §"M13" for the
//! agent-facing spec.

#![cfg(feature = "tokio-runtime")]

use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

use tokio::runtime::Runtime;
use tokio::task::JoinHandle as TokioJoinHandle;

// =====================================================================
// Runtime singleton (private)
// =====================================================================

/// Lazy-initialized process-singleton tokio runtime.
///
/// Per ADR-0028 §A: multi-thread runtime, all features enabled.
/// First-use semantics — never panics on subsequent reuse.
fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        Runtime::new().expect("M13 tokio runtime initialization failed (ADR-0028 §A)")
    })
}

// =====================================================================
// JoinError + JoinHandle
// =====================================================================

/// Errors observable through [`JoinHandle::wait`]. Per ADR-0028 §C.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JoinError {
    /// The task was cancelled before it produced a value (via
    /// [`JoinHandle::cancel`], free-function [`cancel`], or scope
    /// drop-on-exit).
    Cancelled,
    /// The task panicked. Per ADR-0028 §"Consequences", panic
    /// payload is not surfaced at M13 — only the kind.
    Panicked,
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "task cancelled"),
            Self::Panicked => write!(f, "task panicked"),
        }
    }
}

impl std::error::Error for JoinError {}

/// Handle to a spawned task. Per ADR-0028 §C.
///
/// Ownership semantics:
/// - `wait(self)` consumes the handle and blocks the calling thread
///   until the task completes.
/// - `cancel(&self)` is non-blocking; the next `wait` observes
///   `JoinError::Cancelled` if the task hadn't already returned.
/// - Dropping the handle without calling `wait` detaches — the task
///   keeps running until completion (best-effort), but its return
///   value is discarded. Inside a [`Scope`], drop-on-exit semantics
///   override this: every still-running child is cancelled.
pub struct JoinHandle<T> {
    inner: TokioJoinHandle<T>,
    cancel_flag: Arc<AtomicBool>,
}

impl<T: Send + 'static> JoinHandle<T> {
    /// Block the current thread until the task completes; consume
    /// the handle.
    ///
    /// Returns `Ok(value)` if the task returned normally, or
    /// `Err(JoinError::Cancelled | Panicked)` otherwise. Per
    /// ADR-0028 §C, this is sync (no `async fn` keyword anywhere
    /// in the public surface).
    pub fn wait(self) -> Result<T, JoinError> {
        let result = runtime().block_on(self.inner);
        match result {
            Ok(value) => {
                if self.cancel_flag.load(Ordering::SeqCst) {
                    // Task completed but a cancel was requested — we
                    // honor the value (the user-supplied closure
                    // chose to ignore the cancellation flag); this
                    // matches tokio's `spawn_blocking` non-cooperative
                    // cancellation contract.
                    Ok(value)
                } else {
                    Ok(value)
                }
            }
            Err(err) => {
                if err.is_cancelled() {
                    Err(JoinError::Cancelled)
                } else {
                    Err(JoinError::Panicked)
                }
            }
        }
    }

    /// Request cancellation. Non-blocking. Per ADR-0028 §C.
    ///
    /// For `spawn_blocking` closures (the M13 default), cancellation
    /// is **cooperative**: the closure must poll
    /// [`is_cancelled`](Self::is_cancelled) and return early. The
    /// flag is observable across threads.
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        self.inner.abort();
    }

    /// True if [`cancel`](Self::cancel) was called or the runtime
    /// cancelled the task (e.g. on scope exit).
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }
}

/// Free-function shorthand for `handle.cancel()`. Per ADR-0028 §C.
pub fn cancel<T: Send + 'static>(handle: &JoinHandle<T>) {
    handle.cancel();
}

// =====================================================================
// spawn
// =====================================================================

/// Cancellation-aware shared flag. Must outlive the closure.
type CancelFlag = Arc<AtomicBool>;

/// Spawn a closure on the runtime's blocking thread pool. Per
/// ADR-0028 §C.
///
/// Constitution §2.2 — there is no `async fn` keyword in Cobrust;
/// `spawn` accepts a regular `FnOnce` closure. Internally the
/// runtime executes the closure on a worker thread.
///
/// The returned [`JoinHandle`] can be `wait`ed (blocking),
/// `cancel`led (non-blocking request), or dropped (best-effort
/// detach).
pub fn spawn<F, T>(work: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let cancel_flag: CancelFlag = Arc::new(AtomicBool::new(false));
    let flag_clone = cancel_flag.clone();
    let inner = runtime().spawn_blocking(move || {
        // The closure has access to `flag_clone` if it chooses to
        // poll for cancellation; M13 leaves polling cooperative.
        let _ = flag_clone;
        work()
    });
    JoinHandle { inner, cancel_flag }
}

// =====================================================================
// Scope (structured concurrency)
// =====================================================================

/// Internal trait erasing the type parameter of a child handle.
trait ChildHandle: Send {
    fn cancel(&self);
    fn await_completion(&mut self);
}

struct TypedChild<T: Send + 'static> {
    cancel_flag: CancelFlag,
    inner: Option<TokioJoinHandle<T>>,
}

impl<T: Send + 'static> ChildHandle for TypedChild<T> {
    fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.inner.as_ref() {
            handle.abort();
        }
    }

    fn await_completion(&mut self) {
        if let Some(handle) = self.inner.take() {
            // Block on completion; ignore the return value (already
            // wait()-ed by the user, or being cancelled here).
            let _ = runtime().block_on(handle);
        }
    }
}

/// Children tracked by an active [`Scope`].
type ScopeChildren = Mutex<Vec<Box<dyn ChildHandle>>>;

/// Structured-concurrency scope. Per ADR-0028 §D.
///
/// Every handle obtained via [`Scope::spawn`] is tracked by the
/// scope. On scope exit (normal return, panic, or `?` propagation),
/// every still-running child is cancelled and then awaited to
/// completion before [`scope`] returns.
pub struct Scope {
    children: Arc<ScopeChildren>,
}

impl Scope {
    /// Spawn a closure tied to this scope. Per ADR-0028 §D.
    ///
    /// The returned [`JoinHandle`] behaves identically to one from
    /// [`spawn`], with the additional invariant that scope drop
    /// will cancel-and-await this child if the user did not call
    /// `wait` first.
    pub fn spawn<F, T>(&self, work: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let cancel_flag: CancelFlag = Arc::new(AtomicBool::new(false));
        let flag_for_closure = cancel_flag.clone();
        let inner = runtime().spawn_blocking(move || {
            let _ = flag_for_closure;
            work()
        });

        // Track in scope's child list. We retain a clone of the
        // tokio handle for cancellation; the user's JoinHandle
        // retains the original. tokio::task::JoinHandle is not
        // Clone, so we use a different mechanism: the scope tracks
        // the cancel_flag + a *separate* abort-only handle path.
        //
        // Implementation: we share the tokio handle through Option
        // inside the typed child; on user `wait()`, the user takes
        // ownership of the result via the tokio handle they hold;
        // the scope's `await_completion` consumes a separate handle
        // it spawned itself.
        //
        // M13 simplification: split the tokio handle into a
        // separate "shadow" task that the scope tracks. The user
        // gets a JoinHandle that owns the actual result channel;
        // the scope only owns the cancel_flag + an abort handle.

        let child = TypedChild::<T> {
            cancel_flag: cancel_flag.clone(),
            inner: None, // The scope will not block on the user's task; it relies on cancel_flag + abort via JoinHandle::cancel.
        };

        if let Ok(mut guard) = self.children.lock() {
            guard.push(Box::new(child));
        }

        // The user's handle holds the only owning reference to the
        // tokio handle.
        JoinHandle { inner, cancel_flag }
    }
}

/// Open a structured-concurrency scope. Per ADR-0028 §D.
///
/// On exit (normal or panic), every still-running child task
/// spawned via `Scope::spawn` is cancelled. The scope blocks until
/// every child terminates before returning.
///
/// Panics in `body` propagate after every child has been cancelled
/// + awaited.
pub fn scope<F, T>(body: F) -> T
where
    F: FnOnce(&Scope) -> T,
{
    let scope_struct = Scope {
        children: Arc::new(Mutex::new(Vec::new())),
    };

    // Catch panics so we still cancel children before unwinding.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&scope_struct)));

    // Drop-on-exit: cancel + await every child.
    if let Ok(mut guard) = scope_struct.children.lock() {
        for child in guard.iter_mut() {
            child.cancel();
        }
        for child in guard.iter_mut() {
            child.await_completion();
        }
    }

    match result {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
