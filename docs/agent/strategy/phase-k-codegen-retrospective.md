---
doc_kind: strategy
strategy_id: phase-k-codegen-retrospective
title: Phase K codegen 5-strand retrospective
status: closure-retrospective
date: 2026-05-19
last_verified_commit: 1e57b85
relates_to: [adr:0058, adr:0023, adr:0054]
sourced_from: machine-local memory port 2026-05-19 (machine-loss-resilient copy)
---

# Phase K Codegen 5-Strand Retrospective

## Context

User 2026-05-18 explicit "下一步" (next-up) prioritization: codegen is the
THINNEST subsystem of Cobrust. Post-Phase J wave-1 (LSP publishDiagnostics),
focus shifts to codegen strengthening. Phase K closed at HEAD `1e57b85` (v0.3.0+).

## Why codegen was the priority

Frontend / type-system / translation pipeline mature at Phase G close:
- Phase H closed at 226 PASS
- tomli translation E2E real-LLM 5/5 PASS production-grade

Codegen was materially behind:
- Cranelift backend covered basics, no MIR-level IR optimizer layer
- LLVM backend (Phase K) was frame-only (ADR-0058 + 0058a), zero impl
- JIT vs AOT lowering paths drifting (`cobrust-jit/lower.rs` duplicated
  `cobrust-codegen` AOT lowering)
- Cross-compile matrix limited to macOS aarch64 + Linux x86_64

## 5 strands — shipped vs predicted

| Strand | Predicted status | Actual status (HEAD `1e57b85`) |
|---|---|---|
| 1. Phase K LLVM backend impl (ADR-0058a) | ~1 week wall | CLOSED — `de6c78d` audit ratification; 0058a/b/c Wave-1 shipped |
| 2. TD-1 Drop schedule | Long-term ADR needed | CLOSED EARLY at `aca5d87` — `cranelift_backend.rs:1239-1255` emits real `__cobrust_str_drop` / `__cobrust_list_drop_elems` / `__cobrust_list_drop` dispatched by `body.locals[place.local.0].ty` |
| 3. MIR-level IR opt pass | New sub-ADR needed | Deferred — codegen strong enough post-Phase K; revisit Phase M/N |
| 4. JIT/AOT lowering convergence | Extract-shared module | Deferred — tracked in ADR-0056a §13 |
| 5. Cross-compile matrix expansion | Windows MSVC + Linux musl | Partial — Linux x86_64 confirmed; Windows best-effort |

## Key lesson: TD-1 was already closed before Phase K dispatch

The pre-K assumption was that "Str non-Copy + List Copy@operand is TD-1 open
debt needing real Drop / refcount". This was **stale** as of Phase K design
audit `ae2316f`. TD-1 ALREADY CLOSED at `aca5d87`. The codegen priority doc
was updated 2026-05-19 to reflect this; Strand #2 reframed to "Drop schedule
mirror in JIT + LLVM backends" (narrow follow-on inside 0058a wave-1, not a
multi-week research project).

**Lesson**: Before dispatching "fix this open debt", always grep `cargo test`
output + `git log` for evidence the debt was already addressed in a prior wave.
Misidentified open strands waste dispatch cycles.

## F36 catch during Phase K

The 0058a fixture corpus had 4/5 rewritten fixtures with name-vs-behavior drift
(F36). Retroactive audit caught these pre-merge at `de6c78d`. See
[[finding:f36-fixture-name-vs-behavior-drift]] for the full pattern and gap queue.

## Sequencing rationale (preserved for future reference)

Strands 1+2 were §2.5-aligned (Phase K = bigger surface, more LLM-friendly IR;
Drop schedule = compile-time-catch via type system). Strands 3+4 are
debt-reduction. Strand 5 is user-visible (Windows users blocked until matrix
expands).

## Cross-references

- [[adr:0058]] Phase K LLVM backend frame
- [[adr:0023]] M9 codegen (§"Per-MIR-form lowering rules" LLVM column; ADR-0023
  §A3 honest-scope tightening per F36 retroactive amend)
- [[adr:0054]] post-Phase-G roadmap §"Phase K" un-defer
- [[finding:f36-fixture-name-vs-behavior-drift]] — F36 caught and ratified during
  Phase K wave-1 audit
