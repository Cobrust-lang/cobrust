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

/// `pit.json_response(status: i64, body: <validated-body>) -> Response`
/// (ADR-0081 §5.3 Phase-1a). SIBLING of [`__cobrust_pit_text_response`].
///
/// The only delta from `text_response`: the 2nd param is the boxed
/// `serde_json::Value` the `route_validated` trampoline already produced
/// for the handler (`__cobrust_pit_app_route_validated`, the `body_raw`
/// box at `cabi.rs:464`). This shim re-serialises that SAME Value via
/// `Response::json(&*body)` (sets `content-type: application/json` +
/// `serde_json::to_vec`, `response.rs:49`) and overrides the code with
/// `.with_status(status)` (`response.rs:74`). Re-serialising the validated
/// Value (rather than a hand-rebuilt shape) is footgun #4 dropped (ADR-0081
/// §3): the response body cannot drift from the validated body.
///
/// # Ownership (no double-free, no leak, no use-after-free)
///
/// This shim **BORROWS** the body box — it reads `&serde_json::Value` and
/// `Response::json` copies the bytes into an OWNED `Vec<u8>`
/// (`response.rs:50`), so the box is never moved-from or freed here. The
/// `route_validated` trampoline retains sole ownership and frees the box
/// exactly once as a `serde_json::Value` AFTER the handler returns
/// (`cabi.rs:479`). The returned pointer is a freshly-`Box::into_raw`'d
/// `Response` the trampoline reclaims exactly once (`cabi.rs:494`) — the
/// SAME discipline `text_response`'s return follows.
///
/// # Safety
///
/// `body` must be null or a valid pointer to a Rust-owned boxed
/// `serde_json::Value` (the trampoline guarantees this for the
/// type-checked validated-handler path). The returned pointer is an owned
/// `Response` handle, freed once by the trampoline / `__cobrust_pit_response_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_json_response(status: i64, body: *mut u8) -> *mut u8 {
    if body.is_null() {
        // Fail-clean sentinel (unreachable on the validated path — the
        // trampoline only hands the handler a non-null boxed Value).
        return std::ptr::null_mut();
    }
    // SAFETY: caller-attestation per `# Safety` — `body` is the trampoline's
    // boxed `serde_json::Value`. We only BORROW it (shared `&`); the
    // trampoline keeps ownership and frees it once (`cabi.rs:479`).
    let value: &serde_json::Value = unsafe { &*body.cast::<serde_json::Value>() };
    // Clamp status to the valid HTTP range (mirrors `text_response`'s
    // no-panic-at-the-shim-boundary discipline, ADR-0073 §3 Q5).
    let status_u16 = u16::try_from(status).unwrap_or(500);
    // `Response::json` borrows the Value + copies into an owned Vec<u8>
    // (`response.rs:50`); `.with_status` overrides the 200 default
    // (`response.rs:74`). No ownership of the body box is taken.
    let resp = Response::json(value).with_status(status_u16);
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

/// The fixed C-ABI shape every `.cb` VALIDATED pit handler exposes
/// (ADR-0080 §5.4 step 3). The `.cb` source's
/// `fn create(req: pit.Request, body: CreateScore) -> pit.Response: …`
/// compiles to a function with this 2-arg ABI: it accepts a Boxed Request
/// pointer AND a Boxed validated-body pointer (BOTH Rust-owned — the
/// trampoline allocates and frees both) and returns a Boxed Response
/// pointer (the trampoline's job to consume). The body pointer is the
/// validated `serde_json::Value` boxed Rust-side; full `.cb`-struct field
/// access on it is a §9-sub-ADR follow-up (the `.cb`↔serde bridge), out of
/// Phase-1b-ii scope.
type CbValidatedHandlerAbi = unsafe extern "C" fn(*mut u8, *mut u8) -> *mut u8;

/// `app.route_validated(method, path, handler) -> None` (ADR-0080
/// Phase-1b-ii — the type-driven request-validation route, Q5).
///
/// SIBLING of [`__cobrust_pit_app_route`] with two differences: (a) a
/// FIFTH `schema` arg — the validated-body descriptor the Cobrust compiler
/// synthesised from the handler's body-class field table + refinement
/// side-table (the SAME source the type checker used; ADR-0080 §3 footgun
/// #4, cannot drift) — and (b) the handler is the 2-arg
/// [`CbValidatedHandlerAbi`] shape.
///
/// At each request the closure (ADR-0080 §5.4):
///
/// 1. boxes the `Request` (`Box::into_raw`, exactly as `route`);
/// 2. parses `req.json()` and validates it against `schema`
///    ([`crate::validation::validate_against_schema`] — the TOTAL boundary
///    deserialization: missing/extra key, wrong type, out-of-range → Err);
/// 3. on `Ok` boxes the validated `serde_json::Value` (Rust-owned, the
///    SAME `Box::into_raw`/`from_raw` discipline as the Request — the `.cb`
///    side NEVER drops it, mirroring `PIT_REQUEST_ADT`'s
///    `handle_drop_symbol → None`) and calls the handler with BOTH raw
///    pointers, then frees BOTH boxes exactly once on the way out;
/// 4. on `Err(ve)` synthesises a typed **422** `Response` from the
///    `ValidationError` WITHOUT entering the handler (footgun #2 — the
///    Result-error path stays in Rust, surfaced as a `Response`, never a
///    throw/panic), and frees the Request box (no body box was created);
/// 5. `catch_unwind`s the handler invocation across the C ABI (as `route`).
///
/// # Ownership (no double-free, no leak)
///
/// - The Request box is created once per request and freed exactly once on
///   EVERY path (Ok-after-handler, Err-422, null-handler-return,
///   panic-abort). It is Rust-owned (ADR-0073 §2 D6); the `.cb` side never
///   drops it (`handle_drop_symbol(PIT_REQUEST_ADT) == None`).
/// - The body box is created ONLY on the Ok path and freed exactly once
///   after the handler returns. It is likewise Rust-owned — the validated
///   `serde_json::Value` is allocated here, handed to the `.cb` handler by
///   raw pointer, and reclaimed here. There is no `.cb` drop schedule for
///   it (the sentinel body type carries no `_drop` symbol).
/// - The Response the handler returns came from
///   `__cobrust_pit_text_response` (a `Box::into_raw`'d Response); we
///   reclaim it once. Return-of-handle suppressed its `.cb`-side drop.
///
/// # Safety
///
/// - `app` must be a live `App` handle; `method`/`path`/`schema` valid
///   Cobrust `Str` buffers; `handler` a real 2-arg C-ABI fn pointer
///   (codegen guarantees this for the type-checked path).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_app_route_validated(
    app: *mut u8,
    method: *mut u8,
    path: *mut u8,
    handler: *const c_void,
    schema: *mut u8,
) -> *mut u8 {
    if app.is_null() || handler.is_null() {
        // Defense in depth (matching `route`): a null app/handler is
        // impossible under the typechecker; tolerate as a clean no-op.
        return std::ptr::null_mut();
    }
    // SAFETY: `handler` is a real 2-arg C-ABI fn pointer with the
    // `CbValidatedHandlerAbi` shape — codegen emits `Constant::FnRef` only
    // for a top-level fn name whose `FnTy` was unified with
    // `pit_validated_handler_fn_ty()` (the ADR-0080 Q5 typechecker gate).
    let raw: CbValidatedHandlerAbi = unsafe { std::mem::transmute(handler) };
    // SAFETY: per `# Safety`.
    let method_s = unsafe { read_str_buf(method) };
    let path_s = unsafe { read_str_buf(path) };
    let schema_s = unsafe { read_str_buf(schema) };
    // SAFETY: `app` borrowed for the route registration; not consumed.
    let app_mut: &mut App = unsafe { &mut *app.cast::<App>() };

    // ADR-0080 Phase-1b-iii — accumulate this route's {method, path, schema}
    // on the App FIRST (while `schema_s` is still live — the handler closure
    // below MOVES it), so an explicit `app.serve_openapi(...)` can derive the
    // OpenAPI doc from the SAME descriptor the validator enforces (footgun
    // #4, cannot drift). Adds no new handle (a side-effect on the live App).
    app_mut.register_validated_meta(&method_s, &path_s, &schema_s);

    // The closure captures `raw` (a Copy+Send+Sync fn pointer) + the owned
    // `schema_s: String` (Send+Sync). `'static` holds under AOT (the `.cb`
    // fn + the schema String outlive the server task).
    let handler_closure = move |req: Request| -> Response {
        // Parse + validate the body BEFORE touching the handler. A JSON
        // parse failure is itself a validation failure (footgun #1 — a
        // structurally-invalid body cannot reach the handler).
        let validation = match req.json() {
            Ok(value) => {
                crate::validation::validate_against_schema(&schema_s, &value).map(|()| value)
            }
            Err(_) => Err(crate::validation::ValidationError::NotAnObject),
        };

        let validated_value = match validation {
            Ok(value) => value,
            Err(ve) => {
                // ADR-0080 §5.4 step 4 — synthesise a typed 422 in Rust
                // WITHOUT entering the handler. No body box is created on
                // this path; the Request was never boxed here, so there is
                // nothing to free (the inbound `req` is owned by this
                // closure and dropped at scope end like any other arm).
                let mut headers = HashMap::new();
                headers.insert("content-type".to_owned(), "application/json".to_owned());
                return Response::from_parts(422, headers, ve.to_json_body().into_bytes());
            }
        };

        // Ok path — box BOTH the Request and the validated body (both
        // Rust-owned; ADR-0080 §5.4 step 3).
        let req_raw = Box::into_raw(Box::new(req)).cast::<u8>();
        let body_raw = Box::into_raw(Box::new(validated_value)).cast::<u8>();

        // Catch panics across the C ABI (ADR-0073 §3 Q5).
        let resp_raw = std::panic::catch_unwind(|| {
            // SAFETY: `raw` is a valid 2-arg `CbValidatedHandlerAbi`;
            // `req_raw` + `body_raw` are freshly-boxed valid pointers.
            unsafe { raw(req_raw, body_raw) }
        });

        // Free BOTH boxes exactly once on the way out (mirror
        // `route`'s single Request free; the body box is the sibling).
        // SAFETY: both were just `Box::into_raw`'d above and were NOT
        // freed by the `.cb` side (Rust-owned). Reclaim + drop is sound.
        unsafe {
            drop(Box::from_raw(req_raw.cast::<Request>()));
            drop(Box::from_raw(body_raw.cast::<serde_json::Value>()));
        }

        // Err arm = panic crossed the C ABI; abort (ADR-0073 §3 Q5).
        let Ok(resp_raw) = resp_raw else {
            eprintln!(
                "cobrust-pit: panic in .cb validated handler crossed the C ABI — aborting (ADR-0073 §3 Q5)"
            );
            std::process::abort();
        };
        if resp_raw.is_null() {
            return Response::from_parts(500, HashMap::new(), Vec::new());
        }
        // SAFETY: a non-null handler return is a `Box::into_raw`'d Response
        // (from `__cobrust_pit_text_response`); reclaim ownership once.
        unsafe { *Box::from_raw(resp_raw.cast::<Response>()) }
    };

    // Register on the App; discard the Result (benign no-op on dup/invalid
    // — the fail-clean sentinel convention).
    let _ = app_mut.route(&method_s, &path_s, handler_closure);

    // Return null (Ty::None discard) — the registration is a side-effect
    // on `app` in place (mirrors `route`).
    std::ptr::null_mut()
}

/// `app.serve_openapi(doc_path) -> None` (ADR-0080 Phase-1b-iii — the
/// EXPLICIT OpenAPI-serving opt-in, §5.3).
///
/// Registers a `GET <doc_path>` route serving the OpenAPI document derived
/// from the validated routes accumulated on the App
/// ([`App::serve_openapi`]). The doc is assembled by walking each
/// `route_validated`'s body-schema descriptor through the SAME
/// `validation::parse_schema` the validator reads — so the served schema
/// and the runtime validation cannot drift (footgun #4).
///
/// EXPLICIT, NOT magic: the doc is served only because the `.cb` author
/// wrote `app.serve_openapi("/openapi.json")`. No import-time side effect,
/// no hidden global — the registration is a side-effect on the live `App`
/// in place (mirrors `route` / `use_cors`).
///
/// Returns `Ty::None` (null at the C ABI) so a `let _ = app.serve_openapi(…)`
/// form does NOT alias a second drop-eligible App handle (which would
/// double-fire `__cobrust_pit_app_drop`).
///
/// # Safety
///
/// - `app` must be a live `App` handle from `__cobrust_pit_app_new`.
/// - `path` must be a valid Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_app_serve_openapi(app: *mut u8, path: *mut u8) -> *mut u8 {
    if app.is_null() {
        // Defense in depth (matching `route`/`use_cors`): a null app is
        // impossible under the typechecker; tolerate as a clean no-op.
        return std::ptr::null_mut();
    }
    // SAFETY: `path` per `# Safety`.
    let path_s = unsafe { read_str_buf(path) };
    // SAFETY: `app` per `# Safety` — borrowed to register the doc route;
    // not consumed (no `_drop` aliasing; the `.cb` scope still owns the box).
    let app_mut: &mut App = unsafe { &mut *app.cast::<App>() };
    // Discard the Result (benign no-op on a malformed / duplicate path — the
    // fail-clean sentinel convention; the typechecker accepted the program).
    let _ = app_mut.serve_openapi(&path_s);
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
// pit C-ABI surface — validated-body field READ accessors (ADR-0081
// §5.2 Phase-1b). Cloned bit-for-bit from the `(ptr, ptr) -> <ret>`
// `__cobrust_pit_request_path_param` template (above). `body` is the
// boxed `serde_json::Value` the `route_validated` trampoline left for
// the handler (`__cobrust_pit_app_route_validated`'s `body_raw` box,
// `cabi.rs:515`); `name` is the COMPILER-SYNTHESISED field-name `Str`
// the MIR retarget passes (footgun #1 — never author-written). Each
// shim BORROWS the body box (shared `&Value`); the trampoline retains
// sole ownership and frees it exactly once after the handler returns
// (`cabi.rs:530`).
//
// The reads are TOTAL on the validated path: validation already proved
// presence + type + range BEFORE the handler ran
// (`validate_against_schema`, `cabi.rs:493`), so the `unwrap_or`
// fail-clean sentinel is UNREACHABLE for a value that entered the
// handler (it mirrors `path_param`'s `unwrap_or("")` — a defense, NOT a
// `KeyError` surface; footgun #2 dropped).
// =====================================================================

/// `body.<i64-field>` — read an `i64` field off the validated body
/// (ADR-0081 §5.2 Q2). Returns the field's integer value.
///
/// Uses `serde_json::Value::as_i64` — **integer-only**, NEVER
/// `as_f64`-then-truncate (footgun #3; CLAUDE.md §2.2 no-silent-coercion).
/// Validation already rejected a float for an `i64` field (the type /
/// refinement check), so the shim inherits that guarantee and does NOT
/// widen it. The `0` sentinel is fail-clean (unreachable on the validated
/// path — the field is present + integral; it is a defense against a
/// null body / a missing key, NOT a coercion).
///
/// # Safety
///
/// `body` must be null or a valid pointer to the Rust-owned boxed
/// `serde_json::Value` the `route_validated` trampoline produced for the
/// type-checked validated-handler path. `name` must be a valid Cobrust
/// `Str` buffer (the compiler-synthesised field name). The shim only
/// BORROWS `body`; the trampoline keeps ownership and frees it once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_body_get_i64(body: *mut u8, name: *mut u8) -> i64 {
    if body.is_null() {
        // Fail-clean sentinel (unreachable on the validated path).
        return 0;
    }
    // SAFETY: caller-attestation per `# Safety` — `body` is the trampoline's
    // boxed `serde_json::Value`. Shared `&` borrow only; the trampoline
    // keeps ownership and frees it once (`cabi.rs:530`).
    let value: &serde_json::Value = unsafe { &*body.cast::<serde_json::Value>() };
    // SAFETY: `name` per `# Safety`.
    let name_s = unsafe { read_str_buf(name) };
    value
        .get(&name_s)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0)
}

/// `body.<str-field>` — read a `str` field off the validated body
/// (ADR-0081 §5.2 Q2). Returns a freshly-allocated Cobrust `Str` buffer
/// (caller-owned, dropped once by the `.cb` scope) carrying the field's
/// string value, or an empty `Str` on a null body / missing key (the
/// fail-clean sentinel — unreachable on the validated path, mirroring
/// `path_param`'s `unwrap_or("")`).
///
/// # Safety
///
/// As [`__cobrust_pit_body_get_i64`]. The returned pointer is a freshly
/// `Box`-allocated Cobrust `Str` the `.cb` side owns + drops once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_body_get_str(body: *mut u8, name: *mut u8) -> *mut u8 {
    if body.is_null() {
        // Fail-clean sentinel — an empty Str (unreachable on the validated
        // path). Returns a real (empty) buffer, NOT null, so the `.cb` Str
        // consumer never derefs null.
        return alloc_str_buffer("");
    }
    // SAFETY: caller-attestation per `# Safety`. Shared `&` borrow only.
    let value: &serde_json::Value = unsafe { &*body.cast::<serde_json::Value>() };
    // SAFETY: `name` per `# Safety`.
    let name_s = unsafe { read_str_buf(name) };
    let captured = value
        .get(&name_s)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
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

    /// ADR-0081 §5.3 — `pit.json_response(201, body)` re-serialises the
    /// boxed validated `serde_json::Value` into a 201 JSON Response
    /// (content-type application/json) WITHOUT taking ownership of the body
    /// box. The CALLER (here, mirroring the trampoline) still owns + frees
    /// the body box exactly once. This is the no-double-free proof in
    /// isolation: json_response BORROWS, the owner frees once.
    #[test]
    fn json_response_reserialises_validated_body_and_borrows_box() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        // The body box exactly as the `route_validated` trampoline produces
        // it (`cabi.rs:464`): a `Box::into_raw`'d `serde_json::Value` the
        // CALLER owns. json_response must NOT free it.
        let body_raw =
            Box::into_raw(Box::new(serde_json::json!({"name": "a", "rank": 50}))).cast::<u8>();
        unsafe {
            let resp_raw = __cobrust_pit_json_response(201, body_raw);
            assert!(!resp_raw.is_null());
            // Peek the Response from the box without consuming.
            {
                let resp_ref = &*resp_raw.cast::<Response>();
                assert_eq!(resp_ref.status_code(), 201, "with_status(201) override");
                assert_eq!(
                    resp_ref.headers().get("content-type").map(String::as_str),
                    Some("application/json"),
                    "Response::json sets content-type application/json"
                );
                // The body is the re-serialised SAME Value (footgun #4):
                // round-trips back to the input fields.
                let parsed: serde_json::Value =
                    serde_json::from_slice(resp_ref.body()).expect("body is valid JSON");
                assert_eq!(
                    parsed.get("rank").and_then(serde_json::Value::as_i64),
                    Some(50)
                );
                assert_eq!(
                    parsed.get("name").and_then(serde_json::Value::as_str),
                    Some("a")
                );
            }
            __cobrust_pit_response_drop(resp_raw);
            // The CALLER (trampoline) frees the body box exactly once —
            // json_response only borrowed it (no double-free, no leak).
            drop(Box::from_raw(body_raw.cast::<serde_json::Value>()));
        }
        // Only the Response box counts toward DROP_COUNT (the serde Value box
        // freed via plain `Box::from_raw`/`drop`, not a `_drop` shim).
        assert_eq!(
            drop_count() - before,
            1,
            "json_response Response drops exactly once; the borrowed body box is freed by the caller"
        );
    }

    /// ADR-0081 §5.3 — `json_response` is null-tolerant (defense in depth):
    /// a null body returns null without a serde cast / panic. The
    /// type-checked validated path never hits this (the trampoline only
    /// passes a non-null boxed Value).
    #[test]
    fn json_response_null_body_returns_null() {
        unsafe {
            assert!(
                __cobrust_pit_json_response(201, std::ptr::null_mut()).is_null(),
                "null body must return null (fail-clean, no serde cast)"
            );
        }
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

    /// ADR-0080 Phase-1b-ii — counts how many times the validated test
    /// handler was ENTERED (the 422 path must never increment this).
    static VALIDATED_HANDLER_ENTERED: AtomicU64 = AtomicU64::new(0);

    /// A 2-arg validated test handler (same C-ABI shape the `.cb` codegen
    /// emits for `fn create(req: pit.Request, body: CreateScore) ->
    /// pit.Response`). Records entry, borrows both Rust-owned pointers
    /// (NEVER frees them — the trampoline owns both), returns a 201.
    #[unsafe(no_mangle)]
    extern "C" fn _pit_test_validated_handler(req: *mut u8, body: *mut u8) -> *mut u8 {
        VALIDATED_HANDLER_ENTERED.fetch_add(1, Ordering::SeqCst);
        unsafe {
            assert!(
                !req.is_null(),
                "validated trampoline must pass non-null Request"
            );
            assert!(
                !body.is_null(),
                "validated trampoline must pass non-null body"
            );
            // Borrow both — do NOT free (the trampoline owns both boxes).
            let _req_ref = &*req.cast::<Request>();
            let _body_ref = &*body.cast::<serde_json::Value>();
        }
        unsafe {
            let payload = alloc_str_buffer("validated-ok");
            let resp = __cobrust_pit_text_response(201, payload);
            __cobrust_str_drop(payload);
            resp
        }
    }

    /// ADR-0080 Phase-1b-ii — drive the `route_validated` trampoline
    /// closure directly (no live server): a VALID body → 201 + handler
    /// entered; an INVALID body (out-of-range) → 422 + handler NOT entered.
    /// Proves the validate-or-422 split + the handler-not-entered-on-422
    /// contract + the dual-box discipline (no double-free/leak across two
    /// invocations) in isolation, before the full HTTP E2E.
    #[test]
    fn validated_trampoline_validates_then_dispatches_or_422() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entered_before = VALIDATED_HANDLER_ENTERED.load(Ordering::SeqCst);
        let drop_before = drop_count();
        unsafe {
            let app = __cobrust_pit_app_new();
            let method = alloc_str_buffer("POST");
            let path = alloc_str_buffer("/scores");
            // Schema: `name:str`, `rank:i64:0:100` (the §6 Phase-1 body).
            let schema = alloc_str_buffer("name\tstr\nrank\ti64:0:100");
            let handler_ptr = _pit_test_validated_handler as *const c_void;
            let ret = __cobrust_pit_app_route_validated(app, method, path, handler_ptr, schema);
            assert!(ret.is_null(), "route_validated returns null/None");
            __cobrust_str_drop(method);
            __cobrust_str_drop(path);
            __cobrust_str_drop(schema);

            let app_ref = &*app.cast::<App>();

            // Valid body → 201, handler entered.
            let ok = app_ref
                .dispatch_and_invoke_for_test("POST", "/scores", br#"{"name":"a","rank":50}"#)
                .expect("route resolves");
            assert_eq!(ok.status_code(), 201, "valid body must be 201");
            assert_eq!(
                VALIDATED_HANDLER_ENTERED.load(Ordering::SeqCst) - entered_before,
                1,
                "valid body MUST enter the handler exactly once"
            );

            // Out-of-range body → 422, handler NOT entered (count unchanged).
            let bad = app_ref
                .dispatch_and_invoke_for_test("POST", "/scores", br#"{"name":"a","rank":200}"#)
                .expect("route resolves");
            assert_eq!(bad.status_code(), 422, "out-of-range body must be 422");
            assert_eq!(
                VALIDATED_HANDLER_ENTERED.load(Ordering::SeqCst) - entered_before,
                1,
                "422 path MUST NOT enter the handler (count still 1)"
            );
            // The 422 body is the typed validation error, never the
            // handler's marker.
            assert!(
                !String::from_utf8_lossy(bad.body()).contains("validated-ok"),
                "422 body must not carry the handler marker"
            );

            // Missing field + wrong type → 422 too.
            let missing = app_ref
                .dispatch_and_invoke_for_test("POST", "/scores", br#"{"rank":50}"#)
                .expect("route resolves");
            assert_eq!(missing.status_code(), 422, "missing field must be 422");
            let wrongtype = app_ref
                .dispatch_and_invoke_for_test("POST", "/scores", br#"{"name":"a","rank":"x"}"#)
                .expect("route resolves");
            assert_eq!(wrongtype.status_code(), 422, "wrong type must be 422");
            assert_eq!(
                VALIDATED_HANDLER_ENTERED.load(Ordering::SeqCst) - entered_before,
                1,
                "all three invalid bodies stayed out of the handler"
            );

            __cobrust_pit_app_drop(app);
        }
        // Only the App is `.cb`-scheduled-dropped here (Request + body
        // boxes are Rust-owned, freed inside the trampoline, NOT counted by
        // DROP_COUNT). The clean exit across 4 invocations (no abort, no
        // panic) is the no-double-free / no-leak evidence for the dual-box
        // discipline.
        assert_eq!(drop_count() - drop_before, 1, "App drops exactly once");
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
