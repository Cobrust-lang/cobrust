//! MIR error taxonomy — ADR-0020 §"Public surface".
//!
//! Every `MirError` is reportable at the user-source span; the
//! lowering / borrow / drop passes never panic on input the type
//! checker accepted (constitution §6: closed-loop validation).
//!
//! ADR-0052b §2 + §6 — every variant carries a uniform
//! `suggestion: Option<&'static str>` field populated at construction
//! time per CLAUDE.md §2.5 Direction B (LLM-first error UX).

use cobrust_frontend::span::Span;
use thiserror::Error;

use crate::tree::{LocalId, PlaceDebug};

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum MirError {
    /// B1 — read of a local already moved.
    #[error("use of moved value `_{local}`")]
    UseAfterMove {
        local: u32,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// B4 — read of a local already dropped.
    #[error("use of dropped value `_{local}`")]
    UseAfterDrop {
        local: u32,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// B2 — second mutable borrow while a first is still live.
    #[error("conflicting mutable borrow of `_{local}`")]
    ConflictingMutBorrow {
        local: u32,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// B3 — shared and mutable borrow on the same root local overlap.
    #[error("shared and mutable borrow of `_{local}` overlap")]
    SharedMutOverlap {
        local: u32,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// B5 — reference outlives the local it references.
    #[error("borrow of `_{local}` escapes its scope")]
    EscapingBorrow {
        local: u32,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Drop-schedule invariant — owning local reaches `Return`
    /// without being dropped.
    #[error("missing drop for owning local `_{local}` on return path")]
    DropMissing {
        local: u32,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Drop-schedule invariant — same local dropped twice on a path.
    #[error("double-drop of `_{local}`")]
    DoubleDrop {
        local: u32,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Lowering error — projection out-of-bounds for the local's type.
    #[error("field projection out of bounds at {place:?}")]
    FieldOutOfBounds {
        place: PlaceDebug,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Lowering error — referenced `DefId` had no recorded type
    /// (defense-in-depth; the type checker should have caught).
    #[error("unresolved DefId {def_id} at lowering time")]
    UnresolvedDefId {
        def_id: u32,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Lowering error — switch terminator emitted with neither a
    /// matching case nor an `otherwise` block.
    #[error("non-exhaustive switch")]
    NonExhaustiveSwitch {
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Catch-all for invariants the lowering should never violate.
    /// Hitting this is a bug in the MIR builder.
    #[error("internal MIR error: {0}")]
    Internal(String),
}

impl MirError {
    /// Discriminant string for property-test classification.
    #[must_use]
    pub fn category(&self) -> &'static str {
        match self {
            Self::UseAfterMove { .. } => "use-after-move",
            Self::UseAfterDrop { .. } => "use-after-drop",
            Self::ConflictingMutBorrow { .. } => "conflicting-mut-borrow",
            Self::SharedMutOverlap { .. } => "shared-mut-overlap",
            Self::EscapingBorrow { .. } => "escaping-borrow",
            Self::DropMissing { .. } => "drop-missing",
            Self::DoubleDrop { .. } => "double-drop",
            Self::FieldOutOfBounds { .. } => "field-out-of-bounds",
            Self::UnresolvedDefId { .. } => "unresolved-defid",
            Self::NonExhaustiveSwitch { .. } => "non-exhaustive-switch",
            Self::Internal(_) => "internal",
        }
    }
}

/// Helper for `MirError::UseAfterMove` style errors at a `LocalId`.
///
/// ADR-0052b §6 — the helper threads the canonical UseAfterMove
/// suggestion (ADR-0052a §7 explicit shared borrow `&s`) so callers
/// inherit the fix path without re-specifying it.
#[must_use]
pub fn use_after_move(local: LocalId, span: Span) -> MirError {
    MirError::UseAfterMove {
        local: local.0,
        span,
        suggestion: Some(
            "change to `&s` to borrow without consuming (ADR-0052a explicit shared borrow)",
        ),
    }
}

/// Extract the construction-time `suggestion: Option<&'static str>`
/// payload from a `MirError`. ADR-0062 §3.4 mirror of
/// `type_error_suggestion_text` for the MIR taxonomy.
#[must_use]
pub fn mir_error_suggestion_text(err: &MirError) -> Option<&'static str> {
    use MirError::*;
    match err {
        UseAfterMove { suggestion, .. }
        | UseAfterDrop { suggestion, .. }
        | ConflictingMutBorrow { suggestion, .. }
        | SharedMutOverlap { suggestion, .. }
        | EscapingBorrow { suggestion, .. }
        | DropMissing { suggestion, .. }
        | DoubleDrop { suggestion, .. }
        | FieldOutOfBounds { suggestion, .. }
        | UnresolvedDefId { suggestion, .. }
        | NonExhaustiveSwitch { suggestion, .. } => *suggestion,
        Internal(_) => None,
    }
}

/// Look up the fix-safety tier code for a `MirError` variant per
/// ADR-0062 §3.4 mapping.
///
/// Returns an opaque `u8` (FormatOnly=0 .. RequiresHumanReview=5) so
/// `cobrust-mir` does not depend on `cobrust-types`. The LSP adapter
/// (`crates/cobrust-lsp/src/diagnostic.rs`) widens this byte into the
/// `FixSafety` enum at the consumer boundary.
///
/// Conservative tagging: borrow / drop fixes that reshape lifetime
/// graphs default to `RequiresHumanReview` (5); trivially-local
/// substitutions (e.g. `&s`-borrow swap) are `LocalEdit` (2).
#[must_use]
pub fn mir_error_fix_safety_code(err: &MirError) -> u8 {
    use MirError::*;
    match err {
        // `&s` borrow substitution — confined to one expression site.
        UseAfterMove { .. } => 2,
        // Reorder reads before drops — local but multi-statement.
        UseAfterDrop { .. } => 2,
        // Release the first borrow — usually a scope rewrite.
        ConflictingMutBorrow { .. } => 2,
        // Release shared borrow before taking mutable.
        SharedMutOverlap { .. } => 2,
        // Borrow outlives owner — lifetime restructuring.
        EscapingBorrow { .. } => 5,
        // Add `drop(<local>)` or transfer ownership.
        DropMissing { .. } => 2,
        // Control-flow surgery to reach a single drop.
        DoubleDrop { .. } => 5,
        // Compiler-internal (defense-in-depth — should never user-facing).
        FieldOutOfBounds { .. } | UnresolvedDefId { .. } | Internal(_) => 5,
        // Add wildcard / cover all cases.
        NonExhaustiveSwitch { .. } => 2,
    }
}
