---
doc_kind: adr
adr_id: 0047
title: "LeetCode coverage strategy — Tier A discovery + B/C ramp decision gate"
status: proposed
date: 2026-05-11
last_verified_commit: TBD
supersedes: []
superseded_by: []
relates_to: [adr:0019, adr:0038, adr:0044, adr:0045, adr:0046]
discovered_by_review: review-claude session 4bb35f43 LC-100 stress sweep dispatch
---

# ADR-0047: LeetCode coverage strategy — Tier A discovery + B/C ramp decision gate

## Context

### Strategic motivation — the 3816 question

User asked 2026-05-11 night, after W2 leetcode wedge landed (10 easy
programs @ `9caef99`, getting-started doc shipped): can Cobrust be
driven to **LeetCode 3816 全通关** (the full canonical problem set,
~3816 problems at present count)?

Two answers exist; both are honest:

- **A — Yes, eventually.** With sufficient ADR-0044a-like binding
  passes, sufficient codegen primitives, and sufficient stdlib
  coverage, every LeetCode problem can be expressed in Cobrust.
- **B — Not now, not as a strategic frame.** 3816 是吞噬注意力的陷阱:
  - **Bug 队列爆炸**: every codegen gap surfaced by a new problem
    becomes an open bug or a `failure.md`. At 3816 problems sampled
    blindly, bug queue grows faster than fix throughput.
  - **W1 (AI translator) sideline**: the AI-native compiler is the
    dual-mandate (constitution §1.2). Multi-month LeetCode grind
    pushes translator work off the roadmap.
  - **LeetCode IP boundary**: redistributing 3816 problem statements
    verbatim is a copyright concern. Cobrust must paraphrase
    algorithm descriptions in own words. Direct mirror-mode is
    not viable.
  - **Coverage ≠ correctness signal**: a high pass rate against
    LeetCode does not translate to a high pass rate against real
    Python programs. The synthetic distribution (algorithm
    interview problems) drifts from the real Python distribution
    (data processing, web scaffolding, automation scripts) — see
    Consequences §"Neutral" + ADSD F23-candidate framing.

review-claude (session 4bb35f43) proposed an evidence-driven counter:
ship a **Tier A 100-题 discovery sweep** with an explicit T+2-day
decision gate. The sweep's purpose is **not coverage** — it is
**bug discovery + language-gap surfacing + W2 wedge extension**
under a tight time budget that does not sideline W1.

This ADR codifies that policy.

### W2 baseline (post-`9caef99`)

| Program | Category | Source path |
|---|---|---|
| `two_sum.cb` | Array + brute-force | `examples/leetcode/two_sum.cb` |
| `reverse_string.cb` | String manip | `examples/leetcode/reverse_string.cb` |
| `fibonacci.cb` | Recursion / DP base case | `examples/leetcode/fibonacci.cb` |
| `valid_parentheses.cb` | Stack | `examples/leetcode/valid_parentheses.cb` |
| `merge_two_sorted_lists.cb` | Two pointers | `examples/leetcode/merge_two_sorted_lists.cb` |
| `maximum_subarray.cb` | DP / Kadane | `examples/leetcode/maximum_subarray.cb` |
| `binary_search.cb` | Binary search | `examples/leetcode/binary_search.cb` |
| `climbing_stairs.cb` | DP fib-family | `examples/leetcode/climbing_stairs.cb` |
| `stock_best_time.cb` | DP greedy | `examples/leetcode/stock_best_time.cb` |
| `roman_to_integer.cb` | Hash map / string scan | `examples/leetcode/roman_to_integer.cb` |

10 programs, all easy tier, all hand-authored using the ADR-0044
source-level stdin/argv surface (`input("")` + `parse_int` + helper
list/str intrinsics). All compile + run + match oracle on the W2
e2e harness. 0/10 medium-tier exercised. 0/10 hard-tier exercised.

This means: **W2 demonstrates the wedge works, not that the language
surface is sufficient for arbitrary algorithmic Python**. The next
question — "what fails when we 10× the corpus + introduce medium
tier?" — is what Tier A answers.

### Constitution alignment

- §1.1 "Python successor with Rust safety" — every problem must be
  expressible in Cobrust syntactic core (the 30 forms per ADR-0003).
  Tier A discovers which non-core syntactic features are missing.
- §1.2 dual mandate — Tier A's 1-2 day cost must NOT push the
  translator (§1.2 W1) roadmap. Time budget is hard-capped.
- §5.2 Scientific — Tier A produces a falsifiable pass-rate signal;
  Tier B/C ramp decision is evidence-driven, not intuition-driven.
- §6 Closed-loop validation — every failing program produces a
  `failure.md` with stderr + suspected cause; no silent failures.
- §7 Milestones M0..M14 are language-internal. ADR-0047 introduces a
  user-traction-adjacent extension that is policy-scoped (not a new
  M-milestone), aligned with ADR-0045's user-traction gate.

### Cross-anchor: ADR-0045 user-traction gate

ADR-0045 codifies that each release tag binds to ≥1 external-user
scenario. The current Capability tier (W2 wedge `two_sum.cb`) is a
single program; Tier A produces a 100-program user-scenario set
that future release tags can cherry-pick from. ADR-0047 is the
**Capability tier's expansion source**: every release tag from
v0.1.3+ can adopt a Tier A `.cb` program as its bound scenario.

### Cross-anchor: ADR-0038 Phase F roadmap

Phase F §F.1 "AI Python 加速器" wedge is the W1 (translator) half.
Phase F §F.2 candidates (stdlib coverage, numerical tier, REPL
extensions) align with what Tier A might expose. ADR-0047 does NOT
amend ADR-0038's milestone list; it adds a 1-2 day side-track that
informs Phase F sequencing.

## Options considered

### Option A — Status quo: W2 wedge at 10 easy, no extension

- Pros:
  - Zero cost. Focus stays on W1 (translator) per ADR-0038.
  - Avoids LeetCode IP boundary entirely.
- Cons:
  - Empirically known: the 10-program W2 corpus exercises only the
    "happy path" of source-level stdin/argv + the 7-helper runtime
    surface listed in `examples/leetcode/README.md`. Whether the
    language is usable for any medium-tier algorithmic program is
    not yet measured.
  - No falsifiable signal for "is the language usable on novel
    algorithmic problems?" Decision to scale (or not) is based on
    intuition — exactly the F19/F20-family anti-pattern that
    ADR-0042 + ADR-0045 codify against.
  - User-traction gap remains for any LeetCode-style audience:
    "Cobrust can do Two Sum" does not generalize to "Cobrust can
    do Container With Most Water".

**Rejected.** The empirical question (what gaps does a 10× corpus
surface?) deserves an empirical answer, time-capped.

### Option B — Tier A 100-题 discovery sweep + decision gate → conditional Tier B / C (CHOSEN)

- **Tier A** (this sprint): 100 programs, 10 algorithm categories
  × 10 programs each, Easy 60 + Medium 40 mix. 4 buckets, 4 P7
  sonnet dev/test pairs in parallel, 1-2 day wall-clock.
- **Decision gate** at T+2 day, applied after Phase 3 triage:
  - **GO Tier B** if pass rate ≥ 70% AND top-5 bug pattern fix-cost
    estimate < 1 day → ramp to 500 programs (~1 week sprint, post
    bug fix-pack)
  - **HOLD for fix-pack** if pass rate < 70% OR bug queue > 20
    distinct patterns → 1-2 day fix-pack sprint first, re-baseline
    Tier A before deciding ramp
  - **SKIP back to W1** if pass rate ≥ 90% (Tier A saturated, ramp
    has marginal ROI) — close the side-track, return to W1
    (translator) roadmap
- **Tier B** (conditional): 500 programs, ramp algorithm category
  coverage to medium-tier breadth + hard-tier introduction.
  ~1-week sprint. Re-decision gate at T+1-week.
- **Tier C** (conditional, conditional): 3816 programs full
  coverage. Multi-month commitment. Only ramped if Tier B closes
  with pass rate ≥ 80% AND no W1-blocking dependencies.

- Pros:
  - **Evidence-driven**: pass rate signal is falsifiable; ramp
    decision has a numeric threshold, not a vibe.
  - **Time-capped**: 1-2 day cost preserves W1 (constitution §1.2)
    as the dual-mandate.
  - **Surfaces gaps systematically**: 4-bucket × 10-category mix
    (see Implementation map) covers Cobrust language surface
    breadth — arrays, control flow, recursion, hashing, etc.
  - **Decision gate prevents注意力陷阱**: if Tier A surfaces a deep
    bug queue, HOLD verdict forces fix-pack before ramp — won't
    let coverage drive bug队列爆炸.
  - **LeetCode IP boundary respected**: algorithm descriptions are
    paraphrased in own words by P7 test agents; no LeetCode
    problem text mirrored.
  - **Reusable**: Tier A `.cb` corpus becomes a regression test
    suite (`examples/leetcode-stress/`) — any future codegen
    refactor must keep the 100-program corpus green.

- Cons:
  - 1-2 day P9 opus + 8 P7 sonnet dev/test pair concurrent → W1
    paused for the sprint window. Mitigated by hard 8-hr Phase 2
    cap (escalate if exceeded).
  - Pass-rate signal value depends on category-mix representativeness.
    The 10 chosen categories (see Implementation map) are justified
    by ADSD F23-candidate framing — "synthetic stress distribution
    drift" — but acknowledged as not equal to "real Python program
    distribution".
  - Decision gate thresholds (70% / 90%) are heuristic. Calibration
    happens once: the first Tier A pass empirically informs whether
    the 70% bar was too tight, too loose, or aligned with the
    bug-fix throughput.

**Chosen.** Evidence-driven ramp policy + time-capped discovery + IP
boundary respected + reusable artifact corpus.

### Option C — All-in Tier C 3816 upfront

- Pros:
  - Maximal coverage; would close the user question definitively.
- Cons:
  - **注意力陷阱**: multi-month LeetCode-driven work pushes W1
    (translator) off the roadmap entirely. Violates dual mandate.
  - **Bug 队列爆炸**: no decision gate means every codegen gap
    surfaced becomes immediate pressure to fix — fix throughput
    cannot match discovery rate.
  - **LeetCode IP boundary**: paraphrasing 3816 problem statements
    in own words is a multi-week task itself (~10 P7 sonnet
    sprints).
  - **Coverage ≠ correctness against real Python distribution**:
    even if Cobrust passes 3816/3816, the synthetic stress
    distribution may not generalize. The signal is high-cost +
    low-actionability.
  - No falsifiable kill-switch: at what pass rate is "Tier C done"?
    The decision becomes asymptotic.

**Rejected** — strategic mis-allocation; pure coverage-driven; lacks
evidence-driven kill-switch.

## Decision

Adopt **Option B**.

### Tier A definition

- **Size**: 100 programs total
- **Difficulty mix**: Easy 60 + Medium 40 (no Hard tier in Tier A;
  hard requires features Cobrust does not yet have — graph algos,
  segment trees, advanced DP forms)
- **Category mix**: 10 algorithm categories × 10 programs each (see
  Implementation map)
- **Wedge target**: each program reads stdin per the ADR-0044
  binding (`input("")` + `parse_int` + the 13-helper runtime
  surface enumerated in `examples/leetcode/README.md`).
  No new compiler primitives may be introduced for Tier A — the
  sprint's purpose is to **measure** the current surface, not
  extend it. Codegen extension proposals surface in Phase 3
  triage as ADR proposals or findings.
- **Bucketing**: 4 buckets × 25 programs each (B1/B2/B3 = 30
  programs each across 3 categories; B4 = 10 programs across the
  remaining 1 category + spare slack). Each bucket = 1 P7 sonnet
  TEST agent (oracle corpus first) + 1 P7 sonnet DEV agent
  (impl after test corpus landed) per TDD pair pattern per
  memory `feedback_subagent_model_tier.md` §"Extension 2026-05-11".

### Decision gate (T+2 day, post Phase 3 triage)

| Pass rate | Bug queue | Verdict | Next action |
|---|---|---|---|
| ≥ 90% | any | **SKIP back to W1** | Close Tier A; ADR-0047 final-stamp; resume W1 (translator) roadmap. Tier A corpus stays as regression test. |
| 70-89% | top-5 fix < 1 day | **GO Tier B** | Plan Tier B (500 programs, ~1 week). Add fix-pack pre-Tier-B if any top-5 bug is BLOCK-severity. |
| 70-89% | top-5 fix ≥ 1 day OR > 20 patterns | **HOLD for fix-pack** | Dispatch fix-pack sprint (1-2 day). Re-baseline Tier A subset (~30 programs) post-fix. Then re-evaluate ramp. |
| < 70% | any | **HOLD for fix-pack** | Same as above; lower threshold means more aggressive fix-pack scope. |

The gate is **policy**, not a fully mechanical decision. User
(P10 CTO) makes the拍板 call based on the Phase 4 `[P9-LC100-COMPLETION]`
report recommendation. The thresholds exist to constrain wishful
thinking, not to remove human judgment.

### LeetCode IP boundary (binding)

- Each `examples/leetcode-stress/<NN>-<slug>/README.md` MUST
  describe the algorithm in **own words**.
- The README MUST NOT include LeetCode's verbatim problem
  description, examples, constraints, or follow-up sections.
- The README format follows `examples/leetcode/README.md` style:
  bracket-tier categorization, paraphrased problem statement (1-2
  paragraphs), input format spec, oracle expectation, run command.
- Spot-check: 5 random programs' READMEs reviewed by P9 (Phase 3
  triage) for IP-boundary compliance. Any verbatim mirror = block
  merge until rewritten.

### Category mix justification (10 categories × 10 programs)

The 10 categories are chosen to maximize Cobrust **language-surface
coverage** per program slot, not to mirror LeetCode's tag taxonomy.
Each category exercises a distinct combination of source-level
features + runtime helpers + codegen primitives:

| Category | Language surface exercised | Bucket |
|---|---|---|
| 1. **Arrays** | list_new/get/set + index iteration + bounds | B1 |
| 2. **Two pointers** | dual-cursor while loops + early termination | B1 |
| 3. **Hash maps** | (currently) emulated via parallel lists; surfaces stdlib-gap pressure for a real hash type | B1 |
| 4. **Stack / Queue** | LIFO/FIFO list operations + push/pop emulation | B2 |
| 5. **Linked list** | self-referential structures (currently) emulated via parallel arrays + index "next pointers" | B2 |
| 6. **Binary tree** | recursive traversal + nullable children emulation | B2 |
| 7. **Dynamic programming** | memoization tables + state transitions + 1D/2D DP | B3 |
| 8. **Binary search** | logarithmic search + mid-computation + boundary handling | B3 |
| 9. **Bit manipulation** | bitwise ops + parity/count + XOR tricks | B3 |
| 10. **Math + Greedy + Recursion** | integer math + greedy choice + tail-recursive idioms | B4 |

10 categories cover the canonical algorithmic surface that any
Python-successor language must handle to feel "Python-equivalent
for algorithmic work". Categories deliberately exclude advanced
graph algorithms, segment trees, and persistent data structures —
those expose gaps Cobrust will not close in Phase F.1, and would
inflate the failure rate without actionable signal at this stage.

**Counter-argument acknowledged**: 10 categories ≠ the LeetCode
tag distribution. A category-frequency-weighted sweep would more
accurately reflect "average algorithmic problem". But this sprint's
goal is **language-surface coverage**, not problem-distribution
mirror. The 10-per-category uniform allocation deliberately
oversamples rare categories (e.g. bit manip, recursion-only) to
maximize gap-surfacing per program slot. Tier B (if ramped) can
re-balance to a frequency-weighted distribution at 500 programs.

## Implementation map (binding)

### Bucket split

| Bucket | Categories (3 each, 10 problems per category) | Programs | TDD pair |
|---|---|---|---|
| **B1** | Arrays + Two pointers + Hash maps | 30 | P7-B1-TEST + P7-B1-DEV |
| **B2** | Stack/Queue + Linked list + Binary tree | 30 | P7-B2-TEST + P7-B2-DEV |
| **B3** | DP + Binary search + Bit manip | 30 | P7-B3-TEST + P7-B3-DEV |
| **B4** | Math/Greedy/Recursion + 0 spare | 10 | P7-B4-TEST + P7-B4-DEV |

(B1+B2+B3 = 90, B4 = 10, total = 100. Difficulty mix: Easy 6 +
Medium 4 per category, totaling Easy 60 + Medium 40 across all
buckets.)

### Directory layout (created by P7 test agents in Phase 2)

```
examples/leetcode-stress/
├── README.md                      # Tier A overview + per-bucket index
├── 001-array-running-sum/
│   ├── README.md                  # paraphrased problem + I/O spec + oracle
│   ├── test.toml                  # oracle test corpus: input + expected stdout
│   ├── solution.cb                # P7 DEV writes after test.toml landed
│   └── failure.md                 # IF solution.cb compile/runtime fails (P7 DEV writes)
├── 002-array-...
└── 100-recursion-...
```

The `test.toml` format follows the existing `examples/leetcode_fixtures/`
convention (TOML keys `input` + `expected_stdout` + `expected_exit_code`).

### Compiler / stdlib touch list — NONE expected

This is a critical constraint: **Tier A does not extend the
compiler**. No new MIR ops, no new HIR forms, no new runtime
helpers. P7 DEV agents who hit a missing primitive write a
`failure.md` documenting the gap; they do NOT extend the
compiler. The decision to extend lands in Phase 3 triage as
candidate ADRs/findings.

### Test integration

A new harness `crates/cobrust-cli/tests/lc100_stress_e2e.rs`
(P7 DEV per bucket adds entries) batch-runs each `solution.cb`
against its `test.toml` and asserts oracle match. Existing
`leetcode_corpus_e2e.rs` machinery (W2 baseline) is the template.

If a `solution.cb` is absent (compile failed pre-link) or its
oracle does not match, the e2e harness records PASS / COMPILE-FAIL
/ RUNTIME-FAIL. Phase 3 aggregates the matrix.

## Backward compatibility

- W2 wedge `examples/leetcode/*.cb` (10 programs at `9caef99`)
  unchanged.
- ADR-0044 stdin/argv surface unchanged.
- All M11..M14 + Phase F.1 milestones unaffected.
- No new public language surface introduced; no PRELUDE change.

## Done means

### Done means — Phase 1 (P9 opus solo, this ADR)

- [x] ADR-0047 authored on `feature/lc100-stress-sweep` worktree;
      status `proposed`.
- [x] ADR roster `docs/agent/adr/README.md` updated with ADR-0047
      row.
- [ ] CTO ratifies via `[P10-RATIFY-0047]`. (Pause point;
      Phase 2 does not start until ratification.)
- [ ] Status flipped `proposed` → `accepted` at CTO ratification
      commit + `last_verified_commit` stamped.

### Done means — Phase 2 (4-way P7 sonnet pair, post-ratify)

- [ ] 4 buckets × {test corpus, dev impl} = 8 P7 sonnet sprints
      complete.
- [ ] `examples/leetcode-stress/` populated with 100 program dirs,
      each containing README.md + test.toml + solution.cb (+
      failure.md if applicable).
- [ ] `crates/cobrust-cli/tests/lc100_stress_e2e.rs` runs the
      full 100-program matrix; CI runs it on every push.
- [ ] LeetCode IP-boundary spot-check passes (5 random READMEs
      reviewed; no verbatim LeetCode text).
- [ ] 5-gate baseline still green on `feature/lc100-stress-sweep`:
      fmt 0 / clippy 0 / build 0 / test green (+ N new tests,
      0 fails, +K ignored for KNOWN_FAIL) / doc-coverage 0.

### Done means — Phase 3 (P9 opus triage)

- [ ] Per-bucket pass rate aggregated.
- [ ] Failure taxonomy: top-5 patterns identified with frequency
      × severity ranking.
- [ ] Per pattern: `docs/agent/findings/lc100-<slug>.md` finding
      authored.
- [ ] F22 / F23 ADSD-candidate entries drafted if surfaced:
  - **F22 candidate**: "coverage drive without bug-fix cadence" —
    pattern of accumulating bugs faster than fixing during
    coverage-driven sprints.
  - **F23 candidate**: "synthetic stress distribution drift" —
    pattern of stress-corpus pass rate not predicting real-program
    pass rate.

### Done means — Phase 4 (P9 opus decision report)

- [ ] `[P9-LC100-COMPLETION]` report posted to CTO.
- [ ] Ramp recommendation: GO Tier B / HOLD for fix-pack / SKIP
      back to W1 — per decision gate table.
- [ ] If GO Tier B: rough Tier B sprint plan (500 programs, ~1
      week).
- [ ] If HOLD: fix-pack dispatch plan.
- [ ] If SKIP: ADR-0047 final stamp "Tier A sufficient; no
      Tier B at this time".
- [ ] CTO ratifies ramp decision before any Tier B / fix-pack
      sprint starts.

## Consequences

### Positive

- **Evidence-driven ramp policy**: numeric pass-rate gate replaces
  intuition / scope creep. Aligns with ADR-0045 user-traction
  systemic prevention.
- **Time-capped discovery**: 1-2 day budget preserves W1 dual
  mandate (§1.2 translator). Hard stop if Phase 2 exceeds 8 hr.
- **Reusable artifact**: `examples/leetcode-stress/` becomes
  regression test suite. Any future codegen refactor must keep
  the 100-program corpus green. This is high-value sediment.
- **Surfaces gaps systematically**: 10-category × 10-program
  matrix exercises language surface breadth that the W2 10-program
  set cannot.
- **F22 / F23 candidate framing**: if the sweep surfaces
  coverage-without-fix-cadence or synthetic-distribution-drift,
  Phase 3 promotes candidates to ADSD F-pattern entries — adds
  empirical evidence to the ADSD catalogue (currently confirmed
  through F21; F22+ slots open).
- **User-traction expansion**: ADR-0045's Capability tier
  scenario pool grows from 1 (Two Sum) to up to 100. Future
  release tags can cherry-pick representative Tier A programs as
  release-binding scenarios.

### Negative

- **W1 paused for 1-2 day window**: the translator (constitution
  §1.2 half) does not progress during the sprint. Acceptable
  because the sprint produces falsifiable signal that informs
  whether to keep extending the language half or to refocus on
  W1.
- **Token cost**: 8 P7 sonnet sprints × ~30-60 min wall-clock
  each, with potential per-program LLM tokens for paraphrased
  README generation and `.cb` authoring. Not constitution §8
  "token cost is not a constraint" violation, but worth tracking.
- **IP-boundary discipline burden**: P9 must spot-check 5 READMEs;
  P7 test agents must internalize the IP rule. Mitigation: every
  P7 test agent prompt includes the IP-boundary constraint
  verbatim.
- **Decision gate thresholds heuristic**: 70% / 90% are calibrated
  by the first Tier A pass. If the calibration is off (e.g. real
  ceiling is 95%, so 90% triggers SKIP when GO would have been
  right), the policy can be amended (ADR-0047a).

### Neutral / unknown

- **Pass-rate-vs-real-Python predictiveness**: synthetic stress
  corpora can pass while real-Python programs fail (ADSD F23
  candidate). The sweep gives a *language-surface* signal, not a
  *real-Python-translator* signal. Phase 3 triage should call out
  this gap explicitly in any decision recommendation.
- **Optimal category mix**: 10 categories uniform-allocated is a
  starting hypothesis. Phase 3 findings may suggest a
  frequency-weighted mix for Tier B; or a feature-gap-driven mix
  (e.g. heavy on hash maps until a real hash type lands).
- **`examples/leetcode/` vs `examples/leetcode-stress/`** directory
  split rationale: keep W2 wedge corpus stable as user-facing
  onboarding (10 programs, all green). Stress corpus is harness-
  facing (100 programs, expected mixed pass/fail). When Tier A
  closes with SKIP verdict, the merge candidate stress corpus
  *might* graduate to `leetcode/` proper if 100% green — but that
  is a Phase 4 follow-up decision, not Tier A.

## Evidence

### Strategic context evidence

- User question 2026-05-11 night: "LeetCode 3816 全通关" — direct
  user prompt.
- review-claude session 4bb35f43 analysis turn — proposed
  evidence-driven Tier A counter-frame; dispatch authored
  `/Users/hakureirm/codespace/Study/review-claude-handoff/handoff-pack/dispatches/2026-05-11-lc100-stress-sweep.md`.
- User Option α 三连闭环 2026-05-11 — pre-authorized LC-100 Tier A
  sprint as next-wave dispatch ("听你的推荐" + closure).

### Prior-art evidence

- ADR-0044 — W2 stdin/argv binding (`9caef99`); 10-program W2
  corpus baseline.
- ADR-0045 — user-traction milestone gate; ADR-0047 expands the
  Capability-tier scenario pool that ADR-0045 governs.
- ADR-0046 — release.yml asset consolidation; downstream of
  ADR-0045's verification policy.
- ADR-0038 — Phase F roadmap; ADR-0047 is policy-scoped + does NOT
  amend the F.1/F.2/F.3 milestone list.
- ADSD F19/F20/F21 — F1 Sediment Family; ADR-0047 may surface F22
  (coverage-without-cadence) and F23 (synthetic-drift) candidates
  in Phase 3 triage.
- Memory `feedback_subagent_model_tier.md` §"Extension 2026-05-11"
  D-matrix — D5 P9 opus + sonnet × 4 pair pattern; LC-100 sprint
  follows this binding.
- Memory `cto_operations_runbook.md` §"Dev/test pair pattern" —
  binding for the 4 P7 test → P7 dev sequencing.

### Existing corpus evidence (W2 baseline @ `9caef99`)

`examples/leetcode/` contents enumerated in Context §"W2 baseline".
All 10 programs have W2 e2e harness pass at HEAD `9caef99`
(verified via CI run `25665683966`).

### Anticipated bucket distribution (binding for Phase 2 P7 prompts)

Bucket B1 (Arrays + Two Pointers + Hash maps), 30 programs:
algorithm slots are filled by P7-B1-TEST agent during Phase 2 from
the agent's own knowledge of standard algorithmic problems —
**not** from LeetCode problem text. Algorithms paraphrased.

Bucket B2 (Stack/Queue + Linked list + Binary tree), 30 programs:
ditto, P7-B2-TEST authors paraphrased algorithm dirs.

Bucket B3 (DP + Binary search + Bit manip), 30 programs: ditto,
P7-B3-TEST.

Bucket B4 (Math + Greedy + Recursion), 10 programs: ditto,
P7-B4-TEST.

Total 100 programs across 4 buckets.

## Cross-references

- **CLAUDE.md** §1.1, §1.2 (dual mandate), §5.2 (Scientific),
  §6 (Closed-loop validation), §7 (milestones).
- **ADR-0019** §"Definition of usable" four-tier anchor — internal
  language anchor; ADR-0047 informs the Capability tier's
  user-scenario pool.
- **ADR-0038** Phase F roadmap §F.1 wedge — ADR-0047 is the
  Capability-side stress complement.
- **ADR-0044** Source-level stdin/argv binding (W2 wedge) — ADR-0047
  reuses the binding; does NOT extend it.
- **ADR-0045** User-traction milestone gate — ADR-0047 expands the
  Capability-tier scenario pool.
- **ADR-0046** release.yml asset consolidation — downstream
  release-readiness gate that consumes ADR-0045-anchored scenarios.
- **ADSD `failure-modes-catalogue.md`** F19/F20/F21 confirmed +
  F22/F23 candidates — Phase 3 triage drafts.
- **review-claude dispatch** `/Users/hakureirm/codespace/Study/review-claude-handoff/handoff-pack/dispatches/2026-05-11-lc100-stress-sweep.md`
  — sprint authoring source.
- **Memory** `feedback_subagent_model_tier.md` §"Extension
  2026-05-11" D-matrix; `cto_operations_runbook.md` §"Dev/test pair
  pattern"; `feedback_quantitative_claims_verify.md` §"Extension
  2026-05-11" self-audit grep rule.

## Why this ADR now

Three converging signals:

1. **User direct prompt** (LeetCode 3816 全通关) — addressing
   honestly requires a policy, not a hand-wave.
2. **W2 wedge baseline landed** (`9caef99`) — first 10 programs are
   green; the next question is the 10× sweep, not the 100×.
3. **ADR-0045 user-traction gate** demands ≥1 Capability scenario
   per release tag — at single-program scarcity (Two Sum only),
   the user-traction policy has insufficient stock. Tier A
   expands the scenario inventory by ~100×.

Without this ADR, the 3816 question gets a vibe-driven answer
("we should do more LeetCode" / "it's a trap"). With this ADR, the
answer is empirical: run 100, measure, decide.

— P9 opus tech-lead, 2026-05-11
