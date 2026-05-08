//! M7.6 Bucket B — Complex dtype well-typed tests (per ADR-0021 §3 + §4).
//!
//! At M7.6 the dtype enum is widened to seven variants
//! (`Int32 / Int64 / Float32 / Float64 / Bool / Complex64 /
//! Complex128`). The `Array` tagged-union widening — and full
//! ufunc/linalg routing — is deferred to a follow-up sprint per
//! ADR-0021 §3 "Consequences"; this M7.6 binding ADR pins the
//! decisions and ships the dtype-tier surface that downstream consumers
//! observe (`Dtype::from_python_string`, `Dtype::to_python_string`,
//! `Dtype::item_size`, `Dtype::is_complex`, `Dtype::is_floating`, plus
//! `result_type` extended NEP 50 promotion).
//!
//! These ≥ 30 well-typed programs exercise the dtype surface across
//! happy paths.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::imprecise_flops)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::excessive_precision)]
#![allow(clippy::similar_names)]
#![allow(clippy::approx_constant)]
#![allow(clippy::uninlined_format_args)]

use cobrust_numpy::{Dtype, result_type, unary_math_dtype};

// ---- Test 1-4 — from_python_string for new dtype strings ----------------

#[test]
fn complex64_long_string_parses() {
    assert_eq!(
        Dtype::from_python_string("complex64").unwrap(),
        Dtype::Complex64
    );
}

#[test]
fn complex128_long_string_parses() {
    assert_eq!(
        Dtype::from_python_string("complex128").unwrap(),
        Dtype::Complex128
    );
}

#[test]
fn complex64_typechar_parses() {
    assert_eq!(Dtype::from_python_string("c8").unwrap(), Dtype::Complex64);
}

#[test]
fn complex128_typechar_parses() {
    assert_eq!(Dtype::from_python_string("c16").unwrap(), Dtype::Complex128);
}

// ---- Test 5-6 — to_python_string round-trip --------------------------------

#[test]
fn complex64_to_python_string() {
    assert_eq!(Dtype::Complex64.to_python_string(), "complex64");
}

#[test]
fn complex128_to_python_string() {
    assert_eq!(Dtype::Complex128.to_python_string(), "complex128");
}

// ---- Test 7-8 — to_rust_variant_name --------------------------------------

#[test]
fn complex64_rust_variant_name() {
    assert_eq!(Dtype::Complex64.to_rust_variant_name(), "Complex64");
}

#[test]
fn complex128_rust_variant_name() {
    assert_eq!(Dtype::Complex128.to_rust_variant_name(), "Complex128");
}

// ---- Test 9-10 — item_size matches ADR-0021 §3 -----------------------------

#[test]
fn complex64_item_size_is_8() {
    assert_eq!(Dtype::Complex64.item_size(), 8);
}

#[test]
fn complex128_item_size_is_16() {
    assert_eq!(Dtype::Complex128.item_size(), 16);
}

// ---- Test 11-14 — is_complex helper ---------------------------------------

#[test]
fn complex64_is_complex() {
    assert!(Dtype::Complex64.is_complex());
}

#[test]
fn complex128_is_complex() {
    assert!(Dtype::Complex128.is_complex());
}

#[test]
fn floats_are_not_complex() {
    assert!(!Dtype::Float32.is_complex());
    assert!(!Dtype::Float64.is_complex());
}

#[test]
fn ints_and_bool_are_not_complex() {
    assert!(!Dtype::Int32.is_complex());
    assert!(!Dtype::Int64.is_complex());
    assert!(!Dtype::Bool.is_complex());
}

// ---- Test 15-18 — is_floating helper --------------------------------------

#[test]
fn complex_dtypes_are_floating() {
    assert!(Dtype::Complex64.is_floating());
    assert!(Dtype::Complex128.is_floating());
}

#[test]
fn float_dtypes_are_floating() {
    assert!(Dtype::Float32.is_floating());
    assert!(Dtype::Float64.is_floating());
}

#[test]
fn integer_dtypes_are_not_floating() {
    assert!(!Dtype::Int32.is_floating());
    assert!(!Dtype::Int64.is_floating());
}

#[test]
fn bool_is_not_floating() {
    assert!(!Dtype::Bool.is_floating());
}

// ---- Test 19-22 — Display ------------------------------------------------

#[test]
fn complex64_display() {
    assert_eq!(format!("{}", Dtype::Complex64), "complex64");
}

#[test]
fn complex128_display() {
    assert_eq!(format!("{}", Dtype::Complex128), "complex128");
}

#[test]
fn complex64_debug_works() {
    let s = format!("{:?}", Dtype::Complex64);
    assert!(s.contains("Complex64"));
}

#[test]
fn complex128_debug_works() {
    let s = format!("{:?}", Dtype::Complex128);
    assert!(s.contains("Complex128"));
}

// ---- Test 23-30 — result_type NEP 50 complex extension --------------------

#[test]
fn complex64_with_complex64_stays_c64() {
    assert_eq!(
        result_type(Dtype::Complex64, Dtype::Complex64),
        Dtype::Complex64
    );
}

#[test]
fn complex128_with_complex128_stays_c128() {
    assert_eq!(
        result_type(Dtype::Complex128, Dtype::Complex128),
        Dtype::Complex128
    );
}

#[test]
fn complex64_with_float32_stays_c64() {
    assert_eq!(
        result_type(Dtype::Complex64, Dtype::Float32),
        Dtype::Complex64
    );
    assert_eq!(
        result_type(Dtype::Float32, Dtype::Complex64),
        Dtype::Complex64
    );
}

#[test]
fn complex64_with_bool_stays_c64() {
    assert_eq!(result_type(Dtype::Complex64, Dtype::Bool), Dtype::Complex64);
    assert_eq!(result_type(Dtype::Bool, Dtype::Complex64), Dtype::Complex64);
}

#[test]
fn complex64_with_float64_widens_to_c128() {
    assert_eq!(
        result_type(Dtype::Complex64, Dtype::Float64),
        Dtype::Complex128
    );
    assert_eq!(
        result_type(Dtype::Float64, Dtype::Complex64),
        Dtype::Complex128
    );
}

#[test]
fn complex64_with_int_widens_to_c128() {
    assert_eq!(
        result_type(Dtype::Complex64, Dtype::Int32),
        Dtype::Complex128
    );
    assert_eq!(
        result_type(Dtype::Complex64, Dtype::Int64),
        Dtype::Complex128
    );
}

#[test]
fn complex128_dominates_anything() {
    for d in [
        Dtype::Bool,
        Dtype::Int32,
        Dtype::Int64,
        Dtype::Float32,
        Dtype::Float64,
        Dtype::Complex64,
    ] {
        assert_eq!(result_type(Dtype::Complex128, d), Dtype::Complex128);
        assert_eq!(result_type(d, Dtype::Complex128), Dtype::Complex128);
    }
}

#[test]
fn complex_promotion_is_symmetric() {
    let all = [
        Dtype::Bool,
        Dtype::Int32,
        Dtype::Int64,
        Dtype::Float32,
        Dtype::Float64,
        Dtype::Complex64,
        Dtype::Complex128,
    ];
    for a in all {
        for b in all {
            assert_eq!(
                result_type(a, b),
                result_type(b, a),
                "promotion must be symmetric: {a:?} + {b:?}"
            );
        }
    }
}

// ---- Test 31-32 — unary_math_dtype preserves complex precision ------------

#[test]
fn unary_math_complex64_preserved() {
    assert_eq!(unary_math_dtype(Dtype::Complex64), Dtype::Complex64);
}

#[test]
fn unary_math_complex128_preserved() {
    assert_eq!(unary_math_dtype(Dtype::Complex128), Dtype::Complex128);
}
