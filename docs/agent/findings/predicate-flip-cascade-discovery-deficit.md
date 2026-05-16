---
doc_kind: finding
finding_id: predicate-flip-cascade-discovery-deficit
last_verified_commit: 49009a8
dependencies: [adr:0050c, finding:adr-cross-surface-bug-fix-scope-creep, finding:adr-scope-reality-divergence]
discovered_by: post-Wave-2 audit teammate 2026-05-16 (a15e69b315007f341), F-W2-6
severity: P2 (methodology integrity)
status: open_candidate_for_adsd_F30
related: [adr-cross-surface-bug-fix-scope-creep, adr-scope-reality-divergence, adsd-pair-pattern-impl-gap, lower-constant-str-zero-pointer-m9-stub, fstring-hole-mir-type-dispatch]
---

# Finding: predicate-flip cascade discovery deficit (F30 candidate)

## Hypothesis

When a shared MIR/codegen predicate flips — e.g. `Copy → non-Copy` for `Ty::Str` per ADR-0050c — the stable population of locals exposed to the new predicate path explodes. F29-style §"Consequences" enumeration captures **direct consumers** (call sites of the shared infrastructure) but cannot enumerate **latent consumers** — code paths that existed in the codebase but were never exercised under the old predicate because the predicate gated them off. Recovery wall-time during impl scales with the size of the latent-consumer set.

## Method

Empirical baseline from Cobrust 2026-05-16 ADR-0050c Wave-2 dispatch:

- ADR-0050c §"Consequences" enumerated **27 consumers** via a thorough F29-compliant pre-impl audit.
- list[str] DEV recovery (agent `a2056acb07469204f`) surfaced **7 additional consumers** as cascade bugs that the enumeration missed (named in audit Lane 2 + Lane 3, filed as findings `lower-constant-str-zero-pointer-m9-stub.md`, `fstring-hole-mir-type-dispatch.md`, and the 4 other Wave-2 cascade fixes per merge `aca5d87`).
- Miss rate: **~26%** (7 / 27).

The 7 latent consumers were structurally invisible to F29-style enumeration because:
- Some were M9-era stubs that returned safe placeholders under the old predicate (e.g. `lower_constant(Str)` returning 0). Code-path enumeration found them by symbol but couldn't infer "wrong placeholder under new predicate".
- Some were dispatch sites that branched on a runtime-stable type witness (Cranelift IR value type) that no longer correlated with the MIR type after the flip (e.g. f-string hole dispatching on `i64` Cranelift type because Str pointers happen to be i64 in IR).
- Some were synthetic-slot bookkeeping bugs (e.g. `set_param_count` off-by-one) that produced zero-overhead under the old predicate (which never enumerated non-Copy locals at all) and double-free under the new.

Each latent consumer was a real bug that pre-existed Wave 2; they were masked by the predicate's pre-flip semantics.

## Result

**Pattern confirmed**. The F29 enumeration discipline is necessary but not sufficient for predicate-flip ADRs. The structural blind spot is "consumers that exist but are unreachable under the current predicate state".

## Conclusion — ADSD F30 candidate

> **F30 — Predicate-flip cascade discovery deficit**
>
> Symptom: a sub-ADR proposes flipping a shared MIR / codegen / type-system predicate (e.g. `is_copy_type`, `is_drop_eligible`, `is_pointer_type`). F29-style §"Consequences" enumeration captures direct consumers but misses latent consumers — code paths the old predicate gated off. Impl-time recovery surfaces these as cascade bugs serially; the recovery wall-time scales with the latent-consumer set size, not the direct-consumer set size.
>
> **SOP fix**: every predicate-flip ADR should mandate a "shadow-flip dry-run" workflow:
>
> 1. Land the predicate flip behind a feature flag (`#[cfg(predicate_flip_NN)]` or runtime config) in the design-only ADR commit.
> 2. Run the entire `cargo test --workspace` test matrix with the flag ON.
> 3. Classify each new failure: direct-consumer (enumerated in §"Consequences"), latent-consumer (new), or genuine test broken by the flip semantics.
> 4. Enumerate the latent consumers in a §"Consequences addendum" before removing the flag.
> 5. The pre-flag baseline + post-flag baseline diff IS the F29 enumeration; the audit verifies completeness by comparing diff against §"Consequences".
>
> **Cost**: ~2x design-ADR effort (the shadow-flip itself takes a few hours), but pays back ~10x in impl wall-time by surfacing latent consumers at design time when the cost of enumeration-mismatch is 1 line of doc, not 1 hour of impl debugging.
>
> **Empirical baseline (Cobrust 2026-05-16)**: ADR-0050c Wave 2 — 27 direct consumers + 7 latent consumers = 26% miss rate. list[str] DEV recovery agent stalled at 600s mid-investigation; cascade bugs surfaced serially over ~5h recovery wall-time. A shadow-flip dry-run during ADR-0050c design (P9-E1 sprint) could have surfaced all 7 within 1-2h, allowing the impl PAIR DEV to start with a complete §"Consequences" enumeration.

## Pattern signal

Look for F30 whenever:

1. A sub-ADR proposes flipping a **shared predicate** (a function returning bool that gates MIR / codegen / type-check behavior on type or value shape).
2. The §"Consequences" enumeration uses **static grep** of call sites rather than **runtime-observed** consumer behavior.
3. The codebase has multiple eras of code (e.g. M9 stubs, W2 Phase 3 paths, M-AI surfaces) where different eras gated off the predicate differently.

When the symptom appears: insist on shadow-flip dry-run before §"Consequences" finalization.

## Cross-references

- `[[../adr/0050c-str-ownership.md]]` — empirical baseline.
- `[[lower-constant-str-zero-pointer-m9-stub.md]]` — latent consumer #1.
- `[[fstring-hole-mir-type-dispatch.md]]` — latent consumer #2.
- `[[adr-cross-surface-bug-fix-scope-creep.md]]` — F29 candidate (Wave 1 origin); F30 is the structural successor.
- `[[adr-scope-reality-divergence.md]]` — F27 candidate (Wave 1 origin); F30 extends F27's "verify-at-HEAD" discipline to "verify-under-shadow-flip".
- `[[adsd-pair-pattern-impl-gap.md]]` — F28 (PAIR pattern); F30 is the structural reason why even proper P10-direct PAIR doesn't catch all bugs at TEST time.
- ADSD upstream — `https://github.com/Cobrust-lang/agent-driven-development` (F30 proposal).
