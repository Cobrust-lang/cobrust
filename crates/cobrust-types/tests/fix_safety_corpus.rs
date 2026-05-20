//! ADR-0062 §6 — FixSafety acceptance gate corpus.
//!
//! Five+ unit tests that cover the six-tier ladder + Display wire form
//! + per-variant TypeError tier classification + Suggestion struct
//! shape. Every test references the ADR row it covers in the doc-string.
//!
//! Pre-reads:
//! - `docs/agent/adr/0062-fix-safety-ladder.md` §3 + §6.
//! - `crates/cobrust-types/src/fix_safety.rs` (the public surface).

#![allow(clippy::uninlined_format_args)]
#![allow(clippy::single_match_else)]

use cobrust_frontend::span::{FileId, Span};
use cobrust_types::{
    FixSafety, Suggestion, TypeError, type_error_fix_safety, type_error_suggestion_text,
};
use cobrust_types::ty::{Ty, VarId};

fn span() -> Span {
    Span::new(FileId::SYNTHETIC, 0, 1)
}

// ===========================================================================
// §6.1 — six-tier ladder, lowest → highest (ADR-0062 §6 acceptance gate row 2)
// ===========================================================================

#[test]
fn t01_fix_safety_default_is_requires_human_review() {
    // ADR-0062 §6 row 1 + §3.1 conservative default.
    assert_eq!(FixSafety::default(), FixSafety::RequiresHumanReview);
}

#[test]
fn t02_fix_safety_ladder_ord_low_to_high() {
    // ADR-0062 §6 row 2 + §3.1 declaration order = tier order.
    assert!(FixSafety::FormatOnly < FixSafety::BehaviorPreserving);
    assert!(FixSafety::BehaviorPreserving < FixSafety::LocalEdit);
    assert!(FixSafety::LocalEdit < FixSafety::ApiChanging);
    assert!(FixSafety::ApiChanging < FixSafety::TargetChanging);
    assert!(FixSafety::TargetChanging < FixSafety::RequiresHumanReview);
}

#[test]
fn t03_fix_safety_requires_human_review_is_top() {
    // ADR-0062 §6 row 3.
    let all_lower = [
        FixSafety::FormatOnly,
        FixSafety::BehaviorPreserving,
        FixSafety::LocalEdit,
        FixSafety::ApiChanging,
        FixSafety::TargetChanging,
    ];
    for lower in all_lower {
        assert!(FixSafety::RequiresHumanReview > lower);
    }
}

// ===========================================================================
// §6.2 — kebab-case Display wire form (ADR-0062 §1.2 Zero precedent)
// ===========================================================================

#[test]
fn t04_fix_safety_display_kebab_case_all_tiers() {
    // ADR-0062 §6 row 4 — Zero-language wire form.
    let pairs = [
        (FixSafety::FormatOnly, "format-only"),
        (FixSafety::BehaviorPreserving, "behavior-preserving"),
        (FixSafety::LocalEdit, "local-edit"),
        (FixSafety::ApiChanging, "api-changing"),
        (FixSafety::TargetChanging, "target-changing"),
        (FixSafety::RequiresHumanReview, "requires-human-review"),
    ];
    for (tier, expected) in pairs {
        assert_eq!(tier.to_string(), expected);
    }
}

// ===========================================================================
// §6.3 — Suggestion struct shape (ADR-0062 §3.2)
// ===========================================================================

#[test]
fn t05_suggestion_new_carries_message_and_safety() {
    // ADR-0062 §6 row 5 — construction shape.
    let s = Suggestion::new("change to `if x != 0:`", FixSafety::BehaviorPreserving);
    assert_eq!(s.message, "change to `if x != 0:`");
    assert_eq!(s.fix_safety, FixSafety::BehaviorPreserving);
    assert!(s.replacement.is_none());
}

#[test]
fn t06_suggestion_with_replacement_carries_substitution() {
    let s = Suggestion::with_replacement(
        "change to `if x != 0:`",
        FixSafety::BehaviorPreserving,
        "if x != 0:",
    );
    assert_eq!(s.replacement.as_deref(), Some("if x != 0:"));
}

// ===========================================================================
// §6.4 — per-TypeError-variant tier classification (ADR-0062 §3.3)
// ===========================================================================

#[test]
fn t07_implicit_truthiness_is_behavior_preserving() {
    // §2.5 canonical: `if x:` → `if x != 0:` preserves semantics.
    let err = TypeError::ImplicitTruthiness {
        actual: Ty::Int,
        span: span(),
        suggestion: Some("change to `if x != 0:`"),
    };
    assert_eq!(type_error_fix_safety(&err), FixSafety::BehaviorPreserving);
}

#[test]
fn t08_unknown_name_is_local_edit() {
    let err = TypeError::UnknownName {
        name: "x".into(),
        span: span(),
        suggestion: Some("declare with `let <name> = …` first"),
    };
    assert_eq!(type_error_fix_safety(&err), FixSafety::LocalEdit);
}

#[test]
fn t09_occurs_check_is_requires_human_review() {
    // Recursive-type impossibility — beyond compiler auto-fix capability.
    let err = TypeError::OccursCheck {
        var: VarId(0),
        ty: Ty::Int,
        span: span(),
        suggestion: Some("add a type annotation"),
    };
    assert_eq!(type_error_fix_safety(&err), FixSafety::RequiresHumanReview);
}

#[test]
fn t10_mutable_default_is_behavior_preserving() {
    // Compiler-mandated `None`-default rewrite; Python semantics never
    // matched the user's apparent intent.
    let err = TypeError::MutableDefault {
        span: span(),
        suggestion: Some("use `None` as the default"),
    };
    assert_eq!(type_error_fix_safety(&err), FixSafety::BehaviorPreserving);
}

#[test]
fn t11_arity_mismatch_is_local_edit() {
    let err = TypeError::ArityMismatch {
        expected: 1,
        actual: 0,
        span: span(),
        suggestion: Some("add the missing argument at the call site"),
    };
    assert_eq!(type_error_fix_safety(&err), FixSafety::LocalEdit);
}

// ===========================================================================
// §6.5 — Suggestion::for_type_error roundtrip (combine §3.2 + §3.3)
// ===========================================================================

#[test]
fn t12_suggestion_for_type_error_implicit_truthiness() {
    let err = TypeError::ImplicitTruthiness {
        actual: Ty::Int,
        span: span(),
        suggestion: Some("change to `if x != 0:` (use `.is_some()` for Option)"),
    };
    let s = Suggestion::for_type_error(&err).expect("populated");
    assert_eq!(s.fix_safety, FixSafety::BehaviorPreserving);
    assert!(s.message.contains("if x != 0"));
    // replacement is forward-compat empty for Wave-1; future micro-ADR
    // may populate it incrementally per ADR-0062 §7 risk row 4.
    assert!(s.replacement.is_none());
}

#[test]
fn t13_suggestion_for_type_error_returns_none_on_empty_suggestion() {
    let err = TypeError::ImplicitTruthiness {
        actual: Ty::Int,
        span: span(),
        suggestion: None,
    };
    assert!(Suggestion::for_type_error(&err).is_none());
}

#[test]
fn t14_type_error_suggestion_text_extracts_static_str() {
    let err = TypeError::ImplicitTruthiness {
        actual: Ty::Int,
        span: span(),
        suggestion: Some("change to `if x != 0:`"),
    };
    assert_eq!(
        type_error_suggestion_text(&err),
        Some("change to `if x != 0:`")
    );
}

#[test]
fn t15_type_error_multiple_has_no_singular_suggestion() {
    // Multiple aggregates children; the aggregate has no singular suggestion.
    let err = TypeError::Multiple(vec![]);
    assert!(type_error_suggestion_text(&err).is_none());
    // The aggregate tier defaults to RequiresHumanReview (conservative).
    assert_eq!(type_error_fix_safety(&err), FixSafety::RequiresHumanReview);
}
