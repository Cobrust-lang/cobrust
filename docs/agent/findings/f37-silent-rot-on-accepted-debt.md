---
doc_kind: finding
finding_id: f37-silent-rot-on-accepted-debt
title: "F37: silent rot on accepted-debt findings"
status: ratified_2026-05-19
date: 2026-05-19
last_verified_commit: 1e57b85
discovered_by: P9 dispatch — list_polymorphic AmbiguousType incident; lc100-str-use-after-move misattribution for 3+ days
severity: P1 (masks real regressions, misdiagnoses, and latent fix opportunities)
related: [finding:f36-fixture-name-vs-behavior-drift, finding:lc100-str-use-after-move-regression-from-adr0050c, finding:list-polymorphic-instantiation-ambiguity-root-cause]
cross_refs: [upstream ADSD PR #1 F-pattern catalogue]
sourced_from: machine-local memory port 2026-05-19 (machine-loss-resilient copy)
---

# F37: Silent Rot on Accepted-Debt Findings

## Pattern

A finding marked `status: accepted_as_honest_debt` absorbs every newly-discovered
test failure that lands on the same surface, without anyone re-investigating
whether the new failure shares the SAME root cause as the original finding.

## Incident (list_polymorphic AmbiguousType, 2026-05-16..19)

- **2026-05-16**: `findings/lc100-str-use-after-move-regression-from-adr0050c.md`
  filed for LC-100 mass-failure, status `accepted_as_honest_debt`, root cause
  attributed to ADR-0050c `Str=non-Copy / UseAfterMove`.
- **2026-05-16..18**: Subsequent DG verify runs (Wave 3, Phase G, H, I, J, K)
  all surfaced LC-100 failures lumped into the "100-program
  `accepted_as_honest_debt`" bucket. No one reviewed WHICH 100 programs or what
  the actual error mode was.
- **2026-05-19**: P9 dispatch investigates `test_lc01_two_sum_oracle_match` in
  isolation. Error: `AmbiguousType { suggestion: "add an explicit type
  annotation" }`. NOT `UseAfterMove`. The `lc100-str-use-after-move` finding was
  wrong about the root cause for at least 7 of the 100 programs (now PASSing
  post-fix), and likely wrong about 70+ of them (since `list_new(n)` without
  annotation is endemic in LC programs).

## The discipline rule

A test that is **NOT** explicitly marked `#[ignore = "<reason>; deferred to
<ticket-or-phase>"]` but FAILS for 3+ days is **NEVER** legitimate
`accepted_as_honest_debt`. Either:

1. The test gets explicitly `#[ignore]`'d with a citation of the relevant finding
   (anchors the deferral to the source code).
2. OR the finding status is `active_p1_blocker` / `active_p0_blocker` (indicates
   it's an open bug that should not be drifting in CI noise).

If neither holds, the failure becomes invisible to CI / audit / P9-dispatch
context, masking:
- Real regressions (newly-broken tests with the same name)
- Misdiagnoses (other root causes the existing finding never considered)
- Latent fix opportunities (the bug may be 5-line fixable if investigated;
  staying in honest-debt indefinitely is the wrong default if cost < 1 day)

## How to apply

When a sub-agent / P9 / P10 sees a finding cited as the reason a test is failing:

1. Cross-check: does the failing test name appear in the finding's §"Cross-references"
   or §"Affected programs"? If not, this finding may not cover that specific test —
   investigate before lumping.
2. Cross-check: does the test have `#[ignore = "..."]` on it that cites this finding?
   If not, the failure is silent rot and should be either (a) repaired or (b) the
   test `#[ignore]`'d with explicit pointer to the finding.
3. Cross-check: run the failing test in isolation
   (`cargo test test_name -- --nocapture`) and verify the error mode matches what
   the finding claims. If the predicted error mode (e.g., `UseAfterMove`) does not
   match the observed mode (e.g., `AmbiguousType`), the finding is empirically wrong
   and must be superseded — NOT used as justification to keep ignoring the failure.

## Status tag matrix

| Test state | Finding status | Verdict |
|---|---|---|
| `#[ignore = "F123 ..."]` + active finding F123 | `accepted_as_honest_debt` | OK |
| NOT `#[ignore]`'d + active finding | `accepted_as_honest_debt` | ANTI-PATTERN — silent rot |
| NOT `#[ignore]`'d + active finding | `active_p1_blocker` | OK (open bug) |
| `#[ignore]`'d + active finding | `active_p1_blocker` | Mild contradiction — pick one |
| `#[ignore]`'d + finding superseded | `superseded` | OK; un-ignore on next sprint |

## Cross-references

- [[finding:list-polymorphic-instantiation-ambiguity-root-cause]] — the new finding
  (2026-05-19) that superseded the misattribution.
- [[finding:lc100-str-use-after-move-regression-from-adr0050c]] — the superseded
  finding (was `accepted_as_honest_debt` from 2026-05-16 to 2026-05-19).
- [[finding:f36-fixture-name-vs-behavior-drift]] — sibling failure mode where the
  test SHAPE drifts; this F37 is about the test STATUS drifting.
- `feedback_quantitative_claims_verify` — CTO must grep numerical claims pre-merge;
  F37 is the qualitative-claim analogue (verify error-mode claims pre-supersede).
