---
doc_kind: adr
adr_id: 0049
title: "Alpha honesty and onboarding hardening before external AI-surface exposure"
status: proposed
date: 2026-05-13
last_verified_commit: 801eeb6
supersedes: []
superseded_by: []
relates_to: [adr:0045, adr:0048]
---

# ADR-0049: Alpha honesty and onboarding hardening before external AI-surface exposure

## Context

Main HEAD `801eeb6` merged the ADR-0048 alpha flat-function surfaces M-AI.0 `cobrust.llm`, M-AI.1 `cobrust.prompt`, and M-AI.2 `cobrust.tool` after green DG heavy gates (`25776023205`). A post-merge three-lane ADSD audit then found that the branch is a strong **internal alpha baseline** but is not yet an honest, self-serve **external alpha baseline**.

Three independent audit lanes converged:

- **Persona-Mei BLOCK**: a Python-oriented first tester would likely bounce because the first-time path to try `llm_*`, `prompt_*`, and `tool_*` is not explicit in top-level docs; flat-function usage is not obvious from the `cobrust.llm/prompt/tool` naming; empty-string failure semantics feel too silent for self-serve trial.
- **Deep-source-read BLOCK**: several surfaces present stronger names/docs than the current implementation warrants. `llm_stream` is documented/tested like real chunk streaming but effectively behaves as collect-all single response; `input(prompt)` non-literal path comment/ABI/codegen do not line up; `llm_complete_with_tools` sounds like it executes tools but currently only prompt-augments and dispatches; router/config failures are widely collapsed to `""` / `[]`.
- **Tactical audit BLOCK**: top-level release/install docs still drift on 0.1.1 vs 0.1.2 in places; `docs/agent/modules/stdlib.md` has duplicate frontmatter keys; `scripts/doc-coverage.sh` enforces M-AI.2 but not M-AI.0/M-AI.1; main entry docs do not yet expose a concise “AI alpha quickstart” path.

This is not a reason to revert M-AI.0..M-AI.2. It is a reason to harden them before treating the current main branch as an external-facing alpha baseline.

## Options considered

1. **Option A — continue feature expansion immediately**
   - Pros: maximizes momentum on new AI-native surfaces.
   - Cons: compounds confusion and bakes misleading semantics deeper into docs/tests/user expectations; audit findings stay open while surface area grows.
   - **Rejected.**

2. **Option B — narrow hardening sprint before more surface expansion**
   - Pros: resolves honesty gaps, first-time-user friction, and gate blind spots while the surface area is still small; converts audit findings into explicit contracts.
   - Cons: delays the next visible feature batch by one sprint.
   - **Chosen.**

3. **Option C — relabel current branch as internal-only and defer fixes indefinitely**
   - Pros: avoids immediate work.
   - Cons: conflicts with ADR-0045 user-traction discipline and leaves main with alpha surfaces that look more self-serve than they are.
   - **Rejected.**

## Decision

Adopt **Option B**. The next sprint is a three-lane hardening sprint with no new user-visible AI surface area beyond clarification and honesty corrections.

### Lane 1 — API honesty hardening (P0/P1)

Resolve implementation/doc/test mismatches on the merged AI alpha surfaces:

- `llm_stream` must either:
  - implement real ordered chunk streaming, or
  - be renamed/reframed/documented/tested as a collect-all alpha shim.
- The non-literal `input(prompt)` path must align across comment, intrinsic lowering, ABI shape, and codegen.
- `llm_complete_with_tools` must either:
  - gain a minimal actual tool-execution loop, or
  - be explicitly renamed/reframed as prompt-augment + dispatch only.
- Silent `""` / `[]` collapse for config/provider/router failures must be surfaced more honestly for alpha users (diagnostic channel, explicit sentinel docs, or other narrow mechanism decided in implementation).

### Lane 2 — onboarding and release-facing cleanup (P1/P2)

Make the first external alpha experience self-serve enough to match ADR-0045 discipline:

- Normalize release/install references to current `v0.1.2` where top-level docs still show `v0.1.1` snippets.
- Add a concise top-level “AI alpha quickstart” or equivalent pointer from README/getting-started to the architecture/config path.
- Explicitly state that current AI alpha surfaces are **flat prelude functions**, not `import cobrust.llm` module-path syntax.
- Explain empty-on-failure semantics in plain user-facing terms until Lane 1 changes land.

### Lane 3 — doc/gate coverage hardening (P1/P2)

Make drift mechanically harder to reintroduce:

- Remove duplicate frontmatter keys in `docs/agent/modules/stdlib.md`.
- Extend `scripts/doc-coverage.sh` with explicit M-AI.0 and M-AI.1 coverage blocks parallel to M-AI.2.
- Fix misleading “checks passed” echo placement so the script’s output reflects actual completion ordering.

## Consequences

- **Positive**
  - Converts three independent BLOCK audit verdicts into a bounded, testable sprint.
  - Keeps ADR-0048 momentum while reducing the chance of external trust loss.
  - Strengthens ADSD discipline by promoting doc/gate drift into executable checks.

- **Negative**
  - Delays the next Phase F.2 surface expansion by at least one sprint.
  - May force small API-name or semantic corrections soon after alpha merge, creating short-term churn.

- **Neutral / unknown**
  - Lane 1 may reveal that some current alpha names should be softened rather than implemented fully in one sprint.
  - If tooling for surfaced diagnostics is expensive, the sprint may choose documentation-first honesty over a deeper runtime/status-channel design.

## Evidence

- Persona audit: `[PERSONA-MEI-COMPLETION]` — first external alpha tester cannot plausibly self-serve.
- Deep-source-read: `[DEEP-SOURCE-READ-COMPLETION]` — `llm_stream`, `input(prompt)`, tool dispatch, and silent-failure honesty gaps.
- Tactical audit: `[POST-MERGE-AUDIT-COMPLETION]` — 0.1.1/0.1.2 drift, stdlib module frontmatter sediment, M-AI.0/M-AI.1 doc-coverage blind spot.
- Related decisions: ADR-0045 (user-traction milestone gate), ADR-0048 (AI-friendly framing + Phase F.2 alpha surfaces).
- Related files: `README.md`, `docs/human/*/getting-started.md`, `docs/agent/modules/stdlib.md`, `scripts/doc-coverage.sh`, `crates/cobrust-stdlib/src/{llm,prompt,tool}.rs`, `crates/cobrust-cli/src/build/intrinsics.rs`, `crates/cobrust-codegen/src/cranelift_backend.rs`.
