---
doc_kind: adr
adr_id: 0006
title: Type system shape, inference algorithm, and proof obligations for the static core
status: accepted
date: 2026-04-30
last_verified_commit: afe26db
supersedes: []
superseded_by: []
dependencies: [adr:0003, adr:0005]
---

# ADR-0006: Type system shape, inference algorithm, and proof obligations for the static core

## Context

Constitution `CLAUDE.md` §7 places `mod:types` as the M2 deliverable
("type checker for the static core, no `dyn` yet"). `mod:types.md`
listed indicative goals (structural typing, ADTs, generics,
bidirectional inference, exhaustive `match`) but deferred specifics
to "the M2 ADR." This is that ADR.

The type system must:

- **Honour `CLAUDE.md` §2.2.** No silent coercion. No implicit
  truthiness. No subtyping at value level. `is` removed. Mutable
  defaults rejected. Async-coloring eliminated.
- **Be *static* and *structural*.** §2.1 keeps Python's expressive
  surface; §2.2 drops dynamic dispatch as default. Records match by
  shape, not nominal identity.
- **Be sound for the static core.** §5.2 ("scientific") demands an
  enumerated proof obligation list, even if the proof itself is
  deferred.
- **Have *concrete*, *finite* error categories.** §6 demands ADRs
  for cross-cutting decisions; the error taxonomy is the API of the
  type checker for downstream tooling.

The HIR shape is fixed by `adr:0005`. The 30-form input surface is
fixed by `adr:0003`. This ADR fills in the typing rules, the
inference algorithm, and the proof obligation list.

## Options considered

1. **Hindley-Milner with global type inference, no annotations
   required.**
   - Pros: minimal annotation burden.
   - Cons: HM does not interact well with structural records, ADTs
     with constructors, or polymorphic recursion. Error messages
     degrade catastrophically when annotations are absent. Rejected
     for §5.1 ("elegance: one way per thing — but error messages
     come first").

2. **Bidirectional checking — annotations propagate inward, types
   synthesize outward; HM-style unification only at unannotated
   leaves.** *(chosen)*
   - Pros: well-understood, plays well with structural records and
     ADTs, gives precise diagnostics, lets the user opt into more or
     fewer annotations without affecting checkability.
   - Cons: type checker has two modes (`check` and `synth`) — small
     extra complexity, but the architectural payoff is high.

3. **Full dependent / refinement types.**
   - Pros: maximum expressivity.
   - Cons: out of scope for M2; the constitution's "static core"
     phrasing precludes refinements at this milestone. Rejected for
     M2; revisit at M5+.

4. **Nominal records with explicit subtyping.**
   - Pros: simpler implementation in some respects.
   - Cons: §2.2 explicitly chooses *no implicit subtyping* and
     §5.1 chooses structural fits. Rejected.

## Decision

Adopt **option 2**: a bidirectional type checker over the HIR with
the following structural guarantees:

### Type universe (`Ty`)

```
Ty := Bool
    | Int                                 -- machine integer; M2 single-width (i64)
    | Float                               -- f64
    | Imag                                -- complex; reserved at M2
    | Str
    | Bytes
    | None                                -- unit type; the value is `None`
    | Never                               -- bottom; result of `raise` / divergent calls
    | Tuple([Ty])                         -- positional, size-fixed
    | List(Ty)                            -- homogeneous; element type known
    | Set(Ty)                             -- homogeneous
    | Dict(Ty, Ty)                        -- homogeneous keys / values
    | Record({ field: Ty, ... })          -- structural; closed at M2
    | Fn([Param], return: Ty, captures)   -- ordered + named params, single return
    | Generic(GenericVar)                 -- a free type variable
    | ADT(AdtId, [Ty])                    -- nominal sum-of-products from class_def with traits
    | Alias(AliasId, [Ty])                -- transparent type-alias application
    | TypeVar(VarId)                      -- inference unknown; resolved by unification
```

**No `dyn`.** Trait objects are deferred to a later milestone behind
an explicit opt-in keyword.

**`Never` is a *bottom* type.** It is a subtype-of-everything for the
purpose of *flow* (a `raise` in one branch lets the other branch
determine the join), but value-level subtyping does **not** exist.
The only place `Never` appears in a non-divergence context is when
flow analysis discharges it during meet/join — the type checker
treats `Never` joined with `T` as `T`.

### Param shape

```
Param := Positional(Ty) | Named(name: String, Ty, default?: ConstExpr)
```

`captures` is currently a `Vec<CaptureSpec>` — symmetric to the HIR
placeholder; structurally the type system records *which DefIds the
function references that are not parameters or local bindings*. M2
does not enforce explicit `copy` / `ref` / `move` capture (§2.2);
that lands at M3.

### Records: structural, closed

A record type `{ x: Int, y: Int }` is **closed** in M2 — exactly
those fields, no more, no fewer. Two records unify iff they share
identical field-set names and per-field types unify pointwise.

**No row polymorphism in M2.** The full row-polymorphic record (e.g.
`{ x: Int, ... }`) is a deliberate non-goal — M2 ships with the
simpler "closed" form, and the door is left open for ADR-0007 to
introduce row variables if downstream evidence demands it.

### ADTs and exhaustive matching

ADTs arise from `class_def` (form 4). A class `Foo` declares a
nominal type. A class with a `base` clause introduces sum structure
where the base is the discriminator, and constructor classes are
the sum constituents. M2 supports the conservative reading: every
class is a constructor, and `match` over its parent base is checked
for exhaustiveness against the constructor set.

A `match` is **exhaustive** iff every constructor of the scrutinee's
ADT is covered by at least one arm whose pattern would match it
(taking guards conservatively as "may not match"). Exhaustiveness
*is* a type error in M2 (`TypeError::NonExhaustiveMatch`), not a
warning. The wildcard pattern `_` always discharges exhaustiveness.

For non-ADT scrutinees (built-in types like `Int`), exhaustiveness
requires a wildcard or a name-binding pattern; literal patterns
alone are non-exhaustive.

### Generics

`type Result[T] = Ok | Err` introduces a parametric alias. A
`fn map[T, U](xs: List[T], f: Fn(T) -> U) -> List[U]` introduces a
parametric function. Generic instantiation is by argument-driven
unification at the call site.

M2's generic system is **prenex** (all type parameters at the
outermost binder). Higher-rank generics are deferred.

### Inference algorithm: bidirectional

Two judgments:

```
Γ |- e ⇒ τ        -- synthesis: under context Γ, expression e synthesizes τ
Γ |- e ⇐ τ        -- checking:  under context Γ, expression e checks against τ
```

Rules (selected — full set in §"Selected typing rules" below):

- A `let x: T = e` *checks* `e ⇐ T` and binds `x : T`.
- A `let x = e` *synthesizes* `e ⇒ T` and binds `x : T`.
- A `Call(f, args)` synthesizes by first synthesizing `f ⇒ Fn(...)`
  then *checking* each argument against the corresponding parameter
  type.
- A `Lambda(p, e)` checks against `Fn(P, R, _)` by binding `p : P`
  and checking `e ⇐ R`. With no expected type, the lambda is
  *not synthesizable* and the user must annotate (consistent with
  bidirectional folklore).
- A literal `42` synthesizes `Int`; `"x"` synthesizes `Str`; `True`
  / `False` synthesize `Bool`; `None` synthesizes `None`.
- An `if e: a else: b` checks against `T` by checking each branch
  against `T`. It synthesizes `T` only when both branches synthesize
  the same `T` (after `Never`-discharge).

Unification is **first-order with occurs-check**. The checker
maintains a substitution map; on completion, all `TypeVar`s should
be resolved or the program is rejected with
`TypeError::AmbiguousType`.

### `if x` and the implicit-truthiness rule

§2.2 forbids implicit truthiness. The type checker enforces:

- `if x:` requires `x : Bool` (or `x` is the result of a call
  whose return type is `Bool`).
- `while x:` requires `x : Bool`.
- `not x` requires `x : Bool`.
- `x and y`, `x or y` require both operands `: Bool` and yield
  `Bool`.

If `x : Option[T]`, the user must write `if x.is_some()` or pattern-match.
If `x : List[T]`, the user must write `if x.is_empty().not()` or
`if !x.is_empty()`. The error category is
`TypeError::ImplicitTruthiness`.

### `is` is removed

`is` cannot reach the type checker — the parser rejects it (ADR-0003)
and the lowering rejects it again (ADR-0005). If it nonetheless
appears (e.g. via a future macro that emits HIR directly), the type
checker rejects with `TypeError::UseOfDroppedFeature("is")`.

### Mutable default arguments

Already syntactically rejected (parser:
`ParseError::NonLiteralDefault`). The type checker enforces the
semantic rule once more: any default whose type is mutable
(`List`, `Set`, `Dict`) is `TypeError::MutableDefault`.

### Selected typing rules

Below `Γ` is the typing context, `Σ` the substitution.

```
                              Γ(x) = τ
                              ─────────── (T-Var)
                              Γ ⊢ x ⇒ τ

       Γ ⊢ e₁ ⇒ Fn([P₁..Pn], R, _)        Γ ⊢ eᵢ ⇐ Pᵢ for i ∈ 1..n
       ──────────────────────────────────────────────────────────── (T-Call)
                          Γ ⊢ e₁(e₂..eₙ) ⇒ R

         Γ, x : τ ⊢ b ⇒ σ        Γ, x : τ ⊢ b ⇐ σ' (when annotated)
         ──────────────────  ────────────────────────────────────── (T-Let)
         Γ ⊢ let x = b in e   Γ ⊢ let x : τ = b in e

       Γ ⊢ s ⇒ ADT(A, [τ])    ∀ ctors c of A. ∃ arm matching c
       ──────────────────────────────────────────────────────── (T-Match-Exh)
                              Γ ⊢ match s ⇒ τ_arms

                       Γ ⊢ e ⇒ Bool
                       ─────────────── (T-If-Cond)
                       Γ ⊢ if e ⇒ τ_branches

         Γ ⊢ e ⇒ τ    τ ≠ Bool
         ───────────────────────────── (T-If-Cond-Reject)
         Γ ⊢ if e  ⇒  ImplicitTruthiness
```

The full rule set lives next to the implementation
(`crates/cobrust-types/src/check.rs` row docstrings); this ADR pins
the *shape* and the *non-negotiables*.

### Error taxonomy (`TypeError`)

The complete enumeration shipped at M2:

- `UnknownName { name, span }` — name use that lowering didn't
  resolve (defense-in-depth; lowering catches first).
- `ArityMismatch { expected, actual, span }` — call has wrong number
  of positional args.
- `KeywordArgMismatch { name, span }` — call passes a keyword the
  callee does not accept.
- `MissingArgument { name, span }` — call omits a required argument.
- `TypeMismatch { expected, actual, span }` — expected and actual
  types don't unify.
- `NonExhaustiveMatch { uncovered: Vec<String>, span }` — `match`
  doesn't cover all constructors and has no wildcard.
- `RowConflict { field, ty1, ty2, span }` — record types disagree
  on the type of a shared field (placeholder for M2.5 row work; M2
  reports as `TypeMismatch` from inside the record-unification step,
  but keeps the variant for forward-compatibility).
- `ImplicitTruthiness { actual, span }` — non-Bool used in a
  truthiness position.
- `UseOfDroppedFeature { name, span }` — defense-in-depth.
- `MutableDefault { span }` — semantic re-check.
- `AmbiguousType { span }` — inference left a `TypeVar` un-resolved.
- `DuplicateField { name, span }` — record literal lists the same
  field twice.
- `OccursCheck { var, ty, span }` — unification would build an
  infinite type.
- `NotCallable { actual, span }` — call target's synthesized type is
  not a function type.
- `NotIndexable { actual, span }` — indexing a non-indexable.
- `NotIterable { actual, span }` — `for x in e` where `e` does not
  synthesize an iterable.
- `BreakOutsideLoop { span }` / `ContinueOutsideLoop { span }` —
  flow misuse.
- `ReturnOutsideFn { span }` — flow misuse.
- `YieldOutsideFn { span }` — flow misuse.

The variant list is the public taxonomy; downstream tooling can
match on it.

### Soundness proof obligation list

Per §5.2 ("scientific"), this ADR enumerates the obligations whose
discharge constitutes a soundness proof for the static core. The
proofs themselves are tracked as a future finding
(`find:type-soundness-proof`); the *enumeration* is fixed by this
ADR.

1. **Progress**: a well-typed expression is either a value or
   reduces under M2's small-step semantics. (Small-step semantics
   landing alongside MIR; the obligation is enumerated now.)
2. **Preservation**: reduction preserves types.
3. **Lowering preservation**: the AST→HIR lowering preserves the
   intended denotation. (This is what justifies type-checking the
   HIR rather than the AST.)
4. **Decidability of inference**: bidirectional inference for the
   M2 fragment terminates on every well-formed HIR (no implicit
   subtyping, no higher-rank polymorphism — both are deliberately
   excluded to keep this trivial).
5. **Pattern exhaustiveness completeness**: the exhaustiveness
   checker accepts iff every value of the scrutinee's type is
   covered.
6. **Implicit-truthiness rejection completeness**: the type checker
   rejects every program where a non-`Bool` reaches a truthiness
   position.
7. **`is` non-occurrence**: no `is` operator can appear in any
   well-typed program (defended by parser, by lowering, and by the
   type checker — three layers).
8. **Mutable-default rejection**: no well-typed program has a
   parameter default whose type is one of the M2-mutable types
   (`List`, `Set`, `Dict`).
9. **No silent coercion**: for every operator, both operands have
   types that **exactly** unify (no implicit `Int → Float`, no
   implicit `Bool → Int`). A user-written `to_float(x)` is the only
   path.

The proof itself can be discharged via mechanization (Coq, Lean,
Rocq) at a future milestone. The constitution does **not** require
the proof at M2; it requires the *enumeration* to be authoritative
so that the proof's scope is locked.

## Consequences

- **Positive**
  - The type system shape is fully pinned. Future contributors do
    not have to re-derive what M2 includes / excludes.
  - Bidirectional inference gives precise diagnostics — the
    "expected type" comes from the user's annotation, not the
    checker's guesswork.
  - The error taxonomy is enumerable, finite, and stable for
    downstream tooling.
  - The proof obligation list pre-commits the static-core soundness
    target without forcing the proof to ship at M2.

- **Negative**
  - No row polymorphism in M2 — this will be felt when users want
    "any record with at least these fields." The cost is paid in
    annotations.
  - No subtyping means `Never` flows are encoded in the *meet*, not
    in a subtyping rule — slightly less Haskell-clean, but
    explicit.
  - Bidirectional inference requires top-level type annotations on
    most function definitions — accepted as a design tax for
    diagnostics quality.

- **Neutral / unknown**
  - Whether `Imag` (the complex-imaginary literal) makes it into M2
    typing rules: literals are accepted at parse time, but the type
    is reserved as a future numeric ADR. Calls / arithmetic on
    `Imag` are `TypeError::TypeMismatch` at M2.
  - Whether closure capture is tracked at M2 or deferred to M3:
    structurally the field exists; semantically M2 accepts any
    capture. M3 will tighten this in concert with the constitution's
    `copy` / `ref` / `move` capture rule.
  - Performance characteristics of the unifier on adversarial inputs
    — not benchmarked yet; tracked as a follow-up finding (the
    well-/ill-typed suite is the operational gate at M2).

## Evidence

- Constitution `CLAUDE.md` §2.2 (drops), §5.1 (elegance), §5.2
  (scientific — proof obligations enumerable), §7 (M2 done means).
- `adr:0003` — surface forms; the type system covers exactly these.
- `adr:0005` — HIR shape; the type system runs over this, not the AST.
- `crates/cobrust-types/src/{ty.rs, infer.rs, check.rs, error.rs}` —
  implementation pinned to this ADR.
- `crates/cobrust-types/tests/{well_typed.rs, ill_typed.rs}` —
  the curated suite, ≥ 50 well-typed and ≥ 50 ill-typed programs.
