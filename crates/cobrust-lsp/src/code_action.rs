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

use std::collections::HashMap;

use cobrust_hir::LoweringError;
use cobrust_mir::MirError;
use cobrust_types::{FixSafety, TypeError, type_error_fix_safety};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Diagnostic, TextEdit, Url, WorkspaceEdit,
};

use crate::diagnostic::DIAG_DATA_FIX_SAFETY_KEY;

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

/// Read the ADR-0062 FixSafety tier from a `Diagnostic.data` JSON
/// payload written by `diagnostic.rs::attach_fix_safety_data`.
///
/// Returns `None` when the diagnostic carries no `data`, or when the
/// payload's `fix_safety` key is missing / non-numeric / out-of-range.
/// Out-of-range values default to `RequiresHumanReview` via
/// [`fix_safety_from_code`]; this fn returns `None` in that case so
/// callers can distinguish "not annotated" from "explicitly conservative".
#[must_use]
pub fn fix_safety_from_diagnostic_data(diag: &Diagnostic) -> Option<FixSafety> {
    let data = diag.data.as_ref()?;
    let code = data.get(DIAG_DATA_FIX_SAFETY_KEY)?.as_u64()?;
    let code_u8: u8 = u8::try_from(code).ok()?;
    Some(fix_safety_from_code(code_u8))
}

/// ADR-0057e §3.2 — build `CodeAction[]` for a slice of diagnostics.
///
/// For each diagnostic in `diagnostics`:
///
/// - Read the FixSafety tier from `Diagnostic.data` (written by
///   `diagnostic.rs::attach_fix_safety_data`).
/// - Map tier → [`CodeActionKind`] via [`code_action_kind_for_fix_safety`].
/// - Read the suggestion text from `Diagnostic.relatedInformation[0].message`
///   (written by `diagnostic.rs::with_suggestion`).
/// - If the tier is `BehaviorPreserving` or `LocalEdit`, emit a
///   `CodeAction` with a `WorkspaceEdit` replacing the diagnostic's
///   range with the suggestion text.
/// - If the tier is `ApiChanging` or `FormatOnly`, emit a CodeAction
///   with title-only (no `edit` payload, suggestion-only).
/// - If the tier maps to `None` (`TargetChanging` / `RequiresHumanReview`),
///   skip — no CodeAction is emitted, the diagnostic stays
///   message-only.
///
/// Honest scope: the suggestion text *is* the replacement text. This
/// works for the §2.5-canonical `ImplicitTruthiness` case ("change to
/// `if x != 0:`") but is naive for hint-style suggestions where the
/// text is prose, not source. Future sub-ADRs may add per-variant
/// edit factories; wave-3 ships the conservative-default common path.
///
/// Returns an empty vec if no diagnostic produces a CodeAction.
#[must_use]
pub fn build_code_actions(diagnostics: &[Diagnostic], uri: &Url) -> Vec<CodeActionOrCommand> {
    let mut out: Vec<CodeActionOrCommand> = Vec::new();

    for diag in diagnostics {
        let Some(tier) = fix_safety_from_diagnostic_data(diag) else {
            continue;
        };
        let Some(kind) = code_action_kind_for_fix_safety(tier) else {
            continue; // TargetChanging / RequiresHumanReview → message-only
        };
        let Some(suggestion) = diag
            .related_information
            .as_ref()
            .and_then(|infos| infos.first())
            .map(|info| info.message.clone())
        else {
            continue; // no suggestion text → nothing to surface as title
        };

        let edit = match tier {
            // Auto-apply-eligible tiers attach a WorkspaceEdit with the
            // suggestion as replacement text.
            FixSafety::BehaviorPreserving | FixSafety::LocalEdit => {
                let text_edit = TextEdit {
                    range: diag.range,
                    new_text: suggestion.clone(),
                };
                let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                changes.insert(uri.clone(), vec![text_edit]);
                Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                })
            }
            // Suggest-only tiers: no edit payload, suggestion in title.
            FixSafety::ApiChanging | FixSafety::FormatOnly => None,
            // Filtered above; unreachable here.
            FixSafety::TargetChanging | FixSafety::RequiresHumanReview => continue,
        };

        let action = CodeAction {
            title: suggestion,
            kind: Some(kind),
            diagnostics: Some(vec![diag.clone()]),
            edit,
            command: None,
            is_preferred: None,
            disabled: None,
            data: None,
        };
        out.push(CodeActionOrCommand::CodeAction(action));
    }

    out
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
