//! Lowering-time error variants.
//!
//! Every variant is span-bearing. The lowering surfaces these as a
//! `Result<Module, LoweringError>` so that `cobrust-types` and
//! downstream tools never have to defensively handle panics.
//!
//! The full taxonomy is fixed by ADR-0005 §"Error taxonomy".

use thiserror::Error;

use cobrust_frontend::span::Span;

/// Categorised lowering failure. Pinned by `adr:0005`.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum LoweringError {
    /// A `name_expr` (form 23) that does not resolve in any
    /// enclosing scope.
    #[error("unknown name `{name}` at {span}")]
    UnknownName { name: String, span: Span },

    /// A constitution-dropped form snuck past the parser. Defense
    /// in depth (`CLAUDE.md` §2.2).
    #[error("the form `{name}` is not part of Cobrust (dropped feature) at {span}")]
    DroppedFeature { name: &'static str, span: Span },

    /// A parameter default whose value isn't a literal expression.
    /// The parser already flags this syntactically; we re-flag
    /// semantically in case future AST versions widen the allowed
    /// shapes.
    #[error("parameter default must be a literal expression at {span}")]
    MutableDefault { span: Span },

    /// An or-pattern whose branches bind a different set of names.
    #[error("or-pattern branches must bind the same set of names at {span}")]
    OrPatternBindingMismatch { span: Span },

    /// Two bindings with the same name in the same scope (e.g.
    /// duplicate parameter, duplicate let in the same block before
    /// any read).
    #[error("duplicate binding `{name}` (first at {first}, second at {second})")]
    DuplicateBinding {
        name: String,
        first: Span,
        second: Span,
    },

    /// Assignment whose target name is not in scope.
    #[error("assignment to unknown binding `{name}` at {span}")]
    AssignToUnknown { name: String, span: Span },
}
