---
doc_kind: finding
finding_id: mf31-while-else-not-skipped-on-break
last_verified_commit: b364c3d
dependencies: [adr:0050a]
discovered_by: post-Wave-1 audit teammate 2026-05-16 (a881fa4b4aa1e07be) — Lane 1 break_continue_mir_corpus review (test m19 inline self-disclosure)
severity: P3
status: open
related: [adr:0050a]
---

# Finding: MIR while-else clause is NOT skipped when `break` fires (contradicts ADR-0050a §"Semantics")

## Hypothesis

ADR-0050a §"Semantics" L154-158 specifies that `break` inside a `while ... else: foo` loop must skip the `else` clause (Python-compatible semantics: the `else` runs only on natural loop exhaustion, not on `break`). The hypothesis: the MIR lowering routes `break`'s `Goto(exit_block)` and the `else_block`'s lowered writes to the same `exit_block`, so `break` does NOT skip the `else`.

## Method

Read `crates/cobrust-mir/src/lower.rs:715-722` (while-with-else lowering) and `crates/cobrust-mir/tests/break_continue_mir_corpus.rs:334-351` (`m19_break_in_while_else_skips_else`).

## Result

Confirmed. The `else_block`'s `lower_block` writes are appended to `exit_block`, which is exactly where the `break`-emitted `Terminator::Goto(exit_block)` terminator lands. After `break` fires, control reaches `exit_block` and the `else` body executes — contradicting ADR-0050a §"Semantics".

The MIR test `m19` honestly self-discloses this divergence inline:

> "lower_loop L719-722 the else_block writes ARE appended to exit_block, so we DOC that break currently does NOT skip the else"

The test currently locks the impl-bug as expected behavior rather than the ADR-0050a spec. No regression test asserts the spec-correct semantics.

## Conclusion

**Severity**: P3. Python-incompatibility on `while ... else:` patterns. Relatively rare in the LeetCode wedge audience but real; will surface when users port Python code with this idiom.

**Closure pathway**: MIR `lower_loop` needs a separate `else_block` that is reachable only via the natural loop-exit edge (`cond → exit`), not via the `break → exit` edge. The current single-`exit_block` design conflates both paths.

**Action**:
- Standing P3 finding.
- ADR-0050a follow-up addendum or a new sub-ADR (0050a.1 or M-F.3.0.x) names this as the closure target.
- Test `m19` should be updated to assert spec-correct semantics (else NOT executed after break) once the MIR fix lands; today it asserts the bug-locked behavior.
- Not a Wave 2 blocker.

## Cross-references

- `[[../adr/0050a-loop-control-flow.md]]` §"Semantics" — the contract this MIR lowering violates.
- `crates/cobrust-mir/src/lower.rs:715-722` — the lowering site.
- `crates/cobrust-mir/tests/break_continue_mir_corpus.rs:334-351` — the test that documents the divergence.
- `[[adr-cross-surface-bug-fix-scope-creep.md]]` — F29 ADSD candidate; this finding is a sibling pattern.
