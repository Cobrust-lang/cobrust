//! `std.fmt` — f-string runtime helpers.
//!
//! ADR-0025 §"Public surface (binding)" pins the API. ADR-0019
//! §"M11 — Standard library" §"Modules" describes:
//!
//! > `std.fmt` | f-string runtime helpers (already lowered in
//! > HIR; this is the runtime side)
//!
//! HIR-tier f-string lowering decomposes `f"{x}"` into a sequence
//! of static text + value-formatting calls. The runtime helpers
//! here are what those calls land on.

// =====================================================================
// Per-type formatters
// =====================================================================

/// Format an integer as a decimal string. Cobrust's `f"{i}"`
/// lowers to a call here.
pub fn format_int(i: i64) -> String {
    i.to_string()
}

/// Format a float as a string. Uses the `FormatArg::Float`
/// strategy: integer-valued floats display with `.0`; non-integer
/// values use the shortest round-trip repr.
pub fn format_float(x: f64) -> String {
    if x.fract() == 0.0 && x.is_finite() {
        format!("{x:.1}")
    } else {
        format!("{x}")
    }
}

/// Format a bool as `True` / `False` (matches Python's repr).
pub fn format_bool(b: bool) -> String {
    if b { "True".into() } else { "False".into() }
}

/// Identity. Provided for completeness in the f-string codegen.
pub fn format_str(s: &str) -> String {
    s.to_string()
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
    fn format_int_basic() {
        assert_eq!(format_int(0), "0");
        assert_eq!(format_int(42), "42");
        assert_eq!(format_int(-7), "-7");
    }

    #[test]
    fn format_int_max() {
        assert_eq!(format_int(i64::MAX), "9223372036854775807");
    }

    #[test]
    fn format_int_min() {
        assert_eq!(format_int(i64::MIN), "-9223372036854775808");
    }

    #[test]
    fn format_float_integer_value() {
        assert_eq!(format_float(3.0), "3.0");
        assert_eq!(format_float(-7.0), "-7.0");
    }

    #[test]
    fn format_float_fractional() {
        let s = format_float(3.14);
        assert!(s.starts_with("3.14"));
    }

    #[test]
    fn format_float_zero() {
        assert_eq!(format_float(0.0), "0.0");
    }

    #[test]
    fn format_float_neg_zero() {
        // -0.0.fract() == -0.0, but we render with "{:.1}" so
        // representation may be "-0.0". Implementation-defined;
        // accept either.
        let s = format_float(-0.0);
        assert!(s == "-0.0" || s == "0.0");
    }

    #[test]
    fn format_float_nan_or_inf_does_not_panic() {
        let _s1 = format_float(f64::NAN);
        let _s2 = format_float(f64::INFINITY);
        let _s3 = format_float(f64::NEG_INFINITY);
    }

    #[test]
    fn format_bool_true() {
        assert_eq!(format_bool(true), "True");
    }

    #[test]
    fn format_bool_false() {
        assert_eq!(format_bool(false), "False");
    }

    #[test]
    fn format_str_identity() {
        assert_eq!(format_str("hi"), "hi");
        assert_eq!(format_str(""), "");
    }
}
