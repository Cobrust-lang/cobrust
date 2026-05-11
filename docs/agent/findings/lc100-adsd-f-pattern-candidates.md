---
doc_kind: finding
finding_id: lc100-adsd-f-pattern-candidates
last_verified_commit: e91caed
dependencies: [adr:0047]
related: [lc100-tier-a-summary, lc100-pattern-c-test-corpus-defects, multi-agent-cobrust-topology]
discovered_by: lc-100-tier-a-stress-sweep
audience: review-claude (for codification into ADSD failure-modes-catalogue.md)
---

# Finding: LC-100 surfaces ADSD F-pattern candidates F22 + F23

## Hypothesis

ADR-0047 anticipated that the LC-100 sweep might surface two ADSD
F-pattern candidates:

- **F22 candidate** — "coverage drive without bug-fix cadence":
  pattern of accumulating bugs faster than fixing during
  coverage-driven sprints
- **F23 candidate** — "synthetic stress distribution drift":
  pattern of stress-corpus pass rate not predicting real-program
  pass rate

This finding evaluates both candidates against LC-100 empirical
data and proposes F22 / F23 codification text for review-claude
to lift into the ADSD repo
(`Cobrust-lang/agent-driven-development/handoff-pack/adsd/failure-modes-catalogue.md`).

## Method

- Reviewed ADR-0047 anticipation language.
- Aggregated LC-100 results: 77/100 pass, 23 failures, 3 patterns
  (8 A + 15 C + 1 B-overlapping-A).
- Compared LC-100 results against the constitution §"AI-Native
  Compiler" L0-L3 closed loop — the dual mandate's W1 half.
- Surveyed prior multi-agent findings
  (`multi-agent-cobrust-topology.md`) for related F-pattern
  precedents.

## Result

### F22 candidate evaluation — "coverage drive without bug-fix cadence"

**Question**: Does the LC-100 sweep exhibit the F22 pattern?

**Answer**: **NO — F22 does not fire on LC-100 as currently
executed**. But F22-candidate is a real failure mode that LC-100
**could have hit had ADR-0047 not codified the decision gate**.

#### Why F22 didn't fire

ADR-0047 hard-coded these defenses against F22:

1. **Time cap**: 1-2 day Phase 2 budget. Pattern: "if Phase 2
   exceeds 8 hr, escalate." Prevents the sprint from becoming
   open-ended coverage chase.
2. **No compiler extension in Phase 2**: P7 DEV agents who hit
   a gap write `failure.md`; they do NOT extend the compiler.
   Bugs are surfaced, not fixed in-flight.
3. **Decision gate at T+2 day**: explicit GO / HOLD / SKIP
   decision after Phase 3 triage. Prevents drift from
   "discovery" to "ongoing coverage maintenance".

These defenses converted what could have been a 6-month LeetCode
grind (coverage-driven, bug队列爆炸) into a 1-2 day measurement
with empirical signal. The pattern was anticipated and prevented.

#### Recommended F22 codification text

> **F22 — Coverage drive without bug-fix cadence**
>
> **Trigger**: Sub-agent or sprint dispatched against a large
> external corpus (e.g., 100+ test programs, 1000+ library
> functions) with implicit goal of "high coverage".
>
> **Symptom**: Bug discovery rate exceeds bug-fix rate by 3x or
> more. The bug backlog grows monotonically across sprint days.
> Sprint extends past initial budget without decision criterion
> for stop. Sub-agents shift focus from "find next bug" to
> "manage bug backlog" without explicit governance.
>
> **Mechanism**: Coverage drives discovery; discovery surfaces
> bugs; bugs add to backlog; backlog incentivizes more discovery
> to "complete" the corpus. The implicit promise of "if we just
> cover N more programs, we'll know what's broken" is unfalsifiable
> — there is no N at which coverage is "enough".
>
> **Counter-pattern (codified in ADR-0047)**:
> - Time-cap Phase 2 with hard escalation trigger (8 hr in LC-100's
>   case)
> - Forbid in-flight bug fixes; require failure.md authorship
> - Mandate a numeric decision gate (% pass rate threshold) at
>   T+budget that produces GO / HOLD / SKIP verdicts
> - Mandate a fix-pack budget before any ramp (Tier B / C)
>
> **Precedent**: ADR-0047 LC-100 Tier A (2026-05-11) — anticipated
> F22 and codified the time-cap + decision-gate counter-pattern.
> Empirically: 23 failures discovered, 0 fixed in-flight, decision
> gate produces actionable ramp recommendation. F22 prevented.
>
> **Cross-references**: ADR-0047 §"Done means — Phase 4",
> Constitution §1.2 dual mandate (W1 vs W2 attention budget)

### F23 candidate evaluation — "synthetic stress distribution drift"

**Question**: Does the LC-100 sweep exhibit F23 pattern?

**Answer**: **PARTIAL — F23-like phenomenon observed, but
manifestation is broader than ADR-0047's anticipation**.

#### What LC-100 actually surfaced

ADR-0047 anticipated F23 as "stress corpus passes; real Python
programs fail". This narrow framing is **not yet falsifiable** —
Cobrust has not yet attempted a real-Python translation through
the AI-Native Compiler's L0-L3 loop (`audit-3a-stateful-prompt-design`
demonstrated mechanism on `tomli::parse_int` but not full
library translation through L3 downstream-validation).

Instead, LC-100 surfaced a **different pattern** that deserves its
own F-slot codification: **oracle authorship without independent
verification**. This is sister-pattern to F23 but mechanistically
distinct.

#### Recommended F23 (revised) codification text

> **F23 — Oracle authorship without independent verification**
>
> **Trigger**: Test fixture (oracle data, expected_stdout, golden
> file) authored by a sub-agent that ALSO authors the algorithm
> description / problem statement / specification. No independent
> computation (Python reference, hand trace, second-reader pass)
> is mandated between authorship and acceptance.
>
> **Symptom**: When implementation is later written by a different
> sub-agent and tested against the oracle, a large fraction (10-65%
> in LC-100's case) of tests fail. Investigation reveals the
> oracle is mathematically inconsistent with the algorithm
> description — the implementation is correct; the test data is
> wrong.
>
> **Mechanism**: A single sub-agent's mental model of an algorithm
> includes both the description and the expected output. Both
> artifacts inherit the agent's belief, which may diverge from
> ground truth. Without an independent computation path between
> the two, the divergence remains hidden until a third agent
> (implementer) exposes it.
>
> **Empirical evidence**: ADR-0047 LC-100 Phase 2 (2026-05-11).
> 4 P7 sonnet TEST agents authored paraphrased problem statements
> + test.toml oracles. 4 P7 sonnet DEV agents implemented.
> Result: 15 of 23 failures (65%) were "test corpus error" —
> the implementation was correct; the oracle was wrong.
>
> **Counter-pattern**:
> - Require P7-VERIFY second-reader role: author runs reference
>   implementation (Python) to derive oracle, then commits the
>   verify script + auto-generated test.toml together
> - CI re-runs verify.py against test.toml at corpus-edit time;
>   divergence fails CI
> - Or split TEST agent role: P7-TEST-SPEC writes problem
>   description; P7-TEST-ORACLE writes test data via reference
>   impl; the two must agree
>
> **Precedent**: ADR-0047 LC-100 Tier A. ADR-0047a amendment
> proposed (not yet authored) to add verify.py mandate to TEST
> agent prompts before Tier B sprint.
>
> **Cross-references**: finding `lc100-pattern-c-test-corpus-defects.md`,
> memory `feedback_quantitative_claims_verify.md` (sister
> observation for numerical-claim verification)

#### The original F23 (synthetic distribution drift) is still a
#### valid candidate — but unmeasured

The ORIGINAL F23 framing ("stress corpus pass rate ≠ real Python
pass rate") cannot yet be evaluated for Cobrust. It requires:

- A real Python library translated end-to-end through L0-L3
- Comparison of synthetic stress pass rate (LC-100) against
  real library translation pass rate

This measurement is **future work**, anchored on the audit-3a
follow-up sprint (real-LLM full-library translation) currently
queued. F23-original should remain "candidate" status pending
that data.

**Recommendation**: split F23 into F23-A (revised, oracle
authorship; codified above) and F23-B (synthetic-distribution
drift; remain candidate pending audit-3a follow-up).

### Other F-pattern observations from LC-100

#### Sub-pattern (no candidate, just observation)

LC-100 reinforced two previously-codified F-patterns from
`multi-agent-cobrust-topology.md`:

- **F-multi-agent-staleness**: P7-B4 DEV agent's bucket-internal
  test corpus error count (1) and Pattern C across-bucket
  count (15) initially differed in the dispatch. Aggregation
  required independent grep to reconcile. Reinforces existing
  catalogue entry.
- **F-quantitative-claims-verify**: P7-DEV failure.md reports
  numerical claims ("Pattern A affects ≥ 8 programs"). CTO
  smoke-check protocol requires independent literal grep
  (`grep -c "misaligned"` = 8 confirmed). Reinforces existing
  memory rule from `feedback_quantitative_claims_verify.md`.

These are not new F22/F23 — they're confirmations of existing
catalogue entries firing again, in expected mechanistic ways.

## Conclusion — handoff to review-claude

This finding does NOT codify F22 / F23 into the ADSD repo
directly. Codification is the review-claude role per the ADSD
repo's authorship policy (Cobrust contributes empirical evidence
+ candidate text; review-claude validates + lifts into
`failure-modes-catalogue.md`).

### Action for review-claude (post-merge)

1. Read this finding + the 3 sibling LC-100 findings (A / B / C
   + summary).
2. Evaluate the proposed F22 + F23-A codification text against
   existing F-pattern catalogue (F19/F20/F21 et al).
3. Land F22 + F23-A into `failure-modes-catalogue.md` if accepted,
   with `precedent: ADR-0047 LC-100 Tier A` anchor.
4. Mark F23-B (synthetic-distribution-drift) as `candidate /
   pending audit-3a follow-up` in the catalogue.

### Action for Cobrust CTO (P10)

1. Acknowledge F22 / F23-A candidate evidence in
   `[P10-RATIFY-LC100]` or equivalent ratification turn.
2. If LC-100 fix-pack proceeds (Option H), ensure the corpus
   amendment (verify.py mandate per Pattern C finding) lands
   atomically with corpus oracle corrections — this is the
   F23-A counter-pattern implementation.
3. Decision on whether to author ADR-0047a (amendment formalizing
   the verify.py mandate for future stress corpora) — P9
   recommends YES; CTO 拍板.

## Cross-references

- ADR-0047 §"Done means — Phase 3" — codifies F22/F23 candidate
  drafting
- Finding `lc100-tier-a-summary.md` — quantitative evidence
- Finding `lc100-pattern-c-test-corpus-defects.md` — F23-A
  primary evidence
- Finding `multi-agent-cobrust-topology.md` — prior 6-pattern
  ADSD codification, F-pattern slot anchor
- Memory `feedback_quantitative_claims_verify.md` — sister
  observation
- ADSD repo `Cobrust-lang/agent-driven-development/handoff-pack/adsd/failure-modes-catalogue.md`
  (review-claude side; codification destination)
