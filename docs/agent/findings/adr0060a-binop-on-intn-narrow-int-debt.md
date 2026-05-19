---
doc_kind: finding
finding_id: adr0060a-binop-on-intn-narrow-int-debt
last_verified_commit: 2d20ae5
dependencies: [adr:0060a, adr:0006]
discovered_by: P9 Phase M sprint 2026-05-19 — narrow-int corpus authoring (pm_a03 + pm_a04 ignore)
severity: P2 (additive language-surface follow-up; non-blocking)
status: RESOLVED at 2d20ae5 (2026-05-19 Phase M follow-up sprint)
related: [adr:0060a §3.3 §3.5 §3.6, adr:0006 §"Type universe"]
---

## §0. Resolution (2026-05-19, commit 2d20ae5)

Two minimal `cobrust-types/src/check.rs` edits closed this finding:

1. **`synth_bin` arithmetic family whitelist** — added `Ty::IntN(_)`
   to the result-type match arm (Add/Sub/Mul/Div/FloorDiv/Mod/Pow/MatMul).
   `Ty::IntN(w) + Ty::IntN(w)` now resolves to `Ty::IntN(w)` per
   ADR-0060a §3.2 unification rule. Codegen lowers the BinOp at the
   narrow width directly (LLVM `build_int_add` is width-polymorphic
   on iN).

2. **`ItemKind::Let` + `StmtKind::Let` annotation sites pre-narrow
   integer literals** — when the annotation is `Ty::IntN(_)` and the
   value-expression is a literal-like integer (`Lit::Int(_)` or
   `-Lit::Int(_)`), the synthesised `Ty::Int` narrows to the
   annotation width before unify. The dedicated overflow diagnostic
   (`TypeError::NarrowIntOverflow` per ADR-0060a §3.6) lands later
   as a separate diagnostic-surface item.

Test closure: `pm_a03_i8_add_well_typed` + `pm_a04_intn_is_copy`
un-ignored + PASS. Added `pm_a09_intn_negative_literal_narrows` +
`pm_a10_intn_add_mul_chain` as F34 closure anchors for the
negative-literal + arithmetic-chain sub-cases.

The `i32(...)` / `i8(...)` source-level cast surface + the
`TypeError::NarrowIntOverflow` literal-fit diagnostic remain
ADR-0060a §3.5 §3.6 deferrable items (separate from this finding).

# Finding: BinOp + integer-literal arithmetic does not yet route through `Ty::IntN(_)`

## §1. Empirical anchor

Phase M corpus tests landed two `#[ignore]` slots with this finding ID:

- `crates/cobrust-types/tests/phase_m_type_corpus.rs::pm_a03_i8_add_well_typed`
- `crates/cobrust-types/tests/phase_m_type_corpus.rs::pm_a04_intn_is_copy`

The empirical failure mode at HEAD `e731369` (Phase M impl landed):

```
pm_a03: TypeMismatch { expected: Int, actual: IntN(8), ... }
  on `return (a + b)` where a, b: i8

pm_a04: TypeMismatch { expected: IntN(32), actual: Int, ... }
  on `let x: i32 = 0` where literal `0` synthesises as Ty::Int
```

## §2. Precise root cause

ADR-0060a §3.1-§3.4 (parser + types + codegen) ships the **type
identity** for narrow ints. ADR-0060a §3.5 (cast lowering) and §3.6
(literal-fit guard) declare the additive **value-flow** rules but
those land in a follow-up sub-sprint (cast surface). Today:

- `synth_binop` in `cobrust-types/src/check.rs` unifies the operands
  with `Ty::Int` for the arithmetic family (Add/Sub/Mul/Div/Mod/Pow).
  No corresponding `IntN(_)` arm exists.
- `synth_lit_int` returns `Ty::Int` regardless of the surrounding
  annotation. No literal-fit / annotation-aware narrowing happens.
- No `i32(...)` / `i8(...)` cast token surface exists in `lower_cast`
  beyond the existing `int(...)` / `float(...)` set.

## §3. Classification

Additive language-surface debt — not a bug in the Phase M closure.
The 5 gaps Phase M targets are **type-identity** gaps (the parser
accepting + the type system distinguishing the new shapes). The
value-flow follow-ups (cast lowering + literal narrowing) are a
distinct sub-sprint specified ex-ante in ADR-0060a §3.3-§3.6.

## §4. Resolution plan (ADR-0060a cast-surface sub-sprint)

Three additive code edits:

1. `cobrust-mir/src/lower.rs:1615` extend `lower_cast` token map:
   ```rust
   "i32" => (CastKind::IntNarrow(32), Ty::IntN(32)),
   "i8"  => (CastKind::IntNarrow(8),  Ty::IntN(8)),
   "i16" => (CastKind::IntNarrow(16), Ty::IntN(16)),
   ```

2. `cobrust-mir/src/lib.rs` add `CastKind::IntNarrow(u8)` variant.

3. `cobrust-codegen/src/llvm_backend.rs` + `cranelift_backend.rs`
   add the cast lowering arms (`build_int_truncate` +
   `build_int_s_extend` for LLVM; `ireduce` + `sextend` for Cranelift).

4. `cobrust-types/src/check.rs::synth_binop`: add `(Ty::IntN(w),
   Ty::IntN(w))` arm returning `Ty::IntN(w)`. Per `is_value_type` rule.

5. `cobrust-types/src/check.rs::synth_lit_int`: when surrounding
   annotation is `Ty::IntN(w)`, evaluate literal as i128, fit-check
   against `[-(2^(w-1)), 2^(w-1) - 1]`, and synthesise as the narrow
   type. On overflow fire `TypeError::NarrowIntOverflow { width,
   literal, span, suggestion: ... }` per ADR-0060a §3.6.

## §5. F36 + F37 compliance

- **F36**: fixture names (`pm_a03_i8_add_well_typed`, `pm_a04_intn_is_copy`)
  honestly describe the eventual passing behavior. The `#[ignore]`
  + finding cross-reference records exactly why they don't pass yet.
- **F37**: this finding is the explicit ignore-debt cross-reference
  for both tests, exactly as the F37 rule requires.

## §6. Acceptance signal

When ADR-0060a §3.5 + §3.6 sub-sprint lands, both ignored tests must
be un-ignored and PASS. The `last_verified_commit` field flips from
TBD to the sub-sprint merge SHA.

## §7. Cross-references

- ADR-0060a §3.3 §3.5 §3.6 — value-flow specification
- ADR-0006 §"Type universe" — `Ty::Int` baseline
- `crates/cobrust-types/src/check.rs:2076` — `synth_binop` site
- `crates/cobrust-mir/src/lower.rs:1615` — `lower_cast` site
