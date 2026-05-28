//! C-ABI shims — the runtime surface a compiled `.cb` program binds
//! onto when it does `import molt` and calls `molt.now()` + the
//! `DateTime` borrowing methods (ADR-0072 fifth-module generalization —
//! datetime, rebrand of `python-dateutil`).
//!
//! # The chain
//!
//! These `#[no_mangle] extern "C"` shims are the L4 (runtime) layer of
//! the ecosystem-import chain. The Cobrust toolchain (L1 typecheck → L2
//! MIR intrinsic-rewrite → L3 codegen externs) retargets the source
//! calls onto these exact symbols; `cobrust build` static-links the
//! resulting `libmolt.a` after `libcobrust_stdlib.a` (Linux wraps both
//! in `--start-group/--end-group` so the `__cobrust_str_*` forward
//! references below resolve under single-pass GNU ld too — per
//! ADR-0072 Q5).
//!
//! # ABI — handle pattern (mirrors `den.Connection` / `strike.Response`)
//!
//! - **Handles** (`DateTime`) cross as opaque `*mut u8` pointers,
//!   `Box::into_raw`'d on construction by `now()` and `Box::from_raw`'d
//!   exactly once at the `.cb` scope-exit drop (the `_drop` shim).
//! - **Strings** cross as Cobrust `Str` buffers. Per ADR-0072 Q5 this
//!   crate has **no Rust-level dependency on `cobrust-stdlib`** in the
//!   production build; the `__cobrust_str_*` primitives are declared
//!   `extern "C"` here and resolved from the always-linked
//!   `libcobrust_stdlib.a`. (For in-crate unit tests, `cobrust-stdlib`
//!   is a dev-dependency so the `extern "C"` decls resolve under
//!   `cargo test`.)
//!
//! # Ownership (ADR-0072 §5 prime risk — same care as den / strike)
//!
//! - `now()` **returns** a freshly-Boxed `DateTime` handle the `.cb`
//!   caller owns; the caller's MIR drop schedule frees it once at scope
//!   exit via the `_drop` shim.
//! - `isoformat()` / `unix_timestamp()` **borrow** the DateTime (`&*` —
//!   immutable view; we project the rendered RFC3339 string / the
//!   unix epoch seconds without moving anything). They never rebox or
//!   free.
//! - A `DROP_COUNT` instrument lets the test suite assert each handle
//!   is dropped exactly once (no leak, no double-free).
//!
//! # Backing representation
//!
//! `DateTime` is a thin newtype around `time::OffsetDateTime` (the
//! workspace-shared `time` 0.3 crate already used by
//! `cobrust-llm-router`). `now()` calls `OffsetDateTime::now_utc()`;
//! `isoformat()` renders via the `Rfc3339` well-known format
//! description (the ISO-8601 subset that matches Python
//! `datetime.isoformat()` for UTC values); `unix_timestamp()` returns
//! `OffsetDateTime::unix_timestamp()` (seconds since 1970-01-01T00:00:00Z).
//! No external date-parsing surface (`parse`) in the first proof — that
//! is a tracked follow-up.

// C-ABI-boundary cast allows — mirror `cobrust-den/src/cabi.rs`'s
// crate-level allows (the casts are intrinsic to the opaque-pointer /
// length ABI and are correct here):
// - `i64 <-> usize` length round-trips: Cobrust `Str` lengths are
//   non-negative and well under `usize::MAX` on the 64-bit targets the
//   AOT backend supports.
// - `*mut u8 -> *mut DateTime`: the pointer was produced by
//   `Box::into_raw` (correctly aligned) and only ever cast back to its
//   original type, so the alignment-narrowing lint is a false positive.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_ptr_alignment)]

use std::sync::atomic::{AtomicU64, Ordering};

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

// =====================================================================
// Cobrust Str-buffer ABI — declared here, resolved from
// libcobrust_stdlib.a at link time (ADR-0072 Q5; no Rust dep).
// =====================================================================

unsafe extern "C" {
    /// Allocate a fresh empty Cobrust `Str` buffer.
    fn __cobrust_str_new() -> *mut u8;
    /// Append `len` UTF-8 bytes at `ptr` to the buffer.
    fn __cobrust_str_push_static(buf: *mut u8, ptr: *const u8, len: i64);
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

/// Total `DateTime` handle drops performed by the `_drop` shim this
/// process. Read by the test suite to assert no-leak / no-double-free.
pub static DROP_COUNT: AtomicU64 = AtomicU64::new(0);

/// Current `DROP_COUNT`. Test-only accessor.
#[must_use]
pub fn drop_count() -> u64 {
    DROP_COUNT.load(Ordering::SeqCst)
}

// =====================================================================
// DateTime handle backing type.
// =====================================================================

/// Opaque `DateTime` handle — a thin newtype over
/// `time::OffsetDateTime`. Boxed and trafficked as `*mut u8` across the
/// C boundary. Not `pub` — the `.cb` caller only ever sees the opaque
/// pointer.
pub(crate) struct DateTime {
    inner: OffsetDateTime,
}

impl DateTime {
    fn from_offset(inner: OffsetDateTime) -> Self {
        Self { inner }
    }
}

// =====================================================================
// molt C-ABI surface.
// =====================================================================

/// `molt.now() -> DateTime`. Returns a freshly-Boxed `DateTime` handle
/// carrying the current UTC time. The caller owns the handle; the
/// MIR drop schedule frees it once at scope exit via
/// `__cobrust_molt_datetime_drop`.
///
/// Never panics — `OffsetDateTime::now_utc()` is total on supported
/// platforms.
///
/// # Safety
///
/// The returned pointer is an owned `DateTime` handle, freed once via
/// `__cobrust_molt_datetime_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_molt_now() -> *mut u8 {
    let dt = DateTime::from_offset(OffsetDateTime::now_utc());
    Box::into_raw(Box::new(dt)).cast::<u8>()
}

/// `DateTime.isoformat() -> str`. BORROWS the DateTime handle (never
/// frees it) and returns its RFC3339 rendering as a freshly-allocated
/// Cobrust `Str` buffer. The RFC3339 subset matches Python
/// `datetime.isoformat()` for UTC-offset datetimes.
///
/// Returns an empty Str on a null handle (sentinel; no panic across
/// the C boundary).
///
/// # Safety
///
/// `dt` must be null or a live `DateTime` handle from
/// `__cobrust_molt_now` (not yet dropped). The returned pointer is an
/// owned Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_molt_datetime_isoformat(dt: *mut u8) -> *mut u8 {
    if dt.is_null() {
        return alloc_str_buffer("");
    }
    // SAFETY: caller attests `dt` is a live DateTime handle. We only
    // BORROW it — no rebox / free.
    let dt_ref = unsafe { &*dt.cast::<DateTime>() };
    let rendered = dt_ref.inner.format(&Rfc3339).unwrap_or_default();
    alloc_str_buffer(&rendered)
}

/// `DateTime.unix_timestamp() -> i64`. BORROWS the DateTime handle and
/// returns its unix epoch seconds (`OffsetDateTime::unix_timestamp()`).
/// On a null handle returns `0` (sentinel; matches the strike
/// fail-clean convention for primitive returns).
///
/// # Safety
///
/// `dt` must be null or a live `DateTime` handle from
/// `__cobrust_molt_now` (not yet dropped).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_molt_datetime_unix_timestamp(dt: *mut u8) -> i64 {
    if dt.is_null() {
        return 0;
    }
    // SAFETY: caller attests `dt` is a live DateTime handle.
    let dt_ref = unsafe { &*dt.cast::<DateTime>() };
    dt_ref.inner.unix_timestamp()
}

/// Drop a `DateTime` handle. `Box::from_raw` + drop, exactly once, at
/// the `.cb` scope exit (ADR-0072 §5 risk 1). Idempotent on null.
///
/// # Safety
///
/// `dt` must be null or a `DateTime` handle from `__cobrust_molt_now`
/// that has not already been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_molt_datetime_drop(dt: *mut u8) {
    if dt.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership.
    drop(unsafe { Box::from_raw(dt.cast::<DateTime>()) });
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
    /// that don't assert count skip it. Mirrors strike's lock pattern.
    static DROP_COUNTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    unsafe extern "C" {
        fn __cobrust_str_drop(buf: *mut u8);
        fn __cobrust_str_ptr(buf: *mut u8) -> *const u8;
        fn __cobrust_str_len(buf: *mut u8) -> i64;
    }
    unsafe fn drop_str_for_test(buf: *mut u8) {
        unsafe { __cobrust_str_drop(buf) }
    }
    unsafe fn read_buf(buf: *mut u8) -> String {
        if buf.is_null() {
            return String::new();
        }
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

    /// End-to-end through the cabi shims, exactly as a compiled `.cb`
    /// program would call them — proving `.now().isoformat() + .now().
    /// unix_timestamp()` round-trips and each handle drops exactly once.
    ///
    /// This is the molt-specific drop-once evidence per ADR-0072 §4
    /// done-means 5 (the "drop-count assertion in the shim" the den
    /// first proof established as the chain's drop-correctness witness).
    #[test]
    fn cabi_round_trip_drops_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let dt = __cobrust_molt_now();
            assert!(!dt.is_null(), "molt.now() must return a handle");

            // isoformat() — borrow, RFC3339 render.
            let iso_buf = __cobrust_molt_datetime_isoformat(dt);
            let rendered = read_buf(iso_buf);
            // RFC3339 is `YYYY-MM-DDTHH:MM:SS(.fff)?Z` — assert the
            // shape with a couple of cheap invariants rather than a
            // specific value (the time advances between test runs).
            assert!(
                rendered.len() >= 20,
                "expected RFC3339-shaped string, got: {rendered}"
            );
            assert!(rendered.contains('T'), "RFC3339 must contain T");
            assert!(
                rendered.ends_with('Z') || rendered.contains('+') || rendered.contains('-'),
                "RFC3339 must carry a timezone suffix, got: {rendered}"
            );
            drop_str_for_test(iso_buf);

            // unix_timestamp() — borrow, project as i64.
            let stamp = __cobrust_molt_datetime_unix_timestamp(dt);
            // Sometime between 2024-01-01 (1_704_067_200) and the year
            // 2100 (4_102_444_800) — cheap sanity bracket. The exact
            // value doesn't matter (it's the wall clock).
            assert!(
                stamp > 1_700_000_000,
                "unix_timestamp seems too small: {stamp}"
            );
            assert!(
                stamp < 4_102_444_800,
                "unix_timestamp seems too large: {stamp}"
            );

            // unix_timestamp() AGAIN — confirms borrow didn't consume.
            let stamp2 = __cobrust_molt_datetime_unix_timestamp(dt);
            assert_eq!(stamp, stamp2, "borrow must not advance the handle");

            // Drop exactly once.
            __cobrust_molt_datetime_drop(dt);
        }
        assert_eq!(
            drop_count() - before,
            1,
            "DateTime handle must drop exactly once"
        );
    }

    /// Null-pointer tolerance — every borrowing accessor must return a
    /// well-defined sentinel rather than panic / segfault. Drops on
    /// null are no-ops (don't touch the counter).
    #[test]
    fn cabi_null_handles_are_tolerated() {
        unsafe {
            let empty_iso = __cobrust_molt_datetime_isoformat(std::ptr::null_mut());
            assert_eq!(read_buf(empty_iso), "");
            drop_str_for_test(empty_iso);

            let zero_stamp = __cobrust_molt_datetime_unix_timestamp(std::ptr::null_mut());
            assert_eq!(zero_stamp, 0);

            let before = drop_count();
            __cobrust_molt_datetime_drop(std::ptr::null_mut());
            assert_eq!(drop_count(), before, "null drop must be a no-op");
        }
    }
}
