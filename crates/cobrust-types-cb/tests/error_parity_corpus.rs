//! Per-variant round-trip parity corpus — ADR-0055b Wave-2.
//!
//! F28 strict-separation: TEST scope only. No impl body present.
//! All tests are `#[ignore = "ADR-0055b Wave-2 DEV impl pending"]`.
//!
//! Each test constructs a Rust `TypeError` + a matching `TypeErrorCb`,
//! then asserts `parity_check(&rust_err, &cb_err) == Ok(())`.
//!
//! 25 tests — one per `TypeError` variant per ADR-0055b §4.1 compliance
//! matrix. Variant ordering matches `type_error_variant_name` in
//! `cobrust-types-parity`.
//!
//! ## Anchors (F34)
//! - `error_parity_corpus.rs::test_type_mismatch` — Class 3 representative
//! - `error_parity_corpus.rs::test_multiple` — recursive variant
//! - `error_parity_corpus.rs::test_occurs_check` — VarId-as-i64 variant

#![allow(clippy::unwrap_used)]
#![allow(clippy::todo)]

use cobrust_frontend::span::Span;
use cobrust_types::TypeError;
use cobrust_types::ty::{Ty, VarId};
use cobrust_types_cb::error_cb::{TypeErrorCb, type_error_cb_from_rust};
use cobrust_types_parity::{TyArena, parity_check};

fn dummy_span() -> Span {
    Span::new(0, 1)
}

// =====================================================================
// Class 1: Name-only variants (span + suggestion only) — 8 variants
// =====================================================================

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_break_outside_loop() {
    let rust_err = TypeError::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_continue_outside_loop() {
    let rust_err = TypeError::ContinueOutsideLoop {
        span: dummy_span(),
        suggestion: Some("remove the continue"),
    };
    let cb_err = TypeErrorCb::ContinueOutsideLoop {
        span: dummy_span(),
        suggestion: Some("remove the continue".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_return_outside_fn() {
    let rust_err = TypeError::ReturnOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::ReturnOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_yield_outside_fn() {
    let rust_err = TypeError::YieldOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::YieldOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_mutable_default() {
    let rust_err = TypeError::MutableDefault {
        span: dummy_span(),
        suggestion: Some("change default to None"),
    };
    let cb_err = TypeErrorCb::MutableDefault {
        span: dummy_span(),
        suggestion: Some("change default to None".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_ambiguous_type() {
    let rust_err = TypeError::AmbiguousType {
        span: dummy_span(),
        suggestion: Some("add a type annotation"),
    };
    let cb_err = TypeErrorCb::AmbiguousType {
        span: dummy_span(),
        suggestion: Some("add a type annotation".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_dict_spread_not_supported() {
    let rust_err = TypeError::DictSpreadNotSupported {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::DictSpreadNotSupported {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_borrow_of_non_place() {
    let rust_err = TypeError::BorrowOfNonPlace {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::BorrowOfNonPlace {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Class 2: String payload — 5 variants
// =====================================================================

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_unknown_name() {
    let rust_err = TypeError::UnknownName {
        name: "foo".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::UnknownName {
        name: "foo".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_keyword_arg_mismatch() {
    let rust_err = TypeError::KeywordArgMismatch {
        name: "verbose".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::KeywordArgMismatch {
        name: "verbose".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_missing_argument() {
    let rust_err = TypeError::MissingArgument {
        name: "x".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::MissingArgument {
        name: "x".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_duplicate_field() {
    let rust_err = TypeError::DuplicateField {
        name: "name".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::DuplicateField {
        name: "name".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_use_of_dropped_feature() {
    // UseOfDroppedFeature::name is &'static str in Rust → String in cb.
    let rust_err = TypeError::UseOfDroppedFeature {
        name: "is",
        span: dummy_span(),
        suggestion: Some("use same_object(a, b) instead"),
    };
    let cb_err = TypeErrorCb::UseOfDroppedFeature {
        name: "is".to_string(),
        span: dummy_span(),
        suggestion: Some("use same_object(a, b) instead".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Class 3: Ty payload (i64 arena handles) — 8 variants
// =====================================================================

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_type_mismatch() {
    // Anchor: error_parity_corpus.rs::test_type_mismatch
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Str,
        span: dummy_span(),
        suggestion: Some("change to 'x: int'"),
    };
    // cb side: arena handles assigned by type_error_cb_from_rust stub (DEV fills).
    // For contract shape test: construct directly with placeholder handles.
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 1,
        span: dummy_span(),
        suggestion: Some("change to 'x: int'".to_string()),
    };
    let mut arena = TyArena::new();
    // parity_check canonicalizes both sides: DEV impl aligns arena handles.
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_row_conflict() {
    let rust_err = TypeError::RowConflict {
        field: "age".to_string(),
        ty1: Ty::Int,
        ty2: Ty::Str,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::RowConflict {
        field: "age".to_string(),
        ty1: 0,
        ty2: 1,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_implicit_truthiness() {
    let rust_err = TypeError::ImplicitTruthiness {
        actual: Ty::Int,
        span: dummy_span(),
        suggestion: Some("change to 'if x != 0:'"),
    };
    let cb_err = TypeErrorCb::ImplicitTruthiness {
        actual: 0,
        span: dummy_span(),
        suggestion: Some("change to 'if x != 0:'".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_occurs_check() {
    // Anchor: error_parity_corpus.rs::test_occurs_check
    // VarId-as-i64: var field is i64 on the cb side.
    let rust_err = TypeError::OccursCheck {
        var: VarId(0),
        ty: Ty::List(Box::new(Ty::Var(VarId(0)))),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::OccursCheck {
        var: 0,
        ty: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_not_callable() {
    let rust_err = TypeError::NotCallable {
        actual: Ty::Int,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::NotCallable {
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_not_indexable() {
    let rust_err = TypeError::NotIndexable {
        actual: Ty::Bool,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::NotIndexable {
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_not_iterable() {
    let rust_err = TypeError::NotIterable {
        actual: Ty::Float,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::NotIterable {
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_not_hashable() {
    let rust_err = TypeError::NotHashable {
        actual: Ty::List(Box::new(Ty::Int)),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::NotHashable {
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Class 4: Composite + special — 5 variants
// =====================================================================

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_arity_mismatch() {
    let rust_err = TypeError::ArityMismatch {
        expected: 2,
        actual: 3,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::ArityMismatch {
        expected: 2,
        actual: 3,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_non_exhaustive_match() {
    let rust_err = TypeError::NonExhaustiveMatch {
        uncovered: vec!["None".to_string(), "Err".to_string()],
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::NonExhaustiveMatch {
        uncovered: vec!["None".to_string(), "Err".to_string()],
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_unknown_method() {
    let rust_err = TypeError::UnknownMethod {
        type_name: "Int".to_string(),
        method_name: "push".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::UnknownMethod {
        type_name: "Int".to_string(),
        method_name: "push".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_multiple_flat() {
    // Anchor: error_parity_corpus.rs::test_multiple
    // Multiple: flat list — ADR-0055b §2 callers flatten before construction.
    let rust_err = TypeError::Multiple(vec![
        TypeError::BreakOutsideLoop { span: dummy_span(), suggestion: None },
        TypeError::ReturnOutsideFn { span: dummy_span(), suggestion: None },
    ]);
    let cb_err = TypeErrorCb::Multiple(vec![
        TypeErrorCb::BreakOutsideLoop { span: dummy_span(), suggestion: None },
        TypeErrorCb::ReturnOutsideFn { span: dummy_span(), suggestion: None },
    ]);
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_multiple_nested_two_levels() {
    // ADR-0055b §2: harness corpus exercises ≤2-level Multiple.
    let inner = TypeError::Multiple(vec![
        TypeError::AmbiguousType { span: dummy_span(), suggestion: None },
    ]);
    let rust_err = TypeError::Multiple(vec![inner]);

    let cb_inner = TypeErrorCb::Multiple(vec![
        TypeErrorCb::AmbiguousType { span: dummy_span(), suggestion: None },
    ]);
    let cb_err = TypeErrorCb::Multiple(vec![cb_inner]);
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_multiple_singleton() {
    // Multiple([single_err]) — degenerate but valid.
    let rust_err = TypeError::Multiple(vec![
        TypeError::DictSpreadNotSupported { span: dummy_span(), suggestion: None },
    ]);
    let cb_err = TypeErrorCb::Multiple(vec![
        TypeErrorCb::DictSpreadNotSupported { span: dummy_span(), suggestion: None },
    ]);
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Bridge stub smoke tests
// =====================================================================

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_bridge_stub_unknown_name() {
    // type_error_cb_from_rust stub: verifies it can be called (will panic until DEV fills).
    let rust_err = TypeError::UnknownName {
        name: "bar".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    let cb_err = type_error_cb_from_rust(&rust_err, &mut arena);
    // After DEV impl: variant name must match.
    let rust_variant = cobrust_types_parity::type_error_variant_name(&rust_err);
    let cb_variant = cobrust_types_cb::error_cb::type_error_cb_variant_name(&cb_err);
    assert_eq!(rust_variant, cb_variant);
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_bridge_stub_type_mismatch() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Str,
        actual: Ty::Bool,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    let cb_err = type_error_cb_from_rust(&rust_err, &mut arena);
    let rust_variant = cobrust_types_parity::type_error_variant_name(&rust_err);
    let cb_variant = cobrust_types_cb::error_cb::type_error_cb_variant_name(&cb_err);
    assert_eq!(rust_variant, cb_variant);
}

#[test]
#[ignore = "ADR-0055b Wave-2 DEV impl pending"]
fn test_bridge_stub_multiple() {
    let rust_err = TypeError::Multiple(vec![
        TypeError::MutableDefault { span: dummy_span(), suggestion: None },
    ]);
    let mut arena = TyArena::new();
    let cb_err = type_error_cb_from_rust(&rust_err, &mut arena);
    let rust_variant = cobrust_types_parity::type_error_variant_name(&rust_err);
    let cb_variant = cobrust_types_cb::error_cb::type_error_cb_variant_name(&cb_err);
    assert_eq!(rust_variant, cb_variant);
}
