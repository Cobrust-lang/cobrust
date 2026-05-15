---
doc_kind: finding
finding_id: adsd-pair-pattern-impl-gap
last_verified_commit: fe45659
dependencies: [adr:0050, finding:adr-scope-reality-divergence]
discovered_by: user 2026-05-16 — surfaced the architectural gap between ADSD §"Dev/test pair pattern" (assumes P9 can dispatch P7) and Claude Code's single-layer sub-agent model (P9 has no Agent tool)
severity: P0 (methodology integrity)
status: open_candidate_for_adsd_F28
related: [adsd-F27-adr-pre-dispatch-source-verification-gate, adsd-F28-pair-pattern-impl-gap]
---

# Finding: ADSD PAIR pattern is ceremonial under Claude Code single-layer sub-agent architecture

## Hypothesis

ADSD §"Dev/test pair pattern (2026-05-11+ MANDATORY for D1-D3 + D5)" prescribes "P9 spawns P7-TEST sonnet first, then P9 reviews test corpus, then P9 spawns P7-DEV sonnet". The hypothesis: this pattern presumes a multi-layer agent architecture where a sub-agent (P9) can itself dispatch further sub-agents (P7-TEST, P7-DEV).

## Method

Inspection of Claude Code's sub-agent tool surface as exposed to P9 when launched via `Agent(subagent_type=general-purpose, model=opus, run_in_background=true)` from the main P10 session:

- P9 receives a constrained tool set determined by the `subagent_type` definition (Bash, Read, Edit, Write, Grep, etc.).
- **P9 does NOT receive the `Agent` tool.** The single-layer sub-agent architecture has no recursion.
- Even if P9 had it, the sub-sub-agent would be a fresh-context branch with no SendMessage path back to P10's coordination layer.

User surfaced this 2026-05-16 after observing that Wave 1 P9-A and P9-B prompts include the full "P7-TEST then P7-DEV" PAIR ceremony but P9 cannot literally execute it.

## Result

**The PAIR pattern as written in `cto_operations_runbook.md` §"Dev/test pair pattern" is structurally ceremonial when implemented via P9 sub-agent dispatch.**

What actually happens when a P9 prompt says "spawn P7-TEST sonnet first, then P7-DEV sonnet":

1. P9 has no Agent tool. It cannot literally spawn anything.
2. P9 either (a) ignores the instruction and writes TEST + DEV itself as a single Opus pass, or (b) writes them as two sequential phases of its own work (still single Opus, still confirmation-bias-prone), or (c) SendMessages back to P10 asking to spawn a P7 — workable but coordination-heavy.
3. The spike commits ADR-0050a (`1998dbe`) and ADR-0050b (`909811f`) landed as single-Opus work on `feature/f3-break-continue` and `feature/f3-for-loop` respectively. The "independent test author eliminates same-agent bias" justification from `cto_operations_runbook.md` §"Why this matters" is **not satisfied** by single-layer dispatch.

Three workarounds exist; only #3 honors the original PAIR intent:

1. **Solo P9 (current default, ceremonial PAIR)** — P9 writes TEST + DEV itself. Cheap; same-agent bias remains.
2. **P9 → SendMessage P10 → P10 dispatches P7** — workable; P10's coordination overhead increases linearly with sprint count; SendMessage round-trip adds latency.
3. **P10 directly dispatches TEST + DEV pair, no P9 in between** — true double-blind. P10 acts as coordinator: passes TEST's corpus to DEV as required input. Costs P10 ~2× the dispatch ceremony per sprint but delivers the methodological guarantee.

## Conclusion

**Actionable for Cobrust runbook** — `cto_operations_runbook.md` §"Dev/test pair pattern" is updated 2026-05-16 to mark the P9-dispatches-P7 pattern as **structurally invalid under Claude Code single-layer sub-agent architecture** and replace it with the P10-direct-PAIR pattern for any D1-D3 / D5 sprint requiring real double-blind discipline. A new memory entry `feedback_adsd_pair_pattern_impl_gap.md` captures the lesson so future sessions don't re-pattern P9-PAIR ceremony.

**Actionable for Wave 1 in-flight sprints** — P9-A and P9-B continue uninterrupted; their work lands as single-Opus contract-seal + corpus. The post-Wave-1 audit teammate I'll spawn at merge time gains an explicit assignment: verify the test corpus exercises real semantics (not just type-check happy paths) + verify edge-case coverage looks like independent thinking rather than impl-driven afterthought. This is the mitigation pathway for single-Opus bias when retroactive PAIR isn't feasible.

**Actionable for Wave 2 + Wave 3** — P10 directly dispatches TEST + DEV pairs as two parallel `Agent(subagent_type=general-purpose, model=sonnet|opus)` calls. The TEST agent reports `[TEST-CORPUS-READY]` with file paths + assertion count; P10 reviews; P10 then dispatches DEV with TEST's commit SHA + corpus paths as required input. No P9 layer for these sprints. The P9 layer is preserved for ADR-authoring sprints (D5 design-only like P9-C dict design) where dispatch isn't needed.

**Actionable for ADSD upstream methodology** — propose new failure-mode-catalogue entry:

> **F28 — PAIR pattern impl gap under single-layer sub-agent architecture**
>
> Symptom: ADSD §"Dev/test pair pattern" prescribes a P9-spawns-P7-TEST-then-P7-DEV protocol that presumes multi-layer agent dispatch. Single-layer sub-agent platforms (Claude Code as of 2026-05-16) do not expose `Agent` to sub-agents, making the prescribed PAIR ceremony unimplementable as written. Sub-agents either silently ignore the instruction and proceed solo (ceremonial PAIR, same-agent bias retained), SendMessage back to the orchestrator (workable but coordination-heavy), or fall back to single-pass implementation.
>
> SOP fix: ADSD methodology should declare PAIR pattern's implementation-layer responsibility explicitly. Under multi-layer platforms (Claude Code's own future Agent-Agent recursion, autonomous frameworks like AutoGen / CrewAI), P9 dispatches P7 as written. Under single-layer platforms, the orchestrator (P10) directly dispatches both TEST + DEV agents as parallel calls and acts as the PAIR coordinator; P9 layer is reserved for ADR-authoring + strategic decomposition sprints where dispatch isn't required.
>
> Empirical baseline (2026-05-16): Cobrust ADR-0050 Phase F.3 Wave 1 dispatched 3 P9 Opus sprints with PAIR ceremony in the prompt; 2 of them (P9-A break/continue, P9-B for-loop) executed as single-Opus contract-seal-and-corpus (ceremonial PAIR); 1 (P9-C dict design) was ADR-only and didn't need PAIR.

This finding is **standing open** until ADSD-upstream issue is filed (see https://github.com/Cobrust-lang/agent-driven-development).

## Cross-references

- `[[../adr/0050-phase-f3-language-completeness-batch.md]]` — parent batch ADR + Amendment §A7 (this finding's home).
- `[[adr-scope-reality-divergence.md]]` — F27 candidate filed earlier the same day; together F27 + F28 mark two structural ADSD gaps surfaced during the Phase F.3 batch dispatch cycle.
- Cobrust runbook update — `/Users/hakureirm/.claude/projects/-Users-hakureirm-codespace-Study-Cobrust/memory/cto_operations_runbook.md` §"Dev/test pair pattern" (memory file; updated 2026-05-16 to mark the prior pattern as structurally invalid and codify P10-direct PAIR).
- Memory entry — `feedback_adsd_pair_pattern_impl_gap.md` for cross-session persistence.
- ADSD upstream methodology source — `https://github.com/Cobrust-lang/agent-driven-development`.
