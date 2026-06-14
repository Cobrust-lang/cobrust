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

    /// ADR-0080 Phase-1a — attribute access on a class instance
    /// (`Ty::Adt`) named a field that the class does not declare.
    ///
    /// Once `check_class` records each `class`-body field declaration
    /// (`let <name>: <ty> = …`) into the per-Adt field table, the
    /// `Attr` arm resolves a known field to its declared `Ty` and
    /// raises this variant for an unknown one — *instead* of falling
    /// back to `fresh_var()` (which silently unified with anything, so
    /// a typo'd `body.titel` slipped through to a runtime surprise).
    /// This is the §2.5-A compile-time-catch for a mistyped field
    /// access; the `#[error]` message *prints the fix* (§2.5-B): it
    /// names the offending field, the class, and the **declared field
    /// list** so the LLM agent can correct the name on the next turn.
    /// `suggestion` carries the uniform static hint (ADR-0052b §2).
    #[error(
        "no field `{field}` on `{adt}` at {span}; \
         declared fields: {}",
        if known_fields.is_empty() { "(none)".to_string() } else { known_fields.join(", ") }
    )]
    UnknownField {
        field: String,
        adt: Ty,
        known_fields: Vec<String>,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// ADR-0080 Phase-1b-ii — a class field's `where`-clause refinement
    /// predicate is not in the FIXED grammar v1 admits (ADR-0080 Q6).
    ///
    /// The FIXED refinement forms accepted (ADR-0080 Q6 + Phase-2/3a) are:
    /// an `i64` int-range (`lo <= self and self <= hi` + one-sided); an `f64`
    /// float-range (the same shape, inclusive `<=`/`>=` ONLY — a strict
    /// `<`/`>` is rejected, the reals are dense); a `str` length over
    /// `len(self)`; and a `str` `pattern(self, "…")`. Any other shape — an
    /// arbitrary fn call (`weird(self)`), a refinement on the WRONG base type
    /// (`len(self)` on an `i64`, a strict `<` on an `f64`), or a malformed
    /// comparison — raises this variant. Per §2.5-B the message PRINTS THE
    /// FIX: it names the field and ALL FOUR accepted forms (the negative
    /// corpus asserts the rendered text NAMES the relevant form — see #161
    /// `must_reject_with_msg`) so the LLM agent rewrites the predicate on the
    /// next turn. `suggestion` carries the uniform static hint.
    #[error(
        "unsupported refinement `where`-predicate on field `{field}` at {span}: \
         use one of the fixed refinement forms — \
         an i64 int-range `0 <= self and self <= 100` (inclusive); \
         an f64 float-range `0.0 <= self and self <= 1.0` (inclusive `<=`/`>=` ONLY — \
         a strict `<`/`>` is rejected, the reals are dense); \
         a str length `len(self) <= n` (or `len(self) >= n`); \
         or a str pattern `pattern(self, \"<regex>\")`"
    )]
    UnsupportedRefinement {
        field: String,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// ADR-0088 §3 — the Python-canonical free-function `len(x)` builtin
    /// was applied to a value whose type is NOT sized. The SIZED types
    /// `len` accepts (the types that ship with a `len` runtime symbol)
    /// are `str` (`__cobrust_str_len_src`), `list[T]` (`__cobrust_list_len`),
    /// and `dict[K, V]` (`__cobrust_dict_len`). Calling `len(5)` /
    /// `len(3.0)` / `len(true)` is a compile-time error per §2.5-A.
    ///
    /// Per §2.5-B the message PRINTS THE FIX: it names the offending
    /// argument type AND the exact accepted sized-type set, so the LLM
    /// agent corrects the call on the next turn — *instead* of the
    /// pre-ADR-0088 `type mismatch: expected Dict[?,?], found <T>`
    /// diagnostic, whose "expected Dict" leaked the dict-only PRELUDE
    /// stub and mislead the agent toward a dict (§2.5-B violation).
    #[error(
        "`len(x)` needs a sized argument but got `{actual}` at {span}: \
         the free-function `len` accepts a `str`, a `list[T]`, or a `dict[K, V]` \
         (for a number use a comparison; `len` is not defined on `{actual}`)"
    )]
    LenArgNotSized {
        actual: Ty,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// ADR-0092 — `event.send_output("<id>", payload)` named an output
    /// id that the node's `@dora.node(outputs=[...])` decorator does NOT
    /// declare. This LIFTS the dora send-output undeclared-id reject from
    /// RUNTIME (a `cobrust-dora` `eprintln!` + a `-1` return; ADR-0076
    /// Phase 2) to COMPILE TIME (CLAUDE.md §2.5-A compile-time-catch): a
    /// mistyped output id is now a `cobrust check` error, not a silent
    /// runtime drop.
    ///
    /// Raised ONLY when the id is a STRING LITERAL and the module DECLARES
    /// outputs (one or more `dora.declare_output(...)` desugars from the
    /// decorator). A non-literal id (a variable / computed `str`) cannot
    /// be proven statically, so it is SKIPPED — the runtime backstop stays.
    /// A bare `@dora.node` (no `outputs=`) declares NOTHING, so the check
    /// is INERT (no false-positive on the un-typed surface).
    ///
    /// Per §2.5-B the message PRINTS THE FIX: it names the offending id,
    /// the **declared output list**, and — when one declared id is a near
    /// edit-distance match — a `did you mean "<nearest>"?` suggestion, so
    /// the LLM agent rewrites the call on the next turn. `declared` +
    /// `nearest` are owned (dynamic) `String`s so the FIX renders the real
    /// per-node ids; `suggestion` carries the uniform ADR-0052b static
    /// hint (the constant clause the LSP + fix-safety ladder consume).
    #[error(
        "unknown dora output id `{id}` — it is not declared in \
         `@dora.node(outputs=[...])` at {span}; declared outputs: [{}]{}",
        declared.join(", "),
        match nearest {
            Some(n) => format!("; did you mean `{n}`?"),
            None => String::new(),
        }
    )]
    DoraUnknownOutputId {
        id: String,
        declared: Vec<String>,
        nearest: Option<String>,
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// ADR-0093 Phase-2 §"Slice-shape soundness" — a `bytes` slice
    /// expression used a shape the runtime cannot yet honour, so it is
    /// REJECTED at compile time (§2.5-A) instead of silently miscompiling
    /// (§2.2). The ONLY supported `bytes` slice form is the contiguous
    /// `b[lo:hi]` with BOTH non-negative bounds present and the default
    /// step (`__cobrust_bytes_slice(b, lo, hi)`). An open-ended bound
    /// (`b[1:]` / `b[:3]` / `b[:]`), a non-unit step (`b[0:4:2]`), or a
    /// negative bound (`b[1:-1]`) is an ADR-0093 §Phasing deferral.
    ///
    /// **Why a hard reject, not a silent fallthrough.** Before this arm,
    /// every non-`lo:hi` shape type-checked as `Ty::Bytes` then fell
    /// through the MIR `bytes`-index guard to the generic `Projection::
    /// Index` path, where the `Slice` collapsed to `Constant::Int(0)` and
    /// the index projection was a codegen no-op — so the expression
    /// silently evaluated to the WHOLE base buffer (e.g. `b"hello"[1:]`
    /// gave `len 5`, not CPython's `4`). That is the exact §2.2
    /// silent-coercion / §2.5 compile-time-catch-miss the constitution
    /// most forbids; a wrong answer at exit 0 with no diagnostic.
    ///
    /// Per §2.5-B the message PRINTS THE FIX: it names the supported
    /// `b[lo:hi]` form so the LLM agent rewrites the slice on the next
    /// turn (e.g. `b[1:]` → `b[1:len(b)]`). `suggestion` carries the
    /// uniform ADR-0052b static hint.
    #[error(
        "unsupported `bytes` slice shape at {span}: only a contiguous \
         `b[lo:hi]` slice with both non-negative bounds present and the \
         default step is supported (an open-ended `b[1:]`/`b[:3]`, a \
         non-unit step `b[0:4:2]`, or a negative bound `b[1:-1]` is not \
         yet supported); write both explicit bounds, e.g. `b[1:len(b)]`"
    )]
    UnsupportedSliceShape {
        span: Span,
        suggestion: Option<&'static str>,
    },

    /// F90 / ADR-0102 (§2.5-A) — an `int ** int` POWER with a NEGATIVE
    /// LITERAL exponent (`2 ** -1`, `base ** -3`). Cobrust pins
    /// `int ** int -> int` (a static `i64` result), but a negative
    /// exponent yields a non-integer in Python (`2 ** -1 == 0.5`), so an
    /// `int`-typed result is impossible — this is a COMPILE-TIME reject
    /// (mirrors F79's negative-literal scalar-index reject), NOT a silent
    /// wrong value (§2.2). A runtime-DYNAMIC negative exponent (a variable,
    /// not a literal) TRAPS at runtime via `__cobrust_ipow` (exit 3); only
    /// the literal case is catchable here.
    ///
    /// Per §2.5-B the message PRINTS THE FIX: use a float base so the
    /// result is a float (`float(base) ** exp`, or write the base as a
    /// float literal `2.0 ** -1`). `suggestion` carries the uniform static
    /// hint (ADR-0052b §2).
    #[error(
        "`int ** int` with a negative exponent at {span} yields a non-integer \
         (e.g. `2 ** -1 == 0.5`), but Cobrust pins `int ** int -> int`; \
         use a float base so the result is a float — write `float(base) ** exp` \
         or make the base a float literal (e.g. `2.0 ** -1`)"
    )]
    NegativePowExponent {
        span: Span,
        suggestion: Option<&'static str>,
    },
}
