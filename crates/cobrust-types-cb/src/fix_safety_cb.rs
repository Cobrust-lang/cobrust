//! cb mirror of `cobrust-types::fix_safety` — ADR-0062 §3 + Phase H
//! parity contract per ADR-0055b §4.
//!
//! Mirrors `FixSafety` + `Suggestion` 1:1 so Phase H byte-parity tests
//! pass. `SuggestionCb` carries an owned `String` for the suggestion
//! `message` + `replacement` (vs. Rust `&'static str` / `Option<String>`)
//! per ADR-0055b §6 risk 1 (cb owns its strings).
//!
//! ## Surface invariants (ADR-0055b §4 + ADR-0062 §3.5)
//!
//! - `FixSafetyCb` mirrors `FixSafety` 1:1: 6 variants, identical names,
//!   identical declaration order so `derive(Ord)` agrees.
//! - `SuggestionCb { message: String, fix_safety: FixSafetyCb,
//!   replacement: Option<String> }` mirrors `Suggestion`.
//! - `type_error_cb_fix_safety` mirrors `cobrust_types::type_error_fix_safety`
//!   variant-by-variant — every `TypeErrorCb` variant returns the same tier
//!   as its Rust counterpart returns under `type_error_fix_safety`.
//! - Kebab-case `Display` wire form is byte-equal to Rust per ADR-0062 §1.2.
//!
//! ## Anchor symbols (F34)
//!
//! - `fix_safety_cb.rs::FixSafetyCb` — the 6-variant mirror enum
//! - `fix_safety_cb.rs::SuggestionCb` — the owned-string Suggestion mirror
//! - `fix_safety_cb.rs::type_error_cb_fix_safety` — per-variant tier lookup

use std::fmt;

use crate::error_cb::TypeErrorCb;

/// cb mirror of `cobrust_types::FixSafety`.
///
/// Variant declaration order is identical so the derived `Ord`
/// reflects the same tier ladder. ADR-0062 §3.1. The default is
/// `RequiresHumanReview` (conservative; declared last so the
/// `derive(Default)` lands on it via `#[default]`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FixSafetyCb {
    /// Whitespace / formatting only.
    FormatOnly,
    /// Semantically equivalent rewrite within the function body.
    BehaviorPreserving,
    /// Confined to a single call-site or binding.
    LocalEdit,
    /// Changes the public API of a function or type.
    ApiChanging,
    /// Changes target platform, ABI, or linking contract.
    TargetChanging,
    /// Semantic ambiguity beyond compiler's ability to assess.
    #[default]
    RequiresHumanReview,
}

impl fmt::Display for FixSafetyCb {
    /// Kebab-case wire form per ADR-0062 §1.2 — byte-equal to Rust
    /// `FixSafety` Display per Phase H parity contract.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            FixSafetyCb::FormatOnly => "format-only",
            FixSafetyCb::BehaviorPreserving => "behavior-preserving",
            FixSafetyCb::LocalEdit => "local-edit",
            FixSafetyCb::ApiChanging => "api-changing",
            FixSafetyCb::TargetChanging => "target-changing",
            FixSafetyCb::RequiresHumanReview => "requires-human-review",
        };
        f.write_str(s)
    }
}

impl FixSafetyCb {
    /// Decode a tier code (the opaque `u8` returned by
    /// `cobrust_mir::mir_error_fix_safety_code` /
    /// `cobrust_hir::lowering_error_fix_safety_code`) into a
    /// `FixSafetyCb`.
    ///
    /// Out-of-range codes default to `RequiresHumanReview` per the
    /// conservative-default rule (ADR-0062 §3.1 §7 risk row 1).
    #[must_use]
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => FixSafetyCb::FormatOnly,
            1 => FixSafetyCb::BehaviorPreserving,
            2 => FixSafetyCb::LocalEdit,
            3 => FixSafetyCb::ApiChanging,
            4 => FixSafetyCb::TargetChanging,
            _ => FixSafetyCb::RequiresHumanReview,
        }
    }

    /// Encode `self` as the opaque tier `u8` (0..5).
    #[must_use]
    pub fn as_code(self) -> u8 {
        match self {
            FixSafetyCb::FormatOnly => 0,
            FixSafetyCb::BehaviorPreserving => 1,
            FixSafetyCb::LocalEdit => 2,
            FixSafetyCb::ApiChanging => 3,
            FixSafetyCb::TargetChanging => 4,
            FixSafetyCb::RequiresHumanReview => 5,
        }
    }
}

/// cb mirror of `cobrust_types::Suggestion` with owned strings per
/// ADR-0055b §6 risk 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionCb {
    /// Human-readable fix prose (owned).
    pub message: String,
    /// Safety tier per ADR-0062 §3.1.
    pub fix_safety: FixSafetyCb,
    /// Machine-applicable text replacement at the diagnostic span.
    pub replacement: Option<String>,
}

impl SuggestionCb {
    #[must_use]
    pub fn new(message: impl Into<String>, fix_safety: FixSafetyCb) -> Self {
        Self {
            message: message.into(),
            fix_safety,
            replacement: None,
        }
    }

    #[must_use]
    pub fn with_replacement(
        message: impl Into<String>,
        fix_safety: FixSafetyCb,
        replacement: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            fix_safety,
            replacement: Some(replacement.into()),
        }
    }
}

/// Look up the fix-safety tier for a `TypeErrorCb` variant.
///
/// Phase H byte-parity contract per ADR-0055b §4 + ADR-0062 §3.3 — the
/// per-variant mapping is identical to the Rust-side
/// `cobrust_types::type_error_fix_safety` (each variant in Rust maps to
/// its same-named cb mirror under this function).
#[must_use]
#[allow(clippy::enum_glob_use)]
pub fn type_error_cb_fix_safety(err: &TypeErrorCb) -> FixSafetyCb {
    use TypeErrorCb::*;
    match err {
        UnknownName { .. }
        | ArityMismatch { .. }
        | MissingArgument { .. }
        | KeywordArgMismatch { .. }
        | TypeMismatch { .. }
        | NonExhaustiveMatch { .. }
        | AmbiguousType { .. }
        | DuplicateField { .. }
        | NotCallable { .. }
        | NotIndexable { .. }
        | NotIterable { .. }
        | BreakOutsideLoop { .. }
        | ContinueOutsideLoop { .. }
        | ReturnOutsideFn { .. }
        | YieldOutsideFn { .. }
        | BorrowOfNonPlace { .. }
        | UnknownMethod { .. } => FixSafetyCb::LocalEdit,
        ImplicitTruthiness { .. } | MutableDefault { .. } | NotHashable { .. } => {
            FixSafetyCb::BehaviorPreserving
        }
        RowConflict { .. }
        | UseOfDroppedFeature { .. }
        | OccursCheck { .. }
        | DictSpreadNotSupported { .. }
        | Multiple(_) => FixSafetyCb::RequiresHumanReview,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fix_safety_cb_display_byte_parity() {
        // ADR-0062 §3.5 wire-form parity with Rust.
        assert_eq!(FixSafetyCb::FormatOnly.to_string(), "format-only");
        assert_eq!(
            FixSafetyCb::BehaviorPreserving.to_string(),
            "behavior-preserving"
        );
        assert_eq!(FixSafetyCb::LocalEdit.to_string(), "local-edit");
        assert_eq!(FixSafetyCb::ApiChanging.to_string(), "api-changing");
        assert_eq!(FixSafetyCb::TargetChanging.to_string(), "target-changing");
        assert_eq!(
            FixSafetyCb::RequiresHumanReview.to_string(),
            "requires-human-review"
        );
    }

    #[test]
    fn fix_safety_cb_ord_lowest_to_highest() {
        assert!(FixSafetyCb::FormatOnly < FixSafetyCb::RequiresHumanReview);
        assert!(FixSafetyCb::BehaviorPreserving < FixSafetyCb::LocalEdit);
    }

    #[test]
    fn fix_safety_cb_default_is_requires_human_review() {
        assert_eq!(FixSafetyCb::default(), FixSafetyCb::RequiresHumanReview);
    }

    #[test]
    fn fix_safety_cb_code_roundtrip() {
        for variant in [
            FixSafetyCb::FormatOnly,
            FixSafetyCb::BehaviorPreserving,
            FixSafetyCb::LocalEdit,
            FixSafetyCb::ApiChanging,
            FixSafetyCb::TargetChanging,
            FixSafetyCb::RequiresHumanReview,
        ] {
            assert_eq!(FixSafetyCb::from_code(variant.as_code()), variant);
        }
        // Out-of-range defaults to RequiresHumanReview.
        assert_eq!(
            FixSafetyCb::from_code(99),
            FixSafetyCb::RequiresHumanReview
        );
    }
}
