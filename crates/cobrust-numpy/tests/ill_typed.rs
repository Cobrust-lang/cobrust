//! M7.0 ill-typed program suite — at least 50 programs rejected.
//!
//! Per ADR-0013 §"M7.0 scope window": "≥ 50 ill-typed programs
//! rejected". The "type" check here is the runtime contract — most
//! shape/dtype/value mismatches that would be type errors in a richer
//! type system surface as `Result::Err(NumpyError { kind: ... })` at
//! the M7.0 surface; M7.1+ may lift some into compile-time errors as
//! the static core consumes cobrust-numpy.
//!
//! Each test asserts the expected `NumpyErrorKind` so we don't accept
//! the wrong rejection.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::approx_constant)]

use cobrust_numpy::{Dtype, NumpyErrorKind, arange, array, ones, zeros};

// ---- 1..15 — dtype string rejections (closed set per ADR-0013) ----------

#[test]
fn i01_dtype_complex128_now_supported_at_m76() {
    // M7.0 (ADR-0013) had complex128 as out-of-scope. M7.6 (ADR-0021 §3)
    // widens the dtype enum to include `Complex128`. The historical
    // "ill-typed" test is preserved as a regression marker that the
    // string is now accepted; the constructor-level surface still
    // returns `LinalgDtypeUnsupported` until the Array tagged-union
    // widening lands per ADR-0021 §"Consequences" follow-up.
    let dt = Dtype::from_python_string("complex128").unwrap();
    assert_eq!(dt, Dtype::Complex128);
}

#[test]
fn i02_dtype_unknown_int8() {
    let err = Dtype::from_python_string("int8").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i03_dtype_unknown_int16() {
    let err = Dtype::from_python_string("int16").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i04_dtype_unknown_uint32() {
    let err = Dtype::from_python_string("uint32").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i05_dtype_unknown_uint64() {
    let err = Dtype::from_python_string("uint64").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i06_dtype_unknown_float16() {
    let err = Dtype::from_python_string("float16").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i07_dtype_unknown_object() {
    let err = Dtype::from_python_string("object").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i08_dtype_unknown_str() {
    let err = Dtype::from_python_string("str").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i09_dtype_unknown_datetime64() {
    let err = Dtype::from_python_string("datetime64").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i10_dtype_unknown_empty_string() {
    let err = Dtype::from_python_string("").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i11_dtype_unknown_garbage() {
    let err = Dtype::from_python_string("not-a-dtype").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i12_dtype_case_sensitive_int32() {
    let err = Dtype::from_python_string("INT32").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i13_dtype_case_sensitive_bool() {
    let err = Dtype::from_python_string("Bool").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i14_dtype_complex64_now_supported_at_m76() {
    // M7.0 (ADR-0013) had complex64 as out-of-scope. M7.6 (ADR-0021 §3)
    // widens the dtype enum to include `Complex64`. The historical
    // "ill-typed" test is preserved as a regression marker that the
    // string is now accepted.
    let dt = Dtype::from_python_string("complex64").unwrap();
    assert_eq!(dt, Dtype::Complex64);
}

#[test]
fn i15_dtype_unknown_void() {
    let err = Dtype::from_python_string("void").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

// ---- 16..30 — array() shape mismatches ----------------------------------

#[test]
fn i16_array_too_few_values() {
    let err = array(&[1.0, 2.0], &[2, 2], Dtype::Int64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i17_array_too_many_values() {
    let err = array(&[1.0, 2.0, 3.0, 4.0, 5.0], &[2, 2], Dtype::Int32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i18_array_empty_buffer_nonempty_shape() {
    let err = array(&[], &[1], Dtype::Int64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i19_array_5_for_4_shape() {
    let err = array(&[1.0; 5], &[2, 2], Dtype::Float64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i20_array_3_for_4_shape() {
    let err = array(&[1.0, 2.0, 3.0], &[2, 2], Dtype::Float32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i21_array_3d_shape_off_by_one() {
    let err = array(&[1.0; 23], &[2, 3, 4], Dtype::Int64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i22_array_4d_short_buffer() {
    let err = array(&[1.0; 15], &[2, 2, 2, 2], Dtype::Int32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i23_array_scalar_shape_with_two_values() {
    let err = array(&[1.0, 2.0], &[], Dtype::Int64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i24_array_scalar_shape_with_no_values() {
    let err = array(&[], &[], Dtype::Float64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i25_array_1d_off_by_one_high() {
    let err = array(&[1.0; 11], &[10], Dtype::Int32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i26_array_1d_off_by_one_low() {
    let err = array(&[1.0; 9], &[10], Dtype::Int32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i27_array_bool_shape_mismatch() {
    let err = array(&[1.0, 0.0], &[3], Dtype::Bool).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i28_array_2x3_off_by_one() {
    let err = array(&[1.0; 7], &[2, 3], Dtype::Float64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i29_array_5x5_short() {
    let err = array(&[1.0; 24], &[5, 5], Dtype::Int64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i30_array_3x3_long() {
    let err = array(&[1.0; 10], &[3, 3], Dtype::Int32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

// ---- 31..40 — arange contract violations --------------------------------

#[test]
fn i31_arange_zero_step_int() {
    let err = arange(0.0, 5.0, 0.0, Dtype::Int64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn i32_arange_zero_step_float() {
    let err = arange(0.0, 1.0, 0.0, Dtype::Float64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn i33_arange_zero_step_int32() {
    let err = arange(0.0, 5.0, 0.0, Dtype::Int32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn i34_arange_zero_step_float32() {
    let err = arange(0.0, 5.0, 0.0, Dtype::Float32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn i35_arange_bool_unsupported() {
    let err = arange(0.0, 5.0, 1.0, Dtype::Bool).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BoolArangeUnsupported);
}

#[test]
fn i36_arange_bool_unsupported_with_negative_step() {
    let err = arange(5.0, 0.0, -1.0, Dtype::Bool).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::BoolArangeUnsupported);
}

#[test]
fn i37_arange_bool_unsupported_zero_step_priority() {
    // ZeroStep is checked before BoolArangeUnsupported (matches numpy
    // which raises ZeroDivisionError first).
    let err = arange(0.0, 5.0, 0.0, Dtype::Bool).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn i38_arange_bool_negative_zero_step() {
    let err = arange(0.0, 5.0, -0.0, Dtype::Int64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn i39_arange_bool_huge_range_with_zero_step() {
    let err = arange(0.0, 1e9, 0.0, Dtype::Int64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

#[test]
fn i40_arange_bool_negative_range_zero_step() {
    let err = arange(-5.0, 5.0, 0.0, Dtype::Float64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ZeroStep);
}

// ---- 41..55 — assorted rejection cases ----------------------------------

#[test]
fn i41_dtype_python_alias_unknown() {
    let err = Dtype::from_python_string("d").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i42_dtype_python_alias_unknown_l() {
    let err = Dtype::from_python_string("l").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i43_dtype_unicode() {
    let err = Dtype::from_python_string("U10").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i44_dtype_typespec_byteorder_unsupported() {
    let err = Dtype::from_python_string(">f8").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i45_dtype_typespec_unsupported_le() {
    let err = Dtype::from_python_string("<i4").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i46_array_long_buffer_for_zero_dim() {
    // A non-empty buffer for an empty zero-dim shape is a ShapeMismatch.
    let err = array(&[1.0], &[0], Dtype::Float64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i47_array_long_buffer_for_2d_zero_dim() {
    let err = array(&[1.0, 2.0, 3.0], &[3, 0], Dtype::Int64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i48_array_buffer_too_long_huge_shape() {
    let err = array(&[1.0; 100], &[7, 7], Dtype::Int32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i49_array_buffer_one_short_huge_shape() {
    let err = array(&[1.0; 48], &[7, 7], Dtype::Int32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i50_array_buffer_one_long_huge_shape() {
    let err = array(&[1.0; 50], &[7, 7], Dtype::Int32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i51_dtype_int128_unsupported() {
    let err = Dtype::from_python_string("int128").unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::UnsupportedDtype);
}

#[test]
fn i52_array_3d_off_by_one_high() {
    let err = array(&[1.0; 25], &[2, 3, 4], Dtype::Int64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i53_array_3d_off_by_two() {
    let err = array(&[1.0; 22], &[2, 3, 4], Dtype::Float64).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i54_array_4d_off_by_one() {
    let err = array(&[1.0; 17], &[2, 2, 2, 2], Dtype::Float32).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

#[test]
fn i55_array_5d_short() {
    let err = array(&[1.0; 31], &[2, 2, 2, 2, 2], Dtype::Bool).unwrap_err();
    assert_eq!(err.kind, NumpyErrorKind::ShapeMismatch);
}

// Extra sanity — confirm zeros/ones never produce shape-mismatch errors
// at the M7.0 surface (the type system enforces shape via &[usize]).

#[test]
fn i56_zeros_does_not_raise_for_typed_shapes() {
    let _ = zeros(&[3, 3], Dtype::Int32).unwrap();
    let _ = ones(&[3, 3], Dtype::Float64).unwrap();
}
