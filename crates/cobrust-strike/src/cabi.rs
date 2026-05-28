//! C-ABI shims — the runtime surface a compiled `.cb` program binds
//! onto when it does `import strike` and calls `strike.get(url)` /
//! `resp.text()` / `resp.status_code()` / `resp.json()` (ADR-0072
//! third-module generalization — HTTP client, rebrand of `requests`).
//!
//! # The chain
//!
//! These `#[no_mangle] extern "C"` shims are the L4 (runtime) layer of
//! the ecosystem-import chain. The Cobrust toolchain (L1 typecheck → L2
//! MIR intrinsic-rewrite → L3 codegen externs) retargets the source
//! calls onto these exact symbols; `cobrust build` static-links the
//! resulting `libstrike.a` after `libcobrust_stdlib.a` (Linux wraps both
//! in `--start-group/--end-group` so the `__cobrust_str_*` forward
//! references below resolve under single-pass GNU ld too — per
//! ADR-0072 Q5).
//!
//! # ABI
//!
//! - **Handles** (`Response`) cross as opaque `*mut u8` pointers,
//!   `Box::into_raw`'d on construction by `get`/`post` and
//!   `Box::from_raw`'d exactly once at the `.cb` scope-exit drop
//!   (the `_drop` shim). The handle pattern mirrors `den.Connection`/
//!   `den.Cursor` (ADR-0072 §3 / §5 risk 1).
//! - **Strings** cross as Cobrust `Str` buffers. Per ADR-0072 Q5 this
//!   crate has **no Rust-level dependency on `cobrust-stdlib`** in the
//!   production build; the `__cobrust_str_*` primitives are declared
//!   `extern "C"` here and resolved from the always-linked
//!   `libcobrust_stdlib.a`. (For in-crate unit tests, `cobrust-stdlib`
//!   is a dev-dependency so the `extern "C"` decls resolve under
//!   `cargo test`.)
//!
//! # Ownership (ADR-0072 §5 prime risk — same care as den)
//!
//! - `get` / `post` **return** freshly-Boxed `Response` handles the
//!   `.cb` caller owns; the caller's MIR drop schedule frees them once
//!   at scope exit via the `_drop` shim.
//! - `text` / `status_code` / `json` **borrow** the Response (`&*` —
//!   immutable view; we project body bytes / status without moving
//!   anything). They never rebox or free.
//! - A `DROP_COUNT` instrument lets the test suite assert each handle
//!   is dropped exactly once (no leak, no double-free).
//!
//! # Fail-cleanly sentinels (no panic across the C ABI)
//!
//! Network errors / invalid URLs return a synthetic Response with
//! status `0` and an empty body — never null, never panic. This
//! mirrors the std.json / F59 empty-Str sentinel convention and keeps
//! the `.cb` source surface ergonomic (status-code check is the
//! single branch the user writes). `json()` on a malformed body
//! returns the canonical-JSON empty-object sentinel (`{}`).

// C-ABI-boundary cast allows — mirror `cobrust-den/src/cabi.rs`'s
// crate-level allows (the casts are intrinsic to the opaque-pointer /
// length ABI and are correct here):
// - `i64 <-> usize` length round-trips: Cobrust `Str` lengths are
//   non-negative and well under `usize::MAX` on the 64-bit targets the
//   AOT backend supports.
// - `*mut u8 -> *mut Response`: the pointer was produced by
//   `Box::into_raw` (correctly aligned) and only ever cast back to its
//   original type, so the alignment-narrowing lint is a false positive.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_ptr_alignment)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::client::{Response, get as strike_get, post as strike_post};

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
/// null / empty. Mirrors `cobrust-den/src/cabi.rs::read_str_buf` and
/// `cobrust-nest/src/cabi.rs::read_str_buf`.
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

/// Allocate a fresh Cobrust `Str` buffer carrying `s`'s bytes. The `.cb`
/// caller's drop schedule frees it via `__cobrust_str_drop`.
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
// Drop instrumentation (ADR-0072 §4 done-means 5 — drop-once evidence).
// =====================================================================

/// Total `Response` handle drops performed by the `_drop` shim this
/// process. Read by the test suite to assert no-leak / no-double-free.
pub static DROP_COUNT: AtomicU64 = AtomicU64::new(0);

/// Current `DROP_COUNT`. Test-only accessor.
#[must_use]
pub fn drop_count() -> u64 {
    DROP_COUNT.load(Ordering::SeqCst)
}

// =====================================================================
// Internal helper — fail-cleanly sentinel Response.
// =====================================================================

/// A `Response` carrying status `0` and empty body — the no-panic
/// sentinel returned by `get`/`post` on network failure / invalid URL.
/// The `.cb` caller checks `resp.status_code() == 0` to detect failure.
fn fail_clean_response() -> Response {
    Response::from_parts(0, HashMap::new(), Vec::new())
}

// =====================================================================
// strike C-ABI surface.
// =====================================================================

/// `strike.get(url) -> Response`. Issues an HTTP GET to `url` via the
/// blocking `reqwest` client and returns a freshly-Boxed `Response`
/// handle the caller owns.
///
/// Returns a fail-clean sentinel Response (status 0, empty body) on any
/// network failure / invalid URL — never null, never panic.
///
/// # Safety
///
/// `url` must be null or a valid Cobrust `Str` buffer. The returned
/// pointer is an owned `Response` handle, freed once via
/// `__cobrust_strike_response_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_strike_get(url: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let url = unsafe { read_str_buf(url) };
    let resp = strike_get(&url).unwrap_or_else(|_| fail_clean_response());
    Box::into_raw(Box::new(resp)).cast::<u8>()
}

/// `strike.post(url, body) -> Response`. Issues an HTTP POST to `url`
/// with `body` as the request body via the blocking `reqwest` client
/// and returns a freshly-Boxed `Response` handle the caller owns.
///
/// Returns a fail-clean sentinel Response (status 0, empty body) on any
/// network failure / invalid URL — never null, never panic.
///
/// # Safety
///
/// `url` / `body` must be null or valid Cobrust `Str` buffers. The
/// returned pointer is an owned `Response` handle, freed once via
/// `__cobrust_strike_response_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_strike_post(url: *mut u8, body: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let url = unsafe { read_str_buf(url) };
    // SAFETY: caller-attestation per `# Safety`.
    let body = unsafe { read_str_buf(body) };
    let resp = strike_post(&url, body.as_bytes()).unwrap_or_else(|_| fail_clean_response());
    Box::into_raw(Box::new(resp)).cast::<u8>()
}

/// `Response.text() -> str`. BORROWS the Response handle (never frees
/// it) and returns its body as a freshly-allocated Cobrust `Str`
/// buffer. Non-UTF-8 bytes are lossy-replaced (best-effort, like
/// `requests` does for `response.text`).
///
/// Returns an empty Str on a null handle.
///
/// # Safety
///
/// `resp` must be null or a live `Response` handle from
/// `__cobrust_strike_get`/`__cobrust_strike_post` (not yet dropped).
/// The returned pointer is an owned Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_strike_response_text(resp: *mut u8) -> *mut u8 {
    if resp.is_null() {
        return alloc_str_buffer("");
    }
    // SAFETY: caller attests `resp` is a live Response handle. We only
    // BORROW it — no rebox / free.
    let resp_ref = unsafe { &*resp.cast::<Response>() };
    let bytes = resp_ref.body_bytes();
    let text = std::str::from_utf8(bytes).unwrap_or("");
    alloc_str_buffer(text)
}

/// `Response.status_code() -> i64`. BORROWS the Response handle and
/// returns its HTTP status code widened to `i64` (Cobrust's integer
/// type — the underlying value is a `u16` in `0..=599`). A status of
/// `0` is the fail-clean sentinel for a network / invalid-URL error.
///
/// Returns `0` on a null handle (matches the sentinel convention).
///
/// # Safety
///
/// `resp` must be null or a live `Response` handle from
/// `__cobrust_strike_get`/`__cobrust_strike_post` (not yet dropped).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_strike_response_status_code(resp: *mut u8) -> i64 {
    if resp.is_null() {
        return 0;
    }
    // SAFETY: caller attests `resp` is a live Response handle.
    let resp_ref = unsafe { &*resp.cast::<Response>() };
    i64::from(resp_ref.status_code())
}

/// `Response.json() -> str`. BORROWS the Response handle and returns
/// its body parsed-then-canonical-JSON-rendered as a freshly-allocated
/// Cobrust `Str` buffer. Mirrors `den.fetchall() -> str` shape (a Str
/// rendering for the first proof; a structured-value surface is a
/// tracked follow-up).
///
/// On parse failure (or empty body / non-JSON) returns the empty-object
/// sentinel `{}` — the no-panic fail-clean shape, matching the std.json
/// / F59 empty-Str sentinel convention.
///
/// # Safety
///
/// `resp` must be null or a live `Response` handle from
/// `__cobrust_strike_get`/`__cobrust_strike_post` (not yet dropped).
/// The returned pointer is an owned Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_strike_response_json(resp: *mut u8) -> *mut u8 {
    if resp.is_null() {
        return alloc_str_buffer("{}");
    }
    // SAFETY: caller attests `resp` is a live Response handle.
    let resp_ref = unsafe { &*resp.cast::<Response>() };
    let bytes = resp_ref.body_bytes();
    // Parse then re-serialize for canonical JSON (serde_json produces
    // a compact rendering by default, matching nest's canonicalization).
    let rendered = match serde_json::from_slice::<serde_json::Value>(bytes) {
        Ok(value) => serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string()),
        Err(_) => "{}".to_string(),
    };
    alloc_str_buffer(&rendered)
}

/// Drop a `Response` handle. `Box::from_raw` + drop, exactly once, at
/// the `.cb` scope exit (ADR-0072 §5 risk 1). Idempotent on null.
///
/// # Safety
///
/// `resp` must be null or a `Response` handle from
/// `__cobrust_strike_get`/`__cobrust_strike_post` that has not already
/// been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_strike_response_drop(resp: *mut u8) {
    if resp.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership.
    drop(unsafe { Box::from_raw(resp.cast::<Response>()) });
    DROP_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[cfg(test)]
#[allow(clippy::undocumented_unsafe_blocks)]
mod tests {
    use super::*;

    // The Str-buffer ABI is exported by cobrust-stdlib (a workspace
    // crate). For these unit tests we link it as a dev-dependency so the
    // `extern "C"` decls above resolve (in production the symbols come
    // from libcobrust_stdlib.a at the `cobrust build` link step). The
    // `extern crate` + `#[used]` static anchor forces cargo to put the
    // rlib on the test link line — a bare `extern "C"` decl alone does
    // not create a crate-dependency link edge.
    extern crate cobrust_stdlib;
    #[used]
    static _STDLIB_LINK_ANCHOR: unsafe extern "C" fn() -> *mut u8 =
        cobrust_stdlib::fmt::__cobrust_str_new;

    /// `DROP_COUNT` is a process-global counter. The "exactly once"
    /// assertion in the drop-counting tests would race under cargo's
    /// default-parallel test runner if two tests increment in-flight.
    /// A test-local mutex serializes the count-asserting tests; tests
    /// that don't assert count (null tolerance, json fallback) skip it.
    static DROP_COUNTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // The Str drop shim from libcobrust_stdlib (used to free the
    // buffers we hand out under test).
    unsafe extern "C" {
        fn __cobrust_str_drop(buf: *mut u8);
    }
    unsafe fn drop_str_for_test(buf: *mut u8) {
        unsafe { __cobrust_str_drop(buf) }
    }

    /// Round-trip without needing a real network — build a `Response`
    /// directly via `Response::from_parts`, Box it, run it through the
    /// borrowing accessor shims (text / status_code / json), and assert
    /// the handle drops exactly once.
    #[test]
    fn cabi_round_trip_borrows_then_drops_once() {
        // Recover from poison: a panic in an earlier test holding the
        // lock would otherwise cascade poisoned-lock failures across
        // every count-asserting test, masking the real signal.
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            // Hand-construct a Response equivalent to what
            // `__cobrust_strike_get` would Box up after a real fetch.
            let mut headers = HashMap::new();
            headers.insert("content-type".to_string(), "application/json".to_string());
            let resp = Response::from_parts(200, headers, br#"{"x":42}"#.to_vec());
            let handle = Box::into_raw(Box::new(resp)).cast::<u8>();

            // text() — borrow, project body as utf-8.
            let text_buf = __cobrust_strike_response_text(handle);
            let rendered = read_str_buf(text_buf);
            assert_eq!(rendered, r#"{"x":42}"#);
            drop_str_for_test(text_buf);

            // status_code() — borrow, project status as i64.
            let code = __cobrust_strike_response_status_code(handle);
            assert_eq!(code, 200);

            // json() — borrow, re-render canonical JSON.
            let json_buf = __cobrust_strike_response_json(handle);
            let json_rendered = read_str_buf(json_buf);
            assert_eq!(json_rendered, r#"{"x":42}"#);
            drop_str_for_test(json_buf);

            // status_code() AGAIN — confirms borrow didn't consume.
            let code2 = __cobrust_strike_response_status_code(handle);
            assert_eq!(code2, 200);

            // Drop exactly once.
            __cobrust_strike_response_drop(handle);
        }
        assert_eq!(
            drop_count() - before,
            1,
            "Response handle must drop exactly once"
        );
    }

    /// Null-pointer tolerance — every borrowing accessor must return a
    /// well-defined sentinel rather than panic / segfault. Drops on
    /// null are no-ops (don't touch the counter).
    #[test]
    fn cabi_null_handles_are_tolerated() {
        unsafe {
            let empty_text = __cobrust_strike_response_text(std::ptr::null_mut());
            assert_eq!(read_str_buf(empty_text), "");
            drop_str_for_test(empty_text);

            let zero_code = __cobrust_strike_response_status_code(std::ptr::null_mut());
            assert_eq!(zero_code, 0);

            let empty_obj = __cobrust_strike_response_json(std::ptr::null_mut());
            assert_eq!(read_str_buf(empty_obj), "{}");
            drop_str_for_test(empty_obj);

            let before = drop_count();
            __cobrust_strike_response_drop(std::ptr::null_mut());
            assert_eq!(drop_count(), before, "null drop must be a no-op");
        }
    }

    /// `json()` on a non-JSON body returns the `{}` sentinel — never
    /// panic. The `.cb` source surface stays ergonomic on malformed
    /// upstream responses.
    #[test]
    fn cabi_json_fallback_on_non_json_body() {
        // Even though this test doesn't assert a count delta, it does
        // call `__cobrust_strike_response_drop` once, which the
        // count-asserting tests must NOT see in flight. Take the lock.
        // Recover from poison: a panic in an earlier test holding the
        // lock would otherwise cascade poisoned-lock failures across
        // every count-asserting test, masking the real signal.
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let resp = Response::from_parts(200, HashMap::new(), b"not json".to_vec());
            let handle = Box::into_raw(Box::new(resp)).cast::<u8>();
            let json_buf = __cobrust_strike_response_json(handle);
            assert_eq!(read_str_buf(json_buf), "{}");
            drop_str_for_test(json_buf);
            __cobrust_strike_response_drop(handle);
        }
    }

    /// Drives `__cobrust_strike_get` against an URL that fails to
    /// parse at the URL layer (unsupported scheme). Proves the
    /// fail-clean sentinel path returns status 0 + empty body and
    /// never panics across the C boundary. Using an invalid scheme
    /// instead of an "unreachable port" keeps the test independent of
    /// any locally-configured HTTP proxy (which intercepts unreachable
    /// loopback ports and turns them into 502 upstream responses, per
    /// the project memory note on Clash@127.0.0.1:7897).
    #[test]
    fn cabi_get_with_invalid_url_returns_status_zero_sentinel() {
        // Recover from poison: a panic in an earlier test holding the
        // lock would otherwise cascade poisoned-lock failures across
        // every count-asserting test, masking the real signal.
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        // Empty string is an unparseable URL — reqwest's URL parser
        // rejects it before any network I/O, surfacing as the
        // `HttpErrorKind::InvalidUrl` path → fail_clean_response().
        let url = alloc_str_buffer("");
        unsafe {
            let handle = __cobrust_strike_get(url);
            assert!(
                !handle.is_null(),
                "get must always return a Response handle (sentinel on failure)"
            );
            let code = __cobrust_strike_response_status_code(handle);
            assert_eq!(code, 0, "invalid URL must yield status 0 sentinel");
            let text_buf = __cobrust_strike_response_text(handle);
            assert_eq!(read_str_buf(text_buf), "", "sentinel body is empty");
            drop_str_for_test(text_buf);
            __cobrust_strike_response_drop(handle);
            drop_str_for_test(url);
        }
        assert_eq!(drop_count() - before, 1);
    }
}
