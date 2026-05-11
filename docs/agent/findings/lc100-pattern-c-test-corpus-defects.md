---
doc_kind: finding
finding_id: lc100-pattern-c-test-corpus-defects
last_verified_commit: e91caed
dependencies: [adr:0047]
related: [lc100-pattern-a-rodata-literal-misalignment, lc100-pattern-b-list-of-str-gap]
discovered_by: lc-100-tier-a-stress-sweep
---

# Finding: LC-100 Pattern C — Test corpus defects mask correct implementations

## Hypothesis

ADR-0047 Phase 2 ran an 8-way P7 sonnet TDD pair process: 4 P7-TEST
agents authored paraphrased problem statements + oracle test.toml
fixtures BEFORE 4 P7-DEV agents wrote `solution.cb`. The hypothesis:
TDD pair discipline ensures the oracle is the ground truth and a
.cb failure means the implementation diverges from the spec. LC-100
falsifies this hypothesis: 15 of 23 failures (~65%) are caused by
the oracle being mathematically inconsistent with the
algorithm description in the program's own README — the
implementation is correct; the test data is wrong.

## Method

- Read all 23 `examples/leetcode-stress/<NNN>-<slug>/failure.md` on
  `feature/lc100-stress-sweep` at HEAD `e91caed`.
- Independent grep for "test corpus" phrase in failure.md content.
- Cross-validated each P7-DEV's claim ("test corpus error, case N")
  by re-deriving the correct oracle for a sample of cases.

## Result

### Affected programs (15 of 23 failures = 65% of total failures)

Bucket B1 + B2 (8 programs):

```
examples/leetcode-stress/008-array-third-maximum/failure.md
examples/leetcode-stress/030-hashmap-two-sum-indices/failure.md
examples/leetcode-stress/037-reverse-polish-eval/failure.md
examples/leetcode-stress/039-decode-nested-depth/failure.md
examples/leetcode-stress/053-symmetric-tree/failure.md
examples/leetcode-stress/054-path-sum-exists/failure.md
examples/leetcode-stress/057-lowest-common-ancestor/failure.md
examples/leetcode-stress/059-flatten-tree-to-list/failure.md
```

Bucket B3 + B4 (7 programs):

```
examples/leetcode-stress/061-coin-change-min/failure.md
examples/leetcode-stress/064-house-robber-linear/failure.md
examples/leetcode-stress/067-partition-equal-subset/failure.md
examples/leetcode-stress/074-peak-element-binary-search/failure.md
examples/leetcode-stress/078-koko-eating-speed/failure.md
examples/leetcode-stress/080-count-negative-sorted-matrix/failure.md
examples/leetcode-stress/097-gas-station-circular/failure.md
```

### Defect taxonomy

The 15 corpus defects partition into 4 sub-classes:

#### C1 — Arithmetic / DP miscount (8 cases)

The expected_stdout encodes a value off-by-one or wrong-by-arithmetic
relative to a correct hand-trace of the algorithm:

- **008** array-third-maximum: expected "1" for [5,2,5,1,3] but
  third distinct max is 2 (P7-DEV verified via
  `sorted(set([5,2,5,1,3]),reverse=True)[2]`)
- **061** coin-change-min: expected "4" for amount=27 with
  coins={1,5,10} but minimum is 5 (10+10+5+1+1)
- **064** house-robber-linear: expected "19" for [6,7,1,3,8,2]
  but max non-adjacent sum is 15
- **067** partition-equal-subset: expected "false" for [3,3,3,4,5]
  but {3,3,3} sums to 9 = half(18), so true is correct
- **078** koko-eating-speed: 2 cases miscomputed (K=4 not feasible
  for H=5 with piles=[3,6,7,11], min K=7; K=15 feasible for H=8
  with piles=[30,11,23,4,20], not K=23)
- **080** count-negative-sorted-matrix: expected 7 but 0 is not
  strictly negative; actual count = 6

#### C2 — Algorithm-definition contradiction (4 cases)

The expected_stdout assumes a different algorithm than the README
declares:

- **037** reverse-polish-eval: 5-token sequence with 1 operator
  is not balanced RPN; expected "2" requires a different
  reduction model
- **039** decode-nested-depth: no single formula yields oracle
  across all 5 cases (README's `max(2v,1)` rule and `score(A)+1`
  rule both fail at least one case)
- **053** symmetric-tree: case 2 tree is symmetric by the README's
  mirror definition but expected "false"
- **057** lowest-common-ancestor: README says "including either
  node being an ancestor of the other" but expected output for
  LCA(1,4) treats LCA strictly (excludes self)

#### C3 — Tree / linked-list encoding bug (2 cases)

The parallel-array encoding in test.toml does not represent the
intended tree topology:

- **054** path-sum-exists: 8-node tree expected to contain path
  sum 22 but the encoded tree's paths are {33, 27, 19} — likely
  case-1 oracle authored from the canonical LeetCode tree
  ([5,4,8,11,null,13,4,7,2,null,null,null,1]) but encoded
  incorrectly
- **059** flatten-tree-to-list: 6-node tree has node5 unreachable
  from root (no L/R pointer references index 5), but expected
  output includes node5's value

#### C4 — Ambiguous specification (1 case)

The problem has multiple correct answers but the test fixes one:

- **074** peak-element-binary-search: array [1,2,1,3] has two
  valid peak indices (1 and 3); expected "3" but algorithm finds
  "1" first; both are correct per "find ANY peak"

#### C5 — Constraint violation (1 case)

The input violates a precondition the README states:

- **030** hashmap-two-sum-indices: README says "exactly one valid
  pair" but case-4 input [1,3,5,7] target=8 has two pairs (0,3
  and 1,2); algorithm returns (1,2), oracle expects (0,3)

#### C6 — Total balance miscount (1 case)

- **097** gas-station-circular: case-2 [gas=2,3,4; cost=3,4,3]
  has total gas=9 < total cost=10; no valid start exists, correct
  answer is -1; oracle expected "2"

### Why P7-TEST agents made these mistakes

Hypothesis (from review-claude session 4bb35f43 dispatch context):

1. **Time pressure**: P7-TEST agents authored 30 programs per
   bucket × 5 oracle cases per program = 150 cases per agent in
   a 30-60 min wall-clock window. Hand-tracing each case is
   ~1-2 min; at 150 cases × 1.5 min = 3.75 hr per agent
   theoretical minimum. Sonnet agents under sprint pressure likely
   approximated via "what does the algorithm probably do" rather
   than rigorous trace.
2. **LeetCode-from-memory bias**: paraphrased algorithm
   descriptions may have been written from the test agent's prior
   exposure to LeetCode-canonical examples, with the agent
   transcribing remembered "answer" from a different input.
   Programs 054 + 059 fit this pattern (tree encoding diverges
   from canonical LeetCode tree for the same problem).
3. **No second-reader pass**: the TDD pair structure has P7-DEV
   reading the test.toml, but DEV's task is implementation, not
   oracle verification. Until DEV runs the implementation, no
   one has actually computed the expected_stdout.

### The TDD pair didn't fail; the oracle authorship process did

The TDD pair pattern is sound. The defect is upstream: oracle
authorship without independent verification. A second-reader
pass (P7-VERIFY agent re-computing expected_stdout via Python or
hand trace before DEV starts) would have caught all 15 of these.

### Pattern C is a process finding, not a Cobrust finding

Crucially: **none of these 15 failures indicate a Cobrust language
or codegen defect**. Every one of the 15 .cb implementations
compiles + runs correctly per the README's algorithm description.
The failure is in the test.toml oracle data.

## Conclusion — actionable proposal

### Fix scope

Two parallel remediation tracks:

**Track 1 — corpus fix-pack (1-2 hr)**

Update 15 `test.toml` files with corrected expected_stdout based
on independent hand-trace per the README algorithm:

- For C1 cases (008, 061, 064, 067, 078, 080): recompute via
  Python reference (verified in failure.md notes for each)
- For C2 cases (037, 039, 053, 057): align oracle with README's
  declared algorithm; rewrite README if oracle's intended algorithm
  differs
- For C3 cases (054, 059): fix tree encoding to match the
  canonical LeetCode topology (or update README to describe the
  actual encoded tree)
- For C4 (074): either accept multiple valid answers in oracle
  format (separator-list) or pick inputs with unique peak
- For C5 (030): replace case-4 input with one satisfying
  uniqueness precondition
- For C6 (097): set expected_stdout to "-1" for case-2

**Track 2 — process fix (ADR-0047 amendment)**

Amend ADR-0047 §"Bucketing" or add ADR-0047a:

> P7-TEST agents must include a `verify.py` script in each program
> directory that re-derives expected_stdout from input via a
> reference Python implementation. The verify.py output is the
> source of truth; test.toml is auto-generated from verify.py.
> CI runs verify.py against test.toml at corpus-edit time.

This is a process-level fix that would have prevented all 15
defects. It is also a generalizable pattern for future stress
corpora (Tier B / C).

### Pattern C's contribution to LC-100 pass rate

With Pattern C fixed (corpus corrections only, no compiler change):
- 15 failures → 0 failures
- Pass rate moves from 77/100 → 92/100

If Pattern A is also fixed:
- 84/100 (Pattern A only) + 15 (Pattern C) - 1 (024 still
  Pattern-B-blocked) = 98/100 pass rate
- The remaining 2 = 024 (Pattern B) + ε

This is **the highest-ROI fix in the LC-100 backlog**: 15 programs
turn green with no compiler work, only ~2 hr of corpus correction.
Pattern C should be the FIRST item in any fix-pack.

## ADSD F-pattern candidate (F23 family)

Pattern C is empirical evidence for an ADSD failure mode that may
deserve codification (`F23-candidate: oracle authorship without
independent verification`). The pattern is:

> When a sub-agent authors both the test fixture AND the algorithm
> description, the test fixture inherits the agent's mental-model
> divergence from the truth. Without independent computation
> (Python reference, hand trace, second-reader pass), the test
> fixture is not ground truth — it is the agent's belief about
> ground truth, which is correlated with the algorithm description
> it also authored.

This is a generalizable AI-driven-development failure mode that
applies beyond Cobrust. Recommended for codification into the
ADSD `failure-modes-catalogue.md` as F23 or F24, pending
review-claude's enumeration.

## Cross-references

- ADR-0047 §"Bucketing" — TDD pair binding
- ADR-0047 §"Done means — Phase 2" — gate that admitted the
  defective oracles
- Memory `feedback_subagent_model_tier.md` — sonnet model tier
  binding for P7-TEST agents
- Memory `feedback_quantitative_claims_verify.md` — sister
  observation (sub-agents propagate stale / wrong numerical
  claims when not independently verified)
- review-claude session 4bb35f43 — the dispatch authoring source
  (TDD pair pattern proposed there)
- Finding `lc100-pattern-a-rodata-literal-misalignment.md` —
  sister cluster in this sweep
- Finding `lc100-pattern-b-list-of-str-gap.md` — sister
  cluster in this sweep
