//! `std.math` — sqrt / pow / sin / cos / abs / floor / ceil / round
//! plus `pi` / `e` constants.
//!
//! ADR-0025 §"Public surface (binding)" pins the API; ADR-0019
//! §"M11 — Standard library" §"Modules" requires this binding
//! surface. ADR-0012 "translate the surface, bind the core"
//! applies — Rust's `f64` already has the correct numerics; we
//! project a Cobrust-shaped surface.

// =====================================================================
// Constants
// =====================================================================

/// π (16 digits).
pub const PI: f64 = std::f64::consts::PI;

/// Euler's number (16 digits).
pub const E: f64 = std::f64::consts::E;

// =====================================================================
// Single-arg float ops
// =====================================================================

/// `√x`. NaN if `x < 0.0`.
pub fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

/// `x.powf(y)` — float exponent.
pub fn pow(x: f64, y: f64) -> f64 {
    x.powf(y)
}

/// `sin(x)` (radians).
pub fn sin(x: f64) -> f64 {
    x.sin()
}

/// `cos(x)` (radians).
pub fn cos(x: f64) -> f64 {
    x.cos()
}

/// `|x|` for floats.
pub fn abs_f64(x: f64) -> f64 {
    x.abs()
}

/// `|x|` for integers. Saturates at `i64::MAX` to avoid panicking
/// on `i64::MIN`. Constitution §2.2 forbids silent overflow paths.
pub fn abs_i64(x: i64) -> i64 {
    x.checked_abs().unwrap_or(i64::MAX)
}

/// `⌊x⌋`.
pub fn floor(x: f64) -> f64 {
    x.floor()
}

/// `⌈x⌉`.
pub fn ceil(x: f64) -> f64 {
    x.ceil()
}

/// Round half-away-from-zero (matches Python 2 / C semantics; Python
/// 3 uses banker's rounding, which Cobrust will gain via `round_even`
/// at M11.x).
pub fn round(x: f64) -> f64 {
    x.round()
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
    clippy::missing_panics_doc,
    clippy::approx_constant
)]
mod tests {
    use super::*;

    #[test]
    fn pi_close_to_3_14() {
        assert!((PI - 3.141_592_653_589_79).abs() < 1e-10);
    }

    #[test]
    fn e_close_to_2_71() {
        assert!((E - 2.718_281_828_459_04).abs() < 1e-10);
    }

    #[test]
    fn sqrt_zero() {
        assert_eq!(sqrt(0.0), 0.0);
    }

    #[test]
    fn sqrt_one() {
        assert_eq!(sqrt(1.0), 1.0);
    }

    #[test]
    fn sqrt_four() {
        assert_eq!(sqrt(4.0), 2.0);
    }

    #[test]
    fn sqrt_negative_is_nan() {
        assert!(sqrt(-1.0).is_nan());
    }

    #[test]
    fn pow_basic() {
        assert_eq!(pow(2.0, 3.0), 8.0);
    }

    #[test]
    fn pow_zero_zero() {
        assert_eq!(pow(0.0, 0.0), 1.0);
    }

    #[test]
    fn pow_negative_exponent() {
        assert_eq!(pow(2.0, -1.0), 0.5);
    }

    #[test]
    fn pow_fractional_exponent() {
        assert!((pow(4.0, 0.5) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn sin_zero() {
        assert!(sin(0.0).abs() < 1e-15);
    }

    #[test]
    fn sin_pi_over_two() {
        assert!((sin(PI / 2.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn cos_zero() {
        assert_eq!(cos(0.0), 1.0);
    }

    #[test]
    fn cos_pi() {
        assert!((cos(PI) - (-1.0)).abs() < 1e-10);
    }

    #[test]
    fn abs_f64_positive() {
        assert_eq!(abs_f64(1.0), 1.0);
    }

    #[test]
    fn abs_f64_negative() {
        assert_eq!(abs_f64(-1.0), 1.0);
    }

    #[test]
    fn abs_f64_zero() {
        assert_eq!(abs_f64(0.0), 0.0);
    }

    #[test]
    fn abs_f64_negative_zero() {
        assert_eq!(abs_f64(-0.0), 0.0);
    }

    #[test]
    fn abs_i64_positive() {
        assert_eq!(abs_i64(1), 1);
    }

    #[test]
    fn abs_i64_negative() {
        assert_eq!(abs_i64(-1), 1);
    }

    #[test]
    fn abs_i64_zero() {
        assert_eq!(abs_i64(0), 0);
    }

    #[test]
    fn abs_i64_min_saturates() {
        // i64::MIN's positive doesn't fit; saturate at i64::MAX.
        assert_eq!(abs_i64(i64::MIN), i64::MAX);
    }

    #[test]
    fn floor_positive() {
        assert_eq!(floor(1.7), 1.0);
    }

    #[test]
    fn floor_negative() {
        assert_eq!(floor(-1.2), -2.0);
    }

    #[test]
    fn floor_integer() {
        assert_eq!(floor(3.0), 3.0);
    }

    #[test]
    fn ceil_positive() {
        assert_eq!(ceil(1.2), 2.0);
    }

    #[test]
    fn ceil_negative() {
        assert_eq!(ceil(-1.7), -1.0);
    }

    #[test]
    fn ceil_integer() {
        assert_eq!(ceil(3.0), 3.0);
    }

    #[test]
    fn round_half_away_from_zero() {
        assert_eq!(round(0.5), 1.0);
        assert_eq!(round(-0.5), -1.0);
    }

    #[test]
    fn round_quarter_down() {
        assert_eq!(round(0.4), 0.0);
        assert_eq!(round(-0.4), 0.0);
    }

    #[test]
    fn round_three_quarters_up() {
        assert_eq!(round(0.75), 1.0);
        assert_eq!(round(-0.75), -1.0);
    }
}
