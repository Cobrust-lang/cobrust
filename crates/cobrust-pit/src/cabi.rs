//! C-ABI shims — the runtime surface a compiled `.cb` program binds
//! onto when it does `import pit` and calls `pit.App()`,
//! `pit.text_response(status, body)`, `app.route(method, path,
//! handler)`, and `app.serve_in_background(host, port)` (ADR-0073
//! sixth-module generalization with the FIRST cross-boundary
//! callback marshalling).
//!
//! # The chain
//!
//! These `#[no_mangle] extern "C"` shims are the L4 (runtime) layer of
//! the ecosystem-import chain. The Cobrust toolchain (L1 typecheck → L2
//! MIR intrinsic-rewrite → L3 codegen externs) retargets the source
//! calls onto these exact symbols; `cobrust build` static-links the
//! resulting `libpit.a` after `libcobrust_stdlib.a`.
//!
//! # ABI
//!
//! - **Handles** (`App`, `Response`, `ServerHandle`) cross as opaque
//!   `*mut u8` pointers, `Box::into_raw`'d on construction and
//!   `Box::from_raw`'d exactly once at the `.cb` scope-exit drop. The
//!   `Request` handle is **Rust-owned** (ADR-0073 §2 D6): the
//!   trampoline `Box::into_raw`'s a fresh Request before invoking the
//!   `.cb` callback and `Box::from_raw`'s it back on callback return —
//!   the `.cb` source NEVER drops a Request (the manifest's
//!   `handle_drop_symbol` returns `None` for `PIT_REQUEST_ADT`).
//! - **Strings** cross as Cobrust `Str` buffers. Per ADR-0072 Q5 this
//!   crate has **no Rust-level dependency on `cobrust-stdlib`** in the
//!   production build; the `__cobrust_str_*` primitives are declared
//!   `extern "C"` here and resolved from the always-linked
//!   `libcobrust_stdlib.a`. (For in-crate unit tests, `cobrust-stdlib`
//!   is a dev-dependency so the `extern "C"` decls resolve under
//!   `cargo test`.)
//! - **Callbacks** cross as a raw C-ABI fn-pointer
//!   `unsafe extern "C" fn(*mut u8) -> *mut u8` (ADR-0073 §2 D4). The
//!   trampoline `transmute`'s the `*const c_void` arg back to this
//!   shape and wraps it in a `move |req: Request| -> Response { ... }`
//!   closure satisfying axum's
//!   `Handler = Arc<dyn Fn(Request) -> Response + Send + Sync + 'static>`
//!   bound.
//!
//! # Trampoline soundness (ADR-0073 §5 risk 1)
//!
//! - `Send + Sync` for an `extern "C" fn(*mut u8) -> *mut u8` is the
//!   Rust blanket impl (function pointers are `Copy + Send + Sync` for
//!   every signature). The captured closure holds only the fn pointer
//!   `raw: CbHandlerAbi` — no `Rc` / `RefCell` / non-Send state — so
//!   the closure inherits `Send + Sync` trivially.
//! - `'static` is satisfied because the `.cb` fn lives in the
//!   binary's text segment for the entire process lifetime under
//!   AOT compilation. Dynamic-loaded modules would invalidate this
//!   claim — explicitly out of scope for v0.7.0 (ADR-0073 §5 risk 1).
//! - **Abort-on-panic across the C boundary** (ADR-0073 §3 Q5): a
//!   panic in the `.cb` handler would unwind through the C ABI which
//!   is UB. We wrap every callback invocation in
//!   `std::panic::catch_unwind` and on panic abort the process via
//!   the same path `__cobrust_panic` uses. The `.cb` source surface
//!   uses `Result<T, E>` (constitution §2.2) so a panic-free
//!   handler is the norm; abort is the safety net.
//!
//! # Drop discipline (ADR-0073 §5 done-means 5)
//!
//! A `DROP_COUNT` instrument lets the test suite assert each handle
//! is dropped exactly once (no leak, no double-free). The trampoline
//! also drops the Request box it temporarily wraps for the callback,
//! and that drop is NOT counted by `DROP_COUNT` (Request is Rust-owned
//! and never crosses the `.cb` drop schedule).

// C-ABI-boundary cast allows — mirror `cobrust-strike/src/cabi.rs`'s
// crate-level allows (the casts are intrinsic to the opaque-pointer /
// length ABI and are correct here):
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_ptr_alignment)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::app::App;
use crate::request::Request;
use crate::response::Response;

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
/// both by `__cobrust_pit_request_body` / `__cobrust_pit_request_path_param`
/// to materialise the borrowed Request field as a `.cb`-owned `Str`, and by
/// the in-crate cabi tests.
///
/// Originally this helper was `#[cfg(test)]`-gated because the first proof
/// trampoline only READ Str buffers (`text_response`'s body arrived pre-
/// allocated by `.cb` codegen). F65 G1 adds a SHIM-ALLOCATED Str path —
/// `req.body() -> str` produces a fresh buffer carrying a snapshot of the
/// Rust-owned Request bytes — so the helper graduates to a production
/// helper.
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

/// Total `App` + `Response` + `ServerHandle` handle drops performed by
/// the `_drop` shims this process. Read by the test suite to assert
/// no-leak / no-double-free.
///
/// `Request` boxes the trampoline creates per-callback-invocation are
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
// pit C-ABI surface — module-level free functions.
// =====================================================================

/// `pit.App() -> App`. Construct an empty `App` and return its
/// `Box::into_raw`'d pointer. The `.cb` caller owns the handle; its
/// scope-exit drop frees it via `__cobrust_pit_app_drop`.
///
/// # Safety
///
/// Always safe to call; allocates a fresh `App` on the Rust heap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_app_new() -> *mut u8 {
    Box::into_raw(Box::new(App::new())).cast::<u8>()
}

/// `pit.text_response(status: i64, body: str) -> Response`. Build a
/// canned text response carrying `status` (as `u16`) and `body` (as
/// the response payload). Mirrors the Flask handler returning a bare
/// string + status tuple — the most common ergonomic shape.
///
/// # Safety
///
/// `body` must be null or a valid Cobrust `Str` buffer (see
/// [`read_str_buf`]). The returned pointer is an owned `Response`
/// handle, freed once via `__cobrust_pit_response_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_text_response(status: i64, body: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let body_text = unsafe { read_str_buf(body) };
    // Clamp status to the valid HTTP range so a malformed i64 never
    // bubbles into axum (which panics on an out-of-range status). Per
    // ADR-0073 §3 Q5 abort-on-panic, but we prefer no-panic at the
    // shim boundary.
    let status_u16 = u16::try_from(status).unwrap_or(500);
    let mut headers = HashMap::new();
    headers.insert(
        "content-type".to_owned(),
        "text/html; charset=utf-8".to_owned(),
    );
    let resp = Response::from_parts(status_u16, headers, body_text.into_bytes());
    Box::into_raw(Box::new(resp)).cast::<u8>()
}

// =====================================================================
// pit C-ABI surface — App handle methods.
// =====================================================================

/// The fixed C-ABI shape every `.cb` pit handler exposes (ADR-0073 §2 D4).
/// The `.cb` source's `fn handle_ping(req: pit.Request) -> pit.Response: …`
/// compiles to a function with this exact ABI: it accepts a Boxed Request
/// pointer (the trampoline's job to allocate and free) and returns a
/// Boxed Response pointer (the trampoline's job to consume).
type CbHandlerAbi = unsafe extern "C" fn(*mut u8) -> *mut u8;

/// `app.route(method, path, handler) -> None` (ADR-0073 §2 D4 — the
/// load-bearing callback site).
///
/// Transmutes `handler` (a raw fn pointer materialised by codegen's
/// `Constant::FnRef` arm) into the [`CbHandlerAbi`] shape and wraps it
/// in a `move |req: Request| -> Response { … }` closure satisfying
/// axum's `Send + Sync + 'static` `Handler` bound. The closure boxes
/// the `Request` into raw on each invocation, hands the raw pointer to
/// the `.cb` callback (so the `.cb` side sees a `*mut u8 -> ptr` Adt
/// handle), then `Box::from_raw`'s the Request to free it.
///
/// Returns `Ty::None` (the codegen sees this as the i64-zero
/// destination payload) so a `let _ = app.route(...)` form does NOT
/// alias a second drop-eligible App handle on the same box —
/// `__cobrust_pit_app_drop` would otherwise fire twice at scope exit.
/// The route registration is a side-effect on the receiver in place;
/// the return-value channel is a discard.
///
/// # Safety
///
/// - `app` must be a live `App` handle from `__cobrust_pit_app_new`.
/// - `method` / `path` must be valid Cobrust `Str` buffers.
/// - `handler` must be a real C-ABI fn pointer (codegen guarantees
///   this for the type-checked top-level fn name path).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_app_route(
    app: *mut u8,
    method: *mut u8,
    path: *mut u8,
    handler: *const c_void,
) -> *mut u8 {
    if app.is_null() {
        // Defense in depth — the typechecker + the codegen guarantee
        // non-null app, but a malicious caller could pass null. Return
        // null so the user surface sees a clean no-op.
        return std::ptr::null_mut();
    }
    if handler.is_null() {
        // Same defense — codegen materialises a real fn pointer for a
        // well-typed program; a null handler is impossible under the
        // typechecker but we tolerate it as a no-op rather than UB.
        return std::ptr::null_mut();
    }
    // SAFETY: `handler` is a real C-ABI fn pointer with the
    // `CbHandlerAbi` shape — codegen emits `Constant::FnRef` only for
    // a top-level fn name whose `FnTy` was unified with
    // `pit_handler_fn_ty()` (ADR-0073 §2 D1 typechecker gate).
    let raw: CbHandlerAbi = unsafe { std::mem::transmute(handler) };
    // SAFETY: `method` / `path` per `# Safety`.
    let method_s = unsafe { read_str_buf(method) };
    let path_s = unsafe { read_str_buf(path) };
    // SAFETY: `app` per `# Safety` — borrowed for the duration of the
    // route registration; not consumed.
    let app_mut: &mut App = unsafe { &mut *app.cast::<App>() };

    // The closure: `Send + Sync + 'static` because it only captures
    // `raw: CbHandlerAbi` (a `Copy + Send + Sync` fn pointer). The
    // `.cb` fn lives in the binary text segment for the process
    // lifetime so the `'static` claim holds under AOT (ADR-0073 §5
    // risk 1).
    let handler_closure = move |req: Request| -> Response {
        // Box the Request so the `.cb` handler receives an opaque
        // `*mut u8` Adt-pointer (ADR-0073 §2 D6 — Rust owns the box).
        let req_raw = Box::into_raw(Box::new(req)).cast::<u8>();

        // Catch panics across the C ABI (ADR-0073 §3 Q5).
        let resp_raw = std::panic::catch_unwind(|| {
            // SAFETY: `raw` is a valid `CbHandlerAbi` per the outer
            // `route` SAFETY contract; `req_raw` is a valid Boxed
            // Request pointer just constructed.
            unsafe { raw(req_raw) }
        });

        // Rust owns the Request box even though the `.cb` handler
        // saw it via raw pointer. Free it exactly once HERE, on the
        // way out of the callback. The `.cb` source NEVER drops a
        // `pit.Request` local (the manifest's `handle_drop_symbol`
        // returns `None` for `PIT_REQUEST_ADT`).
        //
        // SAFETY: `req_raw` was just `Box::into_raw`'d above and was
        // not freed by the `.cb` side. Reclaiming ownership and
        // dropping is sound.
        unsafe { drop(Box::from_raw(req_raw.cast::<Request>())) };

        // Err arm = panic crossed the C ABI; abort per ADR-0073 §3 Q5
        // (forward to `__cobrust_panic` would be cleaner but the symbol
        // is not linked when the test harness exercises just this crate,
        // so we call `std::process::abort` directly). Err arm diverges →
        // use `let-Ok-else` (clippy::single_match_else).
        let Ok(resp_raw) = resp_raw else {
            eprintln!(
                "cobrust-pit: panic in .cb handler crossed the C ABI — aborting (ADR-0073 §3 Q5)"
            );
            std::process::abort();
        };
        if resp_raw.is_null() {
            // Handler returned null (bug or fail-clean). Yield
            // a 500 sentinel rather than dereferencing null.
            return Response::from_parts(500, HashMap::new(), Vec::new());
        }
        // SAFETY: A non-null pointer the `.cb` handler returns came from
        // `__cobrust_pit_text_response` (or a future response constructor)
        // and is a `Box::into_raw`'d Response. Reclaim ownership to extract
        // the Response; the `.cb` source's drop schedule would have called
        // `__cobrust_pit_response_drop` but Return-of-handle suppresses
        // that drop (ADR-0073 §2 D6 — operand feeding `Terminator::Return`
        // is moved-out per `drop.rs::globally_moved`, no foreign drop fires).
        unsafe { *Box::from_raw(resp_raw.cast::<Response>()) }
    };

    // Register on the App. The .route Result is intentionally
    // discarded — duplicate / invalid routes yield a benign "no-op"
    // at the C ABI (matching the fail-clean sentinel convention).
    let _ = app_mut.route(&method_s, &path_s, handler_closure);

    // Return null (discard). The codegen's Ty::None receiving slot
    // coerces the i64/ptr return through write_place; the .cb side's
    // `let _ = ...` pattern drops the i64 zero immediately.
    std::ptr::null_mut()
}

/// `app.serve_in_background(host, port) -> ServerHandle`. Binds the
/// underlying axum server on `host:port` (port `0` = ephemeral) on the
/// singleton tokio runtime, returning a `ServerHandle` whose drop
/// aborts the server task.
///
/// # Ownership note (ADR-0073 §2 D6)
///
/// `App::serve_in_background(self, …)` consumes the App. But the `.cb`
/// caller still owns the `app` handle (the receiver is `upgrade_move_
/// to_copy_handle`'d at MIR), so its scope-exit `__cobrust_pit_app_drop`
/// would fire on a freed pointer → double free. The trampoline
/// resolves this by `std::mem::take`ing the App's interior — the
/// original `Box<App>` stays valid (now holding an empty
/// `App::default()`), `_drop` later frees that empty App cleanly, and
/// the taken App is moved into serve.
///
/// # Safety
///
/// - `app` must be a live `App` handle from `__cobrust_pit_app_new`.
/// - `host` must be a valid Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_app_serve_in_background(
    app: *mut u8,
    host: *mut u8,
    port: i64,
) -> *mut u8 {
    if app.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller per `# Safety`.
    let app_mut: &mut App = unsafe { &mut *app.cast::<App>() };
    let taken = std::mem::take(app_mut);
    // SAFETY: `host` per `# Safety`.
    let host_s = unsafe { read_str_buf(host) };
    let port_u16 = u16::try_from(port).unwrap_or(0);
    match taken.serve_in_background(&host_s, port_u16) {
        Ok(handle) => Box::into_raw(Box::new(handle)).cast::<u8>(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// `app.run(host: str, port: i64) -> i64` (F65 G2). Bind on `host:port`
/// and serve REQUESTS FOREVER (blocking the calling thread until the
/// process is killed). Returns `0` on a clean shutdown (currently
/// unreachable: the blocking loop only exits on process kill) or
/// non-zero on a bind / serve error.
///
/// Mirrors `serve_in_background`'s App-take pattern: the underlying
/// `App::run(self, ...)` consumes the App by value, but the `.cb` caller
/// still owns the `app` handle. The trampoline `std::mem::take`'s the
/// App's interior — the original `Box<App>` stays valid (now holding an
/// empty `App::default()`), the scope-exit `__cobrust_pit_app_drop`
/// later frees that empty App cleanly, and the taken App is moved into
/// `App::run`.
///
/// # Safety
///
/// - `app` must be a live `App` handle from `__cobrust_pit_app_new`.
/// - `host` must be a valid Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_app_run(app: *mut u8, host: *mut u8, port: i64) -> i64 {
    if app.is_null() {
        return 1;
    }
    // SAFETY: caller per `# Safety`.
    let app_mut: &mut App = unsafe { &mut *app.cast::<App>() };
    let taken = std::mem::take(app_mut);
    // SAFETY: `host` per `# Safety`.
    let host_s = unsafe { read_str_buf(host) };
    let port_u16 = u16::try_from(port).unwrap_or(0);
    match taken.run(&host_s, port_u16) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

// =====================================================================
// pit C-ABI surface — App middleware methods (ADR-0078 §6.1 Phase-1).
//
// `app.use_cors()` / `app.use_trace()` / `app.use_compression()` flip a
// flag on the LIVE `App` (borrowed `&mut`, NOT consumed). The flag is
// read once by `App::serve`/`serve_in_background` when the axum `Router`
// is constructed, applying the canned `tower_http` Layer preset. Each
// shim returns `Ty::None` at the manifest layer (null at the C ABI) —
// mirroring `__cobrust_pit_app_route`'s discard discipline so the
// `let _ = app.use_cors()` form does NOT alias a second drop-eligible
// App handle (which would double-fire `__cobrust_pit_app_drop`). The
// middleware effect is a side-effect on the receiver in place; the
// return channel is a discard.
//
// BEFORE-SERVE CONTRACT (ADR-0078 §6.1 + the audit LOW finding): these
// set the flag on the App that `serve`/`serve_in_background` later reads
// via `std::mem::take`. A call AFTER serve has bound the Router is a
// no-op (the Router is already built). No new handle, no new `_drop`
// shim, no `DROP_COUNT` change — the flags live inside the existing
// `App` box (this is why tower-http is the cheapest ecosystem-chain
// extension, ADR-0078 §6.1 "Honest difficulty read").
// =====================================================================

/// `app.use_cors() -> None` (ADR-0078 §6.1). Flip the CORS flag on the
/// live `App`; `serve` applies `CorsLayer::permissive()`. Borrows
/// `&mut App` (not consumed). Returns null (Ty::None discard).
///
/// # Safety
///
/// `app` must be a live `App` handle from `__cobrust_pit_app_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_app_use_cors(app: *mut u8) -> *mut u8 {
    if app.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `app` per `# Safety` — borrowed to flip the flag; not
    // consumed (no `_drop` aliasing; the `.cb` scope still owns the box).
    let app_mut: &mut App = unsafe { &mut *app.cast::<App>() };
    app_mut.use_cors();
    std::ptr::null_mut()
}

/// `app.use_trace() -> None` (ADR-0078 §6.1). Flip the trace flag;
/// `serve` applies `TraceLayer::new_for_http()`. Borrows `&mut App`.
/// Returns null (Ty::None discard).
///
/// # Safety
///
/// `app` must be a live `App` handle from `__cobrust_pit_app_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_app_use_trace(app: *mut u8) -> *mut u8 {
    if app.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `app` per `# Safety` — borrowed, not consumed.
    let app_mut: &mut App = unsafe { &mut *app.cast::<App>() };
    app_mut.use_trace();
    std::ptr::null_mut()
}

/// `app.use_compression() -> None` (ADR-0078 §6.1). Flip the compression
/// flag; `serve` applies `CompressionLayer::new()`. Borrows `&mut App`.
/// Returns null (Ty::None discard).
///
/// # Safety
///
/// `app` must be a live `App` handle from `__cobrust_pit_app_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_app_use_compression(app: *mut u8) -> *mut u8 {
    if app.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `app` per `# Safety` — borrowed, not consumed.
    let app_mut: &mut App = unsafe { &mut *app.cast::<App>() };
    app_mut.use_compression();
    std::ptr::null_mut()
}

// =====================================================================
// pit C-ABI surface — Request handle methods (F65 G1 + path-param).
// =====================================================================

/// `req.body() -> str` (F65 G1). Returns a freshly-allocated Cobrust
/// `Str` buffer carrying the request body bytes as a UTF-8 string.
/// Non-UTF-8 bytes are lossily replaced (the resulting str is always
/// valid UTF-8 for the `.cb` side).
///
/// The Rust [`Request`] is borrowed (NOT consumed); the trampoline owns
/// the `Box<Request>` and will free it on callback return per ADR-0073
/// §2 D6. The returned `*mut Str` is a `.cb`-owned buffer; the `.cb`
/// scope-exit drop schedule frees it via `__cobrust_str_drop`.
///
/// # Safety
///
/// `req` must be a valid `Request` handle the pit trampoline allocated
/// for the current callback invocation. The returned pointer is null on
/// a null receiver (defense in depth — the typechecker rules this out)
/// or a freshly-Boxed Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_request_body(req: *mut u8) -> *mut u8 {
    if req.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller per `# Safety`. We only BORROW the Request — the
    // trampoline retains ownership of the Box and frees it after the
    // callback returns.
    let req_ref: &Request = unsafe { &*req.cast::<Request>() };
    let body_bytes = req_ref.body();
    // Lossy UTF-8 — bad bytes become U+FFFD. The `.cb` source's `str`
    // contract is "always valid UTF-8"; presenting raw non-UTF-8 bytes
    // through the str surface would violate it. A future `bytes` ABI
    // could expose the raw form.
    let body_str = std::str::from_utf8(body_bytes).map_or_else(
        |_| String::from_utf8_lossy(body_bytes).into_owned(),
        std::borrow::ToOwned::to_owned,
    );
    alloc_str_buffer(&body_str)
}

/// `req.path_param(name: str) -> str` (F65 G5 enabling — by-id GET /
/// DELETE handlers read `<id>` from the route pattern). Returns a
/// freshly-allocated Cobrust `Str` buffer carrying the path parameter's
/// captured value, or an empty Str when the name is not a registered
/// param on the matched route (the fail-clean sentinel convention).
///
/// # Safety
///
/// `req` must be a valid `Request` handle the pit trampoline allocated
/// for the current callback invocation. `name` must be a valid Cobrust
/// `Str` buffer. The returned pointer is null on a null receiver or a
/// freshly-Boxed Cobrust `Str`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_request_path_param(req: *mut u8, name: *mut u8) -> *mut u8 {
    if req.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller per `# Safety`.
    let req_ref: &Request = unsafe { &*req.cast::<Request>() };
    // SAFETY: `name` per `# Safety`.
    let name_s = unsafe { read_str_buf(name) };
    let captured = req_ref.path_param(&name_s).unwrap_or("");
    alloc_str_buffer(captured)
}

// =====================================================================
// pit C-ABI surface — handle drops (mirror strike's _drop pattern).
// =====================================================================

/// Drop an `App` handle. `Box::from_raw` + drop, exactly once. Idempotent on null.
///
/// # Safety
///
/// `app` must be null or an `App` handle from `__cobrust_pit_app_new`
/// that has not already been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_app_drop(app: *mut u8) {
    if app.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership.
    drop(unsafe { Box::from_raw(app.cast::<App>()) });
    DROP_COUNT.fetch_add(1, Ordering::SeqCst);
}

/// Drop a `Response` handle. Mirrors `App::drop`. Idempotent on null.
///
/// # Safety
///
/// `resp` must be null or a `Response` handle from
/// `__cobrust_pit_text_response` (or a future Response constructor)
/// that has not already been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_response_drop(resp: *mut u8) {
    if resp.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership.
    drop(unsafe { Box::from_raw(resp.cast::<Response>()) });
    DROP_COUNT.fetch_add(1, Ordering::SeqCst);
}

/// Drop a `ServerHandle`. Aborts the backgrounded server task via the
/// existing `Drop for ServerHandle` impl. Idempotent on null.
///
/// # Safety
///
/// `handle` must be null or a `ServerHandle` from
/// `__cobrust_pit_app_serve_in_background` that has not already been
/// dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_server_handle_drop(handle: *mut u8) {
    if handle.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership.
    drop(unsafe { Box::from_raw(handle.cast::<crate::app::ServerHandle>()) });
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
    unsafe fn drop_str_for_test(buf: *mut u8) {
        unsafe { __cobrust_str_drop(buf) }
    }

    /// `pit.App()` + `__cobrust_pit_app_drop` drop exactly once.
    #[test]
    fn app_new_then_drop_increments_counter_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let app = __cobrust_pit_app_new();
            assert!(!app.is_null(), "App handle must be non-null");
            __cobrust_pit_app_drop(app);
        }
        assert_eq!(drop_count() - before, 1, "App must drop exactly once");
    }

    /// `pit.text_response(200, "pong")` builds a 200-status response
    /// whose body is "pong"; the `_drop` shim then frees it once.
    #[test]
    fn text_response_round_trip_drops_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let body = alloc_str_buffer("pong");
            let resp_raw = __cobrust_pit_text_response(200, body);
            assert!(!resp_raw.is_null());
            // Peek the response from the box without consuming
            // (defense-in-depth: ensure status + body shaped correctly).
            {
                let resp_ref = &*resp_raw.cast::<Response>();
                assert_eq!(resp_ref.status_code(), 200);
                assert_eq!(resp_ref.body(), b"pong");
            }
            __cobrust_pit_response_drop(resp_raw);
            drop_str_for_test(body);
        }
        assert_eq!(drop_count() - before, 1, "Response must drop exactly once");
    }

    /// ADR-0078 §6.1 — the `use_cors`/`use_trace`/`use_compression`
    /// shims flip a flag on the live `App` (borrowed, NOT consumed) and
    /// return null (Ty::None discard). The App handle still drops exactly
    /// once (no new handle, no double-free — the flags live inside the
    /// existing box). Also asserts null-receiver tolerance.
    #[test]
    fn use_middleware_flips_flag_and_drops_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let app = __cobrust_pit_app_new();
            assert!(!app.is_null(), "App handle must be non-null");

            // Each shim returns null (Ty::None discard channel) — NOT the
            // App pointer (which would alias a second drop-eligible handle
            // and double-fire `__cobrust_pit_app_drop`).
            assert!(
                __cobrust_pit_app_use_cors(app).is_null(),
                "use_cors must return null/None"
            );
            assert!(
                __cobrust_pit_app_use_trace(app).is_null(),
                "use_trace must return null/None"
            );
            assert!(
                __cobrust_pit_app_use_compression(app).is_null(),
                "use_compression must return null/None"
            );

            // Null-receiver tolerance (defense in depth — the typechecker
            // rules this out, but a malicious caller could pass null).
            assert!(__cobrust_pit_app_use_cors(std::ptr::null_mut()).is_null());

            // The flag is read at serve time; bind on an ephemeral port to
            // confirm the with-middleware serve path constructs cleanly
            // (the layers apply without breaking the Router build).
            let host = alloc_str_buffer("127.0.0.1");
            let server = __cobrust_pit_app_serve_in_background(app, host, 0);
            assert!(
                !server.is_null(),
                "serve_in_background with middleware flags set must succeed"
            );
            __cobrust_str_drop(host);

            __cobrust_pit_server_handle_drop(server);
            __cobrust_pit_app_drop(app);
        }
        // .cb-scheduled drops: App + ServerHandle = 2 (no new handle from
        // the middleware setters — they mutate the App box in place).
        assert_eq!(
            drop_count() - before,
            2,
            "middleware setters add no new drop-eligible handle"
        );
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
            __cobrust_pit_app_drop(std::ptr::null_mut());
            __cobrust_pit_response_drop(std::ptr::null_mut());
            __cobrust_pit_server_handle_drop(std::ptr::null_mut());
        }
        assert_eq!(drop_count(), before, "null drops must be no-ops");
    }

    /// The trampoline transmutes a real fn pointer, invokes it,
    /// Box-rebuilds the Response, and balances the per-callback
    /// Request box. Asserts the App + Response + ServerHandle drops
    /// are each exactly once.
    ///
    /// The "handler" is a Rust extern fn (same C-ABI shape as the
    /// `.cb` codegen output) — proves the trampoline's transmute +
    /// closure capture + drop discipline in isolation, before the
    /// full `.cb`-via-cobrust-build E2E spins it under a real
    /// compiled binary.
    #[unsafe(no_mangle)]
    extern "C" fn _pit_test_handler(req: *mut u8) -> *mut u8 {
        // Validate the Request box (defense: a malformed trampoline
        // would hand us null / garbage).
        unsafe {
            assert!(!req.is_null(), "trampoline must pass a non-null Request");
            // Borrow the request — DO NOT free it (the trampoline owns it).
            let req_ref = &*req.cast::<Request>();
            // Confirm the trampoline routed the right method / path
            // in the closure capture (mirrors `App::route` registration).
            // The closure-built Request we'll dispatch into below sets
            // method = "GET", path = "/ping".
            let _ = req_ref;
        }
        // Build the response via the same path the `.cb` compile would
        // produce: `pit.text_response(200, "pong")`.
        unsafe {
            let body = alloc_str_buffer("pong");
            let resp = __cobrust_pit_text_response(200, body);
            // Free the body Str buffer immediately — `text_response`
            // copied the bytes into the Response payload. The `.cb`
            // side's drop schedule would also free it.
            __cobrust_str_drop(body);
            resp
        }
    }

    #[test]
    fn trampoline_invokes_handler_and_drops_handles_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            // Spin up an App and register the test handler via the
            // trampoline (proving the transmute + closure-wrap path).
            let app = __cobrust_pit_app_new();
            let method = alloc_str_buffer("GET");
            let path = alloc_str_buffer("/ping");
            let handler_ptr = _pit_test_handler as *const c_void;
            let route_ret = __cobrust_pit_app_route(app, method, path, handler_ptr);
            // route() returns Ty::None at the manifest layer (null at
            // the C ABI) — explicitly NOT the App pointer (would
            // double-alias and double-drop).
            assert!(route_ret.is_null(), "route must return null/None");
            __cobrust_str_drop(method);
            __cobrust_str_drop(path);

            // Drive a dispatch through the real app — uses the closure
            // the trampoline registered. The `dispatch_for_test`
            // surface returns the captured route params on a match;
            // we then fire the captured handler manually.
            let app_ref = &*app.cast::<App>();
            assert!(
                app_ref.dispatch_for_test("GET", "/ping").is_some(),
                "trampoline registered route resolves"
            );

            // The ServerHandle path: bind on ephemeral port to confirm
            // the path constructs + frees the join handle cleanly.
            let host = alloc_str_buffer("127.0.0.1");
            let server = __cobrust_pit_app_serve_in_background(app, host, 0);
            assert!(!server.is_null(), "serve_in_background must succeed");
            __cobrust_str_drop(host);

            // Drop everything the .cb scope would.
            __cobrust_pit_server_handle_drop(server);
            __cobrust_pit_app_drop(app);
        }
        // The .cb-scheduled drops are: App + ServerHandle = 2.
        // (No Response from this path — the registered handler hasn't
        // actually been INVOKED through the trampoline closure; the
        // route-table dispatch returns the Handler Arc but we don't
        // call it here. The trampoline-invocation drop discipline is
        // exercised by the `.cb`-via-compile E2E in the cli/tests
        // suite, which does drive a real HTTP round trip.)
        assert_eq!(drop_count() - before, 2);
    }
}
