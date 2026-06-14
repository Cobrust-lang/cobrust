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

/// Natural logarithm. NaN if `x < 0`, -∞ if `x == 0`.
pub fn log(x: f64) -> f64 {
    x.ln()
}

/// `eˣ`.
pub fn exp(x: f64) -> f64 {
    x.exp()
}

/// `tan(x)` (radians).
pub fn tan(x: f64) -> f64 {
    x.tan()
}

// =====================================================================
// C-ABI shims — M-F.3.3 gap (b) intrinsic-rewrite targets.
// Each exported symbol matches the `__cobrust_math_*` name in
// `crates/cobrust-cli/src/build/intrinsics.rs`.
// =====================================================================

/// `sqrt(x) -> f64` C-ABI shim.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_sqrt(x: f64) -> f64 {
    x.sqrt()
}

/// `floor(x) -> f64` C-ABI shim.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_floor(x: f64) -> f64 {
    x.floor()
}

/// `ceil(x) -> f64` C-ABI shim.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_ceil(x: f64) -> f64 {
    x.ceil()
}

/// `round(x) -> f64` C-ABI shim.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_round(x: f64) -> f64 {
    x.round()
}

/// `abs(x) -> f64` C-ABI shim.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_abs(x: f64) -> f64 {
    x.abs()
}

/// ADR-0089 §5 — type-preserving `abs(x) -> i64` C-ABI shim. The
/// intrinsic-rewrite (`crates/cobrust-cli/src/build/intrinsics.rs`,
/// `Kind::MathAbs`) targets this symbol when the `abs(x)` argument
/// resolves to `Int` (Python's `abs` is type-preserving:
/// `abs(-5) == 5` an int, NOT `5.0`). Delegates to [`abs_i64`], which
/// saturates `i64::MIN` at `i64::MAX` to avoid the overflow panic
/// (Constitution §2.2 forbids silent overflow). DISTINCT from
/// `__cobrust_math_abs` (the f64 path).
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_int_abs(x: i64) -> i64 {
    abs_i64(x)
}

/// `pow(base, exp) -> f64` C-ABI shim.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_pow(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}

/// F90 / ADR-0102 — integer power `base ** exp -> i64` C-ABI shim, the
/// runtime target of the `.cb` `int ** int` operator. Cobrust's static
/// `int ** int -> int` rule pins an `i64` result (a negative-LITERAL
/// exponent is a COMPILE-TIME reject at `check.rs` `synth_bin`, §2.5-A);
/// this shim handles the runtime-dynamic cases the type checker cannot
/// see at compile time:
///
/// - **Negative runtime exponent** (`base ** n` where `n < 0` is NOT a
///   literal, e.g. a variable) — Python yields a *float* (`2 ** -1 ==
///   0.5`), but a `Ty::Int`-result shim cannot return a float without
///   silently changing the value's type (a §2.2 silent-coercion hole).
///   So we TRAP (panic → exit 3) rather than return a wrong-typed /
///   truncated value. The compile-time reject already covers the common
///   literal case with a FIX-printing diagnostic (§2.5-B); this trap is
///   the safety net for the dynamic case.
/// - **Overflow** (`2 ** 63` and beyond) — `i64::checked_pow` returns
///   `None`; we TRAP rather than silently WRAP (Constitution §2.2 forbids
///   silent overflow). CPython promotes to bignum here; Cobrust's i64 has
///   no bignum (yet), so a trap is the honest surface.
///
/// CPython identities preserved by `checked_pow`: `base ** 0 == 1` for
/// every base (incl. `0 ** 0 == 1`), `base ** 1 == base`. `exp` is `u32`
/// for `checked_pow`; a non-negative `exp` beyond `u32::MAX` is
/// astronomically unreachable for a non-overflowing `i64` base (only
/// `base ∈ {-1, 0, 1}` survive, and those are handled by the small-exp
/// fast path mathematically — `checked_pow` still caps the cast), so we
/// trap on an out-of-`u32`-range exponent too (overflow for `|base| >=
/// 2`, defensively uniform for the edge bases).
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_ipow(base: i64, exp: i64) -> i64 {
    if exp < 0 {
        crate::panic::panic(
            "integer ** with a negative exponent yields a non-integer (e.g. `2 ** -1 == 0.5`); \
             a negative power requires a float base — write `float(base) ** exp`",
        );
    }
    let Ok(exp_u32) = u32::try_from(exp) else {
        crate::panic::panic("integer ** overflow: exponent too large for i64 result");
    };
    match base.checked_pow(exp_u32) {
        Some(v) => v,
        None => crate::panic::panic(
            "integer ** overflow: result does not fit in i64 (Cobrust has no bignum; \
             use a float base, e.g. `float(base) ** exp`)",
        ),
    }
}

/// `sin(x) -> f64` C-ABI shim.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_sin(x: f64) -> f64 {
    x.sin()
}

/// `cos(x) -> f64` C-ABI shim.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_cos(x: f64) -> f64 {
    x.cos()
}

/// `tan(x) -> f64` C-ABI shim.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_tan(x: f64) -> f64 {
    x.tan()
}

/// `log(x) -> f64` C-ABI shim (natural log).
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_log(x: f64) -> f64 {
    x.ln()
}

/// `exp(x) -> f64` C-ABI shim.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_exp(x: f64) -> f64 {
    x.exp()
}

// =====================================================================
// ADR-0083 PART-2 — INT-returning rounding shims.
//
// CPython `math.floor` / `math.ceil` / `math.trunc` return a Python
// `int` (NOT a float): `math.floor(2.7) == 2` (an int), and on a
// NEGATIVE input the three DIVERGE — that divergence is load-bearing:
//
//   floor → round toward −∞   `floor(-1.5) == -2`
//   ceil  → round toward +∞    `ceil(-1.5) == -1`
//   trunc → round toward ZERO  `trunc(-1.5) == -1`, `trunc(1.9) == 1`
//
// These are DISTINCT symbols from the f64-returning `__cobrust_math_floor`
// / `_ceil` above (the bare-function `floor(x)` PRELUDE intrinsic path,
// `f64 -> f64`): the `_int` suffix marks the Python `math.`-qualified
// surface that returns `i64`. The `as i64` cast is the `f64::floor`
// result truncated to integer — exact for all in-`i64`-range inputs
// (Strict-tier: there is no last-ULP question, the value is an integer).
// =====================================================================

/// `math.floor(x) -> i64` C-ABI shim — round toward −∞, returning an
/// integer (CPython `math.floor`). DISTINCT from `__cobrust_math_floor`
/// (`f64 -> f64`, the bare-function path).
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_floor_int(x: f64) -> i64 {
    x.floor() as i64
}

/// `math.ceil(x) -> i64` C-ABI shim — round toward +∞, returning an
/// integer (CPython `math.ceil`).
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_ceil_int(x: f64) -> i64 {
    x.ceil() as i64
}

/// `math.trunc(x) -> i64` C-ABI shim — round toward ZERO, returning an
/// integer (CPython `math.trunc`). On a negative input this differs from
/// `floor`: `trunc(-1.5) == -1` whereas `floor(-1.5) == -2`.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_trunc_int(x: f64) -> i64 {
    x.trunc() as i64
}

// =====================================================================
// ADR-0083 PART-2 — BOOL-returning IEEE-754 classification shims.
//
// CPython `math.isnan` / `math.isinf` / `math.isfinite` return `bool`.
// The Rust C-ABI `-> bool` lowers to an LLVM `i1` return, mirroring
// `coil.any` / `coil.all` (the proven `Buffer -> bool` shape) and
// `__cobrust_fang_verify_password`. Strict-tier exact — the IEEE-754
// classification of an `f64` is unambiguous and platform-stable.
// =====================================================================

/// `math.isnan(x) -> bool` C-ABI shim — `True` iff `x` is NaN.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_isnan(x: f64) -> bool {
    x.is_nan()
}

/// `math.isinf(x) -> bool` C-ABI shim — `True` iff `x` is ±∞.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_isinf(x: f64) -> bool {
    x.is_infinite()
}

/// `math.isfinite(x) -> bool` C-ABI shim — `True` iff `x` is neither
/// NaN nor ±∞ (`isfinite(inf) == False`, `isfinite(nan) == False`).
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_isfinite(x: f64) -> bool {
    x.is_finite()
}

// =====================================================================
// ADR-0083 PART-2 — angle-conversion shims (`f64 -> f64`).
//
// CPython `math.degrees(pi) == 180.0`, `math.radians(180.0) == pi`.
// Rust's `f64::to_degrees` / `to_radians` are the EXACT same scaling
// (`x * 180/π` / `x * π/180`) — Strict-tier exact. `copysign` / `fmod`
// are NOT here: they are BARE libm two-arg symbols (declared in codegen
// alongside `pow` / `atan2` / `hypot`), no shim needed.
// =====================================================================

/// `math.degrees(x) -> f64` C-ABI shim — radians → degrees.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_degrees(x: f64) -> f64 {
    x.to_degrees()
}

/// `math.radians(x) -> f64` C-ABI shim — degrees → radians.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_math_radians(x: f64) -> f64 {
    x.to_radians()
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

    // F90 / ADR-0102 — `__cobrust_ipow` non-trapping integer-power cases.
    // The TRAP cases (negative exponent, overflow) call `process::exit(3)`
    // and so cannot be exercised in-process; they are covered end-to-end by
    // `cobrust-cli/tests/power_e2e.rs` (the built executable's exit code).
    #[test]
    fn ipow_basic_and_identities() {
        assert_eq!(__cobrust_ipow(2, 10), 1024);
        assert_eq!(__cobrust_ipow(3, 3), 27);
        assert_eq!(__cobrust_ipow(10, 3), 1000);
        // `base ** 0 == 1` for every base, incl. `0 ** 0 == 1` (CPython).
        assert_eq!(__cobrust_ipow(2, 0), 1);
        assert_eq!(__cobrust_ipow(0, 0), 1);
        assert_eq!(__cobrust_ipow(7, 0), 1);
        // `base ** 1 == base`.
        assert_eq!(__cobrust_ipow(5, 1), 5);
        assert_eq!(__cobrust_ipow(0, 5), 0);
        // negative BASE, non-negative exponent stays integer (sign tracks
        // parity): `(-2) ** 3 == -8`, `(-2) ** 2 == 4`.
        assert_eq!(__cobrust_ipow(-2, 3), -8);
        assert_eq!(__cobrust_ipow(-2, 2), 4);
        // just-below-overflow: `2 ** 62` fits in i64.
        assert_eq!(__cobrust_ipow(2, 62), 1i64 << 62);
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

    // -- ADR-0083 PART-2: int-returning rounding shims ------------------
    // Oracle (/opt/homebrew/bin/python3.11): the THREE diverge on a
    // negative input — that is the load-bearing distinction.

    #[test]
    fn floor_int_rounds_toward_neg_inf() {
        // math.floor(-1.5) == -2, math.floor(2.7) == 2.
        assert_eq!(__cobrust_math_floor_int(-1.5), -2);
        assert_eq!(__cobrust_math_floor_int(2.7), 2);
        assert_eq!(__cobrust_math_floor_int(3.0), 3);
    }

    #[test]
    fn ceil_int_rounds_toward_pos_inf() {
        // math.ceil(-1.5) == -1, math.ceil(2.1) == 3.
        assert_eq!(__cobrust_math_ceil_int(-1.5), -1);
        assert_eq!(__cobrust_math_ceil_int(2.1), 3);
        assert_eq!(__cobrust_math_ceil_int(3.0), 3);
    }

    #[test]
    fn trunc_int_rounds_toward_zero() {
        // math.trunc(-1.5) == -1 (NOT -2, distinguishing it from floor),
        // math.trunc(1.9) == 1.
        assert_eq!(__cobrust_math_trunc_int(-1.5), -1);
        assert_eq!(__cobrust_math_trunc_int(1.9), 1);
        assert_eq!(__cobrust_math_trunc_int(-1.9), -1);
    }

    #[test]
    fn floor_ceil_trunc_diverge_on_negative() {
        // The whole point of having all three: -1.5 maps differently.
        assert_eq!(__cobrust_math_floor_int(-1.5), -2);
        assert_eq!(__cobrust_math_ceil_int(-1.5), -1);
        assert_eq!(__cobrust_math_trunc_int(-1.5), -1);
    }

    // -- ADR-0083 PART-2: bool-returning IEEE-754 classification --------

    #[test]
    fn isnan_truth_table() {
        assert!(__cobrust_math_isnan(f64::NAN));
        assert!(!__cobrust_math_isnan(1.0));
        assert!(!__cobrust_math_isnan(f64::INFINITY));
    }

    #[test]
    fn isinf_truth_table() {
        assert!(__cobrust_math_isinf(f64::INFINITY));
        assert!(__cobrust_math_isinf(f64::NEG_INFINITY));
        assert!(!__cobrust_math_isinf(1.0));
        assert!(!__cobrust_math_isinf(f64::NAN));
    }

    #[test]
    fn isfinite_truth_table() {
        assert!(__cobrust_math_isfinite(1.0));
        assert!(!__cobrust_math_isfinite(f64::INFINITY));
        assert!(!__cobrust_math_isfinite(f64::NAN));
    }

    // -- ADR-0083 PART-2: angle conversion ------------------------------

    #[test]
    fn degrees_pi_is_180() {
        assert!((__cobrust_math_degrees(PI) - 180.0).abs() < 1e-10);
    }

    #[test]
    fn radians_180_is_pi() {
        assert!((__cobrust_math_radians(180.0) - PI).abs() < 1e-10);
    }

    #[test]
    fn degrees_radians_round_trip() {
        assert!((__cobrust_math_radians(__cobrust_math_degrees(1.0)) - 1.0).abs() < 1e-10);
    }
}
