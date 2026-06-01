//! C-ABI shims — the runtime surface a compiled `.cb` program binds
//! onto when it does `import redis` and calls `redis.connect(url)` /
//! `client.set(k, v)` / `client.get(k)` / `client.delete(k)` /
//! `client.exists(k)` (the Phase-A KV verbs), plus the Phase-B
//! cache/counter/hash verbs `client.expire(k, secs)` / `client.incr(k)` /
//! `client.incr_by(k, n)` / `client.hset(k, field, v)` /
//! `client.hget(k, field)` (ADR-0078 Phase-1c — cache/KV, rebrand of
//! redis-py).
//!
//! # The chain
//!
//! These `#[no_mangle] extern "C"` shims are the L4 (runtime) layer of
//! the ecosystem-import chain. The Cobrust toolchain (L1 typecheck → L2
//! MIR intrinsic-rewrite → L3 codegen externs) retargets the source
//! calls onto these exact symbols; `cobrust build` static-links the
//! resulting `libredis.a` after `libcobrust_stdlib.a` (Linux wraps both
//! in `--start-group/--end-group` so the `__cobrust_str_*` forward
//! references below resolve under single-pass GNU ld too — per
//! ADR-0072 Q5).
//!
//! # ABI
//!
//! - **Handle** (`Client`) crosses as an opaque `*mut u8` pointer,
//!   `Box::into_raw`'d on construction by `connect` and `Box::from_raw`'d
//!   exactly once at the `.cb` scope-exit drop (the `_drop` shim). The
//!   handle pattern mirrors `den.Connection` (a stateful resource owned
//!   by value).
//! - **Strings** cross as Cobrust `Str` buffers. Per ADR-0072 Q5 this
//!   crate has **no Rust-level dependency on `cobrust-stdlib`** in the
//!   production build; the `__cobrust_str_*` primitives are declared
//!   `extern "C"` here and resolved from the always-linked
//!   `libcobrust_stdlib.a`. (For in-crate unit tests, `cobrust-stdlib`
//!   is a dev-dependency so the `extern "C"` decls resolve under
//!   `cargo test`.)
//!
//! # Ownership (ADR-0078 §3.7 — the one delta from strike: `&mut`)
//!
//! - `connect` **returns** a freshly-Boxed `Client` handle the `.cb`
//!   caller owns; the caller's MIR drop schedule frees it once at scope
//!   exit via the `_drop` shim.
//! - `set` / `get` / `delete` / `exists` / `expire` / `incr` /
//!   `incr_by` / `hset` / `hget` **borrow** the `Client` handle `&mut`
//!   (redis sync `Connection` command methods take `&mut self` — unlike
//!   strike's `&` read-only borrow on `Response`). The `&mut` is entirely
//!   inside the shim, invisible to the `.cb` aliasing model: each call is
//!   a separate borrow-then-release at the shim boundary, exactly like
//!   two sequential `conn.execute` calls in the den e2e. They never rebox
//!   or free the handle.
//! - A `DROP_COUNT` instrument lets the test suite assert each handle is
//!   dropped exactly once (no leak, no double-free).
//!
//! # Fail-cleanly sentinels (no panic across the C ABI)
//!
//! `connect` on an invalid URL / unreachable server returns a
//! **disconnected sentinel** `Client` (never null) whose every command
//! returns the per-type sentinel (empty str / `0` / `false`). A command
//! that errors at runtime (server reply error, dropped connection) maps
//! to the same per-type sentinel. A missing key reads as the
//! empty-string sentinel. This mirrors the strike status-0 / den null
//! fail-clean conventions and keeps the `.cb` source surface ergonomic
//! (no exception handling — constitution §2.2).

// C-ABI-boundary cast allows — mirror `cobrust-strike/src/cabi.rs`'s
// crate-level allows (the casts are intrinsic to the opaque-pointer /
// length ABI and are correct here):
// - `i64 <-> usize` length round-trips: Cobrust `Str` lengths are
//   non-negative and well under `usize::MAX` on the 64-bit targets the
//   AOT backend supports.
// - `*mut u8 -> *mut Client`: the pointer was produced by
//   `Box::into_raw` (correctly aligned) and only ever cast back to its
//   original type, so the alignment-narrowing lint is a false positive.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_ptr_alignment)]

use std::sync::atomic::{AtomicU64, Ordering};

use crate::client::Client;

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
/// null / empty. Mirrors `cobrust-strike/src/cabi.rs::read_str_buf` and
/// `cobrust-den/src/cabi.rs::read_str_buf`.
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

/// Total `Client` handle drops performed by the `_drop` shim this
/// process. Read by the test suite to assert no-leak / no-double-free.
pub static DROP_COUNT: AtomicU64 = AtomicU64::new(0);

/// Current `DROP_COUNT`. Test-only accessor.
#[must_use]
pub fn drop_count() -> u64 {
    DROP_COUNT.load(Ordering::SeqCst)
}

// =====================================================================
// redis C-ABI surface.
// =====================================================================

/// `redis.connect(url) -> Client`. Opens a sync connection to the redis
/// server named by `url` (a single canonical `redis://` URL) and returns
/// a freshly-Boxed `Client` handle the caller owns.
///
/// On ANY failure (invalid URL / unreachable server / no redis running)
/// returns a **disconnected sentinel** `Client` (whose every command
/// returns the per-type sentinel) — NEVER null, NEVER a panic across the
/// C ABI. The `.cb` source surface therefore never branches on null; a
/// connection failure surfaces as empty-str / `0` / `false` reads.
///
/// # Safety
///
/// `url` must be null or a valid Cobrust `Str` buffer. The returned
/// pointer is an owned `Client` handle, freed once via
/// `__cobrust_redis_client_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_connect(url: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let url = unsafe { read_str_buf(url) };
    let client = Client::connect(&url).unwrap_or_else(|_| Client::disconnected());
    Box::into_raw(Box::new(client)).cast::<u8>()
}

/// `Client.set(key, value)`. BORROWS the `Client` handle `&mut` (never
/// frees it), runs `SET key value`. Side-effect only — returns nothing
/// (no drop-eligible handle minted).
///
/// A null handle, the disconnected sentinel, or a command error are all
/// no-ops (the value simply is not stored) — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` / `value` must be
/// null or valid Cobrust `Str` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_set(client: *mut u8, key: *mut u8, value: *mut u8) {
    if client.is_null() {
        return;
    }
    // SAFETY: caller attests `client` is a live Client handle. We BORROW
    // it mutably (redis command methods take `&mut self`) — no rebox /
    // free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    // SAFETY: caller-attestation per `# Safety`.
    let value = unsafe { read_str_buf(value) };
    // Fail-clean: an error (disconnected sentinel / server error) is a
    // silent no-op — the .cb caller observes the absence on the next get.
    let _ = client_ref.set(&key, &value);
}

/// `Client.get(key) -> str`. BORROWS the `Client` handle `&mut` (never
/// frees it), runs `GET key`, and returns the stored value as a freshly-
/// allocated Cobrust `Str` buffer.
///
/// Returns the empty-string sentinel for a missing key, a null handle,
/// the disconnected sentinel, or a command error (absent == empty,
/// ADR-0078 §2.3-1) — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer. The returned pointer is an owned Cobrust
/// `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_get(client: *mut u8, key: *mut u8) -> *mut u8 {
    if client.is_null() {
        return alloc_str_buffer("");
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    // Missing key (`Ok(None)`) and any error both render the empty-str
    // sentinel — the fail-clean convention.
    let value = client_ref.get(&key).ok().flatten().unwrap_or_default();
    alloc_str_buffer(&value)
}

/// `Client.delete(key) -> i64`. BORROWS the `Client` handle `&mut`
/// (never frees it), runs `DEL key`, and returns the number of keys
/// removed (`0` or `1`).
///
/// Returns `0` for a null handle, the disconnected sentinel, or a
/// command error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_delete(client: *mut u8, key: *mut u8) -> i64 {
    if client.is_null() {
        return 0;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    client_ref.delete(&key).unwrap_or(0)
}

/// `Client.exists(key) -> bool`. BORROWS the `Client` handle `&mut`
/// (never frees it), runs `EXISTS key`, and returns whether the key is
/// present.
///
/// Returns `false` for a null handle, the disconnected sentinel, or a
/// command error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_exists(client: *mut u8, key: *mut u8) -> bool {
    if client.is_null() {
        return false;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    client_ref.exists(&key).unwrap_or(false)
}

/// `Client.expire(key, seconds) -> bool`. BORROWS the `Client` handle
/// `&mut` (never frees it), runs `EXPIRE key seconds`, and returns
/// whether the TTL was set (`true` when the key exists and the timeout
/// was applied).
///
/// Returns `false` for a null handle, the disconnected sentinel, or a
/// command error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer. `seconds` is a plain `i64` scalar.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_expire(
    client: *mut u8,
    key: *mut u8,
    seconds: i64,
) -> bool {
    if client.is_null() {
        return false;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    client_ref.expire(&key, seconds).unwrap_or(false)
}

/// `Client.incr(key) -> i64`. BORROWS the `Client` handle `&mut` (never
/// frees it), runs `INCR key`, and returns the value AFTER the increment
/// (the atomic-counter new value; `1` on the first increment of a fresh
/// key).
///
/// Returns `0` for a null handle, the disconnected sentinel, or a command
/// error (e.g. the stored value is not an integer) — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_incr(client: *mut u8, key: *mut u8) -> i64 {
    if client.is_null() {
        return 0;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    client_ref.incr(&key).unwrap_or(0)
}

/// `Client.incr_by(key, delta) -> i64`. BORROWS the `Client` handle
/// `&mut` (never frees it), runs `INCRBY key delta`, and returns the
/// value AFTER the increment.
///
/// Returns `0` for a null handle, the disconnected sentinel, or a command
/// error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer. `delta` is a plain `i64` scalar.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_incr_by(
    client: *mut u8,
    key: *mut u8,
    delta: i64,
) -> i64 {
    if client.is_null() {
        return 0;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    client_ref.incr_by(&key, delta).unwrap_or(0)
}

/// `Client.hset(key, field, value) -> bool`. BORROWS the `Client` handle
/// `&mut` (never frees it), runs `HSET key field value`, and returns
/// whether a NEW field was created (`true` when `field` did not
/// previously exist in the hash).
///
/// Returns `false` for a null handle, the disconnected sentinel, or a
/// command error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` / `field` / `value`
/// must be null or valid Cobrust `Str` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_hset(
    client: *mut u8,
    key: *mut u8,
    field: *mut u8,
    value: *mut u8,
) -> bool {
    if client.is_null() {
        return false;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    // SAFETY: caller-attestation per `# Safety`.
    let field = unsafe { read_str_buf(field) };
    // SAFETY: caller-attestation per `# Safety`.
    let value = unsafe { read_str_buf(value) };
    client_ref.hset(&key, &field, &value).unwrap_or(false)
}

/// `Client.hget(key, field) -> str`. BORROWS the `Client` handle `&mut`
/// (never frees it), runs `HGET key field`, and returns the stored field
/// value as a freshly-allocated Cobrust `Str` buffer.
///
/// Returns the empty-string sentinel for a missing field/hash, a null
/// handle, the disconnected sentinel, or a command error (absent ==
/// empty, mirroring `get`) — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` / `field` must be
/// null or valid Cobrust `Str` buffers. The returned pointer is an owned
/// Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_hget(
    client: *mut u8,
    key: *mut u8,
    field: *mut u8,
) -> *mut u8 {
    if client.is_null() {
        return alloc_str_buffer("");
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    // SAFETY: caller-attestation per `# Safety`.
    let field = unsafe { read_str_buf(field) };
    // Missing field (`Ok(None)`) and any error both render the empty-str
    // sentinel — the fail-clean convention, mirroring `get`.
    let value = client_ref
        .hget(&key, &field)
        .ok()
        .flatten()
        .unwrap_or_default();
    alloc_str_buffer(&value)
}

/// Drop a `Client` handle. `Box::from_raw` + drop, exactly once, at the
/// `.cb` scope exit (ADR-0072 §5 risk 1). Dropping the `Client` closes
/// the underlying TCP connection (RAII — no forgot-to-close footgun).
/// Idempotent on null.
///
/// # Safety
///
/// `client` must be null or a `Client` handle from
/// `__cobrust_redis_connect` that has not already been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_drop(client: *mut u8) {
    if client.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership.
    drop(unsafe { Box::from_raw(client.cast::<Client>()) });
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
    /// A test-local mutex serializes the count-asserting tests.
    static DROP_COUNTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // The Str drop shim from libcobrust_stdlib (used to free the
    // buffers we hand out under test).
    unsafe extern "C" {
        fn __cobrust_str_drop(buf: *mut u8);
    }
    unsafe fn drop_str_for_test(buf: *mut u8) {
        unsafe { __cobrust_str_drop(buf) }
    }

    /// The full fail-clean vertical slice WITHOUT a redis server: connect
    /// to an unreachable port → a disconnected sentinel handle → every
    /// shim returns its per-type sentinel (empty str / 0 / false) and the
    /// handle drops exactly once. This is the always-on, server-less
    /// proof of the no-panic-at-C-ABI guarantee + the borrow-then-drop
    /// lifecycle (the cabi twin of the e2e fail-clean test).
    #[test]
    fn cabi_fail_clean_path_returns_sentinels_and_drops_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            // redis://127.0.0.1:1/ — port 1 has nothing listening, so
            // connect fails clean → disconnected sentinel handle.
            let url = alloc_str_buffer("redis://127.0.0.1:1/");
            let client = __cobrust_redis_connect(url);
            assert!(
                !client.is_null(),
                "connect must always return a Client handle (sentinel on failure)"
            );
            drop_str_for_test(url);

            let key = alloc_str_buffer("greeting");
            let value = alloc_str_buffer("hello");

            // set — silent no-op on the sentinel (no panic).
            __cobrust_redis_client_set(client, key, value);

            // get — empty-str sentinel (absent / disconnected).
            let got = __cobrust_redis_client_get(client, key);
            assert_eq!(read_str_buf(got), "", "disconnected get is empty-str");
            drop_str_for_test(got);

            // delete — 0 keys removed.
            let n = __cobrust_redis_client_delete(client, key);
            assert_eq!(n, 0, "disconnected delete removes 0");

            // exists — false.
            let present = __cobrust_redis_client_exists(client, key);
            assert!(!present, "disconnected exists is false");

            // --- Phase-B verbs, same disconnected-sentinel fail-clean ---
            let field = alloc_str_buffer("field");

            // expire — false (TTL not set on the disconnected sentinel).
            let set_ttl = __cobrust_redis_client_expire(client, key, 60);
            assert!(!set_ttl, "disconnected expire is false");

            // incr — 0 sentinel (no atomic increment on a dead connection).
            let n1 = __cobrust_redis_client_incr(client, key);
            assert_eq!(n1, 0, "disconnected incr is 0");

            // incr_by — 0 sentinel.
            let n2 = __cobrust_redis_client_incr_by(client, key, 5);
            assert_eq!(n2, 0, "disconnected incr_by is 0");

            // hset — false (no new field created on the sentinel).
            let created = __cobrust_redis_client_hset(client, key, field, value);
            assert!(!created, "disconnected hset is false");

            // hget — empty-str sentinel (absent / disconnected).
            let hgot = __cobrust_redis_client_hget(client, key, field);
            assert_eq!(read_str_buf(hgot), "", "disconnected hget is empty-str");
            drop_str_for_test(hgot);

            drop_str_for_test(field);

            // A second get AGAIN — confirms the &mut borrows didn't
            // consume the handle (two sequential &mut method calls on the
            // same handle local, the den-e2e aliasing shape).
            let got2 = __cobrust_redis_client_get(client, key);
            assert_eq!(read_str_buf(got2), "");
            drop_str_for_test(got2);

            drop_str_for_test(key);
            drop_str_for_test(value);

            // Drop exactly once.
            __cobrust_redis_client_drop(client);
        }
        assert_eq!(
            drop_count() - before,
            1,
            "Client handle must drop exactly once"
        );
    }

    /// Null-pointer tolerance — every shim must return a well-defined
    /// sentinel rather than panic / segfault. Drops on null are no-ops
    /// (don't touch the counter).
    #[test]
    fn cabi_null_handles_are_tolerated() {
        unsafe {
            let key = alloc_str_buffer("k");

            // set on null — no-op, no panic.
            __cobrust_redis_client_set(std::ptr::null_mut(), key, key);

            let empty = __cobrust_redis_client_get(std::ptr::null_mut(), key);
            assert_eq!(read_str_buf(empty), "");
            drop_str_for_test(empty);

            let zero = __cobrust_redis_client_delete(std::ptr::null_mut(), key);
            assert_eq!(zero, 0);

            let f = __cobrust_redis_client_exists(std::ptr::null_mut(), key);
            assert!(!f);

            // --- Phase-B verbs on null — same per-type sentinels ---
            let null_ttl = __cobrust_redis_client_expire(std::ptr::null_mut(), key, 60);
            assert!(!null_ttl);

            let null_incr = __cobrust_redis_client_incr(std::ptr::null_mut(), key);
            assert_eq!(null_incr, 0);

            let null_incr_by = __cobrust_redis_client_incr_by(std::ptr::null_mut(), key, 5);
            assert_eq!(null_incr_by, 0);

            let null_hset = __cobrust_redis_client_hset(std::ptr::null_mut(), key, key, key);
            assert!(!null_hset);

            let null_hget = __cobrust_redis_client_hget(std::ptr::null_mut(), key, key);
            assert_eq!(read_str_buf(null_hget), "");
            drop_str_for_test(null_hget);

            drop_str_for_test(key);

            let before = drop_count();
            __cobrust_redis_client_drop(std::ptr::null_mut());
            assert_eq!(drop_count(), before, "null drop must be a no-op");
        }
    }

    /// connect with an unparseable URL (a bare non-URL string) must also
    /// yield the non-null disconnected sentinel — the invalid-URL branch
    /// of the fail-clean path, distinct from the unreachable-port branch.
    #[test]
    fn cabi_connect_invalid_url_yields_non_null_sentinel() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let url = alloc_str_buffer("not a redis url");
            let client = __cobrust_redis_connect(url);
            assert!(
                !client.is_null(),
                "invalid URL still yields a sentinel handle"
            );
            drop_str_for_test(url);
            __cobrust_redis_client_drop(client);
        }
        assert_eq!(drop_count() - before, 1);
    }
}
