---
doc_kind: module
module_id: mod:types
crate: cobrust-types
last_verified_commit: afe26db
dependencies: [mod:hir, adr:0006]
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

## Cross-references

- `adr:0006` — type system shape + inference + proof obligations.
- `mod:hir` — input.
- `mod:mir` — downstream consumer (M3+).
- Constitution `CLAUDE.md` §2.2 (drop `is`, drop implicit truthiness),
  §7 (M2 done means).
