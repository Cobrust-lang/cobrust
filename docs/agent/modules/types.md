---
doc_kind: module
module_id: mod:types
crate: cobrust-types
last_verified_commit: e85630f
dependencies: [mod:hir, adr:0006, adr:0041]
---

# Module: types

## Purpose

Static structural type system + bidirectional type checker. M2 ships
the static core (no `dyn`); `dyn` is opt-in and added later.

## Status

- **M2 — delivered.** 54 well-typed programs accepted, 54 ill-typed
  programs rejected (each with the right `TypeError` discriminant).

## Public surface (M2)

```rust
pub fn check(module: &hir::Module) -> Result<TypedModule, TypeError>;

pub struct TypedModule {
    pub def_types: HashMap<u32, Ty>,
    pub hir: hir::Module,
}

pub enum Ty {
    Bool, Int, Float, Imag, Str, Bytes, None, Never,
    Tuple(Vec<Ty>),
    List(Box<Ty>), Set(Box<Ty>), Dict(Box<Ty>, Box<Ty>),
    Record(Record),
    Fn(FnTy),
    Adt(AdtId, Vec<Ty>),
    Alias(AliasId, Vec<Ty>),
    Generic(GenericVar),
    Var(VarId),
}

pub struct Record { pub fields: BTreeMap<String, Ty> }
pub struct FnTy {
    pub positional: Vec<Ty>,
    pub named: Vec<(String, Ty)>,
    pub var_positional: Option<Box<Ty>>,
    pub var_keyword: Option<Box<Ty>>,
    pub return_ty: Box<Ty>,
}

pub struct VarAllocator { /* atomic */ }
```

## Inference algorithm

Bidirectional, as pinned by `adr:0006` §"Inference algorithm:
bidirectional".

- `synth(e)` synthesises a type from `e`.
- `check(e, expected)` checks `e` against an expected type, extending
  the running substitution if necessary.
- Unification is first-order with occurs-check.
- The implementation lives at `crates/cobrust-types/src/{infer.rs, check.rs}`.

## Type system shape

- **Structural typing** by default — record types match by field
  signature, not by nominal identity.
- **Algebraic data types** with exhaustive pattern matching.
- **Generics** with explicit type parameters (prenex; higher-rank
  deferred).
- **No `dyn` in M2.** Trait objects arrive in M3+ behind an explicit
  opt-in keyword.
- **No subtyping at value level**; coercions are explicit functions.
- **No row polymorphism in M2**; closed records only.
- **`if x` requires `x: bool`.** Implicit truthiness is a type error
  (`TypeError::ImplicitTruthiness`).
- **`is` is unrepresentable** — defended at three layers (parser,
  lowering, type checker).
- **Mutable defaults** rejected at parse time and re-checked at the
  type-check layer (`TypeError::MutableDefault`).

## Error taxonomy

`TypeError` variants — pinned by `adr:0006` §"Error taxonomy":

- `UnknownName` — name not found (defense-in-depth; lowering catches first)
- `ArityMismatch`, `KeywordArgMismatch`, `MissingArgument`
- `TypeMismatch`
- `NonExhaustiveMatch`
- `RowConflict` (forward-compat placeholder; M2 surfaces as
  `TypeMismatch` from inside record unification)
- `ImplicitTruthiness`
- `UseOfDroppedFeature`
- `MutableDefault`
- `AmbiguousType`
- `DuplicateField`
- `OccursCheck`
- `NotCallable` / `NotIndexable` / `NotIterable`
- `BreakOutsideLoop` / `ContinueOutsideLoop` / `ReturnOutsideFn` /
  `YieldOutsideFn`
- `Multiple` (composite container for multi-error reporting)

## ADR-0041 §H8 — tuple Index returns indexed element type

Pre-fix: `(Ty::Tuple(items), IndexKind::Expr(_))` returned
`items.first()` regardless of the index expression. This silently
typed every tuple index as the first-element type — `t[1]` on
`Tuple(i64, str, bool)` synthesised `Ty::Int` not `Ty::Str`.

Post-fix: when the index expression is a literal int (with optional
unary minus), constant-fold via `literal_int_value` +
`resolve_tuple_index`. Negative indices fold from the right (Python
semantics: `t[-1]` is the last element). Out-of-range indices
synthesise `Ty::Never` (defense-in-depth — runtime would panic). For
non-constant indices (e.g. dynamic `t[i]`), the conservative fallback
remains `items.first()` — row polymorphism (M3+) widens this to a
union.

Same ADR landed `Tuple[A, B, C]` annotation handling at
`lower_generic_type`; pre-fix this fell through to a fresh inference
variable which surfaced as `AmbiguousType` whenever the tuple
appeared in a return-type annotation.

## Invariants (M2)

- Type errors never emit a "best guess" type — either inferred or
  hard error.
- Compile-time exhaustiveness: every `match` either covers all
  constructors or has a wildcard / binding pattern.
- Soundness for the static core is tracked as proof obligation list
  in `adr:0006` §"Soundness proof obligation list" (proof itself
  deferred OK per constitution §5.2).

## Done means (M2 — DONE)

- [x] Curated suite: 54 well-typed programs accepted
      (`crates/cobrust-types/tests/well_typed.rs`).
- [x] Curated suite: 54 ill-typed programs rejected with the right
      error category (`crates/cobrust-types/tests/ill_typed.rs`).
- [x] All M1 "core 30 forms" type-check with reasonable annotations
      (verified by w52, w26, w50, etc.).
- [x] `adr:0006` accepted; implementation matches.

## Non-goals

- No runtime reflection in M2.
- No effect system in M2 (deferred).
- No subtyping (deliberate; constitution §2.2).
- No row polymorphism in M2 (future ADR may revisit).

## ADR-0050a M-F.3.0 — `break` / `continue` scope discipline

| Surface | Anchor |
|---|---|
| Counter | `Ctx::loop_depth: usize` (L82-84 of `check.rs`) — non-zero ⇒ inside loop scope. |
| Increment | `check_loop` L415 (While arm) + L434 (For arm) — increments before checking body, decrements after. |
| Reject | `check_stmt` L308-319 — `StmtKind::Break` / `Continue` returns `TypeError::BreakOutsideLoop` / `ContinueOutsideLoop` if `loop_depth == 0`. |
| Diverges | Both branches return `BlockOutcome::Diverges` so statements after them are flagged as unreachable. |
| Scope discipline (ADR-0050a §"Scope discipline") | `check_fn` L264-279 of `check.rs` — saves `loop_depth`, sets to 0 for the function body, restores on return. This prevents a nested `fn` inside a loop from seeing the outer loop's scope. |

Test corpus: `crates/cobrust-types/tests/break_continue_types_corpus.rs`
— 18 well-typed acceptance + 16 ill-typed rejection cases. Covers
nested-fn boundary (b13/b14), while-else boundary (b11/b12), and
deep nesting (a09 at 5 levels).

## M-F.3.3 — f64 and `as`-cast type-checking (ADR-0050 §A1)

| Feature | Location | Notes |
|---|---|---|
| `synth_expr` Cast arm | `types/src/check.rs` `synth_expr` | resolves `ExprKind::Cast { expr, target }` by name-matching `target` |
| Allowed cast pairs | `i64 → f64`, `f64 → i64` | constitution §2.2: no bool→f64, no str→anything |
| Rejected cast pairs | all others | `TypeError::TypeMismatch { expected, actual, span }` |
| `finalize` usage | `types/src/infer.rs` `finalize(&from_ty, &subst, span)` | resolves any inference vars before the pair check |
| `lower_named_type("f64")` | `types/src/check.rs` | maps `"f64"` → `Ty::Float`; `"i64"` → `Ty::Int` |

Invariants:
- Cast type-checking resolves the TARGET type from the raw AST type name (not via HIR type-lowering, since the HIR type is an AST `Type`).
- `bool → f64` is rejected (only `bool → i64` via `BoolToInt` CastKind — not surfaced in source yet).

## M-F.3.4 — dict type-checking (ADR-0050d sub-sprint b)

| Feature | Location | Notes |
|---|---|---|
| `Ty::Dict(K, V)` parametric | `types/src/ty.rs` | already exists; both K and V are boxed `Ty` |
| `Ty::is_hashable()` predicate | `types/src/ty.rs::Ty::is_hashable` | true for `bool` / `i64` / `str` / `bytes` / `None` / `Never` / `Tuple(items if all hashable)`; false for `f64` / `imag` / `list` / `set` / `dict` / `record` / `fn` / `adt` / `alias` / `generic` / `var` (Phase G extends ADT to hashable-if-trait-impl) |
| `synth_dict_lit` hashability + spread guard | `types/src/check.rs::synth_expr` `ExprKind::Dict` arm | after entry-wise K/V unify, `subst.apply(K)` + check hashability; `DictEntry::Spread` rejected at first occurrence with `DictSpreadNotSupported` |
| `synth_comp` dict-comp K hashability | `types/src/check.rs::synth_comp` `CompKind::Dict` arm | check K hashable after entry synth |
| `validate_hashable_dict(&HirType)` | `types/src/check.rs` | walks HIR type tree (preserves spans), surfaces `NotHashable` at `Let` annot / fn params / fn return / type alias |
| `TypeError::NotHashable { actual: Ty, span }` | `types/src/error.rs` | "dict key type `{actual}` is not Hashable at {span}" |
| `TypeError::DictSpreadNotSupported { span }` | `types/src/error.rs` | "dict spread (`**other`) is not supported in dict literals (Phase G feature) at {span}" |
| `iter_element(Dict(K,_)) -> K` | `types/src/check.rs::iter_element` | unchanged; `for k in d:` already keys-mode via this rule |
| Method-intrinsic recognition | `types/src/check.rs::try_synth_dict_method` | `d.keys() -> List[K]` / `d.values() -> List[V]` / `d.items() -> List[Tuple[K, V]]` / `d.get(k) -> V` / `d.get(k, default) -> V` (sentinel-pair scope cap per §"Surface coverage matrix" caveat) / `d.copy() -> Dict[K, V]` (shallow clone per Decision 10A) |
| Row-polymorphic widening for `dict_is_empty` | `types/src/check.rs::is_list_polymorphic_intrinsic_name` + `instantiate_list_polymorphic` Dict arm | `dict_is_empty(d: Dict[i64, i64])` PRELUDE stub widens to `Dict[?A, ?B]` at every call site so any (K, V) shape unifies |
| Empty-literal disposition | `types/src/check.rs::synth_expr` `ExprKind::Dict` empty arm | `let d = {}` (no annot) synthesises `Ty::Dict(?K, ?V)`; final resolution at `check()` surfaces `AmbiguousType` if no later use pins K/V. Fresh-K inference deferred to Phase G |

Invariants:
- Hashable K is enforced at every Dict construction + annotation site;
  rejection occurs at type-check time (no runtime NaN surprises per
  constitution §2.2).
- Spread (`{**other}`) in non-comprehension dict literal is rejected
  with `DictSpreadNotSupported`; the parser AST variant
  `DictEntry::Spread` stays for forward compat to Phase G dict-merge.
- Method-intrinsic dispatch happens **before** the generic
  `synth_call` path so the type-checker returns precise types for
  `.keys()` / etc. without falling back to fresh-var Attr resolution.
- The type-checker side recognises dict methods but the codegen side
  remains an M12.x stub for sub-sprint d; downstream MIR / codegen
  emit may not yet honour the recognised type.

## Cross-references

- `adr:0006` — type system shape + inference + proof obligations.
- `adr:0050a` — break/continue contract seal (loop scope discipline).
- `adr:0050` §A1 — M-F.3.3 f64 gap table.
- `adr:0050d` — M-F.3.4 dict design (sub-sprint a..g blueprint).
- `mod:hir` — input.
- `mod:mir` — downstream consumer (M3+).
- Constitution `CLAUDE.md` §2.2 (drop `is`, drop implicit truthiness),
  §7 (M2 done means).
