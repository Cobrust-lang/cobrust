//! C-ABI shims — the runtime surface a compiled `.cb` program binds
//! onto when it does `import dora` and calls `dora.Node("detector")`,
//! `dora.node(handler)`, `node.run()`, `node.shutdown()`,
//! `event.id()`, `event.data_str()`.
//!
//! ADR-0076 Phase 1 — ninth ecosystem-module proof. Third module on the
//! ADR-0073 cross-boundary callback chain (after pit + hood). Phase 1
//! is intentionally SYNTHETIC: `__cobrust_dora_node_run` mocks one canned
//! message arrival without depending on the real dora-rs daemon. Same
//! pattern as F65's synthetic-LLM provider — the chain is proven without
//! the real infra wired.
//!
//! # `dora-real` feature — #146 Phase A (the synthetic → real swap)
//!
//! The DEFAULT build keeps the synthetic trampoline above. Building with
//! `--features dora-real` swaps the L4 runtime body from the canned-event
//! trampoline to a REAL `dora_node_api::DoraNode` + a blocking
//! `events.recv()` loop firing the `.cb` callback once per real
//! `Event::Input` (dora-real-integration-plan §9 spike-proven; mirrors how
//! `coil` gates `faer` behind `coil-faer`). The **C-ABI symbol surface is
//! IDENTICAL across both builds** — the 11 `#[unsafe(no_mangle)] extern
//! "C"` shim signatures below are single-definition; only the private
//! `*_impl` bodies + the two handle-struct shapes are
//! `#[cfg]`-split (`real` / `synthetic` submodules). So the ecosystem
//! manifest (`cobrust-types`), the MIR retarget, and the codegen
//! `Constant::FnRef` callback never change — this is a `cabi.rs`-local
//! body swap, NOT a compiler change (the spike's load-bearing insight).
//!
//! Under `dora-real`, `__cobrust_dora_node_new` calls
//! `DoraNode::init_from_env()` (the daemon-spawned path; the dora
//! `integration_testing` mode — driven by `DORA_TEST_WITH_INPUTS` — makes
//! this run hermetically with NO daemon, which the F36-honest E2E uses for
//! a real round-trip) and stashes the live `(DoraNode, EventStream)` in
//! the Node handle; `node.run()` drains the REAL `EventStream` firing the
//! callback per `Event::Input`; `event.data_str()` decodes the real
//! `arrow::array::ArrayRef` payload; `event.send_output(id, payload)`
//! publishes a real Arrow `StringArray` on the node's output port via the
//! ambient-node handle (plan §4.4 option 1 — the live `DoraNode` is reached
//! through the Node handle pointer the trampoline threads into a
//! thread-local for the callback's duration). `node.shutdown()` /
//! `node.run()` honor `Event::Stop` / a `None` recv (stream-closed) by
//! breaking the loop. Real-dora is NATIVE-ONLY: tokio-net hard-fails on
//! wasm32 (§9), so the wasm dora story stays synthetic-default.
//!
//! # The chain
//!
//! These `#[no_mangle] extern "C"` shims are the L4 (runtime) layer of
//! the ecosystem-import chain. The Cobrust toolchain (L1 typecheck → L2
//! MIR intrinsic-rewrite → L3 codegen externs) retargets the source
//! calls onto these exact symbols; `cobrust build` static-links the
//! resulting `libdora.a` after `libcobrust_stdlib.a`.
//!
//! # ABI
//!
//! - **Node handle** crosses as an opaque `*mut u8`, `Box::into_raw`'d
//!   on construction and `Box::from_raw`'d exactly once at the `.cb`
//!   scope-exit drop via `__cobrust_dora_node_drop`. Owns the registered
//!   handler closure + the synthetic event queue.
//! - **Event handle** is Rust-owned (ADR-0073 §2 D6, mirrors pit's
//!   Request): the trampoline `Box::into_raw`'s a fresh Event before
//!   invoking the `.cb` callback and `Box::from_raw`'s it back on
//!   callback return. The `.cb` side NEVER drops an Event — the
//!   manifest's `handle_drop_symbol(DORA_EVENT_ADT)` returns `None`.
//! - **Strings** cross as Cobrust `Str` buffers. Per ADR-0072 Q5 this
//!   crate has **no Rust-level dependency on `cobrust-stdlib`** in the
//!   production build; the `__cobrust_str_*` primitives are declared
//!   `extern "C"` here and resolved from the always-linked
//!   `libcobrust_stdlib.a`. (For in-crate unit tests, `cobrust-stdlib`
//!   is a dev-dependency so the `extern "C"` decls resolve under
//!   `cargo test`.)
//! - **Callbacks** cross as a raw C-ABI fn-pointer
//!   `unsafe extern "C" fn(*mut u8) -> *mut u8` (ADR-0073 §5.1 — ONE
//!   callback shape across pit + hood + dora). The dora handler shape
//!   at the source is `fn(event: dora.Event) -> i64`; the wire takes a
//!   `*mut u8` Event pointer (the trampoline allocates + frees the
//!   Event box) and returns a `*mut u8` whose low bits are the i64
//!   return — but since the trampoline currently discards the return
//!   (the handler's side-effect IS the intent for Phase 1, mirroring
//!   hood), the wire-level return is just discarded.
//!
//! # Trampoline soundness (ADR-0073 §5 risk 1, same as pit + hood)
//!
//! - `Send + Sync + Copy` for an `extern "C" fn(*mut u8) -> *mut u8` is
//!   the Rust blanket impl. The captured closure holds only the fn
//!   pointer — no `Rc` / `RefCell` / non-Send state — so it inherits
//!   `Send + Sync` trivially.
//! - `'static` is satisfied because the `.cb` fn lives in the binary's
//!   text segment for the entire process lifetime under AOT compilation.
//!   Dynamic-loaded modules would invalidate this claim — explicitly out
//!   of scope for v0.7.0 (ADR-0073 §5 risk 1).
//! - **Abort-on-panic across the C boundary** (ADR-0073 §3 Q5): a panic
//!   in the `.cb` handler would unwind through the C ABI which is UB.
//!   We wrap every callback invocation in `std::panic::catch_unwind` and
//!   on panic abort the process.
//!
//! # Handler registration + multi-IO model (Phase 1 + Phase 2)
//!
//! Phase 1 ships the explicit registration form `dora.node(handler)` as a
//! module-level free fn taking a callback. The function stores the fn
//! pointer in a process-global slot ([`REGISTERED_HANDLER`]; Phase 1
//! supports a single handler). When `node.run()` fires with NO declared
//! inputs, it reads the global slot, invokes the handler ONCE with a canned
//! `("camera", "frame_001")` Event, and returns 0 (the proven Phase-1
//! single-input path — `dora_hello_e2e`).
//!
//! ADR-0076 Phase 2 adds MULTI-IO via the `@dora.node(inputs=[...],
//! outputs=[...])` decorator desugar. The desugar (cobrust-hir) threads
//! each declared port id to this trampoline as a `dora.declare_input(id)` /
//! `dora.declare_output(id)` register-call emitted at main's prologue
//! BEFORE `dora.node(handler)`:
//!
//! - [`__cobrust_dora_declare_input`] pushes an input id onto the
//!   process-global [`DECLARED_INPUTS`] queue.
//! - [`__cobrust_dora_declare_output`] pushes an output id onto the
//!   process-global [`DECLARED_OUTPUTS`] set.
//! - When `node.run()` fires with a NON-EMPTY [`DECLARED_INPUTS`] queue, it
//!   injects ONE canned event PER declared input id (each `event.id()`
//!   returns its input id; the payload is a canned per-input Str), invoking
//!   the handler once per input — the multi-input dispatch contract.
//! - [`__cobrust_dora_event_send_output`] lets the handler emit a Str
//!   payload on a declared output port. It validates the output id against
//!   [`DECLARED_OUTPUTS`] (an UNDECLARED id is a clear `eprintln!` + `-1`
//!   return, NOT a silent drop) and CAPTURES the emission to stdout as
//!   `output[<id>]=<payload>` so the synthetic E2E can assert it.
//!
//! Phase 2 stays SYNTHETIC (no real zenoh broker / dora-rs daemon — that is
//! a later phase). Arrow list/dict payloads (ADR-0076c), the dora-yaml
//! config path, and the real zenoh runtime are DEFERRED; the canned-payload
//! Str model carries the multi-IO proof.

// C-ABI-boundary cast allows — mirror `cobrust-pit/src/cabi.rs`'s
// crate-level allows (the casts are intrinsic to the opaque-pointer /
// length ABI and are correct here):
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_ptr_alignment)]

use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

// =====================================================================
// Cobrust Str-buffer ABI — declared here, resolved from
// libcobrust_stdlib.a at link time (ADR-0072 Q5; no Rust dep).
// =====================================================================

unsafe extern "C" {
    /// Allocate a fresh empty Cobrust `Str` buffer.
    fn __cobrust_str_new() -> *mut u8;
    /// Append `len` UTF-8 bytes at `ptr` to the buffer.
    fn __cobrust_str_push_static(buf: *mut u8, ptr: *const u8, len: i64);
    /// Borrow the buffer's byte pointer (valid until the next mutation).
    fn __cobrust_str_ptr(buf: *mut u8) -> *const u8;
    /// The buffer's byte length.
    fn __cobrust_str_len(buf: *mut u8) -> i64;
}

/// Read a Cobrust `Str` buffer pointer into an owned `String`. Tolerates
/// null / empty.
///
/// # Safety
///
/// `buf` must be null or a valid Cobrust `Str` buffer produced by
/// `__cobrust_str_new`.
#[allow(dead_code)] // read_str_buf is used by the Node constructor (name argument)
unsafe fn read_str_buf(buf: *mut u8) -> String {
    if buf.is_null() {
        return String::new();
    }
    // SAFETY: caller attests `buf` is a valid Cobrust Str buffer.
    unsafe {
        let ptr = __cobrust_str_ptr(buf);
        let len = __cobrust_str_len(buf);
        if ptr.is_null() || len <= 0 {
            return String::new();
        }
        let bytes = std::slice::from_raw_parts(ptr, len as usize);
        std::str::from_utf8(bytes).unwrap_or("").to_string()
    }
}

/// Allocate a fresh Cobrust `Str` buffer carrying `s`'s bytes. Used by
/// `__cobrust_dora_event_id` and `__cobrust_dora_event_data_str` to
/// materialise event fields as `.cb`-owned Str buffers, and by the
/// in-crate cabi tests.
fn alloc_str_buffer(s: &str) -> *mut u8 {
    // SAFETY: `__cobrust_str_new` returns a valid buffer;
    // `__cobrust_str_push_static` copies `s` into it.
    unsafe {
        let buf = __cobrust_str_new();
        if !s.is_empty() {
            __cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        }
        buf
    }
}

// =====================================================================
// Drop instrumentation (ADR-0073 §5 done-means 5 — drop-once evidence).
// =====================================================================

/// Total `Node` handle drops performed by `_drop` shim this process.
/// Read by the test suite to assert no-leak / no-double-free.
///
/// Event boxes the trampoline creates per-callback-invocation are
/// dropped via plain `Box::from_raw` in the trampoline (Rust-owned per
/// ADR-0073 §2 D6) and are NOT counted here — the counter measures
/// `.cb`-scheduled drops.
pub static DROP_COUNT: AtomicU64 = AtomicU64::new(0);

/// Current `DROP_COUNT`. Test-only accessor.
#[must_use]
pub fn drop_count() -> u64 {
    DROP_COUNT.load(Ordering::SeqCst)
}

// =====================================================================
// dora C-ABI surface — Node + Event handle definitions.
// =====================================================================

/// The fixed C-ABI shape every `.cb` dora handler exposes (ADR-0073 §5.1).
/// The `.cb` source's `fn detect(event: dora.Event) -> i64:` compiles
/// to a fn with this exact ABI: it accepts a Boxed Event pointer (the
/// trampoline's job to allocate + free) and returns a `*mut u8` whose
/// low bits Phase 1 discards (the handler's side-effect IS the intent;
/// mirrors hood's `fn() -> i64` Phase-1 shape).
type CbHandlerAbi = unsafe extern "C" fn(*mut u8) -> *mut u8;

/// Process-global handler slot (single handler per process).
/// `dora.node(handler)` installs into this slot; `node.run()` reads it
/// and dispatches the canned event(s).
///
/// `AtomicPtr<()>` for `Send + Sync` across the synthetic-runtime
/// boundary; the pointer value IS a `CbHandlerAbi` fn pointer (raw fn
/// pointers `Copy + Send + Sync` so the transmute is sound).
static REGISTERED_HANDLER: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// ADR-0076 Phase 2 — process-global queue of DECLARED input ids the
/// `@dora.node(inputs=[...])` decorator threaded here via
/// `dora.declare_input(id)` register-calls (one per id, in source order).
/// `node.run()` injects one canned Event per id in this queue (multi-input
/// dispatch). EMPTY ⇒ the trampoline falls back to the single canned
/// `("camera", "frame_001")` event (Phase-1 single-input behavior).
static DECLARED_INPUTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// ADR-0076 Phase 2 — process-global set of DECLARED output ids the
/// `@dora.node(outputs=[...])` decorator threaded here via
/// `dora.declare_output(id)` register-calls. `event.send_output(id, ...)`
/// validates `id` against this set: an UNDECLARED id is rejected with a
/// clear stderr diagnostic + a `-1` return (NOT a silent drop). Stored as a
/// `Vec` (declared sets are tiny — a handful of ports) for `no_std`-free
/// `const`-initialisable `Mutex` without pulling a `HashSet` ctor.
static DECLARED_OUTPUTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// ADR-0076 Phase 2 — count of `send_output` emissions captured this
/// process (across all declared output ports). Read by the cabi unit tests
/// to assert the capture path fired; the synthetic E2E asserts the stdout
/// `output[<id>]=<payload>` marker instead.
pub static SEND_OUTPUT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Current [`SEND_OUTPUT_COUNT`]. Test-only accessor.
#[must_use]
pub fn send_output_count() -> u64 {
    SEND_OUTPUT_COUNT.load(Ordering::SeqCst)
}

/// Runtime form of a `Node` handle the `.cb` source owns.
///
/// **SYNTHETIC build** (`not(feature = "dora-real")`): captures only the
/// node name; the registered handler lives in the process-global
/// [`REGISTERED_HANDLER`] slot.
///
/// **REAL build** (`feature = "dora-real"`): additionally owns the live
/// `(DoraNode, EventStream)` returned by `DoraNode::init_from_env()` plus
/// the tokio runtime the event loop runs inside — see the
/// [`real`] submodule. `shutdown_called` is read by both builds.
struct DoraNodeHandle {
    /// User-supplied node identifier (e.g. `"detector"`). Under the
    /// synthetic `run` loop it is unused at runtime — preserved so a
    /// future `node.id() -> str` follow-up reads it without re-allocating
    /// the Node. Under the real path it is informational.
    _name: String,
    /// Whether `shutdown()` has been invoked on this Node. The synthetic
    /// shutdown is a soft flag; under the real path it also drops the live
    /// `DoraNode` early (signalling the dora coordinator).
    shutdown_called: bool,
    /// REAL build only — the live dora node + its event stream + the tokio
    /// runtime guard. `None` when `init_from_env()` failed (the run shim
    /// then surfaces the `-1` sentinel rather than aborting). Kept off the
    /// synthetic build entirely so the default has zero real-dora state.
    #[cfg(feature = "dora-real")]
    real: Option<real::RealNode>,
}

/// Runtime form of an Event the trampoline allocates per-callback
/// invocation. Mirrors pit's Request shape — the .cb side sees a `*mut u8`
/// Adt handle and calls borrow-shim methods (`event.id()` /
/// `event.data_str()`) that materialise fresh Cobrust Str buffers from
/// these fields.
///
/// Both builds carry the decoded `id` + `data_str` strings so the borrow
/// shims are build-agnostic. Under the REAL build the trampoline decodes
/// the real `arrow::array::ArrayRef` payload into `data_str` before boxing
/// the Event (so `event.data_str()` returns the REAL wire payload, not a
/// canned string — the load-bearing real-vs-synthetic delta).
struct DoraEventHandle {
    /// Input id this event arrived on (e.g. `"camera"`).
    id: String,
    /// Payload bytes as a UTF-8 string. Synthetic build: the canned
    /// per-input Str. Real build: the decoded Arrow payload (a Utf8
    /// `StringArray` element, or a debug rendering of a non-string array).
    /// Phase B widens this to a structured Arrow / `coil.Buffer` surface
    /// (sub-ADR 0076c).
    data_str: String,
}

// =====================================================================
// dora C-ABI surface — Node lifecycle (constructor + drop).
// =====================================================================

/// `dora.Node(name: str) -> Node`. Construct a synthetic Node handle.
/// The `.cb` caller owns the handle; scope-exit drops it via
/// `__cobrust_dora_node_drop`.
///
/// # Safety
///
/// `name` must be null or a valid Cobrust `Str` buffer (see
/// [`read_str_buf`]).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_node_new(name: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let name_s = unsafe { read_str_buf(name) };
    // SYNTHETIC build: a name-only handle (the canned `run` loop needs no
    // live node). REAL build: also init a real `DoraNode` via
    // `init_from_env()` and stash `(DoraNode, EventStream)` for `run`.
    let handle = DoraNodeHandle {
        _name: name_s,
        shutdown_called: false,
        #[cfg(feature = "dora-real")]
        real: real::init_node(),
    };
    Box::into_raw(Box::new(handle)).cast::<u8>()
}

/// `dora.node(handler) -> i64` (ADR-0073 §5.1 — load-bearing callback
/// site). Phase 1's explicit-registration form: stores the fn pointer
/// in the process-global [`REGISTERED_HANDLER`] slot. Returns 0 (Ty::Int
/// sentinel — registration is a side-effect).
///
/// # Safety
///
/// `handler` must be a real C-ABI fn pointer (codegen guarantees this
/// for the type-checked top-level fn name path — ADR-0073 §2 D1 callback
/// gate ensures the source-level `fn(event: dora.Event) -> i64`
/// signature unifies with `dora_event_handler_fn_ty()`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_node_node(handler: *const c_void) -> i64 {
    if handler.is_null() {
        // Defense in depth — codegen materialises a real fn pointer for
        // a well-typed program; null is impossible under the typechecker
        // but we tolerate it as a no-op rather than UB.
        return 0;
    }
    // Store the raw fn pointer in the global slot. SeqCst so the Phase 1
    // single-threaded `run` immediately sees the stored handler.
    REGISTERED_HANDLER.store(handler as *mut (), Ordering::SeqCst);
    0
}

/// `dora.declare_input(id: str) -> i64` (ADR-0076 Phase 2). Pushes a
/// declared INPUT port id onto the process-global [`DECLARED_INPUTS`]
/// queue. The `@dora.node(inputs=[...])` decorator desugar emits one such
/// call per declared input (in source order) at main's prologue; `run`
/// then injects one canned Event per queued id. Returns 0 (Ty::Int
/// sentinel — declaration is a side-effect).
///
/// # Safety
///
/// `id` must be null or a valid Cobrust `Str` buffer (see [`read_str_buf`]).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_declare_input(id: *mut u8) -> i64 {
    // SAFETY: caller-attestation per `# Safety`.
    let id_s = unsafe { read_str_buf(id) };
    if let Ok(mut q) = DECLARED_INPUTS.lock() {
        q.push(id_s);
    }
    0
}

/// `dora.declare_output(id: str) -> i64` (ADR-0076 Phase 2). Pushes a
/// declared OUTPUT port id onto the process-global [`DECLARED_OUTPUTS`]
/// set. The `@dora.node(outputs=[...])` decorator desugar emits one such
/// call per declared output at main's prologue;
/// [`__cobrust_dora_event_send_output`] validates against this set.
/// Idempotent on a repeat id (a port declared twice is stored once).
/// Returns 0.
///
/// # Safety
///
/// `id` must be null or a valid Cobrust `Str` buffer (see [`read_str_buf`]).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_declare_output(id: *mut u8) -> i64 {
    // SAFETY: caller-attestation per `# Safety`.
    let id_s = unsafe { read_str_buf(id) };
    if let Ok(mut set) = DECLARED_OUTPUTS.lock()
        && !set.iter().any(|o| o == &id_s)
    {
        set.push(id_s);
    }
    0
}

/// `node.run() -> i64`. SYNTHETIC dispatcher. When [`DECLARED_INPUTS`] is
/// EMPTY (the Phase-1 explicit `dora.node(detect)` form), invokes the
/// registered handler exactly once with a canned `("camera", "frame_001")`
/// Event and returns 0. When [`DECLARED_INPUTS`] is NON-EMPTY (the Phase-2
/// `@dora.node(inputs=[...])` form), injects ONE canned Event per declared
/// input id — invoking the handler once per input (multi-input dispatch) —
/// and returns 0. Mirrors what a real dora-rs `EventStream` loop would do
/// across one tick per input; a later phase replaces this with the real
/// `DoraNode::events().into_iter()` driven loop over the zenoh broker.
///
/// # Safety
///
/// `node` must be a live Node handle from `__cobrust_dora_node_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_node_run(node: *mut u8) -> i64 {
    if node.is_null() {
        return -1;
    }
    // REAL build: drive the live `EventStream` from the node handle.
    // SYNTHETIC build: drive the canned-event trampoline. Both read the
    // process-global [`REGISTERED_HANDLER`] and fire the same callback ABI.
    // Exactly one `let ret` is active per build (cfg-mutually-exclusive),
    // so the tail `ret` is unambiguous + clippy-clean (no needless return).
    #[cfg(feature = "dora-real")]
    // SAFETY: caller attests `node` is a live handle from `node_new`.
    let ret = unsafe { real::run_node(node) };
    #[cfg(not(feature = "dora-real"))]
    // SAFETY: caller attests `node` is a live handle from `node_new`.
    let ret = unsafe { run_node_synthetic(node) };
    ret
}

/// The SYNTHETIC `node.run()` body — the canned-event trampoline. Unchanged
/// from the Phase-1/2 synthetic runtime; called from
/// `__cobrust_dora_node_run` under `not(feature = "dora-real")`. Kept as a
/// private inner fn (rather than inline in the shim) so the REAL path can be
/// `#[cfg]`-swapped without touching the exported shim signature.
///
/// # Safety
///
/// `node` must be a live, non-null Node handle from
/// `__cobrust_dora_node_new`.
#[cfg(not(feature = "dora-real"))]
unsafe fn run_node_synthetic(node: *mut u8) -> i64 {
    // Borrow the Node for the duration of the synthetic loop; not consumed.
    // SAFETY: caller per `# Safety`.
    let _handle: &DoraNodeHandle = unsafe { &*node.cast::<DoraNodeHandle>() };

    // Read the global handler slot.
    let raw_ptr = REGISTERED_HANDLER.load(Ordering::SeqCst);
    if raw_ptr.is_null() {
        // No handler registered — Phase 1 fail-clean (no UB, no abort).
        // The .cb source would have called `dora.node(handler)` before
        // `node.run()`; a missing registration is a user bug we surface
        // via the -1 sentinel.
        return -1;
    }
    // SAFETY: `raw_ptr` was stored by `__cobrust_dora_node_node` which
    // guarantees it's a real `CbHandlerAbi` fn pointer (codegen emits
    // `Constant::FnRef` only for a top-level fn name whose `FnTy` was
    // unified with `dora_event_handler_fn_ty()` — ADR-0073 §2 D1).
    let raw: CbHandlerAbi = unsafe { std::mem::transmute::<*mut (), CbHandlerAbi>(raw_ptr) };

    // Build the canned event QUEUE. ADR-0076 Phase 2: one `(id, payload)`
    // per DECLARED input id (the decorator threaded them via
    // `dora.declare_input`), preserving source-declaration order so the
    // handler dispatches on `event.id()` deterministically. When NO inputs
    // were declared (the Phase-1 explicit `dora.node(detect)` form), fall
    // back to the single canned `("camera", "frame_001")` tick — the
    // smallest input that proves the chain, keeping `dora_hello_e2e` green.
    let declared: Vec<String> = DECLARED_INPUTS
        .lock()
        .map(|q| q.clone())
        .unwrap_or_default();
    let events: Vec<(String, String)> = if declared.is_empty() {
        vec![("camera".to_string(), "frame_001".to_string())]
    } else {
        declared
            .into_iter()
            .map(|id| {
                let payload = canned_payload_for(&id);
                (id, payload)
            })
            .collect()
    };

    // Fire the handler once per canned event.
    for (id, data_str) in events {
        let event = DoraEventHandle { id, data_str };
        // SAFETY: `raw` is a valid handler; the shared dispatcher boxes +
        // frees the Event and aborts on a callback panic per ADR-0073 §3 Q5.
        unsafe { fire_callback(raw, event) };
    }
    // Discard the handler return-pointer(s) (mirrors hood's "side-effect IS
    // the intent" pattern). Surface the manifest-declared 0 sentinel so the
    // .cb source's `let _ = node.run()` discards a clean i64.
    0
}

/// Shared callback-dispatch core for ONE event — used by BOTH the synthetic
/// trampoline and the real `EventStream` loop so the box / `catch_unwind` /
/// abort-on-panic / free discipline is single-sourced (ADR-0073 §2 D6 +
/// §3 Q5). Boxes `event` into an opaque `*mut u8` the `.cb` handler
/// receives, invokes the handler under `catch_unwind`, frees the box
/// exactly once, and **aborts the process** if the callback panicked
/// (unwinding through the C ABI is UB).
///
/// # Safety
///
/// `raw` must be a valid `CbHandlerAbi` fn pointer (codegen guarantees this
/// for the type-checked callback path).
unsafe fn fire_callback(raw: CbHandlerAbi, event: DoraEventHandle) {
    // Box the Event so the .cb handler receives an opaque *mut u8
    // Adt-pointer (ADR-0073 §2 D6 — Rust owns the box). The trampoline owns
    // the Box for the callback invocation and frees it on return.
    let event_raw = Box::into_raw(Box::new(event)).cast::<u8>();

    // Catch panics across the C ABI (ADR-0073 §3 Q5).
    let ret_raw = std::panic::catch_unwind(|| {
        // SAFETY: `raw` is a valid `CbHandlerAbi`; `event_raw` is a valid
        // Boxed Event pointer just constructed.
        unsafe { raw(event_raw) }
    });

    // Free the Event box exactly once on the way out. The .cb source NEVER
    // drops a dora.Event local (manifest `handle_drop_symbol` returns None
    // for DORA_EVENT_ADT — mirrors pit.Request).
    // SAFETY: `event_raw` was just `Box::into_raw`'d above; reclaim and drop.
    unsafe { drop(Box::from_raw(event_raw.cast::<DoraEventHandle>())) };

    // Err arm = panic crossed the C ABI; abort per ADR-0073 §3 Q5.
    if ret_raw.is_err() {
        eprintln!(
            "cobrust-dora: panic in .cb handler crossed the C ABI — aborting (ADR-0073 §3 Q5)"
        );
        std::process::abort();
    }
}

/// The canned payload Str for a declared input id (ADR-0076 Phase 2
/// synthetic trampoline). The `camera` input keeps the Phase-1 canonical
/// `"frame_001"` payload (so `event.data_str()` is stable across the
/// single-input no-regression path); every other input id gets a distinct
/// non-empty `"frame_<id>"` canned Str so the handler can tell injected
/// events apart. A real broker replaces this with the actual Arrow payload.
///
/// Synthetic-build only — the `dora-real` path decodes the REAL Arrow
/// payload instead (`real::decode_arrow_payload`), so this canned helper is
/// `#[cfg]`-gated off there to keep `clippy -D warnings` clean.
#[cfg(not(feature = "dora-real"))]
fn canned_payload_for(id: &str) -> String {
    if id == "camera" {
        "frame_001".to_string()
    } else {
        format!("frame_{id}")
    }
}

/// `node.shutdown() -> i64`. Phase 1: idempotent soft flag (no real
/// signal to dora coordinator until Phase 2). Returns 0.
///
/// # Safety
///
/// `node` must be a live Node handle from `__cobrust_dora_node_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_node_shutdown(node: *mut u8) -> i64 {
    if node.is_null() {
        return -1;
    }
    // SAFETY: caller per `# Safety`.
    let handle: &mut DoraNodeHandle = unsafe { &mut *node.cast::<DoraNodeHandle>() };
    handle.shutdown_called = true;
    // REAL build: drop the live `DoraNode`/`EventStream` now so the dora
    // coordinator sees the node leave (idempotent — a second shutdown finds
    // `real` already `None`). The synthetic build has no live node to drop.
    #[cfg(feature = "dora-real")]
    {
        handle.real = None;
    }
    0
}

/// Drop a `Node` handle. `Box::from_raw` + drop, exactly once. Idempotent on null.
///
/// # Safety
///
/// `node` must be null or a `Node` handle from `__cobrust_dora_node_new`
/// that has not already been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_node_drop(node: *mut u8) {
    if node.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership.
    drop(unsafe { Box::from_raw(node.cast::<DoraNodeHandle>()) });
    DROP_COUNT.fetch_add(1, Ordering::SeqCst);
}

// =====================================================================
// dora C-ABI surface — Event borrow methods (F65 G1 pattern from pit).
// =====================================================================

/// `event.id() -> str`. Returns a freshly-allocated Cobrust `Str` buffer
/// carrying the input id (e.g. `"camera"`) the event arrived on. The
/// Rust Event is borrowed (NOT consumed); the trampoline owns the
/// `Box<Event>` and will free it on callback return per ADR-0073 §2 D6.
///
/// # Safety
///
/// `event` must be a valid Event handle the dora trampoline allocated
/// for the current callback invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_event_id(event: *mut u8) -> *mut u8 {
    if event.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller per `# Safety`. We only BORROW the Event — the
    // trampoline retains ownership of the Box and frees it after the
    // callback returns.
    let event_ref: &DoraEventHandle = unsafe { &*event.cast::<DoraEventHandle>() };
    alloc_str_buffer(&event_ref.id)
}

/// `event.data_str() -> str`. Returns a freshly-allocated Cobrust `Str`
/// buffer carrying the event payload bytes as a UTF-8 string. Phase 1
/// payload surface is Str-only; Phase 2 adds Arrow `RecordBatch`
/// accessors.
///
/// # Safety
///
/// Same as [`__cobrust_dora_event_id`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_event_data_str(event: *mut u8) -> *mut u8 {
    if event.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller per `# Safety`. Borrow-only.
    let event_ref: &DoraEventHandle = unsafe { &*event.cast::<DoraEventHandle>() };
    alloc_str_buffer(&event_ref.data_str)
}

/// `event.send_output(output_id: str, payload: str) -> i64` (ADR-0076
/// Phase 2). The handler emits a Str `payload` on the declared `output_id`
/// port. The synthetic trampoline:
///
/// 1. VALIDATES `output_id` against the process-global [`DECLARED_OUTPUTS`]
///    set (populated by `dora.declare_output` from the
///    `@dora.node(outputs=[...])` decorator). An UNDECLARED id is rejected
///    with a clear `eprintln!` diagnostic + a `-1` return — NOT a silent
///    drop (ADR-0076 §6 Phase 2 done-means 2; the typed compile-time
///    `DoraUnknownOutputId` reject is a tracked follow-up — Phase 2 catches
///    it at RUNTIME via this sentinel).
/// 2. CAPTURES the emission by printing `output[<id>]=<payload>` to stdout,
///    so the synthetic E2E can assert the output reached the runtime, and
///    bumps [`SEND_OUTPUT_COUNT`].
///
/// Returns 0 on a successful (declared) emission, `-1` on an undeclared
/// output id. The Event receiver is BORROWED (the trampoline owns the
/// `Box<Event>` and frees it on callback return per ADR-0073 §2 D6); the
/// `.cb` side's `let _ = event.send_output(...)` discards the i64 sentinel.
///
/// NOTE: when NO outputs were declared at all (e.g. a node calling
/// `send_output` with no `@dora.node(outputs=...)` decorator — not a shape
/// the Phase-2 corpus exercises), the [`DECLARED_OUTPUTS`] set is empty so
/// EVERY id is "undeclared" → `-1`. That is the honest fail-closed behavior;
/// a node that emits MUST declare its outputs.
///
/// # Safety
///
/// `event` must be a valid Event handle the dora trampoline allocated for
/// the current callback invocation; `output_id` / `payload` must be null or
/// valid Cobrust `Str` buffers (see [`read_str_buf`]).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_event_send_output(
    event: *mut u8,
    output_id: *mut u8,
    payload: *mut u8,
) -> i64 {
    // The Event is borrowed for symmetry with the other event shims (the
    // synthetic capture validates against the GLOBAL declared-output set,
    // not per-event state — but a real broker routes the send through the
    // Event's originating node, so the borrow models that future shape).
    // SAFETY: caller per `# Safety`. Borrow-only; tolerate null.
    if !event.is_null() {
        let _event_ref: &DoraEventHandle = unsafe { &*event.cast::<DoraEventHandle>() };
    }
    // SAFETY: caller-attestation per `# Safety`.
    let id_s = unsafe { read_str_buf(output_id) };
    let payload_s = unsafe { read_str_buf(payload) };

    // Validate against the declared-output set (BOTH builds — the
    // fail-closed contract is build-agnostic; the §2.5 compile-time-catch
    // follow-up is the typed `DoraUnknownOutputId` reject, Phase B).
    let declared = DECLARED_OUTPUTS
        .lock()
        .map(|set| set.iter().any(|o| o == &id_s))
        .unwrap_or(false);
    if !declared {
        eprintln!(
            "cobrust-dora: send_output on UNDECLARED output id {id_s:?} — declare it via \
             `@dora.node(outputs=[{id_s:?}])` (ADR-0076 Phase 2). Output dropped."
        );
        return -1;
    }

    // Count the emission on BOTH builds (the cabi unit tests read this).
    SEND_OUTPUT_COUNT.fetch_add(1, Ordering::SeqCst);

    // REAL build: publish a real Arrow `StringArray` on the node's output
    // port via the ambient live `DoraNode` (plan §4.4 option 1 — reached
    // through the thread-local Node handle the run loop installed for the
    // callback's duration). SYNTHETIC build: capture the emission by
    // printing the `output[<id>]=<payload>` marker the synthetic E2E asserts.
    // One `let ret` active per build → clippy-clean tail (no needless return).
    #[cfg(feature = "dora-real")]
    let ret = real::send_output(&id_s, &payload_s);
    #[cfg(not(feature = "dora-real"))]
    let ret = {
        println!("output[{id_s}]={payload_s}");
        0
    };
    ret
}

/// Drop an `Event` handle. Phase 1 currently never invoked from the .cb
/// side (Event is Rust-owned per ADR-0073 §2 D6 — manifest returns None
/// for DORA_EVENT_ADT's drop symbol). Exported for completeness +
/// trampoline symmetry with hood/pit; reserved for a future shape where
/// the .cb side might own an Event clone.
///
/// # Safety
///
/// `event` must be null or an Event handle from a Box::into_raw allocation
/// that has not already been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_event_drop(event: *mut u8) {
    if event.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership.
    drop(unsafe { Box::from_raw(event.cast::<DoraEventHandle>()) });
    // NOT counted in DROP_COUNT — Phase 1 the .cb side never owns Event.
    // If a future shape adds Event to the .cb drop schedule, add the
    // increment here to keep the instrument honest.
}

// =====================================================================
// REAL dora-node-api runtime — #146 Phase A (behind `feature = "dora-real"`).
//
// This submodule holds the REAL `DoraNode`-driven bodies the exported
// C-ABI shims delegate to under `--features dora-real`. The synthetic
// trampoline (above) is the DEFAULT; this is the opt-in real path
// (mirrors `coil`'s `coil-faer` gate). NOTHING here changes the exported
// symbol surface, the ecosystem manifest, or the codegen callback.
//
// dora-real-integration-plan §9 spike-proven: `libdora.a` with real
// dora-node-api 0.5.0 + tokio + arrow LINKS into a `.cb` Mach-O and RUNS;
// the one compiler-side change is the `-framework CoreFoundation` link flag
// (cobrust-cli/src/build.rs). The dora `integration_testing` mode (driven
// by `DORA_TEST_WITH_INPUTS`) lets `init_from_env()` run hermetically with
// NO daemon — the F36-honest E2E uses it for a genuine real round-trip.
// =====================================================================

#[cfg(feature = "dora-real")]
mod real {
    use super::{CbHandlerAbi, DoraEventHandle, DoraNodeHandle, REGISTERED_HANDLER, fire_callback};
    use std::cell::Cell;
    use std::sync::atomic::Ordering;

    use dora_node_api::dora_core::config::DataId;
    use dora_node_api::{ArrowData, DoraNode, Event, EventStream, IntoArrow, MetadataParameters};

    /// The live real-dora node state owned by a [`DoraNodeHandle`] under
    /// `feature = "dora-real"`. Holds the `DoraNode` (for `send_output`),
    /// the `EventStream` (drained by `run_node`), and the tokio runtime the
    /// node's internal tasks live in. The runtime is `enter()`-ed for the
    /// init + event-loop scopes (plan §3.2 — the canonical node enters a
    /// multi-thread runtime; for the hermetic `integration_testing` path no
    /// runtime is strictly required, but entering one is harmless and
    /// matches the real daemon-spawned shape).
    pub(super) struct RealNode {
        node: DoraNode,
        events: EventStream,
        // The multi-thread tokio runtime the node's background tasks live
        // in. `run_node` `.enter()`s it for the blocking recv/send window;
        // it is also held for the node's lifetime so the runtime stays up,
        // and dropped (shutting it down) when the Node handle drops.
        rt: tokio::runtime::Runtime,
    }

    thread_local! {
        /// Ambient live-node pointer for the duration of ONE callback
        /// invocation (plan §4.4 option 1). `run_node` sets this to
        /// `&mut self.node` immediately before firing the `.cb` handler and
        /// clears it on return, so the synchronous `event.send_output`
        /// shim — which only receives the Event pointer — can still reach
        /// the live `DoraNode` to publish. Single-threaded within the loop,
        /// so a thread-local raw pointer is sound (no aliasing: the borrow
        /// in `run_node` is released for the callback window).
        static AMBIENT_NODE: Cell<*mut DoraNode> = const { Cell::new(std::ptr::null_mut()) };
    }

    /// Build a multi-thread tokio runtime and initialise a REAL `DoraNode`
    /// via `DoraNode::init_from_env()` inside the runtime context. Returns
    /// `None` on any failure (e.g. no daemon env AND no testing env) so the
    /// `run` shim surfaces the `-1` sentinel rather than aborting across the
    /// C ABI — `node.run()` on a node that never initialised is a clean
    /// fail, not UB.
    pub(super) fn init_node() -> Option<RealNode> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .ok()?;
        let init = {
            let _guard = rt.enter();
            // `init_from_env()` returns the real node when the dora daemon
            // spawned us (DORA_NODE_CONFIG set) OR when the hermetic
            // integration-testing env (DORA_TEST_WITH_INPUTS) is set; it
            // falls back to interactive otherwise. We only keep the node on
            // a clean `Ok` — a hard error (e.g. malformed config) yields
            // `None` → the `-1` run sentinel.
            DoraNode::init_from_env()
        };
        match init {
            Ok((node, events)) => Some(RealNode { node, events, rt }),
            Err(e) => {
                eprintln!(
                    "cobrust-dora (dora-real): DoraNode::init_from_env() failed: {e:#} \
                     — node.run() will return -1 (no daemon / testing env?)"
                );
                None
            }
        }
    }

    /// REAL `node.run()` — drain the live `EventStream`, firing the `.cb`
    /// callback once per `Event::Input` with the DECODED Arrow payload, until
    /// `Event::Stop` or a `None` recv (stream closed). Returns 0 on a clean
    /// drain, `-1` if the node never initialised or no handler was
    /// registered (the same fail-clean sentinels as the synthetic path).
    ///
    /// # Safety
    ///
    /// `node` must be a live, non-null Node handle from
    /// `__cobrust_dora_node_new`.
    pub(super) unsafe fn run_node(node: *mut u8) -> i64 {
        // SAFETY: caller per `# Safety`. Mutable borrow for the loop —
        // `recv()` needs `&mut EventStream` and `send_output` needs
        // `&mut DoraNode`; both are distinct fields of `RealNode`.
        let handle: &mut DoraNodeHandle = unsafe { &mut *node.cast::<DoraNodeHandle>() };

        // Read the global handler slot (same contract as the synthetic path).
        let raw_ptr = REGISTERED_HANDLER.load(Ordering::SeqCst);
        if raw_ptr.is_null() {
            return -1;
        }
        // SAFETY: stored by `__cobrust_dora_node_node`, a real `CbHandlerAbi`.
        let raw: CbHandlerAbi = unsafe { std::mem::transmute::<*mut (), CbHandlerAbi>(raw_ptr) };

        let Some(real) = handle.real.as_mut() else {
            // `init_from_env()` failed in `node_new`; clean -1 (no UB).
            return -1;
        };
        // Split-borrow the distinct fields so the ambient `&mut node`, the
        // `&mut events` recv, AND the runtime-enter guard coexist without a
        // borrow conflict (entering `rt` borrows it while `node`/`events` are
        // borrowed mutably — destructuring once binds all three disjointly).
        let RealNode {
            node: dora_node,
            events,
            rt,
        } = real;
        // Enter the runtime for the blocking `recv()`/`send_output` window.
        let _guard = rt.enter();

        loop {
            // `recv()` blocks; `None` ⇒ the stream closed → end the loop.
            let Some(event) = events.recv() else {
                break;
            };
            match event {
                Event::Input { id, data, .. } => {
                    let id_s = id.as_str().to_string();
                    let data_str = decode_arrow_payload(&data);
                    let ev_handle = DoraEventHandle { id: id_s, data_str };

                    // Install the ambient live-node pointer for the callback
                    // window so `event.send_output` can publish, then clear.
                    AMBIENT_NODE.with(|cell| cell.set(std::ptr::from_mut::<DoraNode>(dora_node)));
                    // SAFETY: `raw` is a valid handler; `fire_callback` boxes
                    // + frees the Event and aborts on a callback panic.
                    unsafe { fire_callback(raw, ev_handle) };
                    AMBIENT_NODE.with(|cell| cell.set(std::ptr::null_mut()));
                }
                Event::Stop(_) => break,
                // Ignore unknown / non-input variants (Event is
                // `#[non_exhaustive]`; the dora docs say ignore unknowns).
                _ => {}
            }
        }
        0
    }

    /// REAL `event.send_output(id, payload)` — publish a length-1 Arrow
    /// `StringArray` on the node's `id` output port via the ambient live
    /// `DoraNode` (plan §4.4 option 1; Phase A scalar/str payload — the
    /// `coil.Buffer ↔ Arrow` widening is Phase B). The output-id validation
    /// against the declared set already happened in the calling shim.
    /// Returns 0 on a successful publish, `-1` if no ambient node is set
    /// (a `send_output` called outside a `run` callback) or the publish
    /// errored. The `MetadataParameters` are empty for Phase A (no metadata
    /// echo yet).
    pub(super) fn send_output(id: &str, payload: &str) -> i64 {
        let node_ptr = AMBIENT_NODE.with(Cell::get);
        if node_ptr.is_null() {
            eprintln!(
                "cobrust-dora (dora-real): send_output(\"{id}\", ...) called with no ambient \
                 node — it must run inside a node.run() callback. Output dropped."
            );
            return -1;
        }
        // SAFETY: `run_node` set `AMBIENT_NODE` to a valid `&mut DoraNode`
        // for the duration of THIS callback and clears it on return; we are
        // synchronously inside that window, single-threaded, so the pointer
        // is live and uniquely ours here.
        let node: &mut DoraNode = unsafe { &mut *node_ptr };
        let arr = payload.to_string().into_arrow();
        match node.send_output(DataId::from(id), MetadataParameters::default(), arr) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("cobrust-dora (dora-real): send_output(\"{id}\") failed: {e:#}");
                -1
            }
        }
    }

    /// Decode a real `arrow::array::ArrayRef` payload into a display string
    /// for `event.data_str()`. The Phase-A wire carries a Utf8 `StringArray`
    /// (the common hello-world payload) which decodes losslessly; any other
    /// arrow type falls back to a debug rendering so the shim never panics
    /// across the C ABI (Phase B adds structured `coil.Buffer` decode).
    fn decode_arrow_payload(data: &ArrowData) -> String {
        // `&ArrowData → String` succeeds for a length-1 Utf8 StringArray.
        if let Ok(s) = String::try_from(data) {
            return s;
        }
        // Fallback: a non-string / non-scalar array → a stable debug string
        // (e.g. a numeric array prints its values) so `data_str()` is always
        // well-defined. The array derefs to `arrow::array::ArrayRef`.
        format!("{:?}", **data)
    }
}

// These unit tests pin the SYNTHETIC trampoline contract (canned events,
// `output[id]=payload` capture, the registration/drop discipline). They
// assert behavior specific to the `not(feature = "dora-real")` build, so
// they are gated OFF under `dora-real` — where `node.run()` drives a REAL
// `EventStream` instead (no canned `frame_001`), and `node_new` calls
// `init_from_env()`. The real path's correctness is proven end-to-end +
// mutation-survivably by the F36-honest `dora_real_node_e2e.rs` (real
// symbols in the binary + a hermetic `integration_testing` round-trip),
// NOT by re-driving a real node from an in-process unit test. The shared
// scaffolding (Str ABI, drop instrument) is identical across both builds.
#[cfg(all(test, not(feature = "dora-real")))]
#[allow(clippy::undocumented_unsafe_blocks)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // The Str-buffer ABI is exported by cobrust-stdlib (a workspace
    // crate). For these unit tests we link it as a dev-dependency so the
    // `extern "C"` decls above resolve.
    extern crate cobrust_stdlib;
    #[used]
    static _STDLIB_LINK_ANCHOR: unsafe extern "C" fn() -> *mut u8 =
        cobrust_stdlib::fmt::__cobrust_str_new;

    /// Serialize the count-asserting tests to keep `DROP_COUNT`
    /// deltas + the global handler slot deterministic under cargo's
    /// default-parallel runner.
    static DROP_COUNTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // The Str drop shim from libcobrust_stdlib (used to free the
    // buffers we hand out under test).
    unsafe extern "C" {
        fn __cobrust_str_drop(buf: *mut u8);
    }

    /// Clear the process-global Phase-2 declared-IO slots + the handler
    /// slot. Every count-asserting test runs under `DROP_COUNTER_LOCK` and
    /// calls this first so a prior test's declared inputs/outputs don't
    /// bleed in (the single-canned-event path requires an EMPTY
    /// `DECLARED_INPUTS`).
    fn reset_dora_globals() {
        REGISTERED_HANDLER.store(std::ptr::null_mut(), Ordering::SeqCst);
        if let Ok(mut q) = DECLARED_INPUTS.lock() {
            q.clear();
        }
        if let Ok(mut s) = DECLARED_OUTPUTS.lock() {
            s.clear();
        }
    }

    /// Sentinel the test handler flips so we can confirm the trampoline
    /// really invoked it + observe the event payload through the borrow
    /// shims.
    static HANDLER_FIRED: AtomicU64 = AtomicU64::new(0);
    static OBSERVED_ID_PTR: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
    static OBSERVED_DATA_PTR: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());

    #[unsafe(no_mangle)]
    extern "C" fn _dora_test_handler(event: *mut u8) -> *mut u8 {
        HANDLER_FIRED.fetch_add(1, Ordering::SeqCst);
        // Exercise both borrow shims under the synthetic event. The
        // returned Str buffers are owned by the test (the trampoline
        // would normally schedule drops via the .cb drop pass — for the
        // test we stash them and free them in the asserting test below).
        unsafe {
            let id_buf = __cobrust_dora_event_id(event);
            let data_buf = __cobrust_dora_event_data_str(event);
            OBSERVED_ID_PTR.store(id_buf, Ordering::SeqCst);
            OBSERVED_DATA_PTR.store(data_buf, Ordering::SeqCst);
        }
        std::ptr::null_mut()
    }

    /// `dora.Node(...)` + `__cobrust_dora_node_drop` drop exactly once.
    #[test]
    fn node_new_then_drop_increments_counter_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let name = alloc_str_buffer("detector");
            let node = __cobrust_dora_node_new(name);
            assert!(!node.is_null(), "Node handle must be non-null");
            __cobrust_str_drop(name);
            __cobrust_dora_node_drop(node);
        }
        assert_eq!(drop_count() - before, 1, "Node must drop exactly once");
    }

    /// Null tolerance — every `_drop` is a no-op on null and never
    /// touches the counter.
    #[test]
    fn null_drops_are_no_ops() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            __cobrust_dora_node_drop(std::ptr::null_mut());
            __cobrust_dora_event_drop(std::ptr::null_mut());
        }
        assert_eq!(drop_count(), before, "null drops must be no-ops");
    }

    /// `run` without a registered handler yields the defensive -1 sentinel.
    #[test]
    fn run_without_registered_handler_returns_sentinel() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Reset the global slots to ensure deterministic state.
        reset_dora_globals();
        unsafe {
            let name = alloc_str_buffer("naked");
            let node = __cobrust_dora_node_new(name);
            __cobrust_str_drop(name);
            let ret = __cobrust_dora_node_run(node);
            assert_eq!(ret, -1, "run without handler must yield -1 sentinel");
            __cobrust_dora_node_drop(node);
        }
    }

    /// Register a handler then run — the registered fn pointer is invoked
    /// exactly once with the canned ("camera", "frame_001") Event, the
    /// borrow shims surface the expected strings, and the Node drops
    /// exactly once.
    #[test]
    fn trampoline_invokes_handler_with_canned_event_and_drops_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Reset the global slots + sentinels (empty DECLARED_INPUTS ⇒ the
        // single-canned-event Phase-1 path this test pins).
        reset_dora_globals();
        let before_fire = HANDLER_FIRED.load(Ordering::SeqCst);
        let before_drop = drop_count();
        unsafe {
            // Register the test handler via the explicit-form shim.
            let handler_ptr = _dora_test_handler as *const c_void;
            let reg_ret = __cobrust_dora_node_node(handler_ptr);
            assert_eq!(reg_ret, 0, "node() must return Ty::Int sentinel 0");

            // Construct the Node + drive the synthetic run loop.
            let name = alloc_str_buffer("detector");
            let node = __cobrust_dora_node_new(name);
            __cobrust_str_drop(name);
            let run_ret = __cobrust_dora_node_run(node);
            assert_eq!(run_ret, 0, "run must surface the 0 sentinel");

            // Confirm the borrow shims gave the canned values.
            let id_buf = OBSERVED_ID_PTR.swap(std::ptr::null_mut(), Ordering::SeqCst);
            let data_buf = OBSERVED_DATA_PTR.swap(std::ptr::null_mut(), Ordering::SeqCst);
            assert!(!id_buf.is_null(), "event.id() must return non-null Str");
            assert!(
                !data_buf.is_null(),
                "event.data_str() must return non-null Str"
            );
            let id_len = __cobrust_str_len(id_buf);
            let data_len = __cobrust_str_len(data_buf);
            let id_bytes = std::slice::from_raw_parts(__cobrust_str_ptr(id_buf), id_len as usize);
            let data_bytes =
                std::slice::from_raw_parts(__cobrust_str_ptr(data_buf), data_len as usize);
            assert_eq!(
                std::str::from_utf8(id_bytes).unwrap(),
                "camera",
                "canned event id must be 'camera'"
            );
            assert_eq!(
                std::str::from_utf8(data_bytes).unwrap(),
                "frame_001",
                "canned event payload must be 'frame_001'"
            );
            // Free the test-owned Str buffers + Node.
            __cobrust_str_drop(id_buf);
            __cobrust_str_drop(data_buf);
            __cobrust_dora_node_drop(node);

            // Reset the global slot so it doesn't bleed into other tests.
            REGISTERED_HANDLER.store(std::ptr::null_mut(), Ordering::SeqCst);
        }
        assert_eq!(
            HANDLER_FIRED.load(Ordering::SeqCst) - before_fire,
            1,
            "handler must have been invoked exactly once"
        );
        assert_eq!(drop_count() - before_drop, 1, "Node must drop exactly once");
    }

    /// `node.shutdown()` returns 0 on a live Node and -1 on null.
    #[test]
    fn shutdown_returns_clean_sentinel() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            assert_eq!(__cobrust_dora_node_shutdown(std::ptr::null_mut()), -1);
            let name = alloc_str_buffer("shutdownable");
            let node = __cobrust_dora_node_new(name);
            __cobrust_str_drop(name);
            assert_eq!(__cobrust_dora_node_shutdown(node), 0);
            // Idempotent — second shutdown still returns 0.
            assert_eq!(__cobrust_dora_node_shutdown(node), 0);
            __cobrust_dora_node_drop(node);
        }
    }

    // =================================================================
    // ADR-0076 Phase 2 — multi-input dispatch + send_output capture.
    // =================================================================

    /// Records every input id the multi-input handler observed via
    /// `event.id()`, so the test can assert the handler fired once per
    /// declared input. Reset under `DROP_COUNTER_LOCK` per test.
    static OBSERVED_INPUT_IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    #[unsafe(no_mangle)]
    extern "C" fn _dora_multi_input_handler(event: *mut u8) -> *mut u8 {
        HANDLER_FIRED.fetch_add(1, Ordering::SeqCst);
        // SAFETY: the trampoline hands a valid Boxed Event pointer.
        unsafe {
            let id_buf = __cobrust_dora_event_id(event);
            if !id_buf.is_null() {
                let len = __cobrust_str_len(id_buf);
                let bytes = std::slice::from_raw_parts(__cobrust_str_ptr(id_buf), len as usize);
                let id = std::str::from_utf8(bytes).unwrap_or("").to_string();
                if let Ok(mut v) = OBSERVED_INPUT_IDS.lock() {
                    v.push(id);
                }
                __cobrust_str_drop(id_buf);
            }
        }
        std::ptr::null_mut()
    }

    /// Two declared inputs ⇒ the handler fires twice, once per input id, in
    /// declaration order. Pins the multi-input dispatch contract at the
    /// runtime-shim level (the E2E pins it through the whole compile chain).
    #[test]
    fn declared_inputs_inject_one_event_each_in_order() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset_dora_globals();
        if let Ok(mut v) = OBSERVED_INPUT_IDS.lock() {
            v.clear();
        }
        let before_fire = HANDLER_FIRED.load(Ordering::SeqCst);
        unsafe {
            // Declare two inputs (the decorator desugar emits these calls).
            let tick = alloc_str_buffer("tick");
            let camera = alloc_str_buffer("camera");
            assert_eq!(__cobrust_dora_declare_input(tick), 0);
            assert_eq!(__cobrust_dora_declare_input(camera), 0);
            __cobrust_str_drop(tick);
            __cobrust_str_drop(camera);

            // Register + run.
            assert_eq!(
                __cobrust_dora_node_node(_dora_multi_input_handler as *const c_void),
                0
            );
            let name = alloc_str_buffer("sensor");
            let node = __cobrust_dora_node_new(name);
            __cobrust_str_drop(name);
            assert_eq!(__cobrust_dora_node_run(node), 0);
            __cobrust_dora_node_drop(node);
        }
        assert_eq!(
            HANDLER_FIRED.load(Ordering::SeqCst) - before_fire,
            2,
            "handler must fire once per declared input (2 inputs ⇒ 2 fires)"
        );
        let seen = OBSERVED_INPUT_IDS.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec!["tick".to_string(), "camera".to_string()],
            "handler must see both inputs in declaration order"
        );
        reset_dora_globals();
    }

    /// `send_output` on a DECLARED output captures (returns 0 + bumps the
    /// count); on an UNDECLARED output it fails CLOSED (returns -1, no
    /// count bump) — never a silent drop.
    #[test]
    fn send_output_validates_against_declared_outputs() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset_dora_globals();
        let before = send_output_count();
        unsafe {
            // Declare one output `reading`.
            let reading = alloc_str_buffer("reading");
            assert_eq!(__cobrust_dora_declare_output(reading), 0);
            __cobrust_str_drop(reading);

            // A canned Event (the receiver — borrowed).
            let event = Box::into_raw(Box::new(DoraEventHandle {
                id: "camera".to_string(),
                data_str: "frame_001".to_string(),
            }))
            .cast::<u8>();

            // Declared output ⇒ 0.
            let oid = alloc_str_buffer("reading");
            let payload = alloc_str_buffer("frame_001");
            assert_eq!(
                __cobrust_dora_event_send_output(event, oid, payload),
                0,
                "send on a declared output must return 0"
            );
            __cobrust_str_drop(oid);
            __cobrust_str_drop(payload);

            // Undeclared output ⇒ -1, fail-closed.
            let bad = alloc_str_buffer("redaing");
            let payload2 = alloc_str_buffer("x");
            assert_eq!(
                __cobrust_dora_event_send_output(event, bad, payload2),
                -1,
                "send on an UNDECLARED output must return -1 (fail closed)"
            );
            __cobrust_str_drop(bad);
            __cobrust_str_drop(payload2);

            // Reclaim the test-owned Event box.
            drop(Box::from_raw(event.cast::<DoraEventHandle>()));
        }
        assert_eq!(
            send_output_count() - before,
            1,
            "only the DECLARED send is captured (undeclared does not bump the count)"
        );
        reset_dora_globals();
    }

    /// `declare_output` is idempotent — declaring the same id twice stores
    /// it once (so a port re-declared by a noisy desugar still resolves).
    #[test]
    fn declare_output_is_idempotent() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset_dora_globals();
        unsafe {
            let a = alloc_str_buffer("reading");
            let b = alloc_str_buffer("reading");
            __cobrust_dora_declare_output(a);
            __cobrust_dora_declare_output(b);
            __cobrust_str_drop(a);
            __cobrust_str_drop(b);
        }
        assert_eq!(
            DECLARED_OUTPUTS.lock().unwrap().len(),
            1,
            "a port declared twice is stored once"
        );
        reset_dora_globals();
    }
}
