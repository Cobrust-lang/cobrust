---
doc_kind: adr
adr_id: 0055c
parent_adr: 0055
title: "Phase H Tier-2 — `crates/cobrust-types/src/infer.rs` cb port (arena-aware `Subst` + `unify`)"
status: proposed
date: 2026-05-18
last_verified_commit: fd263f4
supersedes: []
superseded_by: []
relates_to: [adr:0055, adr:0055a, adr:0055b, adr:0055e]
discovered_by: ADR-0055 §3.3 sub-ADR roster — Tier-2 wave-3 parallel batch
ratification_path: in-session review per ADR-0050 §"Audit model — teammate-in-session"; ratifies on impl merge under Phase H Wave-3 dispatch
---

# ADR-0055c: `infer.rs` cb port — arena-aware `Subst` + `unify` + `finalize`

## 1. Context

Phase H Tier-2 stage per ADR-0055 §3.3 sub-ADR roster (`infer.rs` cb port, Tier-2, week 2 days 1-4). ADR-0055 §3.5 places this ADR in **Wave 3** (parallel with 0055d) after Wave 2 (Tier-1 0055a + 0055b) confirms the arena-form `TyArena` + `TypeError` surfaces are stable.

`crates/cobrust-types/src/infer.rs` at HEAD `929cd4a` is **259 LOC** containing the bidirectional inference engine consumed by `check.rs` synth/check rules:

- `Subst` — `HashMap<VarId, Ty>`-backed running substitution. Methods: `new`, `get`, `extend`, `apply` (recursive walk), `fully_resolved`.
- `unify(t1: &Ty, t2: &Ty, subst: &mut Subst, span: Span) -> Result<(), TypeError>` — the load-bearing function. Recurses through compound `Ty` variants (Tuple / List / Set / Ref / Dict / Record / Fn / Adt / Alias) emitting `TypeMismatch` / `ArityMismatch` / `OccursCheck` / `KeywordArgMismatch` on failure.
- `finalize(t: &Ty, subst: &Subst, span: Span) -> Result<Ty, TypeError>` — fully-applies the substitution; surfaces `AmbiguousType` if any free `Ty::Var` remains.

This file is the deepest port surface Tier-2 produces: it consumes 0055a's `TyArena` + arena-form `TyEntry` and 0055b's `TypeError` enum, and feeds 0055d's checker (every `synth_*` arm calls `unify` or `subst.apply`). Per ADR-0055 §3.5 Wave-3 budget, this ADR + 0055d combined run ~10-14 days; 0055c specifically lands the smaller of the two Tier-2 ports (259 LOC vs 0055d's 2402 LOC).

The §2.5 §B training-data-overlap binding is salient: `unify` is the canonical "structural equality with substitution" algorithm in HM-style inference. Every future Cobrust translation of an inference engine (Phase J LSP completion ranking; ADR-0054 §11 HIR-shape inference; potential MIR borrow-inference at Phase L) inherits the arena-walking pattern this ADR ratifies.

## 2. Decision

**Port `infer.rs` to `crates/cobrust-types-cb/src/infer.cb`** under the arena-form workaround from ADR-0055a §3. The Rust impl at `crates/cobrust-types/src/infer.rs` stays canonical per ADR-0055 §3.1; the cb mirror is a **proof artifact** verified diff-empty by the ADR-0055e parity harness on the M2 well-typed + ill-typed corpus modulo arena-id renaming.

Concretely, the cb port surface is:

- `Subst` — `struct Subst { map: dict[i64, i64] }` (VarId-as-i64 key → TyId arena-handle value, per 0055a §"Decision" alias convention). Methods mirror Rust 1:1: `subst_new()`, `subst_get(s: &Subst, v: i64) -> Option[i64]`, `subst_extend(s: &mut Subst, v: i64, t: i64)`, `subst_apply(s: &Subst, arena: &mut TyArena, t: i64) -> i64`, `subst_fully_resolved(s: &Subst, arena: &TyArena, t: i64) -> bool`.
- `unify(arena: &mut TyArena, t1: i64, t2: i64, subst: &mut Subst, span: Span) -> Result[(), TypeError]` — the load-bearing function. Receiver pattern is `(arena_handle1, arena_handle2)`; the body looks up `TyEntry` shapes via `arena.lookup(id)` before pattern-matching. Recursive calls thread `&mut TyArena` because compound-arm `subst_apply` may insert fresh entries (per 0055a §6 risk 1 "fresh-handle on substitution").
- `finalize(arena: &mut TyArena, t: i64, subst: &Subst, span: Span) -> Result[i64, TypeError]` — same shape; returns a fresh arena handle after applying the substitution.
- No user-defined trait `impl` (per ADR-0055 §4.1 "User-defined traits NOT shipped"); all functions are free functions parametric in `&TyArena` / `&mut TyArena`.

The receiver-convention codification ex-ante (`&mut TyArena` everywhere `subst_apply` or `unify` is invoked) settles the per-call-site arena-ownership question for 0055d.

## 3. Cb-surface consumption from Tier-1

Tier-2 sub-ADRs consume the surfaces Tier-1 ratified:

- **From 0055a** — `TyArena`, `TyEntry`, `TyId` (i64 alias), `VarId` (i64 alias), helper `lookup(arena, id) -> TyEntry`, `insert(arena, entry) -> i64`, `clone_into_arena` (used at `subst_apply` Tuple / List arms to produce fresh composite entries from renamed children). The `free_vars(arena, id) -> list[i64]` arena-aware utility (0055a §4 surface invariant) is invoked by `subst_fully_resolved` + `unify`'s `OccursCheck` arm.
- **From 0055b** — `TypeError` enum + `display_error(&TyArena, &TypeError) -> str`. The 6 `TypeError::*` variants this file emits (`TypeMismatch`, `ArityMismatch`, `OccursCheck`, `KeywordArgMismatch`, `AmbiguousType` — and through `synth_expr` propagation, every other variant) carry `i64` arena handles in their `Ty` payload fields per 0055b §3 table.

The arena-passing convention codified in 0055a §"Decision" extends here: `subst_apply` takes `&mut TyArena` because it inserts fresh composite entries (Tuple / List / Set / Ref / Dict / Record / Fn / Adt / Alias arms each produce a renamed-children composite that needs an arena slot). `unify` takes `&mut TyArena` because it calls `subst_apply` internally. `finalize` takes `&mut TyArena` for the same reason.

## 4. Arena interaction

Per ADR-0055e §3 + §6 BLOCK rules, all `Ty` outputs of `unify` + `finalize` go through arena-id canonicalization before diff. Three arena-interaction invariants the cb port MUST satisfy:

- **No alias mutation**: every `subst_apply` arm that produces a composite (Tuple, List, Set, Ref, Dict, Record, Fn, Adt, Alias) MUST insert a fresh `TyEntry` rather than mutating an existing arena slot. The 0055a §6 risk 1 "fresh-handle-is-always > all referenced handles" property test extends to this ADR; `subst_apply` outputs MUST respect the post-order arena-id ordering. The Rust impl preserves this by-value via `.clone()`-then-construct; the cb port preserves it by `insert(arena, new_entry)`.
- **Sub-typing via structural arena equality**: ADR-0006 §"Type universe" has no nominal subtyping; `unify` does structural equality + variable resolution. Under arena form, structural equality at compound arms (`(Ty::List(a), Ty::List(b)) => unify(&a, &b, ...)`) becomes `(TyEntry::List(a_id), TyEntry::List(b_id)) => unify(arena, a_id, b_id, ...)` — recursive arena-handle dereference. Tuple-arity mismatch + arity check before recursion preserved.
- **Occurs-check arena walk**: Rust impl computes `other.free_vars()` (returns `Vec<VarId>`) then checks `.contains(&v)`. Cb port calls `free_vars(arena, other_id) -> list[i64]` (0055a §4 surface invariant) then `.contains(v)`. The free-vars walk descends arena handles transitively per 0055a §6 risk 1 mitigation; cycle detection unnecessary per ADR-0055 §5 (Phase H types tree-shaped).

The harness tolerance per 0055e §3 amendment (5-namespace canonical post-order) covers TyId + AdtId + AliasId + FnTyId + RecordId; this ADR's `subst_apply` walks each namespace identically (`Fn(fn_id)` recursing into FnTyArena, `Record(rec_id)` into RecordArena). VarId is an auxiliary namespace (per 0055e §3 closing paragraph) and canonicalizes in first-encounter order during traversal of the `Subst.map` keys.

### 4.1 Cross-namespace handle discipline

The 5-namespace canonicalization imposes a per-arena discipline on this ADR's `subst_apply`:

- **TyArena**: every composite `TyEntry::*` variant inserts produces a new TyId; child handles within the entry refer to other TyIds in the same arena.
- **FnTyArena**: the `Fn(i64)` payload is a FnTyId that the cb mirror's `FnTy` struct stores. `subst_apply` for `TyEntry::Fn(fn_id)` looks up the FnTy entry, recurses through its positional/named/return-type TyIds, and inserts a fresh FnTy entry. This is the rare cross-arena flow — `subst_apply` writes to **two arenas** in one logical call (TyArena for the wrapping `Fn` handle; FnTyArena for the inner FnTy).
- **RecordArena**: the `Record(i64)` payload is a RecordId. Same pattern: lookup → recurse → insert fresh entry.

Cross-arena handle discipline: a TyId NEVER appears in a FnTyArena entry except as a field type (positional / named / return / var-positional / var-keyword); a RecordId NEVER appears in a TyArena entry except as the payload of `Record(id)`. The cb port's type-newtype aliases (`type FnTyId = i64`, `type RecordId = i64` per 0055a §"Decision") prevent accidental cross-namespace handle assignment at type-check time.

## 5. Per-fn complexity

`infer.rs::Subst` + free functions decompose to ~10-12 cb-port functions. Per `infer.rs::unify` the function body is the dominant complexity contributor (~150 LOC out of 259 total); per `infer.rs::Subst::apply` the recursive composite arms add another ~42 LOC; remaining ~67 LOC are trivial accessors + finalize.

| Function | Rust LOC | cb-port complexity | Notes |
|---|---|---|---|
| `Subst::new` | 3 | trivial | `Subst { map: dict_new() }` |
| `Subst::get` | 3 | trivial | dict lookup over arena handle |
| `Subst::extend` | 3 | trivial | dict insert |
| `subst_apply` | ~42 | medium — 9 arena-recursive arms | Tuple/List/Set/Ref/Dict/Record/Fn/Adt/Alias each insert fresh composite entries; Var arm chains through `map` |
| `Subst::fully_resolved` | 4 | trivial | `free_vars(arena, applied_id).is_empty()` |
| `unify` | ~150 | high — 14 match arms | 5 atomic arms; 1 Never arm; 1 Var-anything arm with occurs-check; 8 compound arms (Tuple/List/Set/Ref/Dict/Record/Fn/Adt/Alias) with recursive descent; Generic-eq arm; default `TypeMismatch` arm |
| `finalize` | ~10 | medium — calls `subst_apply` + `free_vars` + `AmbiguousType` construction | mirrors Rust |

The single largest function in this file is `unify` at ~150 LOC + 14 arms; cb port estimate ~165-180 LOC under arena indirection. Per the 0055a §"Decision" `clone_into_arena` convention, no `unify` arm needs explicit cross-arena copy because `unify` always operates on a single shared `&mut TyArena`.

Recursion + mutation via arena handles is the dominant pattern: every compound `unify` arm calls `unify(arena, child_a_id, child_b_id, subst, span)`. The arena-handle dereference at each step is a single `arena.lookup(id)` call producing a `TyEntry`; the body then pattern-matches the `TyEntry` variant. This is mechanically the same shape as the Rust impl's `match (t1.clone(), t2.clone())` — except `.clone()` is replaced by `lookup(arena, t1_id)` + `lookup(arena, t2_id)`.

### 5.1 Variable substitution + occurs-check

The Rust `(Ty::Var(v), other) | (other, Ty::Var(v))` arm has three branches:

- `other.free_vars().contains(&v)` → `OccursCheck` error with `suggestion: Some("add a type annotation — recursive types must be explicit")`.
- Otherwise `subst.extend(v, other)`.

The cb port reproduces this exactly via `(TyEntry::Var(v), _) | (_, TyEntry::Var(v))` pattern + `free_vars(arena, other_id).contains(v)` predicate. The `other` payload in the cb port is an arena handle `i64`, not a value `Ty`; the `subst.extend(v, other_id)` call stores the arena handle.

### 5.2 Compound-arm recursive descent

Each of the 8 compound `unify` arms (Tuple / List / Set / Ref / Dict / Record / Fn / Adt / Alias) follows the same mechanical pattern in the cb port:

1. Pattern-match both sides via `lookup(arena, t1_id)` + `lookup(arena, t2_id)` against the corresponding `TyEntry::*` variant.
2. Arity check (where applicable: Tuple / Fn positional / Fn named / Adt args / Alias args). Emit `ArityMismatch` or `TypeMismatch` per the corresponding Rust arm.
3. Recursive `unify(arena, child_a_id, child_b_id, subst, span)` per child pair.
4. For `Record`: closed-record key-set equality check before per-field unify; emit `TypeMismatch` on key-set divergence.
5. For `Fn`: positional + named arity check; positional unify; named-key + per-named-pair unify; return-type unify.
6. For `Adt` / `Alias`: id equality guard `if id_a == id_b`; arity check; per-arg unify.

The recursive `unify` call shape preserves the Rust impl's depth-first traversal exactly. Drift between Rust + cb at this layer manifests as parity-harness divergence per ADR-0055e §6.

## 6. Risk register

Top 3 risks ranked by impl-blast-radius:

1. **Arena cycle in `unify`** — though Phase H types are tree-shaped per ADR-0055 §5, a buggy `subst_apply` arm could produce an arena handle that, when dereferenced, produces a `TyEntry` whose children include the original handle. `unify` then recurses indefinitely. Mitigation: every `subst_apply` arm that produces a composite MUST insert a fresh `TyEntry` with children pointing to handles **strictly less than** the new entry's handle (per 0055a §6 risk 1 property test "fresh handle is always > all referenced handles"). `unify` termination follows from arena handles forming a DAG under the strict-less-than ordering. Add a property test "unify on adversarial input does not exceed depth N" with N = arena.len() to the Phase 2 sanity stage of ADR-0055e harness.

2. **`Subst` compose order semantics** — the Rust impl does NOT have an explicit `Subst::compose` method; substitutions extend in place via `Subst::extend`. The cb port preserves this exactly: no `compose` function, no left-vs-right composition ambiguity. BUT — the `subst_apply` recursive walk-through-chain (Rust `Ty::Var(v) => match self.map.get(v) { Some(inner) => self.apply(inner), None => Ty::Var(*v) }`) implicitly composes by following the chain transitively. The cb port's `subst_apply` arm for `TyEntry::Var(v)` MUST mirror this transitive-walk semantics exactly: `match map.get(v) { Some(inner_id) => subst_apply(s, arena, inner_id), None => <see below> }`. Drift risk: cb impl might return the raw `v` (without re-inserting) — passes parity on most inputs but diverges on inputs where two adjacent `unify` calls each produce a fresh-Var-entry handle that would otherwise be different. Mitigation: Phase 2 harness includes "chained `Var → Var → Concrete` resolution" property test; canonicalization (0055e §3) ensures both impls' outputs are compared up to arena-id renaming. **None-branch impl-time latitude**: the cb impl MAY relax the mandated `insert(arena, TyEntry::Var(v))` on the None branch — if canonicalization absorbs handle-aliasing, the Rust impl's value-clone behaviour (producing a new `Ty::Var`) is already parity-equivalent to returning the original handle. The cb impl SHOULD prefer returning the original input handle (value-clone semantics without arena insert) to avoid monotonic arena growth; parity harness canonicalization absorbs differences via encounter-order renaming. If the "arena length grows monotonically" property test (§9.1) flags divergence, fall back to the explicit `insert(arena, TyEntry::Var(v))` form at that impl-time decision point.

3. **`occurs_check` arena-walk performance** — Rust `free_vars()` is a single-pass recursive descent over the value-tree `Ty`. Under arena form, it becomes a recursive descent over arena handles, each step a `lookup(arena, id)` call. For deeply-nested types (e.g., `List[List[List[...List[Int]...]]]` with depth D), the cb-side walk is O(D) lookups + O(D) recursion frames vs Rust's O(D) recursion + zero lookups. For M2 corpus, D is bounded by source-text nesting (single-digit); cost is negligible. Risk emerges only post-Phase-H if corpus extends to pathologically-deep types. Mitigation: track 99-th percentile depth in the Phase 2 sanity stage rollup log; escalate to ADR-0055 amendment (memoize `free_vars` per arena handle) only if D > 100 corpus surfaces.

### Deferred concerns

- **`Subst.map` iteration order** — Rust `HashMap<VarId, Ty>` has non-deterministic iteration order; Cobrust `dict[i64, i64]` is `indexmap`-backed (insertion-order preserved per ADR-0050d §"Container internals"). For `subst_apply` + `unify` semantics this is irrelevant (both use point lookups, not iteration). For `display_error` Multiple-aggregation, the iteration order difference might surface — but `Multiple` is constructed by `check.rs` (0055d scope), not by this file. Out-of-scope for 0055c; addressed in 0055d if it surfaces.
- **Cobrust `Result[T, E]` syntax** — Rust `Result<(), TypeError>` ports to cb `Result[(), TypeError]`. Per ADR-0050a §"Option type" baseline, `Result` is a built-in enum; M2 baseline includes `Ok(...)` + `Err(...)` constructors. `?` operator availability in cb at HEAD `929cd4a` is presumed (per ADR-0050a §"Error propagation"); if absent, cb impl unwraps with explicit `match` per `infer.rs::unify` `?`-rich shape. Risk: explicit `match` ladders inflate cb-port LOC by ~30%; accepted within 0055c §10.2 negative-consequence budget.

## 7. Pre-dispatch gate

Required before this ADR's P9 design spike + P10-direct PAIR dispatches:

- [ ] **ADR-0055a merged** — Tier-1 `ty.rs` cb port + `TyArena` + `TyEntry` + arena-aware `subst_var` / `free_vars` / `clone_into_arena` API stable. Per ADR-0055 §3.5 Wave 2 → Wave 3 sequencing.
- [ ] **ADR-0055b merged** — Tier-1 `error.rs` + `lib.rs` cb port + `TypeError` enum + `display_error` API stable. Same Wave 2 → Wave 3 sequencing.
- [ ] **ADR-0055e accepted (Phase 2 sanity baseline merged)** — Rust-vs-Rust diff-empty on M2 corpus confirmed. Canonical-namespace traversal extended to 5 namespaces per the 0055e amendment is in-effect.
- [ ] **Arena re-evaluation gate** — ADR-0055 §5 closing paragraph mandates Tier-2 dispatch revisit the arena disposition. Default: holds. Confirm Tier-1 (0055a + 0055b) impl experience did not surface arena cost.

No dependency on Phase 7.5 (recursive struct types) per ADR-0055 §3.2.

## 8. Cross-ADR coordination

- **Fed by 0055a** — `TyArena` + `TyEntry` + arena-aware `subst_var` / `free_vars`. This ADR's `unify` + `subst_apply` consume those surfaces directly. The "arena re-evaluation gate" (ADR-0055 §5) inherits from 0055a's experience.
- **Fed by 0055b** — `TypeError` enum + every variant this ADR emits (`TypeMismatch`, `ArityMismatch`, `OccursCheck`, `KeywordArgMismatch`, `AmbiguousType`). `display_error` parity (0055b §6 risk 2) is upstream concern; this ADR consumes the enum shape only.
- **Feeds into 0055d (`check.rs` cb port, Tier-2)** — every `synth_*` arm in 0055d calls `unify` or `subst_apply` from this ADR. The `&mut TyArena` receiver convention codified in §2 + §3 settles ownership questions for 0055d's stateful `Ctx` (which holds the `TyArena` for the checker invocation duration).
- **Consumed by ADR-0055e** — parity harness diff-tests both impls' `Subst` + `unify` + `finalize` outputs on the M2 corpus. The 5-namespace canonicalization (per 0055e §3 amendment 2026-05-18) covers every arena-handle this ADR threads through `Subst.map` values + `TypeError` payload fields.
- **Inherits from ADR-0006** — bidirectional inference rules pinned by ADR-0006 §"Selected typing rules". This ADR ports the inference engine under arena form without semantic divergence.
- **Inherits from ADR-0052a Wave-1** — `Ty::Ref(Box<Ty>)` ports under arena as `Ref(i64)` per 0055a §3 table. The `unify` arm for `(Ty::Ref(a), Ty::Ref(b)) => unify(&a, &b, ...)` preserves structural-not-transparent semantics per `infer.rs::unify` Ref-arm doc-comment (no `(Ref(a), b)` cross-arm — the one-way coercion lives at `unify_call_arg` in 0055d scope).

## 9. Wall

~3-5 days per ADR-0055 §3.5 Wave-3 budget (smaller of the two Tier-2 sprints; 0055d is the dominant cost).

- **TEST hours**: ~6-8 (5 property tests in §6 risk register coverage + arena-cycle-termination assertions + cross-namespace canonicalization smoke test).
- **DEV hours**: ~20-28 (port 259 LOC Rust to ~280-320 LOC cb under arena form + harness wire-in).
- **Host**: DG primary per ADR-0055 §9.1 row 5. Mode C (P10-direct PAIR).

### 9.1 Phase 2 harness wire-in plan

Per ADR-0055e §8 Phase 2 sanity baseline, this ADR contributes 3 new property-test entries to the harness corpus:

- **"unify-termination"** — adversarial input: deeply-nested `List[List[...List[Int]...]]` (depth 50). Cb impl + Rust impl both produce `Ok(())` on `unify(t, t)`; canonicalization aligns arena handles.
- **"chained Var resolution"** — input: two adjacent `unify(Var(?0), Int)` + `unify(Var(?0), Int)` calls. Both impls' final `Subst.map` resolve `?0 → Int`; `subst.fully_resolved(Var(?0))` returns `true`.
- **"occurs-check positive"** — input: `unify(Var(?0), List[Var(?0)])`. Both impls produce `Err(TypeError::OccursCheck { var: 0, ty: <List[Var(0)] arena handle>, ... })`; harness BLOCK rules per 0055e §6 enforce variant + canonical-payload identity.

Phase 3 cb-side wire-in (per 0055e §8) lands concurrently with this ADR's impl PAIR — no separate dispatch.

## 10. Consequences

### 10.1 Positive

- First Tier-2 port lands; arena-aware `Subst` + `unify` becomes operational training-data corpus for §2.5 §B (HM-style inference under arena indirection — a pattern future cb crates inherit).
- `unify`'s 14-arm match statement in cb form is the most concentrated arena-recursive pattern in Phase H; the §3 receiver convention codified here scales to 0055d's 50+ functions without re-litigation.
- `finalize` + `AmbiguousType` arm closes the surfaceable diagnostic loop: every Ty in `def_types` either resolves cleanly or produces a single `AmbiguousType` per 0055d's `check()` top.

### 10.2 Negative

- Arena indirection compounds at recursive `unify` calls — each compound arm performs one extra `lookup(arena, id)` per child vs the Rust impl. For deeply-nested types (per §6 risk 3), this is observable; for M2 corpus, negligible.
- `subst_apply` allocating fresh composite entries every call (per §4 invariant 1) means the cb-side `TyArena` grows monotonically through a checker invocation; Rust impl's value-clone shape never inflates the heap. Phase H closure budget unaffected; future memory profiling may surface an arena-trim opportunity.
- The `Subst.map: dict[i64, i64]` shape loses Rust's `Ty`-typed map value; consumers in 0055d must remember the value is a TyId arena handle, not a TyEntry. Type-confusion risk mitigated by Cobrust's i64-newtype alias facility (per 0055a §"Decision" `type TyId = i64`).

### 10.3 Neutral / unknown

- Whether the cb impl uses method-call sugar (`subst.apply(arena, t)`) or free-function form (`subst_apply(&subst, arena, t)`). Per ADR-0052d Phase G method-form, method-call sugar reads more naturally; per ADR-0055 §4.1 "User-defined traits NOT shipped", true `impl` blocks are unavailable. Use free-function form; rename ergonomics revisited at impl-write time if read-poorly.

## 11. Dispatch readiness

- **TEST**: opus (D4 per `feedback_subagent_model_tier` — Tier-2 inference engine is hard / strategic / arena-recursive correctness depends on subtle ordering guarantees).
- **DEV**: opus (D4 — `unify`'s 14-arm match + occurs-check semantics + receiver-convention codification all require §2.5 compile-time-catch discipline).
- **Wall**: ~3-5 days per §9.
- **Post-author audit**: Tier-1 audit fires post-return BEFORE merge per `feedback_post_author_audit_mandatory`. Audit scope: §4 arena-interaction invariants compliance + §5 per-fn complexity table accuracy + §6 risk mitigation evidence in impl + cross-ADR `&mut TyArena` receiver convention surfaced consistently with 0055d.

### 11.1 Symbol-anchor convention compliance (F34)

Per F34 ratified finding (`f34-pre-candidate-numeric-anchor-degradation-high-churn.md`), this ADR adopts symbol anchors throughout — `infer.rs::Subst::apply`, `infer.rs::unify`, `infer.rs::finalize` — instead of numeric `infer.rs:NN` references. The `check.rs` cross-references in §8 follow the same convention (e.g., `check.rs::Ctx::unify_call_arg` rather than `check.rs:1687`). Phase H 0055c + 0055d explicit adoption constitutes the second corroborator that promoted F34 to ratified status (2026-05-18).

### 11.2 Documentation mandate

Per ADR-0055 §9.2, this sub-ADR commit ships triple-doc updates (zh + en + agent) per constitution §3.3:

- `docs/human/{zh,en}/self-host.md` §"Inference engine self-host" — new subsection covering arena-aware `Subst` + `unify` + occurs-check pattern. Examples-before-abstractions per CLAUDE.md §3.1: show a worked `unify(List[?0], List[Int])` call before describing the algorithm.
- `docs/agent/modules/cobrust-types-cb.md` (new) — module-level overview keyed by `module_id: cobrust-types-cb::infer`; cross-references stable per F34 symbol anchors.
- Bilingual sync rule per CLAUDE.md §3.3 — zh + en land in same commit as agent docs + impl.

— P9 Tech Lead, 2026-05-18
