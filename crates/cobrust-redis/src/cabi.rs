//! C-ABI shims — the runtime surface a compiled `.cb` program binds
//! onto when it does `import redis` and calls `redis.connect(url)` /
//! `client.set(k, v)` / `client.get(k)` / `client.delete(k)` /
//! `client.exists(k)` (the Phase-A KV verbs), plus the Phase-B
//! cache/counter/hash verbs `client.expire(k, secs)` / `client.incr(k)` /
//! `client.incr_by(k, n)` / `client.hset(k, field, v)` /
//! `client.hget(k, field)`, plus the Phase-C list/set verbs
//! `client.lpush(k, v)` / `client.rpush(k, v)` / `client.lpop(k)` /
//! `client.rpop(k)` / `client.llen(k)` / `client.sadd(k, m)` /
//! `client.srem(k, m)` / `client.sismember(k, m)` / `client.scard(k)`
//! (ADR-0078 Phase-1c — cache/KV, rebrand of redis-py), plus the Phase-1d
//! LIST-of-str-return verbs `client.lrange(k, start, stop)` /
//! `client.smembers(k)` / `client.hkeys(k)` / `client.hgetall(k)`.
//!
//! Phase-1d ships the multi-element `list[str]` returns. (The Phase-C
//! deferral note here previously claimed redis had "no list-handle
//! precedent" for these — that was STALE: a `Ty::List(Str)` return is
//! first-class Cobrust, and the `__cobrust_list_*` C-ABI machinery + the
//! `.cb` for-loop / index / drop schedule were ALL already shipping. The
//! mint recipe is the SAME `__cobrust_llm_stream`
//! (cobrust-stdlib/src/llm.rs) and `__cobrust_coil_buffer_shape`
//! (cobrust-coil/src/cabi.rs) use for their list returns: allocate an
//! owned list the `.cb` scope drops once. `hgetall` returns a FLAT
//! `[k, v, k, v, ...]` list[str] — a documented Semantic divergence from
//! Python's dict, mirroring coil.shape's list-vs-tuple divergence note.)
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
//!   `incr_by` / `hset` / `hget` / `lpush` / `rpush` / `lpop` / `rpop` /
//!   `llen` / `sadd` / `srem` / `sismember` / `scard` **borrow** the
//!   `Client` handle `&mut`
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

// =====================================================================
// Cobrust List ABI — declared here, resolved from libcobrust_stdlib.a at
// link time (ADR-0072 Q5; no Rust dep — mirrors the Str block above and
// coil's `src/cabi.rs` list externs). The LIST-of-str return verbs
// (`lrange`/`smembers`/`hkeys`/`hgetall`, ADR-0078 Phase-1d) mint an
// owned `List<i64>` whose i64 slots store heap-`Str` pointers (one per
// element, `elem_size=8`); the `.cb` scope owns + drops the list once
// (via the `Ty::List(Str)` drop schedule → `__cobrust_list_drop_elems`),
// so these shims must NOT free it. This is the SAME mint recipe
// `__cobrust_llm_stream` (cobrust-stdlib/src/llm.rs) uses for its
// `list[str]` return — the precedent the Phase-C deferral comment wrongly
// claimed redis had none of.
// =====================================================================

unsafe extern "C" {
    /// Allocate a `List<i64>` with `len` zeroed slots (`len == cap`).
    /// `elem_size` is reserved (M12.x fixes the elem width at i64); for a
    /// `list[str]` the i64 slots hold heap-`Str` pointers.
    fn __cobrust_list_new(elem_size: i64, len: i64) -> *mut u8;
    /// Write `list[i] = v` (out-of-bounds writes are silently dropped).
    fn __cobrust_list_set(list: *mut u8, i: i64, v: i64);
}

/// Mint an owned Cobrust `list[str]` from `items`, the `__cobrust_llm_stream`
/// recipe (cobrust-stdlib/src/llm.rs): `__cobrust_list_new(8, len)` for a
/// `len`-slot `List<i64>`, then for each element allocate a fresh `Str`
/// buffer (via [`alloc_str_buffer`]) and store its pointer as the i64 slot
/// value. The returned list is OWNED by the `.cb` caller: its scope-exit
/// drop schedule (selected by codegen from `Ty::List(Str)`) calls
/// `__cobrust_list_drop_elems(list, __cobrust_str_drop)`, freeing each
/// element `Str` then the list container. The shim therefore must NOT
/// free it.
///
/// An empty `items` mints a valid empty list (len 0) — the fail-clean
/// shape for a null handle / disconnected sentinel / command error /
/// absent key, NEVER a null pointer and NEVER a panic across the C ABI.
fn alloc_str_list(items: &[String]) -> *mut u8 {
    // SAFETY: `__cobrust_list_new(8, len)` returns a valid `List<i64>`
    // with `len` zeroed slots; `__cobrust_list_set` writes each in-bounds
    // slot to a `Str`-buffer pointer produced by `alloc_str_buffer`.
    unsafe {
        let len = items.len() as i64;
        let list = __cobrust_list_new(8, len);
        for (i, s) in items.iter().enumerate() {
            let buf = alloc_str_buffer(s);
            __cobrust_list_set(list, i as i64, buf as i64);
        }
        list
    }
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

/// `Client.lpush(key, value) -> i64`. BORROWS the `Client` handle `&mut`
/// (never frees it), runs `LPUSH key value` (prepend at the head), and
/// returns the list's NEW length after the push.
///
/// Returns `0` for a null handle, the disconnected sentinel, or a command
/// error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` / `value` must be
/// null or valid Cobrust `Str` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_lpush(
    client: *mut u8,
    key: *mut u8,
    value: *mut u8,
) -> i64 {
    if client.is_null() {
        return 0;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    // SAFETY: caller-attestation per `# Safety`.
    let value = unsafe { read_str_buf(value) };
    client_ref.lpush(&key, &value).unwrap_or(0)
}

/// `Client.rpush(key, value) -> i64`. BORROWS the `Client` handle `&mut`
/// (never frees it), runs `RPUSH key value` (append at the tail), and
/// returns the list's NEW length after the push.
///
/// Returns `0` for a null handle, the disconnected sentinel, or a command
/// error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` / `value` must be
/// null or valid Cobrust `Str` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_rpush(
    client: *mut u8,
    key: *mut u8,
    value: *mut u8,
) -> i64 {
    if client.is_null() {
        return 0;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    // SAFETY: caller-attestation per `# Safety`.
    let value = unsafe { read_str_buf(value) };
    client_ref.rpush(&key, &value).unwrap_or(0)
}

/// `Client.lpop(key) -> str`. BORROWS the `Client` handle `&mut` (never
/// frees it), runs `LPOP key` (pop one element from the head), and returns
/// the popped value as a freshly-allocated Cobrust `Str` buffer.
///
/// Returns the empty-string sentinel for an empty/absent list, a null
/// handle, the disconnected sentinel, or a command error (absent ==
/// empty, mirroring `get`) — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer. The returned pointer is an owned Cobrust
/// `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_lpop(client: *mut u8, key: *mut u8) -> *mut u8 {
    if client.is_null() {
        return alloc_str_buffer("");
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    // Empty/absent list (`Ok(None)`) and any error both render the
    // empty-str sentinel — the fail-clean convention, mirroring `get`.
    let value = client_ref.lpop(&key).ok().flatten().unwrap_or_default();
    alloc_str_buffer(&value)
}

/// `Client.rpop(key) -> str`. BORROWS the `Client` handle `&mut` (never
/// frees it), runs `RPOP key` (pop one element from the tail), and returns
/// the popped value as a freshly-allocated Cobrust `Str` buffer.
///
/// Returns the empty-string sentinel for an empty/absent list, a null
/// handle, the disconnected sentinel, or a command error (absent ==
/// empty, mirroring `get`) — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer. The returned pointer is an owned Cobrust
/// `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_rpop(client: *mut u8, key: *mut u8) -> *mut u8 {
    if client.is_null() {
        return alloc_str_buffer("");
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    // Empty/absent list (`Ok(None)`) and any error both render the
    // empty-str sentinel — the fail-clean convention, mirroring `get`.
    let value = client_ref.rpop(&key).ok().flatten().unwrap_or_default();
    alloc_str_buffer(&value)
}

/// `Client.llen(key) -> i64`. BORROWS the `Client` handle `&mut` (never
/// frees it), runs `LLEN key`, and returns the number of elements in the
/// list.
///
/// Returns `0` for an absent key, a null handle, the disconnected
/// sentinel, or a command error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_llen(client: *mut u8, key: *mut u8) -> i64 {
    if client.is_null() {
        return 0;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    client_ref.llen(&key).unwrap_or(0)
}

/// `Client.sadd(key, member) -> i64`. BORROWS the `Client` handle `&mut`
/// (never frees it), runs `SADD key member`, and returns the number of
/// members ADDED (`1` when new, `0` when already present).
///
/// Returns `0` for a null handle, the disconnected sentinel, or a command
/// error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` / `member` must be
/// null or valid Cobrust `Str` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_sadd(
    client: *mut u8,
    key: *mut u8,
    member: *mut u8,
) -> i64 {
    if client.is_null() {
        return 0;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    // SAFETY: caller-attestation per `# Safety`.
    let member = unsafe { read_str_buf(member) };
    client_ref.sadd(&key, &member).unwrap_or(0)
}

/// `Client.srem(key, member) -> i64`. BORROWS the `Client` handle `&mut`
/// (never frees it), runs `SREM key member`, and returns the number of
/// members REMOVED (`1` when present, `0` when absent).
///
/// Returns `0` for a null handle, the disconnected sentinel, or a command
/// error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` / `member` must be
/// null or valid Cobrust `Str` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_srem(
    client: *mut u8,
    key: *mut u8,
    member: *mut u8,
) -> i64 {
    if client.is_null() {
        return 0;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    // SAFETY: caller-attestation per `# Safety`.
    let member = unsafe { read_str_buf(member) };
    client_ref.srem(&key, &member).unwrap_or(0)
}

/// `Client.sismember(key, member) -> bool`. BORROWS the `Client` handle
/// `&mut` (never frees it), runs `SISMEMBER key member`, and returns
/// whether `member` is in the set.
///
/// Returns `false` for a null handle, the disconnected sentinel, or a
/// command error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` / `member` must be
/// null or valid Cobrust `Str` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_sismember(
    client: *mut u8,
    key: *mut u8,
    member: *mut u8,
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
    let member = unsafe { read_str_buf(member) };
    client_ref.sismember(&key, &member).unwrap_or(false)
}

/// `Client.scard(key) -> i64`. BORROWS the `Client` handle `&mut` (never
/// frees it), runs `SCARD key`, and returns the number of members in the
/// set (the cardinality).
///
/// Returns `0` for an absent key, a null handle, the disconnected
/// sentinel, or a command error — never a panic.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_scard(client: *mut u8, key: *mut u8) -> i64 {
    if client.is_null() {
        return 0;
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    client_ref.scard(&key).unwrap_or(0)
}

/// `Client.lrange(key, start, stop) -> list[str]`. BORROWS the `Client`
/// handle `&mut` (never frees it), runs `LRANGE key start stop`, and
/// returns the elements in the (inclusive, redis-native, tail-relative on
/// negatives) index range as a freshly-minted owned Cobrust `list[str]`
/// (`start=0, stop=-1` is the whole list).
///
/// Returns an EMPTY list (len 0) for an absent key, a null handle, the
/// disconnected sentinel, or a command error (absent == empty list,
/// the list analogue of `get`'s empty-str sentinel) — NEVER null, NEVER a
/// panic. The `.cb` scope owns + drops the returned list (via the
/// `Ty::List(Str)` drop schedule); this shim does NOT free it.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer. `start` / `stop` are plain `i64` scalars.
/// The returned pointer is an owned Cobrust `list[str]`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_lrange(
    client: *mut u8,
    key: *mut u8,
    start: i64,
    stop: i64,
) -> *mut u8 {
    if client.is_null() {
        return alloc_str_list(&[]);
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    // An absent key / any error renders the EMPTY list — the fail-clean
    // convention, the list analogue of `get`'s empty-str sentinel.
    let items = client_ref.lrange(&key, start, stop).unwrap_or_default();
    alloc_str_list(&items)
}

/// `Client.smembers(key) -> list[str]`. BORROWS the `Client` handle
/// `&mut` (never frees it), runs `SMEMBERS key`, and returns the set's
/// members as a freshly-minted owned Cobrust `list[str]` (SMEMBERS has no
/// defined order).
///
/// Returns an EMPTY list for an absent key, a null handle, the
/// disconnected sentinel, or a command error — NEVER null, NEVER a panic.
/// The `.cb` scope owns + drops the returned list; this shim does NOT
/// free it.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer. The returned pointer is an owned Cobrust
/// `list[str]`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_smembers(client: *mut u8, key: *mut u8) -> *mut u8 {
    if client.is_null() {
        return alloc_str_list(&[]);
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    let items = client_ref.smembers(&key).unwrap_or_default();
    alloc_str_list(&items)
}

/// `Client.hkeys(key) -> list[str]`. BORROWS the `Client` handle `&mut`
/// (never frees it), runs `HKEYS key`, and returns the hash's field names
/// as a freshly-minted owned Cobrust `list[str]`.
///
/// Returns an EMPTY list for an absent key, a null handle, the
/// disconnected sentinel, or a command error — NEVER null, NEVER a panic.
/// The `.cb` scope owns + drops the returned list; this shim does NOT
/// free it.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer. The returned pointer is an owned Cobrust
/// `list[str]`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_hkeys(client: *mut u8, key: *mut u8) -> *mut u8 {
    if client.is_null() {
        return alloc_str_list(&[]);
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    let items = client_ref.hkeys(&key).unwrap_or_default();
    alloc_str_list(&items)
}

/// `Client.hgetall(key) -> list[str]`. BORROWS the `Client` handle `&mut`
/// (never frees it), runs `HGETALL key`, and returns the hash's
/// field/value pairs as a freshly-minted owned Cobrust `list[str]` —
/// FLAT `[field, value, field, value, ...]` (the documented Semantic
/// divergence from Python's dict, mirroring `coil.shape`'s
/// list-vs-tuple divergence: the flat list is the §2.5-closest honest
/// shape the already-shipping `Ty::List(Str)` machinery supports without
/// a `Dict`-across-C-ABI return shape).
///
/// Returns an EMPTY list for an absent key, a null handle, the
/// disconnected sentinel, or a command error — NEVER null, NEVER a panic.
/// The `.cb` scope owns + drops the returned list; this shim does NOT
/// free it.
///
/// # Safety
///
/// `client` must be null or a live `Client` handle from
/// `__cobrust_redis_connect` (not yet dropped); `key` must be null or a
/// valid Cobrust `Str` buffer. The returned pointer is an owned Cobrust
/// `list[str]`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_redis_client_hgetall(client: *mut u8, key: *mut u8) -> *mut u8 {
    if client.is_null() {
        return alloc_str_list(&[]);
    }
    // SAFETY: caller attests `client` is a live Client handle. BORROW
    // `&mut` — no rebox / free.
    let client_ref = unsafe { &mut *client.cast::<Client>() };
    // SAFETY: caller-attestation per `# Safety`.
    let key = unsafe { read_str_buf(key) };
    let items = client_ref.hgetall(&key).unwrap_or_default();
    alloc_str_list(&items)
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
    // The Phase-1d `list[str]`-return verbs bind `__cobrust_list_new` /
    // `__cobrust_list_set` (declared `extern "C"` above, resolved from
    // libcobrust_stdlib.a at the `cobrust build` link step). A bare
    // `extern "C"` decl alone does NOT create a crate-dependency link
    // edge, so — exactly like the `__cobrust_str_new` anchor — these
    // `#[used]` statics force cargo to put the `__cobrust_list_*` symbols
    // on the test link line.
    #[used]
    static _LIST_NEW_LINK_ANCHOR: unsafe extern "C" fn(i64, i64) -> *mut u8 =
        cobrust_stdlib::collections::__cobrust_list_new;
    #[used]
    static _LIST_SET_LINK_ANCHOR: unsafe extern "C" fn(*mut u8, i64, i64) =
        cobrust_stdlib::collections::__cobrust_list_set;

    /// `DROP_COUNT` is a process-global counter. The "exactly once"
    /// assertion in the drop-counting tests would race under cargo's
    /// default-parallel test runner if two tests increment in-flight.
    /// A test-local mutex serializes the count-asserting tests.
    static DROP_COUNTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // The Str drop shim from libcobrust_stdlib (used to free the
    // buffers we hand out under test).
    unsafe extern "C" {
        fn __cobrust_str_drop(buf: *mut u8);
        /// `list.len()` — used by the `list[str]`-return verb tests.
        fn __cobrust_list_len(list: *mut u8) -> i64;
        /// `list[i]` (the raw i64 slot — a `Str`-buffer pointer here).
        fn __cobrust_list_get(list: *mut u8, i: i64) -> i64;
        /// `list[str]` drop — frees each element `Str` then the container.
        /// This is EXACTLY the drop the codegen `Ty::List(Str)` schedule
        /// selects at `.cb` scope exit (llvm_backend.rs); under test we
        /// invoke it ourselves to prove the minted list frees clean.
        fn __cobrust_list_drop_elems(list: *mut u8, elem_drop_fn: unsafe extern "C" fn(*mut u8));
    }
    unsafe fn drop_str_for_test(buf: *mut u8) {
        unsafe { __cobrust_str_drop(buf) }
    }

    /// Read an owned `list[str]` (the shape the Phase-1d verbs mint) into
    /// an owned `Vec<String>` WITHOUT consuming it (borrows each slot's
    /// `Str` buffer via [`read_str_buf`]). The caller still owns the list
    /// and must free it via [`drop_str_list_for_test`].
    unsafe fn read_str_list_for_test(list: *mut u8) -> Vec<String> {
        unsafe {
            let len = __cobrust_list_len(list);
            (0..len)
                .map(|i| read_str_buf(__cobrust_list_get(list, i) as *mut u8))
                .collect()
        }
    }

    /// Free an owned `list[str]` via the SAME drop the codegen
    /// `Ty::List(Str)` schedule emits (`__cobrust_list_drop_elems` with
    /// `__cobrust_str_drop` per slot) — proves the minted list + its
    /// element `Str`s free clean with no leak / double-free.
    unsafe fn drop_str_list_for_test(list: *mut u8) {
        unsafe { __cobrust_list_drop_elems(list, __cobrust_str_drop) }
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

            // --- Phase-C verbs (lists + sets), same fail-clean sentinels ---
            let member = alloc_str_buffer("member");

            // lpush / rpush — 0 (no element pushed on the dead connection).
            let head_push = __cobrust_redis_client_lpush(client, key, value);
            assert_eq!(head_push, 0, "disconnected lpush is 0");
            let tail_push = __cobrust_redis_client_rpush(client, key, value);
            assert_eq!(tail_push, 0, "disconnected rpush is 0");

            // lpop / rpop — empty-str sentinel (empty/absent list).
            let lpopped = __cobrust_redis_client_lpop(client, key);
            assert_eq!(read_str_buf(lpopped), "", "disconnected lpop is empty-str");
            drop_str_for_test(lpopped);
            let rpopped = __cobrust_redis_client_rpop(client, key);
            assert_eq!(read_str_buf(rpopped), "", "disconnected rpop is empty-str");
            drop_str_for_test(rpopped);

            // llen — 0 (absent list).
            let llen = __cobrust_redis_client_llen(client, key);
            assert_eq!(llen, 0, "disconnected llen is 0");

            // sadd / srem — 0 (no member added/removed).
            let added = __cobrust_redis_client_sadd(client, key, member);
            assert_eq!(added, 0, "disconnected sadd is 0");
            let removed = __cobrust_redis_client_srem(client, key, member);
            assert_eq!(removed, 0, "disconnected srem is 0");

            // sismember — false (not a member of a non-existent set).
            let is_member = __cobrust_redis_client_sismember(client, key, member);
            assert!(!is_member, "disconnected sismember is false");

            // scard — 0 (empty/absent set).
            let card = __cobrust_redis_client_scard(client, key);
            assert_eq!(card, 0, "disconnected scard is 0");

            drop_str_for_test(member);
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

    /// The Phase-1d LIST-of-str-return verbs on the disconnected sentinel:
    /// each mints a VALID EMPTY `list[str]` (len 0, never null, never a
    /// panic across the C ABI — the list analogue of `get`'s empty-str
    /// sentinel), and each frees clean via the SAME
    /// `__cobrust_list_drop_elems(list, __cobrust_str_drop)` drop the
    /// codegen `Ty::List(Str)` schedule emits at `.cb` scope exit (proved
    /// by `drop_str_list_for_test` — a leak / double-free would abort).
    /// This is the cabi twin of the server-less Phase-1d e2e. Split from
    /// the Phase-A/B/C fail-clean test to keep each test under the
    /// 100-line lint ceiling (the four list verbs + their reads + drops).
    #[test]
    fn cabi_phase_1d_str_list_verbs_mint_empty_lists_on_disconnected() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            // Unreachable port → disconnected sentinel handle.
            let url = alloc_str_buffer("redis://127.0.0.1:1/");
            let client = __cobrust_redis_connect(url);
            assert!(!client.is_null());
            drop_str_for_test(url);
            let key = alloc_str_buffer("k");

            // lrange / smembers / hkeys / hgetall — each an EMPTY list[str]
            // (len 0, never null), each freeing clean.
            let lrange_list = __cobrust_redis_client_lrange(client, key, 0, -1);
            assert!(!lrange_list.is_null(), "lrange never returns null");
            assert_eq!(
                read_str_list_for_test(lrange_list),
                Vec::<String>::new(),
                "disconnected lrange is an empty list"
            );
            drop_str_list_for_test(lrange_list);

            let smembers_list = __cobrust_redis_client_smembers(client, key);
            assert!(!smembers_list.is_null(), "smembers never returns null");
            assert_eq!(__cobrust_list_len(smembers_list), 0);
            drop_str_list_for_test(smembers_list);

            let hkeys_list = __cobrust_redis_client_hkeys(client, key);
            assert!(!hkeys_list.is_null(), "hkeys never returns null");
            assert_eq!(__cobrust_list_len(hkeys_list), 0);
            drop_str_list_for_test(hkeys_list);

            // hgetall — the FLAT [k,v,...] divergence shape; empty here.
            let hgetall_list = __cobrust_redis_client_hgetall(client, key);
            assert!(!hgetall_list.is_null(), "hgetall never returns null");
            assert_eq!(__cobrust_list_len(hgetall_list), 0);
            drop_str_list_for_test(hgetall_list);

            drop_str_for_test(key);
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

            // --- Phase-C verbs on null — same per-type sentinels ---
            let null_head_push = __cobrust_redis_client_lpush(std::ptr::null_mut(), key, key);
            assert_eq!(null_head_push, 0);

            let null_tail_push = __cobrust_redis_client_rpush(std::ptr::null_mut(), key, key);
            assert_eq!(null_tail_push, 0);

            let null_head_pop = __cobrust_redis_client_lpop(std::ptr::null_mut(), key);
            assert_eq!(read_str_buf(null_head_pop), "");
            drop_str_for_test(null_head_pop);

            let null_tail_pop = __cobrust_redis_client_rpop(std::ptr::null_mut(), key);
            assert_eq!(read_str_buf(null_tail_pop), "");
            drop_str_for_test(null_tail_pop);

            let null_llen = __cobrust_redis_client_llen(std::ptr::null_mut(), key);
            assert_eq!(null_llen, 0);

            let null_sadd = __cobrust_redis_client_sadd(std::ptr::null_mut(), key, key);
            assert_eq!(null_sadd, 0);

            let null_srem = __cobrust_redis_client_srem(std::ptr::null_mut(), key, key);
            assert_eq!(null_srem, 0);

            let null_sismember = __cobrust_redis_client_sismember(std::ptr::null_mut(), key, key);
            assert!(!null_sismember);

            let null_scard = __cobrust_redis_client_scard(std::ptr::null_mut(), key);
            assert_eq!(null_scard, 0);

            // --- Phase-1d verbs on null — a VALID EMPTY list[str], NOT a
            // null pointer (so the .cb for-loop / index / drop see a
            // well-formed len-0 list), and each frees clean.
            let null_lrange = __cobrust_redis_client_lrange(std::ptr::null_mut(), key, 0, -1);
            assert!(!null_lrange.is_null(), "null lrange mints an empty list");
            assert_eq!(__cobrust_list_len(null_lrange), 0);
            drop_str_list_for_test(null_lrange);

            let null_smembers = __cobrust_redis_client_smembers(std::ptr::null_mut(), key);
            assert!(!null_smembers.is_null());
            assert_eq!(__cobrust_list_len(null_smembers), 0);
            drop_str_list_for_test(null_smembers);

            let null_hkeys = __cobrust_redis_client_hkeys(std::ptr::null_mut(), key);
            assert!(!null_hkeys.is_null());
            assert_eq!(__cobrust_list_len(null_hkeys), 0);
            drop_str_list_for_test(null_hkeys);

            let null_hgetall = __cobrust_redis_client_hgetall(std::ptr::null_mut(), key);
            assert!(!null_hgetall.is_null());
            assert_eq!(__cobrust_list_len(null_hgetall), 0);
            drop_str_list_for_test(null_hgetall);

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

    /// The `list[str]` mint + drop discipline with NON-empty content,
    /// isolated from any redis I/O. Mints a `list[str]` via the exact
    /// `alloc_str_list` helper the four Phase-1d shims use, reads its
    /// elements back (order + content preserved), then frees it via the
    /// SAME `__cobrust_list_drop_elems(list, __cobrust_str_drop)` drop the
    /// codegen `Ty::List(Str)` schedule emits at `.cb` scope exit. Proves
    /// the producer recipe (`__cobrust_list_new(8, len)` + per-element
    /// `Str` buffer + `__cobrust_list_set`) round-trips and frees with no
    /// leak / double-free — the server-less proof of the new return shape.
    /// (A `hgetall`-shaped FLAT `[k, v, k, v]` payload doubles as the
    /// documented flat-list divergence shape.)
    #[test]
    fn cabi_str_list_mint_roundtrips_and_drops_clean() {
        unsafe {
            // A flat [field, value, field, value] payload — the hgetall
            // divergence shape (and a general non-empty list[str]).
            let items = vec![
                "name".to_string(),
                "alice".to_string(),
                "role".to_string(),
                "admin".to_string(),
            ];
            let list = alloc_str_list(&items);
            assert!(!list.is_null(), "a non-empty mint is never null");
            assert_eq!(__cobrust_list_len(list), 4, "len == element count");
            assert_eq!(
                read_str_list_for_test(list),
                items,
                "the minted list preserves element order + content"
            );
            // Free via the codegen Ty::List(Str) drop — no leak / double-free.
            drop_str_list_for_test(list);

            // A 0-element mint is also valid (len 0, never null) and frees
            // clean — the fail-clean empty-list shape.
            let empty = alloc_str_list(&[]);
            assert!(!empty.is_null(), "an empty mint is never null");
            assert_eq!(__cobrust_list_len(empty), 0);
            drop_str_list_for_test(empty);
        }
    }
}
