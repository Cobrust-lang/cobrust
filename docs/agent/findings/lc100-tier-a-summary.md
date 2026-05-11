---
doc_kind: finding
finding_id: lc100-tier-a-summary
last_verified_commit: e91caed
dependencies: [adr:0047, adr:0044]
related: [lc100-pattern-a-rodata-literal-misalignment, lc100-pattern-b-list-of-str-gap, lc100-pattern-c-test-corpus-defects, lc100-adsd-f-pattern-candidates]
discovered_by: lc-100-tier-a-stress-sweep
---

# Finding: LC-100 Tier A stress sweep summary — 77/100 pass, 3 patterns close 99/100

## Hypothesis

ADR-0047 dispatched a 100-program LeetCode stress sweep across 10
algorithm categories with the hypothesis that 4 P7 sonnet TDD
pairs in parallel could surface Cobrust's language-surface gaps
in 1-2 day wall-clock. The decision gate posits:

- ≥ 90% pass → SKIP back to W1 (translator)
- 70-89% pass + top-5 fix < 1 day → GO Tier B (500 programs)
- 70-89% pass + top-5 fix ≥ 1 day OR > 20 patterns → HOLD fix-pack
- < 70% → HOLD fix-pack

This finding aggregates Phase 2 results across 4 buckets and
quantifies the gate inputs.

## Method

- Verified `feature/lc100-stress-sweep` HEAD = `e91caed`.
- Verified 100 program directories, 100 `solution.cb`, 100
  `test.toml`, 100 per-program `README.md` + 1 corpus README =
  101 total.
- Counted `failure.md` files: 23.
- Independent grep for the panic signature and "test corpus"
  phrases established Pattern A (8) + Pattern C (15) counts that
  sum to 23 (= total failures); Pattern B (1, overlaps with
  Pattern A in program 024) is a structural gap discovered in
  the same sweep.
- Spot-checked 5 random program READMEs for LeetCode IP boundary
  compliance (verbatim mirror check). Sampled: 001, 050, 091,
  094, 025.

## Result

### Aggregate pass rate (verified by per-bucket DEV reports)

| Bucket | P | CF | RF | TestErr | Bucket total |
|---|---|---|---|---|---|
| B1 (Arrays / Pointers / Hash) | 27 | 0 | 3 | 0 | 30 |
| B2 (Stack / LL / Tree) | 23 | 0 | 7 | 0 | 30 |
| B3 (DP / BSearch / Bit) | 21 | 0 | 9 | 0 | 30 |
| B4 (Math / Greedy / Rec) | 6 | 0 | 3 | 1* | 10 |
| **Total** | **77** | **0** | **22** | **1** | **100** |

*B4's 1 "test corpus error" classification overlaps with the 22
RUNTIME-FAIL — the bucket is shown per-DEV-report categorization
but Pattern C across buckets totals 15 (this finding cluster).

**77/100 = 77% pass rate.**

### Compile-fail rate (independent of runtime errors)

**0/100 compile failures.** Every solution.cb compiles cleanly
with `cobrust build`. This is a strong signal: Cobrust's
type checker + Cranelift codegen pipeline is robust enough to
accept all 100 algorithms expressed in the ADR-0044 surface.

### Failure pattern taxonomy (sums to 23)

| Pattern | Count | Severity | Fix tier | Finding |
|---|---|---|---|---|
| **A**: `.rodata` literal misalignment in `print_no_nl` / `str_at` | 8 | medium | codegen, 4-6 hr | [lc100-pattern-a-rodata-literal-misalignment.md](lc100-pattern-a-rodata-literal-misalignment.md) |
| **B**: `list[str]` language surface gap | 1 (024) | BLOCK (structural) | ≥ 1 day, ADR-grade | [lc100-pattern-b-list-of-str-gap.md](lc100-pattern-b-list-of-str-gap.md) |
| **C**: Test corpus defects (oracle ≠ algorithm) | 15 | high (mask correct impls) | corpus fix, 1-2 hr | [lc100-pattern-c-test-corpus-defects.md](lc100-pattern-c-test-corpus-defects.md) |

Programs 024 + 056 + 069 + 072 + 090 + 093 + 099 + 100 share
Pattern A. Program 024 additionally shares Pattern B. Programs
008, 030, 037, 039, 053, 054, 057, 059, 061, 064, 067, 074, 078,
080, 097 are Pattern C. No 4th pattern surfaced.

The top-5 are really top-3 — the failure distribution clusters
sharply on 3 distinct causes, not the long tail Phase 1 ADR
anticipated.

### IP boundary spot-check

5 randomly sampled program READMEs reviewed for LeetCode-verbatim
text:

| Program | Status |
|---|---|
| 001 array-running-sum | PASS (algorithm paraphrased; prefix-sum description in own words) |
| 050 rotate-linked-list | PASS (algorithm + approach hint in own words) |
| 091 happy-number-cycle-detect | PASS (Floyd's tortoise-hare named but description paraphrased) |
| 094 gcd-euclidean | PASS (begins reading "Given two non-negative integers, compute their greatest common divisor (GCD)") |
| 025 array-sliding-window-max-sum | PASS (algorithm in own words) |

**IP-boundary spot-check: PASS** (5/5 sampled). No verbatim
LeetCode text observed. Full corpus IP audit deferred to CTO
pre-merge if desired.

### Counterfactual pass rate after fix-pack

Three remediation paths:

- **Pattern C corpus fix only (1-2 hr)**: 77 → 92/100 = **92%**
- **Pattern A codegen fix only (4-6 hr)**: 77 → 84/100 = **84%**
  (program 024 still blocked by Pattern B)
- **Pattern A + C combined (5-8 hr, 1 sonnet sprint)**: 77 → 99/100
  = **99%** (program 024 alone blocked by Pattern B)
- **Pattern A + B + C combined (1-2 day, opus-grade for B)**:
  77 → 100/100 = **100%**

The Pattern C fix is the highest-ROI: 15 programs gained for ~2 hr
of work. Pattern A is the second-highest: 7 programs (8 - 1
Pattern-B overlap on 024) for ~5 hr. Pattern B is forward-looking
infrastructure with marginal LC-100 ROI but structural value for
Tier B / C string-heavy algorithms.

### Time budget actual vs planned

- ADR-0047 planned: 1-2 day wall-clock for Phase 2
- Phase 2 actual: 4 atomic DEV commits landed in sequence,
  pre-Phase-3 timestamp; precise wall-clock not measured but
  within plan.
- Phase 3 (this finding cluster): planned ~2 hr, actual ~1.5 hr
  for triage + 5 findings authored (Pattern A + B + C + summary +
  F-pattern candidates).
- Phase 4 (ramp recommendation): planned ~1 hr.

Within budget overall.

## Conclusion — ramp recommendation per ADR-0047 §"Decision gate"

### Gate inputs (final)

| Input | Value |
|---|---|
| Pass rate | 77/100 = 77% (in 70-89% band) |
| Top-5 pattern count | 3 distinct (Pattern A + B + C) |
| Top-pattern fix cost — Pattern C | 1-2 hr |
| Top-pattern fix cost — Pattern A | 4-6 hr |
| Top-pattern fix cost — Pattern B | ≥ 1 day (BLOCK severity) |
| Compile-fail count | 0/100 |
| LeetCode IP boundary | PASS (spot-check 5/5) |

### Recommendation: **HOLD for fix-pack** (3-4 hr)

Per ADR-0047 gate matrix:

> 70-89% + top-5 fix ≥ 1 day OR > 20 patterns → HOLD for fix-pack

Pattern B alone takes ≥ 1 day → HOLD verdict fires by the literal
gate rule.

**However**, the gate is policy not mechanical. A nuanced read:

- Pattern C is 15/23 of the failures = the dominant cluster, and
  it is **not a Cobrust defect** at all — it's a corpus authoring
  defect. Fixing Pattern C is 1-2 hr and decouples cleanly from
  Cobrust.
- Pattern A is a real codegen defect but the fix is 4-6 hr (1
  sonnet sprint), well within "<1 day" GO threshold.
- Pattern B is BLOCK severity but it is **a single-program
  outlier in LC-100**. Excluding 024 (which is Pattern-B-blocked
  even with Pattern A fix), the top-5 pattern fix cost is 5-8 hr
  total (Pattern A + C only). This lands cleanly in the GO Tier B
  band.

Two equally defensible ramp options:

#### Option G — GO Tier B now, fix-pack as part of Tier B

- 77/100 demonstrates the language surface is largely sufficient
- Pattern C is a process defect, not a language defect; the
  Tier B P7 prompts can embed the "verify.py for each oracle"
  pattern (see Pattern C finding §ADR-0047 amendment proposal)
- Pattern A fix is a 1-sprint side-track that can run in parallel
  with Tier B authoring
- Pattern B is acknowledged as a known gap (open ADR-0048
  candidate); Tier B P7 prompts instruct DEV agents to skip
  string-storing problems pending ADR-0048

#### Option H — HOLD fix-pack (1-2 hr corpus + 4-6 hr Pattern A)

- Land Pattern C fix first (1-2 hr): unblocks 15 programs +
  promotes pass rate to 92/100
- Land Pattern A fix second (4-6 hr): unblocks 7 more programs +
  promotes pass rate to 99/100
- Pattern B becomes the only remaining LC-100 blocker; defer to
  ADR-0048 as a Phase F roadmap item
- Re-baseline LC-100 (5-10 sample programs) post-fix to verify
  99/100 prediction
- Re-evaluate GO Tier B after re-baseline

**P9 recommends Option H** — fix-pack first, re-baseline, then GO
Tier B. Rationale:

1. **Pattern C fix is essentially free** (1-2 hr) and recovers
   15 programs. Not fixing it before Tier B would propagate the
   same defect pattern at 5x the scale (500 programs).
2. **Pattern A fix is high-leverage** (4-6 hr) and closes a
   codegen-level defect that would re-surface across Tier B.
   Closing it now preserves Tier B as a measurement, not a
   re-discovery.
3. **ADR-0047 amendment** (P7-VERIFY second-reader pass per
   Pattern C finding) needs to land before Tier B P7 prompts are
   authored.
4. The fix-pack + re-baseline together cost ~1 day, which
   matches the original "1-2 day Phase 2" envelope — the LC-100
   side-track stays time-capped per constitution §1.2 dual mandate.

**CTO 拍板 final**. P9 surfaces both Option G and Option H for
CTO judgment.

### Followups (regardless of Option G or H)

- ADSD F-pattern candidates F22 (coverage-without-fix-cadence) and
  F23 (synthetic-distribution-drift / oracle-without-independent-
  verification) drafted in finding
  [lc100-adsd-f-pattern-candidates.md](lc100-adsd-f-pattern-candidates.md).
  Review-claude codifies into ADSD repo `Cobrust-lang/agent-driven-development`
  post-merge.
- LC-100 corpus becomes a regression test suite per ADR-0047
  §"Reusable" — any future codegen refactor must keep 100/100
  green post-fix-pack.
- W1 (translator) returns to roadmap focus post-fix-pack +
  re-baseline. ADR-0038 §F.1 wedge resumes.

### Escalations

None. The sprint produced clean falsifiable data + actionable
remediation paths. No P0 / P1 bugs surfaced. The bug-fix throughput
is well within fix-pack capacity.

## Cross-references

- ADR-0047 §"Decision gate" — gate this finding feeds
- ADR-0047 §"Done means — Phase 3" — gate that codified finding
  authorship
- ADR-0047 §"Done means — Phase 4" — `[P9-LC100-COMPLETION]`
  report binding
- ADR-0044 — source-level surface that LC-100 exercises
- Finding `lc100-pattern-a-rodata-literal-misalignment.md`
- Finding `lc100-pattern-b-list-of-str-gap.md`
- Finding `lc100-pattern-c-test-corpus-defects.md`
- Finding `lc100-adsd-f-pattern-candidates.md`
- review-claude session 4bb35f43 dispatch — sprint authoring source
