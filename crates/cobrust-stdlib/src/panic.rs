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
}
