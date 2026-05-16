//! `std.env` — args / var.
//!
//! ADR-0025 §"Public surface (binding)" pins the API. The runtime
//! shim ([`crate::runtime::__cobrust_capture_argv`]) populates
//! [`crate::runtime::CAPTURED_ARGS`] at startup; [`args`] reads
//! from there with a `std::env::args` fallback for tests + non-shim
//! contexts.

// =====================================================================
// args — process argv
// =====================================================================

/// Process arguments, including `argv[0]`. The first element is
/// the program path; subsequent elements are user-supplied args.
///
/// Cobrust source: `std.env.args() -> List[str]`.
///
/// At runtime the values come from the C `main(argc, argv)`
/// captured by [`crate::runtime::__cobrust_capture_argv`]; in test /
/// non-runtime-shim contexts, falls back to `std::env::args()`.
pub fn args() -> Vec<String> {
    if let Some(captured) = crate::runtime::CAPTURED_ARGS.get() {
        return captured.clone();
    }
    std::env::args().collect()
}

// =====================================================================
// ADR-0044 W2 Phase 2 — source-level `argv()` plumbing
// =====================================================================

/// Rust-side helper materializing `CAPTURED_ARGS` (or
/// `std::env::args()` fallback for tests) into a flat `Vec<String>`.
///
/// Per ADR-0044 §"Implementation map", this is the unit-testable
/// counterpart to the `__cobrust_argv` C-ABI shim: tests can hit
/// `argv_list()` directly; codegen-emitted `argv()` callsites land on
/// the shim which materializes a Cobrust `List<Str>` via the existing
/// `__cobrust_list_new` / `__cobrust_str_new` / `__cobrust_list_set`
/// runtime helpers.
pub fn argv_list() -> Vec<String> {
    args()
}

/// C-ABI shim for source-level `argv() -> list[str]`. Materializes a
/// Cobrust `List<Str>` whose i64 slots store heap-allocated Str
/// pointers (one per captured argv element). The list pointer is
/// returned as `*mut u8`; codegen treats it as the same opaque
/// pointer shape that `__cobrust_list_new` produces.
///
/// Per ADR-0044 §"New runtime C-ABI surface", each list element is
/// constructed via `__cobrust_str_new` + `__cobrust_str_push_static`
/// so it shares the heap allocation contract with f-string buffers
/// (drop via `__cobrust_str_drop`).
///
/// # Safety
///
/// No pointer arguments — always safe to call. The returned list and
/// its element Strs are freed by the codegen-emitted drop schedule per
/// ADR-0050c: `Ty::List(Ty::Str)` dispatches to
/// `__cobrust_list_drop_elems(list, __cobrust_str_drop)` at the
/// binding's scope exit (`cranelift_backend.rs::emit_drop_for_ty`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_argv() -> *mut u8 {
    let captured = argv_list();
    // SAFETY: `__cobrust_list_new(8, len)` returns a valid List<i64>
    // pointer with `len` zeroed slots; `__cobrust_list_set(list, i, v)`
    // writes the i64 slot at index `i` (bounds-checked). `__cobrust_str_new`
    // returns a valid Str buffer pointer that survives until
    // `__cobrust_str_drop`.
    unsafe {
        let list = crate::collections::__cobrust_list_new(8, captured.len() as i64);
        for (i, s) in captured.iter().enumerate() {
            let buf = crate::fmt::__cobrust_str_new();
            if !s.is_empty() {
                crate::fmt::__cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
            }
            // Slot stores the Str pointer as an i64 (8 bytes on every
            // supported 64-bit target — ADR-0023 §"Calling convention
            // details" pins the M9 delivery to x86_64 + aarch64).
            crate::collections::__cobrust_list_set(list, i as i64, buf as i64);
        }
        list
    }
}

// =====================================================================
// var — environment variable
// =====================================================================

/// Read environment variable `name`. Returns `None` if absent or
/// not valid UTF-8 (which is an "absent" condition for Cobrust's
/// purposes).
pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
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
    fn args_returns_at_least_one() {
        // In the test runner, argv[0] is always present.
        let a = args();
        assert!(!a.is_empty());
    }

    #[test]
    fn var_present() {
        // PATH is set in every reasonable test environment.
        // If not, the test is skipped (returns early).
        let v = var("PATH");
        if let Some(s) = v {
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn var_absent_returns_none() {
        let v = var("COBRUST_M11_DEFINITELY_NOT_SET_4732");
        assert!(v.is_none());
    }
}
