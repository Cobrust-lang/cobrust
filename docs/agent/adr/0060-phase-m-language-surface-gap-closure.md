---
doc_kind: adr
adr_id: 0060
title: "Phase M frame — language-surface gap closure (5 syntax gaps + 1 OOS)"
status: proposed
date: 2026-05-19
last_verified_commit: b6d536a
supersedes: []
superseded_by: []
relates_to: [adr:0058a, adr:0006, adr:0052a, adr:0023]
discovered_by: P10 Phase M sprint per ADR-0058a §15 F36-driven gap queue
---

# ADR-0060: Phase M frame — language-surface gap closure

## 1. Context

ADR-0058a §15 (F36-driven, 2026-05-19) enumerated **6 language-surface
gaps** between fixture promises in the LLVM-backend test corpus and what
the current Cobrust parser + types crate actually accept. The post-F36
corpus left two `#[ignore]` fixtures (`llvm_type_08_array_i64`,
`llvm_operand_06_deref_ptr`) and four F36-amend renames that still
document the original promise.

This frame ratifies the **closure plan** as three sub-ADRs + one
out-of-scope memo:

| Gap | Sub-ADR | Status |
|---|---|---|
| 1. `i32` narrow-int type | 0060a | accept |
| 2. `i8` narrow-int type | 0060a | accept |
| 3. `None` return type | 0060b | accept |
| 4. `[T; N]` fixed-size array | 0060b | accept |
| 5. `&T` in type-annotation position | 0060b | accept |
| 6. Anonymous struct literal `struct{i64,i64}` | 0060c | OUT-OF-SCOPE |

Constitutional anchors: CLAUDE.md §2.5 §A (compile-time-catch); §2.5 §D
(method-call sugar / training-data overlap — narrow-ints + `&T` annot
are extremely high-frequency LLM-prior shapes); §5.1 (elegant — one way
to do each thing, additive variants only); §6 (atomic commits per
layer).

## 2. §2.5 LLM-first ROI

These 6 gaps are **syntactic patterns LLMs default to writing**.
Empirical evidence:

- LeetCode corpus stress (`tests/lc100/`) author traces: 84/100 fixtures
  initially used PRELUDE str-fn first-arg without `&` (F-line item in
  `finding:leetcode-corpus-parse-int-tok-use-after-move-fixture-debt
  §5.1`). The mechanical refactor needed 226 call-site `&` insertions.
  Lifting `&T` to type-annotation position lets the type signature
  itself encode the borrow contract (LLM prior matches).
- Codegen corpus F34 author traces (`codegen_diff_corpus.rs:443,457,
  483,517`): 4 fixture rename commentaries explicitly note LLM-prior
  surface mismatch (`i32` / `i8` / `None`-return / `struct{...}`).
- ADR-0051 Priority A (`feedback_cobrust_llm_first_design_principle`)
  ranks `&` ergonomics #1 LLM-friendliness deficit.

ROI weighting per gap:

- **0060a (i32 + i8)**: high. Both shapes are aliased by every Rust /
  C / C++ Stack Overflow answer. §2.5 §D method-call sugar.
- **0060b §1 (`-> None`)**: high. Python writes `-> None` constantly;
  Cobrust forbidding it is a needless §2.5 §B (training-data overlap)
  loss.
- **0060b §2 (`&T` annot)**: highest. Direct ADR-0051 Priority A.
- **0060b §3 (`[T; N]`)**: medium. Rust-prior heavy; Python doesn't
  have it. Pays off in numeric Cobrust source.
- **0060c (anonymous struct)**: zero ROI. Tuple + Record already
  cover the use case (§5.1 "one way to do each thing").

## 3. Decision

Three impl sub-ADRs + one OOS memo:

- **ADR-0060a** narrow-int types — i32 + i8 share the same AST /
  type / codegen path; ship together.
- **ADR-0060b** syntax-gap trio — `-> None` return + `&T` annotation +
  `[T; N]` array literal. All parser-side gaps; type system already
  has the needed `Ty::None` / `Ty::Ref` / new `Ty::Array(elem, n)`
  shape. Codegen extends.
- **ADR-0060c** anonymous struct OUT-OF-SCOPE — formal "won't add"
  decision with redirect to tuple / record.

Phase M scope is the §15 queue **only**. Phase M does NOT introduce
new `TypeError::*` variants beyond what 0060a (narrowing-cast) and
0060b (Ref/Array unification) require. Phase M does NOT touch the
LLVM optimization pipeline (0058b scope) or DWARF lowering (0058c
scope).

## 4. Acceptance gates (binding)

- All 5 gap fixtures (`llvm_type_08_array_i64`, `llvm_operand_06_deref_ptr`,
  `llvm_type_02_i64_baseline → llvm_type_02_i32`, `llvm_type_03_i64_passthrough → llvm_type_03_i8`,
  `llvm_type_06_int_return_baseline → llvm_type_06_none_return`) un-ignored
  + PASS.
- Zero regression on Phase H/I/J/K/L baselines.
- LC-100 stress corpus: 100/100 PASS (the empirical §2.5 anchor; no
  fixture rewrite needed since gap closure is additive).
- F36 compliance: every new test fixture name matches its behavior.
- F37 compliance: zero new `#[ignore]` without an explicit finding
  documenting the deferral.

## 5. Sequencing + cross-references

Sub-ADR landing order (sequential to keep impl chain F35-sibling-clean):

1. ADR-0060a impl (parser + types + codegen + tests).
2. ADR-0060b impl (parser + types + codegen + tests).
3. ADR-0060c OOS memo (doc only).
4. ADR-0058a §15 amendment: queue closure cross-reference.

Phase M closure does NOT supersede any prior ADR. It un-stubs the
ADR-0058a §15 queue and is referenced by the closing F36 audit memo.

## 6. Anchors

- 0060-F34: Phase M frame canonical roster
- 0060-F35: sibling 0058a §15 (queue authority)
- 0060-F36: every fixture rename folded back to original promise name
- 0060-F37: any retained `#[ignore]` requires a finding cross-reference

## 7. Cross-references

- ADR-0058a §15 — gap queue authority
- ADR-0058a §4.1 — wave-1 LLVM type table (extended by 0060a/b)
- ADR-0006 §"Type universe" — `Ty::IntN` placement
- ADR-0052a Wave-1 §4.1 — `&` borrow form (paired with 0060b §2)
- ADR-0023 §"Per-MIR-form lowering rules" — Cranelift backend row
  (extended for narrow-ints + array + Ref)
- finding:leetcode-corpus-parse-int-tok-use-after-move-fixture-debt
  §5.1 — empirical `&` ergonomics anchor
