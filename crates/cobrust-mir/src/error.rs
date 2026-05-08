//! MIR error taxonomy — ADR-0020 §"Public surface".
//!
//! Every `MirError` is reportable at the user-source span; the
//! lowering / borrow / drop passes never panic on input the type
//! checker accepted (constitution §6: closed-loop validation).

use cobrust_frontend::span::Span;
use thiserror::Error;

use crate::tree::{LocalId, PlaceDebug};

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum MirError {
    /// B1 — read of a local already moved.
    #[error("use of moved value `_{local}`")]
    UseAfterMove { local: u32, span: Span },

    /// B4 — read of a local already dropped.
    #[error("use of dropped value `_{local}`")]
    UseAfterDrop { local: u32, span: Span },

    /// B2 — second mutable borrow while a first is still live.
    #[error("conflicting mutable borrow of `_{local}`")]
    ConflictingMutBorrow { local: u32, span: Span },

    /// B3 — shared and mutable borrow on the same root local overlap.
    #[error("shared and mutable borrow of `_{local}` overlap")]
    SharedMutOverlap { local: u32, span: Span },

    /// B5 — reference outlives the local it references.
    #[error("borrow of `_{local}` escapes its scope")]
    EscapingBorrow { local: u32, span: Span },

    /// Drop-schedule invariant — owning local reaches `Return`
    /// without being dropped.
    #[error("missing drop for owning local `_{local}` on return path")]
    DropMissing { local: u32, span: Span },

    /// Drop-schedule invariant — same local dropped twice on a path.
    #[error("double-drop of `_{local}`")]
    DoubleDrop { local: u32, span: Span },

    /// Lowering error — projection out-of-bounds for the local's type.
    #[error("field projection out of bounds at {place:?}")]
    FieldOutOfBounds { place: PlaceDebug, span: Span },

    /// Lowering error — referenced `DefId` had no recorded type
    /// (defense-in-depth; the type checker should have caught).
    #[error("unresolved DefId {def_id} at lowering time")]
    UnresolvedDefId { def_id: u32, span: Span },

    /// Lowering error — switch terminator emitted with neither a
    /// matching case nor an `otherwise` block.
    #[error("non-exhaustive switch")]
    NonExhaustiveSwitch { span: Span },

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
#[must_use]
pub fn use_after_move(local: LocalId, span: Span) -> MirError {
    MirError::UseAfterMove {
        local: local.0,
        span,
    }
}
