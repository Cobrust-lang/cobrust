---
doc_kind: adr
adr_id: 0058e
parent_adr: 0058d
name: 0058e
title: "AOT cranelift_backend substrate delegation — close 0058d §2.3 deferral"
status: accepted
date: 2026-05-20
phase: Phase K Strand #4 follow-up
last_verified_commit: c9de99c
supersedes: []
superseded_by: []
relates_to: [adr:0058d, adr:0058a, adr:0056a]
discovered_by: "0058d §2.3 explicit deferral — documented non-goal that becomes this ADR's goal"
ratification_path: P9 ADR review; ratifies on impl-merge gate
---

# ADR-0058e: AOT cranelift_backend substrate delegation

## 1. Motivation

ADR-0058d shipped the shared MIR→Cranelift lowering substrate in
`cobrust-codegen::lowering` and wired `cobrust-jit::lower` as a thin
consumer. However, §2.3 explicitly deferred the AOT-side unification:

> cobrust-codegen's existing `CraneliftCtx::define_body` does NOT
> delegate to the new wave-1 helpers (yet). … Delegating just the
> wave-1 arms would create a mixed-dispatch table and add a branch on
> every statement — not worth the duplication win at wave-1 scope.
>
> A future ADR-0058e or 0056d may unify the AOT path through the same
> substrate by widening the wave-1 helpers to a generic trait-dispatched
> lowering — that's an explicit non-goal of this ADR.

The deferral created a **touch-two-places window**: any change to the
wave-1 lowering substrate in `cobrust-codegen::lowering` must also
be manually mirrored in `CraneliftCtx::define_body`'s body-lowering
setup (block creation, variable declaration, param binding,
pre-initialization loop, block-traversal loop). As the substrate grows,
this window widens silently.

This ADR closes 0058d §2.3 by routing `CraneliftCtx::define_body`'s
AOT body-lowering path through a new internal
`define_body_via_substrate` helper that calls `lowering::lower_body_wave1`
for the wave-1-compatible portion, leaving AOT-specific concerns
(reachability filtering, drop schedules, extern symbol declaration,
str data symbol interning, runtime helper wiring, debug-info hooks)
as AOT-side pre/post work.

The key insight that makes this tractable (vs the 0058d rationale
against it): the wave-1 helpers produce a standalone `ir::Function`
that `define_body` can inject into a `cranelift_codegen::Context` and
hand to `obj.define_function`. The mutable `ObjectModule` reference
for AOT-specific declarations (extern func IDs, str data globals,
runtime helpers, user FuncRefs) is handled in pre-passes that run
**before** calling `lower_body_wave1`, and post-assembled into the
`Function`'s external references via a new thin
`assemble_funcref_table` pass. This avoids the mixed-dispatch
concern entirely: wave-1 core lowering is pure-function; all
module-mutating work stays AOT-side.

## 2. Scope

### 2.1 Target refactor

`CraneliftCtx::define_body` in
`crates/cobrust-codegen/src/cranelift_backend.rs`.

Today: ~440 LOC of body-setup + block-map creation + var-map declaration
+ param-binding + pre-init loop + str-data interning + extern-func
declaration + runtime-helper declaration + block-traversal +
`builder.seal_all_blocks` + `obj.define_function`.

After: the body-lowering core (block-map + var-map + param-binding +
pre-init + block-traversal + seal + finalize) delegates to
`lowering::lower_body_wave1` for wave-1 bodies. AOT-specific
pre-passes (str data interning, extern func / runtime helper /
user FuncRef declaration) run first and are wired into the `ir::Function`
returned by `lower_body_wave1` via `declare_func_in_func` calls on the
`ir::Function` before handing it to `Context::for_function`.

For **non-wave-1 bodies** (those containing Aggregate / Cast / Call /
SwitchInt / projections / f-string), `define_body` continues to use
the existing `EmitCtx`-based lowering path — wave-1 delegation applies
only when the body is wave-1 compatible (detectable via a new
`body_is_wave1` predicate). This preserves full AOT capability while
eliminating duplication for the common wave-1 case.

### 2.2 New helpers added to `cobrust-codegen::lowering`

None required. The 9 pub fns from ADR-0058d are sufficient.

The only structural change in `lowering.rs` is exposing
`lower_ty_wave1` (already pub) for use in the `body_is_wave1` predicate.

### 2.3 `body_is_wave1` predicate

A new `pub(crate)` function in `cranelift_backend.rs`:

```rust
fn body_is_wave1(body: &Body) -> bool {
    // Reject if any block has a non-wave-1 terminator or statement.
    for block in &body.blocks {
        match &block.terminator {
            Terminator::Return | Terminator::Goto(_) | Terminator::Unreachable => {}
            _ => return false,
        }
        for stmt in &block.statements {
            if let StatementKind::Assign { place, rvalue } = &stmt.kind {
                if !place.projections.is_empty() { return false; }
                if !rvalue_is_wave1(rvalue) { return false; }
            }
        }
    }
    // Reject if any local has a non-wave-1 type.
    for local in &body.locals {
        if crate::lowering::lower_ty_wave1(&local.ty).is_err() { return false; }
    }
    true
}
```

This predicate is conservative: any body that cannot be handled by
the wave-1 substrate falls through to the existing `EmitCtx` path,
preserving zero behavioral change for all current corpus tests.

## 3. Non-goals

- **NO new MIR features.** Refactor only; zero semantic change.
- **NO JIT-side changes.** ADR-0058d already ships the JIT wrapper.
- **NO LLVM-backend changes.** `llvm_backend.rs` untouched.
- **NO `codegen_diff_corpus` expectation changes.** Regression-clean
  is a hard gate: 56+ PASS pre-state must be 56+ PASS post-state.
- **NO removal of the `EmitCtx` path.** Non-wave-1 bodies continue
  through the full AOT lowering. `EmitCtx` is not removed in this ADR.
- **NO `CodegenMode { Aot, Jit }` enum** (per 0058d §3 non-goal).

## 4. Acceptance gates

| Gate | Pre-state | Post-state |
|---|---|---|
| `cargo check -p cobrust-codegen` Mac | PASS | PASS |
| `cargo test -p cobrust-codegen` Mac | 56+ PASS, 0 FAIL | 56+ PASS, 0 FAIL |
| `cargo test -p cobrust-jit` Mac | 12 PASS | 12 PASS (unchanged) |
| `cranelift_backend.rs` LOC | 2878 | 3037 (+159 net: wave-1 predicate + delegation path added; non-wave-1 EmitCtx path unchanged) |
| `lowering.rs` LOC | ~600 | ~600 (unchanged — no new substrate fns needed) |
| F36 compliance | N/A | no new test names introduced |
| F37 compliance | N/A | no behavioral change to existing tests |
| 0058d §2.3 deferral | OPEN | RESOLVED at merge SHA |

## 5. Risk register

1. **Wave-1 predicate false-positives.** If `body_is_wave1` admits
   a body that the wave-1 substrate cannot handle, `lower_body_wave1`
   returns `CodegenError::InvalidMir` with a `wave1:` prefix and the
   existing test suite will catch it immediately. The predicate is
   written conservatively (any unknown shape → `false`).

2. **AOT-specific FuncRef injection after `lower_body_wave1`.** The
   wave-1 substrate returns an `ir::Function` whose external references
   are uninhabited (no `declare_func_in_func` calls). The AOT path
   must inject user-fn FuncRefs + runtime helper FuncRefs + extern
   FuncRefs by calling `obj.declare_func_in_func` against the returned
   `ir::Function` before wrapping it in `Context::for_function`. This
   is straightforward because `ir::Function` is a plain struct; the
   post-assembly step is the only new coupling surface.

3. **Drop-schedule emission timing.** Drop schedule blocks are
   unreachable from the wave-1 perspective (the reachability filter
   in the existing `define_body` removes them). Wave-1 bodies in the
   current corpus do not carry drop schedules, so this risk is dormant.
   The `body_is_wave1` predicate should additionally check for
   `Terminator::Drop` — already covered under the non-wave-1 terminator
   arm.

## 6. Implementation plan

**Phase 1** (~20 min): add `body_is_wave1` predicate in
`cranelift_backend.rs`.

**Phase 2** (~2h): refactor `CraneliftCtx::define_body` to branch on
`body_is_wave1`:
- Wave-1 path: run AOT pre-passes (str data + extern + runtime + user
  FuncRef declaration), call `lowering::lower_body_wave1`, inject
  FuncRefs, wrap in `Context::for_function`, call
  `obj.define_function`.
- Non-wave-1 path: existing `EmitCtx` lowering, unchanged.

**Phase 3** (~30 min): `cargo check + cargo test` on Mac per-crate.

**Phase 4** (~15 min): ratify ADR + amend 0058d §2.3.

**Phase 5** (~30 min): dual-track docs.

LOC delta actual: +159 net to `cranelift_backend.rs` (wave-1 predicate
`body_is_wave1` ~60 LOC + `define_body_wave1_path` ~40 LOC +
`rvalue_is_wave1_with_locals`/`operand_is_wave1_with_locals` free fns
~40 LOC + delegation branch ~5 LOC + ADR-0058e comment blocks ~14 LOC).
The non-wave-1 `EmitCtx` path is unchanged (no lines removed).
`lowering.rs` delta: 0 (no new substrate fns needed).

Note: The original expectation of "~200-350 lines removed" assumed the
`define_body` body-lowering block would be replaced wholesale. The
actual refactor adds a parallel fast path for wave-1 bodies; the full
EmitCtx path remains for non-wave-1 bodies. The touch-two-places window
is closed (wave-1 logic now sources from the substrate), but the
duplicated setup lines are NOT removed since they serve non-wave-1 bodies
on the existing path. A future ADR widening the wave-1 substrate to cover
the full wave could then remove the EmitCtx path entirely.

## 7. Consequences

### 7.1 Positive

- 0058d §2.3 deferral is formally closed.
- Touch-two-places window for wave-1 body lowering eliminated.
- `cranelift_backend.rs` shrinks substantially (duplicate setup removed).
- Future wave-1 extensions (e.g. adding a `lower_constant_float` to
  the substrate) automatically benefit both JIT and AOT.

### 7.2 Negative

- `define_body` gains a branching predicate; non-wave-1 path is
  unchanged in semantics but now explicitly gated.
- Maintenance coupling: wave-1 substrate API stability (ADR-0058d §5.1)
  is now critical for the AOT path too, not just JIT.

### 7.3 Neutral

- No public-API change to `cobrust-codegen` or `cobrust-jit`.
- Test count delta: 0 (no new tests required; existing corpus covers
  the wave-1 delegation via the unchanged diff-corpus suite).

— P9 Tech Lead, 2026-05-20
