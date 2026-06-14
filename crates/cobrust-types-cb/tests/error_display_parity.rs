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
// Each enum-variant arm forwards `suggestion.as_deref()` identically by design:
// the trait wants an exhaustive variant→suggestion projector and folding arms
// would lose the exhaustiveness check (ADR-0055b §4 invariant 1 — mirror).
#![allow(clippy::match_same_arms)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::doc_overindented_list_items)]

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
fn test_display_len_arg_not_sized() {
    // Anchor: error_display_parity.rs::test_display_len_arg_not_sized
    //
    // ADR-0088 §3 — `len(x)` on a non-sized arg. The Rust side renders
    // the `actual: Ty` payload (`Ty::Int` -> `i64`); the cb side renders
    // the convention handle 0 -> `i64`. The §2.5-B FIX-text (the accepted
    // sized-type set) must be byte-identical across the two impls.
    let span = dummy_span();
    let rust_err = TypeError::LenArgNotSized {
        actual: Ty::Int,
        span,
        suggestion: None,
    };
    let cb_err = TypeErrorCb::LenArgNotSized {
        actual: 0,
        span,
        suggestion: None,
    };
    let rendered = format!("{rust_err}");
    assert_eq!(
        rendered,
        format!("{cb_err}"),
        "LenArgNotSized Display must be byte-equal"
    );
    // The §2.5-B FIX names the accepted sized types and must NOT carry
    // the misleading "expected Dict".
    assert!(rendered.contains("str") && rendered.contains("list") && rendered.contains("dict"));
    assert!(!rendered.contains("expected Dict"));
}

#[test]
fn test_display_dora_unknown_output_id() {
    // Anchor: error_display_parity.rs::test_display_dora_unknown_output_id
    //
    // ADR-0092 — `event.send_output("<typo>", ...)` against a declared
    // `@dora.node(outputs=[...])` set. The §2.5-B FIX names the declared
    // output ids + the nearest-match "did you mean", all rendered from the
    // owned (dynamic) `id`/`declared`/`nearest` fields, and MUST render
    // byte-identically across the Rust and .cb Display impls — the byte-parity
    // tripwire for the dynamic per-node ids, not just the static suggestion.
    let span = dummy_span();
    let rust_err = TypeError::DoraUnknownOutputId {
        id: "twst".to_string(),
        declared: vec!["pose".to_string(), "twist".to_string()],
        nearest: Some("twist".to_string()),
        span,
        suggestion: None,
    };
    let cb_err = TypeErrorCb::DoraUnknownOutputId {
        id: "twst".to_string(),
        declared: vec!["pose".to_string(), "twist".to_string()],
        nearest: Some("twist".to_string()),
        span,
        suggestion: None,
    };
    let rendered = format!("{rust_err}");
    assert_eq!(
        rendered,
        format!("{cb_err}"),
        "DoraUnknownOutputId Display must be byte-equal"
    );
    // The §2.5-B FIX names the unknown id, the declared set, and the
    // nearest-match "did you mean".
    assert!(rendered.contains("unknown dora output id `twst`"));
    assert!(rendered.contains("declared outputs: [pose, twist]"));
    assert!(rendered.contains("did you mean `twist`?"));
}

#[test]
fn test_display_unsupported_slice_shape() {
    // Anchor: error_display_parity.rs::test_display_unsupported_slice_shape
    //
    // ADR-0093 Phase-2 — an unsupported `bytes` slice shape (open-ended /
    // stepped / negative bound) is rejected at compile time. The §2.5-B FIX
    // names the supported `b[lo:hi]` form, and MUST render byte-identically
    // across the Rust and .cb Display impls (the payload-free, suggestion-
    // only variant — the simplest mirror, so its Display is the cleanest
    // byte-parity tripwire).
    let span = dummy_span();
    let rust_err = TypeError::UnsupportedSliceShape {
        span,
        suggestion: None,
    };
    let cb_err = TypeErrorCb::UnsupportedSliceShape {
        span,
        suggestion: None,
    };
    let rendered = format!("{rust_err}");
    assert_eq!(
        rendered,
        format!("{cb_err}"),
        "UnsupportedSliceShape Display must be byte-equal"
    );
    // The §2.5-B FIX names the supported contiguous form + the deferred shapes.
    assert!(rendered.contains("unsupported `bytes` slice shape"));
    assert!(rendered.contains("b[1:len(b)]"));
}

#[test]
fn test_display_negative_pow_exponent() {
    // Anchor: error_display_parity.rs::test_display_negative_pow_exponent
    //
    // F90 / ADR-0102 — `int ** int` with a negative LITERAL exponent is a
    // compile-time reject. The §2.5-B FIX names the float-base remedy, and
    // MUST render byte-identically across the Rust and .cb Display impls
    // (payload-free, suggestion-only — the simplest mirror).
    let span = dummy_span();
    let rust_err = TypeError::NegativePowExponent {
        span,
        suggestion: None,
    };
    let cb_err = TypeErrorCb::NegativePowExponent {
        span,
        suggestion: None,
    };
    let rendered = format!("{rust_err}");
    assert_eq!(
        rendered,
        format!("{cb_err}"),
        "NegativePowExponent Display must be byte-equal"
    );
    // The §2.5-B FIX names the negative-exponent diagnosis + the float-base fix.
    assert!(rendered.contains("negative exponent"));
    assert!(rendered.contains("float(base) ** exp"));
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
            TypeError::CallbackArgMustBeFnName { suggestion, .. } => suggestion.as_deref(),
            TypeError::CallbackSignatureMismatch { suggestion, .. } => suggestion.as_deref(),
            TypeError::UnknownField { suggestion, .. } => suggestion.as_deref(),
            TypeError::UnsupportedRefinement { suggestion, .. } => suggestion.as_deref(),
            TypeError::LenArgNotSized { suggestion, .. } => suggestion.as_deref(),
            TypeError::DoraUnknownOutputId { suggestion, .. } => suggestion.as_deref(),
            TypeError::UnsupportedSliceShape { suggestion, .. } => suggestion.as_deref(),
            TypeError::NegativePowExponent { suggestion, .. } => suggestion.as_deref(),
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
            TypeErrorCb::CallbackArgMustBeFnName { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::CallbackSignatureMismatch { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::UnknownField { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::UnsupportedRefinement { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::LenArgNotSized { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::DoraUnknownOutputId { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::UnsupportedSliceShape { suggestion, .. } => suggestion.as_deref(),
            TypeErrorCb::NegativePowExponent { suggestion, .. } => suggestion.as_deref(),
        }
    }
}
