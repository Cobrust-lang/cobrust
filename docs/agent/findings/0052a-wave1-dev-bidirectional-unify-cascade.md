---
doc_kind: finding
finding_id: F31-candidate
title: "ADR-0052a Wave 1 — bidirectional `Ref(T) ↔ T` unify produces 142-failure AmbiguousType cascade"
date: 2026-05-17
status: filed
related_to: [adr:0052a, adr:0052, adr:0051]
discovered_by: v1 + v2 DEV dispatch failures during Wave 1 implementation
adsd_family: F31 (sediment family — predicate-flip / inference-layer transparency)
---

# Summary

**Two consecutive DEV dispatches** (v1 `feature/0052a-dev-rejected-prelude-cascade`, v2 `feature/0052a-dev-v2`) implemented ADR-0052a's **original** §3 + §6 text mandating a **bidirectional `Ty::Ref(T) ↔ T` unify rule** in `crates/cobrust-types/src/infer.rs`. Both produced the SAME **142-failure cargo test cascade** including 100+ LC-100 + f64 + f3ls legacy-program regressions. The cascade root is NOT scope-creep or impl bugs — it is the ADR design itself.

# Cascade categorization (verified 2026-05-17)

Total 142 failures (v2 strict-scope baseline):

| Category | Count | Root cause |
|---|---|---|
| LC-100 `AmbiguousType` in legacy code | 77 | Bidirectional unify lets inference variables bind to both `T` and `Ref(T)`; substitution non-unique |
| LC-100 `UseAfterMove` shifted to wrong sites | 23 | Operand-read path inherits incorrect transparency |
| 0052a well-typed all-fail | 30 | New `&s` programs cascading on same inference ambiguity |
| 0052a F30-witness all-fail | 4 | MIR scan for no-clone fails because type-check broke |
| 0052a e2e fail | 3 | Build-and-run fails at type-check |
| 0052a parse fail | 1 | Edge case in `bg0052a_p03_amp_field_access` |
| f64 fstring regression | 6 | f64 inference variables resolved ambiguously between f64 and Ref(f64) |
| f3ls (Phase F.3 honest-debt) re-fired | 3 | Inference ambiguity surfaced previously-ignored failures |

# Mechanism

Bidirectional unify says: `Ref(T)` unifies with `T` in both directions. This was intended as a "transparency rule" to let PRELUDE Str helpers accept both `s: Str` and `&s: &Str`. But its effect at the inference layer was:

```rust
// Type variable `?V` could resolve to BOTH `T` and `Ref(T)`:
unify(?V, Str)      // OK, ?V := Str
unify(?V, Ref(Str)) // also OK via bidirectional rule!
// → ?V resolution becomes non-unique → AmbiguousType
```

Legacy LC-100 programs without any `&s` expression had their type variables become candidates for both `T` and `Ref(T)` — even when the program never constructed a `Ref(T)`. The inference table couldn't pick a unique witness, so type-check rejected with `AmbiguousType`.

# Fix (revised ADR-0052a §3 + §6 + §13 at `bcf9c7d`)

Replace bidirectional unify with **one-way call-site coercion**:

- `Ty::Ref(T)` and `T` are **distinct types at inference**; no unify-arm between them.
- The coercion lives at the **`synth_call_args` call-arg-binding site only**: when formal param type is `T` and actual is `Ty::Ref(T)`, the type checker drops the `Ref` wrapper locally.
- The coercion is (a) local, (b) unidirectional, (c) scoped to fn-call arg binding (not `let`, return, arithmetic).
- `infer.rs` only gets `(Ref(a), Ref(b)) → unify(a, b)` structural arm (same shape as `(List(a), List(b))`) plus `Subst::apply` walking Ref. NO bidirectional arm.

v3 DEV (`feature/0052a-dev-v3`, merged at `6843a33`) implements this correctly. Result: **0 non-0052a regression** vs main HEAD `bcf9c7d`. 12 0052a-prefix residual failures classified as TEST-author-pattern-errors per Phase F.3 honest-debt precedent.

# ADSD F31 candidate framing

**Pattern**: "Inference-layer transparency rule for new wrapper type produces AmbiguousType cascade in legacy code."

This is a sibling pattern to existing F27 (ADR scope-reality divergence) and F30 (predicate-flip cascade discovery deficit). F31 is a more specific failure mode:

- F30 says: predicate flips (`is_copy_type(Ty) → bool` etc.) produce cascading errors on existing consumers.
- F31 says: introducing a new type wrapper (e.g. `Ref(T)`, future `Mut(T)`, `Option(T)` if it were new) and making it bidirectionally unify with its inner type at inference produces AmbiguousType regardless of whether existing code uses the new wrapper.

**SOP recommendation for future wrapper-type sub-ADRs**:
1. Default to one-way coercion at consumption boundaries (call args, indexing, etc.), NOT bidirectional unify.
2. Forbid `(Wrapper(a), b)` and `(b, Wrapper(a))` unify arms in `infer::unify`.
3. The only allowed wrapper unify is `(Wrapper(a), Wrapper(b)) → unify(a, b)` (structural / same-shape).
4. Pre-dispatch checklist: grep the proposed `infer.rs` diff for non-structural cross-wrapper arms; reject in ADR audit.

# Forensics retained

- v1 branch: `feature/0052a-dev-rejected-prelude-cascade` (had additional scope-creep: HIR PRELUDE auto-injection + CLI build.rs refactor)
- v2 branch: `feature/0052a-dev-v2` (strict scope; same cascade as v1 from ADR design alone)
- ADR pre-revision SHA (last-bidirectional): `23cadf6` (sub-ADR 0052a commit) through `9c89222` (TEST merge); revised at `bcf9c7d`.

# Related findings + cross-references

- `[[adr-scope-reality-divergence]]` (F27) — verified-at-HEAD discipline.
- `[[predicate-flip-cascade-discovery-deficit]]` (F30) — predicate-flip cascade; F31 is its sibling for type-wrapper inference rules.
- `[[adsd-pair-pattern-impl-gap]]` (F28) — F28 strict separation worked correctly in v1+v2; the cascade was NOT a PAIR-pattern violation.
- `[[lc100-str-use-after-move-regression-from-adr0050c]]` — the original LC-100 honest-debt that motivated Wave 1.
