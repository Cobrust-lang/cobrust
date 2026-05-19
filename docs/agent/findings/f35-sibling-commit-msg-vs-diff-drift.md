---
doc_kind: finding
finding_id: f35-sibling-commit-msg-vs-diff-drift
title: "F35-sibling: DEV commit message vs diff drift"
status: ratified_2026-05-19
date: 2026-05-18
last_verified_commit: 1e57b85
discovered_by: P10 retroactive audit — 0055d commit 7100849 incident
severity: P2 (audit surface corruption; git log misleads future agents)
related: [finding:f33-agent-self-disciplinary-rule-skip, finding:f34-pre-candidate-numeric-anchor-degradation-high-churn]
cross_refs: [upstream ADSD PR #1 F-pattern catalogue]
sourced_from: machine-local memory port 2026-05-19 (machine-loss-resilient copy)
---

# F35-sibling: DEV Commit Message vs Diff Drift

## Pattern

DEV agent re-scopes mid-sprint (scope correctly narrows), but commit message
preserves original-spec framing. The diff tells a different story than the
commit subject.

## Incident (0055d `7100849`)

DEV agent dispatched with original §2 scope: "cb-side Rust impl mirror of
`check.rs`" (a `check_cb.rs` Rust module). Mid-sprint, scope was correctly
reduced to:

- 80-test `#[ignore]`-marker deletion
- ADR ratification
- `check.cb` doc-ref expansion (98→1390 lines)

Actual diff = doc + test-ignore work. Commit message:

> `feat(check-cb): synth_expr 19-arm + Ctx + method-table cb-mirror (Wave-3 LARGEST DEV)`

This describes the **original** scope (cb-side Rust impl mirror), not the
**final-form** scope (pseudocode doc-ref expansion). A reader of `git log` sees
a false picture of what landed.

## Why this matters

- Future agents reading `git log` to reconstruct sprint history will believe a
  Rust `check_cb.rs` module was implemented when it was not.
- Tier-1 audit surfaces this as post-merge claim drift (F35 family) — an ADSD
  F-pattern catalogued for systemic prevention.
- Doc-only sprints are especially vulnerable: original spec describes impl
  intent; actual commit is doc/ADR/test-corpus work.

## Correct form for 0055d

```
docs(check-cb): expand check.cb doc-ref 98→1390 lines + un-ignore 80 tests (Wave-3 LARGEST DEV)
```

## Wrong form

```
feat(check-cb): synth_expr 19-arm + Ctx + method-table cb-mirror (Wave-3 LARGEST DEV)
```

(`feat` + "cb-mirror" implies a new Rust module was added; neither is true.)

## Rule

Before `git commit -m`, DEV agent MUST answer:
"Does this message describe what is actually in the diff, or what was in the
original dispatch spec?"

If scope changed mid-sprint:
1. Write the commit message to describe the **final diff** (what files changed
   and why).
2. If the original spec framing is historically useful, add it as a parenthetical
   or ADR note — NOT in the `git commit -m` subject line.

## Scope: when does this apply?

Any DEV sprint where implementation scope narrowed from the original dispatch.
Common triggers:
- "Wave-N already shipped this" discovery mid-sprint
- Scope split (impl deferred, doc/ADR only this sprint)
- Re-categorization from `impl` to `doc-only`

## ADSD catalogue status

Filed as new F-pattern candidate (commit-message surface drift), sibling of F35
(post-merge claim drift in agent narrative). Queued for upstream ADSD catalogue
follow-on PR.
