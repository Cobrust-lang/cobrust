---
doc_kind: finding
finding_id: audit-1-codegen-pollution-quarantine-sop
last_verified_commit: 85c7976
dependencies: [adr:0007, adr:0023, adr:0030]
related: [m9-cross-arch-linux-x86_64-validation, codegen-i8-i64-mismatch-at-4-blocks, translator-real-vs-synthetic-status]
---

# Finding: Audit #1 codegen-pollution quarantine SOP

## Hypothesis

In-flight audit #1 sub-agents (Task #35: sonnet `a29961a2908e923b2`
+ Opus `ab5eee69021f44cad`) are running L0..L2 translation of one
tomli function against codex `gpt-5.5`. They were dispatched at HEAD
`b83ea80` / `d2d9852` respectively. Two **separate** codegen bugs
have since been confirmed in those exact baselines:

1. `infer_return_type` Ty::None mishandling for float-via-temp
   arithmetic (silent-wrong-value on macOS arm64; panic on Linux
   x86_64). Documented in
   `docs/agent/findings/m9-cross-arch-linux-x86_64-validation.md`.
   Fix sprint Task #41 in flight.

2. 4+ similar inline compute blocks → Cranelift verifier rejects
   `iadd.i8` with i64 operand; CLI proceeds to emit a binary
   anyway → silent miscompile. Documented in
   `docs/agent/findings/codegen-i8-i64-mismatch-at-4-blocks.md`.
   CLI-hardening Task #42 in flight; codegen narrow-type fix
   Task #43 queued (blocked on #41).

LLM-translated tomli code is highly likely to replicate the
multi-field-conditional pattern that triggers bug #2. Therefore:

- audit #1 L2.behavior FAIL → could be LLM translation quality OR
  codegen pollution; **not attributable to the LLM** until codegen
  is clean.
- audit #1 L2.behavior PASS → could be honestly correct OR silent
  miscompile masking a real fail; **not trustworthy** either.

Neither outcome is signal-bearing in the codegen-polluted state.

## Method

Since CTO cannot SendMessage to in-flight sub-agents (tool not
available in the current environment), the gate must move from
**runtime cancellation** to **merge-time rejection**.

CTO 守闸 protocol (cto_operations_runbook.md) extended:

When `audit-1-tomli-real-llm` and `audit-1-tomli-real-llm-opus`
branches report `[P9-AUDIT1-COMPLETION]`:

### Step Q1 — Verify the binary baseline

```bash
cd <audit-1-worktree>
git log --oneline | head -3
# Confirm the worktree's base commit is BEFORE the codegen fix
# merges. If yes → codegen-polluted.
```

### Step Q2 — Decline merge into main

Do NOT `git merge --no-ff feature/audit-1-tomli-real-llm[-opus] →
main`. Instead, leave the branch standing as a **quarantine
artifact** with a CTO note in `docs/agent/findings/audit-1-tomli-real-llm-result.md`
(once the sub-agent has written it):

> **Quarantine notice (CTO appended):** This finding was generated
> against a codegen-polluted cobrust binary at HEAD `<base-SHA>`.
> Codegen bugs A (`m9-cross-arch-linux-x86_64-validation`) and B
> (`codegen-i8-i64-mismatch-at-4-blocks`) were active in that
> binary. **The L2.behavior PASS/FAIL signal in this finding is
> not attributable to LLM translation quality** until rerun on a
> binary at HEAD ≥ `<post-fix-SHA>`. CTO will dispatch a fixed-
> binary rerun before drawing audit conclusions.

### Step Q3 — Schedule rerun

After Tasks #41 + #42 + #43 all merge, CTO dispatches a **rerun
sprint** (Opus-tier per model rule): same prompt, same chosen
tomli function, fresh worktree at the post-fix HEAD, same
USER_CODEX_API_KEY, fresh tempdir cache_dir. Output a NEW finding
that supersedes the quarantine one. Cross-link both: the
quarantine is the "first attempt" data point; the rerun is the
authoritative §1.2 demonstration.

### Step Q4 — Token budget escalation

If sub-agent's audit-1 token budget was exhausted on the polluted
run, CTO authorises additional Opus budget for the rerun. The
extra cost is attributable to **codegen pollution**, not to the
sub-agent's audit budget. Memory:
`feedback_codegen_pollution_rerun_budget.md` (created on first
escalation event).

## Result

This SOP is **prophylactic**. It hasn't been exercised yet (audit #1
sprints are still running at the time of writing). The procedure
records what will happen when they report back.

## Conclusion

**SendMessage tool unavailability does not weaken CTO control of
audit-#1 trustworthiness.** The merge gate IS the cancellation
point. Polluted findings stay quarantined, never enter main, and
get cleanly superseded by a rerun against fixed binaries.

This is a generalisable pattern: when a tool affordance is missing,
move the enforcement to a phase where you DO have control. CTO
守闸 is the fallback for absent live-cancellation.

## Cross-references

- `m9-cross-arch-linux-x86_64-validation.md` — codegen bug A
- `codegen-i8-i64-mismatch-at-4-blocks.md` — codegen bug B
- `translator-real-vs-synthetic-status.md` — what audit #1 is meant
  to close
- `cto_operations_runbook.md` — extended with this Q1..Q4 procedure
- `feedback_subagent_model_tier.md` — Opus-tier requirement for
  rerun sprint
