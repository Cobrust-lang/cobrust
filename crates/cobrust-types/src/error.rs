//! Type-checking error variants.
//!
//! Pinned by ADR-0006 §"Error taxonomy". Every variant is
//! span-bearing and printable; downstream tooling matches on the
//! variant.
//!
//! ADR-0052b §2 — every variant carries a uniform
//! `suggestion: Option<&'static str>` field populated at construction
//! time per CLAUDE.md §2.5 Direction B (LLM-first error UX).

use thiserror::Error;

use cobrust_frontend::span::Span;

use crate::ty::Ty;

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum TypeError {
    /// Defense-in-depth: lowering already catches most. The type
    /// checker may surface this for capture-only references during
    /// closure analysis.
    #[error("unknown name `{name}` at {span}")]
    UnknownName {
        name: String,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// The call has a wrong number of positional arguments.
    #[error("expected {expected} arguments, got {actual} at {span}")]
    ArityMismatch {
        expected: usize,
        actual: usize,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// The call passes a keyword name the callee does not accept.
    #[error("unknown keyword argument `{name}` at {span}")]
    KeywordArgMismatch {
        name: String,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// The call omits a required argument.
    #[error("missing required argument `{name}` at {span}")]
    MissingArgument {
        name: String,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Two types do not unify.
    #[error("type mismatch: expected `{expected}`, found `{actual}` at {span}")]
    TypeMismatch {
        expected: Ty,
        actual: Ty,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// A `match` does not cover all constructors and has no
    /// wildcard.
    #[error("non-exhaustive match: missing case(s) {uncovered:?} at {span}")]
    NonExhaustiveMatch {
        uncovered: Vec<String>,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Reserved for record-row conflicts; M2 reports as
    /// `TypeMismatch` from inside record unification but keeps the
    /// variant for forward compatibility.
    #[error("conflicting field `{field}` in record types at {span}: `{ty1}` vs `{ty2}`")]
    RowConflict {
        field: String,
        ty1: Ty,
        ty2: Ty,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// `if x:` (or any truthiness position) where `x` does not have
    /// type `Bool`.
    #[error("non-bool used in truthiness position: got `{actual}` at {span}")]
    ImplicitTruthiness {
        actual: Ty,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Defense-in-depth: a constitution-dropped form snuck through.
    #[error("the form `{name}` is not part of Cobrust (dropped feature) at {span}")]
    UseOfDroppedFeature {
        name: &'static str,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Mutable default argument: the parameter's default value type
    /// is one of the M2-mutable types.
    #[error("mutable default argument is forbidden at {span}")]
    MutableDefault {
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Inference left a `Var` un-resolved.
    #[error("ambiguous type at {span} (consider adding an annotation)")]
    AmbiguousType {
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// A record literal lists the same field twice.
    #[error("duplicate field `{name}` at {span}")]
    DuplicateField {
        name: String,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// Unification would create an infinite type.
    #[error("occurs check: cannot unify `?{}` with `{ty}` at {span}", var.0)]
    OccursCheck {
        var: crate::ty::VarId,
        ty: Ty,
        span: Span,
        suggestion: Option<&'static str>,
    },

    #[error("not callable: `{actual}` at {span}")]
    NotCallable {
        actual: Ty,
        span: Span,
        suggestion: Option<&'static str>,
    },

    #[error("not indexable: `{actual}` at {span}")]
    NotIndexable {
        actual: Ty,
        span: Span,
        suggestion: Option<&'static str>,
    },

    #[error("not iterable: `{actual}` at {span}")]
    NotIterable {
        actual: Ty,
        span: Span,
        suggestion: Option<&'static str>,
    },

    #[error("`break` outside any loop at {span}")]
    BreakOutsideLoop {
        span: Span,
        suggestion: Option<&'static str>,
    },

    #[error("`continue` outside any loop at {span}")]
    ContinueOutsideLoop {
        span: Span,
        suggestion: Option<&'static str>,
    },

    #[error("`return` outside any function at {span}")]
    ReturnOutsideFn {
        span: Span,
        suggestion: Option<&'static str>,
    },

    #[error("`yield` outside any function at {span}")]
    YieldOutsideFn {
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// ADR-0050d Decision 7A — dict key type is not Hashable.
    /// Phase F.3 admits `i64`, `str`, `bool`, `none`; rejects `f64`
    /// (NaN != NaN breaks the Hash invariant), `list`, `dict`, `set`,
    /// `tuple`, `record`, `fn`, `imag`. `Ty::is_hashable()` is the
    /// canonical predicate; emitted at `synth_dict_lit` + every
    /// `Dict[K, V]` annotation lower site (`lower_generic_type`).
    #[error("dict key type `{actual}` is not Hashable at {span}")]
    NotHashable {
        actual: Ty,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// ADR-0050d §"Parser amendments" 1 + Decision 1 commentary —
    /// dict-merge `{**other}` is Phase G; Phase F.3 rejects any
    /// `DictEntry::Spread` operand at type-check time. The parser
    /// already emits the AST variant (forward-compat); the type
    /// checker surfaces this rejection at every `Spread` entry inside
    /// a `ExprKind::Dict` literal.
    #[error(
        "dict spread (`**other`) is not supported in dict literals (Phase G feature) at {span}"
    )]
    DictSpreadNotSupported {
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// A composite "we recorded multiple errors" container — use
    /// when the checker wants to surface several diagnostics.
    #[error("multiple type errors")]
    Multiple(Vec<TypeError>),

    /// ADR-0052a Wave-1 §6 — `&expr` where `expr` is not a borrowable
    /// place. Today the parser already rejects literal-borrow,
    /// call-result-borrow, etc. at parse time (Wave-1 §8 cap); this
    /// variant is reserved for type-check-time rejection of shapes
    /// the parser admits but the checker disallows. The `suggestion`
    /// field shape was the Wave-1 forward-compat seed; ADR-0052b §2
    /// promotes the uniform `Option<&'static str>` field across all
    /// variants.
    #[error("cannot borrow non-place expression at {span}")]
    BorrowOfNonPlace {
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// ADR-0052d-prereq §"New error variant" — method-form receiver
    /// matched one of the 5 recognised types (Dict / Str / List /
    /// Float / Int) but the method name is not in that type's method
    /// table. Carries the receiver `type_name` and the attempted
    /// `method_name` so the diagnostic can list available methods
    /// per §2.5 "compile-time-catch" rule. The `suggestion` field is
    /// uniform with the ADR-0052b Direction B shape — static
    /// `&'static str` populated at construction time.
    #[error("method `{method_name}` not found on `{type_name}` at {span}")]
    UnknownMethod {
        type_name: String,
        method_name: String,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// ADR-0073 §2 D1+D8 — a `Callback` parameter slot at an ecosystem
    /// call (`app.route("GET", "/x", handler)`) requires a top-level
    /// `fn` NAME argument. The actual expression was something else
    /// (lambda, call-result, fn-typed local, non-fn name). Per §2.5
    /// Direction B the diagnostic prints the fix the LLM should
    /// apply.
    #[error("callback argument must be a top-level `fn` name at {span}")]
    CallbackArgMustBeFnName {
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// ADR-0073 §2 D1+D8 — a `Callback` parameter slot at an
    /// ecosystem call took a top-level `fn` name, but its signature
    /// does not unify with the manifest-declared callback shape (the
    /// `expected` `FnTy`). The diagnostic carries the rendered
    /// expected vs. actual `FnTy` so the LLM agent sees exactly how
    /// to fix the handler signature.
    #[error("callback signature mismatch: expected `{expected}`, found `{actual}` at {span}")]
    CallbackSignatureMismatch {
        expected: Ty,
        actual: Ty,
        span: Span,
        suggestion: Option<&'static str>,
    },
}
