---
doc_kind: finding
finding_id: adr-scope-reality-divergence
last_verified_commit: 30cf2b2
dependencies: [adr:0050, adr:0050a, adr:0050b, adr:0050d]
discovered_by: ADR-0050 design audit teammate 2026-05-16 (read-only opus, agent `afe53e8f443c7ec32`)
severity: P1
status: open_candidate_for_adsd_F27
related: [adsd-F27-adr-pre-dispatch-source-verification-gate]
---

# Finding: ADR scope-reality divergence on Phase F.3 batch frame

## Hypothesis

ADR-0050 (Phase F.3 language-completeness batch, committed 2026-05-16 at `891d235`) prescribed implementation work for 5 P0 features. The hypothesis: an ADR authored at strategic altitude can over-scope when the author doesn't pre-verify which deliverables already exist on `main`.

## Method

A read-only opus audit teammate ran 5 review lanes (constitution alignment / dependency graph / sub-ADR completeness / resource + risk realism / versioning) against ADR-0050 + `main@30cf2b2` + the in-flight Wave 1 P9 spike branches. Every cited file path + line number in the audit was cross-verified by reading source.

Empirical cross-checks performed:

- `grep -nE "Break|Continue|loop_depth"` in `crates/cobrust-mir/src/lower.rs` + `crates/cobrust-types/src/check.rs` + `crates/cobrust-hir/src/tree.rs`.
- `grep -nE "__cobrust_iter_init|__cobrust_iter_next|__cobrust_iter_drop"` in `crates/cobrust-mir/src/lower.rs` + `crates/cobrust-stdlib/src/iter.rs`.
- `grep -nE "Constant::Float|F64|fadd|fcvt|TokenKind::Float|Ty::Float"` in `crates/cobrust-types/src/ty.rs` + `crates/cobrust-codegen/src/cranelift_backend.rs` + `crates/cobrust-frontend/src/lexer.rs`.
- Read of `feature/f3-break-continue:docs/agent/adr/0050a-loop-control-flow.md` and `feature/f3-for-loop:docs/agent/adr/0050b-for-loop-shape.md` spike commits.

## Result

Three of five P0 features were substantially already shipped at HEAD `30cf2b2`:

1. **break/continue** — fully shipped end-to-end (lexer → AST → parser → unparser → HIR → types `loop_depth` reject → MIR `loop_stack` Goto → Cranelift). Codegen diff corpus `diff_form_16_break` + `diff_form_16_continue` already pass. P9-A's spike commit `1998dbe` independently confirmed.
2. **for-loop** — for-protocol operational over list[i64] + list[str]-via-W2-reinterpret since ADR-0044 W2 Phase 2 amendment. `__cobrust_iter_init` / `_next` / `_drop` shipped. ADR-0050's "for-protocol intentionally placeholder" claim is wrong. P9-B's spike commit `909811f` independently confirmed.
3. **f64** — 80% shipped: `Ty::Float`, `Constant::Float(u64)`, full Cranelift F64 codegen (fadd/fcvt + fpromote), lexer Float token incl. exponent notation, Rust-side stdlib math (sqrt/pow/sin/cos/abs/floor/ceil/round + PI/E), `__cobrust_fmt_float`. Remaining gap is **D2 sonnet scope** (`as` cast syntax + PRELUDE math intrinsic + f-string `{:.Nf}` + `inf`/`nan` literals), not "D4 opus 1-week vertical".

Only TD-1 Str-ownership debt (ADR-0050c) and dict impl (Wave 3 per ADR-0050d) survived the audit as honestly large pieces of work.

The original 4-5 week batch estimate revises to **2-3 weeks**. Opus budget reallocates: P9-D f64 sprint downgrades from D4-opus to D2-sonnet.

P9-A and P9-B independently arrived at the same scope-reality reading in their spike commits **before** the audit verdict landed. Two redundant rediscoveries of the same divergence imply the gap is structural, not random.

## Conclusion

**Actionable for ADR-0050** — addendum lands at the same commit as this finding (per ADSD §F2 addendum-not-rewrite). Audit Findings 2.1 + 2.2 + 2.3 + 5.3 + 1.2 close in §"Amendment 2026-05-16 — Audit verdict + scope correction" of ADR-0050.

**Actionable for ADSD upstream methodology** — propose new failure-mode-catalogue entry:

> **F27 — ADR scope-reality divergence**
>
> Symptom: a Phase batch ADR cites "work needed across crates X / Y / Z" without source-code verification, after authoring at strategic altitude. Sub-agents dispatched against the ADR re-discover at spike time that the work is already partly or fully shipped, then either pivot scope on the branch (organic recovery) or implement redundantly (regression). The strategic-altitude vantage is required to author the batch frame but inherently lossy on local source state.
>
> SOP fix: add an "ADR pre-dispatch source-code verification gate" to ADSD §"Two-phase dispatch SOP". Phase 1 CTO spike must include at least 3 representative `grep -nE` calls against the cited crates before the ADR commits. Findings from the verification commit alongside the ADR or amend the ADR before sub-dispatch.
>
> Empirical baseline (2026-05-16): Cobrust ADR-0050 Phase F.3 batch. 3/5 P0 features over-scoped; 2 redundant rediscoveries by P9-A + P9-B before pre-impl audit landed. Audit was the third independent rediscovery + the first to surface the structural pattern.

This finding is **standing open** until ADSD-upstream issue is filed (see https://github.com/Cobrust-lang/agent-driven-development).

## Cross-references

- `[[../adr/0050-phase-f3-language-completeness-batch.md]]` — parent batch ADR + amendment.
- `[[../adr/0050a-loop-control-flow.md]]` — P9-A independent rediscovery of break/continue scope reality (on `feature/f3-break-continue@1998dbe`).
- `[[../adr/0050b-for-loop-shape.md]]` — P9-B independent rediscovery of for-protocol scope reality (on `feature/f3-for-loop@909811f`).
- `[[../adr/0050d-dict-design.md]]` — P9-C dict design ADR (on `feature/f3-dict-design@8466433`), which correctly verified its own scope pre-write.
- `[[../adr/0048-ai-native-framing-reframe.md]]` — Phase F.2 batch frame precedent; did NOT exhibit this divergence because the M-AI surfaces were genuinely net-new (no pre-existing scaffolding).
- ADSD methodology source: `https://github.com/Cobrust-lang/agent-driven-development`.
