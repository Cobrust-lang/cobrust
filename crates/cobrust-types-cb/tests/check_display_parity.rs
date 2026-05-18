//! Display byte-parity for check-produced TypeErrorCb — ADR-0055d Wave-3 Tier-2.
//!
//! F28 strict-separation: TEST scope only. No impl body present.
//!
//! Tests Display output byte-parity for TypeErrorCb variants that
//! can be produced by check.rs code paths. Exercises the Display
//! impl from error_cb.rs (0055b wave-2) under the convention-based
//! handle→ty-display mapping.
//!
//! All tests are `#[ignore = "ADR-0055d Wave-3 DEV impl pending"]`.
//!
//! ## F34 anchors
//! - `check_display_parity.rs::test_display_type_mismatch_check_site` — check-site representative
//! - `check_display_parity.rs::test_display_not_iterable_check_site` — iter_element path
//! - `check_display_parity.rs::test_display_implicit_truthiness_check_site` — expect_bool path

#![allow(clippy::unwrap_used)]
#![allow(clippy::todo)]

use cobrust_frontend::span::{FileId, Span};
use cobrust_types_cb::error_cb::TypeErrorCb;

fn dummy_span() -> Span {
    Span::new(FileId(0), 0, 1)
}

// =====================================================================
// Display parity: variants produced at synth_expr call sites
// Convention: handle 0 = i64, 1 = str, 2 = bool, 3 = f64 per error_cb.rs.
// =====================================================================

/// Display: TypeMismatch produced at synth_bin (Int expected, Str actual).
/// check.rs::Ctx::synth_bin → TypeError::TypeMismatch.
/// F34 anchor: check_display_parity.rs::test_display_type_mismatch_check_site
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_type_mismatch_check_site() {
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0, // i64
        actual: 1,   // str
        span: dummy_span(),
        suggestion: Some("change to 'x: int'".to_string()),
    };
    let display = format!("{cb_err}");
    // Must match: "type mismatch: expected `i64`, found `str` at <span>"
    assert!(
        display.contains("type mismatch"),
        "Display must contain 'type mismatch': got '{display}'"
    );
    assert!(
        display.contains("i64"),
        "Display must contain expected type 'i64': got '{display}'"
    );
    assert!(
        display.contains("str"),
        "Display must contain actual type 'str': got '{display}'"
    );
}

/// Display: NotIterable produced at iter_element (Int is not iterable).
/// check.rs::Ctx::iter_element → TypeError::NotIterable.
/// F34 anchor: check_display_parity.rs::test_display_not_iterable_check_site
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_not_iterable_check_site() {
    let cb_err = TypeErrorCb::NotIterable {
        actual: 0, // i64
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("not iterable"),
        "Display must contain 'not iterable': got '{display}'"
    );
    assert!(
        display.contains("i64"),
        "Display must show actual type 'i64': got '{display}'"
    );
}

/// Display: ImplicitTruthiness produced at expect_bool (Int in if-condition).
/// check.rs::Ctx::expect_bool → TypeError::ImplicitTruthiness.
/// F34 anchor: check_display_parity.rs::test_display_implicit_truthiness_check_site
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_implicit_truthiness_check_site() {
    let cb_err = TypeErrorCb::ImplicitTruthiness {
        actual: 0, // i64
        span: dummy_span(),
        suggestion: Some("change to 'if x != 0:'".to_string()),
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("non-bool used in truthiness position"),
        "Display must contain truthiness message: got '{display}'"
    );
    assert!(
        display.contains("i64"),
        "Display must show actual type 'i64': got '{display}'"
    );
}

/// Display: NotCallable produced at synth_call (Int not callable).
/// check.rs::Ctx::synth_call → TypeError::NotCallable.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_not_callable_check_site() {
    let cb_err = TypeErrorCb::NotCallable {
        actual: 0, // i64
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("not callable"),
        "Display must contain 'not callable': got '{display}'"
    );
    assert!(
        display.contains("i64"),
        "Display must show actual type: got '{display}'"
    );
}

/// Display: NotIndexable produced at Index arm (Float not indexable).
/// check.rs::Ctx::synth_expr Index arm → TypeError::NotIndexable.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_not_indexable_check_site() {
    let cb_err = TypeErrorCb::NotIndexable {
        actual: 3, // f64
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("not indexable"),
        "Display must contain 'not indexable': got '{display}'"
    );
    assert!(
        display.contains("f64"),
        "Display must show actual type 'f64': got '{display}'"
    );
}

/// Display: NotHashable produced at Dict arm (List key rejected).
/// check.rs::Ctx::synth_expr Dict arm → TypeError::NotHashable.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_not_hashable_check_site() {
    // List handle — any non-standard handle produces "?_" per convention.
    let cb_err = TypeErrorCb::NotHashable {
        actual: 99, // unknown handle → "?_"
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("not Hashable"),
        "Display must contain 'not Hashable': got '{display}'"
    );
}

/// Display: ArityMismatch produced at synth_call.
/// check.rs::Ctx::synth_call → TypeError::ArityMismatch.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_arity_mismatch_check_site() {
    let cb_err = TypeErrorCb::ArityMismatch {
        expected: 2,
        actual: 4,
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("expected 2 arguments"),
        "Display must show 'expected 2 arguments': got '{display}'"
    );
    assert!(
        display.contains("got 4"),
        "Display must show 'got 4': got '{display}'"
    );
}

/// Display: UnknownName produced at lookup_resolved.
/// check.rs::Ctx::lookup_resolved → TypeError::UnknownName.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_unknown_name_check_site() {
    let cb_err = TypeErrorCb::UnknownName {
        name: "missing_var".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("unknown name"),
        "Display must contain 'unknown name': got '{display}'"
    );
    assert!(
        display.contains("missing_var"),
        "Display must contain the name: got '{display}'"
    );
}

/// Display: AmbiguousType produced at check() top-level finalization.
/// check.rs::check() → TypeError::AmbiguousType on leaked free vars.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_ambiguous_type_check_top() {
    let cb_err = TypeErrorCb::AmbiguousType {
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("ambiguous type"),
        "Display must contain 'ambiguous type': got '{display}'"
    );
}

/// Display: DictSpreadNotSupported produced at Dict arm spread rejection.
/// check.rs::Ctx::synth_expr Dict arm DictEntry::Spread → TypeError::DictSpreadNotSupported.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_dict_spread_not_supported() {
    let cb_err = TypeErrorCb::DictSpreadNotSupported {
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("dict spread"),
        "Display must mention 'dict spread': got '{display}'"
    );
}

/// Display: BorrowOfNonPlace produced at Borrow arm non-place rejection.
/// check.rs::Ctx::synth_expr Borrow arm → TypeError::BorrowOfNonPlace.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_borrow_of_non_place() {
    let cb_err = TypeErrorCb::BorrowOfNonPlace {
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("cannot borrow non-place"),
        "Display must mention borrow restriction: got '{display}'"
    );
}

/// Display: MutableDefault produced at lower_default_type.
/// check.rs::Ctx::lower_default_type → TypeError::MutableDefault.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_mutable_default() {
    let cb_err = TypeErrorCb::MutableDefault {
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("mutable default argument"),
        "Display must mention mutable default: got '{display}'"
    );
}

/// Display: NonExhaustiveMatch produced at check_match exhaustiveness check.
/// check.rs::Ctx::check_match → TypeError::NonExhaustiveMatch.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_non_exhaustive_match() {
    let cb_err = TypeErrorCb::NonExhaustiveMatch {
        uncovered: vec!["True".to_string(), "False".to_string()],
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("non-exhaustive match"),
        "Display must mention non-exhaustive match: got '{display}'"
    );
    assert!(
        display.contains("True") || display.contains("False"),
        "Display must include uncovered cases: got '{display}'"
    );
}

/// Display: UnknownMethod produced at try_synth_method_call fallthrough.
/// check.rs::Ctx::try_synth_method_call chain → TypeError::UnknownMethod.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_unknown_method() {
    let cb_err = TypeErrorCb::UnknownMethod {
        type_name: "Int".to_string(),
        method_name: "unknown_method".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("not found on"),
        "Display must contain 'not found on': got '{display}'"
    );
    assert!(
        display.contains("unknown_method"),
        "Display must contain method name: got '{display}'"
    );
    assert!(
        display.contains("Int"),
        "Display must contain type name: got '{display}'"
    );
}

/// Display: BreakOutsideLoop produced at check_stmt Break arm.
/// check.rs::Ctx::check_stmt StmtKind::Break → TypeError::BreakOutsideLoop.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_break_outside_loop() {
    let cb_err = TypeErrorCb::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("`break` outside"),
        "Display must contain 'break outside': got '{display}'"
    );
}

/// Display: YieldOutsideFn produced at Yield/YieldFrom arms.
/// check.rs::Ctx::synth_expr Yield arm → TypeError::YieldOutsideFn.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_yield_outside_fn() {
    let cb_err = TypeErrorCb::YieldOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("`yield` outside"),
        "Display must contain 'yield outside': got '{display}'"
    );
}

/// Display: Multiple wraps check-site errors; output is generic "multiple type errors".
/// check.rs::Ctx::synth_comp / check_match → TypeError::Multiple.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_multiple_check_aggregated() {
    let cb_err = TypeErrorCb::Multiple(vec![
        TypeErrorCb::BreakOutsideLoop {
            span: dummy_span(),
            suggestion: None,
        },
        TypeErrorCb::UnknownName {
            name: "x".to_string(),
            span: dummy_span(),
            suggestion: None,
        },
    ]);
    let display = format!("{cb_err}");
    assert_eq!(
        display, "multiple type errors",
        "Multiple Display must be exactly 'multiple type errors': got '{display}'"
    );
}

/// Display: OccursCheck produced when unify detects infinite type.
/// check.rs::Ctx::synth_expr propagated from infer::unify → TypeError::OccursCheck.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_display_occurs_check() {
    let cb_err = TypeErrorCb::OccursCheck {
        var: 0,
        ty: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let display = format!("{cb_err}");
    assert!(
        display.contains("occurs check"),
        "Display must contain 'occurs check': got '{display}'"
    );
    assert!(
        display.contains("?0"),
        "Display must contain var glyph '?0': got '{display}'"
    );
}
