---
doc_kind: finding
finding_id: m10-sha-pin-hallucination
last_verified_commit: 4186c8e
dependencies: [adr:0019]
related: [audit-3a-stateful-prompt-design, two-bugs-one-fix-option-c-pattern]
---

# Finding: M10 GitHub Actions SHA-pin hallucination

## Hypothesis

M10 13th-review task prescribed "SHA-pin all 3rd-party GitHub Actions to
immutable commit hashes." The sub-agent (Wave 2 F+G M-batch sprint,
commit `e937037`) executed this task and emitted 6 action references with
40-char hex SHA strings and inline version comments. The hypothesis to
test: are the emitted SHAs real?

## Method

CTO verified each pinned SHA by querying the GitHub API:

```bash
gh api repos/<owner>/<repo>/commits/<sha> --jq '.sha'
```

A real SHA returns the full 40-char commit hash. A hallucinated SHA returns
`404 Not Found` — "No commit found for SHA".

Applied to every SHA-pinned line in `.github/workflows/ci.yml` and
`.github/workflows/release.yml` as they appeared after `e937037`.

## Result

### Verification table (6 pinned actions)

| Action | Pinned SHA prefix | Comment claim | API resolves? |
|---|---|---|---|
| `actions/checkout` | `11bd7190...` | v4.2.2 | YES (verified real) |
| `actions/upload-artifact` | `6f51ac03...` | v4.5.0 | YES (verified real) |
| `dtolnay/rust-toolchain` | `4305c38f...` | stable 2025-01 | **NO (fake)** |
| `Swatinem/rust-cache` | `9bdcdea0...` | v2.7.8 | **NO (fake)** |
| `actions/download-artifact` | `fa0a91b8...` | v4.1.8 | **NO (fake)** |
| `softprops/action-gh-release` | `c95fe148...` (39 chars) | v2.2.1 | **NO (fake)** |

Observations:

- 4 of 6 SHAs do not resolve against the GitHub API.
- `softprops/action-gh-release` SHA is only 39 characters — one digit
  short of a valid 40-char SHA. This is a detectable hallucination
  fingerprint: LLM-generated hex strings frequently have off-by-one
  length errors.
- The 2 verified SHAs (`actions/checkout` and `actions/upload-artifact`)
  appear to be real commit SHAs that exist in those repos. The sub-agent
  likely copied these from a reliable source (they are widely documented)
  while generating the other 4 from statistical pattern-completion.
- GitHub Actions runner reported for every fake-SHA action:
  `Unable to resolve action 'dtolnay/rust-toolchain@4305c38f...', unable to find version`

### Impact

- 13 of 14 CI jobs failed at the first step (action resolution) on both
  `v0.1.0-beta.1` and `v0.1.0` release tags.
- The 1 passing job was `scripts/doc-coverage.sh`, which uses only bash
  and no third-party actions.
- CI was red on the public `v0.1.0` stable release for approximately
  4 wall-clock hours before the user noticed via the "1/14" GitHub
  annotation and flagged it.
- The broken CI appeared on the publicly visible `Cobrust-lang/cobrust`
  repository during the release window, degrading the credibility of
  the v0.1.0 release artifact.

## Root cause

Two failure modes from the ADSD `reference/failure-modes-catalogue.md`
combined:

**F13 — plan-vs-execute coherence gap**: The M10 task plan stated "pin to
SHA from GitHub." The sub-agent executed "generate plausible-looking
40-char hex with a confident `# v2.7.8` comment." The gap between
"fetch from GitHub" and "generate from pattern" is invisible at
plan-reading time — both produce the same textual artifact format (a
40-char hex string with a comment). Verification that the artifact
_actually_ came from GitHub rather than being synthesised was never
triggered.

**F1.1 — declared invariant without verification**: The M10 commit message
described the SHAs as "immutable commit SHAs" and listed each action with
its version. No verification command (`gh api repos/...`) was included in
the dispatch prompt or the commit description. The CTO merged `e937037`
without running `git push && watch CI green` before tagging `v0.1.0`.
The snapshot schema invariant (HEAD freshness) was violated for the 5th
time, in a different form: the "invariant" here was "these SHAs
are valid" — declared via code but never machine-verified before merge.

The root is that any task involving an external version reference (commit
SHA, tag, package version) must include a verification command in the
dispatch prompt. When the prompt omits `gh api repos/.../commits/<sha>`,
the sub-agent has no feedback loop and cannot self-correct.

## Fix

CTO reverted the 4 fake SHAs to tag form in commit `4186c8e`:

- `dtolnay/rust-toolchain@4305c38f...` → `@stable`
- `Swatinem/rust-cache@9bdcdea0...` → `@v2`
- `actions/download-artifact@fa0a91b8...` → `@v4`
- `softprops/action-gh-release@c95fe148...` → `@v2`

The 2 verified real SHAs (`actions/checkout@11bd7190...` and
`actions/upload-artifact@6f51ac03...`) were preserved.

Security trade-off: tag-form pins are moveable by the upstream maintainer.
This is marginally weaker than a verified SHA pin. For a v0.1.0
research-artifact release this is acceptable. The proper SHA-pin path
(Option B from the handoff doc) requires querying the GitHub API at
pin time and adding a CI lint that re-verifies all SHA-pinned actions
on every PR merge — this is the M10.1 follow-up.

This sprint (sonnet batch 2c+3) also adds `scripts/snapshot-lint.sh`
and ADR-0042 to close F1.1 permanently for snapshot schema enforcement.
The action-SHA verification gap (a distinct F1.1 surface) is documented
as follow-up work under ADR-0042 §"Future work."

## Lessons learned

- **Include the verification command in every external-reference prompt.**
  Any dispatch prescribing "pin to SHA" must include `gh api repos/<owner>/<repo>/commits/<sha>`
  as a required verification step before commit. Without it, the sub-agent
  has no grounding signal.
- **Off-by-one SHA length is a hallucination fingerprint.**
  A 39-character SHA is immediately detectable as LLM-generated. CI lint
  for action pins should assert `len(sha) == 40` as a pre-merge gate.
- **Watch CI go green before tagging a release.**
  `git push && gh run watch` is a one-minute gate that would have caught
  this before the `v0.1.0` tag was created. The CTO operations runbook
  should mandate this step explicitly for every tag operation.

## Cross-references

- `4186c8e` — CTO revert commit that fixed the 4 fake SHA pins
- `e937037` — M10 Wave 2 F+G M-batch commit that introduced the hallucinated SHAs
- ADR-0019 §"M10 done-means" — the CLI+CI milestone this task was part of
- ADR-0042 — snapshot-lint enforcement, ships in this same commit (sonnet batch 2c+3)
- `docs/agent/findings/two-bugs-one-fix-option-c-pattern.md` — related methodology
- `docs/agent/findings/audit-3a-stateful-prompt-design.md` — earlier F1.1-class gap in prompt design
- Handoff doc: `review-claude-handoff/handoff-pack/dispatches/post-v0.1.0-final-handoff.md` §1
- ADSD failure-modes-catalogue: `review-claude-handoff` team's `reference/failure-modes-catalogue.md` F13 + F1.1
