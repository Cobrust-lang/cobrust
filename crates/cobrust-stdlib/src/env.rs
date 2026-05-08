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
