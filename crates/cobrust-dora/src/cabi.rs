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

/// Runtime form of a `Node` handle the `.cb` source owns. Phase 1
/// captures only the node name; the registered handler lives in the
/// process-global [`REGISTERED_HANDLER`] slot. Phase 2 will fold the
/// handler vector into per-Node state.
struct DoraNodeHandle {
    /// User-supplied node identifier (e.g. `"detector"`). Currently
    /// unused at runtime by Phase 1's `run` synthetic loop — preserved
    /// so a future `node.id() -> str` follow-up reads it without
    /// re-allocating the Node.
    _name: String,
    /// Whether `shutdown()` has been invoked on this Node. Phase 1's
    /// shutdown is a soft flag the synthetic `run` honors (in Phase 2
    /// it becomes a real signal to the dora coordinator).
    shutdown_called: bool,
}

/// Runtime form of an Event the trampoline allocates per-callback
/// invocation. Mirrors pit's Request shape — the .cb side sees a `*mut u8`
/// Adt handle and calls borrow-shim methods (`event.id()` /
/// `event.data_str()`) that materialise fresh Cobrust Str buffers from
/// these fields.
struct DoraEventHandle {
    /// Input id this event arrived on (e.g. `"camera"`).
    id: String,
    /// Payload bytes as a UTF-8 string. Phase 1 ships Str only; Phase 2
    /// widens to Arrow `RecordBatch` via `__cobrust_dora_event_data_arrow`
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
    let handle = DoraNodeHandle {
        _name: name_s,
        shutdown_called: false,
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
        // Box the Event so the .cb handler receives an opaque *mut u8
        // Adt-pointer (ADR-0073 §2 D6 — Rust owns the box). The trampoline
        // owns the Box for the callback invocation and frees it on return.
        let event_raw = Box::into_raw(Box::new(event)).cast::<u8>();

        // Catch panics across the C ABI (ADR-0073 §3 Q5).
        let ret_raw = std::panic::catch_unwind(|| {
            // SAFETY: `raw` is a valid `CbHandlerAbi`; `event_raw` is a
            // valid Boxed Event pointer just constructed.
            unsafe { raw(event_raw) }
        });

        // Free the Event box exactly once on the way out. The .cb source
        // NEVER drops a dora.Event local (manifest `handle_drop_symbol`
        // returns None for DORA_EVENT_ADT — mirrors pit.Request).
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
    // Discard the handler return-pointer(s) (mirrors hood's "side-effect IS
    // the intent" pattern). Surface the manifest-declared 0 sentinel so the
    // .cb source's `let _ = node.run()` discards a clean i64.
    0
}

/// The canned payload Str for a declared input id (ADR-0076 Phase 2
/// synthetic trampoline). The `camera` input keeps the Phase-1 canonical
/// `"frame_001"` payload (so `event.data_str()` is stable across the
/// single-input no-regression path); every other input id gets a distinct
/// non-empty `"frame_<id>"` canned Str so the handler can tell injected
/// events apart. A real broker replaces this with the actual Arrow payload.
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

    // Validate against the declared-output set.
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

    // Capture: print the marker line the synthetic E2E asserts, and bump the
    // count instrument. A real broker would marshal `payload` into an Arrow
    // RecordBatch + publish on the zenoh output channel here.
    println!("output[{id_s}]={payload_s}");
    SEND_OUTPUT_COUNT.fetch_add(1, Ordering::SeqCst);
    0
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

#[cfg(test)]
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
