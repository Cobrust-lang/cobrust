---
doc_kind: adr
adr_id: 0055d
parent_adr: 0055
title: "Phase H Tier-2 — `crates/cobrust-types/src/check.rs` cb port (bidirectional checker under arena form; LARGEST Phase H sub-sprint)"
status: proposed
date: 2026-05-18
last_verified_commit: 9bb3dbc
supersedes: []
superseded_by: []
relates_to: [adr:0055, adr:0055a, adr:0055b, adr:0055c, adr:0055e]
discovered_by: ADR-0055 §3.3 sub-ADR roster — Tier-2 wave-3 parallel batch (largest single sprint in Phase H)
ratification_path: in-session review per ADR-0050 §"Audit model — teammate-in-session"; ratifies on impl merge under Phase H Wave-3 dispatch
---

# ADR-0055d: `check.rs` cb port — bidirectional checker under arena form

## 1. Context

Phase H Tier-2 stage per ADR-0055 §3.3 sub-ADR roster (`check.rs` cb port, Tier-2, weeks 2-3). ADR-0055 §3.5 places this ADR in **Wave 3** (parallel with 0055c) after Wave 2 (Tier-1 0055a + 0055b) confirms the arena-form `TyArena` + `TypeError` surfaces are stable. Per ADR-0055 §3.3 closing sentence + §4 LOC table, this ADR ports `crates/cobrust-types/src/check.rs` — at HEAD `929cd4a` is **2402 LOC**, the **largest single-file port in Phase H** (~71% of Phase H's 3368 total LOC, ~3x the next-largest sub-ADR's 0055a 407 LOC).

`check.rs` contains:

- `TypedModule` — public output: `def_types: HashMap<u32, Ty>` + `hir: Module`.
- `pub fn check(module: &Module) -> Result<TypedModule, TypeError>` — top-level entry; constructs a `Ctx`, invokes `check_module`, finalizes via `subst.apply` over every `def_types` entry, surfaces `AmbiguousType` if any free var remains.
- `Ctx` — stateful struct holding `subst: Subst`, `vars: VarAllocator`, `def_types: HashMap<DefId, Ty>`, `alias_map: HashMap<String, Ty>`, `return_stack: Vec<Ty>`, `loop_depth: usize`, `poly_intrinsic_defs: HashSet<DefId>`.
- ~50 methods on `Ctx`: 4 lifecycle (`new`, `fresh_var`, `record_def`, `lookup_def`), 7 module-checking (`check_module`, `prebind_items`, `prebind_item`, `fn_signature_type`, `check_item`, `check_fn`, `check_class`), 4 block-checking (`check_block`, `check_stmt`, `check_loop`, `iter_element`), 1 match (`check_match`), 2 exhaustiveness (`is_exhaustive`, `uncovered_set`), `synth_expr` (the giant — ~329 LOC), 5 method-table tries (`try_synth_{dict,str,list,float,int}_method`), `try_synth_method_call` chain, `synth_call`, `unify_call_arg` (ADR-0052a one-way `Ref(T) → T` coercion), `synth_bin`, `synth_un`, `synth_comp`, `lit_type`, `lower_default_type`, `validate_hashable_dict`, `lower_type`, `lower_named_type`, `lower_generic_type`, `lookup_resolved`, `instantiate_list_polymorphic`, `expect_bool`, `bind_pattern`.
- ~10 free functions: `is_copy_primitive`, `lit_to_string`, `_dummy`, `{str,list,float,int}_method_suggestion`, `is_list_polymorphic_intrinsic_name`, `literal_int_value`, `resolve_tuple_index`.
- `BlockOutcome` enum + `BlockOutcome::join` helper.

57 distinct `TypeError::*` construction sites across the file (per grep on HEAD `929cd4a`); every construction site carries a `suggestion: Option<&'static str>` field per ADR-0052b §2 Direction B.

This file is the deepest port surface in Phase H: it consumes 0055a (`TyArena` + `TyEntry`), 0055b (`TypeError` + `lib` re-exports), AND 0055c (`Subst` + `unify` + `finalize`). It produces no further downstream surface within Phase H (LSP / JIT / LLVM at Phase I/J/K are out-of-scope per ADR-0054 §11). Per ADR-0055 §8.2 "0055d is the largest single sub-sprint in project history" — P9 design spike for 0055d alone may run multi-day before P10-direct PAIR dispatch.

The §2.5 §B training-data-overlap binding is salient at every method: `synth_expr`'s ~19-arm match over `ExprKind` is the canonical "bidirectional type-checker over HIR" surface. Every future Cobrust translation of a checker (Phase J LSP completion ranking; ADR-0054 §11 HIR-shape borrow-checker; potential MIR-borrow at Phase L) inherits the patterns this ADR ratifies.

## 2. Decision

**Port `check.rs` to `crates/cobrust-types-cb/src/check.cb`** under the arena-form workaround from ADR-0055a §3, consuming arena-aware `Subst` + `unify` + `finalize` from 0055c. The Rust impl at `crates/cobrust-types/src/check.rs` stays canonical per ADR-0055 §3.1; the cb mirror is a **proof artifact** verified diff-empty by the ADR-0055e parity harness on the full M2 well-typed + ill-typed corpus modulo arena-id renaming.

Concretely, the cb port surface is:

- `TypedModule` — `struct TypedModule { def_types: dict[u32, i64], hir: Module }` (TyId-as-i64 value per 0055a §"Decision" alias convention). `hir: Module` reuses the Rust frontend HIR via FFI surface per ADR-0055 §3.1 (frontend stays Rust).
- `pub fn check(arena: &mut TyArena, module: &Module) -> Result[TypedModule, TypeError]` — top-level entry. Threads `&mut TyArena` everywhere (per 0055c §"Decision" receiver convention). Builds `Ctx`, calls `check_module`, finalizes every `def_types` entry via `subst_apply(s, arena, t)`, surfaces `AmbiguousType` on leaked free vars.
- `Ctx` — `struct Ctx { subst: Subst, vars: VarAllocator, def_types: dict[DefId, i64], alias_map: dict[str, i64], return_stack: list[i64], loop_depth: i64, poly_intrinsic_defs: set[DefId] }`. Note: `Ty` payloads everywhere become `i64` arena handles per 0055a convention. `HashMap` → cb `dict[K, V]` (insertion-order per ADR-0050d). `HashSet` → cb `set[T]`. `Vec` → cb `list[T]`.
- ~50 methods ported as free functions with `&mut Ctx` + `&mut TyArena` receiver pair (per ADR-0055 §4.1 "User-defined traits NOT shipped"; method-call sugar via ADR-0052d Phase G method-form makes the call sites read naturally — `ctx.check_module(arena, m)` becomes `check_module(&mut ctx, arena, m)` or `ctx.check_module(arena, m)` per the sugar).
- ~10 free functions ported 1:1 (most are pure predicates / suggestion-string lookups; no arena threading needed).
- `BlockOutcome` enum + `BlockOutcome::join` helper ported as-is (no arena state).

## 3. Cb-surface consumption from Tier-1 + 0055c

This ADR consumes the **largest cross-ADR surface in Phase H**:

- **From 0055a** — `TyArena`, `TyEntry`, `TyId` (i64 alias), `VarId` (i64 alias), `AdtId` / `AliasId` (i64 alias), helpers `lookup`, `insert`, `clone_into_arena`, arena-aware `free_vars` / `is_hashable` / `is_mutable_container` / `subst_var`, `Record` (with parallel `RecordArena`), `FnTy` (with parallel `FnTyArena`), `VarAllocator` (instance-field counter form). `display_ty` is invoked indirectly via `TypeError::*` payload display (0055b scope).
- **From 0055b** — `TypeError` enum (all 25 variants; this ADR emits **every variant** at one construction site or another). Specifically: `TypeMismatch` (synth_bin / synth_un / unify_call_arg cascade), `RowConflict` (synth_expr Record arms), `ArityMismatch` (every method-table try_synth + synth_call), `OccursCheck` (propagated from unify), `KeywordArgMismatch` (synth_call), `UnknownName` (lookup_resolved), `MissingArgument` (synth_call), `DuplicateField` (synth_expr Record literal), `UseOfDroppedFeature` (parser-level defense), `NonExhaustiveMatch` (check_match), `BorrowOfNonPlace` (synth_expr Ref arm per ADR-0052a Wave-1), `UnknownMethod` (try_synth_*_method fall-through), `Multiple` (aggregation in match-arm + synth_comp), `BreakOutsideLoop` / `ContinueOutsideLoop` / `ReturnOutsideFn` / `YieldOutsideFn` (control-flow guards), `MutableDefault` (lower_default_type), `AmbiguousType` (top-level `check()` finalization), `DictSpreadNotSupported` (Dict literal Phase F.3 reject), `NotCallable` / `NotIndexable` / `NotIterable` / `NotHashable` (synth_expr fall-through arms), `ImplicitTruthiness` (expect_bool guard).
- **From 0055c** — `Subst` struct + free functions `subst_new`, `subst_get`, `subst_extend`, `subst_apply`, `subst_fully_resolved`, `unify`, `finalize`. Every `synth_*` arm in this ADR calls at least one of `unify` or `subst_apply`. The `&mut TyArena` receiver convention from 0055c §3 settles ownership across this ADR's ~50 methods.

`lib.cb` re-export consumption (per 0055b §4 invariant): this ADR's `use cobrust_types_cb::{TypeError, Ty, TyId, VarId, Subst, unify, finalize, ...}` line is the binding compile-time check on Tier-1 + 0055c surface stability — if 0055b's `lib.cb` `pub use` list drifts, this file fails to compile.

## 4. Arena interaction

Per ADR-0055e §3 + §6 BLOCK rules, all `Ty` outputs of this ADR's `check()` (every `def_types` value + every `TypeError` payload) go through arena-id canonicalization (TyId + AdtId + AliasId + FnTyId + RecordId — 5-namespace per 0055e amendment 2026-05-18) before diff. Three arena-interaction invariants the cb port MUST satisfy:

- **Single arena per `check()` invocation** — top-level `check()` constructs one `TyArena` (+ parallel `FnTyArena`, `RecordArena`) at the start of the invocation; `Ctx` and every method threaded by `&mut TyArena` consumes this single shared arena. `def_types: dict[u32, i64]` final values are handles into this arena. The parity harness diffs canonicalized arena outputs per 0055e §3; the cb impl's arena must be passable to the canonicalization algorithm without cross-arena coupling.
- **Sub-typing relations stay structural** — ADR-0006 §"Type universe" has no nominal subtyping. Every `synth_*` arm that conceptually compares "is this T equal to that T" delegates to `unify` (which delegates to structural arena equality + var resolution per 0055c §4). Drift risk: a future amendment that introduces row-polymorphic widening (per `prebind_item` `poly_intrinsic_defs` set) must thread through 0055c's `unify` consistently; M2 baseline preserves the structural form.
- **Occurs-check + unification arena-aware** — every `unify` call site in this ADR passes arena handles `i64` rather than value `Ty`. The 0055c §4 invariant ("arena cycle in unify") extends to this ADR's call sites: `synth_expr` for `ExprKind::List` builds a `TyEntry::List(head_id)` via `arena.insert(TyEntry::List(head_id))` where `head_id` is strictly less than the new entry's id. Same for `Tuple`, `Set`, `Ref`, `Dict`, `Record`, `Fn`, `Adt`, `Alias`.

The harness tolerance per 0055e §3 amendment covers TyId + AdtId + AliasId + FnTyId + RecordId. `VarId` (allocated by `VarAllocator::fresh()` — instance-field counter per 0055a §"Decision") is an auxiliary canonicalization namespace per 0055e §3 closing paragraph; first-encounter ordering during traversal aligns Rust + cb outputs.

## 5. Per-fn complexity

`check.rs` decomposes into **~50 ported functions** + ~10 free helpers + 1 enum (`BlockOutcome`). Below is a complexity inventory of the load-bearing functions:

| Function | Rust LOC | cb-port complexity | Notes |
|---|---|---|---|
| `pub fn check` | ~22 | medium | Top-level entry; constructs Ctx + arena + finalizes; surfaces top-level `AmbiguousType` |
| `Ctx::check_module` | ~12 | trivial | prebind + iterate items + run `check_item` |
| `Ctx::prebind_items` + `prebind_item` | ~40 | medium | Decl-position type lowering; populates `def_types` + `poly_intrinsic_defs` |
| `Ctx::fn_signature_type` | ~50 | medium | Build `FnTy` arena entry from `FnBody`; threads `lower_type` |
| `Ctx::check_item` | ~38 | medium | Dispatches over `ItemKind` variants |
| `Ctx::check_fn` | ~85 | medium | Constructs function-scope; pushes `return_stack`; checks body; pops |
| `Ctx::check_class` | ~9 | trivial | Per ADR-0006 §"Class lowering" baseline |
| `Ctx::check_block` | ~8 | trivial | iterate stmts + join `BlockOutcome` |
| `Ctx::check_stmt` | ~126 | high | 12+ arms over `StmtKind`; `Return` / `Break` / `Continue` / `Yield` control-flow guards |
| `Ctx::check_loop` | ~41 | medium | Loop-depth bump; iter_element extraction; body check |
| `Ctx::iter_element` | ~47 | medium | Subst.apply on container type; dispatch over List/Set/Dict/Str/Iter ADTs |
| `Ctx::check_match` | ~44 | medium | Exhaustiveness check + arm-type joining; `NonExhaustiveMatch` construction |
| `Ctx::is_exhaustive` + `uncovered_set` | ~43 | medium | Per ADR-0006 §"Match exhaustiveness" admit/reject set |
| **`Ctx::synth_expr`** | **~329** | **high — the giant** | **~19 top-level `ExprKind` arms covering every expression form. Recursive descent; the dominant complexity in this file. Every arm emits 0-3 `TypeError::*` variants** |
| `Ctx::try_synth_dict_method` | ~130 | medium | 5 method arms (`keys`, `values`, `items`, `get`, `copy`) per ADR-0050d Decision 6A/7A/10A. Dispatches on `base_ty == Ty::Dict(k, v)` arena unpack |
| `Ctx::try_synth_str_method` | ~140 | medium | ~12 method arms per ADR-0050e Phase G method-form |
| `Ctx::try_synth_list_method` | ~110 | medium | ~8 method arms (`append`, `extend`, `pop`, `sort`, `reverse`, `len`, `count`, `index`) |
| `Ctx::try_synth_float_method` | ~60 | medium | ~4 method arms (`is_integer`, `as_integer_ratio`, etc.) |
| `Ctx::try_synth_int_method` | ~75 | medium | ~5 method arms (`bit_length`, `bit_count`, etc.) |
| `Ctx::try_synth_method_call` | ~22 | trivial | Chains 5 method-table tries; returns first `Some(t)` |
| **`Ctx::synth_call`** | **~143** | **high** | **Generic callable handling; binds positional + named args via `unify_call_arg` (ADR-0052a one-way `Ref(T) → T` coercion lives here); emits `ArityMismatch` / `MissingArgument` / `KeywordArgMismatch` / `NotCallable`** |
| `Ctx::unify_call_arg` | ~13 | trivial | ADR-0052a Wave-1 one-way `Ref(T) → T` coercion at the call-arg boundary; per `check.rs::Ctx::unify_call_arg` doc-comment |
| `Ctx::synth_bin` | ~56 | medium | Per-op dispatch (Add/Sub/.../Lt/Le/...); unifies operands; emits `TypeMismatch` on non-numeric/non-str operand |
| `Ctx::synth_un` | ~28 | trivial | Unary op dispatch (`+`, `-`, `~`, `not`) |
| `Ctx::synth_comp` | ~53 | medium | Comprehension scope; iter target binding; element synthesis; ADR-0006 §"Comprehensions" arity |
| `Ctx::lit_type` | ~12 | trivial | Lit → atomic Ty dispatch |
| `Ctx::lower_default_type` | ~17 | trivial | `MutableDefault` detection per ADR-0006 §"Mutable defaults rejected" |
| `Ctx::validate_hashable_dict` | ~44 | medium | Recurses through `HirType` to validate dict key hashability; emits `NotHashable` |
| `Ctx::lower_type` | ~37 | medium | `HirType` → arena `TyEntry` via `lower_named_type` / `lower_generic_type` |
| `Ctx::lower_named_type` | ~30 | trivial | String-keyed lookup; primitives + `alias_map` |
| `Ctx::lower_generic_type` | ~28 | trivial | `list[T]` / `set[T]` / `dict[K, V]` / `tuple[...]` dispatch |
| `Ctx::lookup_resolved` | ~26 | medium | `ResolvedName` → arena handle via `def_types` map; emits `UnknownName` |
| `Ctx::instantiate_list_polymorphic` | ~39 | medium | Fresh-var instantiation per ADR-0050c §F5 row-polymorphic widening |
| `Ctx::expect_bool` | ~13 | trivial | `ImplicitTruthiness` guard per ADR-0006 §"Implicit truthy/falsy" forbidden |
| `Ctx::bind_pattern` | ~102 | high | Pattern-match binding; recursive descent over `PatternKind`; emits `RowConflict` on field overlap, `Multiple` on aggregated errors |
| `BlockOutcome` enum + `join` | ~24 | trivial | Reachability join for `check_block` |
| Free functions (`is_copy_primitive`, `lit_to_string`, suggestion-tables, `is_list_polymorphic_intrinsic_name`, `literal_int_value`, `resolve_tuple_index`) | ~150 | trivial | Pure predicates / lookup tables; no arena threading |

**Total**: ~50 ported functions + ~10 free helpers; **57 distinct `TypeError::*` construction sites** that each must round-trip through 0055b's enum + display surface + 0055e's harness canonicalization.

### 5.1 The giant: `synth_expr`

`Ctx::synth_expr` is **~329 LOC** of recursive descent over `ExprKind` with ~19 top-level arms. Each arm handles one expression form and emits 0-3 `TypeError::*` variants. Per ADR-0055 §8.2, this single function is the dominant complexity in 0055d.

The ~19 top-level arms (per `check.rs::Ctx::synth_expr`; Lit, Format, Name, Tuple, List, Set, Dict, Comp, Lambda, Call, Attr, Index, Bin, Un, Borrow, Await, Yield, YieldFrom, Cast):

- Atomic literals: `Lit` (delegates to `lit_type`), `Name` (delegates to `lookup_resolved`), `Format` (recurses through `FormatPart::Hole`).
- Collection literals: `Tuple`, `List`, `Set`, `Dict` (with `DictEntry::Pair` + `DictEntry::Spread` rejection per ADR-0050d Phase F.3).
- Index / attribute: `Index` (subscript via arena unpack), `Attr` (defers to `try_synth_method_call` for method-dispatch, else fresh-var fallthrough per M2 conservative).
- Calls: `Call` (delegates to `synth_call`; method-form via `try_synth_method_call` short-circuit).
- Borrows: `Ref` (ADR-0052a Wave-1 place-only borrow; emits `BorrowOfNonPlace` on non-place expr).
- Control-flow expr: `Yield` (emits `YieldOutsideFn`), `Lambda` (FnTy construction).
- Binary / unary: `Bin` (delegates to `synth_bin`), `Un` (delegates to `synth_un`), `Comp` (delegates to `synth_comp`).
- Other: `If` (ternary expr), `Match` (delegates to `check_match`), `Star` (parser-only; unreachable).

Each compound arm performs arena inserts (per §4 invariant 1). The recursive-descent pattern is mechanically the same shape as the Rust impl's `match &e.kind { ... }`; cb port substitutes `Ty::*` constructions with `arena.insert(TyEntry::*)` calls.

### 5.2 Cross-cutting concerns

Three cross-cutting concerns thread through every method:

- **Span propagation** — every error construction passes `span` per `error.rs::TypeError` field. Spans flow from `Expr::span` / `Stmt::span` / explicit args. Cb port preserves Rust `Span` value via FFI per ADR-0055 §3.1 (frontend stays Rust); spans are byte-equal not canonicalized per ADR-0055e §3 closing paragraph.
- **Suggestion field round-trip** — every TypeError construction populates `suggestion: Some("...")` per ADR-0052b §2 Direction B. Cb port emits the same literal text byte-for-byte; static-vs-owned distinction is impl-internal per 0055b §6 risk 1.
- **Method-table dispatch parity** — 5 method tables (Dict / Str / List / Float / Int) each contain ~5-12 method arms. Total ~30 method arms must each round-trip diff-empty. Per ADR-0052d-prereq method-call sugar, the dispatch order is Dict → Str → List → Float → Int (per `check.rs::Ctx::try_synth_method_call` ordering); cb port preserves this order exactly so the `UnknownMethod` fallthrough produces an identical diagnostic.

### 5.3 Stateful `Ctx` lifecycle

The cb port's `Ctx` retains the 7 stateful fields from Rust impl:

- `subst: Subst` (from 0055c) — running substitution; mutated by every `unify` call.
- `vars: VarAllocator` — per 0055a §"Decision" instance-field counter form.
- `def_types: dict[DefId, i64]` — every binding's arena-handle type. Populated at `prebind_item` time; resolved at `check()` top.
- `alias_map: dict[str, i64]` — type-alias name → arena handle; populated at `lower_type` of `Alias` arms.
- `return_stack: list[i64]` — function-return-type stack; pushed at `check_fn` entry, popped at exit.
- `loop_depth: i64` — incremented at `check_loop` entry; gates `Break` / `Continue` legality.
- `poly_intrinsic_defs: set[DefId]` — ADR-0050c §F5 row-polymorphic intrinsics; populated at `prebind_item` by name-match against `is_list_polymorphic_intrinsic_name`.

Lifecycle invariants: every `push` on `return_stack` is paired with a `pop` at function-scope close (same shape as Rust impl's `Vec::push` / `Vec::pop` pair). `loop_depth` decrements on loop-scope close. State leaking across function boundaries would produce parity divergences; harness corpus exercises nested function + loop scopes to catch state-leak bugs.

### 5.4 Lowering subsystem

`lower_type` + `lower_named_type` + `lower_generic_type` form a sub-component that translates `HirType` (parser/resolver output) into arena-form `TyEntry`. Per `check.rs::Ctx::lower_type`, the dispatch is:

- Atomic named types (`int`, `float`, `str`, `bytes`, `bool`, `None`, `complex`, `Any`, `Never`) → corresponding arena-form atomic `TyEntry` variant.
- Generic forms (`list[T]`, `set[T]`, `dict[K, V]`, `tuple[...]`, `Optional[T]`) → arena insert with recursively-lowered child arena handles.
- Aliases (`MyAlias`) → `alias_map` lookup; insert `TyEntry::Alias(alias_id, args)` if matched, else `lookup_resolved` path.

Cb port preserves this 1:1; arena inserts produced by the lowering subsystem participate in the same 5-namespace canonicalization per ADR-0055e §3 amendment.

## 6. Risk register

Top 3 risks ranked by impl-blast-radius:

1. **`synth_expr` depth + control-flow flatten cost** — at ~329 LOC with ~19 arms, `synth_expr` is the **largest single function in Phase H** and likely the largest contiguous match-statement in the cb codebase. The cb port faces two correctness risks: (a) **arm-order drift**: cb may reorder the match arms (cosmetic refactor during port) introducing a subtle semantic shift where a more-specific arm gets shadowed by a more-general one (e.g., `Attr` arm vs `Call(callee: Attr)` arm; the latter must come first for method-form dispatch). (b) **control-flow flattening**: nested `if let` / `match` ladders in the Rust impl may be flattened to early-return in the cb port; subtle behavior differences (e.g., where the `subst.apply` lookup happens relative to the error emission) emerge as parity-harness divergences. Mitigation: cb port retains Rust arm-order verbatim; harness Phase 2 sanity stage includes "synth_expr round-trip on every M2 corpus input" property test; canonicalization (0055e §3) ensures arena-handle differences canonicalize but logical-type differences surface as BLOCK per 0055e §6.

2. **Method-table dispatch parity (5 tables = ~30 method arms)** — Dict (5 arms) + Str (~12) + List (~8) + Float (~4) + Int (~5) = ~34 method arms. Each emits 0-3 `TypeError::*` variants per arm. The fallthrough behavior (return `Ok(None)` to chain to next table) is delicate: if cb port returns `Ok(None)` where Rust returns `Err(...)` (or vice versa) on an edge case (unknown method name on an ambiguous receiver type), parity fails per 0055e §6 BLOCK rule. Mitigation: each method-table function in the cb port follows the exact pattern from `check.rs::Ctx::try_synth_*_method`: guard `let Ty::* = base_resolved else { return Ok(None) }`, then per-method-name dispatch with `_ => Ok(None)` fall-through. Harness Phase 2 stage extends corpus with "every method-table fallthrough" coverage: one input per method-table that exercises (a) known method on correct receiver, (b) known method on wrong receiver, (c) unknown method on correct receiver, (d) ambiguous receiver. ~120 corpus entries cover all 5 tables × 4 cases × ~6 arms.

3. **Cross-cutting `suggestion` field round-trip per 0052b** — 57 `TypeError::*` construction sites in `check.rs` each populate `suggestion: Option<&'static str>` per ADR-0052b §2 Direction B. Cb port must emit byte-identical literal text at every site per 0055e §6 BLOCK rule 4 ("`suggestion` field divergence → BLOCK"). Subtle drift risks: (a) **literal-text divergence**: cb impl might emit "change to 'if x != 0:'" vs Rust's "change to `if x != 0:`" (backtick vs quote glyph). (b) **None vs Some**: cb impl might emit `None` where Rust emits `Some(...)` on a refactored construction site. (c) **per-error-class drift**: 6 distinct suggestion-suffix patterns across the 57 sites; cb impl might unify two patterns inadvertently. Mitigation: cb port follows Rust source line-for-line for every `suggestion: Some(...)` field; harness Phase 2 stage includes "suggestion-text byte-equal on every TypeError variant" property test (extends 0055b §6 risk 2 mitigation). The 0055b `display_error` byte-parity test transitively covers this via the harness's per-input granularity.

### Deferred concerns

- **`def_types: dict[u32, Ty>` final value materialization** — Rust impl materializes `def_types: HashMap<u32, Ty>` (resolved `Ty` values) at `check()` top via `subst.apply(t)` per entry. Cb port: `def_types: dict[u32, i64]` (arena handles). Consumer of `TypedModule.def_types` in the Phase J LSP path may want resolved `TyEntry` values not handles; that's a future LSP-port concern (ADR-0055 §11 out-of-scope for Phase H). Phase H harness compares canonicalized arena outputs; handle-vs-value distinction is invisible to harness.
- **`alias_map: dict[str, i64]` warm-up ordering** — Rust `HashMap` has non-deterministic iteration order; Cobrust `dict` preserves insertion order. For `lookup_resolved` and `lower_named_type` (which do point lookups), ordering is irrelevant. For diagnostic surface (e.g., "did you mean ...?" suggestion that iterates the alias_map keys), ordering might surface; M2 baseline has no such surface, so out-of-scope.
- **`return_stack: list[i64]` arena interaction** — `check_fn` pushes the return-type arena handle; `check_stmt::Return` arm `unify`s the expression type against the top-of-stack handle. Pop happens at function-scope close. Cb port preserves this exactly; no arena-stale-handle risk because the pushed handle stays valid for the function scope lifetime (arena append-only per §4 invariant).

## 7. Pre-dispatch gate

Required before this ADR's P9 design spike + P10-direct PAIR dispatches:

- [ ] **ADR-0055a merged** — Tier-1 `ty.rs` cb port + `TyArena` + arena utilities stable.
- [ ] **ADR-0055b merged** — Tier-1 `error.rs` + `lib.rs` cb port + `TypeError` enum + `display_error` + 25-variant compliance matrix stable.
- [ ] **ADR-0055c merged** — Tier-2 `infer.rs` cb port + arena-aware `Subst` + `unify` + `finalize` stable. Receiver convention (`&mut TyArena`) ratified in 0055c §3.
- [ ] **ADR-0055e accepted (Phase 2 sanity baseline merged)** — Rust-vs-Rust diff-empty on M2 corpus confirmed; 5-namespace canonicalization in-effect per 0055e amendment 2026-05-18.
- [ ] **Arena re-evaluation gate** — ADR-0055 §5 closing paragraph mandates Tier-2 dispatch revisit the arena disposition. Default: holds. Confirm Tier-1 + 0055c impl experience did not surface arena cost.

No dependency on Phase 7.5 (recursive struct types) per ADR-0055 §3.2.

## 8. Cross-ADR coordination

- **Fed by 0055a** — `TyArena` + `TyEntry` + arena-aware utilities (`free_vars`, `is_hashable`, `is_mutable_container`, `subst_var`, `clone_into_arena`). This ADR threads `&mut TyArena` through every `Ctx`-method per 0055a §"Decision" receiver convention.
- **Fed by 0055b** — `TypeError` enum (all 25 variants surfaced here). `lib.cb` re-export contract (per 0055b §4 invariant) is the binding compile-time check on Tier-1 surface stability.
- **Fed by 0055c** — `Subst` + `unify` + `finalize`. Every `synth_*` arm calls at least one. The 0055c §3 receiver-convention is inherited.
- **Consumed by ADR-0055e** — parity harness diff-tests both impls' `check()` outputs on the full M2 corpus. Per-input granularity (0055e §2) localizes divergences; canonicalization (0055e §3) tolerates arena-id renaming; per-variant BLOCK rules (0055e §6) catch any divergence in accept/reject, variant name, Span, suggestion, or canonical Ty.
- **Inherits from ADR-0006** — bidirectional type-checking rules pinned by ADR-0006 §"Selected typing rules". This ADR ports the checker under arena form without semantic divergence.
- **Inherits from ADR-0050d** — Dict-as-`dict[K, V]` + Decision 6A/7A/10A method-table. The `try_synth_dict_method` 5 arms (`keys` / `values` / `items` / `get` / `copy`) preserve ADR-0050d §"Surface coverage matrix" semantics exactly.
- **Inherits from ADR-0052a Wave-1** — `Ty::Ref(Box<Ty>)` ports under arena as `Ref(i64)` per 0055a §3 table. The one-way `Ref(T) → T` coercion at `unify_call_arg` (per `check.rs::Ctx::unify_call_arg`) preserves Wave-1 semantics; the `BorrowOfNonPlace` rejection in `synth_expr` Ref arm enforces place-only borrow per Wave-1 §6.
- **Inherits from ADR-0052b** — every `TypeError::*` construction populates `suggestion` per Direction B. Cb port preserves 57 sites byte-for-byte.
- **Inherits from ADR-0052d** (and `0052d-prereq`) — method-call sugar via per-type method tables. The `try_synth_method_call` dispatch chain order (Dict → Str → List → Float → Int) per `check.rs::Ctx::try_synth_method_call` is preserved.

## 9. Wall

~1-2 weeks per ADR-0055 §3.5 Wave-3 budget (the dominant cost of Phase H — largest single-file port at 2402 LOC ≈ ~2800-3200 LOC cb under arena indirection per ADR-0055 §"Option B" ~10% LOC inflation).

- **TEST hours**: ~16-24 (`synth_expr` round-trip property tests on every M2 corpus input + 5-method-table fallthrough corpus extension (~120 entries per §6 risk 2) + suggestion-field byte-equal property test + 57-error-construction-site smoke test).
- **DEV hours**: ~60-100 (port 2402 LOC Rust to ~2800-3200 LOC cb; 57 error construction sites + ~19 `synth_expr` arms + 5 method tables + ~50 method ports). Per ADR-0055 §8.2 "P9 design spike for 0055d alone may run multi-day before P10-direct PAIR dispatch".
- **Host**: DG primary per ADR-0055 §9.1 row 6. Mode C (P10-direct PAIR). Heavy DG load expected — pre/postflight `/tmp/cobrust-*` cleanup per `feedback_heavy_build_offload_to_workstation.md` (235G temp leak incident).

## 10. Consequences

### 10.1 Positive

- Largest Tier-2 port lands; Phase H "done means" bar (ADR-0055 §3.1 closing sentence) becomes achievable — type-checker self-host operationally validated on full M2 corpus.
- 57 `TypeError::*` construction sites + ~30 method-table arms in cb form become the most concentrated §2.5 §B training-data-overlap surface in the project — every future cb-side checker port (LSP completion, MIR borrow-check) inherits these patterns.
- `synth_expr`'s ~19-arm match in cb form is the canonical "bidirectional checker over HIR" surface; LSP code-action providers (Phase J ADR-0057) consume the same pattern.
- The arena-handle threading convention codified in §2 + §3 + §4 closes the receiver-pattern question across the entire Phase H batch. Future self-host crates (HIR shape, MIR rvalues per ADR-0054 §11) inherit the convention without re-litigation.
- Phase H 6-sub-ADR roster completion sets precedent for milestone-layer batch ADRs spawning Wave-0 infra (0055e), Tier-1 wave-2 parallel (0055a + 0055b), and Tier-2 wave-3 parallel (0055c + 0055d). Phase J/K/L inherit.

### 10.2 Negative

- **Largest single sub-sprint in project history** (2402 LOC Rust → ~2800-3200 LOC cb). P9 design spike alone may run multi-day; P10-direct PAIR dispatch wall ~1-2 weeks. If Tier-1 + 0055c slippage compounds, Phase H closes late vs ADR-0055 §3.5's 2.5-3 week total budget.
- Arena indirection compounds at `synth_expr` recursive descent — each compound arm performs one extra `lookup(arena, id)` per arena handle vs Rust impl's value-clone shape. For pathologically-deep types, observable; M2 corpus negligible (per 0055c §6 risk 3 mitigation analysis).
- 57 `TypeError::*` construction sites + ~30 method-table arms = ~87 distinct error-or-method-return surfaces. Each is a potential parity divergence point. Phase 2 harness corpus extension (per §6 risk 2 mitigation: ~120 method-table entries) is the largest single corpus expansion in Phase H.
- The cb `def_types: dict[u32, i64]` final value materializes arena handles, not resolved `TyEntry` values. Consumers expecting Rust's `HashMap<u32, Ty>`-style materialization (e.g., a future Phase J LSP hover renderer) need to thread the arena alongside. Documented as arena-specific surface in cb-mirror agent docs.

### 10.3 Neutral / unknown

- Whether the cb impl uses method-call sugar (`ctx.synth_expr(arena, e)`) or free-function form (`synth_expr(&mut ctx, arena, e)`). Per ADR-0052d Phase G method-form, sugar reads more naturally; per ADR-0055 §4.1 "User-defined traits NOT shipped", true `impl Ctx` blocks unavailable. Default: free-function form with sugar at call sites per ADR-0052d post-Wave-2 surface.
- Whether `_dummy` (per `check.rs::_dummy`) and other test/debug-only functions port at all. Mark as `#[cfg(test)]`-equivalent in cb if Cobrust ships conditional compilation; otherwise omit (Phase H closure budget unaffected).
- Whether `BlockOutcome::join` (free associated fn on the enum) requires user-defined trait. Per ADR-0055 §4.1, no user-defined `impl` — port as free function `block_outcome_join(items: &list[BlockOutcome]) -> BlockOutcome`.

## 11. Dispatch readiness

- **TEST**: opus (D4 per `feedback_subagent_model_tier` — Tier-2 largest port; ~19-arm `synth_expr` correctness + 5 method-table parity + 57 error-construction-site round-trip all require strategic test-design + property-test thinking).
- **DEV**: opus (D4 — `synth_expr` ~19-arm match + 57 error sites + ~30 method-table arms + cross-cutting `suggestion` field round-trip all require §2.5 compile-time-catch discipline at every line; arm-order drift + control-flow flattening + literal-text divergence are subtle correctness risks that demand Opus tier).
- **Wall**: ~1-2 weeks per §9.
- **Post-author audit**: Tier-1 audit fires post-return BEFORE merge per `feedback_post_author_audit_mandatory`. Audit scope: §4 arena-interaction invariants compliance + §5 per-fn complexity table accuracy + §6 risk mitigation evidence in impl + cross-ADR `&mut TyArena` receiver convention consistent with 0055c + 57 `TypeError::*` construction-site byte-equality + 5 method-table fallthrough corpus coverage.

### 11.1 Symbol-anchor convention compliance (F34)

Per F34 ratified finding (`f34-pre-candidate-numeric-anchor-degradation-high-churn.md`), this ADR adopts symbol anchors throughout — `check.rs::Ctx::synth_expr`, `check.rs::Ctx::synth_call`, `check.rs::Ctx::unify_call_arg`, `check.rs::Ctx::try_synth_method_call`, `check.rs::Ctx::try_synth_dict_method`, etc. — instead of numeric `check.rs:NNN` references. `check.rs` is the highest-churn file in Phase G (60-80% growth) and continues to grow during Phase H translation prep; F34 sediment-decay mechanism (numeric anchors drift >100 lines within ~2 weeks) makes symbol anchors load-bearing for this ADR's longevity. Phase H 0055c + 0055d explicit adoption constitutes the second corroborator that promoted F34 from pre-candidate to ratified (2026-05-18).

### 11.2 Documentation mandate

Per ADR-0055 §9.2, this sub-ADR commit ships triple-doc updates (zh + en + agent) per constitution §3.3:

- `docs/human/{zh,en}/self-host.md` §"Bidirectional checker self-host" — new subsection covering arena-aware bidirectional checking pattern + ~19-arm `synth_expr` dispatch + 5-method-table fallthrough chain. Examples-before-abstractions per CLAUDE.md §3.1: show a worked `synth_expr(Call(f, args))` trace before describing the algorithm. Includes retrospective on §4.4 binding closure ("self-host operational after 9 weeks of deferral" per ADR-0055 §8.1).
- `docs/agent/modules/cobrust-types-cb.md` — extends ADR-0055c's module-overview with `module_id: cobrust-types-cb::check` subsection; cross-references stable per F34 symbol anchors.
- Bilingual sync rule per CLAUDE.md §3.3 — zh + en land in same commit as agent docs + impl + Phase H closure retrospective.

— P9 Tech Lead, 2026-05-18

## 12. Count accounting amendment (Tier-1 audit `af6d8ce1eb343127a` 2026-05-18)

**Trigger**: Tier-1 post-author audit of Phase H Wave-3 TEST corpus at branch HEAD `9bb3dbc` found agent-claim drift in test-count projections written during pre-dispatch planning. No test-corpus changes are required; this section corrects the ADR record only.

### 12.1 Actual counts at `9bb3dbc`

| File | Claimed | Actual |
|---|---|---|
| `crates/cobrust-types-cb/tests/check_parity_corpus.rs` (`#[ignore]` count) | 47 | **62** |
| `crates/cobrust-types-cb/tests/check_display_parity.rs` (`#[ignore]` count) | 18 | 18 (unchanged) |
| **Total parity corpus** | **65** | **80** |

### 12.2 Arm-coverage corrections

Audit enumerated per-arm test counts and found two arms with more tests than the planning claim:

- **Dict arm (arm 7 in `synth_expr`)**: 3 tests, not 2. The extra test covers `DictSpreadNotSupported` rejection path distinct from `DuplicateField`.
- **Call arm (arm 10 in `synth_expr`)**: 5 tests, not 2. The extra 3 tests cover: `NotCallable` on non-callable receiver, `KeywordArgMismatch` on positional-only target, `ArityMismatch` under consensus-mode LLM-translated variant.

### 12.3 Extra non-synth tests

The original planning claim of 14 extra (non-arm-pair) tests was understated. Actual count is **22**. The 8 additional extras cover:

- 4 tests for `BlockOutcome::join` reachability join (Added during free-function port; not enumerated in arm-pair breakdown).
- 2 tests for `Ctx` lifecycle state-leak (nested `check_fn` + `loop_depth` decrement).
- 2 tests for `lower_default_type` `MutableDefault` edge cases.

### 12.4 Arm 1 (Lit) coverage note

Audit confirmed: `Lit` arm corpus tests are PASS-only. There is no FAIL path for `Lit` because `lit_type` is total (every `LitKind` maps to a concrete `Ty`). This is correct behavior, not a coverage gap. The `check.cb` doc-ref is already accurate per audit.

### 12.5 No test-corpus changes

The `check_parity_corpus.rs` and `check_display_parity.rs` files are correct at `9bb3dbc`. This amendment updates the ADR record only; no re-dispatch or re-verification is required.
