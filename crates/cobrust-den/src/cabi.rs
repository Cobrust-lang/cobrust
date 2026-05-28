//! C-ABI shims — the runtime surface a compiled `.cb` program binds
//! onto when it does `import den` and calls `den.connect(...)` /
//! `conn.execute(...)` / `cur.fetchall()` (ADR-0072 §4 first proof).
//!
//! # The chain
//!
//! These `#[no_mangle] extern "C"` shims are the L4 (runtime) layer of
//! the ecosystem-import chain. The Cobrust toolchain (L1 typecheck → L2
//! MIR intrinsic-rewrite → L3 codegen externs) retargets the source
//! calls onto these exact symbols; `cobrust build` static-links the
//! resulting `libden.a` after `libcobrust_stdlib.a` (Linux wraps both in
//! `--start-group/--end-group` so the `__cobrust_str_*` forward
//! references below resolve under single-pass GNU ld too — per
//! ADR-0072 Q5).
//!
//! # ABI
//!
//! - **Handles** (`Connection` / `Cursor`) cross as opaque `*mut u8`
//!   pointers, `Box::into_raw`'d on construction and `Box::from_raw`'d
//!   exactly once at the `.cb` scope-exit drop (the `_drop` shims).
//! - **Strings** cross as Cobrust `Str` buffers. Per ADR-0072 Q5 this
//!   crate has **no Rust-level dependency on `cobrust-stdlib`**; the
//!   `__cobrust_str_*` primitives are declared `extern "C"` here and
//!   resolved from the always-linked `libcobrust_stdlib.a`.
//!
//! # Ownership (ADR-0072 §5 prime risk)
//!
//! - `connect` / `execute` **return** freshly-Boxed handles the `.cb`
//!   caller owns; the caller's MIR drop schedule frees them once at
//!   scope exit via the `_drop` shims.
//! - `execute` / `fetchall` **borrow** their handle arg
//!   (`&mut *(ptr as *mut T)`) — they never rebox or free it.
//! - A `DROP_COUNT` instrument lets the test suite assert each handle is
//!   dropped exactly once (no leak, no double-free).
//!
//! # `!Send` (ADR-0072 §5 risk 2)
//!
//! `den`'s `Rc<RefCell<…>>` handles are single-threaded. The first
//! proof is single-threaded; these shims must not be called with a
//! handle that has crossed into a spawned task.

// C-ABI-boundary cast allows — mirror the `cobrust-stdlib` crate-level
// allows (lib.rs §clippy) that the str-buffer shims (`json.rs`,
// `fmt.rs`) rely on. These casts are intrinsic to the opaque-pointer /
// length ABI and are correct here:
// - `i64 <-> usize` length round-trips: Cobrust `Str` lengths are
//   non-negative and well under `usize::MAX` on the 64-bit targets the
//   AOT backend supports.
// - `*mut u8 -> *mut Connection/Cursor`: the pointer was produced by
//   `Box::into_raw` (correctly aligned) and only ever cast back to its
//   original type, so the alignment-narrowing lint is a false positive.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_ptr_alignment)]

use std::sync::atomic::{AtomicU64, Ordering};

use crate::connection::{Connection, Cursor, connect};
use crate::value::Value;

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
/// null / empty. Mirrors `cobrust-stdlib/src/json.rs::read_str_buf`.
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

/// Total handle drops performed by the `_drop` shims this process.
/// Read by the test suite to assert no-leak / no-double-free.
pub static DROP_COUNT: AtomicU64 = AtomicU64::new(0);

/// Current `DROP_COUNT`. Test-only accessor.
#[must_use]
pub fn drop_count() -> u64 {
    DROP_COUNT.load(Ordering::SeqCst)
}

// =====================================================================
// Row rendering (first proof: rows → str — ADR-0072 §4).
// =====================================================================

/// Render a fetched cell to its Python-`repr`-ish text.
fn render_value(v: &Value) -> String {
    match v {
        Value::Null => "None".to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Real(r) => format!("{r}"),
        Value::Text(s) => format!("'{s}'"),
        Value::Blob(b) => format!("<blob {} bytes>", b.len()),
    }
}

/// Render the full result set the way CPython prints
/// `cur.fetchall()` — a `list` of per-row `tuple`s:
/// `[(42,), ('ada', 1)]`. A single-cell row keeps the PEP-249 trailing
/// comma (`(42,)`), matching Python tuple repr.
fn render_rows(rows: &[crate::value::Row]) -> String {
    let mut out = String::from("[");
    for (ri, row) in rows.iter().enumerate() {
        if ri > 0 {
            out.push_str(", ");
        }
        out.push('(');
        let cells = row.cells();
        for (ci, cell) in cells.iter().enumerate() {
            if ci > 0 {
                out.push_str(", ");
            }
            out.push_str(&render_value(cell));
        }
        // Python tuple repr: a 1-tuple carries a trailing comma.
        if cells.len() == 1 {
            out.push(',');
        }
        out.push(')');
    }
    out.push(']');
    out
}

// =====================================================================
// den C-ABI surface.
// =====================================================================

/// `den.connect(path) -> Connection`. Opens (or creates) the SQLite
/// database at `path` (`":memory:"` for an in-memory db) and returns a
/// freshly-Boxed handle the caller owns.
///
/// Returns null on an open failure (the first proof's non-panicking
/// sentinel; a typed-error surface is a follow-up).
///
/// # Safety
///
/// `path` must be null or a valid Cobrust `Str` buffer. The returned
/// pointer is an owned `Connection` handle, freed once via
/// `__cobrust_den_connection_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_den_connect(path: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let path = unsafe { read_str_buf(path) };
    match connect(&path) {
        Ok(conn) => Box::into_raw(Box::new(conn)).cast::<u8>(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// `Connection.execute(sql) -> Cursor`. BORROWS the connection handle
/// (never frees it), runs `sql`, and returns a freshly-Boxed `Cursor`
/// the caller owns.
///
/// Returns null on a null connection or a SQL error (non-panicking
/// sentinel).
///
/// # Safety
///
/// `conn` must be a live `Connection` handle from `__cobrust_den_connect`
/// (not yet dropped); `sql` must be null or a valid Cobrust `Str`. The
/// returned pointer is an owned `Cursor`, freed once via
/// `__cobrust_den_cursor_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_den_connection_execute(conn: *mut u8, sql: *mut u8) -> *mut u8 {
    if conn.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller attests `conn` is a live Connection handle. We only
    // BORROW it — no rebox / free.
    let conn_ref = unsafe { &*conn.cast::<Connection>() };
    // SAFETY: caller-attestation per `# Safety`.
    let sql = unsafe { read_str_buf(sql) };
    match conn_ref.execute(&sql) {
        Ok(cur) => Box::into_raw(Box::new(cur)).cast::<u8>(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// `Cursor.fetchall() -> str`. BORROWS the cursor handle (never frees
/// it) and renders its remaining rows to a Cobrust `Str` buffer (first
/// proof: `str` rendering; row→list[tuple] is the immediate follow-up).
///
/// Returns an empty Str on a null cursor.
///
/// # Safety
///
/// `cur` must be a live `Cursor` handle from
/// `__cobrust_den_connection_execute` (not yet dropped). The returned
/// pointer is an owned Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_den_cursor_fetchall(cur: *mut u8) -> *mut u8 {
    if cur.is_null() {
        return alloc_str_buffer("");
    }
    // SAFETY: caller attests `cur` is a live Cursor handle. We BORROW it
    // mutably (fetchall advances the cursor position) — no rebox / free.
    let cur_ref = unsafe { &mut *cur.cast::<Cursor>() };
    let rows = cur_ref.fetchall();
    alloc_str_buffer(&render_rows(&rows))
}

/// Drop a `Connection` handle. `Box::from_raw` + drop, exactly once, at
/// the `.cb` scope exit (ADR-0072 §5 risk 1). Idempotent on null.
///
/// # Safety
///
/// `conn` must be null or a `Connection` handle from
/// `__cobrust_den_connect` that has not already been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_den_connection_drop(conn: *mut u8) {
    if conn.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership.
    drop(unsafe { Box::from_raw(conn.cast::<Connection>()) });
    DROP_COUNT.fetch_add(1, Ordering::SeqCst);
}

/// Drop a `Cursor` handle. `Box::from_raw` + drop, exactly once, at the
/// `.cb` scope exit. Idempotent on null.
///
/// # Safety
///
/// `cur` must be null or a `Cursor` handle from
/// `__cobrust_den_connection_execute` that has not already been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_den_cursor_drop(cur: *mut u8) {
    if cur.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership.
    drop(unsafe { Box::from_raw(cur.cast::<Cursor>()) });
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
    // `extern crate` (with `#[used]`-style coercion via a referenced
    // item) forces cargo to put the rlib on the test link line so the
    // `__cobrust_str_*` symbols actually resolve — a bare `extern "C"`
    // decl alone does not create a crate-dependency link edge.
    extern crate cobrust_stdlib;
    #[used]
    static _STDLIB_LINK_ANCHOR: unsafe extern "C" fn() -> *mut u8 =
        cobrust_stdlib::fmt::__cobrust_str_new;

    /// End-to-end through the C-ABI shims, exactly as a compiled `.cb`
    /// program would call them — proving the row data round-trips and
    /// each handle drops exactly once.
    #[test]
    fn cabi_round_trip_prints_42_and_drops_once() {
        let before = drop_count();
        unsafe {
            let path = alloc_str_buffer(":memory:");
            let conn = __cobrust_den_connect(path);
            assert!(!conn.is_null(), "connect(:memory:) returned null");
            __cobrust_str_drop_for_test(path);

            let create = alloc_str_buffer("CREATE TABLE t(x INTEGER)");
            let cur0 = __cobrust_den_connection_execute(conn, create);
            assert!(!cur0.is_null());
            __cobrust_str_drop_for_test(create);
            __cobrust_den_cursor_drop(cur0);

            let insert = alloc_str_buffer("INSERT INTO t VALUES (42)");
            let cur1 = __cobrust_den_connection_execute(conn, insert);
            assert!(!cur1.is_null());
            __cobrust_str_drop_for_test(insert);
            __cobrust_den_cursor_drop(cur1);

            let select = alloc_str_buffer("SELECT x FROM t");
            let cur2 = __cobrust_den_connection_execute(conn, select);
            assert!(!cur2.is_null());
            __cobrust_str_drop_for_test(select);

            let rows = __cobrust_den_cursor_fetchall(cur2);
            let rendered = read_str_buf(rows);
            assert_eq!(rendered, "[(42,)]", "fetchall rendering");
            __cobrust_str_drop_for_test(rows);

            __cobrust_den_cursor_drop(cur2);
            __cobrust_den_connection_drop(conn);
        }
        // 3 cursors + 1 connection = 4 drops, exactly once each.
        assert_eq!(drop_count() - before, 4, "every handle drops exactly once");
    }

    // The drop shim for Str is also in libcobrust_stdlib; declared here
    // for the test cleanup path only.
    unsafe extern "C" {
        fn __cobrust_str_drop(buf: *mut u8);
    }
    unsafe fn __cobrust_str_drop_for_test(buf: *mut u8) {
        unsafe { __cobrust_str_drop(buf) }
    }

    #[test]
    fn null_handles_are_tolerated() {
        unsafe {
            assert!(
                __cobrust_den_connection_execute(std::ptr::null_mut(), std::ptr::null_mut())
                    .is_null()
            );
            let empty = __cobrust_den_cursor_fetchall(std::ptr::null_mut());
            assert_eq!(read_str_buf(empty), "");
            __cobrust_str_drop_for_test(empty);
            // Drops on null are no-ops (don't touch the counter).
            let before = drop_count();
            __cobrust_den_connection_drop(std::ptr::null_mut());
            __cobrust_den_cursor_drop(std::ptr::null_mut());
            assert_eq!(drop_count(), before);
        }
    }

    #[test]
    fn render_multi_column_row() {
        let row = crate::value::Row::new(vec![
            Value::Integer(1),
            Value::Text("ada".to_string()),
            Value::Null,
        ]);
        assert_eq!(
            render_rows(std::slice::from_ref(&row)),
            "[(1, 'ada', None)]"
        );
    }
}
