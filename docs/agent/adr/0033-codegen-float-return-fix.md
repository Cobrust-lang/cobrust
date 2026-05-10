---
doc_kind: adr
adr_id: 0033
title: Codegen — fix Ty::None inference gap (P0 cross-arch float-return bug)
status: accepted
date: 2026-05-09
last_verified_commit: 60243ab
supersedes: []
superseded_by: []
dependencies: [adr:0023, adr:0030]
---

# ADR-0033: Codegen — fix Ty::None inference gap (P0 cross-arch float-return bug)

## Context

The M9 cross-arch validation finding
(`docs/agent/findings/m9-cross-arch-linux-x86_64-validation.md`)
confirms a HIGH-severity codegen bug in
`crates/cobrust-codegen/src/cranelift_backend.rs`:

- **Trigger**: any function whose return chain passes through one or
  more `Ty::None` synthetic temps (`_un` / `_bin` / `_callret`,
  introduced by `crates/cobrust-mir/src/lower.rs`).
- **Linux x86_64 (SystemV)**: fatal `unreachable!()` inside
  `cranelift-codegen::isa::x64::inst::emit::CvtFloatToSintSeq` for
  `(Size64, Size8)` (the closure does not handle Size8 destinations).
- **macOS aarch64 (AppleAarch64)**: no panic; silently produces
  wrong float values because the aarch64 truncation path tolerates
  the bogus type pair.

Root cause: `cranelift_backend::operand_ty` for `Operand::Copy(p)` /
`Operand::Move(p)` looks up `body.locals[p.local].ty` (the
**declared** type). For `Ty::None`, `cranelift_scalar_ty` returns
`Some(I8)`. So `_0 = Use(Copy(_un))` (with `_un: Ty::None`) types
the return as I8 even when the actual stored value is F64.
`write_place` (line 1364) then emits `fcvt_to_sint_sat(I8, F64)` to
reconcile, which is what Cranelift x86_64 lowers via the panicking
`CvtFloatToSintSeq` closure.

`infer_local_types` (the codegen-side pre-pass meant to bridge
`Ty::None`) calls into the same `operand_ty` and inherits the same
gap, so chain-depth ≥ 2 (`-(-x)`, `(a + b) * c`, `-a + b`)
also miscompiles. Audit #1 (Task #35, tomli real-LLM E2E) cannot
attribute fail-mode reliably until this is fixed.

## Options considered

1. **Option A (minimal):** patch `infer_return_type` to consult
   `inferred_locals` for `Copy/Move` of a Ty::None local.
   - Pros: smallest diff, fits the finding's stated sketch verbatim.
   - Cons: band-aid. `infer_local_types` itself has the same gap for
     chains of synthetic temps; corpus tests `fr14` / `fr15` /
     `fr16` (depth-2 chains) still miscompile after Option A.

2. **Option B (MIR-level resolution):** rewire `lower.rs` so
   `_bin` / `_un` / `_callret` carry their real `Ty` at MIR time
   (computed from HIR types of operands).
   - Pros: pushes the fix to the architecturally correct layer
     (ADR-0020 invariant: `LocalDecl.ty` fully resolved at MIR).
   - Cons: largest scope. 5+ lowering sites need careful HIR-type
     threading + promotion rules. Risk of cascading breaks across
     the existing 2,430+ test baseline.

3. **Option C (root primitive — chosen):** thread an
   `inferred_locals: &HashMap<LocalId, ir::Type>` through
   `operand_ty` / `rvalue_ty`. Make `infer_local_types` run to a
   fixed-point so chain depth ≥ 2 is closed by iteration. The
   single map then drives both signature inference, return-type
   inference, and Variable declaration.
   - Pros: single-point fix at the root primitive; every consumer
     of `operand_ty` gets the right answer for free; arbitrary chain
     depths handled. ~30-50 LOC.
   - Cons: O(n_locals × n_iters) in inference time — bounded
     (`n_iters` ≤ chain depth, typically 2-3); negligible in
     practice.

## Decision

**Adopt Option C.** Concretely:

1. `infer_local_types` walks to a fixed-point. Each iteration
   re-evaluates every Ty::None local against its rvalue using the
   in-progress map; an iteration that adds nothing ends the loop.
2. `operand_ty` consults `inferred` first; falls back to
   `body.locals[p.local].ty` only when the local is not in the map.
3. `rvalue_ty`, `infer_return_type`, and `body_signature` all take
   the converged map. `define_body` materializes the map once and
   threads it through to every consumer.

Internal-only refactor inside `cranelift_backend.rs`. No MIR
changes, no public-API changes, no feature-flag changes.

Option B remains the correct long-term direction; ADR-0033 closes
the P0 first.

## Consequences

- **Positive**
  - Linux x86_64 panic on the 4 named cross-arch tests
    (`p008_const_float_neg`, `p017_fadd`, `p018_fsub`, `p019_fmul`)
    closed.
  - macOS aarch64 silent miscompile on the same paths closed.
  - The 16-case `crates/cobrust-codegen/tests/float_return_corpus.rs`
    regression net protects depth-2 chain patterns (fr14 / fr15 /
    fr16) that Option A would have left exposed.
  - **Audit #1 (Task #35) becomes attributable to translation
    quality post-fix.** Audit-#1 fail-modes can now be safely
    attributed to LLM translation or tomli specifics, not to a
    silent codegen miscompile under any float-arithmetic path.
  - The Conway-toy stress-test bug
    (`docs/agent/findings/codegen-i8-i64-mismatch-at-4-blocks.md`,
    landed in commit `82c0e00` after this sprint dispatched) has a
    related root cause family (i8/i64 mismatch at chain depth) and
    *may* be partially or fully resolved by this fix as a side
    effect. A separate verification pass after merge confirms.

- **Negative**
  - Inference now runs to fixed-point, costing 2-3× the
    single-pass time. Sub-millisecond per body; not gated.
  - The fix lives at the codegen layer, not the MIR layer.
    ADR-0020's invariant ("LocalDecl.ty fully resolved at MIR")
    technically remains violated by `_un`/`_bin`/`_callret`
    carrying `Ty::None`. Option B is the long-term cleanup.
  - The fix surfaced an orthogonal bug in MIR's `lower_bin`
    division-by-zero assert path (uses `Constant::Int(0)`
    regardless of operand type → produces `fcmp ne <f64>, <i64>`
    IR for float div). Pre-ADR-0033 the verifier accepted the IR
    because the I8-return rejected at emit time first; post-ADR-0033
    the verifier catches it. Float-div is intentionally **not** in
    the ADR-0033 corpus; tracked as a follow-up finding.

- **Neutral / unknown**
  - LLVM backend (`llvm_backend.rs`, `--features llvm`) is not
    touched. Whether the LLVM path has the same gap is open;
    follow-up cross-arch validation under `--features llvm`
    confirms.
  - Renumbering note from the dispatch prompt: ADR-0034 is
    reserved for the audit-#1 hard-bind followup if and when audit
    #1 produces a fail anchor; ADR-0033 is the codegen fix only.

## Acceptance gate (Done means)

1. ✅ `crates/cobrust-codegen/tests/float_return_corpus.rs` exists
   with ≥ 12 cases (16 implemented), all passing post-fix.
2. ✅ The 4 cross-arch-failing tests
   (`p008_const_float_neg`, `p017_fadd`, `p018_fsub`, `p019_fmul`)
   pass on macOS aarch64 cargo test.
3. ✅ `cargo test --workspace --locked` exit 0 on macOS aarch64.
4. ✅ Linux x86_64 cross-arch verification: 60 / 60
   `codegen_well_formed` + 16 / 16 `float_return_corpus` pass on
   `<internal Linux x86_64 validator host>` (per `reference_x86_workstation.md`).
   `cobrust-msgpack::msgpack_fuzz` failed with a 190 GiB allocation
   request — pre-existing, unrelated, not gated.
5. ✅ All 5 gates green on macOS aarch64: `cargo fmt --check`,
   `cargo clippy --workspace --tests -- -D warnings`,
   `cargo build --workspace --locked`, `cargo test --workspace
   --locked`, `bash scripts/doc-coverage.sh`.
6. ✅ ADR-0033 + finding
   `m9-cross-arch-linux-x86_64-validation.md` stamped with
   `last_verified_commit` SHA at fix-commit time.
7. ✅ Atomic commit per constitution §6 (conventional-commits,
   co-author tag).

## Cross-references

- ADR-0023 — M9 codegen ABI + type matrix (the surface this fix lives on).
- ADR-0030 — M11.1 while-leading-if codegen fix (sibling-shape patch).
- `crates/cobrust-codegen/src/cranelift_backend.rs` —
  `infer_return_type`, `infer_local_types`, `operand_ty`,
  `rvalue_ty`, `body_signature_with`, `define_body`.
- `crates/cobrust-mir/src/lower.rs` — synthetic-temp creation sites
  at lines 1239 (`_bin`), 1251 (`_un`), 1190 (`_callret`).
- `docs/agent/findings/m9-cross-arch-linux-x86_64-validation.md` —
  the bug doc.
- `docs/agent/findings/codegen-i8-i64-mismatch-at-4-blocks.md` —
  related Conway-toy stress test bug; verification post-fix is a
  follow-up.
- Cranelift upstream:
  `cranelift-codegen-0.131.1/src/isa/x64/inst/emit.rs:1057` —
  `CvtFloatToSintSeq::cvtt_op` (the panic site).
