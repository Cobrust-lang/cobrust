---
doc_kind: adr
adr_id: 0055
title: "Phase H — Self-host cobrust-types in Cobrust (frame ADR: arena-vs-recursive disposition / crate split / parity-harness contract / completion bar)"
status: proposed
date: 2026-05-18
last_verified_commit: fd263f4
supersedes: []
superseded_by: []
relates_to: [adr:0054, adr:0050d, adr:0048]
discovered_by: P10/user 2026-05-18 — operationalize CLAUDE.md §4.4 self-hosting binding into Phase H batch frame per ADR-0054 §3
parent_adr: 0054
ratification_path: in-session review per ADR-0050 §"Audit model — teammate-in-session"; sub-ADRs 0055a..0055e ratify on dispatch; this frame ratifies on first sub-ADR (0055a) dispatch
---

# ADR-0055: Phase H — Self-host `cobrust-types` in Cobrust (batch frame)

## 1. Context

Phase H self-hosts `cobrust-types` (5 files / 3368 LOC) under the Option B arena workaround: `Ty::List(Box<Ty>)` recursive fields become arena handles (`i64`), deferring Phase 7.5 recursive-type native support while unblocking the full translation now. This 2-sentence frame is the load-bearing executive summary; §5 and §6 below expand the arena disposition in full.

### 1.1 Constitutional binding (§4.4)

`CLAUDE.md` §4.4 (HEAD `8b4366c` lines 200-203) — "Self-hosting roadmap. The compiler is initially in Rust. Once Cobrust reaches sufficient maturity (post-M5), begin self-hosting non-performance-critical compiler stages, prioritizing the type checker and the AST printer first." ADR-0054 §3 operationalized this as Phase H with a 3-week wall-time budget; this ADR is the **batch frame** that ratifies Phase H scope, sub-ADR roster, and the load-bearing arena-vs-recursive disposition (per §5 below).

### 1.2 §2.5 binding (LLM-first design)

Phase H is **§2.5-ranked second among Phase H/I/J/K/L** per ADR-0054 §2 reranking table — high (not highest) §2.5 ROI. Rationale per ADR-0054 §2 row 2: "Type checker self-host produces a `.cb` codebase the LLM can learn from for *every other Cobrust translation*. Closes the 'training-data overlap' gap §2.5 §B rule." Phase H precedes Phase J (highest §2.5 ROI — LSP) because the `cobrust-types-cb` mirror is a Cobrust-source body the LSP dogfoods against during Phase J development (ADR-0054 §2 closing rationale).

### 1.3 Scoping spike precedent

`docs/agent/dispatches/2026-05-18-phase-h-self-host-scoping.md` (P9 scoping spike authored 2026-05-18) supplies the empirical anchors this frame ratifies:

- **File count + LOC**: `wc -l crates/cobrust-types/src/*.rs` at HEAD `8b4366c` = **5 files / 3368 LOC** (ty.rs 407 + check.rs 2402 + error.rs 239 + infer.rs 259 + lib.rs 61) — **NOT** the 12 files / 5500 LOC ADR-0054 §3.1 estimated. Estimate-vs-actual delta tightens Phase H wall-time by ~30% (~2.5 weeks vs ADR-0054 §3.2's 3-week budget; ~0.5 week buffer).
- **Tier-3 files mentioned in earlier scoping prompts (`validate.rs`, `normalize.rs`) do not exist** at HEAD `8b4366c`. Phase H scope is the full mirror of the 5 listed files.
- **Recursive struct types (`Ty::List(Box<Ty>)`, `Ty::Tuple(Vec<Ty>)`, `Ty::Dict(Box<Ty>, Box<Ty>)`)** are the load-bearing language-feature gap. Spike §4 surfaced three options (A: ship Phase 7.5 first; B: arena workaround; C: hybrid). Spike recommended B; this ADR ratifies B per §5.

### 1.4 Phase H vs Phase G framing

| Axis | Phase G (closed) | Phase H (this ADR) |
|---|---|---|
| Mandate | §2.5 LLM-friendliness — ergonomic + verification depth | §4.4 self-hosting binding — proof artifact + training-data corpus |
| Surface posture | depth (4 P0 ergonomic axes) | breadth (full mirror of one crate, 5 files / 3368 LOC) |
| Output kind | Rust impl edits + new error fields | new `.cb` crate parallel to Rust canonical |
| Verification model | 5-gate (cargo build / clippy / fmt / cargo test / corpus) | 5-gate + **parity harness** (ADR-0055e) — Rust impl vs cb impl diff-test on M2 corpus |
| Audience for output | LLM agent emitting v0.3.0+ source | LLM agent + Phase J LSP + future Cobrust translations |

## 2. Options considered

### 2.1 Option A — full Phase H scope, ship Phase 7.5 first (recursive struct types)

- **Pros**: clean port; `Ty::List(Box<Ty>)` translates directly. Recursive struct types unblock future self-host crates (HIR / MIR will need this too).
- **Cons**: +1 week added to Phase H critical path for Phase 7.5 prerequisite; ADR-0050 §A3 already explicitly deferred Phase 7.5 to post-v0.2.0. Loads Phase H with a separate language-feature sprint before the self-host work even begins.
- **Rejected.** Per ADR-0050 §A3, Phase 7.5 has its own dispatch slot; coupling it to Phase H critical path defeats parallel-when-independent CTO mode.

### 2.2 Option B — full Phase H scope under arena workaround (CHOSEN)

- **Pros**: mechanical, uniformly-applied indirection (`Ty::List(Box<Ty>)` → `TyEntry::List(TyId)` where `TyId = i64` indexes into a `vec<TyEntry>` arena). No heap pointers; flat ECS-style layout. Models `indexmap`-backed dict design (ADR-0050d §"Container internals") at the type-universe layer. Port-overhead ~10% LOC per affected file. Phase 7.5 can ship in parallel as a separate ADR for ergonomics without blocking Phase H critical path.
- **Cons**: arena handles add a layer of indirection the Rust impl does not have; `Ty` equality + `Display` impl on the cb side must thread arena access. Parity harness must tolerate arena-id renaming (spike §6 risk 3).
- **Chosen.** Spike §4 recommendation. Phase H minimum-viable with arena; Phase 7.5 ships separately for full ergonomic equivalence + future self-host crates.

### 2.3 Option C — hybrid (Phase 7.5 in parallel; cut over mid-Phase-H)

- **Pros**: arena workaround for Tier-1 sub-ADRs (0055a + 0055b) while Phase 7.5 design spike runs in parallel; cut over to recursive types at Tier-2 (0055c + 0055d) start if Phase 7.5 lands in time.
- **Cons**: two surface forms inside one batch — Tier-1 sub-ADRs ship arena form, Tier-2 sub-ADRs may ship either. Parity harness contract must accept both. Adds cross-sub-ADR coupling.
- **Rejected.** Defeats the F30 SOP cleanliness: a single dispatch wave should not switch ABI mid-flight.

### 2.4 Option D — carved subset (Tier-1 only first; defer Tier-2 to Phase H+)

- **Pros**: smaller batch; Tier-1 (ty.rs + error.rs + lib.rs) ships in ~1 week; Tier-2 (check.rs + infer.rs) deferred to a follow-on sub-batch.
- **Cons**: the §1.1 §4.4 binding is **type checker self-host**, which is `check.rs` — Tier-2. A Tier-1-only ship is `Ty` enum + error enum, not a checker. Fails the §"Done means" bar for Phase H per ADR-0054 §3.1.
- **Rejected.** Per ADR-0054 §3.1 — full crate mirror is the Phase H deliverable, not a Tier-1 carve.

## 3. Decision

Adopt **Option B** — full Phase H scope under the **arena workaround** for recursive struct types. Phase H scope is the full mirror of all 5 source files in `crates/cobrust-types/` into a new workspace member `crates/cobrust-types-cb/` written in Cobrust `.cb` source.

### 3.1 Crate split

- **New workspace member**: `crates/cobrust-types-cb/` (note: `cobrust-types-cb` is the cb mirror; the Rust impl stays at `crates/cobrust-types/` and remains canonical).
- **Production binary still links Rust impl**: `cobrust check` (CLI) + `cobrust build` (translator pipeline) link `crates/cobrust-types/` per spike §1 "Rust impl stays canonical for the foreseeable future."
- **cb mirror is a proof artifact**: compiled by Cobrust itself; differentially-tested against the Rust canonical on the M2 well-typed + ill-typed corpus via the parity harness (ADR-0055e).
- **Phase H "done means"**: cb mirror compiles + passes parity harness on full M2 corpus (well-typed + ill-typed) modulo arena-id renaming per §3.4.

### 3.2 Phase 7.5 disposition (load-bearing decision)

**Recursive struct types remain deferred** (per ADR-0050 §A3). Phase H uses the **arena workaround** uniformly across all sub-ADRs (Tier-1 + Tier-2). No mid-batch cut-over to recursive types per §2.3 Option C rejection. Phase 7.5 may ship in parallel as a separate ADR for future self-host crates (HIR / MIR) but does **not** block Phase H critical path.

### 3.3 Sub-ADR roster (6 sub-ADRs per scoping spike §5)

Scoping spike §5 refined ADR-0054 §3.3's 4-sub-ADR proposal to **6 sub-ADRs** based on empirical file-count + Tier classification:

- **ADR-0055** (this ADR) — Phase H frame. Arena-vs-recursive disposition + crate split + parity-harness contract + completion bar.
- **ADR-0055a** — `ty.rs` cb port (Tier-1; week 1 days 1-3). Arena-based `Ty` enum + `Record` + `FnTy` + `VarAllocator`. Pure data + `Display` impl. Most load-bearing data type for downstream sub-ADRs.
- **ADR-0055b** — `error.rs` + `lib.rs` cb port (Tier-1; week 1 days 3-5). `TypeError` enum + ADR-0052b `suggestion: Option<&'static str>` thread + module exports. Pure data.
- **ADR-0055c** — `infer.rs` cb port (Tier-2; week 2 days 1-4). `Subst` + `unify` + `finalize` over arena `Ty`. Recursive over arena handles.
- **ADR-0055d** — `check.rs` cb port (Tier-2; weeks 2-3). Bidirectional checker over arena. Single largest sub-sprint (2402 LOC).
- **ADR-0055e** — Parity harness contract. M2 corpus diff-test infrastructure (Rust impl vs cb impl). Failure surface per §3.4.

### 3.4 Parity-harness contract (sub-ADR 0055e)

Per spike §6 risk 3, the parity harness lands FIRST (week 1 day 1) as a no-op (both impls produce identical Rust-impl output); cb impl wires in incrementally as ports complete. The failure-surface granularity (spike §7 Q4):

- **All-or-nothing diff on the diagnostic shape**: TypeError variant name + Span (start, end, file) + `suggestion` field + any payload values that are not arena IDs.
- **Arena-id renaming tolerance**: the parity harness canonicalizes `TyId` / `VarId` / `AdtId` integer values to dense-pack form before diff. Two impls may allocate `TyId(7)` vs `TyId(3)` for the same logical type; the harness treats these as equal after canonicalization.
- **Well-typed corpus**: both impls produce `Ok(inferred_ty)` with identical canonicalized `Ty`.
- **Ill-typed corpus**: both impls produce `Err(TypeError)` with identical variant + Span + suggestion + canonicalized payload.

### 3.5 Wave structure

| Wave | Sub-ADRs dispatched in parallel | Duration | DG-load |
|---|---|---|---|
| **Wave 1** | **0055e** parity harness skeleton (no-op baseline) | ~1-2 days | 1 P9 sonnet design + 1 P10-direct PAIR (small surface) |
| **Wave 2** | **0055a** ty.rs cb · **0055b** error.rs + lib.rs cb (parallel; Tier-1) | ~5-7 days | 2 P9 opus design spikes + 2 P10-direct PAIRs |
| **Wave 3** | **0055c** infer.rs cb · **0055d** check.rs cb (parallel; Tier-2; blocks on Wave 2 closure) | ~10-14 days | 2 P9 opus design spikes + 2 P10-direct PAIRs (0055d is largest single sprint) |

**Total Phase H ≈ 2.5-3 weeks** wall-time per scoping spike §2 anchor (tightened from ADR-0054 §3.2's 3-week budget by the 5-files-not-12 anchor).

## 4. Method-by-method scope (full mirror)

Empirical LOC anchors at HEAD `8b4366c` per `wc -l crates/cobrust-types/src/*.rs`:

| File | LOC | Tier | Sub-ADR | Notes |
|---|---|---|---|---|
| `lib.rs` | 61 | Tier-1 | 0055b | Pure module exports + `#![allow]` block. Trivial. |
| `ty.rs` | 407 | Tier-1 | 0055a | `Ty` enum + `Record` + `FnTy` + `VarAllocator`. Pure data + `Display`. **Arena workaround applied here.** |
| `error.rs` | 239 | Tier-1 | 0055b | `TypeError` enum + `Display` + ADR-0052b `suggestion` thread. Pure data. |
| `infer.rs` | 259 | Tier-2 | 0055c | `Subst` + `unify` + `finalize`. Recursive over arena `Ty`. |
| `check.rs` | 2402 | Tier-2 | 0055d | Bidirectional checker. Stateful `Ctx` + 2 `HashMap` (`def_types`, `poly_intrinsic_defs`) + recursive descent over HIR. Largest single-file port. |
| **Total** | **3368** | — | — | |

### 4.1 Per-file feature-gap inventory (spike §3 anchor)

Cobrust-source feature availability at HEAD `8b4366c` per spike §3 table:

| Need | Status | Phase H impact |
|---|---|---|
| `enum` variants (Ty, TypeError) | Shipped (ADR-0050d Dict + ADR-0006 ADT) | None — Ty enum portable under arena form |
| Exhaustive `match` | Shipped (M2 baseline) | None |
| Recursive struct types | **DEFERRED per ADR-0050 §A3 / ADR-0048 §"TD-Recursive-Types Phase 7.5"** | Arena workaround per §5 |
| User-defined traits | **NOT shipped** | Workaround: inline-dispatch via `match` on `Ty` discriminant; no `impl Display for Ty` user-trait form — emit `display_ty(arena, id) -> str` free function instead |
| `HashMap<K, V>` / `HashSet<T>` | Shipped (ADR-0050d as `dict[K, V]` / `set[T]`) | None — `Ctx.def_types` + `Ctx.poly_intrinsic_defs` portable |
| Lifetimes (`&'a T`) | **N/A** (Cobrust uses Drop schedule; no lifetime annotations) | check.rs `&Module` parameter ports to owned-ref or arena handle |
| `Box<T>` heap-alloc | Tied to Phase 7.5 recursive-types | Per §5 arena workaround |
| `Cow<'a, str>` | NOT supported | Use owned `str` (Cobrust strings are GC'd; no clone cost concern) |
| Static atomic counters (`AtomicU32`) | NOT supported | Deferred to ADR-0055a §"Decision" per §7 Q5 |
| `#[derive(Clone, Debug, Hash)]` | Shipped (auto-derive for enum/struct) | None |
| Method-call sugar | Shipped (ADR-0050f Phase G method-form per ADR-0052d) | Improves port ergonomics — `arena.insert(entry)` reads naturally |
| Explicit `&` borrow (ADR-0052a) | Shipped Wave 1 | check.rs's `&Module` / `&mut Ctx` ports cleanly under §2.5 §B Rust-corpus alignment |

## 5. Phase 7.5 dependency disposition (arena workaround detail)

Per spike §4 Option B, recursive struct types deferred per ADR-0050 §A3 are worked around via an arena:

- **Rust impl shape** (HEAD `8b4366c` `crates/cobrust-types/src/ty.rs::Ty (Tuple / List / Set / Dict recursive variants)`):
  ```rust
  Tuple(Vec<Ty>),
  List(Box<Ty>),
  Set(Box<Ty>),
  Dict(Box<Ty>, Box<Ty>),
  ```
- **cb mirror shape** (under arena):
  ```
  Tuple(list[i64])      # list of TyId arena handles
  List(i64)             # single TyId handle
  Set(i64)              # single TyId handle
  Dict(i64, i64)        # key TyId, value TyId
  ```
- **Arena**: `TyArena` is a `vec<TyEntry>` where `TyEntry` is the same enum without recursive variants (recursion replaced by `TyId = i64` indexing into the arena). `Ty` becomes a thin handle `i64` paired with an `&TyArena`.
- **Equality**: structural equality on the cb side dereferences arena handles transitively. Cycle detection unnecessary (Phase H types are tree-shaped per ADR-0006 §"Type universe" — no cyclic types).
- **Display**: parametric in `&TyArena` — the cb mirror's `display_ty(arena, id) -> str` mirrors Rust's `impl Display for Ty`.

**Re-evaluation gate**: spike §6 risk 1 mitigation. ADR-0055 frame ratifies arena disposition with a re-evaluation gate at Tier-2 sub-ADR (0055c / 0055d) start. If Tier-1 (0055a + 0055b) experience surfaces unworkable arena cost, the Tier-2 wave dispatch prompt may revisit (escalate to ADR-0055 amendment + Phase 7.5 prerequisite). Default: arena disposition holds.

## 6. Pre-dispatch acceptance gate (spike §8 binding)

Phase H first sub-sprint (ADR-0055e parity harness skeleton, then 0055a `ty.rs` cb port) dispatches only when:

- [ ] **Arena vs Phase 7.5 disposition decided** — §3.2 + §5 above answer spike §7 Q1. **Done in this ADR.**
- [ ] **Phase G fully closed** — Wave 2 round 2 in-flight (ADR-0052d method-call sugar impl) must close. v0.3.0 tag bound per ADR-0054 §12.3.
- [ ] **`cobrust-cb` compile-and-diff infrastructure spike landed** — a minimal `cobrust build` → `cobrust run` pipeline against a 50-LOC `.cb` smoke file lands as a separate pre-Phase-H spike. Validates Cobrust toolchain can self-build before Phase H consumes it.
- [ ] **Parity harness skeleton landed (Wave 1)** — ADR-0055e infra (no cb impl yet) lands first per spike §6.3 risk mitigation; produces "both impls = Rust impl, diff = empty" baseline before Wave 2 dispatch.
- [ ] **5 spike §7 questions resolved**: (Q1 arena, answered in §3.2); (Q2 Span cb-representation, deferred to ADR-0055b §"Decision"); (Q3 crate scope, full mirror per §3.1); (Q4 parity-harness failure-surface, all-or-nothing per §3.4); (Q5 `AtomicU32` VarAllocator port, deferred to ADR-0055a §"Decision").

## 7. Risk register

Top 3 risks ranked by Phase H critical-path exposure (per spike §6):

1. **Recursive struct types blocker (§5)** — disposition (Option B arena) is ratified in §3.2; re-evaluation gate at Tier-2 start retains optionality. Mitigation: arena uniformly applied per §5; if Tier-1 experience surfaces unworkable cost, Tier-2 dispatch prompt revisits.
2. **`Span` Drop semantics** — `cobrust_frontend::span::Span` is consumed by-value in many `TypeError` variants. Cobrust strings + spans are GC'd / ref-counted, not value types. The cb mirror's `TypeError::Foo { span: Span }` may behave differently under Cobrust's Drop schedule than Rust's move semantics. Mitigation: ADR-0055b explicit `Span` Cobrust-representation subsection (value-type Cobrust struct vs arena-handle); differential test on a corpus that exercises Span propagation across nested errors (e.g. `TypeError::Multiple`).
3. **Parity harness infrastructure** — no existing "two-impl differential test" harness in the repo. Building one (ADR-0055e) needs: (a) common error-shape contract (Rust + cb both serialize TypeError to JSON); (b) corpus runner that compiles cb sources via `cobrust build` then invokes the cb checker on M2 corpus inputs; (c) diff comparator with arena-id renaming tolerance. Mitigation per §3.5 Wave 1: parity harness lands FIRST as no-op; cb impl wires in incrementally.

Additional Phase-H-specific risks beyond spike §6:

4. **Cobrust language-feature gap surfacing mid-port** — Phase H stresses every language feature simultaneously (enums, recursive types via arena, exhaustive match, HashMap-as-dict, HashSet-as-set, Display rendering). Phase G+ ergonomic gaps the test corpus did not surface may emerge. Budget +0.5 week buffer per ADR-0054 §3.4.
5. **`AtomicU32` VarAllocator port** — spike §7 Q5. Rust impl uses static `AtomicU32` counter; Cobrust runtime offering AtomicU32 type is TBD. Mitigation: ADR-0055a §"Decision" enumerates instance-field counter (port-friendly, loses cross-checker uniqueness guarantee) vs Cobrust thread-local atomic (depends on runtime support).

## 8. Consequences

### 8.1 Positive

- §4.4 self-hosting binding becomes operational reality after ~9 weeks of deferral (ADR-0052 §"Out of scope" F-G.2 amendment delayed activation from post-M5 to post-M11; this ADR ratifies that delayed activation as Phase H).
- The cb mirror serves dual purposes: (a) proof artifact per §1.1; (b) training-data corpus for §2.5 §B "training-data overlap" — every future Cobrust translation can learn from a body of Cobrust code that mirrors a well-understood Rust crate.
- Parity harness (ADR-0055e) is the first "two-impl differential test" infrastructure in the repo. Future self-host crates (HIR / MIR / codegen post-Phase-L) reuse the same shape per ADR-0054 §11 out-of-scope.
- Arena disposition (§5) lets Phase H proceed without blocking on Phase 7.5; Phase 7.5 ships separately for ergonomics + future self-host crates without coupling.
- Phase H's 6-sub-ADR roster sets the precedent for milestone-layer batch ADRs spawning a Wave-0 infra sub-ADR (0055e) ahead of Tier-1 + Tier-2 ports.

### 8.2 Negative

- 6 sub-ADRs + parity-harness infrastructure = heaviest doc-tree commitment of any Phase to date (vs Phase G's 4 sub-ADRs + 1 prereq ADR). Doc-coverage gate (constitution §3.3) load is ~6 × triple-doc (zh + en + agent) = 18 doc surfaces.
- 0055d (`check.rs` cb port, 2402 LOC) is the largest single sub-sprint in project history. P9 design spike for 0055d alone may run multi-day before P10-direct PAIR dispatch.
- Arena workaround (§5) introduces a layer of indirection the Rust impl does not have. cb mirror is not a 1:1 line-for-line translation; semantic equivalence holds (via parity harness) but syntactic divergence is real. Phase J LSP dogfooding (ADR-0054 §3.1) sees the arena form, not the recursive-type form Phase 7.5 would deliver.
- Parity harness (ADR-0055e) requires `cobrust build` + `cobrust run` pipeline to be production-quality before Phase H Wave 2 dispatches. Pre-Phase-H "cobrust-cb compile-and-diff infrastructure spike" (per §6 acceptance gate) is a hidden prerequisite that may slip Wave 2.

### 8.3 Neutral / unknown

- Whether `Span` should be a value-type Cobrust struct or an arena-handle (`SpanId` indexing into a `SpanArena`). Deferred to ADR-0055b §"Decision" per §6 spike Q2.
- Whether `AtomicU32 VarAllocator` ports to instance-field counter or Cobrust thread-local atomic. Deferred to ADR-0055a §"Decision" per §6 spike Q5.
- Whether the cb mirror retains `#[derive(Clone, Debug, Hash)]`-equivalent auto-derive semantics 1:1 with Rust. ADR-0050d (dict design) presumes auto-derive for dict keys; cb mirror inherits. Surface stays consistent.
- Whether AST printer self-host (ADR-0054 §3.1 §4.4 "second target") lands as Phase H+ micro-sprint or defers to post-Phase-L. Depends on Phase H closure budget; spike §2 anchors a ~0.5-week buffer that could absorb AST-printer port if Tier-2 closes cleanly.

## 9. Dispatch readiness

Phase H Wave 1 (ADR-0055e parity harness skeleton) dispatches after:

1. ADR-0055 (this frame) ratifies on first sub-ADR dispatch per `ratification_path` frontmatter.
2. Phase G Wave 2 round 2 closes per §6 pre-dispatch acceptance gate.
3. Pre-Phase-H `cobrust-cb compile-and-diff infrastructure spike` lands as a separate ADR-0055-prereq deliverable.

Wave 2 (0055a + 0055b parallel) dispatches after Wave 1 baseline diff = empty confirmed.

Wave 3 (0055c + 0055d parallel) dispatches after Wave 2 closure + Tier-2 re-evaluation gate (per §5) confirms arena disposition holds.

### 9.1 Host routing (per `feedback_heavy_build_offload_to_workstation.md`)

| Sprint | Host | Mode |
|---|---|---|
| 0055e parity harness skeleton design (doc-only) | Mac local | direct |
| 0055e parity harness skeleton impl | DG primary | Mode C |
| 0055a ty.rs cb port (design + impl) | DG primary | Mode C |
| 0055b error.rs + lib.rs cb port (design + impl) | DG primary | Mode C |
| 0055c infer.rs cb port (design + impl) | DG primary | Mode C |
| 0055d check.rs cb port (design + impl) | DG primary | Mode C |

Every `cargo build --workspace` + `cobrust build` invocation runs on DG per heavy-build offload binding policy. Mac local stays for doc-only spikes + targeted single-crate parity-harness reviews.

### 9.2 Documentation mandate (per constitution §3)

Each sub-ADR's commit ships triple-doc updates per ADR-0052 §"Documentation mandate" precedent:

| Sub-ADR | Bilingual surfaces added |
|---|---|
| 0055e | new `docs/human/{zh,en}/self-host.md` §"Parity harness — Rust vs cb diff testing" |
| 0055a | new `docs/human/{zh,en}/self-host.md` §"Type universe under arena workaround" + `design-philosophy.md` §"Why arena, not recursive types (yet)" |
| 0055b | `error-reference.md` revisit: every error variant cross-referenced to cb mirror |
| 0055c | `self-host.md` §"Inference engine self-host" |
| 0055d | `self-host.md` §"Bidirectional checker self-host" + retrospective on §4.4 binding |

## 10. Why this ADR now

- Phase G Wave 2 round 2 is near-closure (ADR-0052d method-call sugar impl remaining); v0.3.0 tag boundary approaches.
- ADR-0054 §10 explicitly named ADR-0055 (Phase H frame) as a pre-Phase-H prep deliverable: "Self-host type checker scoping ADR-0055-prereq (~1 week, Mac-local doc-only): which types + which `cobrust-types` modules get the `.cb` mirror first."
- The scoping spike (`docs/agent/dispatches/2026-05-18-phase-h-self-host-scoping.md`) is fresh (authored 2026-05-18 at `bc10842`); empirical anchors (5 files / 3368 LOC vs ADR-0054's 12 files / 5500 LOC estimate) need ADR-codification before Wave 1 dispatch. Without this frame, sub-ADRs 0055a..e would re-derive scope from the spike + ADR-0054 in each spike, risking drift across the 6 parallel dispatches.
- The arena-vs-recursive disposition (§5) is the load-bearing decision affecting all 5 sub-ADRs. Codifying it ex-ante per CTO operating instruction "default to proceed" + "ADR-or-it-didn't-happen" prevents per-sub-ADR re-litigation.
- Phase H's 6-sub-ADR roster + Wave 0 infra precedent (parity harness landing FIRST) is a new pattern; Phase J/K/L inherit it for similarly-large self-host-style work.

— P9 Tech Lead, 2026-05-18
