//! ADR-0062 §3.5 — LSP CodeAction gating by FixSafety tier.
//!
//! Maps a diagnostic's FixSafety tier to the `CodeActionKind` the LSP
//! client should expose:
//!
//! | Tier | `CodeActionKind` | Auto-apply behaviour |
//! |---|---|---|
//! | `FormatOnly` | `SOURCE_FIX_ALL` | Applied on save / format pass |
//! | `BehaviorPreserving` | `QUICK_FIX` | Apply on user accept |
//! | `LocalEdit` | `QUICK_FIX` | Apply on user accept |
//! | `ApiChanging` | `REFACTOR` | Suggest only, no quick apply |
//! | `TargetChanging` | `EMPTY` | Diagnostic-only, no code action |
//! | `RequiresHumanReview` | `EMPTY` | Diagnostic-only, no code action |
//!
//! `code_action_kind_for_fix_safety` returns `Option<CodeActionKind>`:
//! `None` means "do not emit a CodeAction for this diagnostic" (the
//! LSP wire-shape sends the suggestion text in `related_information`
//! only).

use cobrust_hir::LoweringError;
use cobrust_mir::MirError;
use cobrust_types::{FixSafety, TypeError, type_error_fix_safety};
use tower_lsp::lsp_types::CodeActionKind;

/// Look up the LSP CodeAction kind for a given FixSafety tier.
///
/// ADR-0062 §3.5 gating matrix. `None` means the diagnostic should
/// be surfaced as message-only (no quick apply).
#[must_use]
pub fn code_action_kind_for_fix_safety(tier: FixSafety) -> Option<CodeActionKind> {
    match tier {
        FixSafety::FormatOnly => Some(CodeActionKind::SOURCE_FIX_ALL),
        FixSafety::BehaviorPreserving | FixSafety::LocalEdit => Some(CodeActionKind::QUICKFIX),
        FixSafety::ApiChanging => Some(CodeActionKind::REFACTOR),
        FixSafety::TargetChanging | FixSafety::RequiresHumanReview => None,
    }
}

/// Convenience: look up the CodeAction kind for a `TypeError`.
///
/// Routes through `cobrust_types::type_error_fix_safety` for the
/// per-variant tier classification.
#[must_use]
pub fn code_action_kind_for_type_error(err: &TypeError) -> Option<CodeActionKind> {
    code_action_kind_for_fix_safety(type_error_fix_safety(err))
}

/// Convenience: look up the CodeAction kind for a `MirError`.
///
/// Widens the opaque `u8` tier code (from
/// `cobrust_mir::mir_error_fix_safety_code`) into `FixSafety` here at
/// the LSP-adapter boundary so `cobrust-mir` stays independent of
/// `cobrust-types`.
#[must_use]
pub fn code_action_kind_for_mir_error(err: &MirError) -> Option<CodeActionKind> {
    let code = cobrust_mir::mir_error_fix_safety_code(err);
    code_action_kind_for_fix_safety(fix_safety_from_code(code))
}

/// Convenience: look up the CodeAction kind for a `LoweringError`.
#[must_use]
pub fn code_action_kind_for_lowering_error(err: &LoweringError) -> Option<CodeActionKind> {
    let code = cobrust_hir::lowering_error_fix_safety_code(err);
    code_action_kind_for_fix_safety(fix_safety_from_code(code))
}

/// Decode the opaque `u8` tier code (per `mir_error_fix_safety_code`
/// / `lowering_error_fix_safety_code`) into `FixSafety`.
///
/// Out-of-range codes default to `RequiresHumanReview` per the
/// conservative-default rule (ADR-0062 §3.1 + §7 risk row 1).
#[must_use]
pub fn fix_safety_from_code(code: u8) -> FixSafety {
    match code {
        0 => FixSafety::FormatOnly,
        1 => FixSafety::BehaviorPreserving,
        2 => FixSafety::LocalEdit,
        3 => FixSafety::ApiChanging,
        4 => FixSafety::TargetChanging,
        _ => FixSafety::RequiresHumanReview,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cobrust_frontend::span::{FileId, Span};

    fn span() -> Span {
        Span::new(FileId::SYNTHETIC, 0, 1)
    }

    #[test]
    fn format_only_routes_to_source_fix_all() {
        assert_eq!(
            code_action_kind_for_fix_safety(FixSafety::FormatOnly),
            Some(CodeActionKind::SOURCE_FIX_ALL)
        );
    }

    #[test]
    fn behavior_preserving_routes_to_quickfix() {
        assert_eq!(
            code_action_kind_for_fix_safety(FixSafety::BehaviorPreserving),
            Some(CodeActionKind::QUICKFIX)
        );
    }

    #[test]
    fn local_edit_routes_to_quickfix() {
        assert_eq!(
            code_action_kind_for_fix_safety(FixSafety::LocalEdit),
            Some(CodeActionKind::QUICKFIX)
        );
    }

    #[test]
    fn api_changing_routes_to_refactor() {
        assert_eq!(
            code_action_kind_for_fix_safety(FixSafety::ApiChanging),
            Some(CodeActionKind::REFACTOR)
        );
    }

    #[test]
    fn target_changing_returns_none() {
        assert!(code_action_kind_for_fix_safety(FixSafety::TargetChanging).is_none());
    }

    #[test]
    fn requires_human_review_returns_none() {
        assert!(code_action_kind_for_fix_safety(FixSafety::RequiresHumanReview).is_none());
    }

    #[test]
    fn implicit_truthiness_emits_quickfix() {
        // §2.5 canonical — `if x:` → `if x != 0:` is BehaviorPreserving
        // and surfaces as a QUICKFIX code-action.
        let err = TypeError::ImplicitTruthiness {
            actual: cobrust_types::ty::Ty::Int,
            span: span(),
            suggestion: Some("change to `if x != 0:`"),
        };
        assert_eq!(
            code_action_kind_for_type_error(&err),
            Some(CodeActionKind::QUICKFIX)
        );
    }

    #[test]
    fn occurs_check_emits_no_action() {
        // RequiresHumanReview → no CodeAction.
        let err = TypeError::OccursCheck {
            var: cobrust_types::ty::VarId(0),
            ty: cobrust_types::ty::Ty::Int,
            span: span(),
            suggestion: Some("add a type annotation"),
        };
        assert!(code_action_kind_for_type_error(&err).is_none());
    }

    #[test]
    fn use_after_move_emits_quickfix() {
        // MirError::UseAfterMove → LocalEdit → QUICKFIX.
        let err = MirError::UseAfterMove {
            local: 0,
            span: span(),
            suggestion: Some("change to `&s`"),
        };
        assert_eq!(
            code_action_kind_for_mir_error(&err),
            Some(CodeActionKind::QUICKFIX)
        );
    }

    #[test]
    fn dropped_feature_emits_no_action() {
        // LoweringError::DroppedFeature → RequiresHumanReview → no action.
        let err = LoweringError::DroppedFeature {
            name: "is",
            span: span(),
            suggestion: Some("use `==` instead"),
        };
        assert!(code_action_kind_for_lowering_error(&err).is_none());
    }

    #[test]
    fn fix_safety_from_code_roundtrip() {
        assert_eq!(fix_safety_from_code(0), FixSafety::FormatOnly);
        assert_eq!(fix_safety_from_code(1), FixSafety::BehaviorPreserving);
        assert_eq!(fix_safety_from_code(2), FixSafety::LocalEdit);
        assert_eq!(fix_safety_from_code(3), FixSafety::ApiChanging);
        assert_eq!(fix_safety_from_code(4), FixSafety::TargetChanging);
        assert_eq!(fix_safety_from_code(5), FixSafety::RequiresHumanReview);
        // Out-of-range defaults to RequiresHumanReview (conservative).
        assert_eq!(fix_safety_from_code(99), FixSafety::RequiresHumanReview);
    }
}
