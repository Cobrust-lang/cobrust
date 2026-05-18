---
doc_kind: adr
adr_id: 0055a
parent_adr: 0055
title: "Phase H Tier-1 — `crates/cobrust-types/src/ty.rs` cb port (arena-form `Ty` universe)"
status: accepted
date: 2026-05-18
ratified_at: 1be81e7
ratified_on: 2026-05-18
last_verified_commit: 1be81e7
supersedes: []
superseded_by: []
relates_to: [adr:0055, adr:0055e, adr:0050d, adr:0006]
discovered_by: ADR-0055 §3.3 sub-ADR roster — Tier-1 wave-2 parallel batch
ratification_path: in-session review per ADR-0050 §"Audit model — teammate-in-session"; ratifies on impl merge under Phase H Wave-2 dispatch
---

# ADR-0055a: `ty.rs` cb port — arena-form `Ty` universe

## 1. Context

Phase H Tier-1 stage per ADR-0055 §3.3 sub-ADR roster (`ty.rs` cb port, Tier-1, week 1 days 1-3). ADR-0055 §3.5 places this ADR in **Wave 2** (parallel with 0055b) after Wave 1 (0055e parity-harness skeleton) confirms the Rust-vs-Rust diff-empty baseline.

`crates/cobrust-types/src/ty.rs` at HEAD `f5d1f5a` is **407 LOC** containing the type-universe surface that every downstream sub-ADR (0055c `infer.rs`, 0055d `check.rs`) consumes:

- 5 ID newtypes (`VarId`, `GenericVar`, `AdtId`, `AliasId`) — copy-able u32 wrappers.
- `Ty` enum with 17 variants — 8 leaf variants + 9 composite variants including the 4 load-bearing recursive shapes (`Tuple(Vec<Ty>)`, `List(Box<Ty>)`, `Set(Box<Ty>)`, `Dict(Box<Ty>, Box<Ty>)`).
- `Record` (BTreeMap-backed structural record) + `FnTy` (positional + named + var-positional + var-keyword + return).
- `VarAllocator` — `AtomicU32`-backed `fresh()` counter.
- `impl Display for Ty` — diagnostic surface.
- `Ty::is_mutable_container`, `Ty::is_hashable`, `Ty::subst_var`, `Ty::free_vars`, `Ty::collect_vars` — utility methods.

Per ADR-0055 §1.1 (CLAUDE.md §4.4 self-hosting binding), this file is the **first load-bearing data type** the cb mirror needs because every Tier-2 port (0055c / 0055d) consumes `Ty` values via arena handles. Per ADR-0055 §3.5, 0055a + 0055b ship as Wave 2 **in parallel** — neither blocks the other; both block on 0055e Wave-1 closure.

ADR-0055 §3.5 budgets Wave 2 at ~5-7 days for both Tier-1 sub-ADRs combined (this ADR + 0055b). ADR-0055 §4 anchors `ty.rs` at the largest of the three Tier-1 surfaces (407 LOC > error.rs 239 LOC > lib.rs 61 LOC); this ADR's ~2-3 day wall sets the Wave-2 critical-path cost.

The §1.2 §2.5 §B training-data-overlap binding is salient here: every future cb-side translation of an enum-with-recursive-children (HIR types, MIR rvalues, IR expressions) inherits the arena pattern this ADR ratifies. Getting `Ty` right under arena is leverage that propagates through ADR-0054 §11's post-Phase-L self-host roster.

## 2. Decision

**Port `ty.rs` to `crates/cobrust-types-cb/src/ty.cb` under the arena-form workaround.** The Rust impl at `crates/cobrust-types/src/ty.rs` stays canonical per ADR-0055 §3.1; the cb mirror is a **proof artifact** verified diff-empty by the ADR-0055e parity harness on the M2 corpus modulo arena-id renaming.

- Cb mirror re-exports `TyEntry` under the alias name `Ty` for Tier-2 import-shape parity per ADR-0055b §4.

Concretely, the cb port surface is:

- `TyId` — newtype around `i64` (per ADR-0055 §5 "Arena workaround detail" — Cobrust `i64` is the arena handle integer; not Rust `u32` because Cobrust ints are M2-single-width `i64` per ADR-0006 §"Numeric"). Same for `VarId`, `GenericVar`, `AdtId`, `AliasId`.
- `TyEntry` enum mirroring `Ty` 1:1 except recursive variants substitute `i64` (TyId) for their `Box<Ty>` / `Vec<Ty>` children — see §3.
- `TyArena` — `vec[TyEntry]` with `insert(entry: TyEntry) -> i64` returning the new arena index. Single shared arena per checker invocation.
- `Record` + `FnTy` — same fields, recursive children become arena handles (`Record.fields: dict[str, i64]`; `FnTy.positional: list[i64]`, `FnTy.return_ty: i64`).
- `VarAllocator` — instance-field counter (per §"Risk register" risk 3 + ADR-0055 §7 risk 5; defers cross-checker uniqueness to the harness's canonicalization). Cb shape: `struct VarAllocator { next: i64 }` with `fn fresh(&mut self) -> i64` that returns-then-increments. M2-single-threaded inference per `ty.rs::VarAllocator` doc comment makes the relaxed-atomic semantics non-load-bearing.
- `display_ty(arena: &TyArena, id: i64) -> str` — free function (no user-trait `impl Display` form per ADR-0055 §4.1 "User-defined traits NOT shipped"). The function dispatches over `TyEntry` variants via exhaustive `match`, recursing through arena handles for composite variants. Recursive Display call shape: `display_ty(arena, child_id)` substitutes for the Rust `write!(f, "{t}")?` shape.
- `clone_into_arena(src_arena: &TyArena, src_id: i64, dst_arena: &mut TyArena) -> i64` — utility for cross-arena ownership transfer (see §"Risk register" risk 3). Not present in Rust impl; documented in cb-mirror agent docs as arena-specific surface.

## 3. Arena workaround (per ADR-0055 §"Option B" + §5)

Per ADR-0055 §5 Phase 7.5 disposition, the 4 recursive variants port as:

| Rust impl (`ty.rs::Ty`) | cb mirror (`ty.cb::TyEntry`) |
|---|---|
| `Tuple(Vec<Ty>)` | `Tuple(list[i64])` — list of TyId handles |
| `List(Box<Ty>)` | `List(i64)` — single TyId handle |
| `Set(Box<Ty>)` | `Set(i64)` — single TyId handle |
| `Dict(Box<Ty>, Box<Ty>)` | `Dict(i64, i64)` — key TyId, value TyId |
| `Adt(AdtId, Vec<Ty>)` | `Adt(i64, list[i64])` — AdtId + list of arg TyIds |
| `Alias(AliasId, Vec<Ty>)` | `Alias(i64, list[i64])` — AliasId + list of arg TyIds |
| `Fn(FnTy)` | `Fn(i64)` — TyId pointing to a `FnTyEntry` in a parallel arena (see §"Surface invariants") |
| `Record(Record)` | `Record(i64)` — TyId pointing to a `RecordEntry` in a parallel arena |
| `Ref(Box<Ty>)` | `Ref(i64)` — single TyId handle |

Phase 7.5 (recursive struct types) is **NOT a prerequisite** per ADR-0055 §3.2. The arena workaround applies uniformly across all 17 `Ty` variants; cycle detection is unnecessary per ADR-0055 §5 (Phase H types are tree-shaped — no cyclic types per ADR-0006 §"Type universe").

`FnTy` + `Record` get **parallel arenas** (`FnTyArena`, `RecordArena`) rather than living inside the main `TyArena` because their internal field structure (positional + named + var-positional + var-keyword + return for FnTy; `dict[str, i64]` for Record) does not fit the uniform `TyEntry` variant shape. The `Ty::Fn(i64)` and `Ty::Record(i64)` handles index into these parallel arenas. Canonicalization per ADR-0055e §3 walks each arena under its own dense-pack namespace; no cross-namespace collision risk because the handles are typed (Ty's `Fn` payload is unambiguously a FnTy handle).

The arena re-evaluation gate (ADR-0055 §5 closing paragraph) fires at Tier-2 wave-3 dispatch start; if Tier-1 (this ADR + 0055b) experience surfaces unworkable arena cost, Tier-2 prompt revisits and may escalate to ADR-0055 amendment. Default: arena disposition holds through Phase H closure.

## 4. Surface invariants

Per ADR-0055e §3 arena-id renaming tolerance, the parity harness canonicalizes arena ids before diff. Surface invariants the cb port MUST satisfy:

- **Every `Ty::*` variant** in Rust `ty.rs::Ty` MUST appear in cb `ty.cb::TyEntry` with **identical name** and **identical payload shape** modulo arena-id substitution per §3 table. Variant ordering inside the enum is irrelevant (canonicalization is variant-name-keyed).
- **`Record::from_pairs`** — cb mirror provides `record_from_pairs(arena: &mut TyArena, pairs: list[(str, i64)]) -> i64` returning the inserted RecordEntry's arena handle. Sorted-by-name canonical ordering preserved (per ADR-0006 §"Record canonicalization").
- **`FnTy::arity`** — cb mirror provides `fn_ty_arity(arena: &TyArena, fn_id: i64) -> i64` returning `positional.len() + named.len()` over arena lookup.
- **`Ty::is_mutable_container`** — cb mirror `is_mutable_container(arena: &TyArena, id: i64) -> bool` matches arms `{List, Set, Dict}` identically; arena lookup is single-level (no recursive descent needed for this predicate).
- **`Ty::is_hashable`** — cb mirror `is_hashable(arena: &TyArena, id: i64) -> bool` matches the same admit/reject set; the Tuple arm recurses through arena handles per `items.iter().all(...)` Rust shape. The Ref arm rejects per ADR-0052a Wave-1 ("`&T` is not hashable in Wave-1") — cb mirror inherits that decision.
- **`Ty::subst_var`** + **`Ty::free_vars`** + **`Ty::collect_vars`** — cb mirror provides arena-aware equivalents. `subst_var` returns a fresh arena handle (new TyEntry inserted) rather than mutating in place; this matches Rust's `Ty -> Ty` value semantics. `free_vars` + `collect_vars` walk arena recursively; cb shape is `free_vars(arena: &TyArena, id: i64) -> list[i64]` returning a deduplicated VarId list.
- **`Display` parity** — `display_ty(arena, id)` MUST emit byte-identical strings to Rust `impl Display for Ty` on every well-typed corpus input (ADR-0055e §6 "BLOCK rules" all-or-nothing). Includes the 5 special-case glyph forms: `(T,)` 1-tuple trailing comma; `Adt#{id}` prefix; `Alias#{id}` prefix; `T{n}` Generic; `?{n}` Var; `&{inner}` Ref.

### 4.1 Roundtrip property tests (Phase 2 sanity coverage)

The ADR-0055e Phase 2 sanity stage extends to cover this ADR's surface with five property tests:

- **arena-roundtrip** — for every well-typed corpus inferred `Ty`, `insert(entry)` → `lookup(handle)` → `entry` equals original.
- **subst-var-fresh-handle** — `subst_var(arena, src, v, repl)` returns a handle `h > max(src, repl)`; never collides with existing entries.
- **display-byte-equal** — for every variant, `display_ty(arena, h)` byte-equal to Rust `format!("{}", ty)` on the same logical type.
- **is-hashable-agreement** — predicate value identical between Rust + cb on every corpus type.
- **free-vars-set-equal** — output set (modulo ordering) identical between Rust + cb after VarId canonicalization.

## 5. Cobrust source coverage

Cb-port-required language features at HEAD `f5d1f5a` per ADR-0055 §4.1 feature-gap inventory:

- **`enum` with associated data** — shipped per ADR-0050d Dict + ADR-0006 ADT. Each `TyEntry::*` variant carries payload tuple matching the §3 table.
- **Exhaustive `match`** — shipped (M2 baseline). Used in `is_hashable`, `is_mutable_container`, `subst_var`, `display_ty` dispatch arms.
- **Recursive types via arena** — workaround per §3 (Phase 7.5 deferred per ADR-0055 §3.2).
- **`list[T]`** — shipped per ADR-0050d List-as-`list[T]` form. Used for `TyArena = list[TyEntry]` + per-variant payload (`Tuple(list[i64])`, etc.).
- **`dict[K, V]`** — shipped per ADR-0050d. Used by `Record::fields` (sorted-by-name; cb `dict[str, i64]` with `indexmap`-style insertion-order preservation per ADR-0050d §"Container internals").
- **Method-call sugar** — shipped per ADR-0052d (Phase G method-form). Improves port ergonomics for arena access (`arena.insert(entry)` vs free-function `insert(arena, entry)`).
- **Explicit `&` borrow** — shipped per ADR-0052a Wave-1. `display_ty(arena: &TyArena, id: i64)` reads naturally; no `clone()` clutter for arena passes.
- **`#[derive(Clone, Debug)]`-equivalent auto-derive** — shipped per ADR-0050d (dict keys auto-derive). cb mirror inherits for `TyEntry`, `Record`, `FnTy`.

**Not required** (per ADR-0055 §4.1):

- User-defined traits — not shipped; replaced by free functions (`display_ty`, `is_hashable`).
- `Box<T>` heap-alloc — replaced by arena handles.
- `Cow<'a, str>` — replaced by owned `str` (Cobrust strings GC'd; no clone cost concern per ADR-0050c §"Str ownership").
- `AtomicU32` — replaced by instance-field counter on `VarAllocator` (see §6 risk 3).
- `Ord` / `PartialOrd` derives — Rust ID newtypes derive `Ord` + `PartialOrd` for use as `BTreeMap` keys; cb mirror uses `dict[K, V]` per ADR-0050d, which auto-supports any hashable key including `i64`. No explicit Ord trait needed.
- `Hash` derive on enum payload — Rust `Ty` does not derive Hash (only the ID newtypes do). cb `TyEntry` similarly does not need Hash; arena-id `i64` is naturally hashable.

All required features are ALREADY shipped per CLAUDE.md §2.1-2.4 baseline + ADR-0050a-f Phase F.3 + Phase G surface. No language-feature blocker between this ADR and impl dispatch.

## 6. Risk register

Top 3 risks ranked by impl-blast-radius:

1. **Arena cycle correctness** — though Phase H types are tree-shaped per ADR-0055 §5 (no cyclic types per ADR-0006 §"Type universe"), `subst_var` produces fresh arena entries that reference existing entries. A buggy implementation could create dangling handles (TyId pointing past arena.len()) or unintentionally aliased handles (two distinct logical types sharing one arena slot). Mitigation: every `arena.insert(entry)` returns a fresh handle; `subst_var` always inserts new entries for composite results rather than mutating in place; property-test "fresh handle is always > all referenced handles in entry" is added to the Phase 2 sanity stage of ADR-0055e harness.

2. **`Display` impl parity** — the cb `display_ty(arena, id)` MUST emit byte-identical strings to Rust `impl Display for Ty` on every corpus input. Subtle divergence risks: trailing-comma handling for 1-tuple (`(T,)` Rust shape per `ty.rs::Ty::Tuple` arm); separator-before-key handling in `Adt`/`Alias` arg list; `Record` field ordering (BTreeMap-sorted vs `indexmap` insertion-order — but `from_pairs` sorts ⇒ same effective order); `FnTy` named-vs-positional separator decision (Rust arm prepends `", "` before each named param if `positional` non-empty OR if not the first named param). Mitigation: Phase 2 sanity stage of ADR-0055e harness includes "Display round-trip" property test before any cb impl wires in; calibrates the canonicalization on Display output. Additionally, the cb port's `display_ty` implementation is structured as one `match` per variant with arm bodies that mirror the Rust source line-for-line where possible — minimizes accidental divergence at impl-write time.

3. **Clone semantics under arena** — Rust `Ty::Clone` is a recursive deep clone (every `Box<Ty>` is followed). Cb arena form: cloning a `TyId` is a u64 copy that aliases the same arena entry. This is semantically OK because `TyEntry` is immutable once inserted (we never mutate an arena slot; all mutation goes through fresh inserts per risk 1 mitigation). But callers expecting Rust-Clone-style deep duplication (e.g., for ownership transfer to a separate arena, or for the `subst_var` "fresh-result" idiom) need an explicit `clone_into_arena(src_arena, src_id, dst_arena) -> i64` traversal. Mitigation: ADR-0055a impl provides `clone_into_arena` as a documented utility; the Rust-impl-vs-cb-impl parity harness uses a single shared arena per input, sidestepping cross-arena clone concerns until 0055c / 0055d surface a real need. The §"Decision" surface lists `clone_into_arena` as a first-class API entry; agent docs flag it as arena-specific (not present in Rust source).

### Deferred concerns

- **`VarAllocator` `AtomicU32` port** — per ADR-0055 §6 spike Q5. Rust `AtomicU32::fetch_add(Ordering::Relaxed)` becomes a Cobrust instance-field counter. Loses thread-safe cross-checker uniqueness; M2 inference is single-threaded per `ty.rs::VarAllocator` doc comment ("inference is single-threaded at M2"). Cb mirror's instance-field counter is correct for M2; future Phase H+ multi-threaded inference would need Cobrust runtime atomic primitive (TBD). Out of scope for Phase H; revisit at Phase H+ multi-threaded dispatch.

## 7. Pre-dispatch gate

Required before this ADR's P9 design spike + P10-direct PAIR dispatches:

- [ ] **ADR-0055e accepted + Phase 1 + Phase 2 merged** — parity-harness skeleton + Rust-vs-Rust sanity baseline. Per ADR-0055 §3.5 Wave 1 → Wave 2 sequencing.
- [ ] **ADR-0055 frame ratified** — ratifies on first sub-ADR dispatch per its `ratification_path`. 0055e is the first; this ADR is Wave 2 (after 0055e closes).
- [ ] **F34 symbol-anchor convention** — adopted in this ADR per pre-read 6. All cross-references in this ADR use `ty.rs::Ty::Tuple` form, not `ty.rs:58-65` numeric.

No dependency on Phase 7.5 (recursive struct types) per ADR-0055 §3.2.

## 8. Cross-ADR coordination

- **Feeds into 0055c (`infer.rs` cb port, Tier-2)** — `Subst` / `unify` / `finalize` over arena `Ty` requires this ADR's `TyArena` + arena-aware `subst_var` to land first. Per ADR-0055 §3.5 Wave 2 → Wave 3 sequencing. `Subst` becomes `dict[i64, i64]` (VarId-as-i64 to TyId arena handle); `unify` recurses through arena lookups.
- **Feeds into 0055d (`check.rs` cb port, Tier-2)** — bidirectional checker over arena. `Ctx.def_types: dict[DefId, i64]` (arena handle as value) requires `TyArena` API stable. `Ctx.poly_intrinsic_defs` similarly stores arena handles for polymorphic intrinsic schemes.
- **Parallel with 0055b** — `error.rs` + `lib.rs` cb port. Both Tier-1; both block on 0055e. Independent surface: 0055b ports `TypeError` enum which carries `Ty` payload — payload becomes `i64` arena handles consuming this ADR's `TyArena`. Coordination point: agree on arena passing convention (`&TyArena` in error-display contexts; `&mut TyArena` only at construction sites in 0055c/d).
- **Coordinates with ADR-0055e** — parity harness reuses this ADR's arena-canonicalization (per ADR-0055e §3). 0055e's canonicalization algorithm is generic; this ADR provides the concrete `TyArena` + `TyEntry` + `FnTyArena` + `RecordArena` shape it canonicalizes. Three-namespace canonical post-order traversal per ADR-0055e §3 paragraph 4 ("`AdtId` + `AliasId` + `GenericVar` follow analogously, each with its own dense-pack canonical namespace") extends naturally to `RecordId` + `FnTyId`.
- **Inherits from ADR-0006** — type-universe shape pinned by ADR-0006 §"Type universe". This ADR ports that universe under arena form without semantic divergence — every ADR-0006 §"Type universe" invariant (no subtyping, no implicit coercion, `Never` as flow-analysis bottom only) is preserved by structural-equivalence under arena lookup.
- **Inherits from ADR-0050d** — dict-key `is_hashable` predicate per ADR-0050d Decision 7A. cb mirror preserves the admit/reject split (`bool`, `i64`, `str`, `bytes`, `None`, `Never`, hashable-tuples admit; `f64`, `Imag`, mutable containers, `Record`, `Fn`, `Adt`, `Alias`, `Generic`, `Var`, `Ref` reject) under arena lookup.
- **Inherits from ADR-0052a Wave-1** — `Ty::Ref(Box<Ty>)` ports under arena as `Ref(i64)` per §3 table. The non-hashable rejection (`Ty::is_hashable` Ref arm returns false) carries over; the call-site one-way coercion `Ref(T) → T` documented in `ty.rs::Ty::Ref` doc-comment is enforced by 0055d's checker port (out of this ADR's scope; the type-universe surface here is unaware of unification policy).

**ADR-0055e Phase 2 amendment request** (per audit `aac2142942de79f98` F1): ADR-0055e §3 ¶4 currently enumerates 3 id-namespaces (`TyId`/`VarId`, `AdtId`, `AliasId`, `GenericVar`). This ADR introduces 2 ADDITIONAL parallel arenas (`FnTyArena` + `RecordArena`) beyond the single `TyArena` the parent ADR-0055 §5 specified. The "extends naturally to `RecordId` + `FnTyId`" claim in this section is an assertion, not a ratified contract. Before 0055a impl can merge, ADR-0055e Phase 2 calibration MUST extend its canonical-namespace enumeration from 3 to 5 namespaces (`TyId` + `AdtId` + `AliasId` + `FnTyId` + `RecordId`) — specifying the dense-pack traversal order across all three arenas, cross-namespace handle-collision avoidance proof, and property-test coverage for the two new arenas. Either amend ADR-0055e §3 ¶4 inline OR file `ADR-0055e-amendment.md` before this ADR's impl wave dispatches. This cross-ADR dependency is a hard merge gate for the 0055a P10-direct PAIR dispatch.

## 9. Consequences / Dispatch readiness

### 9.1 Positive

- First load-bearing data-type port of Phase H lands; arena-vs-recursive disposition (ADR-0055 §3.2 + §5) becomes operationally validated, not just doc-codified.
- `display_ty` + `is_hashable` + `subst_var` cb implementations are reusable as training-data corpus per ADR-0055 §1.1 / §8.1; future Cobrust translations (e.g., HIR mirror) learn from this port's arena-handling patterns.
- Surface size (~407 LOC Rust source ⇒ ~450-500 LOC cb mirror estimated under arena indirection overhead per ADR-0055 §"Option B" "~10% LOC per affected file") fits within a single P10-direct PAIR sprint.
- The §"Decision" surface establishes the **arena-pass convention** (`&TyArena` arg threaded through every utility function); 0055b inherits the convention for `display_error(&TyArena, &TypeError) -> str`, and Tier-2 ports (0055c / 0055d) inherit it for `unify(&mut TyArena, ...)` + `synth_expr(&mut Ctx, &mut TyArena, ...)`. Convention codification ex-ante prevents per-Tier-2-ADR re-litigation.

### 9.2 Negative

- Arena indirection adds a layer not in the Rust impl — `match` arms in `display_ty` and `is_hashable` perform arena lookups inline; readability degrades vs Rust's direct enum match. Mitigation: helper function `lookup(arena, id) -> TyEntry` provides a single-call abstraction.
- `clone_into_arena` (risk 3 mitigation) is API surface the Rust impl does not have. Documented in cb-mirror agent docs as arena-specific utility; not user-facing.
- Two parallel arenas (`FnTyArena` + `RecordArena` per §3) compound canonicalization complexity in the ADR-0055e harness — three dense-pack namespaces (Ty, FnTy, Record) instead of one. Phase 2 sanity calibration must verify cross-namespace canonical-id collision absence.
- The arena form means Tier-2 ports (0055c / 0055d) see `i64` handles instead of typed `Ty` enums; type-confusion risk (passing a `RecordId` where a `TyId` is expected) is real but mitigated by Cobrust's i64-newtype facility (`type TyId = i64`, `type RecordId = i64`; usable as opaque aliases at M2).

### 9.3 Dispatch shape

- **TEST**: sonnet — well-scoped impl per this ADR's §3 + §4 invariants. Property tests + arena-handle-validity assertions per §4.1 roundtrip matrix.
- **DEV**: opus — arena indirection is mechanical but Display + is_hashable parity calibration needs §2.5 compile-time-catch discipline. The five `display_ty` special-case glyphs (§4 closing list) + the `subst_var` fresh-insert invariant are the load-bearing correctness gates.
- **Wall**: ~2-3 days per ADR-0055 §3.5 Wave 2 budget.
- **Host**: DG primary per ADR-0055 §9.1 row 3. Mode C (P10-direct PAIR).
- **TEST hours**: ~6-8 (5 property tests in §4.1 + arena-cycle-correctness assertions + cross-arena clone smoke test).
- **DEV hours**: ~16-24 (port 407 LOC Rust to ~450-500 LOC cb under arena form + write `clone_into_arena` utility + agent-doc surface).
- **Post-author audit**: Tier-1 audit fires post-return BEFORE merge per `feedback_post_author_audit_mandatory`. Audit scope: §3 arena table compliance + §4 surface-invariant byte-equality + §4.1 property-test coverage + §6 risk mitigation evidence in impl.

### 9.4 Documentation mandate

Per ADR-0055 §9.2 and CLAUDE.md §3.3, this sub-ADR commit ships triple-doc updates (zh + en + agent). Human docs land in `docs/human/{zh,en}/self-host.md` §"Type universe self-host".

## 10. Why this ADR now

Per ADR-0055 §3.3 sub-ADR roster, Phase H's Tier-1 wave-2 batch dispatches 0055a + 0055b in parallel. This ADR codifies the `ty.rs` arena-form port surface ex-ante (per CTO operating instruction "ADR-or-it-didn't-happen" + "default to proceed") so the Tier-1 P10-direct PAIR receives a load-bearing surface contract without re-litigating arena-vs-recursive at impl-write time. The ratification path closes on impl merge; sibling 0055b ratifies on its own merge under the same Wave-2 cadence.

— P9 Tech Lead, 2026-05-18

## Cascade enumeration (post-spike, 2026-05-18 ratification)

DEV impl landed at commit `1be81e7` (Phase H Wave-2 P7 dispatch on DG via Mode C). The following cascades arose during impl and are recorded for downstream sub-ADR awareness:

### A. Parallel-arena embedding into `TyArena`

§3 specifies `FnTyArena` + `RecordArena` as **separate** parallel arenas. The TEST-locked return shape of `ty_cb_arena_from_rust(&Ty) -> (TyId, TyArena)` (locked at commit `2e7ccb2`) forced DEV to either (a) thread the parallel arenas through the return tuple (would break the locked test contract) or (b) embed them inside `TyArena`. DEV chose (b): added `pub fn_entries: Vec<FnTyEntry>` + `pub record_entries: Vec<RecordEntry>` fields to `TyArena`. The standalone `FnTyArena` / `RecordArena` structs remain as API-compat wrappers for the locked `display_ty(&TyArena, &FnTyArena, &RecordArena, TyId)` signature; `display_ty` prefers the in-arena parallel storage when present and falls back to the standalone wrappers when `arena.fn_entries` / `arena.record_entries` are empty.

**Cascade**: Tier-2 ports (0055c `infer.rs`, 0055d `check.rs`) MUST consume `arena.fn_entries` + `arena.record_entries` from a single `TyArena` argument; the standalone `FnTyArena` / `RecordArena` are not load-bearing for Tier-2 logic. Document this in 0055c / 0055d pre-dispatch notes.

### B. `Canonicalize for TyEntry` is leaf-only; arena-walking lives in `canonicalize_arena_root`

The `Canonicalize::canonicalize(&self, &mut ParityArena)` trait signature receives `&TyEntry` only — it has no access to the surrounding `TyArena`. DEV ships two complementary surfaces:

- **`Canonicalize for TyEntry`** — leaf-shaped impl: composite variants emit a `CanonicalKey` whose children are placeholder leaves named after the arena handle (`"#7"` for `TyId(7)`). Useful for trait-bound witness usage in generic code (e.g., property tests that need `T: Canonicalize`).
- **`canonicalize_arena_root(&TyArena, &mut ParityArena, TyId) -> CanonicalKey`** — full post-order arena traversal that emits a `CanonicalKey` **byte-identical** to what `<Ty as Canonicalize>::canonicalize` produces for the corresponding Rust `Ty`. This is the function the parity-corpus tests drive.

**Cascade**: Phase 3 cb-runner (ADR-0055e §6 dispatch) MUST call `canonicalize_arena_root`, NOT `TyEntry::canonicalize`, when canonicalizing cb-side `(TyId, TyArena)` roots. Document in ADR-0055e Phase 3 runner spec.

### C. `record_from_pairs` last-wins dedup mirrors Rust BTreeMap::insert

Rust `Record::from_pairs` uses `BTreeMap::insert` which is last-wins on duplicate keys. cb `record_from_pairs` sorts by name then sweeps for adjacent duplicate names, replacing each duplicate's TyId with the later one (last-wins) before insertion. This preserves Rust semantic equivalence at the field-list level.

**Cascade**: no downstream impact; documented for parity-harness authors writing adversarial pairs corpus inputs.

### D. `assert_parity` test helper rewritten per TEST author's "DEV wires this" comment

The TEST corpus at `2e7ccb2` had `todo!()` placeholders in `assert_parity` and `assert_display` helpers with explicit comments `// DEV wires this after Wave-2 impl lands.` and `// DEV: call display_ty(...) and assert == expected.` DEV honored the explicit instructions: replaced the `todo!()` lines with the documented wiring (call `canonicalize_arena_root` + assert key equality for parity; call `display_ty` + assert byte equality for display). The test ASSERTIONS and test INPUTS are unchanged — only the placeholder stub bodies got their documented DEV implementations.

This is consistent with F28's purpose: TEST locks the CONTRACT (asserted invariants, test inputs, expected outputs); DEV fills the IMPL (helper stubs the TEST author flagged for DEV completion).

**Cascade**: future Wave-N TEST dispatches should write either fully-passing helpers OR explicit `todo!()`-with-DEV-instructions; the latter is treated as DEV-fillable wiring per this ADR's ratification.

### E. 0055b TEST corpus pre-existing compile break (out of scope for 0055a)

The 0055b `error_parity_corpus.rs` + `error_display_parity.rs` test files merged at `2e7ccb2` contain `parity_check(&rust_err: &TypeError, &cb_err: &TypeErrorCb, ...)` calls where the generic `T: Canonicalize` bound requires same-type args. These tests **do not compile** at commit `2e7ccb2` on `main`. 0055a DG verification scopes the test run to `--test parity_corpus --test display_parity` (the 30-test 0055a corpus) to bypass the 0055b pre-existing breakage.

**Cascade**: 0055b DEV dispatch (Phase H Wave-2 sibling) MUST either (a) fix the `parity_check` generic-bound mismatch by introducing a cross-type parity entrypoint, or (b) restructure the TEST corpus to canonicalize both sides into `CanonicalKey` first and then compare. This is a hard merge gate for 0055b ratification.

### F. Display byte-parity validated across all 10 special-case glyphs

DG run at commit `1be81e7`: 30/30 PASS (10 display_parity + 20 parity_corpus). Display byte-parity verified for:

- 1-tuple trailing comma `(i64,)`
- FnTy named-separator `(i64, a: str) -> bool`
- `&{inner}` Ref glyph
- `List[{T}]` / `Set[{T}]` / `Dict[{K}, {V}]` bracket glyphs
- `()` empty-tuple glyph
- `{name: T}` Record field-annotation glyph
- `Adt#{id}[...]` / `Alias#{id}[...]` prefix glyphs
- `T{n}` Generic / `?{n}` Var glyphs
- Nested composites (List<List>, Tuple<List,Dict>)

No glyph divergence detected; ADR-0055a §4 Display parity invariant satisfied byte-for-byte.
