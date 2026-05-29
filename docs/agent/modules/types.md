---
doc_kind: module
module_id: mod:types
crate: cobrust-types
last_verified_commit: e66dcfb
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
    // ADR-0080 — carried for MIR's validated-body schema synthesis.
    pub adt_fields: HashMap<AdtId, BTreeMap<String, Ty>>,           // Phase-1a field table
    pub adt_refinements: HashMap<(AdtId, String), Refinement>,      // Phase-1b-ii side-table
}

// ADR-0080 Phase-1b-ii / Phase-2 — a per-field value refinement
// (`where`-clause). One side-table entry per refined field; the variant set
// grows by phase (ADR-0080 §6).
pub enum Refinement {
    IntRange { lo: Option<i64>, hi: Option<i64> },   // Phase-1: i64 value range → minimum/maximum
    StrLen   { lo: Option<i64>, hi: Option<i64> },   // Phase-2: str length    → minLength/maxLength
    Pattern  { regex: String },                      // Phase-2: str regex     → pattern
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
- `UnknownField` (ADR-0080 Phase-1a — attribute access on a class
  instance named a field the class does not declare; carries
  `{ field, adt, known_fields, span, suggestion }`; the Display message
  lists `known_fields` as the §2.5-B FIX. FixSafety `LocalEdit`.)
- `UnsupportedRefinement` (ADR-0080 Phase-1b-ii / Phase-2 — a class field's
  `where` refinement predicate is not a fixed grammar for its base type:
  the int-range grammar on an `i64` field (`0 <= self <= 100`), the
  str-length grammar on a `str` field (`len(self) <= n`), or the str-pattern
  grammar on a `str` field (`pattern(self, "<re>")`); also raised for a
  MALFORMED regex in `pattern(...)` (compile-checked at type-check time).
  Carries `{ field, span, suggestion }`; the Display message prints the
  accepted forms as the §2.5-B FIX. FixSafety `LocalEdit`.)
- `Multiple` (composite container for multi-error reporting)

### ADR-0080 Phase-1b-i — class NAME in a type-annotation resolves to its `Adt`

| Feature | Location | Notes |
|---|---|---|
| `class_names` table | `types/src/check.rs` `Ctx::class_names` | `class name → AdtId(c.def_id.0)` — the SAME id the zero-arg ctor's `return_ty` carries (`prebind_item` `ItemKind::Class` arm) |
| population | `types/src/check.rs` `prebind_item` `ItemKind::Class` | recorded in **Pass 1** (prebind), before any body annotation is lowered → forward class refs resolve |
| resolution | `types/src/check.rs` `lower_named_type` | a name in `class_names` lowers to `Ty::Adt(adt_id, [])`; checked AFTER `alias_map` and BEFORE the opaque-`Alias` fall-through |

Invariants:
- A `: Score` annotation and a `Score()` instance UNIFY (both `Ty::Adt(AdtId(score), [])`; unifier `(Adt(a), Adt(b)) if a == b`). Enables class-typed bindings (`let s: Score = Score()`), params (`fn f(s: Score)`), and returns (`-> Score`), combining with Phase-1a typed field access (`s.rank: i64`).
- **Nominal distinctness preserved**: two DIFFERENT classes get two DISTINCT `AdtId`s and do NOT cross-unify (`ill_typed` i156); a non-instance RHS (`let s: Score = 5`) still rejects `TypeMismatch` (i155).
- **No regression of real aliases / unknown names**: a `type Foo = Bar` alias is resolved earlier via `alias_map` (transparently to its RHS, never a class arm); a name that is NOT a class (typo, forward-ref to a non-class, generic-param spelling) is absent from `class_names` and still falls through to the opaque-`Alias` arm exactly as before.
- No new error variant, no Display/error-UX change — a purely internal unification correctness fix (the rejection categories are unchanged `TypeMismatch`).

### ADR-0080 Phase-1b-ii — validated-body refinement side-table + `route_validated` callback gate

| Feature | Location | Notes |
|---|---|---|
| `adt_refinements` side-table | `types/src/check.rs` `Ctx::adt_refinements` + `TypedModule::adt_refinements` | `(AdtId, field) → Refinement`; the sibling of `adt_fields` (Q2 — refinements live BESIDE the field, NOT in `Ty`) |
| refinement interpret | `types/src/check.rs` `check_class` → `interpret_refinement` → `interpret_int_range` / `interpret_str_refinement` / `parse_bound_predicate` / `parse_subject_bound` | reads each `ClassBody::field_refinements` `where`-predicate; dispatches on the field BASE TYPE — `i64` → int-range grammar (`lo <= self`, `self <= hi`, `lo <= self and self <= hi`, `>=` mirror, strict `<`/`>` ±1-shift inclusive); `str` → str-length (`len(self)`-subject bound, SAME shape) or str-pattern (`pattern(self, "<lit>")`); else `TypeError::UnsupportedRefinement` + §2.5-B FIX |
| str grammar (Phase-2) | `types/src/check.rs` `is_len_self_call` / `parse_pattern_call` | `is_len_self_call` recognises a `len(self)` call as the length-bound subject; `parse_pattern_call` recognises `pattern(self, "<string-literal>")` and returns the literal regex. `len`/`pattern` are bound as synthetic refinement keywords at HIR lowering (`cobrust-hir`) so the predicate resolves self-contained |
| regex compile-check (Phase-2) | `types/src/check.rs` `interpret_str_refinement` (`regex::Regex::new`) | a MALFORMED regex in `pattern(...)` is a BUILD-time `TypeError::UnsupportedRefinement` with a FIX (§2.5-B), NOT a per-request runtime panic. New direct dep `regex = "1"` (already in the workspace lock; F64: Cargo.lock staged) |
| validated-handler callback | `types/src/ecosystem.rs` `pit_validated_handler_fn_ty` + `PIT_VALIDATED_BODY_SENTINEL_ADT`; the `route_validated` manifest row | callback `FnTy = fn(pit.Request, <Body>) -> pit.Response` with a sentinel 2nd-param |
| sentinel-slot accept | `types/src/check.rs` `check_callback_arg` | the sentinel 2nd-param slot accepts ANY field-tracked user class (`Ty::Adt` outside the eco range with recorded fields); a non-class param or a 1-arg handler → `CallbackSignatureMismatch` + FIX |

Invariants:
- The refinement side-table + field table are CARRIED on `TypedModule` so MIR synthesises the validated-body schema descriptor for `route_validated` from the SAME source the checker used (footgun #4 — schema + validator cannot drift). `Refinement::descriptor_payload(base_kind)` is the ONE encoder MIR calls; `cobrust-pit`'s `parse_schema` is the ONE decoder — they cannot drift.
- The `where`-predicate is interpreted STRUCTURALLY (the fixed grammar over the lowered HIR expr — `Bin{And}`, `Bin{LtEq/Lt/GtEq/Gt}`, `Lit(Int)`, `Lit(Str)`, `Name("self")`, `Call{Name("len"|"pattern"), …}`), never type-synthesised; `self`'s type is irrelevant.
- The fixed grammar is keyed on the field BASE TYPE: a `len`/`pattern` form on a non-`str` field, or a bare-`self` int-range on a `str` field, is a clear `UnsupportedRefinement` (`ill_typed` i161-i164) — not silently mis-interpreted.
- The value-level constraint (range / length / pattern) stays a RUNTIME guard (a 422 at the request boundary), not a compile-time-checked refinement (the §2.5-superior form is an ADR-0080 §9 follow-up). The regex's WELL-FORMEDNESS is the one part caught at compile time (Phase-2).

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

## ADR-0052a §4.4 + §8 — let-rebind shortcut + `&p.<field>` (Wave-1 closure)

### §4.4 let-rebind shortcut

- **Surface**: `let s = &s` (top-level binding pattern in a `let`
  statement re-binds the same name within the same scope).
- **HIR (`cobrust-hir`)**: `Scope::bind_let_shadow` allows the new
  binding to replace the prior one unconditionally. `Lower::bind_let`
  wraps the call; `lower_let_pattern_with_bindings` is the let-aware
  variant of `lower_pattern_with_bindings`, threaded into both
  module-level and block-level `StmtKind::Let` arms. Sub-patterns
  inside tuples / dicts / class patterns continue to flow through the
  strict bind path so `let (x, x) = ...` still rejects with
  `DuplicateBinding`.
- **Types**: no new code path. The RHS `&s` synthesises `Ty::Ref(Str)`
  per ADR-0052a §6; the new `s` binding adopts that type. Subsequent
  reads through the rebound binding flow through the existing
  one-way `&T → T` call-arg coercion at `synth_call`.
- **MIR**: no new code path. HIR assigns a fresh `DefId` for the
  rebound `s`; `lower_let` allocates a fresh local; `lower_borrow_inner`'s
  `Name` arm emits `Operand::Copy` of the local. No
  `__cobrust_str_clone` is inserted (verified by F30 witness
  `f30wit_04_let_rebind_synthetic_no_clone_no_uaf`).

### §8 Wave-1 `&p.<field>` field-projection borrow

- **Lexer (`cobrust-frontend`)**: `prev_token_completes_postfix` gates
  the `.<digit>` → `Float(.N)` collapse. After an ident / `]` / `)` /
  string / bytes token, `.0` lexes as `Dot Int("0")` instead of
  `Float(".0")`. This unlocks the Rust-style tuple-field syntax.
- **Parser**: `parse_postfix` Dot arm accepts `Int(s)` as the
  attribute name (synthesises `Attribute { name: "0" }`). Identifier
  attribute names continue to flow through `expect_ident()`.
- **Types**: `synth_expr → ExprKind::Attr` resolves numeric attribute
  names against `Ty::Tuple(items)`. Index parsing via
  `name.parse::<usize>()` gates the tuple-field path; non-numeric
  names retain the prior `fresh_var()` ADT-conservative fallback.
  Out-of-bounds tuple indices surface as `NotIndexable` (§2.5
  fix-suggestion path).
- **Borrow validator**: already accepts `Access(Attribute { base, .. })`
  per ADR-0052a §8; no validator change needed. `&p.0` now type-checks
  to `Ty::Ref(items[0])` via the borrow-of-place arm at `check.rs:1332`.

### Invariants preserved

- F31 lock: no new `(Ty::Ref(T), T)` cross-arm in `infer::unify` —
  Cluster A closure does not add a bidirectional unify rule.
- BorrowOfNonPlace continues to fire at the existing §6 emission
  points — only the §8 Wave-1 admitted shapes pass.

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

## Phase I ADR-0056b — `TypeCheckCtx` (Clone+Send Arc-COW snapshot)

Per ADR-0056b §3.3 + §5 + §6 (accepted at `b0e1e9e`):

- `TypeCheckCtx` lives in `cobrust_types::check` and is the
  cross-turn / cross-file incremental type-check state carrier.
- `derive(Clone, Debug, Default)`; `Send + 'static` (compile-time
  asserted by `tests/type_check_ctx_contract.rs`).
- Five internal `Arc<HashMap<...>>` rows for O(1) Clone via
  `Arc::clone` + COW writes via `Arc::make_mut`:
  - `bindings: HashMap<String, Ty>` — name → type
  - `binding_defs: HashMap<String, u32>` — name → owning DefId
    (load-bearing for `invalidate(file_id)` per ADR-0056b §11.2)
  - `def_types: HashMap<u32, Ty>` — DefId → Ty
  - `file_defs: HashMap<u32, Vec<u32>>` — FileId → owned DefIds
  - `binding_defs`, `alias_map`, `subst` — carried for ADR-0056c.

- Public surface (compile-time-catch + training-data-overlap §2.5):

```rust
impl TypeCheckCtx {
    pub fn new() -> Self;
    pub fn lookup(&self, name: &str) -> Option<&Ty>;
    pub fn def_type(&self, def_id: u32) -> Option<&Ty>;
    pub fn alias(&self, name: &str) -> Option<&Ty>;
    pub fn subst(&self) -> &Subst;
    pub fn version(&self) -> u64;                       // ADR-0056b §6
    pub fn binding_count(&self) -> usize;
    pub fn bindings(&self) -> impl Iterator<Item = (&String, &Ty)>;
    pub fn invalidate(&mut self, file_id: u32);         // ADR-0057a §4
    pub fn merge_module(&mut self, &TypedModule, file_id: u32);
}

pub fn check_incremental(
    ctx: &mut TypeCheckCtx,
    module: &Module,
    file_id: u32,
) -> Result<TypedModule, TypeError>;
```

- Per-turn write path: `merge_module` records every Fn/Let/Class
  name → type row + DefId provenance. Redefine replaces in place.
- Per-file invalidate path: drops every DefId row recorded against
  `file_id` from `def_types`, then drops name-keyed `bindings` rows
  whose owning DefId is in the removed set, then defence-in-depth
  drops rows whose RESOLVED TYPE references a removed Adt/Alias.
- `version()` bumps on every `merge_module` / `invalidate` —
  monotone signal for Phase J snapshot freshness (§6).

Tests: `crates/cobrust-types/tests/type_check_ctx_contract.rs`
(16 cases — Clone+Send compile-time + Arc-COW isolation + invalidate
+ version monotonicity + cross-thread snapshot survival).

## Phase I wave-3 ADR-0056c — `invalidate_def` + `binding_def_id`

Per ADR-0056c §4 fn-redefinition lifecycle (accepted at impl-merge).
Two new public methods extend the §3.3 + §5 surface:

```rust
impl TypeCheckCtx {
    /// Per-symbol invalidation (sibling of `invalidate(file_id)`).
    /// Drops one DefId's row from `def_types`, drops name-keyed
    /// `bindings` / `binding_defs` entries whose owner is this
    /// DefId, drops `bindings` rows whose resolved type references
    /// the DefId via `type_refs_any`, removes the DefId from any
    /// `file_defs` vector, bumps `version`.
    pub fn invalidate_def(&mut self, def_id: u32);

    /// Lookup the DefId owning a named binding. Callers of
    /// `invalidate_def` use this to resolve `name → DefId` before
    /// invalidation (e.g. REPL `Session::redefine_fn` in cli/repl.rs).
    pub fn binding_def_id(&self, name: &str) -> Option<u32>;
}
```

- `invalidate_def` is the load-bearing primitive for cross-turn
  fn-redefinition. REPL `evaluate_module` pre-scans top-level fn-defs,
  captures the pre-existing DefIds via `binding_def_id`, then calls
  `invalidate_def(old_def_id)` BEFORE `merge_module` reinstalls the
  fresh binding.
- Internal helper `invalidate_with(file_id, extra: Option<u32>)`
  unifies the file-scoped and DefId-scoped paths (single shared
  removal-set + COW write).
- Public `invalidate(file_id)` semantics unchanged — wave-3 is
  strictly additive to the wave-2 surface.

Tests: `crates/cobrust-cli/tests/session_fn_redef.rs` (8 cases —
identical re-def, arity / param-type / return-type changes,
:type-after-redef, :clear-then-redef, failed-typecheck preserves
old binding, first-def-silent).

## Cross-references

- `adr:0006` — type system shape + inference + proof obligations.
- `adr:0050a` — break/continue contract seal (loop scope discipline).
- `adr:0050` §A1 — M-F.3.3 f64 gap table.
- `adr:0050d` — M-F.3.4 dict design (sub-sprint a..g blueprint).
- `adr:0056b` — Phase I × J handoff primitive (`TypeCheckCtx`).
- `adr:0056c` — Phase I wave-3 fn-redefinition + per-symbol invalidation.
- `adr:0057a` — Phase J wave-1 LSP `publishDiagnostics` consumer.
- `mod:hir` — input.
- `mod:mir` — downstream consumer (M3+).
- `mod:cli` — REPL `Session` carrier (ADR-0029 + ADR-0056b §3.3).
- Constitution `CLAUDE.md` §2.2 (drop `is`, drop implicit truthiness),
  §7 (M2 done means).
