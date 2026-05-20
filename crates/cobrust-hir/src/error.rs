//! Lowering-time error variants.
//!
//! Every variant is span-bearing. The lowering surfaces these as a
//! `Result<Module, LoweringError>` so that `cobrust-types` and
//! downstream tools never have to defensively handle panics.
//!
//! The full taxonomy is fixed by ADR-0005 §"Error taxonomy".
//!
//! ADR-0052b §2 — every variant carries a uniform
//! `suggestion: Option<&'static str>` field populated at construction
//! time per CLAUDE.md §2.5 Direction B (LLM-first error UX). The
//! scope expansion to `LoweringError` (beyond ADR-0052b's literal
//! `TypeError + MirError` text) was forced by the Wave-2 corpus
//! `s0052b_01/16/20/27/28/29` which trigger HIR-lower as the actual
//! catch surface for `UnknownName` / `DroppedFeature`. The Direction B
//! pattern naturally extends across all user-visible error types per
//! §2.5 LLM-first rule.

use thiserror::Error;

use cobrust_frontend::span::Span;

/// Categorised lowering failure. Pinned by `adr:0005`.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum LoweringError {
    /// A `name_expr` (form 23) that does not resolve in any
    /// enclosing scope.
    #[error("unknown name `{name}` at {span}")]
    UnknownName {
        name: String,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// A constitution-dropped form snuck past the parser. Defense
    /// in depth (`CLAUDE.md` §2.2).
    #[error("the form `{name}` is not part of Cobrust (dropped feature) at {span}")]
    DroppedFeature {
        name: &'static str,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// A parameter default whose value isn't a literal expression.
    /// The parser already flags this syntactically; we re-flag
    /// semantically in case future AST versions widen the allowed
    /// shapes.
    #[error("parameter default must be a literal expression at {span}")]
    MutableDefault {
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// An or-pattern whose branches bind a different set of names.
    #[error("or-pattern branches must bind the same set of names at {span}")]
    OrPatternBindingMismatch {
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Two bindings with the same name in the same scope (e.g.
    /// duplicate parameter, duplicate let in the same block before
    /// any read).
    #[error("duplicate binding `{name}` (first at {first}, second at {second})")]
    DuplicateBinding {
        name: String,
        first: Span,
        second: Span,
        suggestion: Option<&'static str>,
    },

    /// Assignment whose target name is not in scope.
    #[error("assignment to unknown binding `{name}` at {span}")]
    AssignToUnknown {
        name: String,
        span: Span,
        suggestion: Option<&'static str>,
    },
}

/// Extract the construction-time `suggestion: Option<&'static str>`
/// payload from a `LoweringError`. ADR-0062 §3.4 mirror of
/// `type_error_suggestion_text` for the HIR-lower taxonomy.
#[must_use]
pub fn lowering_error_suggestion_text(err: &LoweringError) -> Option<&'static str> {
    use LoweringError::*;
    match err {
        UnknownName { suggestion, .. }
        | DroppedFeature { suggestion, .. }
        | MutableDefault { suggestion, .. }
        | OrPatternBindingMismatch { suggestion, .. }
        | DuplicateBinding { suggestion, .. }
        | AssignToUnknown { suggestion, .. } => *suggestion,
    }
}

/// Look up the fix-safety tier code for a `LoweringError` variant per
/// ADR-0062 §3.4 mapping. Returns an opaque `u8` (FormatOnly=0 ..
/// RequiresHumanReview=5) so `cobrust-hir` does not depend on
/// `cobrust-types`. The LSP adapter widens the byte into `FixSafety`
/// at the consumer boundary.
///
/// ADR-0062 §3.4: lowering errors are mostly `LocalEdit` (typo /
/// declaration fix) or `RequiresHumanReview` (dropped feature, or-pattern
/// shape mismatch).
#[must_use]
pub fn lowering_error_fix_safety_code(err: &LoweringError) -> u8 {
    use LoweringError::*;
    match err {
        // Typo / declaration fix — call-site only.
        UnknownName { .. } => 2,
        // Dropped feature — must rewrite to a supported construct.
        DroppedFeature { .. } => 5,
        // `None`-default rewrite is compiler-mandated (semantically equivalent).
        MutableDefault { .. } => 1,
        // Or-pattern binding shape — usually requires re-design.
        OrPatternBindingMismatch { .. } => 5,
        // Rename one of the duplicate bindings — local edit.
        DuplicateBinding { .. } => 2,
        // Add `let <name> = …` declaration first — local edit.
        AssignToUnknown { .. } => 2,
    }
}
