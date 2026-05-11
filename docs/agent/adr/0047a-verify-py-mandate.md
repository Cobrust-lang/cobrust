---
doc_kind: adr
adr_id: 0047a
title: "Tier B P7-TEST mandate — verify.py independent oracle for every program"
status: accepted
date: 2026-05-12
last_verified_commit: 32e7015
supersedes: []
superseded_by: []
relates_to: [adr:0045, adr:0047]
discovered_by: LC-100 Tier A empirical evidence (15/23 = 65% failures were oracle-authoring defects)
ratified_by: P10 CTO 2026-05-12 (post Sprint 2 DEV close, Option H 99/100 stable)
---

# ADR-0047a: Tier B P7-TEST mandate — verify.py independent oracle for every program

## Context

LC-100 Tier A (per ADR-0047) closed at HEAD `32e7015` with 99/100 pass rate. Of the 23 initial failures, **15 (65%) were test corpus oracle defects** rather than Cobrust language gaps. The defects were uniform in shape: a P7-TEST sonnet agent authored both the algorithm paraphrase in README and the `expected_stdout` in test.toml from its own mental execution of the algorithm. With no independent verification path, arithmetic / DP-trace / tree-encoding mistakes encoded directly into the oracle — silently invalidating the gate for programs whose `solution.cb` was algorithmically correct.

Finding `lc100-pattern-c-test-corpus-defects.md` enumerates the 15 defects with verified arithmetic + DP-trace evidence. Finding `lc100-adsd-f-pattern-candidates.md` codifies the systemic pattern as **F23-A — Oracle authorship without independent verification** (counterpart to F22 coverage-without-fix-cadence, which ADR-0047's gate did successfully suppress).

Sprint 1 (commit `2d952e0`) closed the 15 defects post-hoc. But at Tier B scale (500 programs per ADR-0047 ramp), the same defect rate would produce ≈75 oracle-defect failures — burying real codegen / stdlib gaps under noise. This ADR codifies the prevention: every Tier B `.cb` program ships with a **verify.py** reference Python implementation, run automatically against the `test.toml` corpus to confirm the oracle BEFORE solution.cb authorship begins.

ADR-0045 (user-traction milestone gate) already mandates **independent verification at the release tag**; this ADR extends the principle to **per-program TDD authorship within Tier B sprints**. Same anti-pattern (declared-without-independent-verification), different surface.

## Options considered

### Option A — Status quo: P7-TEST authors both README + test.toml from mental execution

- Pros: minimal process overhead per program.
- Cons: empirically demonstrated 65% oracle-defect rate at Tier A scale. Scales to ~75 defects at Tier B. **Rejected.**

### Option B — verify.py per program, P7-TEST mandate (CHOSEN)

For every program in `examples/leetcode-stress/<NN>-<slug>/`, the P7-TEST sonnet agent additionally authors `verify.py` — a reference Python 3 implementation of the same algorithm. The P7-TEST sprint then runs verify.py against each `[[cases]]` entry in `test.toml` and confirms the actual stdout matches `expected_stdout`. Discrepancies are fixed in `test.toml` (or `verify.py` if the implementation has a bug — but the algorithm correctness check is independent of the oracle's expected value).

The DEV agent's solution.cb is then validated against the verify.py-confirmed oracle. The P7-TEST agent's mental-execution path is no longer the sole oracle source.

- Pros: empirically validated (Sprint 1's 15 corrections were all derivable by running reference Python). Scales: 1 verify.py per program is ~20-50 LOC, well within sonnet capability. CI gate: a release-readiness-style harness re-runs verify.py against test.toml at corpus-edit time, catching drift.
- Cons: each program now has 3 artifacts (README + test.toml + verify.py) instead of 2; sprint authoring time per program rises ~30-50%.
- **Chosen.**

### Option C — Strict TDD with mocked CPython differential per program (heavyweight)

- Pros: maximum rigor.
- Cons: out of proportion to Tier B scale. **Rejected** (defer to Tier C 3816 if it ever fires).

## Decision

Adopt Option B. Codify in `cto_operations_runbook.md` §"Dev/test pair pattern" and in P7-TEST dispatch prompts for all Tier B sprints. Concrete requirements:

1. **Every program directory MUST contain** `README.md` + `test.toml` + `verify.py` (Tier B onward). Tier A's existing 100 programs are grandfathered — verify.py back-fill is a Tier B Sprint 0 nice-to-have, not a blocker.
2. **verify.py contract**:
   - Reads stdin verbatim (no argv unless the program uses `argv()`).
   - Writes stdout matching the algorithm's deterministic output.
   - Python 3.11+ (project standard). No third-party deps beyond stdlib.
   - Pure-function preferred; no global side effects.
3. **P7-TEST sprint loop**:
   ```bash
   for each program:
     1. Author README.md (paraphrased algorithm, IP-safe).
     2. Author verify.py (reference impl in own words — not LeetCode editorial paste).
     3. Author test.toml [[cases]] with input + expected_stdout.
     4. Run: for each case, `printf "$input" | python3 verify.py` → diff against expected_stdout.
     5. If diff non-empty: fix test.toml's expected_stdout (oracle was wrong) OR fix verify.py (impl was wrong); P7-TEST decides via 1-min algorithm review.
     6. Commit only when ALL cases match.
   ```
4. **Sprint exit gate**: P7-TEST's `[P7-TEST-CORPUS-READY]` report MUST include a "verify.py oracle audit" section: per-program rows showing `case_count` + `verify_py_matches`. P9 + CTO smoke-check this section as part of corpus review.
5. **Backward compatibility**: existing W2 wedge programs (`examples/leetcode/*.cb` shipped via ADR-0044) are NOT subject to retroactive verify.py — they already have a different validation path (e2e harness with explicit oracle in `leetcode_corpus_e2e.rs`).

## Consequences

### Positive

- Closes ADSD F23-A systemically. Oracle-authorship-without-independent-verification cannot recur at Tier B scale because verify.py is the mechanical check.
- Empirically grounded: Sprint 1 post-hoc corrections were all derivable by running reference Python (e.g. coin-change DP trace, BFS level-order count). Tier B prevents the 75-defect tax.
- Aligns with ADR-0045 user-traction principle at a finer granularity (per-program-oracle independence vs per-release-binary independence).
- IP-safe: verify.py is paraphrased reference impl, not LeetCode editorial paste. Same IP boundary as README.md (own words required).

### Negative

- +30-50% authoring time per program at the P7-TEST sprint side. For Tier B (500 programs) this is ~3-5 extra P7 sonnet hours total; well within sprint budget.
- Sprint output volume grows by 1 file per program (verify.py). Manageable.
- verify.py itself can have bugs. Mitigation: P7-TEST + P7-DEV are different agents (TDD pair pattern); DEV's solution.cb running against verify.py-validated oracle is the secondary check. If both verify.py and solution.cb are wrong in the same way, P9 triage catches it during Phase 3.

### Neutral / unknown

- Whether verify.py should also be invoked at CI time (release-readiness style) is open. Initial decision: **no CI invocation** — verify.py is a sprint-time tool, the test.toml is what CI consumes. A future ADR may revisit if drift becomes empirically common.
- Whether Tier C (3816) needs Option C strict-differential rigor (vs verify.py reasonable rigor) is open; deferred to a Tier C planning ADR if Tier C fires.

## Evidence

- `docs/agent/findings/lc100-pattern-c-test-corpus-defects.md` — 15 oracle defects with per-program corrected values.
- `docs/agent/findings/lc100-adsd-f-pattern-candidates.md` §F23-A — systemic codification.
- Sprint 1 commit `2d952e0` — 15 corpus corrections applied; pre/post pass rate 77/100 → 92/100.
- ADR-0045 — user-traction milestone gate (same anti-pattern, release-tag-level surface).

## Cross-references

- ADR-0045 — user-traction milestone gate, per-release independent verification.
- ADR-0047 — LeetCode coverage strategy parent; this is an amendment.
- finding `lc100-pattern-c-test-corpus-defects.md` — empirical evidence base.
- finding `lc100-adsd-f-pattern-candidates.md` — F23-A confirmation.
- memory `feedback_quantitative_claims_verify.md` §"Extension 2026-05-11: sub-agent self-audit can lie" — same family at sub-agent-self-report-level.

## Why this ADR now

Sprint 1 of Option H (LC-100 Tier A fix-pack) closed 15 oracle defects. Without this ADR, Tier B would re-incur the same systemic defect at 5× scale. With this ADR, every Tier B program ships with a mechanical oracle-correctness check that does not depend on the same agent's mental execution. The ADR is small + focused + amends ADR-0047 narrowly; the implementation is sprint-prompt-level (P7-TEST dispatch prompt amendment, no compiler change).
