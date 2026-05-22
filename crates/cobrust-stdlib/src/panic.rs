//! `std.panic` — panic / assert.
//!
//! ADR-0025 §"Public surface (binding)" pins the API. ADR-0024
//! §"Exit-code scheme" code 3 = internal panic; the M11 runtime
//! panic handler exits with that code (constitution §2.2 confines
//! exceptions to "truly unrecoverable").

use std::io::Write;

// =====================================================================
// Surface helpers
// =====================================================================

/// Panic with `msg`. Writes a structured diagnostic to stderr and
/// exits the process with code 3 (per ADR-0024 §"Exit-code scheme").
///
/// Constitution §2.2 confines this to "truly unrecoverable"; user
/// code returns `Result<T, E>` for recoverable errors.
pub fn panic(msg: &str) -> ! {
    write_diagnostic(msg);
    std::process::exit(crate::runtime::exit_codes::INTERNAL_PANIC as i32);
}

/// If `cond` is false, [`panic`] with `msg`. Otherwise no-op.
pub fn assert(cond: bool, msg: &str) {
    if !cond {
        panic(msg);
    }
}

fn write_diagnostic(msg: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "cobrust panic: {msg}");
}

// =====================================================================
// C ABI shims — what codegen-emitted calls land on
// =====================================================================

/// C-ABI shim for `std.panic.panic`. Codegen emits calls here.
///
/// # Safety
///
/// `ptr` must be a valid pointer to `len` bytes of UTF-8-encoded
/// text. Codegen always emits this with a `.rodata` pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_panic(ptr: *const u8, len: usize) -> ! {
    if ptr.is_null() || len == 0 {
        panic("(empty panic message)");
    }
    // SAFETY: caller-attestation per the `# Safety` clause.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let msg = std::str::from_utf8(bytes).unwrap_or("(non-utf8 panic message)");
    panic(msg)
}

/// C-ABI shim for `std.panic.assert`. Codegen emits calls here.
///
/// # Safety
///
/// `ptr`/`len` per [`__cobrust_panic`]. `cond` is a 0/1 bool.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_assert(cond: bool, ptr: *const u8, len: usize) {
    if cond {
        return;
    }
    if ptr.is_null() || len == 0 {
        panic("assertion failed");
    }
    // SAFETY: caller-attestation per the `# Safety` clause.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let msg = std::str::from_utf8(bytes).unwrap_or("assertion failed (non-utf8 message)");
    panic(msg);
}

/// C-ABI hookable symbol for `Result::unwrap_err()` runtime path
/// (ADR-0059g §3.4).
///
/// **Purpose**: provides a named address that lldb can break on via
/// the DAP `setExceptionBreakpoints` `result_err` filter (ADR-0059f
/// §3.4). When a Cobrust `Result::unwrap_err()` codepath fires at
/// runtime, the runtime calls this function just before panicking
/// — lldb halts at the function entry, allowing the user / LLM
/// agent to inspect the `Err(...)` value before the process exits.
///
/// **Codegen-side note**: codegen does NOT auto-emit calls to this
/// symbol wave-5 — the call-site lowering of `?` / `unwrap_err` is
/// out-of-scope per ADR-0059g §4 non-goal. Wave-5 ships the symbol
/// so lldb has a target to break on; a future ADR closes the
/// codegen lowering.
///
/// # Safety
///
/// `ptr` must be a valid pointer to `len` bytes of UTF-8-encoded
/// text describing the `Err(...)` payload. Codegen always emits
/// this with a `.rodata` pointer (when the codegen lowering ships).
/// Until that ships, manual calls from tests or runtime helpers
/// must observe the same precondition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_result_err_panic(ptr: *const u8, len: usize) -> ! {
    if ptr.is_null() || len == 0 {
        panic("Result::Err panic (empty payload)");
    }
    // SAFETY: caller-attestation per the `# Safety` clause.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let msg = std::str::from_utf8(bytes).unwrap_or("Result::Err panic (non-utf8 payload)");
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "cobrust Result::Err panic: {msg}");
    std::process::exit(crate::runtime::exit_codes::INTERNAL_PANIC as i32);
}

#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::format_push_string,
    clippy::let_unit_value,
    clippy::ignored_unit_patterns,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::manual_is_multiple_of,
    clippy::redundant_closure_for_method_calls,
    clippy::match_same_arms,
    clippy::map_unwrap_or,
    clippy::needless_pass_by_value,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;

    #[test]
    fn assert_true_no_op() {
        // Should not panic / exit.
        assert(true, "should not fire");
    }

    #[test]
    fn write_diagnostic_does_not_panic() {
        // Smoke: just exercises the stderr write path.
        super::write_diagnostic("test");
    }

    // The actual `panic` and `__cobrust_panic` paths exit the
    // process; they're verified end-to-end via the examples
    // integration tests, not unit tests (a unit test cannot assert
    // process exit without forking).

    /// ADR-0059g §3.4 — `__cobrust_result_err_panic` symbol smoke:
    /// verify the symbol address is non-null. The exit path is
    /// unit-test-unfriendly per the existing module comment; we just
    /// confirm the symbol is reachable as a function pointer so lldb's
    /// `breakpoint set --name __cobrust_result_err_panic` has a target.
    #[test]
    fn result_err_panic_symbol_is_exported() {
        let f: unsafe extern "C" fn(*const u8, usize) -> ! = super::__cobrust_result_err_panic;
        // Force the function-pointer comparison so the linker keeps
        // the symbol present in the test binary.
        let addr = f as *const ();
        assert!(!addr.is_null());
    }
}
