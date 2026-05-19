---
doc_kind: adr
adr_id: 0058d
parent_adr: 0058
title: "Phase K Strand #4 — JIT/AOT MIR→Cranelift lowering convergence (eliminate cobrust-jit lower.rs drift surface)"
status: accepted
date: 2026-05-19
phase: Phase K Strand #4
last_verified_commit: 0590731
supersedes: []
superseded_by: []
relates_to: [adr:0056a, adr:0058, adr:0058a, adr:0023]
discovered_by: P9 — audit ae2316f1c51dbd6be Gate 7 + ADR-0056a §13 noted-debt
ratification_path: P9 ADR review; ratifies on impl-merge gate
---

# ADR-0058d: JIT/AOT MIR→Cranelift lowering convergence

## 1. Motivation

ADR-0056a §13 (impl-time amendment, `710fadd`) shipped `cobrust-jit`
as a separate workspace crate with its own minimal MIR→Cranelift
lowering at `crates/cobrust-jit/src/lower.rs` (430 LOC). The amendment
explicitly named the convergence point as deferred:

> §3.2's `CodegenMode { Aot, Jit }` enum is **deferred** to ADR-
> 0056b's `lower_module<M: ClifModule>` extraction sprint —
> wave-1's standalone `JitEngine` is the cleaner first step.

ADR-0056b shipped wave-1 control-flow + stdlib for `cobrust-codegen`
without doing the extraction. The audit
`ae2316f1c51dbd6be` Gate 7 ("JIT/AOT convergence") + the original
0056a §13 noted-debt entry name the drift risk explicitly: every
MIR feature added to `cobrust-codegen` post-wave-1 widens the
silent-divergence window. cobrust-jit lower.rs currently
handles a strict subset (Int / BinOp Add+Sub+Mul / UnOp Neg+Plus /
Place::local without projections / Return+Goto+Unreachable).
cobrust-codegen handles the full M11/M12 wave (control-flow,
calls, runtime helpers, dicts, lists, strings, floats). The
narrow JIT surface is intentional — REPL wave-1 only needs
arithmetic — but the long-term contract from 0056a §6 ("JIT
lowers IDENTICALLY to AOT") cannot be honored without a shared
lowering substrate.

This ADR is Phase K Strand #4: extract the **shared MIR→Cranelift
IR lowering substrate** from `cobrust-codegen/src/cranelift_backend.rs`
as module-generic `pub` fns, then refactor `cobrust-jit/src/lower.rs`
to consume them. cobrust-jit becomes a thin wrapper; cobrust-codegen
remains the single source of truth for lowering semantics.

The extraction is **scope-narrowed to the wave-1 surface
cobrust-jit actually exercises today**. AOT-specific features that
cobrust-jit doesn't need (runtime helpers, extern symbol
declaration, drop schedules, dict/list intrinsics, str literal
emission, projections) stay in `CraneliftCtx::define_body`.

## 2. Scope

### 2.1 Extraction surface (new pub in `cobrust-codegen`)

A new module `crates/cobrust-codegen/src/lowering.rs`:

```rust
pub mod lowering {
    use cobrust_mir::{Body, Constant, BinOp, UnOp, Operand, Place, Rvalue,
                      Statement, StatementKind, Terminator, LocalId, BlockId};
    use cranelift_codegen::ir::{self, AbiParam, InstBuilder, Signature,
                                UserFuncName};
    use cranelift_codegen::isa::CallConv;
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
    use std::collections::HashMap;

    use crate::error::CodegenError;

    /// Wave-1 lowering surface — module-agnostic.
    pub fn lower_constant(
        builder: &mut FunctionBuilder<'_>,
        c: &Constant,
        block_id: BlockId,
    ) -> Result<ir::Value, CodegenError>;

    pub fn lower_place(
        builder: &mut FunctionBuilder<'_>,
        var_map: &HashMap<LocalId, Variable>,
        place: &Place,
        block_id: BlockId,
    ) -> Result<ir::Value, CodegenError>;

    pub fn lower_operand(
        builder: &mut FunctionBuilder<'_>,
        var_map: &HashMap<LocalId, Variable>,
        op: &Operand,
        block_id: BlockId,
    ) -> Result<ir::Value, CodegenError>;

    pub fn lower_rvalue_wave1(
        builder: &mut FunctionBuilder<'_>,
        var_map: &HashMap<LocalId, Variable>,
        rvalue: &Rvalue,
        block_id: BlockId,
    ) -> Result<ir::Value, CodegenError>;

    pub fn lower_terminator_wave1(
        builder: &mut FunctionBuilder<'_>,
        var_map: &HashMap<LocalId, Variable>,
        block_map: &HashMap<BlockId, ir::Block>,
        term: &Terminator,
        block_id: BlockId,
        return_local: LocalId,
    ) -> Result<(), CodegenError>;

    pub fn lower_body_wave1(
        body: &Body,
        call_conv: CallConv,
    ) -> Result<ir::Function, CodegenError>;

    pub fn body_signature_wave1(
        body: &Body,
        call_conv: CallConv,
    ) -> Result<Signature, CodegenError>;
}
```

The `_wave1` suffix is load-bearing: it documents the narrow MIR
surface (Int / BinOp Add+Sub+Mul / UnOp Neg+Plus / Place::local /
Return+Goto+Unreachable). The unsuffixed `lower_constant` / `lower_place` /
`lower_operand` cover wave-1's primitives — these compose with
the `_wave1` higher-level helpers but do not themselves carry
the wave-1 limit (a future wave-2 extension can extend
`lower_constant` to cover Str / Float / FnRef as cobrust-codegen
internal AOT path already does).

### 2.2 Consumer change (cobrust-jit)

`crates/cobrust-jit/Cargo.toml`: add `cobrust-codegen` as a
workspace dependency.

`crates/cobrust-jit/src/lower.rs` collapses from 430 LOC to ~50
LOC by re-exporting / wrapping cobrust-codegen's pub fns. The
JIT-specific concern (host-ISA construction with `is_pic=false`,
JitError taxonomy) stays in `cobrust-jit::engine` / `error`.

Error-type bridge: cobrust-codegen returns `CodegenError`; cobrust-jit
needs `JitError`. A `From<CodegenError> for JitError` impl
(narrow: Wave1Lowering variant carries the inner message) lands
with the refactor.

### 2.3 AOT-side consumption

cobrust-codegen's existing `CraneliftCtx::define_body` does NOT
delegate to the new wave-1 helpers (yet). The stateful AOT path
has its own statement/rvalue/terminator dispatchers that need to
handle the full wave (runtime helpers, FnRef calls, projections,
str data symbols, drop schedules). Delegating just the wave-1
arms would create a mixed-dispatch table and add a branch on
every statement — not worth the duplication win at wave-1 scope.

The shared substrate exists primarily to **prevent JIT drift**.
A future ADR-0058e or 0056d may unify the AOT path through the
same substrate by widening the wave-1 helpers to a generic
trait-dispatched lowering — that's an explicit non-goal of this
ADR.

## 3. Non-goals

- **NO LLVM-side refactor.** Phase K wave-1 (ADR-0058a) closed at
  `e25f768`; this ADR explicitly avoids touching
  `crates/cobrust-codegen/src/llvm_backend.rs`.
- **NO new MIR features.** Refactor only; no semantic change.
- **NO AOT-side behavioral change.** `CraneliftCtx::define_body`
  is unchanged. cobrust-codegen's 355-test corpus must pass
  bit-identically.
- **NO `CodegenMode { Aot, Jit }` enum.** ADR-0056a §13 already
  deferred this; ADR-0058d does not revive it. JIT/AOT choice
  remains at the crate boundary.
- **NO unification of AOT define_body through wave-1 helpers**
  (per §2.3 above). That is a separate ADR.

## 4. Acceptance gates

| Gate | Pre-state | Post-state |
|---|---|---|
| cobrust-codegen tests | 355 PASS | 355 PASS (zero change) |
| cobrust-jit tests | 12 PASS (1 unit + 11 integration) | 12 PASS |
| cobrust-codegen LOC | 2825 (cranelift_backend.rs) | 2825 ± small (new lowering.rs ~250 LOC added) |
| cobrust-jit/lower.rs LOC | 430 | ≤280 (≥150 LOC shrink) |
| POSTFLIGHT | n/a | clean (`/tmp/cobrust-*` rm'd) |
| `cargo test -p cobrust-codegen -p cobrust-jit` on DG | n/a | TEST_EXIT=0 |
| `cargo check -p cobrust-codegen` Mac | n/a | clean |
| `cargo check -p cobrust-jit` Mac | n/a | clean |

## 5. Risk register

1. **API stability of the now-pub lowering fns.** The substrate is
   consumed internally by cobrust-jit + future Phase K LLVM-side
   convergence. We declare the wave-1 surface as **stable-for-
   wave-1**: any change requires a sub-ADR. Adding helpers (e.g.
   `lower_constant_str`) is non-breaking; removing or changing
   the wave-1 signature is breaking. Documented in
   `docs/agent/modules/codegen.md`.

2. **Error type bridge cost.** `CodegenError` and `JitError` have
   different variant taxonomies. The `From` impl narrows
   CodegenError variants encountered during wave-1 lowering to a
   single `JitError::Wave1Lowering(String)` — preserves the
   diagnostic message but loses the structured variant. Acceptable
   for wave-1 (REPL caller doesn't pattern-match on wave-1 lowering
   variants).

3. **Test coverage drift after extraction.** Existing
   cobrust-codegen tests exercise the lowering inside
   `CraneliftCtx::define_body`. The extracted wave-1 helpers
   need their own direct unit tests. Phase 3 dispatch adds two
   unit tests in `crates/cobrust-codegen/src/lowering.rs`:
   round-trip `1 + 2` through `lower_body_wave1` and reject
   `Constant::Str(_)` via `lower_constant` (which returns
   `CodegenError::Unsupported("wave1: Constant::Str")`).
   Total +2 tests.

## 6. Consequences

### 6.1 Positive

- Drift surface closed: cobrust-jit wave-1 lowering now sources
  from the same `cobrust-codegen` module its AOT sibling does.
- Future Phase K LLVM convergence (a hypothetical 0058e) gains a
  pre-existing wave-1 module-generic surface to widen.
- cobrust-jit/lower.rs simplifies dramatically (~430 → ~150 LOC),
  reducing the JIT-side surface to maintain.

### 6.2 Negative

- New cross-crate dependency: cobrust-jit now depends on
  cobrust-codegen (previously only mirrored constants). Compile-time
  graph deepens slightly; no runtime cost.
- `cobrust-codegen::lowering` becomes a stable public API surface
  with all the maintenance ceremony that implies.

### 6.3 Neutral

- Test count delta +2 (new cobrust-codegen wave-1 unit tests);
  existing 12 + 355 unchanged.
- No public-API break for either crate: cobrust-jit's public
  surface (`JitEngine` / `JitHandle` / `JitError`) is preserved;
  cobrust-codegen's existing pub surface gains a new module
  but no removals.

## 7. Dispatch readiness

P9 single-track refactor; 5 phases (ADR author + extraction +
refactor + DG verify + docs + ratify), ~3-5h wall.

— P9 Tech Lead, 2026-05-19
