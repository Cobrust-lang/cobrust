//! Type-checking error variants.
//!
//! Pinned by ADR-0006 §"Error taxonomy". Every variant is
//! span-bearing and printable; downstream tooling matches on the
//! variant.

use thiserror::Error;

use cobrust_frontend::span::Span;

use crate::ty::Ty;

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum TypeError {
    /// Defense-in-depth: lowering already catches most. The type
    /// checker may surface this for capture-only references during
    /// closure analysis.
    #[error("unknown name `{name}` at {span}")]
    UnknownName { name: String, span: Span },

    /// The call has a wrong number of positional arguments.
    #[error("expected {expected} arguments, got {actual} at {span}")]
    ArityMismatch {
        expected: usize,
        actual: usize,
        span: Span,
    },

    /// The call passes a keyword name the callee does not accept.
    #[error("unknown keyword argument `{name}` at {span}")]
    KeywordArgMismatch { name: String, span: Span },

    /// The call omits a required argument.
    #[error("missing required argument `{name}` at {span}")]
    MissingArgument { name: String, span: Span },

    /// Two types do not unify.
    #[error("type mismatch: expected `{expected}`, found `{actual}` at {span}")]
    TypeMismatch {
        expected: Ty,
        actual: Ty,
        span: Span,
    },

    /// A `match` does not cover all constructors and has no
    /// wildcard.
    #[error("non-exhaustive match: missing case(s) {uncovered:?} at {span}")]
    NonExhaustiveMatch { uncovered: Vec<String>, span: Span },

    /// Reserved for record-row conflicts; M2 reports as
    /// `TypeMismatch` from inside record unification but keeps the
    /// variant for forward compatibility.
    #[error("conflicting field `{field}` in record types at {span}: `{ty1}` vs `{ty2}`")]
    RowConflict {
        field: String,
        ty1: Ty,
        ty2: Ty,
        span: Span,
    },

    /// `if x:` (or any truthiness position) where `x` does not have
    /// type `Bool`.
    #[error("non-bool used in truthiness position: got `{actual}` at {span}")]
    ImplicitTruthiness { actual: Ty, span: Span },

    /// Defense-in-depth: a constitution-dropped form snuck through.
    #[error("the form `{name}` is not part of Cobrust (dropped feature) at {span}")]
    UseOfDroppedFeature { name: &'static str, span: Span },

    /// Mutable default argument: the parameter's default value type
    /// is one of the M2-mutable types.
    #[error("mutable default argument is forbidden at {span}")]
    MutableDefault { span: Span },

    /// Inference left a `Var` un-resolved.
    #[error("ambiguous type at {span} (consider adding an annotation)")]
    AmbiguousType { span: Span },

    /// A record literal lists the same field twice.
    #[error("duplicate field `{name}` at {span}")]
    DuplicateField { name: String, span: Span },

    /// Unification would create an infinite type.
    #[error("occurs check: cannot unify `?{}` with `{ty}` at {span}", var.0)]
    OccursCheck {
        var: crate::ty::VarId,
        ty: Ty,
        span: Span,
    },

    #[error("not callable: `{actual}` at {span}")]
    NotCallable { actual: Ty, span: Span },

    #[error("not indexable: `{actual}` at {span}")]
    NotIndexable { actual: Ty, span: Span },

    #[error("not iterable: `{actual}` at {span}")]
    NotIterable { actual: Ty, span: Span },

    #[error("`break` outside any loop at {span}")]
    BreakOutsideLoop { span: Span },

    #[error("`continue` outside any loop at {span}")]
    ContinueOutsideLoop { span: Span },

    #[error("`return` outside any function at {span}")]
    ReturnOutsideFn { span: Span },

    #[error("`yield` outside any function at {span}")]
    YieldOutsideFn { span: Span },

    /// A composite "we recorded multiple errors" container — use
    /// when the checker wants to surface several diagnostics.
    #[error("multiple type errors")]
    Multiple(Vec<TypeError>),
}
