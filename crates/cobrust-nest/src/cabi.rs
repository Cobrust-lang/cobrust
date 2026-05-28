//! C-ABI shims — the runtime surface a compiled `.cb` program binds
//! onto when it does `import nest` and calls `nest.loads_str(toml)`
//! (ADR-0072 second-module generalization).
//!
//! # The chain
//!
//! These `#[no_mangle] extern "C"` shims are the L4 (runtime) layer of
//! the ecosystem-import chain. The Cobrust toolchain (L1 typecheck → L2
//! MIR intrinsic-rewrite → L3 codegen externs) retargets the source
//! call onto this exact symbol; `cobrust build` static-links the
//! resulting `libnest.a` after `libcobrust_stdlib.a` (Linux wraps both
//! in `--start-group/--end-group` so the `__cobrust_str_*` forward
//! references below resolve under single-pass GNU ld too — per
//! ADR-0072 Q5).
//!
//! # ABI
//!
//! - **Strings** cross as Cobrust `Str` buffers. Per ADR-0072 Q5 this
//!   crate has **no Rust-level dependency on `cobrust-stdlib`** in the
//!   production build; the `__cobrust_str_*` primitives are declared
//!   `extern "C"` here and resolved from the always-linked
//!   `libcobrust_stdlib.a`. (For in-crate unit tests, `cobrust-stdlib`
//!   is a dev-dependency so the `extern "C"` decls resolve under
//!   `cargo test`.)
//!
//! # Ownership
//!
//! Pure value-in-value-out (`Str → Str`): the input Str is BORROWED
//! (read into an owned Rust `String` then released — the `.cb` caller's
//! drop schedule frees the input buffer at scope exit). The returned
//! `*mut u8` Str buffer is owned by the caller and freed exactly once
//! by the existing `__cobrust_str_drop` at scope exit. NO handles, NO
//! callbacks — the chain handles this case natively (ADR-0072 §5 risk 1
//! "scope-local handle drop" is a non-concern here because there is no
//! handle).

// C-ABI-boundary cast allows — mirror `cobrust-den/src/cabi.rs`'s
// crate-level allows (the casts are intrinsic to the opaque-pointer /
// length ABI and are correct here):
// - `i64 <-> usize` length round-trips: Cobrust `Str` lengths are
//   non-negative and well under `usize::MAX` on the 64-bit targets the
//   AOT backend supports.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]

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
/// null / empty. Mirrors `cobrust-den/src/cabi.rs::read_str_buf`.
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
// nest C-ABI surface.
// =====================================================================

/// `nest.loads_str(toml) -> str`. Parses the TOML source `toml` and
/// returns its canonical JSON rendering as a freshly-allocated Cobrust
/// `Str` buffer.
///
/// Reuses the same code path as the `cobrust-nest-json` subprocess
/// bridge: `nest::loads` to parse, `nest::table_to_json` to convert,
/// `serde_json::to_string` to canonicalize.
///
/// On a parse error the returned buffer carries a JSON object of shape
/// `{"err": "<message>"}` — a non-panicking sentinel matching the
/// `cobrust-nest-json` bridge's convention. (A typed `Result[str, E]`
/// surface is a follow-up; the first proof keeps the chain pure
/// value-in-value-out.)
///
/// # Safety
///
/// `toml` must be null or a valid Cobrust `Str` buffer. The returned
/// pointer is an owned Cobrust `Str` buffer, freed once by the existing
/// `__cobrust_str_drop` at the `.cb` scope exit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_nest_loads_str(toml: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let src = unsafe { read_str_buf(toml) };
    match crate::loads(&src) {
        Ok(table) => {
            let json = crate::table_to_json(&table);
            let rendered =
                serde_json::to_string(&json).unwrap_or_else(|e| format!("{{\"err\": \"{e}\"}}"));
            alloc_str_buffer(&rendered)
        }
        Err(e) => {
            // Match the cobrust-nest-json subprocess bridge's error
            // shape so downstream tooling sees a uniform surface.
            let payload = serde_json::json!({"err": format!("{e}")});
            alloc_str_buffer(&payload.to_string())
        }
    }
}

#[cfg(test)]
#[allow(clippy::undocumented_unsafe_blocks)]
mod tests {
    use super::*;

    // The Str-buffer ABI is exported by cobrust-stdlib (a workspace
    // crate). For these unit tests we link it as a dev-dependency so the
    // `extern "C"` decls above resolve (in production the symbols come
    // from libcobrust_stdlib.a at the `cobrust build` link step). The
    // `extern crate` + the `#[used]` static anchor forces cargo to put
    // the rlib on the test link line — a bare `extern "C"` decl alone
    // does not create a crate-dependency link edge.
    extern crate cobrust_stdlib;
    #[used]
    static _STDLIB_LINK_ANCHOR: unsafe extern "C" fn() -> *mut u8 =
        cobrust_stdlib::fmt::__cobrust_str_new;

    // The Str drop shim from libcobrust_stdlib (used to free the
    // buffers we hand out under test).
    unsafe extern "C" {
        fn __cobrust_str_drop(buf: *mut u8);
    }
    unsafe fn drop_str_for_test(buf: *mut u8) {
        unsafe { __cobrust_str_drop(buf) }
    }

    /// End-to-end through the C-ABI shim, exactly as a compiled `.cb`
    /// program would call it — proving a simple TOML→JSON round-trip.
    #[test]
    fn cabi_round_trip_simple_key_value() {
        unsafe {
            let input = alloc_str_buffer("title = \"hello\"\n");
            let out = __cobrust_nest_loads_str(input);
            let rendered = read_str_buf(out);
            assert_eq!(rendered, r#"{"title":"hello"}"#);
            drop_str_for_test(input);
            drop_str_for_test(out);
        }
    }

    #[test]
    fn cabi_round_trip_nested_table() {
        unsafe {
            let input = alloc_str_buffer("[server]\nport = 8080\n");
            let out = __cobrust_nest_loads_str(input);
            let rendered = read_str_buf(out);
            assert_eq!(rendered, r#"{"server":{"port":8080}}"#);
            drop_str_for_test(input);
            drop_str_for_test(out);
        }
    }

    #[test]
    fn cabi_null_input_yields_empty_object() {
        // Null / empty TOML parses to an empty top-level table → `{}`.
        unsafe {
            let out = __cobrust_nest_loads_str(std::ptr::null_mut());
            let rendered = read_str_buf(out);
            assert_eq!(rendered, "{}");
            drop_str_for_test(out);
        }
    }

    #[test]
    fn cabi_parse_error_yields_err_sentinel() {
        // An unterminated string literal forces a TomliError. The shim
        // returns a `{"err": "..."}` JSON sentinel rather than null /
        // panic (matches the cobrust-nest-json subprocess bridge).
        unsafe {
            let input = alloc_str_buffer("title = \"oops");
            let out = __cobrust_nest_loads_str(input);
            let rendered = read_str_buf(out);
            assert!(
                rendered.starts_with(r#"{"err":"#),
                "parse-error sentinel expected, got: {rendered}"
            );
            drop_str_for_test(input);
            drop_str_for_test(out);
        }
    }
}
