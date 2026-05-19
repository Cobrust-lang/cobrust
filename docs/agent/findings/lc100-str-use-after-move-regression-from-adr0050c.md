---
doc_kind: finding
finding_id: lc100-str-use-after-move-regression-from-adr0050c
last_verified_commit: 09006f6
dependencies: [adr:0050c]
discovered_by: CTO post-Wave-2 DG verify 2026-05-16 — DG verify bithma12o on 09006f6 returned 108 failures including 100 LC-100 tests; Mac leetcode_corpus_e2e 10/12 fail with same root cause
severity: P1 (downgraded from P0 per P10 disposition 2026-05-16 — honest-debt with Phase G closure target)
status: superseded
superseded_by: list-polymorphic-instantiation-ambiguity-root-cause
superseded_on: 2026-05-19
related: [list-polymorphic-instantiation-ambiguity-root-cause, predicate-flip-cascade-discovery-deficit, adr:0050c, adr:0050]
---

# Finding: LC-100 leetcode corpus mass-regression from ADR-0050c Str=non-Copy

> **SUPERSEDED 2026-05-19 by
> `findings/list-polymorphic-instantiation-ambiguity-root-cause.md`**.
>
> The "Str=non-Copy cascade" hypothesis below was empirically falsified.
> The new finding's `list_poly_pure_i64_triple` test demonstrates a
> pure-i64 program — no `&s`, no `str_*` calls, no `f64`, no `as`
> cast — that ALSO fails with `AmbiguousType` on `let nums =
> list_new(n); list_set(nums, ..., ...); list_get(nums, ...)`. The
> true root cause is `instantiate_list_polymorphic` allocating
> independent fresh `Ty::Var`s per `Ty::List(_)` slot and leaving the
> bare-`i64` scalar element slots unconstrained. Fix landed in
> commit `c4d607e` via `instantiate_intrinsic_signature` (shared
> elem var per call site). DG verify post-fix: LC e2e went
> 4 PASS / 8 FAIL → 7 PASS / 5 FAIL (test_lc01 + test_lc02 both OK);
> LC-100 stress went 9 PASS / 94 FAIL → 16 PASS / 87 FAIL (+7).
>
> The Str=non-Copy concern in §"Hypothesis" below remains potentially
> valid for a SUBSET of LC programs that exercise the
> str-with-multiple-reads pattern, but that subset is the b1 batch
> (29 programs still 0/29 PASS) — and the predicted error mode there
> is `UseAfterMove`, not `AmbiguousType`. That residual issue is a
> separate finding queue when LC-100 b1 batch is re-investigated.

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

### Path D — accepted disposition 2026-05-16 (honest-debt; user-directed)

P10/user directed 2026-05-16 ("标记 LC-100 为 honest-debt,继续推进"): treat the LC-100 mass-regression as documented honest-debt mirroring f3ls22's disposition, keep ADR-0050c Option A (Str=non-Copy uniformly) intact, and proceed to Wave 3 dispatch.

**What this means in practice**:
- ADR-0050c Decision stays as Option A. No amendment. Str remains non-Copy uniformly across operand-level and drop-level.
- LC-100 corpus stays failing on integrated main at `09006f6` and any subsequent commit until Phase G closes via explicit-borrow syntax or a source-level `clone()` builtin.
- v0.2.0 stable tag readiness criterion #1 is reframed: M-F.3.0..M-F.3.4 closure measured by §"Done means" in ADR-0050 (test corpus turn-green + ADR accepted), not by LC-100 corpus state. The LC-100 corpus was a *Phase F.1 user-traction wedge* per ADR-0047; Phase F.3 prioritized §1.1 language-half soundness (drop schedule + ownership) over wedge cosmetics. The trade-off is explicit.
- Release-readiness P7 sonnet GO (ADR-0045 user-traction gate) must explicitly enumerate LC-100 as a known regression in the v0.2.0 release notes, with the Phase G closure pointer.
- Wave 3 dispatch unblocks. Dict impl + string stdlib + file IO proceed; the LC-100 surface inherits the same Str=non-Copy semantics by construction.

**ADSD F30 candidate strengthened, not weakened, by this disposition**: the audit + my own conflict-resolution missed the predicate-flip cascade on a 100-program corpus. The Path D acceptance is honest about the cost but does NOT vacate the methodology lesson. F30's proposed SOP fix (shadow-flip dry-run with feature flag before §"Consequences" enumeration is finalized) remains the upstream proposal. The empirical baseline now has a P1 honest-debt receipt to point at.

**Long-term deferral — addendum 2026-05-16 (P10/user re-disposition)**

P10/user directed 2026-05-16 (verbatim): "LC-100 我觉得你留到以后语言更成熟了再重写吧, 现在暂时不管他了, 因为咱们暂时也没人用, 暂时." Translation: leave LC-100 alone until the language matures; nobody's using it right now.

What this means in practice:
- LC-100 corpus is **NOT** a Phase G closure target with any specific binding. It is **indefinite long-term tech debt** to be revisited when the language has external users + a recursive-struct-types + explicit-borrow surface (post-Phase G, possibly v0.3.0+).
- v0.2.0 stable release-readiness gate does NOT enumerate LC-100 as a blocker. Release notes mention it as a known limitation without a closure ETA.
- M-F.3.5's `clone()` builtin remains the inline-clone-at-callsite mitigation idiom; documented in zh/en getting-started §"Step 2.8" — but no batch sprint will retroactively apply it to LC-100 programs at Phase F.3 close.
- The 14 corpus-pattern-error e2e tests across Wave 2/3 (f3ls22/23/25 + f3str16/17/22 + f3fio01/03/05/06/10/11/12 + f3fio_bug03) inherit the same long-term deferral disposition: documented honest-debt, no scheduled closure sprint.

**Why this is a defensible call**:

- Nobody is using the LC-100 corpus today (user attestation). User-traction comes from §1.1 language-half completeness (which Phase F.3 delivers: dict + f64 + list[str] + string stdlib + file IO) and §1.2 AI-native progress (which Phase F.2 delivers).
- The 100-program LC-100 corpus was a Phase F.1 user-traction wedge per ADR-0047. Phase F.3's broader §1.1 surface supersedes the wedge framing.
- ADR-0050c Option A (Str=non-Copy uniformly) buys correct drop schedule + structural ownership soundness — properties that compound across every future language feature. Burning that to make LC-100 green is the wrong trade.

This addendum supersedes the earlier "Path D-prime / Path A / Phase G closure scope" framing.

## Cross-references

- `[[../adr/0050c-str-ownership.md]]` — needs amendment.
- `[[../adr/0050-phase-f3-language-completeness-batch.md]]` — Phase F.3 batch frame; §A1 verified-at-HEAD missed this consumer class.
- `[[predicate-flip-cascade-discovery-deficit.md]]` — F30 candidate; this finding is the strongest empirical case yet.
- `examples/leetcode/reverse_string.cb`, `corpus/leetcode/*` — affected programs.
- `crates/cobrust-mir/src/lower.rs::is_copy_type` (lines ~1909-1934 post-2a walk-back) — fix site for Path A.
- DG verify run `bithma12o` on `09006f6` — empirical baseline.
- Mac `cargo test -p cobrust-cli --test leetcode_corpus_e2e` — confirmation.
