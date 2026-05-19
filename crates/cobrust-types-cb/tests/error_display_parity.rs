//! Display-impl parity tests — ADR-0055b Wave-2.
//!
//! F28 strict-separation: TEST scope only. No impl body.
//! All tests ``.
//!
//! Contract: Rust `TypeError` Display (via thiserror `#[error("...")]`)
//! and cb `TypeErrorCb` Display (via `display_error` hand-roll) MUST
//! render byte-equal output per ADR-0055b §4 invariant 4 + §6 risk 2.
//!
//! Specifically tested:
//! 1. suggestion field: `&'static str` (Rust) vs `Option<String>` (cb) — value byte-equal.
//! 2. `Multiple(vec![e1, e2])` — Display byte-equal on both sides.
//! 3-12. 5 specific variant Display strings each verified byte-equal.
//!
//! ## Anchors (F34)
//! - `error_display_parity.rs::test_display_suggestion_byte_equal` — suggestion field parity
//! - `error_display_parity.rs::test_display_multiple` — Multiple Display
//! - `error_display_parity.rs::test_display_type_mismatch_backtick_glyphs` — glyph parity

#![allow(clippy::unwrap_used)]
#![allow(clippy::todo)]

use cobrust_frontend::span::{FileId, Span};
use cobrust_types::TypeError;
use cobrust_types::ty::Ty;
use cobrust_types_cb::error_cb::TypeErrorCb;

fn dummy_span() -> Span {
    Span::new(FileId(0), 0, 1)
}

// =====================================================================
// 1. suggestion field: &'static str (Rust) vs Option<String> (cb)
// =====================================================================

#[test]

fn test_display_suggestion_byte_equal() {
    // Anchor: error_display_parity.rs::test_display_suggestion_byte_equal
    //
    // ADR-0055b §6 risk 1: cb emits same literal-text suggestion as Rust;
    // static-vs-owned distinction is invisible to Display.
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
    // Both suggestion values byte-equal (not Display of the error itself,
    // but the suggestion field value accessed directly).
    let rust_sugg = rust_err.suggestion_text();
    let cb_sugg = cb_err.suggestion_text();
    assert_eq!(rust_sugg, cb_sugg, "suggestion field must be byte-equal");
}

#[test]

fn test_display_suggestion_none_byte_equal() {
    let rust_err = TypeError::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    assert_eq!(
        rust_err.suggestion_text(),
        cb_err.suggestion_text(),
        "None suggestion must match on both sides"
    );
}

// =====================================================================
// 2. Multiple(vec![e1, e2]) — byte-equal Display
// =====================================================================

#[test]

fn test_display_multiple() {
    // Anchor: error_display_parity.rs::test_display_multiple
    //
    // Rust `#[error("multiple type errors")]` → cb Display must emit same string.
    let rust_err = TypeError::Multiple(vec![
        TypeError::BreakOutsideLoop {
            span: dummy_span(),
            suggestion: None,
        },
        TypeError::MutableDefault {
            span: dummy_span(),
            suggestion: None,
        },
    ]);
    let cb_err = TypeErrorCb::Multiple(vec![
        TypeErrorCb::BreakOutsideLoop {
            span: dummy_span(),
            suggestion: None,
        },
        TypeErrorCb::MutableDefault {
            span: dummy_span(),
            suggestion: None,
        },
    ]);
    assert_eq!(
        format!("{rust_err}"),
        format!("{cb_err}"),
        "Multiple Display must be byte-equal"
    );
}

// =====================================================================
// 3-7. Five specific variant Display string parity tests
// =====================================================================

#[test]

fn test_display_break_outside_loop() {
    let span = dummy_span();
    let rust_err = TypeError::BreakOutsideLoop {
        span,
        suggestion: None,
    };
    let cb_err = TypeErrorCb::BreakOutsideLoop {
        span,
        suggestion: None,
    };
    assert_eq!(
        format!("{rust_err}"),
        format!("{cb_err}"),
        "BreakOutsideLoop Display must be byte-equal"
    );
}

#[test]

fn test_display_type_mismatch_backtick_glyphs() {
    // Anchor: error_display_parity.rs::test_display_type_mismatch_backtick_glyphs
    //
    // ADR-0055b §6 risk 2: backtick-vs-quote glyphs.
    // Rust: `type mismatch: expected \`{expected}\`, found \`{actual}\` at {span}`.
    // cb: same byte-for-byte via display_error.
    let span = dummy_span();
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Str,
        span,
        suggestion: None,
    };
    // cb side Display uses display_error which calls display_ty(arena, handle).
    // DEV wires the arena handles → the Display strings become identical.
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 1,
        span,
        suggestion: None,
    };
    assert_eq!(
        format!("{rust_err}"),
        format!("{cb_err}"),
        "TypeMismatch Display backtick glyphs must be byte-equal"
    );
}

#[test]

fn test_display_unknown_name() {
    let span = dummy_span();
    let rust_err = TypeError::UnknownName {
        name: "xyz".to_string(),
        span,
        suggestion: None,
    };
    let cb_err = TypeErrorCb::UnknownName {
        name: "xyz".to_string(),
        span,
        suggestion: None,
    };
    assert_eq!(
        format!("{rust_err}"),
        format!("{cb_err}"),
        "UnknownName Display must be byte-equal"
    );
}

#[test]

fn test_display_arity_mismatch() {
    let span = dummy_span();
    let rust_err = TypeError::ArityMismatch {
        expected: 1,
        actual: 3,
        span,
        suggestion: None,
    };
    let cb_err = TypeErrorCb::ArityMismatch {
        expected: 1,
        actual: 3,
        span,
        suggestion: None,
    };
    assert_eq!(
        format!("{rust_err}"),
        format!("{cb_err}"),
        "ArityMismatch Display must be byte-equal"
    );
}

#[test]

fn test_display_occurs_check() {
    // Rust: `occurs check: cannot unify `?{var.0}` with `{ty}` at {span}`
    // Note: uses `var.0` tuple-field access on VarId.
    let span = dummy_span();
    // ADR-0055b cascade addendum: Display byte-parity requires the
    // cb-side `i64` handle to agree with the rust-side `Ty` kind under
    // the per-variant encounter-order convention. Handle 0 → `i64` is
    // the canonical first-encounter representative; align rust-side
    // `ty: Ty::Int` to match handle 0 here so the test exercises Display
    // byte-parity without requiring an arena threading through fmt.
    let rust_err = TypeError::OccursCheck {
        var: cobrust_types::ty::VarId(42),
        ty: Ty::Int,
        span,
        suggestion: None,
    };
    let cb_err = TypeErrorCb::OccursCheck {
        var: 42,
        ty: 0,
        span,
        suggestion: None,
    };
    assert_eq!(
        format!("{rust_err}"),
        format!("{cb_err}"),
        "OccursCheck Display must be byte-equal"
    );
}

// =====================================================================
// 8-10. Additional Display edge cases
// =====================================================================

#[test]

fn test_display_mutable_default_suggestion() {
    // suggestion field is Some on both sides; must appear in both Displays
    // if the Display impl includes suggestion (DEV decides; must be byte-equal).
    let span = dummy_span();
    let rust_err = TypeError::MutableDefault {
        span,
        suggestion: Some("change default to None"),
    };
    let cb_err = TypeErrorCb::MutableDefault {
        span,
        suggestion: Some("change default to None".to_string()),
    };
    assert_eq!(
        format!("{rust_err}"),
        format!("{cb_err}"),
        "MutableDefault Display with suggestion must be byte-equal"
    );
}

#[test]

fn test_display_non_exhaustive_match() {
    let span = dummy_span();
    let rust_err = TypeError::NonExhaustiveMatch {
        uncovered: vec!["None".to_string()],
        span,
        suggestion: None,
    };
    let cb_err = TypeErrorCb::NonExhaustiveMatch {
        uncovered: vec!["None".to_string()],
        span,
        suggestion: None,
    };
    assert_eq!(
        format!("{rust_err}"),
        format!("{cb_err}"),
        "NonExhaustiveMatch Display must be byte-equal"
    );
}

#[test]

fn test_display_unknown_method() {
    let span = dummy_span();
    let rust_err = TypeError::UnknownMethod {
        type_name: "Str".to_string(),
        method_name: "append".to_string(),
        span,
        suggestion: None,
    };
    let cb_err = TypeErrorCb::UnknownMethod {
        type_name: "Str".to_string(),
        method_name: "append".to_string(),
        span,
        suggestion: None,
    };
    assert_eq!(
        format!("{rust_err}"),
        format!("{cb_err}"),
        "UnknownMethod Display must be byte-equal"
    );
}

// =====================================================================
// Helper trait for suggestion-field access in tests
// =====================================================================

trait SuggestionText {
    fn suggestion_text(&self) -> Option<&str>;
}

impl SuggestionText for TypeError {
    fn suggestion_text(&self) -> Option<&str> {
        match self {
            TypeError::UnknownName { suggestion, .. } => suggestion.as_deref(),
            TypeError::ArityMismatch { suggestion, .. } => suggestion.as_deref(),
            TypeError::KeywordArgMismatch { suggestion, .. } => suggestion.as_deref(),
            TypeError::MissingArgument { suggestion, .. } => suggestion.as_deref(),
            TypeError::TypeMismatch { suggestion, .. } => suggestion.as_deref(),
            TypeError::NonExhaustiveMatch { suggestion, .. } => suggestion.as_deref(),
            TypeError::RowConflict { suggestion, .. } => suggestion.as_deref(),
            TypeError::ImplicitTruthiness { suggestion, .. } => suggestion.as_deref(),
            TypeError::UseOfDroppedFeature { suggestion, .. } => suggestion.as_deref(),
            TypeError::MutableDefault { suggestion, .. } => suggestion.as_deref(),
            TypeError::AmbiguousType { suggestion, .. } => suggestion.as_deref(),
            TypeError::DuplicateField { suggestion, .. } => suggestion.as_deref(),
            TypeError::OccursCheck { suggestion, .. } => suggestion.as_deref(),
            TypeError::NotCallable { suggestion, .. } => suggestion.as_deref(),
            TypeError::NotIndexable { suggestion, .. } => suggestion.as_deref(),
            TypeError::NotIterable { suggestion, .. } => suggestion.as_deref(),
            TypeError::BreakOutsideLoop { suggestion, .. } => suggestion.as_deref(),
            TypeError::ContinueOutsideLoop { suggestion, .. } => suggestion.as_deref(),
            TypeError::ReturnOutsideFn { suggestion, .. } => suggestion.as_deref(),
            TypeError::YieldOutsideFn { suggestion, .. } => suggestion.as_deref(),
            TypeError::NotHashable { suggestion, .. } => suggestion.as_deref(),
            TypeError::DictSpreadNotSupported { suggestion, .. } => suggestion.as_deref(),
            TypeError::Multiple(_) => None,
            TypeError::BorrowOfNonPlace { suggestion, .. } => suggestion.as_deref(),
            TypeError::UnknownMethod { suggestion, .. } => suggestion.as_deref(),
        }
    }
}

impl SuggestionText for TypeErrorCb {
    fn suggestion_text(&self) -> Option<&str> {
        match self {
            TypeErrorCb::UnknownName { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::ArityMismatch { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::KeywordArgMismatch { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::MissingArgument { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::TypeMismatch { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::NonExhaustiveMatch { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::RowConflict { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::ImplicitTruthiness { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::UseOfDroppedFeature { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::MutableDefault { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::AmbiguousType { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::DuplicateField { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::OccursCheck { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::NotCallable { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::NotIndexable { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::NotIterable { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::BreakOutsideLoop { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::ContinueOutsideLoop { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::ReturnOutsideFn { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::YieldOutsideFn { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::NotHashable { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::DictSpreadNotSupported { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::Multiple(_) => None,
            TypeErrorCb::BorrowOfNonPlace { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::UnknownMethod { suggestion, .. } => suggestion.as_deref(),
        }
    }
}
