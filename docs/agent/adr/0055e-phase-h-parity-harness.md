---
doc_kind: adr
adr_id: 0055e
title: "Phase H parity harness contract — Rust impl vs cb impl diff-test on M2 corpus"
status: proposed
date: 2026-05-18
last_verified_commit: 929cd4af24b614853dd73a1db96835553fea235c
supersedes: []
superseded_by: []
relates_to: [adr:0055, adr:0054]
parent_adr: 0055
ratification_path: in-session review per ADR-0050 §"Audit model — teammate-in-session"; this ADR ratifies on its own impl merge (parity-harness skeleton lands first per ADR-0055 §3.5 Wave 1)
---

# ADR-0055e: Phase H parity harness contract

## 1. Context

ADR-0055 §3.3 lists six sub-ADRs for Phase H and places **0055e first** in the Wave-1/2/3 dispatch plan (§3.5): the parity-harness skeleton lands ahead of every Tier-1 + Tier-2 cb port so that each port wires into a working diff-test infrastructure as it merges.

- The Rust impl at `crates/cobrust-types/` is canonical (per ADR-0055 §3.1) and remains the production type checker linked by `cobrust check`.
- The cb mirror at `crates/cobrust-types-cb/` (future, lands per 0055a-d) is a **proof artifact + training-data corpus** per ADR-0055 §1.1 + §8.1.
- The parity-harness contract this ADR ratifies is the **only** mechanism that proves the cb mirror is semantically equivalent to the Rust impl. Without it, Phase H's "done means" bar (ADR-0055 §3.1 closing sentence) is unfalsifiable.

Two arena-form properties of the cb mirror (per ADR-0055 §5) impose harness obligations the prior translation-pipeline differential tests (ADR-0039) do not address:

- **Arena-id renaming**: `TyId(7)` on the Rust side may legitimately correspond to `TyId(3)` on the cb side for the same logical type. Naive bit-equality fails.
- **Two-impl runner**: the harness runs the Rust crate via Cargo and the cb crate via `cobrust build` + `cobrust run`; orchestrating both binaries and merging their JSON outputs is new infrastructure.

This ADR ratifies the parity-granularity choice, the arena-id renaming tolerance algorithm, the test corpus, the failure surface, and the phased implementation schedule.

## 2. Decision — parity granularity

ADR-0055 §3.4 names the contract "all-or-nothing diff on the diagnostic shape" but defers the **granularity unit** (what is one diff invocation) to this ADR. Two options:

- **Per-input granularity (CHOSEN)** — feed a single HIR Module into both checkers; assert outputs identical modulo arena-id renaming. One diff invocation = one input file. Test-runner emits one assertion per file in the M2 corpus.
- **Per-module granularity (rollup-only)** — run the full M2 corpus through both checkers as a single batch; compare the final `TypeCtx` rollup (count of accepts, count of rejects, aggregate `def_types` size). One diff invocation = one M2 run. Useful as a smoke-check, not as a primary contract.

**Decision: per-input granularity is the primary contract. Per-module granularity is reported as a rollup smoke-check only.**

Rationale:

- **Tighter feedback**: when the cb impl diverges, per-input granularity points to the exact `.cb`/`.py`-source input that diverged. Per-module rolls up into "5 mismatches somewhere in 250 files" which is unactionable.
- **Wave-2/3 incremental wiring**: as each cb port lands (0055a → 0055b → 0055c → 0055d), the per-input harness can mark currently-unsupported features as `cfg(skip)` per input. Per-module forces all-or-nothing wiring which is incompatible with the Wave-2-then-Wave-3 ratchet.
- **Failure-localization audit**: when a divergence surfaces, the per-input diff already names the offending input. No re-running needed.

The per-module rollup is reported as a sanity log line at the end of the harness invocation (`Total: Rust accept=N1 reject=M1, cb accept=N2 reject=M2`) so a divergence in counts is visible at-a-glance even before per-input details.

## 3. Arena-id renaming tolerance

Arena ids (`TyId`, `VarId`, `AdtId`, `AliasId`, `GenericVar`) are not stable across runs or impls. The Rust impl uses `AtomicU32::fetch_add(Ordering::Relaxed)` for `VarId` allocation; the cb impl per ADR-0055a may use an instance-field counter (spike §7 Q5). Allocation order differs whenever the visitor walk order differs by a single statement; even within a single impl, two runs may allocate different ids depending on thread interleaving.

**Comparison algorithm — canonical post-order traversal:**

1. Each impl emits its TypeError + inferred `Ty` outputs as JSON with raw arena ids in place.
2. The harness post-processes each impl's JSON: it performs a **post-order traversal** over every `Ty` and `Span` node, assigning a dense-pack canonical id (0, 1, 2, ...) in traversal order. All references to that node within the same output get the canonical id substituted.
3. Output as `(kind, child-canonical-ids)` tuples — i.e., a `Ty::List(TyId(7))` whose target is a `Ty::Int` becomes `("List", [child_0])` where `child_0` = `("Int", [])`.
4. Diff the canonical tuples lexicographically.

Properties:

- **Sound**: structurally-equivalent types canonicalize identically; structurally-divergent types canonicalize differently. Cycle detection unnecessary per ADR-0055 §5 (Phase H types are tree-shaped — no cyclic types per ADR-0006 §"Type universe").
- **Tight**: does not over-tolerate. Two `Ty::List(TyId)` where one points to `Int` and the other to `Str` canonicalize to `("List", [("Int",[])])` vs `("List", [("Str",[])])` — a real divergence, surfaced.
- **Deterministic**: post-order traversal order is fixed by the harness; both impls' raw outputs are canonicalized by the same code path (not by the impls themselves). Eliminates impl-side canonicalization-bug risk.
- **Idempotent**: canonicalizing a canonicalized output yields itself. Useful for snapshot-test integration (`insta` reuse per §5).

`VarId` canonicalization follows the same rule — `Var(VarId(7))` becomes `Var(canonical_0)` in first-encounter order. If the cb impl unifies in a different order and produces `Var(VarId(3))` for the same logical inference variable, canonicalization aligns them. `AdtId` + `AliasId` + `GenericVar` follow analogously, each with its own dense-pack canonical namespace (no cross-namespace renaming).

**Amendment 2026-05-18 (per ADR-0055a §8 F1 cross-ADR dep)**: 0055a §3 introduces 2 parallel arenas (`FnTyArena` + `RecordArena`) beyond the single TyArena parent §5 specified. Canonical-namespace post-order traversal extends to 5 namespaces: TyId + AdtId + AliasId + FnTyId + RecordId. Each namespace canonicalizes independently before cross-namespace consistency check. `VarId` and `GenericVar` remain auxiliary canonicalization namespaces (not counted in the 5 primary arenas) and follow the same dense-pack rule as before.

**Out of scope for canonicalization**: `Span` byte offsets — these come from the parser and must match raw (no canonicalization). The harness asserts raw `Span` equality on every TypeError variant. If the parser is shared between impls (cb mirror reuses Rust frontend per ADR-0055 §"Crate split"), `Span` equality holds trivially. If a future sub-ADR ports the parser too, `Span` becomes a canonicalization concern and this ADR amends.

**Algorithm complexity**: O(N) where N is total node count across all `Ty` / `Span` outputs for one input. Phase H corpus expected to peak at ~10K nodes per input (deeply-nested generics + tuples); O(N) is comfortably under any plausible budget.

## 4. Test corpus

Reuse existing M2 test programs:

- `crates/cobrust-types/tests/well_typed.rs` — N well-typed programs (current count confirmed during Phase 1 skeleton work; HEAD `1fbed82` exposes the file but counting at write-time is out of scope for a doc-only ADR).
- `crates/cobrust-types/tests/ill_typed.rs` — M ill-typed programs, each annotated with the expected `TypeError` variant.

**Both checkers must produce identical accept/reject + identical error variants on identical inputs.** The harness runs every program through both impls and diffs canonical outputs per §3.

**Input representation**: HIR Module (post-parse + post-resolve). Both impls consume HIR directly, not source text. This avoids re-implementing the lexer + parser + name-resolver on the cb side (the cb mirror is `cobrust-types`-equivalent only — frontend stays Rust per ADR-0055 §3.1).

**Why HIR not source text**:

- The cb mirror's input is HIR per ADR-0055 §3.1 — the production type checker consumes HIR. Feeding HIR to both impls matches production reality.
- Source-text inputs would force a frontend re-port (lexer + parser + name-resolver) that ADR-0055 §3.1 explicitly excludes from Phase H scope.
- HIR is structurally stable across the cb port — `cobrust_hir::Module` is a Rust type; the cb mirror imports it via FFI or, in Phase 3, the cb runner deserializes a pre-computed HIR JSON.

**Corpus extension policy**: as Phase H Tier-1 + Tier-2 ports land, each sub-ADR may extend the corpus with arena-form-specific inputs (e.g., deeply-nested `List[List[List[Int]]]` to exercise arena indirection; chained `unify` calls across distinct `VarId` allocations; `Span` propagation through `TypeError::Multiple` aggregation). Corpus additions land in the same commit as the sub-ADR's impl per ADR-0052 atomic-commit precedent.

**Corpus stability gate**: any corpus addition that fails the Phase 2 Rust-vs-Rust sanity check (i.e., the addition itself surfaces a canonicalization bug) blocks the sub-ADR's merge until the canonicalization algorithm is calibrated. Per §10 risk 1, this is the load-bearing self-test.

## 5. Harness location

New Rust workspace member: `crates/cobrust-types-parity/`.

- **Why a separate crate** (not a `[[test]]` in `cobrust-types`): the harness depends on **both** `cobrust-types` (Rust canonical) and `cobrust-types-cb` (cb mirror, future). A `cobrust-types` integration test cannot depend on a future workspace member without introducing a cyclic dev-dependency.
- **Crate kind**: `[lib]` + single `[[test]]` binary named `parity`. The lib exposes the canonicalization algorithm (§3) as a reusable function; the test binary drives the corpus.
- **Why NOT cb**: the harness orchestrates both impls; it must compile + invoke the cb toolchain. Self-hosting the harness itself is post-Phase-L scope per ADR-0054 §11.
- **Dependencies**: `cobrust-types` (path), `cobrust-types-cb` (path, optional under feature `cb-impl`), `cobrust-hir` (path), `cobrust-frontend` (path), `serde_json` (canonical-output diff format), `insta` (snapshot-test infra reuse from existing parity-style tests).

Single integration test binary `tests/parity.rs` enumerates the M2 corpus, runs each input through both impls (via direct Rust calls + via `std::process::Command` on `cobrust run` for the cb side), and asserts canonical equality per §3.

**Crate-layout sketch** (subject to ADR-0055e Phase 1 impl):

```
crates/cobrust-types-parity/
├── Cargo.toml
├── src/
│   ├── lib.rs           # canonicalization API (public)
│   ├── canon.rs         # post-order traversal + dense-pack renaming
│   ├── corpus.rs        # M2 corpus enumeration
│   └── runner.rs        # Rust-side + cb-side impl drivers
└── tests/
    └── parity.rs        # integration test binary
```

**Feature flags**:

- `cb-impl` (default-off until 0055a + 0055d land) — gates the cb-side runner. When off, the harness runs Rust-vs-Rust per §8 Phase 2.
- `snapshot` (default-on) — gates the `insta` snapshot machinery. Off-by-default would lose CI value; on-by-default keeps the harness loud about drift.

## 6. Failure surface

**All-or-nothing per-input**. Any divergence on any input → BLOCK Phase H ratification.

Concrete failure rules (per ADR-0055 §3.4 binding):

- Accept/reject divergence (Rust says Ok, cb says Err, or vice versa) → BLOCK.
- TypeError variant name divergence (Rust says `ImplicitTruthiness`, cb says `TypeMismatch`) → BLOCK.
- `Span` raw byte-offset divergence on any TypeError variant → BLOCK.
- `suggestion` field divergence (Rust says `"change to 'if x != 0:'"`, cb says `None`) → BLOCK.
- Canonical `Ty` payload divergence on any TypeError payload or on any well-typed inferred type → BLOCK.

The harness does **not** attempt partial-credit weighted diff. A single per-input divergence fails the test binary; CI fails; Phase H ratification halts; the offending sub-ADR (whoever's port introduced the divergence) reopens.

**Tolerance**: arena-id renaming per §3 is the **only** tolerance. No tolerance for `Span` drift, no tolerance for variant rename, no tolerance for `suggestion` field absence.

## 7. Pre-dispatch acceptance gate

Phase H Wave 1 (this ADR's impl skeleton) dispatches independently of Tier-1/2 ports — there is no port to verify yet. Required only:

- [ ] **ADR-0055 frame accepted** — ratifies on first sub-ADR dispatch per its `ratification_path`. This ADR (0055e) is the "first sub-ADR" per ADR-0055 §3.5 Wave 1. ADR-0055 thus ratifies on this ADR's impl merge.
- [ ] **No dependency on cb impl** — Phase 1 (skeleton + canonicalization) + Phase 2 (Rust-vs-Rust sanity) ship before any cb port exists. The harness's diff-empty baseline is "Rust impl vs itself, output identical" — a tautological pass that exercises the canonicalization algorithm and the corpus runner without requiring `cobrust-types-cb` to exist.
- [ ] **M2 corpus stable** — `well_typed.rs` + `ill_typed.rs` test sources at HEAD `1fbed82` are the harness input source. Corpus drift between Phase 1 skeleton and Wave 2 dispatch is tolerable (new entries land per §4 corpus-extension policy).

No dependency on Phase G closure, no dependency on `cobrust-cb compile-and-diff infrastructure spike` (that spike is Phase 3 prerequisite per §8). Phase 1 + Phase 2 are Rust-only work.

## 8. Implementation phases

| Phase | Scope | Estimated wall-time | Blocks on |
|---|---|---|---|
| **Phase 1** | Harness skeleton (`crates/cobrust-types-parity/`) + canonicalization algorithm (§3) + corpus runner enumerating `well_typed.rs` + `ill_typed.rs` + JSON serialization of canonical tuples | ~3 days | Nothing — Rust-only |
| **Phase 2** | Rust-vs-Rust sanity — both "impls" are the same Rust `cobrust_types::check`. The diff must be empty on the full M2 corpus. Validates canonicalization correctness + corpus runner end-to-end | ~1 day | Phase 1 |
| **Phase 3** | cb-side wiring — deferred until 0055a (`ty.rs` cb port) + 0055d (`check.rs` cb port) land. Harness invokes the cb checker via `std::process::Command` + `cobrust run`; merges JSON output with the Rust side; runs canonical diff. Done concurrently with Tier-2 sub-ADR closure | concurrent with 0055d closure (~done at Wave 3 end) | 0055a, 0055d, pre-Phase-H `cobrust build`/`cobrust run` spike (per ADR-0055 §6) |

Total Phase 1 + Phase 2 wall-time: ~4 days. Phase 3 wires in incrementally and does not add to critical path beyond Wave 3 closure.

## 9. Sub-ADR roster

**Single ADR. No further sub-sprints.**

The parity harness is one indivisible piece of infrastructure: the canonicalization algorithm, the corpus runner, and the cb-side wiring all share one crate and one test binary. Splitting into sub-ADRs would not buy parallelism (Phase 1 + 2 are sequential within a single P9 spike + P10-direct PAIR; Phase 3 is concurrent with 0055d's PAIR not a separate dispatch).

## 10. Risk register

Top 3:

1. **Arena-id renaming over-tolerance hiding real divergence** — if canonicalization is too aggressive (e.g., collapses `Ty::List(TyId(a))` and `Ty::List(TyId(b))` when the canonical traversal order differs by impl), it may mask a legitimate semantic divergence. Mitigation: Phase 2 Rust-vs-Rust sanity must include adversarial test cases where the same Rust impl is run twice with different `VarAllocator` seeds; canonicalization must report empty diff. If the adversarial test cannot reach empty diff, the algorithm is too tight; if it always reaches empty diff regardless of structural change, the algorithm is too loose. Calibration during Phase 2.
2. **Per-input vs per-module granularity migration cost** — if Phase 3 wire-in surfaces a need to rollup multiple inputs (e.g., cross-input `VarAllocator` state leaks on the cb side), the per-input granularity (§2) may force per-input reset that the actual impl does not perform. Mitigation: harness invokes each input in a **fresh `Ctx`** on both sides; cross-input state is impl-level, not harness-level. If a real cross-input bug surfaces, escalate to ADR-0055 amendment (add per-module granularity as a second contract layer).
3. **Integration with future cb test runner** — Phase 3 depends on `cobrust build` + `cobrust run` producing stable JSON output that the harness can parse. If the cb runner's JSON schema drifts during 0055a-d port work, the harness's deserializer must be updated in lockstep. Mitigation: ADR-0055 §6 pre-Phase-H "cobrust-cb compile-and-diff infrastructure spike" must lock the JSON schema before Wave 3; harness depends on that schema as a contract surface (not on the runner's free-form output).

## 11. Consequences

### 11.1 Positive

- Phase H ratification has a falsifiable bar — diff-empty on M2 corpus modulo arena-id renaming.
- The canonicalization algorithm (§3) is reusable for future self-host crates (HIR / MIR per ADR-0054 §11) where arena-id renaming is similarly load-bearing.
- Per-input granularity (§2) localizes failures to single inputs, which keeps Wave 2/3 incremental wiring tractable.
- Phase 1 + Phase 2 land before any cb port exists, which means a baseline harness is observable in CI before sub-ADRs 0055a-d dispatch. Wave 2/3 sub-ADRs do not own harness infrastructure; they only own corpus extensions + wire-in updates.
- ADR-0055 frame ratifies on this ADR's merge per its `ratification_path` frontmatter — Phase H's load-bearing arena-vs-recursive disposition (ADR-0055 §3.2) becomes operationally binding the moment 0055e Phase 1 lands.

### 11.2 Negative

- New workspace member adds doc-coverage load (zh + en + agent triples) per ADR-0055 §8.2. The harness is infrastructure not user-facing; agent docs are mandatory but human docs may be a single shared §"Parity harness" section in `self-host.md` rather than dedicated pages.
- Phase 3 cb-side wiring depends on `cobrust build`/`cobrust run` JSON schema stability — a hidden coupling surface. Surfaces in §10 risk 3.
- Canonicalization algorithm (§3) is non-trivial code that itself needs property-tests + adversarial calibration (§10 risk 1). Phase 2 budget allocates 1 day for this; if calibration takes longer, Phase 3 dispatch slips.

### 11.3 Neutral / unknown

- Whether the cb impl's `cobrust build` step emits stable JSON or free-form text. Deferred to pre-Phase-H `cobrust-cb compile-and-diff infrastructure spike` per ADR-0055 §6 acceptance gate.
- Whether `insta` snapshot fixtures committed into the parity crate become a maintenance burden when M2 corpus extends. Mitigation: snapshots regenerable via `cargo insta accept` per existing project workflow.

## 12. Dispatch readiness

- **TEST hours**: ~4-6 hours (canonicalization algorithm property tests + corpus-runner integration test).
- **DEV hours**: ~12-16 hours (Phase 1 + Phase 2; Phase 3 amortized into 0055d closure).
- **Wall**: ~1 week (Phase 1 + Phase 2 sequential; Phase 3 concurrent with Wave 3).
- **Host**: DG primary per ADR-0055 §9.1 row 2.
- **Mode**: C (P10-direct PAIR).

— P9 Tech Lead, 2026-05-18
