---
doc_kind: adr
adr_id: 0054
parent_adr: none (roadmap layer)
title: "Post-Phase-G roadmap — Phase H self-host type checker / I REPL JIT / J LSP / K LLVM Backend / L Debugger ordering + agent-velocity timeline calibration"
status: proposed
date: 2026-05-18
last_verified_commit: bc10842
supersedes: []
superseded_by: []
relates_to: [adr:0019, adr:0023, adr:0029, adr:0051, adr:0052, adr:0052b]
discovered_by: P10/user 2026-05-18 "when can LLVM Backend / LSP / Debugger / REPL JIT be scheduled?" + Phase G Wave-2 round-2 near-closure
ratification_path: P9 frame-ADR review; ratifies on first sub-ADR (0055 / 0056 / 0057 / 0058 / 0059) dispatch
---

# ADR-0054: Post-Phase-G roadmap — Phase H/I/J/K/L ordering + agent-velocity timeline calibration

## 1. Context

### 1.1 Phase G near-closure state

Phase G (ADR-0052 batch frame) is mostly closed at HEAD `bc10842`:

- Wave 1 (Direction A — explicit `&` borrow): ADR-0052a accepted; 3-attempt design recovery captured at ADR-0052a §13.
- Wave 2 round 1: ADR-0052b (Error UX), ADR-0052c (`@py_compat` L2 hard-bind), ADR-0052d-prereq (method-dispatch infra) all accepted.
- Wave 2 round 2 (partial): ADR-0052f (parser §8 cap relaxation for `&Call(Attr(...))`) accepted at `94e5544`; ADR-0052g (`&CallResult` type-check) landed at `bc10842`.
- Outstanding: Direction D method-call sugar full impl (ADR-0052d) still in wave-2 round-2 dispatch queue.

v0.3.0 stable tag binding pending Wave-2 closure. ADR-0051 (CLAUDE.md §2.5 LLM-first design principle) is the active constitutional north star binding every roadmap decision in this ADR.

### 1.2 The roadmap question

User asked 2026-05-18: "when can LLVM Backend / LSP / Debugger / REPL JIT be scheduled?" This ADR is the answer. It does NOT decide individual designs; sub-ADRs 0055 / 0056 / 0057 / 0058 / 0059 own those at dispatch time. It DOES decide: phase ordering, §2.5 ROI reranking rationale, agent-velocity timeline calibration, parallelism map, prep-work scoping, and out-of-scope deferral.

This ADR is a frame-layer roadmap ADR — same structural role ADR-0052 played for Phase G + ADR-0050 played for Phase F.3 — but at the milestone-layer (Phase H/I/J/K/L) above the sub-ADR-batch layer.

### 1.3 Constitutional anchors

- **CLAUDE.md §2.5** (HEAD `bc10842` lines 67-87) — LLM-first design principle. Two operational selection rules: compile-time-catch-errors + maximize-overlap-with-training-data. Audit-teammate rubric per ADR-0051 §"Consequences".
- **CLAUDE.md §4.4** — "Self-hosting roadmap. The compiler is initially in Rust. Once Cobrust reaches sufficient maturity (post-M5), begin self-hosting non-performance-critical compiler stages, prioritizing the type checker and the AST printer first." Phase H operationalizes this. (ADR-0052 §"Out of scope" item 7 F-G.2 amendment delayed activation from post-M5 to post-M11; this ADR ratifies that delayed activation as Phase H.)
- **CLAUDE.md §7 Milestones table** — M14 REPL accepted (ADR-0029 control-flow + JIT deferred to M14.1); M9 codegen accepted with Cranelift default + LLVM `--features llvm` opt-in (ADR-0023). LSP not in §7 table — explicitly post-Phase-G surface per ADR-0050 §M-F.3.9.

## 2. §2.5 ROI reranking rationale

The user-listed order ("LLVM Backend / LSP / Debugger / REPL JIT") is **NOT** the ship order. §2.5 binding reranks to:

| Rank | Phase | Surface | §2.5 ROI | Rationale |
|---|---|---|---|---|
| 1 | **J** | LSP server | **highest** | Cursor / Continue / Cody / Aider all consume LSP `Diagnostic.relatedInformation` + `CodeAction.title` fields. ADR-0052b §"Out of scope" L259 explicitly flags LSP as the structured-`suggestion` consumer. Direct LLM-amplifier surface — every IDE-driven LLM dev session benefits. |
| 2 | **H** | Self-host type checker | high | Constitution §4.4 binding (post-M11 per F-G.2). Type checker self-host produces a `.cb` codebase the LLM can learn from for *every other Cobrust translation*. Closes the "training-data overlap" gap §2.5 §B rule. |
| 3 | **I** | REPL JIT (M14.1) | medium | Translation pipeline L1 loop speedup (one-shot AOT call per stmt → JIT dispatch). Speeds the translation closed-loop measurably but no new LLM-surface contract. |
| 4 | **K** | LLVM Backend | §2.5-neutral | Product credibility (release perf + cross-platform). Numpy-tier workloads benchmark below numpy without it. But the LLM doesn't write code differently because of LLVM — neutral on the operational selection rules. |
| 5 | **L** | Debugger | §2.5 ~0 | Single-step + breakpoint + var-inspect — human-facing. Agents don't single-step; they read stderr + retry. Ship for human-developer ergonomics; do not slot ahead of any §2.5-positive phase. |

The user-listed order (K → J → L → I) reflects human-developer instinct: ship the perf-credible thing first, then IDE integration, then debugger, then JIT. §2.5 inverts this: LSP first because IDE-LLM session quality is the largest agent multiplier; debugger last because agent debugging doesn't single-step.

**This reranking is the load-bearing decision of this ADR.** Sub-ADR dispatch follows the H/I/J/K/L letter order, but the §2.5 ROI columns above bind the *priority* of each phase, not just sequence.

Phase H precedes Phase J despite J's higher §2.5 ROI because:
- Phase J (LSP) needs incremental type-check infrastructure that Phase I (REPL JIT) shakes out under the lighter REPL surface.
- Phase H self-host produces the `cobrust-types-cb` mirror — a body of Cobrust code the LSP can dogfood against during Phase J development.
- Phase H + Phase I can OVERLAP (different code paths); Phase J blocks on Phase I (incremental type-check context). See §9.

## 3. Phase H — Self-host type checker

### 3.1 Scope

- **Mirror** `crates/cobrust-types` (~12 source files, ~5500 LOC at HEAD `bc10842`) into a parallel `crates/cobrust-types-cb/` written in Cobrust `.cb` source.
- Rust impl stays **canonical** (the `cobrust check` binary still links the Rust crate). The `.cb` mirror is a **proof artifact** — compiled by Cobrust itself, then differentially-tested against the Rust canonical on the M2 well-typed + ill-typed corpus.
- Constitution §4.4 says "non-performance-critical stages first" — type checker is the prioritized starting point. AST printer is the second target (post-Phase-H if surplus capacity).

### 3.2 Wall-time

- **3 weeks agent-velocity** (was ~3 months human-developer estimate; 4x compression per Wave 1+2 empirical calibration §8).
- 3 sub-ADRs (0055a / 0055b / 0055c) per major stage: type-check inference, borrow-check projection (B1..B5 obligations), unification.

### 3.3 Sub-ADRs

- **ADR-0055** — Phase H frame ADR. Same structural role this ADR plays for the H-L roadmap.
- **ADR-0055-prereq** — Scoping ADR (~1 week). Decides which types + which crate split: full mirror of `cobrust-types` vs. carved-out subset (e.g. just `Ty` + `check_expr` + `unify` first).
- **ADR-0055a** — Type-check inference port (~1 week sub-phase).
- **ADR-0055b** — Borrow-check projection port (~1 week sub-phase).
- **ADR-0055c** — Unification + error-emission port (~1 week sub-phase).

### 3.4 Risk

Self-host stresses every language feature simultaneously (the type checker uses dicts, lists, options, results, traits, pattern match, generics-via-typeparams). Phase H will discover Phase F+G ergonomic gaps that the test corpus did not surface. Budget +1 week buffer for retroactive sub-ADRs against Cobrust language surface.

## 4. Phase I — REPL JIT (M14.1)

### 4.1 Scope

- Control-flow REPL: lift M14's HIR-interpreter limit (ADR-0029 §"Evaluation surface" table) — `if` / `while` / `for` / user `fn` / comprehensions all evaluable from REPL.
- Cranelift JIT runtime invoke: replace M14's "one-shot AOT compile per stdlib call" path with `cranelift-jit::JITModule` direct invoke.
- Reuses M11.2 `FnRef::Call` lowering already shipped (verified at HEAD `bc10842` per ADR-0023 §"Per-MIR-form lowering rules" Cranelift column).

### 4.2 Wall-time

- **1 week agent-velocity**.
- Single sub-ADR (0056). No further sub-decomposition needed — surface is small + bounded by ADR-0029's pre-existing public API contract (`Session::step()`).

### 4.3 Sub-ADRs

- **ADR-0056** — REPL JIT (M14.1 completion). Lifts ADR-0029 §"Evaluation surface" deferred forms.

### 4.4 Risk

Low. Cranelift JIT integration is well-trodden (used by `rustc_codegen_cranelift` + `wasmtime`). Main risk is cold-start budget: ADR-0029's <200ms bar must hold under `cranelift-jit` link-time. Mitigation: lazy-init `JITModule` on first non-introspection statement.

## 5. Phase J — LSP server (the biggest §2.5 payoff)

### 5.1 Scope

LSP `textDocument/*` surface for `cobrust-language-server` binary in a new `crates/cobrust-lsp/` crate:

- **textDocument/diagnostic**: every `cobrust check` error round-trips through LSP `Diagnostic` shape. Wires ADR-0052b `suggestion: Option<&'static str>` to LSP `Diagnostic.relatedInformation` + `CodeAction.title`. **This is the load-bearing §2.5 forward-compat hook.**
- **textDocument/hover**: type-of-expression on cursor (consumes `Session::step()` `:type` directive path from ADR-0029).
- **textDocument/completion**: identifier completion (extends ADR-0029 §"Tab completion sources" — keywords + stdlib + session bindings + workspace symbols).
- **textDocument/definition**: goto-def (consumes HIR `DefId` resolution).
- **textDocument/rename**: rename refactor (cross-file workspace symbol table).
- **Protocol**: `tower-lsp` v0.20+ binding (mature Rust crate, MIT-licensed; ADR-0012 "translate the surface, bind the core" applies).

### 5.2 Wall-time

- **2-3 weeks agent-velocity**.
- 4 sub-ADRs (frame + 3 per-feature).

### 5.3 Sub-ADRs

- **ADR-0057** — Phase J frame ADR (LSP architecture + tower-lsp binding + crate split).
- **ADR-0057a** — Diagnostics (ADR-0052b `suggestion` → LSP `relatedInformation` / `CodeAction` wiring).
- **ADR-0057b** — Hover + completion (consumes Phase I REPL incremental context).
- **ADR-0057c** — Goto-def + rename (workspace symbol table).

### 5.4 Risk

Medium. Protocol-revision risk (LSP v3.17 → v3.18 evolving). Mitigation: pin `tower-lsp` major version + document the version in ADR-0057 §"Decision". Cross-file workspace indexing (rename) is the heaviest sub-surface; sub-ADR 0057c may slip into Phase K window.

### 5.5 §2.5 binding

Phase J is the operational realization of §2.5 §B "training-data overlap" at the *tooling layer*: every LLM-driven IDE session (Cursor / Continue / Cody / Aider) emits LSP-aware editing actions. The agent-LLM sees diagnostics through the LSP envelope; the structured `suggestion` field travels intact end-to-end. Without Phase J, ADR-0052b's structured shape stays a private contract within the `cobrust check` binary.

## 6. Phase K — LLVM Backend

### 6.1 Scope

- Full LLVM IR lowering parallel to Cranelift, per ADR-0023 §"Backend feature flag layout" `--features llvm` opt-in path.
- Release-mode `-O3` codegen: ADR-0023 §"LLVM `-O3` ≥ 30% smaller binary acceptance" pinned the bar. Phase K closes the bar empirically.
- Cross-platform target matrix expansion (ADR-0023 §"Target triple matrix" reachable rows: `x86_64-apple-darwin` + `aarch64-unknown-linux-gnu` move from "reachable" to "delivered").
- DWARF debug-info emission (paired with Phase L; LLVM `DIBuilder` API).

### 6.2 Wall-time

- **3-4 weeks agent-velocity**.
- LLVM C API + linker integration are genuinely complex; compression smaller than other phases (per §8 calibration risk note).
- 3 sub-ADRs (frame + 2 per-stage).

### 6.3 Sub-ADRs

- **ADR-0058** — Phase K frame ADR (LLVM 18+ binding, `inkwell` 0.9+ activation, target matrix expansion).
- **ADR-0058a** — Per-MIR-form LLVM lowering (mirrors ADR-0023 §"Per-MIR-form lowering rules" LLVM column).
- **ADR-0058b** — DWARF debug-info emission (shared with Phase L).
- **ADR-0058c** — Cross-platform target matrix (host-arch + macOS Intel + Linux ARM64).

### 6.4 Risk

High among the 5 phases. LLVM API stability is the constraint — `inkwell` major-version updates lag LLVM major-version updates. Compression ratio for Phase K is empirically smaller (~2x) than for self-contained pure-Rust work (~4-8x). Phase K wall-time estimates conservative.

## 7. Phase L — Debugger

### 7.1 Scope

- Pairs with Phase K (shares DWARF emission from ADR-0058b).
- REPL integration: `cobrust debug examples/foo.cb` enters a debugger session with breakpoint + step + var-inspect on the Phase I JIT path.
- Debug protocol: DAP (Debug Adapter Protocol; same vendor neutrality as LSP). VS Code / nvim-dap / other DAP-aware IDEs work out of box.

### 7.2 Wall-time

- **1 week agent-velocity** post-Phase-K closure.
- Single sub-ADR (0059).

### 7.3 Sub-ADRs

- **ADR-0059** — Phase L debugger (DAP server + DWARF consumer + REPL JIT breakpoint instrumentation).

### 7.4 Risk

Low post-K. DWARF emission is the gate; once Phase K ships it, debugger consumption is well-trodden (lldb / gdb / VS Code dap-client all consume DWARF + DAP without Cobrust-specific bridging).

## 8. Timeline calibration evidence (agent-velocity vs human-developer)

This ADR's wall-time table compresses estimates 4-8x vs human-developer cadence. The compression ratio is empirically grounded in Wave 1 + Wave 2 sprints:

### 8.1 Wave 1 (Direction A — explicit `&` borrow)

- Shipped in 1 session (~48 hours of human-wall-time including overnight) including 3-attempt design recovery (ADR-0052a §13 captures the design-bug discovery + recovery).
- Human-developer estimate for "introduce a new borrow form to a non-borrow-checker language with predicate-flip + cascade-discovery on ~16 file:line anchors" would be ~2 weeks (predicate-flip is canonically a 1-2 week task in compiler engineering).
- Compression: ~7x.

### 8.2 Wave 2 round 1 (ADR-0052b + 0052c + 0052d-prereq)

- 3 sub-ADRs (Error UX rewrite ~62 file:line edits + `@py_compat` L2 tier-aware verifier + method-dispatch infra design-only spike) shipped in 1 session (~24 hours human-wall-time).
- Human-developer estimate for the same scope (3 mid-sized refactors across 3 crates, parallel-dispatch-friendly): ~3 weeks (1 week per refactor sequentially, ~2 weeks parallel).
- Compression: ~5-7x.

### 8.3 Wave 2 round 2 (ADR-0052f impl)

- Parser §8 cap relaxation for `&Call(Attr(...))` shipped via single-agent dispatch in ~1-2 hours human-wall-time including 5-gate verification.
- Human-developer estimate for parser-level cap change with corpus regression check: ~2 days.
- Compression: ~10x for surgical surface-area changes.

### 8.4 Compression ratio summary

| Sprint class | Empirical compression | Why |
|---|---|---|
| Surgical (single-crate, no cross-cutting) | ~8-10x | LLM batch-edits + parallel-test all 5 gates simultaneously |
| Mid-scope (3 crates, 50-100 file:line edits) | ~5-7x | Wave-2 round-1 baseline |
| Large-scope (predicate-flip + cascade discovery) | ~4-5x | Wave-1 baseline; design-bug recovery is the dominant cost |
| External-system-bound (LLVM API, LSP protocol revisions) | ~2-3x | Less compression — work bottlenecked on external-system docs + version-stability |

### 8.5 Calibration applied to Phase H-L

- Phase H (self-host type checker): 3 weeks at ~6x compression vs ~4-month human estimate.
- Phase I (REPL JIT): 1 week at ~8x compression vs ~2-month human estimate.
- Phase J (LSP): 2-3 weeks at ~5x compression vs ~3-month human estimate.
- Phase K (LLVM): 3-4 weeks at ~3x compression vs ~3-month human estimate (external-system-bound).
- Phase L (Debugger): 1 week at ~6x compression vs ~1-2-month human estimate.

**Total post-Phase-G runway: ~10-12 weeks agent-velocity** (= ~11-13 months human-developer cadence).

## 9. Dispatch ordering + parallelism

```
Phase G closure (Wave-2 round-2 complete)
       │
       ▼
       ├─────► Phase H frame (ADR-0055-prereq + 0055 + 0055a/b/c)
       │              │
       │              │ OVERLAP — different code paths
       │              ▼
       ├─────► Phase I (ADR-0056) REPL JIT
       │              │
       └──────────────┴────► Phase J (ADR-0057 + 0057a/b/c) LSP server
                                   │
                                   ▼
                            Phase K (ADR-0058 + 0058a/b/c) LLVM Backend
                                   │
                                   ▼
                            Phase L (ADR-0059) Debugger
```

### 9.1 Overlap rules

- **Phase H + I OVERLAP** after Phase H frame ADR lands. H touches `crates/cobrust-types-cb/` (new); I touches `crates/cobrust-cli/src/repl.rs` + `cobrust-codegen` JIT integration. Zero file-path overlap.
- **Phase J blocks on Phase I**. LSP `textDocument/hover` + `textDocument/completion` need incremental type-check context that Phase I produces (REPL Session state machine is the precedent for incremental update).
- **Phase K sequential after Phase J**. Phase J can ship without LLVM (Cranelift backend at `-O0` is sufficient for IDE-driven editing latency). Phase K then opens release-mode credibility.
- **Phase L sequential after Phase K**. DWARF emission is shared from Phase K's ADR-0058b sub-ADR.

### 9.2 Critical path

Phase G closure → Phase H frame → (Phase H sub-ADRs ∥ Phase I) → Phase J → Phase K → Phase L

Critical path length: ~10-12 weeks. Non-critical: Phase H sub-ADR completion (can extend into Phase J window without blocking).

## 10. Pre-Phase-H prep work (immediate, before frame H ADR)

Before Phase H frame ADR-0055 dispatches, three lightweight prep deliverables land first:

- **LSP interface scoping spike** (~1 day, Mac-local doc-only): wire `TypeError::* { suggestion }` (per ADR-0052b §3) → LSP `Diagnostic.relatedInformation` shape on paper. No code; doc-only ADR-0057-prereq or §"Evidence" pin on ADR-0057 frame. Validates the §"§2.5 binding" claim in §5.5 ex-ante.
- **REPL JIT spike** (~1 day, DG-local cargo POC): minimal Cranelift JIT POC reusing M11.2 `FnRef::Call` lowering path. Builds confidence in ADR-0056's 1-week wall-time.
- **Self-host type checker scoping ADR-0055-prereq** (~1 week, Mac-local doc-only): which types + which `cobrust-types` modules get the `.cb` mirror first. Decides Phase H's three sub-ADR scope boundary (inference vs borrow vs unify).

These three preps run in parallel after Phase G Wave-2 round-2 closes. Total parallel wall-time ~1 week.

## 11. Out of scope (this ADR)

- **v0.3.0 cargo / crates.io publish workflow** — separate v0.3.0 release ADR, post-Wave-2-round-2 closure. This ADR is roadmap-layer; release-mechanics ADRs are publish-layer.
- **WASM target** — per CLAUDE.md §7 Phase F out-of-scope + ADR-0023 §"Target triple matrix" `wasm32-unknown-unknown` row. Defer to post-Phase-L. WASM is its own M+ deliverable, not a Phase-K sub-target.
- **Mobile / iOS toolchain** — post-v1.0.0. Cross-arch toolchain work is post-Phase-L, post-LLVM-Backend, post-cross-platform-matrix.
- **LLM router strategy v2** (ADR-0048 batch frame revision) — post-Phase-J. LSP-driven LLM consumption pattern needs to mature first before router routing-strategy revision becomes data-informed.
- **Self-host CLI / parser / MIR / codegen** — Phase H limits self-host to type checker (§4.4 "non-performance-critical stages first"). Parser/MIR/codegen self-host is post-Phase-L.
- **AST printer self-host** — Phase H §3.1 §4.4 "second target" reference. If Phase H closes with surplus capacity, AST printer port lands as Phase H+ micro-sprint; otherwise deferred to post-Phase-L.

## 12. Consequences

### 12.1 Positive

- §2.5 ROI reranking codified: LSP > REPL JIT > LLVM > Debugger by binding LLM-amplifier rule, not human-developer instinct.
- Roadmap visibility: user can now answer "when does X ship?" against the 5-phase H/I/J/K/L timeline.
- Wave 1+2 empirical compression calibration captured (§8); future roadmap ADRs reuse the table.
- Parallelism map (§9) maximizes dispatch utilization — Phase H + I OVERLAP saves ~1 week vs sequential.
- ADR-0052b structured `suggestion` field's forward-compat value is operationalized (Phase J §5.5 wires it to LSP).
- Constitution §4.4 self-hosting binding (post-M11 per F-G.2) gets a concrete Phase H slot, exiting indefinite-deferral status.

### 12.2 Negative

- 5-phase roadmap is a heavy commitment. If Phase G Wave-2 round-2 surfaces unexpected scope (e.g. ADR-0052d method-dispatch infra impl ships harder than ADR-0052d-prereq estimated), Phase H start slips, cascade across all 5 phases.
- Compression-ratio claim (§8) is calibrated on 3 sprints. Sample size small. Phase K's lower-compression risk (§8.4) explicitly acknowledged but unverified empirically.
- LSP protocol-revision risk (§5.4) is non-deterministic; can require sub-ADR ratification revisions mid-Phase-J.
- Self-host Phase H is the largest single phase (3 weeks) and the highest design-discovery risk (§3.4) — Phase H may surface retroactive sub-ADRs against language surface (e.g. trait-system gaps the test corpus didn't expose).

### 12.3 Neutral

- v0.3.0 stable tag still binds on Wave-2 round-2 closure (per ADR-0052 §"Consequences" Negative item 5) — not on this ADR's H-L roadmap. Tag boundary is unchanged.
- Phase L (debugger) is human-facing and §2.5-ROI ~0; including it in the roadmap is product-completeness, not LLM-amplifier. Justified by paired-with-Phase-K DWARF emission cost-share.
- DAP protocol (§7.1) is sibling-vendor-neutral to LSP — Phase L surface-area minimum once Phase K DWARF is in place.

## 13. Why this ADR now

- Phase G is near-closure (Wave-2 round-2 ADR-0052g landed at `bc10842`; ADR-0052d method-call-sugar impl remaining).
- v0.3.0 prep needs known post-G runway. Without this ADR, the post-G work is unscoped and the v0.3.0 release ADR has no successor-phase to commit against.
- User-explicit roadmap question 2026-05-18 ("when can LLVM Backend / LSP / Debugger / REPL JIT be scheduled?") — this ADR is the binding response.
- §2.5 ROI reranking is non-obvious; without ADR-codification, future dispatch may default to user-listed (K → J → L → I) order against §2.5 §2 binding. Codifying H/I/J/K/L ordering ex-ante prevents drift.
- Wave 1+2 empirical compression data is freshest now (within 1-3 sessions of the sprints). Calibration captured before details fade.

## 14. Evidence

- `CLAUDE.md` §2.5 (HEAD `bc10842` lines 67-87) — constitutional north star binding ROI rerank.
- `CLAUDE.md` §4.4 — self-hosting binding (post-M5 → post-M11 per ADR-0052 §"Out of scope" F-G.2 amendment).
- `CLAUDE.md` §7 — milestone table M9 + M14 anchors.
- ADR-0019 — milestone scope ladder (M9 codegen / M14 REPL).
- ADR-0023 — M9 Cranelift default + LLVM `--features llvm` opt-in (Phase K activates `--features llvm` as Phase-K default).
- ADR-0029 — M14 REPL accepted; M14.1 control-flow + JIT deferred → Phase I.
- ADR-0051 — §2.5 constitutional ratification; audit-teammate rubric Phase H+ inherits.
- ADR-0052 — Phase G batch frame; this ADR mirrors its structural role at the milestone layer.
- ADR-0052b §"Out of scope" L259 — LSP structured-`suggestion` forward-compat hook (Phase J §5.1 consumes).
- Wave 1 empirical: ADR-0052a §13 design-bug recovery + ~48h session.
- Wave 2 round 1 empirical: ADR-0052b + 0052c + 0052d-prereq landed ~24h session.
- Wave 2 round 2 empirical: ADR-0052f impl 1-2h single-agent dispatch.

— P9 Tech Lead, 2026-05-18
