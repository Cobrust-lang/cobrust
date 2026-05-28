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
//! # Phase 1 handler registration model
//!
//! Phase 1 ships the explicit registration form `dora.node(handler)` as a
//! module-level free fn taking a callback. The function stores the fn
//! pointer in a process-global slot (Phase 1 supports a single handler;
//! multi-node-per-process is Phase 2 alongside the decorator-form
//! `@dora.node(inputs=..., outputs=...)` desugar — see findings file
//! `f68-dora-phase1-followups.md`). When `node.run()` fires, it reads
//! the global slot, invokes it once with a canned `("camera",
//! "frame_001")` Event, and returns 0.

// C-ABI-boundary cast allows — mirror `cobrust-pit/src/cabi.rs`'s
// crate-level allows (the casts are intrinsic to the opaque-pointer /
// length ABI and are correct here):
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_ptr_alignment)]

use std::ffi::c_void;
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

/// Process-global handler slot (Phase 1 single-node-per-process).
/// `dora.node(handler)` installs into this slot; `node.run()` reads it
/// and dispatches the canned event. Phase 2 will replace this with a
/// per-Node handler vector keyed by input id (`@dora.node(inputs=...)`).
///
/// `AtomicPtr<()>` for `Send + Sync` across the synthetic-runtime
/// boundary; the pointer value IS a `CbHandlerAbi` fn pointer (raw fn
/// pointers `Copy + Send + Sync` so the transmute is sound).
static REGISTERED_HANDLER: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

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

/// `node.run() -> i64`. SYNTHETIC dispatcher: invokes the registered
/// handler exactly once with a canned `("camera", "frame_001")` Event
/// and returns 0. Mirrors what a real dora-rs `EventStream` loop would
/// do for one tick; the Phase 2 sprint replaces this with the real
/// `DoraNode::events().into_iter()` driven loop.
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

    // Allocate the canned Event. Phase 1 ships a single ("camera",
    // "frame_001") tick — the smallest input that proves the chain.
    let event = DoraEventHandle {
        id: "camera".to_string(),
        data_str: "frame_001".to_string(),
    };
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
    // Phase 1 discards the handler return-pointer (mirrors hood's
    // "side-effect IS the intent" pattern). Surface the manifest-declared
    // 0 sentinel so the .cb source's `let _ = node.run()` discards a
    // clean i64.
    0
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
        // Reset the global slot to ensure deterministic state.
        REGISTERED_HANDLER.store(std::ptr::null_mut(), Ordering::SeqCst);
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
        // Reset the global slot + sentinels.
        REGISTERED_HANDLER.store(std::ptr::null_mut(), Ordering::SeqCst);
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
}
