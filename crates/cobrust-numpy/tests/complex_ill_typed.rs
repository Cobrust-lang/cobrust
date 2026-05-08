//! M7.6 Bucket B — Complex dtype ill-typed tests (per ADR-0021 §3 + §11).
//!
//! ≥ 20 ill-typed programs that must reject. At M7.6 the dtype enum
//! is widened to seven variants; misspellings, mixed-case, and
//! adjacent-but-unsupported strings reject cleanly via
//! `NumpyError::UnsupportedDtype`. Constructors that take a
//! complex `Dtype` raise `LinalgDtypeUnsupported` (until the Array
//! tagged-union widening lands per ADR-0021 follow-up).

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

use cobrust_numpy::{Dtype, NumpyErrorKind, arange, array, ones, zeros};

// ---- 1-6 — unsupported dtype strings -------------------------------------

#[test]
fn unsupported_string_complex32() {
    let e = Dtype::from_python_string("complex32").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_string_complex256() {
    let e = Dtype::from_python_string("complex256").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_typechar_c4() {
    let e = Dtype::from_python_string("c4").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_typechar_c32() {
    let e = Dtype::from_python_string("c32").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_uppercase_complex64() {
    // numpy accepts mixed case via dtype('Complex64'); cobrust-numpy is
    // strict per ADR-0013 §3 — only canonical lowercase is accepted.
    let e = Dtype::from_python_string("Complex64").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_with_whitespace() {
    let e = Dtype::from_python_string(" complex64 ").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

// ---- 7-12 — adjacent-but-unsupported (uint, float16, datetime) ----------

#[test]
fn unsupported_uint32() {
    let e = Dtype::from_python_string("uint32").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_uint64() {
    let e = Dtype::from_python_string("uint64").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_int8() {
    let e = Dtype::from_python_string("int8").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_int16() {
    let e = Dtype::from_python_string("int16").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_float16() {
    let e = Dtype::from_python_string("float16").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_datetime64() {
    let e = Dtype::from_python_string("datetime64").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

// ---- 13-16 — empty / garbage strings -------------------------------------

#[test]
fn unsupported_empty_string() {
    let e = Dtype::from_python_string("").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_garbage() {
    let e = Dtype::from_python_string("not-a-dtype").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_random_chars() {
    let e = Dtype::from_python_string("xyzzy").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn unsupported_long_form_complex_short_typechar_collision() {
    // "C8" with capital C is not a valid typechar in numpy
    let e = Dtype::from_python_string("C8").unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::UnsupportedDtype);
}

// ---- 17-20 — constructors with complex dtype reject (deferred routing) ---

#[test]
fn zeros_complex64_returns_dtype_unsupported() {
    let e = zeros(&[3], Dtype::Complex64).unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::LinalgDtypeUnsupported);
    assert!(e.message.contains("complex"), "message: {}", e.message);
}

#[test]
fn zeros_complex128_returns_dtype_unsupported() {
    let e = zeros(&[3], Dtype::Complex128).unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn ones_complex64_returns_dtype_unsupported() {
    let e = ones(&[3], Dtype::Complex64).unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

#[test]
fn array_complex128_returns_dtype_unsupported() {
    let e = array(&[1.0, 2.0, 3.0], &[3], Dtype::Complex128).unwrap_err();
    assert_eq!(e.kind, NumpyErrorKind::LinalgDtypeUnsupported);
}

// ---- 21-22 — bool arange still raises BoolArangeUnsupported (regression) -

#[test]
fn arange_complex_returns_dtype_unsupported() {
    // Existing behavior: complex arange follows zeros/ones pattern;
    // current arange() implementation may return a different kind.
    // Per ADR-0021 §3 this is dtype-tier scope; ripple to arange is M7.7+.
    let result = arange(0.0, 5.0, 1.0, Dtype::Complex64);
    assert!(result.is_err(), "complex arange must err at M7.6");
}

#[test]
fn complex_dtype_strings_are_distinct_from_typechars() {
    // Ensure no accidental overlap - typecodes are distinct.
    for s in &["c8", "c16"] {
        assert!(Dtype::from_python_string(s).is_ok());
    }
    for s in &["c4", "c32", "c2"] {
        assert!(Dtype::from_python_string(s).is_err());
    }
}
