---
doc_kind: dispatch-scoping
title: "Phase H self-host scoping spike — crates/cobrust-types-cb feasibility analysis"
status: scoping-spike (NOT a ratified ADR)
date: 2026-05-18
author: P9 Tech Lead
parent_adr: 0054
intends_to_inform: ADR-0055 (Phase H frame) + ADR-0055a/b/c/d/e
last_verified_commit: bc10842
---

# Phase H self-host scoping spike

This is a **scoping spike**, not a ratified ADR. It informs ADR-0055 (Phase H
frame ADR) ex-ante per ADR-0054 §10 "Pre-Phase-H prep work" third bullet.

## 1. Goal

Rewrite `crates/cobrust-types` (HEAD `bc10842`: 5 source files, 3320 LOC) in
Cobrust source (`.cb`) under a new `crates/cobrust-types-cb/` directory.

- **Rust impl stays canonical** for the foreseeable future. The `cobrust check`
  binary still links the Rust crate; the production type checker does not move
  to `.cb`.
- The `.cb` mirror is a **proof artifact** + **differential-test corpus**: it
  proves Cobrust self-host feasibility per CLAUDE.md §4.4 and produces a body
  of Cobrust code the LSP (Phase J) and translation pipeline can dogfood
  against.
- A parity harness (ADR-0055e) runs the M2 well-typed + ill-typed corpus
  through both impls and diff-tests outputs.

## 2. Module-by-module scope

Empirical anchor: `wc -l crates/cobrust-types/src/*.rs` at HEAD `bc10842` =
5 files / 3320 LOC (NOT 12 files / 5500 LOC as ADR-0054 §3.1 estimated — the
estimate-vs-actual delta tightens Phase H wall-time by ~30%).

| File | LOC | Tier | Migrate-when | Notes |
|---|---|---|---|---|
| `lib.rs` | 61 | **Tier-1** | First — week 1 day 1 | Pure module exports + `#![allow]` block. Trivial. |
| `ty.rs` | 407 | **Tier-1** | Week 1 days 1-3 | `Ty` enum + `Record` + `FnTy` + `VarAllocator`. Pure data + `Display` impl. Most load-bearing data type. |
| `error.rs` | 239 | **Tier-1** | Week 1 days 3-5 | `TypeError` enum + `Display` + ADR-0052b `suggestion` field threading. Pure data. |
| `infer.rs` | 259 | **Tier-2** | Week 2 days 1-4 | `Subst` + `unify` + `finalize`. Recursive over `Ty`. Needs heap-alloc + recursion. |
| `check.rs` | 2354 | **Tier-2** | Weeks 2-3 | Bidirectional checker. Stateful `Ctx` + 2 `HashMap` + recursive descent over HIR. Largest single-file port. |

Tier-3 files (e.g. `validate.rs`, `normalize.rs`) mentioned in dispatch prompt:
**do not exist** at HEAD `bc10842`. The types crate has exactly 5 source files
listed above. Phase H scope is full mirror of these 5.

**Total wall-time estimate**: ~2.5 weeks (vs ADR-0054 §3.2's 3-week budget;
~0.5 week buffer absorbed by smaller-than-estimated LOC).

## 3. Cobrust-source feature gaps

Inventory of what's blocked vs. shipped at HEAD `bc10842`:

| Need | Status | Phase H impact |
|---|---|---|
| `enum` variants (Ty enum, TypeError enum) | Shipped (ADR-0050d Dict + ADR-0006 ADT) | None — `Ty` enum portable |
| Recursive struct types (`Ty::List(Box<Ty>)`, `Ty::Tuple(Vec<Ty>)`) | **DEFERRED per ADR-0048 §"TD-Recursive-Types Phase 7.5"** | **Hard blocker for naive port; see §4** |
| Exhaustive `match` | Shipped (M2 baseline) | None |
| User-defined traits | **NOT shipped** (method-dispatch ADR-0052d-prereq landed; user-trait surface absent) | Workaround: inline-dispatch via `match` on `Ty` discriminant; no `impl Display for Ty` user-trait form |
| `HashMap<K, V>` | Shipped as `dict[K, V]` (ADR-0050d) | None — `Ctx.def_types` portable |
| `HashSet<T>` | Shipped as `set[T]` (ADR-0050d) | None — `Ctx.poly_intrinsic_defs` portable |
| Lifetimes (`&'a T`) | **N/A** (Cobrust uses Drop schedule; no lifetime annotations) | check.rs uses `&Module` parameter — port to owned-ref or arena handle |
| `Box<T>` heap-alloc | Tied to Phase 7.5 recursive-types | See §4 workaround |
| `Cow<'a, str>` | NOT supported | Use owned `str` (Cobrust strings are GC'd; no clone cost concern) |
| Static atomic counters (`AtomicU32`) | NOT supported | `VarAllocator` uses thread-local mut counter; port to instance field |
| `#[derive(Clone, Debug, Hash)]` | Shipped (auto-derive for enum/struct) | None |
| Method-call syntax (`s.split(",")`) | Shipped (ADR-0050f Phase G method-form) | Improves port ergonomics |
| Explicit `&` borrow (ADR-0052a) | Shipped Wave 1 | check.rs's `&Module` / `&mut Ctx` ports cleanly |
| String interning / `Span` Drop | Cobrust Span: TBD if value or arena-handle | **Risk** — see §6 |

## 4. Phase 7.5 dependency (the load-bearing question)

**Yes**, recursive struct types (ADR-0048 §"TD-Recursive-Types Phase 7.5"
deferred) is a hard blocker for the naive port of `Ty::List(Box<Ty>)` +
`Ty::Tuple(Vec<Ty>)` + `Ty::Dict(Box<Ty>, Box<Ty>)`.

Three options:

- **Option A — Ship Phase 7.5 before Phase H**: clean port; ~1 week added to
  Phase H critical path. Recursive-types unblocks future self-host crates too
  (HIR, MIR will need this anyway).
- **Option B — Arena workaround**: replace `Ty` enum variants holding `Box<Ty>`
  with an `i64` index into a `vec<TyEntry>` arena. `Ty::List(Box<Ty>)` becomes
  `TyEntry::List(TyId)` where `TyId = i64`. No heap pointers; flat ECS-style
  layout. ~2 days port-overhead per file (ty.rs + infer.rs + check.rs each gain
  ~10% LOC for arena indirection).
- **Option C — Hybrid**: ship Phase 7.5 in parallel as ADR-0055-prereq-a (Mac
  doc-only spike) while Phase H Tier-1 ports proceed via arena workaround; cut
  over to recursive-types at Tier-2 start if Phase 7.5 lands in time.

**Recommendation**: **Option B (arena workaround) for Phase H minimum-viable**.
Phase 7.5 ships separately as its own ADR for ergonomics + future self-host
crates. Rationale: arena indirection is uniformly applied + mechanical; Phase
7.5 unblocks future work but is not strictly required for the §1 proof artifact
goal.

## 5. Sub-ADR roster

Phase H frame ADR-0055 spawns these sub-ADRs:

- **ADR-0055** — Phase H frame: arena-vs-recursive disposition (per §4 above),
  crate split (`cobrust-types-cb/` as workspace member), parity-harness
  contract, completion bar (= what makes Phase H "done").
- **ADR-0055a** — `ty.rs` cb port (Tier-1; week 1 days 1-3). Arena-based `Ty`
  enum + Record + FnTy + VarAllocator.
- **ADR-0055b** — `error.rs` + `lib.rs` cb port (Tier-1; week 1 days 3-5).
  TypeError enum + ADR-0052b `suggestion` thread.
- **ADR-0055c** — `infer.rs` cb port (Tier-2; week 2 days 1-4). Subst + unify +
  finalize over arena Ty.
- **ADR-0055d** — `check.rs` cb port (Tier-2; weeks 2-3). Bidirectional checker
  over arena. Single largest sub-sprint.
- **ADR-0055e** — Parity harness: M2 corpus diff-test infrastructure. Rust impl
  vs cb impl on the well-typed + ill-typed corpora. Failure surface = "any
  diagnostic divergence" (TypeError variant + Span + suggestion all must
  match modulo arena-id renaming).

ADR-0054 §3.3 originally proposed 4 sub-ADRs (frame + a/b/c). This scoping
spike refines to **6 sub-ADRs (frame + a/b/c/d + parity harness)** based on
empirical file-count + Tier classification.

## 6. Risk register

Top 3 risks ranked by Phase H critical-path exposure:

1. **Recursive struct types blocker (§4)** — disposition is the load-bearing
   ADR-0055 frame decision. Wrong call (Option A when B suffices, or B when
   ergonomics demand A) cascades through all Tier-2 sub-ADRs. Mitigation:
   ADR-0055 frame ratifies disposition with a re-evaluation gate at Tier-2
   start.
2. **Span Drop semantics** — `cobrust_frontend::span::Span` is consumed
   by-value in many `TypeError` variants. Cobrust strings + spans are GC'd /
   ref-counted, not value types. The cb mirror's `TypeError::Foo { span: Span }`
   may behave differently under Cobrust's Drop schedule than Rust's move
   semantics. Mitigation: ADR-0055b explicit `Span` Cobrust-representation
   subsection; differential test on a corpus that exercises Span propagation
   across nested errors (e.g. `TypeError::Multiple`).
3. **Parity harness infrastructure** — there is no existing "two-impl
   differential test" harness in the repo. Building one (ADR-0055e) needs:
   (a) common error-shape contract (Rust + cb both serialize TypeError to
   JSON); (b) corpus runner that compiles cb sources via `cobrust build` then
   invokes the cb checker on M2 corpus inputs; (c) diff comparator with
   arena-id renaming tolerance. Mitigation: parity harness lands FIRST (week 1
   day 1) as a no-op (both impls produce identical Rust-impl output); cb impl
   wires in incrementally as ports complete.

## 7. Decision points before Phase H frame ADR-0055 commits

Open questions ADR-0055 must answer:

1. **Arena vs. Phase 7.5 disposition (§4)** — Option A, B, or C? Binding
   decision for all Tier-1 + Tier-2 ports.
2. **`Span` cb-representation** — value-type (Cobrust struct) or arena-handle
   (i64 into a `SpanArena`)? Affects every TypeError variant.
3. **Crate scope** — full mirror (5 files, 3320 LOC) or carved subset (Tier-1
   only first, defer Tier-2 to Phase H+)? ADR-0054 §3.1 implies full; this
   spike's §2 LOC anchor supports full within 2.5-week budget.
4. **Parity-harness failure-surface granularity** — `TypeError` variant +
   span + suggestion all-or-nothing diff, or per-field weighted diff? Affects
   ADR-0055e contract.
5. **`AtomicU32` VarAllocator port** — instance-field counter (port-friendly,
   loses cross-checker uniqueness guarantee) or Cobrust thread-local atomic
   (depends on Cobrust runtime offering AtomicU32 type, currently TBD)? Affects
   ty.rs Tier-1 port.

## 8. Pre-dispatch acceptance gate

Phase H first sub-sprint (ADR-0055a `ty.rs` cb port) dispatches only when:

- [ ] **Phase 7.5 disposition decided** — §7 Q1 answered in ADR-0055 frame.
- [ ] **Phase G fully closed** — Wave 2 round 2 in-flight (ADR-0052d
      method-call sugar impl) must close. v0.3.0 tag bound per ADR-0054 §12.3.
- [ ] **cobrust-cb compile-and-diff infrastructure spike landed** — a minimal
      `cobrust build` → `cobrust run` pipeline against a 50-LOC `.cb` smoke
      file lands as a separate pre-Phase-H spike. Validates Cobrust toolchain
      can self-build before Phase H consumes it.
- [ ] **ADR-0055 frame ratified** — 5 §7 questions resolved + risk-register §6
      mitigations committed.
- [ ] **Parity harness skeleton landed** — ADR-0055e infra (no cb impl yet)
      lands first per §6.3 risk mitigation; produces "both impls = Rust impl,
      diff = empty" baseline.

— P9 Tech Lead, 2026-05-18
