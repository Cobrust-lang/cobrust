---
doc_kind: finding
finding_id: lc100-str-use-after-move-regression-from-adr0050c
last_verified_commit: 09006f6
dependencies: [adr:0050c]
discovered_by: CTO post-Wave-2 DG verify 2026-05-16 — DG verify bithma12o on 09006f6 returned 108 failures including 100 LC-100 tests; Mac leetcode_corpus_e2e 10/12 fail with same root cause
severity: P0 (regression from baseline; blocks v0.2.0 readiness per ADR-0050 §"v0.2.0 stable tag binding")
status: open_pending_strategic_decision
related: [predicate-flip-cascade-discovery-deficit, adr:0050c, adr:0050]
---

# Finding: LC-100 leetcode corpus mass-regression from ADR-0050c Str=non-Copy

## Hypothesis

ADR-0050c §"Decision" chose **Option A** (Full-Drop schedule + explicit `__cobrust_str_clone`; Str non-Copy uniformly across operand-level and drop-level). The Phase 2a walk-back (during list[str] DEV recovery) loosened **List** to Copy-at-operand-but-non-Copy-at-drop, but kept **Str** as non-Copy uniformly. The hypothesis: this asymmetry causes every PRELUDE-using Str sequence (`let n = str_len(s); let c = str_at(s, i)`) to trigger UseAfterMove, breaking the ~100 LC-100 corpus programs that use this idiom.

## Method

DG full-workspace verify on `09006f6` (Wave 2 final, post-merge + audit-findings + f3ls29 fix) returned exit 101 with **3182 passed / 108 failed / 14 ignored**. Mac confirmation via `cargo test -p cobrust-cli --test leetcode_corpus_e2e --locked -- test_lc01 test_lc02`:

- `test_lc01_two_sum_oracle_match`: `AmbiguousType { span: ... }` — likely f64 DEV's as-cast inference clashing with two_sum.cb's untyped arithmetic.
- `test_lc02_reverse_string_oracle_match`: `MIR error: UseAfterMove { local: 2 }` — exactly the predicted Str-non-Copy cascade.

Verified source: `examples/leetcode/reverse_string.cb`:
```cobrust
fn main() -> i64:
    let s = input("")
    let n = str_len(s)        # MOVES s under Str=non-Copy
    let i: i64 = n - 1
    while i >= 0:
        let c = str_at(s, i)  # UseAfterMove — s was moved into str_len
        ...
```

Pre-Wave-2 (Str=Copy): `s` is Copy-at-operand, every PRELUDE call reads without consuming. Works.

Post-Wave-2 (Str=non-Copy): the first PRELUDE call consumes `s`. Subsequent reads fail.

Baseline divergence:
- DG verify on `b364c3d` (post-Wave-1): 3127 passed / 0 failed. LC-100 was green.
- DG verify on `09006f6` (post-Wave-2): 3182 passed / 108 failed. LC-100 is red.
- The regression landed between b364c3d and 09006f6 — specifically in `aca5d87` (list[str] DEV recovery merge containing the ADR-0050c Str=non-Copy flip).

108 = 3 ADR-0050c documented carry-forwards (f3ls22/23/25) + 4 f64 cross-arch (f64e13/14/15/33) + 100 LC-100 + 1 misc.

## Result

**This is a P0 regression.** ADR-0050c §"Consequences" enumerated 27 consumers + the audit identified 7 latent consumers as cascade bugs. None of the 11 enumerated PRELUDE Str-helper call sites (`str_len`, `str_at`, `str_eq`, `str_eq_lit`, `str_ord`, `parse_int`, `parse_int_tok`, `count_toks`, etc.) were flagged as "callers of these now break under Str=non-Copy because callers read the same Str multiple times". This is exactly the **F30 predicate-flip cascade discovery deficit** I filed earlier today as an ADSD candidate.

The audit's framing was wrong: it called LC-100 + leetcode failures "NOT related to ADR-0050c" because the recovery agent's own DG verify reported 149 fails-on-DG and the agent attributed them to f64 / cross-arch. But the actual root cause for 100 of those failures is ADR-0050c Str=non-Copy.

## Conclusion — three resolution paths (P10 decision needed)

### Path A — Walk back Str to Copy-at-operand (mirror Phase 2a List walk-back)

- **Change**: `is_copy_type(Ty::Str) → true` at operand level. Keep Str non-Copy at drop level (so `__cobrust_str_drop` still fires at scope exit per ADR-0050c §"Phase 1" drop schedule).
- **Pros**: Single-line MIR predicate flip. Restores LC-100 corpus green. Preserves the entire Drop-schedule discipline. Mirrors the audit-blessed Phase 2a List walk-back precedent.
- **Cons**: Loses compile-time use-after-move detection on Str (`let a = s; let b = s` would compile under Copy-at-operand). Phase G must surface this via explicit borrow forms.
- **ADR impact**: ADR-0050c needs a §"Amendment" addendum documenting the walk-back. The Decision retains Option A's *spirit* (correct drop schedule) while loosening the *letter* (operand-level Copy). Mirrors how Wave 1 ADR-0050b §"Maintenance burden" addendum was attached.
- **Estimated effort**: 1-2 hour CTO doc + 1 line code + Mac smoke + DG verify.

### Path B — Insert `clone()` calls in LC-100 corpus

- **Change**: Manually edit each of ~100 LC-100 programs to insert `str_clone(s)` or similar before each non-first PRELUDE call.
- **Pros**: Honors ADR-0050c's strict Option A.
- **Cons**: Source-level `clone(s)` builtin doesn't exist yet (ADR-0050c §"Phase G" deferred). Even if it did, manually editing 100 user-wedge programs is anti-pattern; the language should not force this on users for a basic PRELUDE pattern.
- **Effort**: Several days. Not recommended.

### Path C — Revert ADR-0050c entirely from main

- **Change**: `git revert aca5d87` (the list[str] DEV merge). Reopens TD-1.
- **Pros**: Restores LC-100 immediately.
- **Cons**: Loses TD-1 closure + list[str] semantic correctness + all 30/33 list_str_e2e tests + the 6 cascade-bug fixes that pre-existed Wave 2 and are now closed. Wave 3 dispatch would need to rebuild ADR-0050c from scratch.
- **Effort**: Brutal. Throws away ~5h of recovery work + audit closure.

### Recommendation

**Path A** — walk-back Str to Copy-at-operand, mirroring Phase 2a List walk-back. The audit blessed the List walk-back on §5.1 "one way" + "preserve PRELUDE shapes without explicit borrow" grounds. The same rationale applies symmetrically to Str. ADR-0050c amendment addendum names the symmetric walk-back as "Phase 2a' — Str Copy@operand walk-back" and documents the cost (deferred compile-time use-after-move detection on Str).

## Cross-references

- `[[../adr/0050c-str-ownership.md]]` — needs amendment.
- `[[../adr/0050-phase-f3-language-completeness-batch.md]]` — Phase F.3 batch frame; §A1 verified-at-HEAD missed this consumer class.
- `[[predicate-flip-cascade-discovery-deficit.md]]` — F30 candidate; this finding is the strongest empirical case yet.
- `examples/leetcode/reverse_string.cb`, `corpus/leetcode/*` — affected programs.
- `crates/cobrust-mir/src/lower.rs::is_copy_type` (lines ~1909-1934 post-2a walk-back) — fix site for Path A.
- DG verify run `bithma12o` on `09006f6` — empirical baseline.
- Mac `cargo test -p cobrust-cli --test leetcode_corpus_e2e` — confirmation.
