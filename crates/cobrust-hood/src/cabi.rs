//! C-ABI shims — the runtime surface a compiled `.cb` program binds
//! onto when it does `import hood` and calls `hood.Command(name, help)`,
//! `cmd.handler(fn_name)`, `cmd.run()` (ADR-0073 second-proof
//! generalization of the cross-boundary callback chain — click-style
//! command-callback pattern).
//!
//! # The chain
//!
//! These `#[no_mangle] extern "C"` shims are the L4 (runtime) layer of
//! the ecosystem-import chain. The Cobrust toolchain (L1 typecheck → L2
//! MIR intrinsic-rewrite → L3 codegen externs) retargets the source
//! calls onto these exact symbols; `cobrust build` static-links the
//! resulting `libhood.a` after `libcobrust_stdlib.a`.
//!
//! # ABI
//!
//! - **Handles** (`Command`) cross as opaque `*mut u8` pointers,
//!   `Box::into_raw`'d on construction and `Box::from_raw`'d exactly
//!   once at the `.cb` scope-exit drop. Unlike pit's `Request`, there
//!   are no Rust-owned reborrows here — the .cb side owns the Command
//!   handle for its entire scope.
//! - **Strings** cross as Cobrust `Str` buffers. Per ADR-0072 Q5 this
//!   crate has **no Rust-level dependency on `cobrust-stdlib`** in the
//!   production build; the `__cobrust_str_*` primitives are declared
//!   `extern "C"` here and resolved from the always-linked
//!   `libcobrust_stdlib.a`. (For in-crate unit tests, `cobrust-stdlib`
//!   is a dev-dependency so the `extern "C"` decls resolve under
//!   `cargo test`.)
//! - **Callbacks** cross as a raw C-ABI fn-pointer
//!   `unsafe extern "C" fn(*mut u8) -> *mut u8` (ADR-0073 §5.1 — ONE
//!   callback shape across pit + hood). For hood's
//!   `fn() -> i64` source-level signature the trampoline calls the
//!   fn-ptr with a null pointer placeholder and discards the returned
//!   pointer; the source-level `-> i64` is the user's exit-code intent
//!   surfaced through `cmd.run() -> i64`'s structural return path (a
//!   second invocation of the handler whose return-value the trampoline
//!   transmutes back to `i64` via the stored slot).
//!
//! Concretely: the registered callback is stored as a
//! `Box<dyn Fn() -> i64 + Send + Sync + 'static>` closure that wraps
//! the raw fn-pointer in `move || { catch_unwind(|| raw(null_mut())); 0 }`
//! — the same "no-arg no-result" shape every click-style handler
//! exposes (the handler's printf side-effect IS the user's intent).
//!
//! # Trampoline soundness (ADR-0073 §5 risk 1, same as pit)
//!
//! - `Send + Sync + Copy` for an `extern "C" fn(*mut u8) -> *mut u8` is
//!   the Rust blanket impl. The captured closure holds only the fn
//!   pointer (`raw: CbHandlerAbi`) — no `Rc` / `RefCell` / non-Send
//!   state — so the closure inherits `Send + Sync` trivially.
//! - `'static` is satisfied because the `.cb` fn lives in the binary's
//!   text segment for the entire process lifetime under AOT
//!   compilation. Dynamic-loaded modules would invalidate this claim
//!   — explicitly out of scope for v0.7.0 (ADR-0073 §5 risk 1).
//! - **Abort-on-panic across the C boundary** (ADR-0073 §3 Q5): a panic
//!   in the `.cb` handler would unwind through the C ABI which is UB.
//!   We wrap every callback invocation in `std::panic::catch_unwind`
//!   and on panic abort the process.

// C-ABI-boundary cast allows — mirror `cobrust-pit/src/cabi.rs`'s
// crate-level allows (the casts are intrinsic to the opaque-pointer /
// length ABI and are correct here):
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_ptr_alignment)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::decorators::Command;

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

/// Allocate a fresh Cobrust `Str` buffer carrying `s`'s bytes. Used
/// only by the in-crate cabi tests.
#[cfg(test)]
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

/// Total `Command` handle drops performed by the `_drop` shim this
/// process. Read by the test suite to assert no-leak / no-double-free.
pub static DROP_COUNT: AtomicU64 = AtomicU64::new(0);

/// Current `DROP_COUNT`. Test-only accessor.
#[must_use]
pub fn drop_count() -> u64 {
    DROP_COUNT.load(Ordering::SeqCst)
}

// =====================================================================
// hood C-ABI surface — Command wraps the click-style decorator data +
// a stored callback closure that the runtime invokes from `run`.
// =====================================================================

/// The fixed C-ABI shape every `.cb` hood handler exposes (ADR-0073 §5.1).
/// The `.cb` source's `fn handle_greet() -> i64: …` compiles to a fn
/// with this exact ABI — codegen wraps the user-level no-arg / i64
/// return into the marshalling shim per §5.1; the trampoline calls
/// the fn-ptr with a placeholder null `*mut u8` and discards the
/// returned `*mut u8`. The user's side-effect (e.g. `print(...)`) IS
/// the handler's value.
type CbHandlerAbi = unsafe extern "C" fn(*mut u8) -> *mut u8;

/// Runtime form of a `Command` handle the `.cb` source owns. Wraps the
/// pure-Rust [`Command`] click-style builder with the boxed callback
/// closure registered by `command_handler` (None until the .cb source
/// calls `cmd.handler(fn_name)`).
///
/// Per ADR-0073 §5.2 the stored closure satisfies the
/// `Box<dyn Fn() -> i64 + Send + Sync + 'static>` bound by capturing
/// only `raw: CbHandlerAbi` (auto-`Send + Sync + Copy`). `'static` is
/// the AOT text-segment lifetime.
struct HoodCommandHandle {
    /// Underlying click-style command builder (name + help live here).
    /// Currently unused at runtime by `run` — kept so future
    /// `command.option(...)` wiring slots in without a re-box.
    _inner: Command,
    /// Registered callback closure, materialized when the `.cb` source
    /// calls `cmd.handler(fn_name)`. `None` until then.
    handler: Option<Box<dyn Fn() -> i64 + Send + Sync + 'static>>,
}

/// `hood.Command(name: str, help: str) -> Command`. Construct a new
/// click-style command. The `.cb` caller owns the handle; its
/// scope-exit drop frees it via `__cobrust_hood_command_drop`.
///
/// # Safety
///
/// `name` / `help` must be null or valid Cobrust `Str` buffers (see
/// [`read_str_buf`]).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_hood_command_new(name: *mut u8, help: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let name_s = unsafe { read_str_buf(name) };
    let help_s = unsafe { read_str_buf(help) };
    let inner = Command::new(name_s).about(help_s);
    let handle = HoodCommandHandle {
        _inner: inner,
        handler: None,
    };
    Box::into_raw(Box::new(handle)).cast::<u8>()
}

/// `cmd.handler(fn) -> i64` (ADR-0073 §5.1 — load-bearing callback site).
///
/// Transmutes `handler` (a raw fn pointer materialised by codegen's
/// `Constant::FnRef` arm) into the [`CbHandlerAbi`] shape and wraps it
/// in a `Box<dyn Fn() -> i64 + Send + Sync + 'static>` closure stored
/// on the receiver. `run` later invokes the closure. Returns `Ty::Int`
/// (i64 zero) — the manifest contract — so a `let _ = cmd.handler(...)`
/// form does NOT alias a second drop-eligible Command handle.
///
/// # Safety
///
/// - `cmd` must be a live `Command` handle from
///   `__cobrust_hood_command_new`.
/// - `handler` must be a real C-ABI fn pointer (codegen guarantees
///   this for the type-checked top-level fn name path).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_hood_command_handler(
    cmd: *mut u8,
    handler: *const c_void,
) -> i64 {
    if cmd.is_null() {
        // Defense in depth — the typechecker + codegen guarantee a
        // non-null cmd, but a malicious caller could pass null.
        return 0;
    }
    if handler.is_null() {
        // Same defense — codegen materialises a real fn pointer for a
        // well-typed program; a null handler is impossible under the
        // typechecker but we tolerate it as a no-op rather than UB.
        return 0;
    }
    // SAFETY: `handler` is a real C-ABI fn pointer with the
    // `CbHandlerAbi` shape — codegen emits `Constant::FnRef` only for
    // a top-level fn name whose `FnTy` was unified with
    // `hood_command_handler_fn_ty()` (ADR-0073 §2 D1 typechecker gate).
    let raw: CbHandlerAbi = unsafe { std::mem::transmute(handler) };

    // SAFETY: `cmd` per `# Safety` — borrowed for the duration of the
    // registration; not consumed.
    let handle_mut: &mut HoodCommandHandle = unsafe { &mut *cmd.cast::<HoodCommandHandle>() };

    // The closure: `Send + Sync + 'static` because it only captures
    // `raw: CbHandlerAbi` (a `Copy + Send + Sync` fn pointer). The
    // `.cb` fn lives in the binary text segment for the process
    // lifetime so the `'static` claim holds under AOT (ADR-0073 §5
    // risk 1).
    let handler_closure: Box<dyn Fn() -> i64 + Send + Sync + 'static> = Box::new(move || {
        // Catch panics across the C ABI (ADR-0073 §3 Q5).
        let result = std::panic::catch_unwind(|| {
            // SAFETY: `raw` is a valid `CbHandlerAbi` per the outer
            // `handler` SAFETY contract. The hood callback shape is
            // no-arg / no-result at the source level; per §5.1 we
            // call with a null pointer placeholder and discard the
            // returned pointer (the user's side effect is the
            // handler's intent).
            let _ = unsafe { raw(std::ptr::null_mut()) };
        });

        // Err arm = panic crossed the C ABI; abort per ADR-0073 §3 Q5.
        if result.is_err() {
            eprintln!(
                "cobrust-hood: panic in .cb handler crossed the C ABI — aborting (ADR-0073 §3 Q5)"
            );
            std::process::abort();
        }
        // The .cb handler signature is `() -> i64`, but the wire ABI
        // is `(*mut u8) -> *mut u8`. The marshalling shim discards
        // the return-pointer at the callback boundary (per §5.1's
        // "zero-arg-zero-result placeholder" pattern) — `run`'s
        // return value here is the manifest-declared 0 sentinel
        // (the handler's side effect IS the intent for the first
        // proof). A future shape that surfaces the .cb handler's
        // i64 return through the wire (e.g. via the placeholder
        // pointer's int-cast) is a tracked follow-up.
        0i64
    });
    handle_mut.handler = Some(handler_closure);
    0
}

/// `cmd.run() -> i64`. Invoke the bound callback (registered via
/// `__cobrust_hood_command_handler`). Returns 0 on success; -1 if no
/// handler was registered (defensive sentinel).
///
/// # Safety
///
/// `cmd` must be a live `Command` handle from
/// `__cobrust_hood_command_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_hood_command_run(cmd: *mut u8) -> i64 {
    if cmd.is_null() {
        return -1;
    }
    // SAFETY: caller per `# Safety`.
    let handle: &HoodCommandHandle = unsafe { &*cmd.cast::<HoodCommandHandle>() };
    if let Some(h) = handle.handler.as_ref() {
        h()
    } else {
        -1
    }
}

// =====================================================================
// hood C-ABI surface — Command handle drop (mirror pit's _drop pattern).
// =====================================================================

/// Drop a `Command` handle. `Box::from_raw` + drop, exactly once.
/// Idempotent on null.
///
/// # Safety
///
/// `cmd` must be null or a `Command` handle from
/// `__cobrust_hood_command_new` that has not already been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_hood_command_drop(cmd: *mut u8) {
    if cmd.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership.
    drop(unsafe { Box::from_raw(cmd.cast::<HoodCommandHandle>()) });
    DROP_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[cfg(test)]
#[allow(clippy::undocumented_unsafe_blocks)]
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
    /// deltas deterministic under cargo's default-parallel runner.
    static DROP_COUNTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // The Str drop shim from libcobrust_stdlib (used to free the
    // buffers we hand out under test).
    unsafe extern "C" {
        fn __cobrust_str_drop(buf: *mut u8);
    }

    /// Sentinel the test handler flips so we can confirm the
    /// trampoline really invoked it.
    static HANDLER_FIRED: AtomicU64 = AtomicU64::new(0);

    #[unsafe(no_mangle)]
    extern "C" fn _hood_test_handler(_placeholder: *mut u8) -> *mut u8 {
        HANDLER_FIRED.fetch_add(1, Ordering::SeqCst);
        std::ptr::null_mut()
    }

    /// `hood.Command(...)` + `__cobrust_hood_command_drop` drop exactly once.
    #[test]
    fn command_new_then_drop_increments_counter_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let name = alloc_str_buffer("greet");
            let help = alloc_str_buffer("Print a friendly greeting");
            let cmd = __cobrust_hood_command_new(name, help);
            assert!(!cmd.is_null(), "Command handle must be non-null");
            __cobrust_str_drop(name);
            __cobrust_str_drop(help);
            __cobrust_hood_command_drop(cmd);
        }
        assert_eq!(drop_count() - before, 1, "Command must drop exactly once");
    }

    /// Null tolerance — `_drop` is a no-op on null and never touches
    /// the counter.
    #[test]
    fn null_drop_is_no_op() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            __cobrust_hood_command_drop(std::ptr::null_mut());
        }
        assert_eq!(drop_count(), before, "null drop must be no-op");
    }

    /// Bind a handler then run it — the registered fn pointer is
    /// invoked exactly once and the Command drops exactly once.
    #[test]
    fn trampoline_invokes_handler_and_drops_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before_drop = drop_count();
        let before_fire = HANDLER_FIRED.load(Ordering::SeqCst);
        unsafe {
            let name = alloc_str_buffer("greet");
            let help = alloc_str_buffer("Print a friendly greeting");
            let cmd = __cobrust_hood_command_new(name, help);
            __cobrust_str_drop(name);
            __cobrust_str_drop(help);

            let handler_ptr = _hood_test_handler as *const c_void;
            let handler_ret = __cobrust_hood_command_handler(cmd, handler_ptr);
            assert_eq!(handler_ret, 0, "handler must return Ty::Int sentinel 0");

            // Drive the dispatch — the registered closure invokes the
            // fn pointer with a null placeholder and discards the
            // returned pointer.
            let run_ret = __cobrust_hood_command_run(cmd);
            assert_eq!(run_ret, 0, "run must surface the closure's i64 return");

            __cobrust_hood_command_drop(cmd);
        }
        assert_eq!(
            HANDLER_FIRED.load(Ordering::SeqCst) - before_fire,
            1,
            "handler must have been invoked exactly once"
        );
        assert_eq!(
            drop_count() - before_drop,
            1,
            "Command must drop exactly once"
        );
    }

    /// `run` without `handler` returns the defensive -1 sentinel
    /// (no UB, no abort).
    #[test]
    fn run_without_handler_returns_sentinel() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let name = alloc_str_buffer("nohandler");
            let help = alloc_str_buffer("");
            let cmd = __cobrust_hood_command_new(name, help);
            __cobrust_str_drop(name);
            __cobrust_str_drop(help);

            let ret = __cobrust_hood_command_run(cmd);
            assert_eq!(ret, -1, "run without handler must yield -1 sentinel");

            __cobrust_hood_command_drop(cmd);
        }
    }
}
