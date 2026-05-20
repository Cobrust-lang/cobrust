//! ADR-0062 §3 — Fix-safety ladder.
//!
//! Six-tier safety ladder consumed by LSP code-actions (ADR-0057a)
//! and JSON diagnostic output. The tier is a machine-routable signal
//! that LLM agents read to decide whether a suggested fix is safe to
//! auto-apply.
//!
//! `FixSafety` is the enum; `Suggestion` is the structured carrier
//! `(message, fix_safety, replacement)` introduced by ADR-0062 §3.2.
//! The existing `Option<&'static str>` field on each `TypeError +
//! MirError + LoweringError` variant stays in place (the suggestion
//! TEXT lives at construction time per ADR-0052b §2). `fix_safety_of`
//! provides the per-variant tier lookup that LSP + JSON consume.
//!
//! Variant declaration order IS the tier order: `FormatOnly` < ... <
//! `RequiresHumanReview` (asserted by `test_fix_safety_format_only_is_lowest_tier`).
//!
//! Wire form is kebab-case per Zero-language precedent (ADR-0062 §1.2):
//! `format-only` / `behavior-preserving` / `local-edit` / `api-changing`
//! / `target-changing` / `requires-human-review`.

use std::fmt;

use crate::TypeError;

/// Safety tier for a suggested fix, from the compiler's perspective.
///
/// Variants are declared lowest-tier first so the derived `Ord` /
/// `PartialOrd` impl reflects the safety order. ADR-0062 §3.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FixSafety {
    /// Whitespace / formatting only. Never changes semantics.
    /// Auto-apply unconditionally.
    FormatOnly,
    /// Semantically equivalent rewrite within the function body.
    /// Auto-apply if no downstream tests fail.
    BehaviorPreserving,
    /// Changes confined to a single call-site or binding.
    /// Auto-apply with caution; may require adjacent test update.
    LocalEdit,
    /// Changes the public API of a function or type.
    /// Auto-apply only with explicit user confirmation.
    ApiChanging,
    /// Changes target platform, ABI, or linking contract.
    /// Never auto-apply.
    TargetChanging,
    /// Semantic ambiguity or migration risk beyond the compiler's
    /// ability to assess. Always requires human review before apply.
    RequiresHumanReview,
}

impl Default for FixSafety {
    /// Conservative default: assume human review required.
    /// ADR-0062 §3.1 + §7 risk-register row 1.
    fn default() -> Self {
        FixSafety::RequiresHumanReview
    }
}

impl fmt::Display for FixSafety {
    /// Kebab-case wire form per ADR-0062 §1.2 Zero precedent.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            FixSafety::FormatOnly => "format-only",
            FixSafety::BehaviorPreserving => "behavior-preserving",
            FixSafety::LocalEdit => "local-edit",
            FixSafety::ApiChanging => "api-changing",
            FixSafety::TargetChanging => "target-changing",
            FixSafety::RequiresHumanReview => "requires-human-review",
        };
        f.write_str(s)
    }
}

/// Structured suggestion carrier per ADR-0062 §3.2.
///
/// Pairs the human-readable `message` (Wave-2 `&'static str` per
/// ADR-0052b) with the machine-routable `fix_safety` tier and an
/// optional `replacement` payload that the LSP code-action can apply
/// directly. When `replacement` is `None` the fix is descriptive only;
/// the agent must reason about the substitution itself.
///
/// This struct is the forward-shape for diagnostic JSON emit and LSP
/// `related_information`. Wave-2 variant payload stays
/// `Option<&'static str>` for construction-site stability per
/// ADR-0052b §2; consumers obtain the structured `Suggestion` via
/// `Suggestion::for_type_error` (and the parallel helpers below) which
/// looks up the per-variant `FixSafety` tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Human-readable fix prose. Mirror of ADR-0052b construction-time
    /// `&'static str`; owned `String` here so the carrier can be sent
    /// across `serde` / LSP boundaries without lifetime constraints.
    pub message: String,
    /// Safety tier per ADR-0062 §3.1.
    pub fix_safety: FixSafety,
    /// Machine-applicable text replacement at the diagnostic span.
    /// `None` means the fix requires agent reasoning. ADR-0062 §3.2 +
    /// §7 risk row 4 (partial coverage acceptable).
    pub replacement: Option<String>,
}

impl Suggestion {
    /// Construct a `Suggestion` from a `(message, fix_safety)` pair
    /// with no replacement text. Common case for ADR-0062 wave-1.
    #[must_use]
    pub fn new(message: impl Into<String>, fix_safety: FixSafety) -> Self {
        Self {
            message: message.into(),
            fix_safety,
            replacement: None,
        }
    }

    /// Construct a `Suggestion` with a machine-applicable replacement.
    #[must_use]
    pub fn with_replacement(
        message: impl Into<String>,
        fix_safety: FixSafety,
        replacement: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            fix_safety,
            replacement: Some(replacement.into()),
        }
    }

    /// Build a structured `Suggestion` for a `TypeError` variant by
    /// looking up the variant's per-tier classification + carrying its
    /// construction-time `suggestion: Option<&'static str>` message.
    ///
    /// Returns `None` when the variant has no suggestion populated
    /// (variant-level `None` or compiler-internal variants like
    /// `TypeError::Multiple`).
    #[must_use]
    pub fn for_type_error(err: &TypeError) -> Option<Self> {
        let msg = type_error_suggestion_text(err)?;
        Some(Self::new(msg, type_error_fix_safety(err)))
    }
}

/// Extract the construction-time `suggestion: Option<&'static str>`
/// payload from a `TypeError`. Mirrors the LSP `with_suggestion`
/// dispatcher (`crates/cobrust-lsp/src/diagnostic.rs::type_error_to_diagnostic_single`).
#[must_use]
pub fn type_error_suggestion_text(err: &TypeError) -> Option<&'static str> {
    use TypeError::*;
    match err {
        UnknownName { suggestion, .. }
        | ArityMismatch { suggestion, .. }
        | KeywordArgMismatch { suggestion, .. }
        | MissingArgument { suggestion, .. }
        | TypeMismatch { suggestion, .. }
        | NonExhaustiveMatch { suggestion, .. }
        | RowConflict { suggestion, .. }
        | ImplicitTruthiness { suggestion, .. }
        | UseOfDroppedFeature { suggestion, .. }
        | MutableDefault { suggestion, .. }
        | AmbiguousType { suggestion, .. }
        | DuplicateField { suggestion, .. }
        | OccursCheck { suggestion, .. }
        | NotCallable { suggestion, .. }
        | NotIndexable { suggestion, .. }
        | NotIterable { suggestion, .. }
        | BreakOutsideLoop { suggestion, .. }
        | ContinueOutsideLoop { suggestion, .. }
        | ReturnOutsideFn { suggestion, .. }
        | YieldOutsideFn { suggestion, .. }
        | NotHashable { suggestion, .. }
        | DictSpreadNotSupported { suggestion, .. }
        | BorrowOfNonPlace { suggestion, .. }
        | UnknownMethod { suggestion, .. } => *suggestion,
        Multiple(_) => None,
    }
}

/// Look up the `FixSafety` tier for a `TypeError` variant per
/// ADR-0062 §3.3 mapping table.
///
/// Conservative defaults: when in doubt, prefer the safer (higher)
/// tier — `RequiresHumanReview` over `LocalEdit`, never the inverse.
#[must_use]
pub fn type_error_fix_safety(err: &TypeError) -> FixSafety {
    use TypeError::*;
    match err {
        // Typo / declaration fixes — confined to one line.
        UnknownName { .. } => FixSafety::LocalEdit,
        // Arity changes — call-site only (callee signature unchanged).
        ArityMismatch { .. } | MissingArgument { .. } => FixSafety::LocalEdit,
        // Keyword rename — call-site only.
        KeywordArgMismatch { .. } => FixSafety::LocalEdit,
        // Type rewrite — confined to expression / annotation.
        TypeMismatch { .. } => FixSafety::LocalEdit,
        // Match exhaustiveness — adding wildcard preserves semantics
        // for the previously-covered paths; the wildcard arm body is
        // user-supplied so behavior of the new path is undetermined.
        NonExhaustiveMatch { .. } => FixSafety::LocalEdit,
        // Record row conflict — structural, requires human design intent.
        RowConflict { .. } => FixSafety::RequiresHumanReview,
        // §2.5 canonical: `if x:` → `if x != 0:` preserves semantics
        // for all `actual` types under Cobrust's explicit-truthiness rule.
        ImplicitTruthiness { .. } => FixSafety::BehaviorPreserving,
        // Dropped feature — fix is "use a different construct entirely".
        UseOfDroppedFeature { .. } => FixSafety::RequiresHumanReview,
        // Mutable default → `None`-default + body assign: compiler-mandated;
        // Python semantics never matched user intent.
        MutableDefault { .. } => FixSafety::BehaviorPreserving,
        // Type annotation add — local binding only.
        AmbiguousType { .. } => FixSafety::LocalEdit,
        // Duplicate field — remove the duplicate.
        DuplicateField { .. } => FixSafety::LocalEdit,
        // Occurs check — impossible recursive type; requires structural change.
        OccursCheck { .. } => FixSafety::RequiresHumanReview,
        // Wrong receiver — local rewrite of the call shape.
        NotCallable { .. } | NotIndexable { .. } | NotIterable { .. } => FixSafety::LocalEdit,
        // Control-flow placement — move the statement into a containing block.
        BreakOutsideLoop { .. }
        | ContinueOutsideLoop { .. }
        | ReturnOutsideFn { .. }
        | YieldOutsideFn { .. } => FixSafety::LocalEdit,
        // Hashable key — `f.to_bits() as i64` substitution.
        NotHashable { .. } => FixSafety::BehaviorPreserving,
        // Phase-G feature gap — fix is "wait for Phase G or rewrite manually".
        DictSpreadNotSupported { .. } => FixSafety::RequiresHumanReview,
        // Borrow of non-place — restructure to bind first then borrow.
        BorrowOfNonPlace { .. } => FixSafety::LocalEdit,
        // Method name typo — call-site only.
        UnknownMethod { .. } => FixSafety::LocalEdit,
        // Multiple — children carry their own tiers; the aggregate has no
        // singular safety classification.
        Multiple(_) => FixSafety::RequiresHumanReview,
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
    fn test_fix_safety_default_is_requires_human_review() {
        // ADR-0062 §6 acceptance gate row 1.
        assert_eq!(FixSafety::default(), FixSafety::RequiresHumanReview);
    }

    #[test]
    fn test_fix_safety_format_only_is_lowest_tier() {
        // ADR-0062 §6 acceptance gate row 2 — declaration order = tier order.
        assert!(FixSafety::FormatOnly < FixSafety::BehaviorPreserving);
        assert!(FixSafety::BehaviorPreserving < FixSafety::LocalEdit);
        assert!(FixSafety::LocalEdit < FixSafety::ApiChanging);
        assert!(FixSafety::ApiChanging < FixSafety::TargetChanging);
        assert!(FixSafety::TargetChanging < FixSafety::RequiresHumanReview);
    }

    #[test]
    fn test_fix_safety_requires_human_review_is_highest_tier() {
        // ADR-0062 §6 acceptance gate row 3.
        assert!(FixSafety::RequiresHumanReview > FixSafety::TargetChanging);
        assert!(FixSafety::RequiresHumanReview > FixSafety::FormatOnly);
    }

    #[test]
    fn test_fix_safety_display_kebab_case() {
        // ADR-0062 §6 acceptance gate row 4 (wire form).
        assert_eq!(FixSafety::FormatOnly.to_string(), "format-only");
        assert_eq!(
            FixSafety::BehaviorPreserving.to_string(),
            "behavior-preserving"
        );
        assert_eq!(FixSafety::LocalEdit.to_string(), "local-edit");
        assert_eq!(FixSafety::ApiChanging.to_string(), "api-changing");
        assert_eq!(FixSafety::TargetChanging.to_string(), "target-changing");
        assert_eq!(
            FixSafety::RequiresHumanReview.to_string(),
            "requires-human-review"
        );
    }

    #[test]
    fn test_suggestion_has_fix_safety() {
        // ADR-0062 §6 acceptance gate row 5.
        let s = Suggestion::new("change to `if x != 0:`", FixSafety::BehaviorPreserving);
        assert_eq!(s.fix_safety, FixSafety::BehaviorPreserving);
        assert_eq!(s.message, "change to `if x != 0:`");
        assert!(s.replacement.is_none());
    }

    #[test]
    fn test_suggestion_with_replacement_carries_substitution() {
        let s = Suggestion::with_replacement(
            "change to `if x != 0:`",
            FixSafety::BehaviorPreserving,
            "if x != 0:",
        );
        assert_eq!(s.replacement.as_deref(), Some("if x != 0:"));
    }

    #[test]
    fn test_type_error_implicit_truthiness_is_behavior_preserving() {
        // ADR-0062 §3.3 canonical row — §2.5 LLM-first justification.
        let err = TypeError::ImplicitTruthiness {
            actual: crate::ty::Ty::Int,
            span: span(),
            suggestion: Some("change to `if x != 0:` (use `.is_some()` for Option)"),
        };
        assert_eq!(type_error_fix_safety(&err), FixSafety::BehaviorPreserving);
        let s = Suggestion::for_type_error(&err).expect("suggestion populated");
        assert_eq!(s.fix_safety, FixSafety::BehaviorPreserving);
        assert!(s.message.contains("if x != 0"));
    }

    #[test]
    fn test_type_error_occurs_check_is_requires_human_review() {
        // ADR-0062 §3.3 — recursive-type impossibility = human review.
        let err = TypeError::OccursCheck {
            var: crate::ty::VarId(0),
            ty: crate::ty::Ty::Int,
            span: span(),
            suggestion: Some("add a type annotation — recursive types must be explicit"),
        };
        assert_eq!(type_error_fix_safety(&err), FixSafety::RequiresHumanReview);
    }

    #[test]
    fn test_type_error_unknown_name_is_local_edit() {
        // ADR-0062 §3.3 — typo fix is call-site local.
        let err = TypeError::UnknownName {
            name: "x".into(),
            span: span(),
            suggestion: Some("declare with `let <name> = …` first"),
        };
        assert_eq!(type_error_fix_safety(&err), FixSafety::LocalEdit);
    }

    #[test]
    fn test_type_error_multiple_returns_no_suggestion() {
        // `Multiple` aggregates children; no singular suggestion text.
        let err = TypeError::Multiple(vec![]);
        assert!(type_error_suggestion_text(&err).is_none());
        assert!(Suggestion::for_type_error(&err).is_none());
        // Tier defaults to conservative RequiresHumanReview.
        assert_eq!(type_error_fix_safety(&err), FixSafety::RequiresHumanReview);
    }

    #[test]
    fn test_type_error_none_suggestion_returns_none() {
        // Variant with suggestion: None → no structured Suggestion built.
        let err = TypeError::ImplicitTruthiness {
            actual: crate::ty::Ty::Int,
            span: span(),
            suggestion: None,
        };
        assert!(Suggestion::for_type_error(&err).is_none());
    }
}
