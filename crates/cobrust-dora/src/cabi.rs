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

// =====================================================================
// Cobrust `bytes`-buffer ABI (ADR-0093) — declared here, resolved from
// libcobrust_stdlib.a at link time (the SAME no-Rust-dep pattern as the
// Str ABI above). The `data_bytes()` accessor mints a `bytes` via
// `__cobrust_bytes_from_raw`; `send_output_bytes` reads the borrowed
// `bytes` payload via `__cobrust_bytes_ptr` + `__cobrust_bytes_len`
// (an O(1) `&[u8]` read — NOT an O(n) `__cobrust_bytes_get` loop).
// =====================================================================

unsafe extern "C" {
    /// Mint a FRESH owned `bytes` from a `(ptr, len)` slice (the
    /// `.cb`-owned handle the scope drops once via `__cobrust_bytes_drop`).
    /// NULL ptr / `len <= 0` → a valid EMPTY buffer.
    fn __cobrust_bytes_from_raw(ptr: *const u8, len: i64) -> *mut u8;
    /// Borrow a `bytes` handle's raw byte pointer (NULL/empty → null).
    fn __cobrust_bytes_ptr(b: *mut u8) -> *const u8;
    /// Borrow a `bytes` handle's byte length (NULL → 0).
    fn __cobrust_bytes_len(b: *mut u8) -> i64;
}

/// Borrow a `bytes` handle as a read-only `&[u8]` via the O(1)
/// `(ptr, len)` ABI. Tolerates null / empty (→ `&[]`). BORROW-only — the
/// handle is NOT consumed (the `.cb` scope still drops it once). Used by
/// the real `send_output_bytes` to read the payload into an Arrow
/// `BinaryArray` blob.
///
/// # Safety
///
/// `b` must be null or a valid `bytes` handle from
/// `__cobrust_bytes_from_raw` not yet dropped; the returned slice is
/// valid only while `b` lives.
unsafe fn bytes_buf_as_slice<'a>(b: *mut u8) -> &'a [u8] {
    if b.is_null() {
        return &[];
    }
    // SAFETY: caller attests `b` is a valid `bytes` handle.
    unsafe {
        let ptr = __cobrust_bytes_ptr(b);
        let len = __cobrust_bytes_len(b);
        if ptr.is_null() || len <= 0 {
            return &[];
        }
        std::slice::from_raw_parts(ptr, len as usize)
    }
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
    /// The `coil.Buffer` numeric surface (ADR-0076c (D)-B-1a) is the
    /// SIBLING [`Self::data_buffer`] field below — `data_str` stays the
    /// Utf8 / non-numeric path (the named ADR-0076c divergence).
    data_str: String,
    /// ADR-0076c (D)-B-1a — the typed-numeric payload decoded as a
    /// `coil::Array` (the `.cb` `coil.Buffer` payload), when the wire dtype
    /// is one of the 5 supported primitives (`Float64/Float32/Int64/Int32/
    /// Bool`). `None` when the payload is non-numeric (Utf8 → use
    /// `data_str`) / an unsupported dtype (`UInt8`/`Utf8`/n-D — the named
    /// ADR-0076c divergences). Decoded ONCE at recv time (real build) /
    /// canned (synthetic) so `event.data_buffer()` is a pure
    /// borrow-and-clone shim — it never re-reads the wire (mirrors how
    /// `data_str` is pre-decoded, avoiding an arrow lifetime in this
    /// `'static` struct).
    data_buffer: Option<coil::Array>,
    /// ADR-0076c (D)-B-1b / ADR-0093 Phase 2 — the RAW byte payload
    /// decoded as an owned `Vec<u8>` (the `.cb` `bytes` payload), when the
    /// wire dtype is Arrow `Binary` (a single-row blob) OR `UInt8` (a flat
    /// byte list — the COMPLEMENT of `data_buffer`, which DEFERS these two
    /// as a named ADR-0076c divergence). `None` when the payload is a
    /// numeric `coil.Buffer` dtype / `Utf8` / null-bearing / unexpected —
    /// `event.data_bytes()` then mints an EMPTY `bytes` (len 0), NEVER a
    /// silent garbage read (§2.2). Decoded ONCE at recv time (real build) /
    /// canned (synthetic) so `event.data_bytes()` is a pure
    /// borrow-and-mint shim. A `0xFF`/`0x00` byte round-trips EXACTLY
    /// (raw, never UTF-8-lossy).
    data_bytes: Option<Vec<u8>>,
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
        let event = DoraEventHandle {
            id,
            data_str,
            // ADR-0076c (D)-B-1a — the synthetic build hands a canned
            // typed Float64 buffer so `event.data_buffer()` resolves the
            // symbol + the `.cb` build/type-check path runs without a
            // broker (mirrors how synthetic `data_str` returns a canned
            // `"frame_001"`). The real build replaces this with the live
            // Arrow decode.
            data_buffer: Some(canned_buffer()),
            // ADR-0076c (D)-B-1b — the synthetic build hands a canned RAW
            // `bytes` payload carrying a NON-UTF-8 byte (`b"\x00\xff\x01"`)
            // so `event.data_bytes()` resolves the symbol AND proves
            // BYTE-FIDELITY end-to-end (a `\xff` is preserved exactly — the
            // raw bytes path is never UTF-8-lossy). The real build replaces
            // this with the live Arrow Binary/UInt8 decode.
            data_bytes: Some(canned_bytes()),
        };
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

/// The canned typed `coil::Array` the SYNTHETIC build hands from
/// `event.data_buffer()` (ADR-0076c (D)-B-1a). A 1-D `Float64` `[1.0, 2.0,
/// 3.0]` — small, exactly-representable, and a Float64 (the dominant
/// numeric robotics dtype) so the `.cb` build/type-check path runs without
/// a live broker (the symbol resolves + the chain links). The real build
/// replaces this with the decode of the live Arrow `ArrayRef`.
///
/// Synthetic-build only — the `dora-real` path decodes the REAL Arrow
/// payload instead (`real::decode_arrow_buffer`), so this canned helper is
/// `#[cfg]`-gated off there to keep `clippy -D warnings` clean.
#[cfg(not(feature = "dora-real"))]
fn canned_buffer() -> coil::Array {
    // Build via coil's own `array_f64` constructor (re-exported at the
    // crate root) so this crate needs NO direct `ndarray` dep — the same
    // 1-D `[len]` Float64 shape coil's `.cb` constructors produce.
    coil::array_f64(&[1.0_f64, 2.0, 3.0], &[3]).expect("canned [3] Float64 buffer is well-shaped")
}

/// The canned RAW `bytes` payload the SYNTHETIC build hands from
/// `event.data_bytes()` (ADR-0076c (D)-B-1b). A 3-byte sequence carrying
/// a NON-UTF-8 byte (`0x00, 0xff, 0x01`) so the `.cb` build/type-check
/// path runs without a live broker AND the BYTE-FIDELITY contract is
/// proven end-to-end: a `0xff` round-trips EXACTLY (the raw bytes path is
/// never UTF-8-lossy, unlike the str path that would corrupt `\xff`). The
/// real build replaces this with the decode of the live Arrow
/// `Binary`/`UInt8` payload.
///
/// Synthetic-build only — the `dora-real` path decodes the REAL Arrow
/// payload instead (`real::decode_arrow_bytes`), so this canned helper is
/// `#[cfg]`-gated off there to keep `clippy -D warnings` clean.
#[cfg(not(feature = "dora-real"))]
fn canned_bytes() -> Vec<u8> {
    vec![0x00, 0xff, 0x01]
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

/// A fresh EMPTY 1-D `Float64` `coil::Array` (`array([], dtype=float64)`).
/// The well-defined `coil.Buffer` fallback `data_buffer()` returns for a
/// null event / a non-numeric / unsupported-dtype payload (the named
/// ADR-0076c divergences) — an empty Buffer is a valid `.cb` handle the
/// caller still drops ONCE, never a null the drop schedule would mishandle.
/// Build-agnostic (both the synthetic + real `data_buffer` use it).
fn empty_buffer() -> coil::Array {
    coil::array_f64(&[], &[0]).expect("empty [0] Float64 buffer is well-shaped")
}

/// `event.data_buffer() -> coil.Buffer` (ADR-0076c (D)-B-1a). Returns a
/// freshly-Boxed `coil::Array` handle carrying the event's typed-numeric
/// payload — the `.cb` `coil.Buffer` surface for the 5 supported dtypes
/// (`Float64/Float32/Int64/Int32/Bool`). The handle is `.cb`-OWNED: the
/// caller's scope-exit drop frees it ONCE via the existing
/// `__cobrust_coil_buffer_drop` (the manifest `handle_drop_symbol(COIL_BUFFER_ADT)`
/// resolves it — NO new drop symbol). The Event itself is BORROWED (the
/// trampoline retains its `Box<Event>` and frees it on callback return per
/// ADR-0073 §2 D6); only the cloned Buffer is on the `.cb` drop schedule.
///
/// The payload was decoded ONCE at recv time (real build) / canned
/// (synthetic) into the Event's `data_buffer` field; this shim is a pure
/// borrow-and-clone (it never re-reads the wire — no arrow lifetime crosses
/// the C ABI). When the payload is non-numeric (Utf8 → use `data_str`) or
/// an unsupported dtype (`UInt8`/`Utf8`/n-D — the named ADR-0076c
/// divergences), the field is `None` and this returns an EMPTY Float64
/// buffer (a well-defined `.cb` Buffer the caller still drops once — never
/// a null the `.cb` drop schedule would choke on). The dora-real decode
/// already logged the divergence in that case.
///
/// # Safety
///
/// `event` must be a valid Event handle the dora trampoline allocated for
/// the current callback invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_event_data_buffer(event: *mut u8) -> *mut u8 {
    if event.is_null() {
        return Box::into_raw(Box::new(empty_buffer())).cast::<u8>();
    }
    // SAFETY: caller per `# Safety`. Borrow-only — the trampoline retains
    // ownership of the Box and frees it after the callback returns.
    let event_ref: &DoraEventHandle = unsafe { &*event.cast::<DoraEventHandle>() };
    // Clone the pre-decoded typed payload (or the empty fallback when the
    // wire dtype was non-numeric / unsupported). Box it as the
    // `COIL_BUFFER_ADT` handle the `.cb` scope owns + drops once.
    let arr = event_ref.data_buffer.clone().unwrap_or_else(empty_buffer);
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// `event.send_output_buffer(output_id: str, buf: coil.Buffer) -> i64`
/// (ADR-0076c (D)-B-1a). The handler emits a typed-numeric Arrow array
/// (bridged from the `coil.Buffer`) on the declared `output_id` port — the
/// numeric-payload SIBLING of `event.send_output(id, str)`. A DISTINCT
/// method name (NOT a `send_output` overload) for §2.5 compile-time
/// clarity — an LLM picks `send_output_buffer` vs `send_output`
/// unambiguously.
///
/// The trampoline (BOTH builds):
/// 1. VALIDATES `output_id` against [`DECLARED_OUTPUTS`] (the same
///    fail-closed backstop as `send_output` — an UNDECLARED id is a clear
///    stderr diagnostic + `-1`, never a silent drop). The compile-time
///    `DoraUnknownOutputId` check ALSO fires for this method now (check.rs
///    extends its method match), so a literal typo'd id is caught at
///    `cobrust check`; this runtime check covers the dynamic-id case.
/// 2. bumps [`SEND_OUTPUT_COUNT`], then dispatches:
///    - REAL: bridge the `coil::Array` → a typed Arrow array → publish via
///      the ambient live `DoraNode`.
///    - SYNTHETIC: capture a `output[<id>]=buffer[len=<n>]` marker on
///      stdout (the synthetic E2E asserts it) — no arrow dep referenced.
///
/// The `buf` handle is BORROWED (read, never freed) — the `.cb` scope's
/// drop schedule still owns it and frees it ONCE via
/// `__cobrust_coil_buffer_drop` at scope exit. Returns 0 on a declared
/// emission, `-1` on an undeclared output id (the `.cb` `let _ = ...`
/// discards the sentinel).
///
/// # Safety
///
/// `event` must be a valid Event handle for the current callback;
/// `output_id` must be null or a valid Cobrust `Str` buffer; `buf` must be
/// null or a live `coil.Buffer` handle (a boxed `coil::Array` from a coil
/// constructor / `__cobrust_dora_event_data_buffer`) that the caller has
/// NOT yet dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_event_send_output_buffer(
    event: *mut u8,
    output_id: *mut u8,
    buf: *mut u8,
) -> i64 {
    // Borrow the Event for symmetry with `send_output` (the synthetic
    // capture validates against the GLOBAL declared set, not per-event
    // state; the real broker routes through the originating node).
    // SAFETY: caller per `# Safety`. Borrow-only; tolerate null.
    if !event.is_null() {
        let _event_ref: &DoraEventHandle = unsafe { &*event.cast::<DoraEventHandle>() };
    }
    // SAFETY: caller-attestation per `# Safety`.
    let id_s = unsafe { read_str_buf(output_id) };

    // Validate against the declared-output set (BOTH builds — the same
    // fail-closed contract as `send_output`; the §2.5 compile-time
    // `DoraUnknownOutputId` reject fires for this method too at check time).
    let declared = DECLARED_OUTPUTS
        .lock()
        .map(|set| set.iter().any(|o| o == &id_s))
        .unwrap_or(false);
    if !declared {
        eprintln!(
            "cobrust-dora: send_output_buffer on UNDECLARED output id {id_s:?} — declare it via \
             `@dora.node(outputs=[{id_s:?}])` (ADR-0076c). Output dropped."
        );
        return -1;
    }

    // Count the emission on BOTH builds (the cabi unit tests read this).
    SEND_OUTPUT_COUNT.fetch_add(1, Ordering::SeqCst);

    // REAL build: bridge `coil::Array` → typed Arrow → publish via the
    // ambient `DoraNode`. SYNTHETIC build: capture a length marker (NO arrow
    // dep referenced — the synthetic default build has zero arrow). Exactly
    // one `let ret` active per build → clippy-clean tail.
    #[cfg(feature = "dora-real")]
    // SAFETY: caller attests `buf` is null or a live `coil.Buffer` handle.
    let ret = unsafe { real::send_output_buffer(&id_s, buf) };
    #[cfg(not(feature = "dora-real"))]
    // SAFETY: caller attests `buf` is null or a live `coil.Buffer` handle.
    let ret = unsafe {
        let n = buffer_len(buf);
        println!("output[{id_s}]=buffer[len={n}]");
        0
    };
    ret
}

/// Borrow a `coil.Buffer` handle and read its element count, tolerating
/// null (→ 0). Used by the SYNTHETIC `send_output_buffer` capture marker
/// (the real build reads the len through the bridge instead, so this is
/// `#[cfg]`-gated off there to keep `clippy -D warnings` clean). Borrow-only
/// — never frees the handle.
///
/// # Safety
///
/// `buf` must be null or a live `coil.Buffer` handle (a boxed `coil::Array`)
/// the caller has not dropped.
#[cfg(not(feature = "dora-real"))]
unsafe fn buffer_len(buf: *mut u8) -> usize {
    if buf.is_null() {
        return 0;
    }
    // SAFETY: caller per `# Safety`. Borrow-only.
    let arr: &coil::Array = unsafe { &*buf.cast::<coil::Array>() };
    arr.size()
}

/// `event.data_bytes() -> bytes` (ADR-0076c (D)-B-1b / ADR-0093 Phase 2).
/// The RAW-BYTES sibling of [`__cobrust_dora_event_data_buffer`]: mints a
/// fresh `.cb` `bytes` value carrying the event's RAW byte payload (Arrow
/// `Binary`/`UInt8` on the real build; a canned non-UTF-8 `b"\x00\xff\x01"`
/// on the synthetic build). The decode happened ONCE at recv (the
/// `data_bytes` field), so this shim is a pure borrow-and-mint — it never
/// re-reads the wire.
///
/// When the payload was non-bytes (a numeric `coil.Buffer` dtype → use
/// `data_bytes`'s SIBLING `data_buffer`; a `Utf8` string → use `data_str`;
/// a null-bearing / unexpected array — the named ADR-0076c divergences),
/// the field is `None` and this mints an EMPTY `bytes` (len 0): a
/// well-defined `.cb` value the caller still drops once, NEVER a silent
/// garbage read or UB (§2.2). A null event likewise mints an empty bytes.
/// BYTE-FIDELITY: a `0xFF`/`0x00` byte round-trips EXACTLY (raw, never
/// UTF-8-lossy).
///
/// The returned `*mut bytes` is `.cb`-owned + scope-exit-drops via the
/// EXISTING `__cobrust_bytes_drop` (no new drop registration — `bytes` is
/// a full type whose drop lives in `libcobrust_stdlib.a`, always linked).
///
/// # Safety
///
/// `event` must be a valid Event handle the dora trampoline allocated for
/// the current callback invocation, OR null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_event_data_bytes(event: *mut u8) -> *mut u8 {
    // Resolve the decoded payload (or an empty slice for a null event /
    // non-bytes payload), then MINT a fresh `.cb`-owned bytes carrying a
    // COPY. `__cobrust_bytes_from_raw` tolerates a null ptr / 0 len → a
    // valid empty buffer, so every path yields a non-null droppable handle.
    let empty: &[u8] = &[];
    let slice: &[u8] = if event.is_null() {
        empty
    } else {
        // SAFETY: caller per `# Safety`. Borrow-only — the trampoline
        // retains ownership of the Box and frees it after the callback.
        let event_ref: &DoraEventHandle = unsafe { &*event.cast::<DoraEventHandle>() };
        event_ref.data_bytes.as_deref().unwrap_or(empty)
    };
    // SAFETY: `slice` is a valid `(ptr, len)` (or empty → null ptr / 0
    // len, which the mint tolerates). The mint copies the bytes into a
    // fresh heap buffer the `.cb` scope owns.
    unsafe { __cobrust_bytes_from_raw(slice.as_ptr(), slice.len() as i64) }
}

/// `event.send_output_bytes(output_id: str, b: bytes) -> i64`
/// (ADR-0076c (D)-B-1b). The RAW-BYTES sibling of
/// [`__cobrust_dora_event_send_output_buffer`]: emits a RAW byte payload
/// (the `.cb` `bytes` → a dora `send_output` of an Arrow `BinaryArray`
/// blob) on the DECLARED `output_id` port. A DISTINCT method name (NOT a
/// `send_output` overload) for §2.5 compile-time clarity.
///
/// The trampoline (BOTH builds):
/// 1. VALIDATES `output_id` against [`DECLARED_OUTPUTS`] (the same
///    fail-closed backstop as `send_output_buffer` — an UNDECLARED id is a
///    clear stderr diagnostic + `-1`. The compile-time `DoraUnknownOutputId`
///    check ALSO fires for this method now (check.rs extends its match), so
///    a literal typo'd id is caught at `cobrust check`; this covers the
///    dynamic-id case).
/// 2. bumps [`SEND_OUTPUT_COUNT`], then dispatches:
///    - REAL: read the borrowed `bytes` → an Arrow `BinaryArray` blob →
///      publish via the ambient live `DoraNode`.
///    - SYNTHETIC: capture a `output[<id>]=bytes[len=<n>]` marker on stdout
///      (the synthetic E2E asserts it) — no arrow dep referenced.
///
/// The `b` handle is BORROWED (read via `__cobrust_bytes_ptr`, never
/// freed) — the `.cb` scope's drop schedule still owns it and frees it
/// ONCE via `__cobrust_bytes_drop` at scope exit. Returns 0 on a declared
/// emission, `-1` on an undeclared output id.
///
/// # Safety
///
/// `event` must be a valid Event handle for the current callback, or null;
/// `output_id` must be null or a valid Cobrust `Str` buffer; `b` must be
/// null or a live `bytes` handle (from `__cobrust_bytes_from_raw` /
/// `event.data_bytes()`) the caller has NOT yet dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dora_event_send_output_bytes(
    event: *mut u8,
    output_id: *mut u8,
    b: *mut u8,
) -> i64 {
    // Borrow the Event for symmetry with `send_output_buffer` (the
    // synthetic capture validates against the GLOBAL declared set).
    // SAFETY: caller per `# Safety`. Borrow-only; tolerate null.
    if !event.is_null() {
        let _event_ref: &DoraEventHandle = unsafe { &*event.cast::<DoraEventHandle>() };
    }
    // SAFETY: caller-attestation per `# Safety`.
    let id_s = unsafe { read_str_buf(output_id) };

    // Validate against the declared-output set (BOTH builds — the same
    // fail-closed contract as `send_output_buffer`; the §2.5 compile-time
    // `DoraUnknownOutputId` reject fires for this method too at check time).
    let declared = DECLARED_OUTPUTS
        .lock()
        .map(|set| set.iter().any(|o| o == &id_s))
        .unwrap_or(false);
    if !declared {
        eprintln!(
            "cobrust-dora: send_output_bytes on UNDECLARED output id {id_s:?} — declare it via \
             `@dora.node(outputs=[{id_s:?}])` (ADR-0076c). Output dropped."
        );
        return -1;
    }

    // Count the emission on BOTH builds (the cabi unit tests read this).
    SEND_OUTPUT_COUNT.fetch_add(1, Ordering::SeqCst);

    // REAL build: read the borrowed `bytes` → an Arrow Binary blob →
    // publish via the ambient node. SYNTHETIC build: capture a length
    // marker (NO arrow dep referenced). Exactly one `let ret` active per
    // build → clippy-clean tail.
    #[cfg(feature = "dora-real")]
    // SAFETY: caller attests `b` is null or a live `bytes` handle.
    let ret = unsafe { real::send_output_bytes(&id_s, b) };
    #[cfg(not(feature = "dora-real"))]
    // SAFETY: caller attests `b` is null or a live `bytes` handle.
    let ret = unsafe {
        let n = bytes_buf_as_slice(b).len();
        println!("output[{id_s}]=bytes[len={n}]");
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

    // ADR-0076c (D)-B-1a — the typed-numeric Arrow↔coil bridge. arrow
    // reached through dora_node_api's `pub use arrow` re-export (dora-node
    // -api 0.5.0 lib.rs L89 — NO new Cargo.toml dep). The 5 concrete array
    // types (each impls `arrow::array::Array`, the `to_data()` bound on
    // `DoraNode::send_output`'s `data: impl Array` 3rd arg) + `DataType`
    // (arrow re-exports `arrow_schema::DataType`) for the recv-time dtype
    // dispatch.
    use dora_node_api::arrow::array::{
        BinaryArray, BooleanArray, Float32Array, Float64Array, Int32Array, Int64Array, UInt8Array,
    };
    use dora_node_api::arrow::datatypes::DataType;

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
                    // ADR-0076c (D)-B-1a — ALSO decode the typed-numeric
                    // payload into an owned `coil::Array` (a `'static`
                    // value — no arrow lifetime crosses into the Event
                    // struct) when the wire dtype is one of the 5
                    // supported primitives; `None` for Utf8 / unsupported
                    // (the named divergences). `event.data_buffer()` then
                    // hands a clone of this.
                    let data_buffer = decode_arrow_buffer(&data);
                    // ADR-0076c (D)-B-1b — ALSO decode the RAW byte payload
                    // (Arrow `Binary`/`UInt8` → an owned `Vec<u8>`; `None`
                    // for the numeric / Utf8 / null-bearing / unexpected
                    // cases — the COMPLEMENT of `decode_arrow_buffer`).
                    // `event.data_bytes()` then mints a `bytes` from this.
                    let data_bytes = decode_arrow_bytes(&data);
                    let ev_handle = DoraEventHandle {
                        id: id_s,
                        data_str,
                        data_buffer,
                        data_bytes,
                    };

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

    /// ADR-0076c (D)-B-1a — decode a real `arrow::array::ArrayRef` payload
    /// into an owned `coil::Array` (the `.cb` `coil.Buffer`) for the 5
    /// supported dtypes (`Float64/Float32/Int64/Int32/Boolean`). Returns
    /// `None` for any other arrow type (`Utf8` → use `data_str`;
    /// `UInt8`/`Int8`/n-D/nested → the named ADR-0076c divergences, logged
    /// once here), so `event.data_buffer()` hands the empty-buffer fallback.
    ///
    /// NULL BITMAP (ADR-0076c §U2 + REPAIR MAJOR) — `coil::Array` has NO
    /// null concept (it is a dense `ndarray`), so a null-BEARING arrow array
    /// CANNOT round-trip faithfully: reading `.values()` / `.value(i)` would
    /// SILENTLY materialise a null slot as the raw underlying buffer value
    /// (e.g. `[Some(1.0), None, Some(3.0)]` → `[1.0, 0.0, 3.0]`, a null Bool
    /// → `false`). That is a silent data alteration on a known-HIGH-risk
    /// path, so we treat `null_count() > 0` as a NAMED ADR-0076c divergence:
    /// LOG it (a dropped numeric payload is never silent) and return `None`
    /// (→ `data_buffer()` hands the empty-buffer fallback, same as the other
    /// divergences) RATHER than fabricate a value for the null. A NULL-FREE
    /// array (`null_count() == 0` — the dora numeric-payload common case)
    /// round-trips bit-faithfully through the dense path below.
    ///
    /// The bridge is FLAT 1-D (ADR-0076c §1.3: dora arrays are
    /// flat-columnar, shape lives in metadata). `data.values()` yields a
    /// `&ScalarBuffer<T>` that derefs to `&[T]`; we `.to_vec()` it into the
    /// matching `coil::array_*` constructor with a 1-D `[len]` shape. Bool
    /// has no `.values() -> &[bool]` (it is a bit-packed `BooleanBuffer`),
    /// so we materialise it via `.value(i)`. The roundtrip is
    /// bit-faithful: an `Int64` stays `Int64` (no float up-cast), a
    /// `Float64` stays `Float64`.
    pub(super) fn decode_arrow_buffer(data: &ArrowData) -> Option<coil::Array> {
        // `data` derefs to `arrow::array::ArrayRef` (`Arc<dyn Array>`);
        // `data.data_type()` + `data.as_any().downcast_ref::<_>()` is the
        // dora-canonical typed-read idiom (arrow-convert `into_vec`).
        let len = data.len();
        // NULL BITMAP guard (REPAIR MAJOR / ADR-0076c §U2 named divergence) —
        // a null-bearing array would silently fabricate values for its null
        // slots (coil has no null concept). Reject it as a logged divergence
        // BEFORE the dense decode so no null is ever silently altered. (This
        // is checked across ALL dtypes — `Array::null_count()` is on the
        // `dyn Array` trait, no downcast needed.)
        if data.null_count() > 0 {
            eprintln!(
                "cobrust-dora (dora-real): event.data_buffer() — the input {:?} array carries \
                 {} null(s); coil.Buffer has no null concept, so a faithful decode is impossible \
                 (ADR-0076c named divergence: null bitmap). Returning an empty buffer rather than \
                 silently materialising nulls as 0/false. Use a null-free array (or event.data_str \
                 for a non-numeric payload).",
                data.data_type(),
                data.null_count(),
            );
            return None;
        }
        match data.data_type() {
            DataType::Float64 => {
                let a = data.as_any().downcast_ref::<Float64Array>()?;
                // `a.values()` is a `&ScalarBuffer<f64>` that derefs to
                // `&[f64]` — pass it directly (no copy needed before the
                // constructor's own `to_vec`).
                coil::array_f64(a.values(), &[len]).ok()
            }
            DataType::Float32 => {
                let a = data.as_any().downcast_ref::<Float32Array>()?;
                coil::array_f32(a.values(), &[len]).ok()
            }
            DataType::Int64 => {
                let a = data.as_any().downcast_ref::<Int64Array>()?;
                coil::array_i64(a.values(), &[len]).ok()
            }
            DataType::Int32 => {
                let a = data.as_any().downcast_ref::<Int32Array>()?;
                coil::array_i32(a.values(), &[len]).ok()
            }
            DataType::Boolean => {
                let a = data.as_any().downcast_ref::<BooleanArray>()?;
                // BooleanArray is bit-packed: collect via `.value(i)`.
                let vals: Vec<bool> = (0..a.len()).map(|i| a.value(i)).collect();
                coil::array_bool(&vals, &[len]).ok()
            }
            // Utf8 → `data_str` carries it; everything else is a named
            // ADR-0076c divergence (`UInt8` images, `Int8`, n-D, nested).
            // Log ONCE so a dropped numeric payload is never silent.
            other => {
                eprintln!(
                    "cobrust-dora (dora-real): event.data_buffer() — arrow dtype {other:?} is not \
                     one of the 5 supported coil.Buffer dtypes (Float64/Float32/Int64/Int32/Bool) \
                     (ADR-0076c divergence: UInt8/Utf8/n-D deferred). Use event.data_str() for a \
                     Utf8 payload. data_buffer() returns an empty buffer."
                );
                None
            }
        }
    }

    /// ADR-0076c (D)-B-1b / ADR-0093 Phase 2 — decode a real
    /// `arrow::array::ArrayRef` payload into an owned `Vec<u8>` (the `.cb`
    /// `bytes`) for the RAW-byte dtypes Arrow `Binary` (a single-row blob)
    /// and `UInt8` (a flat byte list). This is the COMPLEMENT of
    /// [`decode_arrow_buffer`], which EXPLICITLY DEFERS `Binary`/`UInt8` as
    /// a named ADR-0076c divergence precisely because THIS accessor owns
    /// them. Returns `None` for any other arrow type (a numeric
    /// `coil.Buffer` dtype → use `data_buffer`; `Utf8` → use `data_str`;
    /// anything else — logged once), so `event.data_bytes()` mints the
    /// EMPTY-bytes fallback rather than a silent garbage read (§2.2).
    ///
    /// NULL BITMAP — a null-bearing array CANNOT round-trip a raw byte
    /// payload faithfully (a `bytes` has no null concept; reading a null
    /// slot's underlying buffer value would SILENTLY fabricate a byte). So,
    /// mirroring `decode_arrow_buffer`'s null handling, `null_count() > 0`
    /// is a named divergence: LOG it (a dropped payload is never silent) and
    /// return `None` (→ an EMPTY bytes), NEVER silent corruption.
    ///
    /// BYTE-FIDELITY: `Binary`'s `arr.value(0) -> &[u8]` and `UInt8`'s
    /// `arr.values() -> &[u8]` are copied byte-exact into the `Vec<u8>`,
    /// so a `0xFF`/`0x00` byte round-trips EXACTLY.
    pub(super) fn decode_arrow_bytes(data: &ArrowData) -> Option<Vec<u8>> {
        // NULL BITMAP guard (mirrors `decode_arrow_buffer`) — reject a
        // null-bearing array as a logged divergence BEFORE the dense read so
        // no null is ever silently materialised as a byte. (`null_count()`
        // is on the `dyn Array` trait — no downcast needed.)
        if data.null_count() > 0 {
            eprintln!(
                "cobrust-dora (dora-real): event.data_bytes() — the input {:?} array carries \
                 {} null(s); a raw `bytes` payload has no null concept, so a faithful decode is \
                 impossible (ADR-0076c named divergence: null bitmap). Returning an EMPTY bytes \
                 rather than silently fabricating a byte for the null.",
                data.data_type(),
                data.null_count(),
            );
            return None;
        }
        match data.data_type() {
            // A `Binary` array carries variable-length byte blobs; the dora
            // raw-bytes payload is a SINGLE-row blob — read row 0's `&[u8]`.
            // An empty array (`len() == 0`) → an empty bytes (no row to
            // read).
            DataType::Binary => {
                let a = data.as_any().downcast_ref::<BinaryArray>()?;
                // `data.is_empty()` (the `dyn Array` trait method) — 0 rows
                // ⇒ no blob to read ⇒ empty bytes.
                if data.is_empty() {
                    Some(Vec::new())
                } else {
                    Some(a.value(0).to_vec())
                }
            }
            // A `UInt8` array is a FLAT byte list — `arr.values()` is a
            // `&ScalarBuffer<u8>` that derefs to `&[u8]`; copy it byte-exact.
            DataType::UInt8 => {
                let a = data.as_any().downcast_ref::<UInt8Array>()?;
                Some(a.values().to_vec())
            }
            // A numeric `coil.Buffer` dtype → `data_buffer` carries it; a
            // `Utf8` string → `data_str`; anything else is unexpected. Log
            // ONCE so a dropped raw payload is never silent → empty bytes.
            other => {
                eprintln!(
                    "cobrust-dora (dora-real): event.data_bytes() — arrow dtype {other:?} is not a \
                     raw-bytes dtype (Binary/UInt8). Use event.data_buffer() for a numeric payload \
                     or event.data_str() for a Utf8 payload. data_bytes() returns an empty bytes."
                );
                None
            }
        }
    }

    /// REAL `event.send_output_bytes(id, b)` — read a borrowed `bytes`
    /// handle as a `&[u8]` (via the `__cobrust_bytes_ptr`/`_len` ABI),
    /// build a length-1 Arrow `BinaryArray` blob from it, and publish on the
    /// `id` output port via the ambient live `DoraNode` (ADR-0076c
    /// (D)-B-1b; the raw-bytes SIBLING of `send_output_buffer`). The
    /// output-id validation already happened in the calling shim. `b` is
    /// BORROWED (read, never freed — the `.cb` scope owns + drops it once
    /// via `__cobrust_bytes_drop`). Returns 0 on a successful publish, `-1`
    /// if no ambient node is set or the publish errored. A null / empty `b`
    /// publishes a single-row EMPTY blob (a valid wire payload, NOT an
    /// error — the symmetric inverse of `data_bytes()`'s empty decode).
    ///
    /// # Safety
    ///
    /// `b` must be null or a live `bytes` handle (from
    /// `__cobrust_bytes_from_raw` / `event.data_bytes()`) the caller has not
    /// dropped.
    pub(super) unsafe fn send_output_bytes(id: &str, b: *mut u8) -> i64 {
        let node_ptr = AMBIENT_NODE.with(Cell::get);
        if node_ptr.is_null() {
            eprintln!(
                "cobrust-dora (dora-real): send_output_bytes(\"{id}\", ...) called with no \
                 ambient node — it must run inside a node.run() callback. Output dropped."
            );
            return -1;
        }
        // SAFETY: `run_node` set `AMBIENT_NODE` to a valid `&mut DoraNode`
        // for the duration of THIS callback and clears it on return; we are
        // synchronously inside that window, single-threaded, so the pointer
        // is live and uniquely ours here.
        let node: &mut DoraNode = unsafe { &mut *node_ptr };
        // SAFETY: caller attests `b` is null or a live `bytes` handle.
        // Borrow-only — read the bytes; the `.cb` scope frees the handle.
        let payload: &[u8] = unsafe { super::bytes_buf_as_slice(b) };
        // A length-1 `BinaryArray` carrying the single byte blob (the wire
        // shape `data_bytes()` reads back via `BinaryArray::value(0)`).
        let arr = BinaryArray::from(vec![payload]);
        match node.send_output(DataId::from(id), MetadataParameters::default(), arr) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("cobrust-dora (dora-real): send_output_bytes(\"{id}\") failed: {e:#}");
                -1
            }
        }
    }

    /// REAL `event.send_output_buffer(id, buf)` — bridge a borrowed
    /// `coil::Array` (`coil.Buffer`) into a typed Arrow array and publish it
    /// on the `id` output port via the ambient live `DoraNode` (ADR-0076c
    /// (D)-B-1a; the numeric SIBLING of `send_output(id, &str)`). The
    /// output-id validation against the declared set already happened in the
    /// calling shim. `buf` is BORROWED (read, never freed — the `.cb` scope
    /// owns + drops it once via `__cobrust_coil_buffer_drop`).
    ///
    /// Each `coil::Array` arm collects its (possibly non-contiguous) elements
    /// via `.iter().copied()` into a `Vec<T>` — the same flat collection
    /// `array_repr`/`to_json` use — then builds the matching
    /// `arrow::array::{Float64Array, ...}` (each impls `arrow::array::Array`,
    /// so it satisfies `send_output`'s `data: impl Array` bound directly via
    /// `.to_data()`). Returns 0 on a successful publish, `-1` if no ambient
    /// node is set (a `send_output_buffer` called outside a `run` callback)
    /// or the publish errored.
    ///
    /// # Safety
    ///
    /// `buf` must be null or a live `coil.Buffer` handle (a boxed
    /// `coil::Array`) the caller has not dropped.
    pub(super) unsafe fn send_output_buffer(id: &str, buf: *mut u8) -> i64 {
        let node_ptr = AMBIENT_NODE.with(Cell::get);
        if node_ptr.is_null() {
            eprintln!(
                "cobrust-dora (dora-real): send_output_buffer(\"{id}\", ...) called with no \
                 ambient node — it must run inside a node.run() callback. Output dropped."
            );
            return -1;
        }
        if buf.is_null() {
            eprintln!(
                "cobrust-dora (dora-real): send_output_buffer(\"{id}\", <null buffer>) — nothing \
                 to publish. Output dropped."
            );
            return -1;
        }
        // SAFETY: `run_node` set `AMBIENT_NODE` to a valid `&mut DoraNode`
        // for the duration of THIS callback and clears it on return; we are
        // synchronously inside that window, single-threaded, so the pointer
        // is live and uniquely ours here.
        let node: &mut DoraNode = unsafe { &mut *node_ptr };
        // SAFETY: caller attests `buf` is a live `coil.Buffer` handle.
        // Borrow-only — we read its elements; the `.cb` scope frees it.
        let arr: &coil::Array = unsafe { &*buf.cast::<coil::Array>() };

        // Bridge `coil::Array` → typed Arrow (the OUT half of the bridge —
        // single-sourced in `coil_to_arrow` so the hermetic round-trip test
        // exercises the SAME code) → publish via the ambient node. The
        // `ArrayRef` (`Arc<dyn Array>`) satisfies `send_output`'s
        // `data: impl Array` bound (`.to_data()`).
        let data = coil_to_arrow(arr);
        match node.send_output(DataId::from(id), MetadataParameters::default(), data) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("cobrust-dora (dora-real): send_output_buffer(\"{id}\") failed: {e:#}");
                -1
            }
        }
    }

    /// The OUT half of the `ndarray ↔ arrow` bridge (ADR-0076c (D)-B-1a):
    /// `coil::Array` → a typed `arrow::array::ArrayRef` (`Arc<dyn Array>`)
    /// for the 5 supported dtypes. Each arm collects the (possibly
    /// non-contiguous) elements via `.iter().copied()` into a `Vec<T>` — the
    /// same flat collection `array_repr`/`to_json` use — then builds the
    /// matching `Float64Array`/.../`BooleanArray` (each impls
    /// `arrow::array::Array`) and `Arc`s it. Single-sourced so
    /// `send_output_buffer` AND the hermetic round-trip test share ONE
    /// bridge (a divergence between "what we publish" and "what we test"
    /// would otherwise be possible — F36 discipline). DTYPE-FAITHFUL: an
    /// `Int64` stays `Int64` (no float up-cast), a `Float64` stays `Float64`.
    pub(super) fn coil_to_arrow(arr: &coil::Array) -> dora_node_api::arrow::array::ArrayRef {
        use coil::Array as CArr;
        use std::sync::Arc;
        match arr {
            CArr::Float64(a) => {
                let v: Vec<f64> = a.iter().copied().collect();
                Arc::new(Float64Array::from(v))
            }
            CArr::Float32(a) => {
                let v: Vec<f32> = a.iter().copied().collect();
                Arc::new(Float32Array::from(v))
            }
            CArr::Int64(a) => {
                let v: Vec<i64> = a.iter().copied().collect();
                Arc::new(Int64Array::from(v))
            }
            CArr::Int32(a) => {
                let v: Vec<i32> = a.iter().copied().collect();
                Arc::new(Int32Array::from(v))
            }
            CArr::Bool(a) => {
                let v: Vec<bool> = a.iter().copied().collect();
                Arc::new(BooleanArray::from(v))
            }
        }
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
                data_buffer: None,
                data_bytes: None,
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

    // =================================================================
    // ADR-0076c (D)-B-1a — typed-numeric coil.Buffer round-trip (synthetic
    // build). The HERMETIC ndarray↔arrow bridge round-trip lives in the
    // `dora-real` test module below (it needs arrow); these pin the
    // build-agnostic shim contract on the synthetic default.
    // =================================================================

    /// Reclaim + drop a `coil.Buffer` handle (boxed `coil::Array`) the
    /// `data_buffer` shim handed out — the synthetic test stands in for the
    /// `.cb` scope's `__cobrust_coil_buffer_drop` (cobrust-dora deps coil
    /// `default-features = false`, so coil's cabi drop symbol is not linked
    /// into this crate's test build — the Box reclaim frees the same
    /// allocation the drop shim would).
    ///
    /// # Safety
    /// `buf` must be a non-null `coil.Buffer` handle from
    /// `__cobrust_dora_event_data_buffer` not yet dropped.
    unsafe fn drop_coil_buffer(buf: *mut u8) {
        assert!(!buf.is_null(), "buffer handle must be non-null");
        // SAFETY: caller attests `buf` is a live boxed coil::Array.
        drop(unsafe { Box::from_raw(buf.cast::<coil::Array>()) });
    }

    /// `event.data_buffer()` on the SYNTHETIC build returns the canned
    /// Float64 `[1.0, 2.0, 3.0]` typed buffer (so the `.cb` build/type-check
    /// path resolves the symbol without a broker). Asserts the dtype +
    /// shape + values are the canned numeric payload, then drops it once.
    #[test]
    fn data_buffer_returns_canned_float64_payload() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let event = Box::into_raw(Box::new(DoraEventHandle {
                id: "camera".to_string(),
                data_str: "frame_001".to_string(),
                data_buffer: Some(canned_buffer()),
                data_bytes: Some(canned_bytes()),
            }))
            .cast::<u8>();

            let buf = __cobrust_dora_event_data_buffer(event);
            assert!(
                !buf.is_null(),
                "data_buffer() must return a non-null Buffer"
            );
            // Borrow the boxed coil::Array + assert the canned Float64 shape.
            let arr: &coil::Array = &*buf.cast::<coil::Array>();
            assert!(
                matches!(arr, coil::Array::Float64(_)),
                "canned data_buffer payload must be Float64; got {:?}",
                arr.dtype()
            );
            assert_eq!(arr.shape(), vec![3], "canned payload shape must be [3]");
            assert_eq!(arr.size(), 3, "canned payload size must be 3");
            if let coil::Array::Float64(a) = arr {
                let vals: Vec<f64> = a.iter().copied().collect();
                assert_eq!(
                    vals,
                    vec![1.0_f64, 2.0, 3.0],
                    "canned values must be [1,2,3] (exactly representable)"
                );
            }
            drop_coil_buffer(buf);
            drop(Box::from_raw(event.cast::<DoraEventHandle>()));
        }
    }

    /// `event.data_buffer()` on a `None`-payload event (a non-numeric / Utf8
    /// wire payload — the named ADR-0076c divergence) returns a well-defined
    /// EMPTY Float64 buffer the caller still drops ONCE (never a null the
    /// `.cb` drop schedule would mishandle). Also covers the null-event
    /// fallback.
    #[test]
    fn data_buffer_none_and_null_return_empty_droppable_buffer() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            // (a) a None-payload event → empty buffer.
            let event = Box::into_raw(Box::new(DoraEventHandle {
                id: "cmd".to_string(),
                data_str: "go".to_string(),
                data_buffer: None,
                data_bytes: None,
            }))
            .cast::<u8>();
            let buf = __cobrust_dora_event_data_buffer(event);
            assert!(
                !buf.is_null(),
                "None payload must still yield a non-null (empty) Buffer"
            );
            let arr: &coil::Array = &*buf.cast::<coil::Array>();
            assert_eq!(
                arr.size(),
                0,
                "the None-payload fallback Buffer must be empty"
            );
            assert!(
                matches!(arr, coil::Array::Float64(_)),
                "the empty fallback Buffer is Float64"
            );
            drop_coil_buffer(buf);
            drop(Box::from_raw(event.cast::<DoraEventHandle>()));

            // (b) a null event → empty buffer (defense-in-depth, no UB).
            let buf2 = __cobrust_dora_event_data_buffer(std::ptr::null_mut());
            assert!(
                !buf2.is_null(),
                "null event must yield a non-null (empty) Buffer"
            );
            let arr2: &coil::Array = &*buf2.cast::<coil::Array>();
            assert_eq!(
                arr2.size(),
                0,
                "the null-event fallback Buffer must be empty"
            );
            drop_coil_buffer(buf2);
        }
    }

    /// `event.send_output_buffer` validates against the declared-output set
    /// (BOTH builds): a DECLARED id returns 0 + bumps the count; an
    /// UNDECLARED id fails CLOSED (-1, no count bump) — never a silent drop.
    /// The `buf` is BORROWED (the test still owns + drops it once after).
    #[test]
    fn send_output_buffer_validates_against_declared_outputs() {
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

            // A canned Event (borrowed) + a Buffer to emit (borrowed by the
            // shim — the test owns + drops it once at the end).
            let event = Box::into_raw(Box::new(DoraEventHandle {
                id: "camera".to_string(),
                data_str: "frame_001".to_string(),
                data_buffer: Some(canned_buffer()),
                data_bytes: Some(canned_bytes()),
            }))
            .cast::<u8>();
            let buf = Box::into_raw(Box::new(canned_buffer())).cast::<u8>();

            // Declared output ⇒ 0.
            let oid = alloc_str_buffer("reading");
            assert_eq!(
                __cobrust_dora_event_send_output_buffer(event, oid, buf),
                0,
                "send_output_buffer on a declared output must return 0"
            );
            __cobrust_str_drop(oid);

            // Undeclared output ⇒ -1, fail-closed (no count bump).
            let bad = alloc_str_buffer("redaing");
            assert_eq!(
                __cobrust_dora_event_send_output_buffer(event, bad, buf),
                -1,
                "send_output_buffer on an UNDECLARED output must return -1 (fail closed)"
            );
            __cobrust_str_drop(bad);

            // The shim BORROWED buf (never freed it) — the test frees it ONCE
            // here (mirrors the .cb scope's single `__cobrust_coil_buffer_drop`).
            drop_coil_buffer(buf);
            drop(Box::from_raw(event.cast::<DoraEventHandle>()));
        }
        assert_eq!(
            send_output_count() - before,
            1,
            "only the DECLARED send_output_buffer is captured (undeclared does not bump)"
        );
        reset_dora_globals();
    }

    /// A null `buf` to `send_output_buffer` on a DECLARED output is tolerated
    /// (the synthetic capture reads len=0; no UB) and still counts as a
    /// declared emission attempt — the marker just reports an empty buffer.
    #[test]
    fn send_output_buffer_tolerates_null_buffer() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset_dora_globals();
        let before = send_output_count();
        unsafe {
            let reading = alloc_str_buffer("reading");
            assert_eq!(__cobrust_dora_declare_output(reading), 0);
            __cobrust_str_drop(reading);
            let oid = alloc_str_buffer("reading");
            // Null event + null buffer: declared id ⇒ 0 (synthetic capture
            // reads buffer_len(null) = 0).
            assert_eq!(
                __cobrust_dora_event_send_output_buffer(
                    std::ptr::null_mut(),
                    oid,
                    std::ptr::null_mut()
                ),
                0,
                "a declared send with a null buffer must still return 0 (no UB)"
            );
            __cobrust_str_drop(oid);
        }
        assert_eq!(
            send_output_count() - before,
            1,
            "the declared null-buffer send is counted"
        );
        reset_dora_globals();
    }

    // =================================================================
    // ADR-0076c (D)-B-1b / ADR-0093 Phase 2 — the RAW-BYTES accessor
    // (`event.data_bytes()` / `event.send_output_bytes`) synthetic-build
    // shim contract — the `bytes` siblings of the `data_buffer` tests.
    // =================================================================

    // The `bytes` ABI from libcobrust_stdlib (mint / borrow / free).
    unsafe extern "C" {
        fn __cobrust_bytes_from_raw(ptr: *const u8, len: i64) -> *mut u8;
        fn __cobrust_bytes_ptr(b: *mut u8) -> *const u8;
        fn __cobrust_bytes_len(b: *mut u8) -> i64;
        fn __cobrust_bytes_drop(b: *mut u8);
    }

    /// Read a minted `bytes` handle back into an owned `Vec<u8>` and DROP
    /// it (standing in for the `.cb` scope's `__cobrust_bytes_drop`).
    /// SAFETY: `b` must be a non-null `bytes` handle from
    /// `__cobrust_dora_event_data_bytes` not yet dropped.
    unsafe fn read_bytes_and_drop(b: *mut u8) -> Vec<u8> {
        assert!(!b.is_null(), "bytes handle must be non-null");
        // SAFETY: caller attests `b` is a live `bytes` handle; borrow then
        // free exactly once.
        unsafe {
            let len = __cobrust_bytes_len(b) as usize;
            let out = if len == 0 {
                Vec::new()
            } else {
                let ptr = __cobrust_bytes_ptr(b);
                std::slice::from_raw_parts(ptr, len).to_vec()
            };
            __cobrust_bytes_drop(b);
            out
        }
    }

    /// `event.data_bytes()` on the SYNTHETIC build returns the canned RAW
    /// `b"\x00\xff\x01"` (the BYTE-FIDELITY proof: a `0xff`/`0x00`
    /// round-trips EXACTLY — the raw bytes path is never UTF-8-lossy),
    /// then drops it once.
    #[test]
    fn data_bytes_returns_canned_non_utf8_payload() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let event = Box::into_raw(Box::new(DoraEventHandle {
                id: "camera".to_string(),
                data_str: "frame_001".to_string(),
                data_buffer: Some(canned_buffer()),
                data_bytes: Some(canned_bytes()),
            }))
            .cast::<u8>();
            let b = __cobrust_dora_event_data_bytes(event);
            let got = read_bytes_and_drop(b);
            assert_eq!(
                got,
                vec![0x00_u8, 0xff, 0x01],
                "canned data_bytes must round-trip the non-UTF-8 payload byte-exact"
            );
            drop(Box::from_raw(event.cast::<DoraEventHandle>()));
        }
    }

    /// `event.data_bytes()` on a `None`-payload event (a numeric / Utf8 /
    /// null-bearing wire payload — the named divergence) returns a
    /// well-defined EMPTY `bytes` (len 0) the caller still drops ONCE,
    /// NEVER a silent garbage read (§2.2). Also covers the null-event
    /// fallback.
    #[test]
    fn data_bytes_none_and_null_return_empty_droppable_bytes() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            // (a) a None-bytes event → empty bytes.
            let event = Box::into_raw(Box::new(DoraEventHandle {
                id: "cmd".to_string(),
                data_str: "go".to_string(),
                data_buffer: None,
                data_bytes: None,
            }))
            .cast::<u8>();
            let b = __cobrust_dora_event_data_bytes(event);
            assert!(
                !b.is_null(),
                "None payload must yield a non-null empty bytes"
            );
            assert_eq!(
                __cobrust_bytes_len(b),
                0,
                "the fallback bytes must be empty"
            );
            __cobrust_bytes_drop(b);
            drop(Box::from_raw(event.cast::<DoraEventHandle>()));

            // (b) a null event → empty bytes (defense-in-depth, no UB).
            let b2 = __cobrust_dora_event_data_bytes(std::ptr::null_mut());
            assert!(
                !b2.is_null(),
                "null event must yield a non-null empty bytes"
            );
            assert_eq!(
                __cobrust_bytes_len(b2),
                0,
                "the null-event bytes must be empty"
            );
            __cobrust_bytes_drop(b2);
        }
    }

    /// `event.send_output_bytes` validates against the declared-output set
    /// (BOTH builds): a DECLARED id returns 0 + bumps the count; an
    /// UNDECLARED id fails CLOSED (-1, no count bump) — never a silent drop.
    /// The `b` is BORROWED (the test still owns + drops it once after).
    #[test]
    fn send_output_bytes_validates_against_declared_outputs() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset_dora_globals();
        let before = send_output_count();
        unsafe {
            let reading = alloc_str_buffer("reading");
            assert_eq!(__cobrust_dora_declare_output(reading), 0);
            __cobrust_str_drop(reading);

            let event = Box::into_raw(Box::new(DoraEventHandle {
                id: "camera".to_string(),
                data_str: "frame_001".to_string(),
                data_buffer: None,
                data_bytes: Some(canned_bytes()),
            }))
            .cast::<u8>();
            // A bytes payload to emit (borrowed by the shim — the test owns
            // + drops it once at the end).
            let raw: [u8; 3] = [0x00, 0xff, 0x01];
            let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);

            // Declared output ⇒ 0.
            let oid = alloc_str_buffer("reading");
            assert_eq!(
                __cobrust_dora_event_send_output_bytes(event, oid, b),
                0,
                "send_output_bytes on a declared output must return 0"
            );
            __cobrust_str_drop(oid);

            // Undeclared output ⇒ -1, fail-closed (no count bump).
            let bad = alloc_str_buffer("redaing");
            assert_eq!(
                __cobrust_dora_event_send_output_bytes(event, bad, b),
                -1,
                "send_output_bytes on an UNDECLARED output must return -1 (fail closed)"
            );
            __cobrust_str_drop(bad);

            // The shim BORROWED b (never freed it) — the test frees it ONCE
            // here (mirrors the .cb scope's single `__cobrust_bytes_drop`).
            __cobrust_bytes_drop(b);
            drop(Box::from_raw(event.cast::<DoraEventHandle>()));
        }
        assert_eq!(
            send_output_count() - before,
            1,
            "only the DECLARED send_output_bytes is captured (undeclared does not bump)"
        );
        reset_dora_globals();
    }

    /// A null `b` to `send_output_bytes` on a DECLARED output is tolerated
    /// (the synthetic capture reads len=0; no UB) and still counts as a
    /// declared emission attempt.
    #[test]
    fn send_output_bytes_tolerates_null_bytes() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset_dora_globals();
        let before = send_output_count();
        unsafe {
            let reading = alloc_str_buffer("reading");
            assert_eq!(__cobrust_dora_declare_output(reading), 0);
            __cobrust_str_drop(reading);
            let oid = alloc_str_buffer("reading");
            assert_eq!(
                __cobrust_dora_event_send_output_bytes(
                    std::ptr::null_mut(),
                    oid,
                    std::ptr::null_mut()
                ),
                0,
                "a declared send with a null bytes must still return 0 (no UB)"
            );
            __cobrust_str_drop(oid);
        }
        assert_eq!(
            send_output_count() - before,
            1,
            "the declared null-bytes send is counted"
        );
        reset_dora_globals();
    }

    /// DROP BALANCE — 1000 mint→`data_bytes()`→read→drop cycles. A
    /// double-free / use-after-free would crash here; a leak shows under a
    /// sanitizer run. Each iteration mints a fresh Event-canned bytes, hands
    /// it through `data_bytes()` (which mints a FRESH `.cb` bytes), reads it
    /// byte-exact, and drops it once.
    #[test]
    fn thousand_event_bytes_drop_balance() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            for i in 0..1000u32 {
                let event = Box::into_raw(Box::new(DoraEventHandle {
                    id: "camera".to_string(),
                    data_str: "frame_001".to_string(),
                    data_buffer: None,
                    data_bytes: Some(canned_bytes()),
                }))
                .cast::<u8>();
                let b = __cobrust_dora_event_data_bytes(event);
                let got = read_bytes_and_drop(b);
                assert_eq!(
                    got,
                    vec![0x00_u8, 0xff, 0x01],
                    "event {i}: data_bytes must stay byte-exact across the drop hammer"
                );
                drop(Box::from_raw(event.cast::<DoraEventHandle>()));
            }
        }
    }
}

// =====================================================================
// ADR-0076c (D)-B-1a — HERMETIC ndarray↔arrow bridge round-trip tests.
// These are the UNCONDITIONAL correctness proof for the typed-numeric
// payload bridge (the live `dora_real_node_e2e` Part C is the integration
// cherry; THIS is the bit-faithfulness gate). Gated `feature = "dora-real"`
// because the bridge touches the real `arrow` crate (the synthetic default
// has zero arrow). Each test builds a `coil::Array`, runs it OUT through
// `real::coil_to_arrow` (the SAME bridge `send_output_buffer` publishes
// with), wraps the result as the `ArrowData` an `Event::Input` would carry,
// runs it back IN through `real::decode_arrow_buffer` (the SAME decode
// `data_buffer()` returns), and asserts the round-trip is bit-identical AND
// dtype-faithful (Int64 stays Int64, Float64 stays Float64 — no up-cast).
// =====================================================================
#[cfg(all(test, feature = "dora-real"))]
#[allow(clippy::undocumented_unsafe_blocks)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::float_cmp)]
mod arrow_bridge_tests {
    use super::*;
    use dora_node_api::ArrowData;

    // ADR-0076c B-1b — the bytes drop-balance test calls the
    // `__cobrust_bytes_*` family; anchor the stdlib rlib so those
    // `extern "C"` decls resolve under `cargo test --features dora-real`
    // (the same dev-dep anchor the synthetic test mod uses for Str).
    extern crate cobrust_stdlib;
    #[used]
    static _STDLIB_LINK_ANCHOR: unsafe extern "C" fn() -> *mut u8 =
        cobrust_stdlib::fmt::__cobrust_str_new;

    /// Wrap a `coil::Array` as the `ArrowData` an `Event::Input { data }`
    /// would carry: run the OUT bridge (`coil_to_arrow`) then box it in the
    /// `ArrowData(ArrayRef)` newtype dora hands the recv loop. This is the
    /// exact wire shape `decode_arrow_buffer` reads on a real input.
    fn as_arrow_data(arr: &coil::Array) -> ArrowData {
        ArrowData(real::coil_to_arrow(arr))
    }

    /// Round-trip a `coil::Array` through OUT→wire→IN and return the decoded
    /// `coil::Array` (or `None` if the decode rejected the dtype).
    fn round_trip(arr: &coil::Array) -> Option<coil::Array> {
        let wire = as_arrow_data(arr);
        real::decode_arrow_buffer(&wire)
    }

    /// Float64 `[0.5, 1.5, 2.5]` (exactly-representable — no last-ULP
    /// platform drift) round-trips bit-identically + stays Float64.
    #[test]
    fn round_trip_float64_is_bit_identical() {
        let orig = coil::array_f64(&[0.5, 1.5, 2.5], &[3]).unwrap();
        let back = round_trip(&orig).expect("Float64 must decode");
        assert!(
            matches!(back, coil::Array::Float64(_)),
            "dtype must stay Float64"
        );
        assert_eq!(
            back, orig,
            "Float64 [0.5,1.5,2.5] must round-trip bit-identical"
        );
    }

    /// Float32 `[0.5, 2.5]` round-trips bit-identically + stays Float32
    /// (NOT widened to Float64).
    #[test]
    fn round_trip_float32_stays_float32() {
        let orig = coil::array_f32(&[0.5_f32, 2.5], &[2]).unwrap();
        let back = round_trip(&orig).expect("Float32 must decode");
        assert!(
            matches!(back, coil::Array::Float32(_)),
            "dtype must stay Float32 (no f64 up-cast)"
        );
        assert_eq!(
            back, orig,
            "Float32 [0.5,2.5] must round-trip bit-identical"
        );
    }

    /// Int64 `[1, 2, 3]` round-trips + stays Int64 (NOT up-cast to a float —
    /// the ROUND-TRIP FIDELITY contract).
    #[test]
    fn round_trip_int64_stays_int64() {
        let orig = coil::array_i64(&[1, 2, 3], &[3]).unwrap();
        let back = round_trip(&orig).expect("Int64 must decode");
        assert!(
            matches!(back, coil::Array::Int64(_)),
            "dtype must stay Int64 (NO float up-cast)"
        );
        assert_eq!(back, orig, "Int64 [1,2,3] must round-trip identical");
    }

    /// Int32 `[10, 20, 30]` round-trips + stays Int32.
    #[test]
    fn round_trip_int32_stays_int32() {
        let orig = coil::array_i32(&[10, 20, 30], &[3]).unwrap();
        let back = round_trip(&orig).expect("Int32 must decode");
        assert!(
            matches!(back, coil::Array::Int32(_)),
            "dtype must stay Int32"
        );
        assert_eq!(back, orig, "Int32 [10,20,30] must round-trip identical");
    }

    /// Bool `[true, false, true, true]` round-trips + stays Bool. Bool is
    /// the bit-packed case (BooleanArray has no `.values() -> &[bool]`), so
    /// this proves the `.value(i)` materialisation path.
    #[test]
    fn round_trip_bool_is_bit_packed_safe() {
        let orig = coil::array_bool(&[true, false, true, true], &[4]).unwrap();
        let back = round_trip(&orig).expect("Bool must decode");
        assert!(matches!(back, coil::Array::Bool(_)), "dtype must stay Bool");
        assert_eq!(
            back, orig,
            "Bool [t,f,t,t] must round-trip identical (bit-packed safe)"
        );
    }

    /// An EMPTY array of EACH dtype round-trips to an empty array of the
    /// same dtype (the zero-length null-bitmap / buffer edge — a common
    /// bridge bug).
    #[test]
    fn round_trip_empty_arrays_per_dtype() {
        let f64e = coil::array_f64(&[], &[0]).unwrap();
        assert_eq!(round_trip(&f64e).unwrap(), f64e, "empty Float64");
        let f32e = coil::array_f32(&[], &[0]).unwrap();
        assert_eq!(round_trip(&f32e).unwrap(), f32e, "empty Float32");
        let i64e = coil::array_i64(&[], &[0]).unwrap();
        assert_eq!(round_trip(&i64e).unwrap(), i64e, "empty Int64");
        let i32e = coil::array_i32(&[], &[0]).unwrap();
        assert_eq!(round_trip(&i32e).unwrap(), i32e, "empty Int32");
        let boole = coil::array_bool(&[], &[0]).unwrap();
        assert_eq!(round_trip(&boole).unwrap(), boole, "empty Bool");
    }

    /// NULL BITMAP (REPAIR MAJOR / ADR-0076c §U2 named divergence) — a
    /// null-BEARING `Float64Array` (`[Some(1.0), None, Some(3.0)]`,
    /// `null_count() == 1`) decodes to `None` rather than SILENTLY
    /// fabricating the null slot as the raw buffer value (`0.0`). coil has
    /// no null concept, so the only faithful answer is the empty-buffer
    /// divergence (logged). This is the guard against the silent
    /// null→0 data-alteration the audit flagged: pre-fix this array decoded
    /// to `[1.0, 0.0, 3.0]` with no log; now it is a clean rejected
    /// divergence. (We assert `None`, NOT the fabricated dense values.)
    #[test]
    fn null_bearing_float64_decodes_to_none_not_silent_zero() {
        // `Array` (the trait) is needed in scope for `null_count()` on the
        // concrete `Float64Array` — in the lib path it resolves via the
        // `Arc<dyn Array>` trait object; here the value is concretely typed.
        use dora_node_api::arrow::array::{Array, Float64Array};
        use std::sync::Arc;
        // A REAL null-bearing array: null_count() == 1 (the middle slot).
        let arr = Float64Array::from(vec![Some(1.0_f64), None, Some(3.0)]);
        assert_eq!(
            arr.null_count(),
            1,
            "the fixture must actually carry a null"
        );
        let wire = ArrowData(Arc::new(arr));
        assert!(
            real::decode_arrow_buffer(&wire).is_none(),
            "a null-bearing Float64 array is the named ADR-0076c null-bitmap divergence → decode \
             None (NOT a silent [1.0, 0.0, 3.0] fabrication of the null as 0.0)"
        );
    }

    /// NULL BITMAP (Bool arm) — a null-bearing `BooleanArray`
    /// (`[Some(true), None, Some(false)]`, `null_count() == 1`) decodes to
    /// `None` rather than silently fabricating the null as `false` (the
    /// bit-packed Bool path is the same silent-alteration risk). Asserts the
    /// divergence, not the fabricated `[true, false, false]`.
    #[test]
    fn null_bearing_bool_decodes_to_none_not_silent_false() {
        use dora_node_api::arrow::array::{Array, BooleanArray};
        use std::sync::Arc;
        let arr = BooleanArray::from(vec![Some(true), None, Some(false)]);
        assert_eq!(
            arr.null_count(),
            1,
            "the fixture must actually carry a null"
        );
        let wire = ArrowData(Arc::new(arr));
        assert!(
            real::decode_arrow_buffer(&wire).is_none(),
            "a null-bearing Bool array is the named ADR-0076c null-bitmap divergence → decode \
             None (NOT a silent [true, false, false] fabrication of the null as false)"
        );
    }

    /// CONTROL — a NULL-FREE `Float64Array` built via the
    /// `Vec<Option<f64>>` constructor with ALL `Some(_)` (so
    /// `null_count() == 0`) still round-trips bit-faithfully through the
    /// dense path: the null guard fires ONLY on a real null, never a
    /// false-positive on a validity-buffer-present-but-all-set array.
    #[test]
    fn all_some_float64_round_trips_through_null_guard() {
        use dora_node_api::arrow::array::{Array, Float64Array};
        use std::sync::Arc;
        let arr = Float64Array::from(vec![Some(0.5_f64), Some(1.5), Some(2.5)]);
        assert_eq!(arr.null_count(), 0, "all-Some fixture must have zero nulls");
        let wire = ArrowData(Arc::new(arr));
        let back = real::decode_arrow_buffer(&wire).expect("a null-FREE Float64 must still decode");
        let expect = coil::array_f64(&[0.5, 1.5, 2.5], &[3]).unwrap();
        assert_eq!(
            back, expect,
            "a null-free (all-Some) Float64 must round-trip bit-identical (no false-positive null \
             rejection)"
        );
    }

    /// A Utf8 `StringArray` (a non-numeric wire payload — the named
    /// ADR-0076c divergence) decodes to `None` (so `data_buffer()` hands the
    /// empty-buffer fallback + `data_str()` carries the string). Proves the
    /// decode does NOT mis-decode / panic on the Utf8 case.
    #[test]
    fn utf8_payload_decodes_to_none_divergence() {
        use dora_node_api::IntoArrow;
        use std::sync::Arc;
        // The exact length-1 Utf8 StringArray Phase-A `send_output` publishes.
        let s = "go".to_string().into_arrow();
        let wire = ArrowData(Arc::new(s));
        assert!(
            real::decode_arrow_buffer(&wire).is_none(),
            "a Utf8 payload is the named ADR-0076c divergence → decode None (use data_str)"
        );
    }

    /// The FULL shim path under a real arrow wire: a 1000-event loop where
    /// each iteration boxes a fresh decoded Buffer via the SAME boxing
    /// `data_buffer()` does (`Box::into_raw(coil::Array)`) and drops it via
    /// the SAME `Box::from_raw` reclaim — asserting balanced drops (no leak,
    /// no double-free) over many events (ADR-0076c §4.1 / plan §5 Phase-B
    /// done-means 5: a 1000-event run shows balanced Buffer drops).
    #[test]
    fn thousand_event_buffer_drop_balance() {
        let orig = coil::array_f64(&[0.5, 1.5, 2.5, 3.5], &[4]).unwrap();
        for i in 0..1000 {
            // Decode a fresh Buffer from a fresh wire array (what the recv
            // loop does per Event::Input), box it (what `data_buffer()`
            // returns), then reclaim+drop it (what the `.cb` scope does once
            // via `__cobrust_coil_buffer_drop`). A leak or double-free here
            // would surface under the test allocator / sanitizer.
            let decoded = round_trip(&orig).expect("Float64 decode");
            assert_eq!(decoded, orig, "event {i}: round-trip must stay identical");
            let boxed = Box::into_raw(Box::new(decoded)).cast::<u8>();
            // SAFETY: just boxed above; reclaim exactly once (the drop shim's
            // single-free contract).
            drop(unsafe { Box::from_raw(boxed.cast::<coil::Array>()) });
        }
    }

    // =================================================================
    // ADR-0076c (D)-B-1b / ADR-0093 Phase 2 — the RAW-BYTES Arrow decode
    // (`decode_arrow_bytes`): Binary blob + UInt8 flat-list, byte-fidelity
    // on a non-UTF-8 payload, null-bitmap divergence, empty edge, and the
    // numeric/Utf8 non-bytes divergence. These are the COMPLEMENT of the
    // `decode_arrow_buffer` tests (which DEFER Binary/UInt8).
    // =================================================================

    /// A `Binary` single-row blob carrying a NON-UTF-8 payload
    /// (`[0x00, 0xff, 0x01]`) decodes byte-EXACT (BYTE-FIDELITY: the raw
    /// bytes path never UTF-8-corrupts a `0xff`).
    #[test]
    fn decode_binary_blob_is_byte_exact() {
        use dora_node_api::arrow::array::BinaryArray;
        use std::sync::Arc;
        let payload: &[u8] = &[0x00, 0xff, 0x01];
        let arr = BinaryArray::from(vec![payload]);
        let wire = ArrowData(Arc::new(arr));
        let got = real::decode_arrow_bytes(&wire).expect("Binary must decode");
        assert_eq!(
            got,
            vec![0x00_u8, 0xff, 0x01],
            "a Binary blob must round-trip byte-exact (non-UTF-8 safe)"
        );
    }

    /// A `UInt8` FLAT list (`[10, 0, 255]`) decodes to the same bytes
    /// (`arr.values() -> &[u8]`), byte-exact.
    #[test]
    fn decode_uint8_flat_list_is_byte_exact() {
        use dora_node_api::arrow::array::UInt8Array;
        use std::sync::Arc;
        let arr = UInt8Array::from(vec![10_u8, 0, 255]);
        let wire = ArrowData(Arc::new(arr));
        let got = real::decode_arrow_bytes(&wire).expect("UInt8 must decode");
        assert_eq!(
            got,
            vec![10_u8, 0, 255],
            "a UInt8 flat list must decode byte-exact"
        );
    }

    /// An EMPTY `Binary` array (0 rows) decodes to an EMPTY bytes (no
    /// blob row to read — never an OOB `value(0)` panic).
    #[test]
    fn decode_empty_binary_is_empty_bytes() {
        use dora_node_api::arrow::array::BinaryArray;
        use std::sync::Arc;
        let arr = BinaryArray::from(Vec::<&[u8]>::new());
        let wire = ArrowData(Arc::new(arr));
        let got = real::decode_arrow_bytes(&wire).expect("empty Binary decodes to empty bytes");
        assert!(got.is_empty(), "an empty Binary array → empty bytes");
    }

    /// NULL BITMAP — a null-bearing `Binary` array decodes to `None`
    /// (→ an EMPTY bytes via the shim) rather than silently fabricating a
    /// byte for the null slot (§2.2). Mirrors the `decode_arrow_buffer`
    /// null guard.
    #[test]
    fn null_bearing_binary_decodes_to_none_not_silent() {
        use dora_node_api::arrow::array::{Array, BinaryArray};
        use std::sync::Arc;
        let arr = BinaryArray::from_opt_vec(vec![Some(&[1_u8, 2][..]), None, Some(&[3][..])]);
        assert_eq!(
            arr.null_count(),
            1,
            "the fixture must actually carry a null"
        );
        let wire = ArrowData(Arc::new(arr));
        assert!(
            real::decode_arrow_bytes(&wire).is_none(),
            "a null-bearing Binary array is the named null-bitmap divergence → decode None \
             (NOT a silent byte fabrication for the null)"
        );
    }

    /// A numeric `Float64` payload (a `coil.Buffer` dtype → `data_buffer`'s
    /// job) decodes to `None` for `data_bytes` (NOT mis-read as raw bytes):
    /// the two accessors are COMPLEMENTARY, never overlapping.
    #[test]
    fn numeric_payload_decodes_to_none_for_bytes() {
        use dora_node_api::arrow::array::Float64Array;
        use std::sync::Arc;
        let arr = Float64Array::from(vec![1.0_f64, 2.0, 3.0]);
        let wire = ArrowData(Arc::new(arr));
        assert!(
            real::decode_arrow_bytes(&wire).is_none(),
            "a numeric Float64 payload is NOT raw bytes → data_bytes decodes None (use data_buffer)"
        );
    }

    /// A `Utf8` string payload decodes to `None` for `data_bytes` (use
    /// `data_str`) — proving the bytes accessor does not mis-claim the
    /// string path.
    #[test]
    fn utf8_payload_decodes_to_none_for_bytes() {
        use dora_node_api::IntoArrow;
        use std::sync::Arc;
        let s = "go".to_string().into_arrow();
        let wire = ArrowData(Arc::new(s));
        assert!(
            real::decode_arrow_bytes(&wire).is_none(),
            "a Utf8 payload is NOT raw bytes → data_bytes decodes None (use data_str)"
        );
    }

    /// DROP BALANCE — 1000 decode→mint→read→drop cycles through the FULL
    /// `data_bytes` mint path (`__cobrust_bytes_from_raw` + the stdlib
    /// `bytes` drop), proving balanced drops over many events.
    #[test]
    fn thousand_event_bytes_drop_balance_real() {
        use dora_node_api::arrow::array::BinaryArray;
        use std::sync::Arc;
        unsafe extern "C" {
            fn __cobrust_bytes_from_raw(ptr: *const u8, len: i64) -> *mut u8;
            fn __cobrust_bytes_len(b: *mut u8) -> i64;
            fn __cobrust_bytes_drop(b: *mut u8);
        }
        let payload: &[u8] = &[0x00, 0xff, 0x01];
        for i in 0..1000 {
            let arr = BinaryArray::from(vec![payload]);
            let wire = ArrowData(Arc::new(arr));
            let decoded = real::decode_arrow_bytes(&wire).expect("Binary decode");
            assert_eq!(decoded, vec![0x00_u8, 0xff, 0x01], "event {i}: byte-exact");
            // SAFETY: mint a fresh `.cb` bytes (what `data_bytes()` does),
            // then drop it once (what the `.cb` scope does). A leak /
            // double-free would surface here.
            unsafe {
                let b = __cobrust_bytes_from_raw(decoded.as_ptr(), decoded.len() as i64);
                assert_eq!(__cobrust_bytes_len(b), 3);
                __cobrust_bytes_drop(b);
            }
        }
    }
}
