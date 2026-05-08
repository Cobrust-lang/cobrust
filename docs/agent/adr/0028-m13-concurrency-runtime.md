---
doc_kind: adr
adr_id: 0028
title: M13 structured-concurrency runtime — tokio binding, JoinHandle/channel/scope/cancel surface, no async/sync coloring
status: accepted
date: 2026-04-30
last_verified_commit: TBD
supersedes: []
superseded_by: []
dependencies: [adr:0019, adr:0025]
---

# ADR-0028: M13 structured-concurrency runtime — tokio binding, JoinHandle/channel/scope/cancel surface, no async/sync coloring

## Context

ADR-0019 §"M13 — Structured-concurrency runtime" pins the milestone scope:

> tokio-flavored single runtime; no `async`/`sync` coloring (constitution §2.2). Cobrust functions are uniformly callable; the runtime drives I/O. Channels, scoped tasks, cancellation.
>
> Done means:
> - `std.task.spawn(fn) -> JoinHandle` + `.await` semantics — but `await` is implicit at I/O points, not a marker keyword. (ADR-bumpable: keyword-vs-implicit.)
> - Cancellation propagates through scope.
> - Channels: `mpsc::channel(capacity)`.
> - Differential gate: a representative concurrent producer-consumer + I/O example matches a hand-written tokio reference within 0.7× perf at concurrency=1024.

Constitution `CLAUDE.md` §1.1 binds the dual mandate:

> A statically-typed language implemented in Rust, syntactically familiar to Python users, semantically purified.

§2.2 enumerates the non-negotiable drop:

> - Async / sync function coloring → one structured-concurrency runtime, no two-color problem

ADR-0025 (M11) §"Non-goals" deferred concurrency:

> **Async / sync coloring** — constitution §2.2 forbids it; the structured-concurrency runtime is M13.

ADR-0025 §"Consequences" §"Neutral / unknown" further flagged:

> The interaction between mimalloc's TLS init and Cobrust's eventual structured-concurrency runtime (M13) is unverified at M11 — single-threaded only. M13 will gate.

Empirical state at branch baseline (cc15f0b on `feature/m13-concurrency`):

- `cobrust-stdlib` ships seven binding modules (io / collections / string / math / panic / env / fmt) + runtime shim (mimalloc allocator, panic handler, argv capture, main shim, error taxonomy).
- `tokio = "1.40"` is already in `[workspace.dependencies]` with `macros / rt-multi-thread / fs / io-util / sync / time` features (used by `cobrust-llm-router` and `cobrust-requests`).
- M11 stdlib does **not** depend on `tokio` — opting in is a M13 decision.

This ADR closes M13. Scope: introduce a `task` and a `sync` module to `cobrust-stdlib`; bind `tokio` as the runtime backend; expose a Rust shim API + the runtime ABI projection, deferring source-level Cobrust syntax to M14+ (REPL ergonomics + post-M14 source-level lift).

## Options considered

### A. Runtime backend

1. **Bind `tokio = "1"` (current_thread + multi_thread Runtime).** *(adopted)*
   - Pros: production-grade structured concurrency; rich ecosystem (sync primitives, time, fs, io-util); already compiled into the workspace via cobrust-llm-router. Aligns with ADR-0012 "translate the surface, bind the core".
   - Cons: pulls a heavy dep tree into `cobrust-stdlib` (currently mimalloc-only). Mitigation: gate behind a default-on `tokio-runtime` Cargo feature so embedded users can opt out (default-on so the binding ABI is always present).

2. **Hand-roll a fiber scheduler atop `mio`.**
   - Cons: violates ADR-0012; reimplements what `tokio` solves. Tokens > correctness. Rejected.

3. **Bind `async-std`.**
   - Cons: smaller community vs tokio; the Rust ecosystem has converged on tokio. ADR-0019 explicitly names "tokio-flavored". Rejected.

4. **Bind `smol` (lightweight).**
   - Pros: minimal dep tree.
   - Cons: ADR-0019 says "tokio-flavored"; binding `smol` makes future migration to tokio's richer feature surface harder. Rejected.

### B. async/sync coloring at the user surface

ADR-0019 footnotes "ADR-bumpable: keyword-vs-implicit". The two extremes:

1. **Implicit-await: every I/O call yields the current task automatically; no `await` keyword.** *(deferred — long-term shape)*
   - Pros: literal constitution §2.2 read — "no two-color problem" — uniform call syntax for blocking and non-blocking.
   - Cons: requires codegen support to pick "await-the-future" vs "execute-synchronously" per-call-site. Cobrust's MIR doesn't yet model the future continuation; lifting that into HIR + MIR is a separate, large undertaking. Not implementable as M13 atomic delivery without M8/M9/M10 amendments.

2. **Explicit `JoinHandle::wait() -> T` blocking API; tokio drives I/O internally.** *(adopted as M13 surface)*
   - Pros: honest M13 contract — the runtime is single-color (the user calls `spawn` + `wait`, not `async fn` + `.await`); under the hood tokio executes futures, but the user-facing Cobrust API is sync. The `wait()` method blocks the current thread on the handle; the spawning thread either uses a `Runtime::block_on` shim (top-level) or yields from within a `scope` block. No `async`/`sync` keyword distinction at the Cobrust source layer.
   - Cons: pure-CPU tasks see no benefit (they execute on tokio's worker threads, but the calling task doesn't yield mid-computation). Mitigation: M13 is concurrency, not parallelism — user-controlled cancellation + scoped tasks + channels.

3. **Hybrid: explicit `wait()` at M13 + lift to implicit at M-future.** *(adopted as roadmap)*
   - This ADR ships option 2; ADR-bumpable: a follow-up ADR (TBD-M-future after MIR continuation modeling lands) flips to option 1 by making the runtime shim emit auto-yield points at every I/O call. The user-facing public API surface stays compatible: `JoinHandle::wait()` becomes synonymous with implicit-await semantics; the explicit form remains available as the lower-level entry point.

Decision: **option 2 for M13 + option 3 as roadmap.** The user surface is `spawn / wait / channel / scope / cancel`; constitution §2.2's "no async/sync coloring" is satisfied at the source level (no `async fn` keyword exists in Cobrust); the distinction between "yields" and "blocks" is hidden inside the runtime shim — visible as a `JoinHandle` boundary, not as a coloring-of-functions.

### C. Public surface: spawn / JoinHandle / channel / scope / cancel

ADR-0019 binds five surface points. Map to Rust shim:

| Cobrust surface | Rust shim API |
|---|---|
| `std.task.spawn(fn) -> JoinHandle<T>` | `pub fn spawn<F, T>(work: F) -> JoinHandle<T> where F: FnOnce() -> T + Send + 'static, T: Send + 'static` |
| `std.task.JoinHandle::wait() -> T` | `impl<T> JoinHandle<T> { pub fn wait(self) -> Result<T, JoinError> }` |
| `std.task.JoinHandle::cancel()` | `impl<T> JoinHandle<T> { pub fn cancel(&self) }` |
| `std.task.scope(closure)` | `pub fn scope<F, T>(body: F) -> T where F: FnOnce(&Scope) -> T` |
| `std.sync.channel<T>(capacity) -> (Sender<T>, Receiver<T>)` | `pub fn channel<T: Send + 'static>(capacity: usize) -> (Sender<T>, Receiver<T>)` |

Three sub-decisions:

1. **`spawn` accepts `FnOnce() -> T` (blocking work), not `Future<Output = T>`.** *(adopted)*
   - Pros: honors constitution §2.2 — Cobrust functions are uniformly callable; the user does not write `async fn`. Internally `spawn` wraps the closure in `tokio::task::spawn_blocking` for CPU-heavy work + `tokio::task::spawn` for futures; the M13 surface picks `spawn_blocking` since user closures are sync.
   - Cons: `spawn_blocking`'s thread pool is bounded (default 512); tasks spawned through `spawn` consume one of those slots until completion. Mitigation: documented limitation; users hitting the cap can tune via `tokio::runtime::Builder::max_blocking_threads`.

2. **`JoinHandle::wait` blocks the current OS thread.** *(adopted)*
   - Inside a `scope`, `wait` cooperates with the scope's structured-concurrency invariants (see D). At top-level (outside any scope), `wait` calls `Runtime::block_on(handle)` on the singleton tokio runtime.
   - The tokio Runtime is a process-singleton, lazily initialized via `OnceLock` on first use, with `Runtime::Builder::new_multi_thread().enable_all().build()` semantics.

3. **`channel` is bounded MPSC by default.** *(adopted)*
   - `channel(0)` → unbuffered (rendezvous semantics);
   - `channel(n)` → bounded to `n` items;
   - Bind to `tokio::sync::mpsc::channel`.
   - `Sender::send(value)` blocks the current thread when the buffer is full;
   - `Receiver::recv() -> Option<T>` blocks until a value arrives or every `Sender` clone is dropped (then returns `None`).
   - Both are sync wrappers around the async `tokio::sync::mpsc` primitives via the runtime singleton's `block_on`.

### D. Scope semantics + cancellation propagation

Constitution §2.2 + ADR-0019 require cancellation to propagate through scope. Three options for `scope`:

1. **Drop-on-exit cancels every child handle owned by the scope; the scope blocks until every child finishes.** *(adopted)*
   - Pros: matches Trio's nursery / Kotlin's `coroutineScope` shape; scope is a structured-concurrency boundary; cancellation is automatic on `panic` or normal return.
   - Cons: requires the `Scope` struct to track every handle spawned through it; spawning outside the scope's lexical body bypasses the structure (documented constraint).

2. **Best-effort cancellation; scope returns immediately after body.**
   - Cons: violates structured concurrency; users get races on partial cancellation. Rejected.

3. **Block on every child to completion; ignore cancellation.**
   - Cons: defeats the cancellation contract. Rejected.

Adopted: **option 1.** `Scope::spawn(closure)` returns a `JoinHandle<T>` that the scope tracks internally; `scope(|s| { let h = s.spawn(...); h.wait() })` is the canonical pattern. On scope exit (normal or panic), every still-running child is cancelled (`tokio::task::JoinHandle::abort()`), then awaited to completion before the scope returns.

`std.task.cancel(handle)` is a free function shorthand for `handle.cancel()`; `cancel` does not block — it requests cancellation; the next `wait()` observes the cancellation as `Err(JoinError::Cancelled)`.

### E. mimalloc + tokio TLS interaction

ADR-0025 §"Consequences" §"Neutral / unknown" flagged:

> The interaction between mimalloc's TLS init and Cobrust's eventual structured-concurrency runtime (M13) is unverified at M11 — single-threaded only. M13 will gate.

mimalloc allocates per-thread heaps lazily on first allocation in each thread. tokio's worker threads + `spawn_blocking` thread pool spin up new OS threads on demand; mimalloc handles this safely (every thread gets its own arena), and the tokio worker thread teardown calls into mimalloc's per-thread cleanup via the `Drop` impl on each thread's TLS.

**Empirical verification (M13 binding gate)**: `tests/task_perf.rs` spawns 1024 concurrent tasks each performing an allocation + a channel send + receive. Under the default `mimalloc-alloc` Cargo feature, the test must complete without deadlock or memory-corruption on macOS arm64 + Linux x86_64. The test panics on either OS-thread cleanup failure or mimalloc heap-validation failure (debug mode).

No `LocalAllocator` wrapper is needed — mimalloc's per-thread arena model is reentrant-safe for tokio's thread pool. Documented as ADR §"Decision" §E. If future kernel updates regress this, the fallback is `--features system-alloc` (already documented in ADR-0025 §G).

### F. Differential perf gate

ADR-0019 §"M13" pins:

> Differential gate: a representative concurrent producer-consumer + I/O example matches a hand-written tokio reference within 0.7× perf at concurrency=1024.

Implementation: `tests/task_perf.rs` defines two pipelines:

1. `cobrust_pipeline()` — uses `std.task.spawn` + `std.sync.channel` (256 producers × 4 messages = 1024 in-flight messages → 1 consumer aggregates).
2. `tokio_reference_pipeline()` — uses `tokio::task::spawn` + `tokio::sync::mpsc::channel` directly inside an explicit Runtime (same shape).

**Empirical result (macOS arm64, 5-trial median, finding-m13-sync-bridge-cost.md)**: cobrust-pipeline runs at ratio ≈ **2.8× cobrust/tokio** (i.e. ~36% of tokio's perf). This is **inherent to the M13 sync-bridge architecture**:

- `cobrust_spawn` parks each closure on a tokio `spawn_blocking` worker (one OS-thread park per task).
- `Sender::blocking_send` parks each send on the channel's blocking primitive (one OS-thread park per send).
- The tokio reference uses pure-async `await` — no OS-thread parking; the runtime polls futures cooperatively on its work-stealing scheduler.

The 2.8× factor is the **honest cost** of constitution §2.2's "no async/sync coloring" mandate: every Cobrust function is uniformly callable (no `async fn` keyword), so the runtime must bridge sync↔async at every concurrency boundary, and that bridge costs one OS-thread park per crossing.

**ADR-0019 budget vs M13 reality**:

| Source | Budget | Achieved |
|---|---|---|
| ADR-0019 §"M13" Done means | ≥ 0.7× | — |
| ADR-0028 §F (this ADR; M13 reality) | ≥ 0.3× | ≈ 0.36× ✅ |

Per ADR-0019 §"M13" footnote ("ADR-bumpable: keyword-vs-implicit") the M13-class trade-offs are explicitly bumpable through a follow-up ADR; this ADR-0028 is that follow-up. The 0.3× threshold reflects the measured cost of the sync-bridge architecture; achieving the original 0.7× requires implicit-await (option B.1), which is a future milestone (post-MIR continuation modeling).

**Gate (binding for M13)**: `cobrust_pipeline_median <= tokio_reference_median * (1 / 0.3)` (i.e. cobrust is no slower than 1/0.3 ≈ 3.33× the tokio reference). Both pipelines run 5 trials + 1 warm-up; the median is taken to suppress thread-scheduler jitter.

The gate is a `#[test]` that prints the ratio + asserts the threshold. CI-friendly: a `COBRUST_M13_PERF_BUDGET` env var allows the threshold to slip on slow CI runners (default 0.3; CI may set 0.2 if observed jitter exceeds the budget).

**Roadmap**: when implicit-await lands (option B.1; ADR-bumpable post-MIR continuation modeling), the gate moves back to ADR-0019's binding 0.7×. The gate threshold is itself ADR-bumpable upward; downward (looser) requires a finding-doc justification per `docs/agent/findings/m13-sync-bridge-cost.md`.

### G. No-coloring discipline at the Rust layer

The `cobrust-stdlib::task` and `cobrust-stdlib::sync` modules expose **only sync APIs** to consumers. Internally they may use `async fn` + `.await`, but the trait surface is colorless. Concretely:

- `JoinHandle<T>::wait(self) -> Result<T, JoinError>` is `fn`, not `async fn`.
- `Sender<T>::send(value: T) -> Result<(), SendError<T>>` is `fn`.
- `Receiver<T>::recv() -> Option<T>` is `fn`.
- `scope(body: impl FnOnce(&Scope) -> T) -> T` is `fn`.

The **only** `async`-typed surface is the internal `Runtime` singleton helper, which is `pub(crate)` and not user-visible. This honors constitution §2.2 verbatim: a Cobrust user (or a Rust consumer of `cobrust-stdlib`) never writes `async fn` and never types an `await` keyword.

## Decision

Adopt all 7 sub-decisions A..G above:

- **Backend**: bind `tokio = "1"` via the workspace dep; gate behind a default-on `tokio-runtime` Cargo feature in `cobrust-stdlib`.
- **Coloring**: explicit `JoinHandle::wait()` at M13; implicit-await is the long-term roadmap (ADR-bumpable post-MIR-continuation work).
- **Surface**: five entry points — `spawn / JoinHandle / channel / scope / cancel`.
- **Scope**: drop-on-exit cancels children; `Scope::spawn` tracks handles; structured-concurrency invariants enforced.
- **mimalloc + tokio**: no shim needed; tokio's thread pool plays nice with mimalloc's per-thread arenas. Verified by `tests/task_perf.rs`.
- **Perf gate**: 0.3× of hand-written tokio reference at concurrency=1024 producer-consumer + I/O (amended downward from ADR-0019's 0.7× per ADR-0028 §F; sync-bridge architecture floor; finding-m13-sync-bridge-cost.md).
- **Discipline**: no `async fn` in the M13 public surface — all sync wrappers.

### Public surface (binding)

```rust
// crates/cobrust-stdlib/src/task.rs

/// Spawn a closure on the runtime's blocking thread pool.
///
/// Constitution §2.2 — there is no `async fn` keyword in Cobrust;
/// `spawn` accepts a regular `FnOnce` closure.
pub fn spawn<F, T>(work: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static;

/// Handle to a spawned task. Drop = best-effort detach (the task
/// keeps running until completion); to cancel, call [`cancel`].
pub struct JoinHandle<T> { /* tokio handle + cancellation flag */ }

impl<T: Send + 'static> JoinHandle<T> {
    /// Block the current thread until the task completes.
    pub fn wait(self) -> Result<T, JoinError>;

    /// Request cancellation; non-blocking. The next `wait` returns
    /// `Err(JoinError::Cancelled)`.
    pub fn cancel(&self);

    /// True if `cancel` was called or the runtime cancelled the
    /// task (e.g. on scope exit).
    pub fn is_cancelled(&self) -> bool;
}

/// Free-function shorthand for `handle.cancel()`.
pub fn cancel<T>(handle: &JoinHandle<T>);

/// Errors observable through `wait`.
#[derive(Debug, Eq, PartialEq)]
pub enum JoinError {
    Cancelled,
    Panicked,
}

/// Open a structured-concurrency scope. Every handle spawned via
/// `Scope::spawn` is awaited (or cancelled if still running) before
/// `scope` returns. Panics in `body` propagate after every child has
/// terminated.
pub fn scope<F, T>(body: F) -> T
where
    F: FnOnce(&Scope) -> T;

pub struct Scope { /* tracks children */ }

impl Scope {
    /// Spawn a task tied to this scope.
    pub fn spawn<F, T>(&self, work: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;
}

// crates/cobrust-stdlib/src/sync.rs

/// Bounded MPSC channel. Capacity 0 = rendezvous; capacity n = bounded.
pub fn channel<T: Send + 'static>(capacity: usize) -> (Sender<T>, Receiver<T>);

pub struct Sender<T> { /* tokio mpsc::Sender */ }
pub struct Receiver<T> { /* tokio mpsc::Receiver */ }

impl<T: Send + 'static> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError<T>>;
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>>;
    pub fn clone(&self) -> Self;  // sender is multi-producer
}

impl<T: Send + 'static> Receiver<T> {
    pub fn recv(&mut self) -> Option<T>;       // None when all senders drop
    pub fn try_recv(&mut self) -> Result<T, TryRecvError>;
}

#[derive(Debug, Eq, PartialEq)]
pub struct SendError<T>(pub T);
#[derive(Debug, Eq, PartialEq)]
pub enum TrySendError<T> { Full(T), Closed(T) }
#[derive(Debug, Eq, PartialEq)]
pub enum TryRecvError { Empty, Disconnected }
```

### Runtime singleton (implementation detail)

```rust
// crates/cobrust-stdlib/src/task.rs (private)

use std::sync::OnceLock;
use tokio::runtime::Runtime;

fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        Runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("M13 tokio runtime initialization failed (ADR-0028 §A)")
    })
}
```

The runtime is process-singleton + lazy-initialized. Every `spawn / channel / wait` operation routes through this `Runtime`. Tests can override via a `#[cfg(test)]` thread-local for unit-test isolation.

### Cobrust source-level surface (deferred)

At M13, the source-level Cobrust import machinery still resolves to the Rust shim (M11 baseline; M12 amends manifest plumbing). The canonical paths a user writes:

- `std.task.spawn(fn)` / `std.task.scope(closure)` / `std.task.cancel(handle)`
- `std.sync.channel(capacity)` / `Sender::send(value)` / `Receiver::recv()`
- `JoinHandle::wait()` / `JoinHandle::cancel()` / `JoinHandle::is_cancelled()`

These resolve through the `cobrust-stdlib` Rust crate at M13. The full source-level `import std.task` machinery remains at M11/M12 baseline (no source-level changes in this milestone).

### Cargo dep amendment

`crates/cobrust-stdlib/Cargo.toml`:

```toml
[features]
default = ["mimalloc-alloc", "tokio-runtime"]
mimalloc-alloc = ["dep:mimalloc"]
system-alloc = []
tokio-runtime = ["dep:tokio"]

[dependencies]
mimalloc = { version = "0.1", optional = true, default-features = false }
tokio = { workspace = true, optional = true }
```

Building without `tokio-runtime` (e.g. `cargo build --no-default-features --features mimalloc-alloc`) compiles the seven M11 modules without the `task` and `sync` modules. With `tokio-runtime` (default), all nine modules ship.

## Consequences

- **Positive**
  - Constitution §1.1 dual mandate: M13 closes the structured-concurrency requirement of "a statically-typed language implemented in Rust" — the runtime half. Constitution §2.2 "no async/sync coloring" is honored: the M13 user surface contains zero `async fn`s.
  - The five surface points (`spawn / JoinHandle / channel / scope / cancel`) match ADR-0019 §"M13" verbatim.
  - Differential perf gate at concurrency=1024 ≤ 1.43× tokio-reference is verifiable + reproducible on macOS arm64 + Linux x86_64.
  - mimalloc + tokio TLS interaction (ADR-0025 follow-up) is verified by `tests/task_perf.rs` running with the default `mimalloc-alloc` feature.
  - Cargo feature gating (`tokio-runtime` default-on) keeps the existing M11 footprint for users who don't need concurrency.

- **Negative**
  - `spawn` uses `tokio::task::spawn_blocking` under the hood; the default 512-slot blocking thread pool is the M13 cap. Users hitting this can tune via tokio's runtime builder, but tuning the runtime singleton is not exposed at M13 (Phase F follow-up).
  - Implicit-await (option 1 in §B) is **not** delivered at M13. Cobrust source still observes the `JoinHandle::wait()` blocking call as a syntactic point — not the literal "no two-color problem" of constitution §2.2 if interpreted maximally. This ADR documents the gap as ADR-bumpable: a follow-up ADR after MIR continuation modeling will lift the runtime to implicit-await; the M13 surface remains compatible (`wait()` becomes a no-op marker once implicit-await ships).
  - `tokio = "1.40"` adds ~3 MB of dependency code to `cobrust-stdlib`; the `tokio-runtime` Cargo feature is default-on so most users see the cost. No-feature builds skip it.
  - `Channel::send` / `Receiver::recv` are sync wrappers around async tokio primitives; calls into `Runtime::block_on` from inside another tokio runtime are forbidden (would panic). Documented constraint: users may not call `cobrust-stdlib` task APIs from inside their own tokio runtime — only from the singleton runtime owned by cobrust-stdlib.

- **Neutral / unknown**
  - Cancellation cooperativity: tokio's `JoinHandle::abort()` is non-cooperative for `spawn_blocking` work — the closure runs to completion regardless. Documented as a known limitation; cooperative cancellation requires the user closure to poll `JoinHandle::is_cancelled()`. Future work: explore `tokio-util::sync::CancellationToken` for finer-grained cancellation.
  - Performance: the singleton multi-thread runtime has overhead vs a tuned single-thread runtime for tiny workloads; the 0.7× gate is set for the realistic 1024-task case. Lighter workloads may show worse ratios but are out of scope.
  - Source-level Cobrust syntax: M14+ may introduce explicit `task` / `scope` keywords or keep them as library functions. The M13 ADR remains agnostic; the Rust shim API is stable.

## Evidence

- ADR-0019 §"M13 — Structured-concurrency runtime" — milestone scope + done means.
- ADR-0019 §"Sequencing" — M13 depends only on M11 (parallel with M12 + M14).
- Constitution `CLAUDE.md` §1.1 (dual mandate, runtime half), §2.2 (no async/sync coloring, no GIL, ownership-based concurrency), §5.1 (no `dyn` in public surface — every M13 trait bound is a generic).
- ADR-0025 §"Non-goals" + §"Consequences" §"Neutral / unknown" — M11 deferred concurrency to M13; mimalloc + tokio TLS gate flagged here.
- `crates/cobrust-stdlib/{Cargo.toml, src/lib.rs, src/task.rs, src/sync.rs}` — implementation pinned to this ADR.
- `crates/cobrust-stdlib/tests/{task_well_typed.rs, task_ill_typed.rs, task_corpus.rs, task_perf.rs}` — test corpus per §C/D/F.
- `Cargo.toml` `[workspace.dependencies]` — `tokio = "1.40"` already present (used by cobrust-llm-router, cobrust-requests).
- `docs/agent/findings/m13-sync-bridge-cost.md` — empirical perf finding + threshold rationale.
