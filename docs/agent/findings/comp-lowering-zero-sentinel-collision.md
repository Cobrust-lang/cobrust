---
doc_kind: finding
finding_id: comp-lowering-zero-sentinel-collision
last_verified_commit: b364c3d
dependencies: [adr:0027, adr:0050b]
discovered_by: post-Wave-1 audit teammate 2026-05-16 (a881fa4b4aa1e07be) — Lane 2 "Retirement clean" check
severity: P2
status: open_pending_phase_g_consolidation
related: [adr:0050b, finding:adr-cross-surface-bug-fix-scope-creep]
---

# Finding: list-comprehension lowering still uses iter-protocol 0-sentinel collision

## Hypothesis

ADR-0050b retired the iter-protocol path for `for` loops on a soundness-bug basis: `__cobrust_iter_next` returned `0` to signal exhaustion, which the MIR `SwitchInt { cases: [(SwitchValue::Bool(false), exit_block)], … }` interpreted as exit — collision with legitimate `0` elements in `list[i64]`. The hypothesis: the same iter-protocol path remains in use by list comprehensions and carries the same bug shape.

## Method

Read `crates/cobrust-mir/src/lower.rs:1493-1576` (comprehension lowering) and `crates/cobrust-stdlib/src/iter.rs:278-349` (the iter-protocol runtime shims), and cross-checked against `for_protocol_corpus.rs:6-15` which explicitly acknowledges "comprehension desugar still uses them (ADR-0041 §H6; Phase G will fold these onto the length-bound primitive)".

## Result

Confirmed. List comprehension lowering at `lower.rs:1568-1576` emits:

```
SwitchInt {
    operand: opt_local /* i64 from __cobrust_iter_next */,
    cases: [(SwitchValue::Bool(false), exit_block)],
    otherwise: body_block,
}
```

For a comprehension over a list whose first element is `0` — e.g. `[print_int(x) for x in [0, 1, 2]]` — `__cobrust_iter_next` returns `0` on the first call (the actual element value), MIR routes to `exit_block` immediately, and the comprehension under-iterates. The runtime's `done` flag (iter.rs:249, 330) protects only the *second* `0` after exhaustion — the *first* legitimate-`0` from the underlying list still routes to exit.

This is a **silent miscompile** in user code that uses comprehensions over `list[i64]` containing `0`.

## Conclusion

**Severity**: P2. User-observable silent under-iteration on comprehensions over `list[i64]` containing `0`. Currently unblocked in code; not in any release-gated test.

**Closure pathway options**:

1. **Phase G consolidation** (audit recommendation) — fold comprehension lowering onto the same length-bound primitive `__cobrust_list_len` + `__cobrust_list_get` that ADR-0050b for-loops use. Closes the bug + honors constitution §5.1 "one way to do each thing".
2. **Wave 2 opportunistic fix** — if Wave 2 P9-E2 (list[str]) lowering touches comprehension code paths, fold in the same fix. ADR-0050c Str-ownership flip is a natural carrier.
3. **Intermediate fix** — change `__cobrust_iter_next`'s ABI to return a `(value, done_flag)` pair via two i64 returns or a struct, eliminating the sentinel-overload entirely. Larger blast radius; defer to Phase G.

**Action**:
- Wave 2 P9-E2 dispatch prompt (when CTO writes it) should include this finding as a required read + recommend opportunistic closure if scope permits without expanding the sprint.
- Phase G consolidation ADR (when authored post-v0.2.0) names this as a P0 closure target alongside ADR-0050b §"Future work".

## Cross-references

- `[[../adr/0050b-for-loop-shape.md]]` — supersession that fixed for-loops but left comprehensions on the bug.
- `[[adr-cross-surface-bug-fix-scope-creep.md]]` — F29 ADSD candidate; this finding is the empirical baseline.
- `[[../adr/0027-m12x-codegen-stdlib-amendments.md]]` — original ADR that introduced the iter-protocol path.
- `crates/cobrust-mir/src/lower.rs:1493-1576` — comprehension lowering on the buggy path.
- `crates/cobrust-mir/src/lower.rs:726-875` — for-loop lowering on the fixed path (for diff).
- `crates/cobrust-stdlib/src/iter.rs:278-349` — `__cobrust_iter_*` runtime shims.
- `crates/cobrust-stdlib/tests/for_protocol_corpus.rs:6-15` — honest pre-disclosure comment.
