---
doc_kind: finding
finding_id: adr-cross-surface-bug-fix-scope-creep
last_verified_commit: b364c3d
dependencies: [adr:0050b, finding:comp-lowering-zero-sentinel-collision]
discovered_by: post-Wave-1 audit teammate 2026-05-16 (a881fa4b4aa1e07be) — Lane 2 supersession-soundness review + ADSD discipline check
severity: P1 (methodology integrity)
status: open_candidate_for_adsd_F29
related: [adr-scope-reality-divergence, adsd-pair-pattern-impl-gap, comp-lowering-zero-sentinel-collision, mf31-while-else-not-skipped-on-break]
---

# Finding: Cross-surface bug-fix scope creep under single-Opus PAIR retrofit (F29 candidate)

## Hypothesis

A sub-ADR (here ADR-0050b) identifies a soundness bug in a shipped runtime path and reroutes **its own consumer** around the bug. The narrow-scope discipline that gives the supersession its smallest-correct-increment virtue also leaves **sibling consumers** of the same shared infrastructure on the bug. Under single-Opus PAIR retrofit (F28 mitigation), the corpus author's scope is by definition narrow to the sub-ADR's surface, so the cross-surface gap is structurally invisible at corpus-write time.

## Method

Reviewed ADR-0050b §"Decision" (supersession of iter-protocol for for-loops) + searched for other consumers of `__cobrust_iter_init` / `_next` / `_drop` in the codebase. Cross-checked the ADR's §"Consequences" section for explicit enumeration of remaining consumers.

## Result

Confirmed empirically:

1. **Bug origin**: ADR-0027 §4 iter-protocol returns `i64 0` from `__cobrust_iter_next` to signal exhaustion. MIR-level `SwitchInt { cases: [(SwitchValue::Bool(false), exit_block)], … }` interprets this as exit. Collision: any legitimate `0` element in a `list[i64]` triggers premature exit.

2. **ADR-0050b's fix**: For-loop lowering at `lower.rs:726-875` uses length-bound index (`__cobrust_list_len` + `__cobrust_list_get`) instead. The iter-protocol path is **retired for for-loops**.

3. **Remaining consumer NOT enumerated in ADR-0050b §"Consequences"**: list-comprehension lowering at `lower.rs:1493-1576` continues to use `__cobrust_iter_*` with the same SwitchInt shape and the same 0-collision bug. ADR-0050b §"Future work" mentions Phase G consolidation but does not name comprehension as a current-bug-bearing consumer.

4. **Corpus author scope blindness**: P9-B's TEST corpus (`for_range_e2e.rs` + `well/ill_typed.rs` w55..w70 / i55..i62) covers for-loop semantics exhaustively, including the 0-collision regression. It does **not** cover comprehensions because that surface is scope-adjacent, not in the sprint's brief. Under single-Opus solo TDD (F28 mitigation), the corpus author's awareness is bounded to the sprint's surface.

## Conclusion

**ADSD upstream methodology candidate F29**:

> **F29 — Cross-surface bug-fix scope creep under narrow-sub-ADR supersession**
>
> Symptom: a sub-ADR identifies a soundness bug in shared infrastructure (a runtime ABI, a shared lowering primitive, a stdlib helper) and reroutes its own consumer around the bug, without fixing the runtime path or updating sibling consumers. The smallest-correct-increment virtue of the supersession is undermined by the sibling-consumers-still-bugged residual. Under single-layer sub-agent platforms (F28), the corpus author can't probe sibling surfaces, so the gap is structurally invisible at sprint time.
>
> SOP fix: when a sub-ADR claims to fix a soundness bug in **shared infrastructure**, its §"Consequences" section MUST enumerate **every current consumer of the shared infrastructure** and state per consumer:
> - **also-fixed** (changes propagated in this sprint), or
> - **fixed-later-with-anchor** (finding filed + Phase target + ETA), or
> - **accepted-as-known-debt** (rationale + bound).
>
> Post-sprint audit lane must verify the enumeration is complete by grep'ing for the shared infrastructure's symbol names against the codebase.
>
> Empirical baseline (Cobrust 2026-05-16): ADR-0050b retired the iter-protocol path for `for` loops on a soundness-bug basis but did NOT enumerate comprehension lowering as a remaining consumer carrying the same bug. Closed by finding `comp-lowering-zero-sentinel-collision` for the Cobrust side; F29 stands open as ADSD upstream candidate.

**Action**:
- File `comp-lowering-zero-sentinel-collision.md` (DONE — sibling finding).
- ADR-0050b §"Maintenance burden" addendum that names the comprehension consumer + Phase G consolidation target.
- Future ADSD-upstream issue at `https://github.com/Cobrust-lang/agent-driven-development` proposing F29.

## Pattern signal

Look for F29 whenever:

1. A sub-ADR's §"Decision" includes the phrase "retires X" or "supersedes X" or "routes around bug Y in shared component Z".
2. Z is **shared infrastructure**: a runtime ABI, an MIR lowering primitive, a stdlib helper.
3. The §"Consequences" section names only the immediate consumer's gain, not the sibling consumers' continued exposure.

When the symptom appears: post-sprint audit MUST enumerate via grep before declaring closure.

## Cross-references

- `[[../adr/0050b-for-loop-shape.md]]` — supersession that fixed the immediate consumer but left siblings exposed.
- `[[comp-lowering-zero-sentinel-collision.md]]` — the empirical sibling-bug finding.
- `[[mf31-while-else-not-skipped-on-break.md]]` — sibling pattern: ADR-0050a left an MIR semantics gap honestly disclosed in test m19 but no formal finding pre-audit.
- `[[adsd-pair-pattern-impl-gap.md]]` — F28; F29 is the natural successor when F28 PAIR-retrofit reveals scope-bounded blindness.
- `[[adr-scope-reality-divergence.md]]` — F27; together F27 + F28 + F29 mark three structural ADSD gaps surfaced during the Phase F.3 batch dispatch cycle.
- ADSD upstream — `https://github.com/Cobrust-lang/agent-driven-development`.
