---
doc_kind: module
module_id: mod:mir
crate: cobrust-mir
last_verified_commit: 62ef6bd
dependencies: [mod:types]
---

# Module: mir

## Purpose

Mid-level IR: control-flow-explicit form fed to `mod:codegen`. Locals,
basic blocks, terminators.

## Status

M0 — empty stub. First delivery at M3+.

## Public surface (target)

```rust
pub fn lower(typed: &types::TypedModule, sess: &mut Session) -> Result<Module, MirError>;

pub struct Module { /* fns: Vec<Body> */ }
pub struct Body { /* locals, basic_blocks, terminators */ }
```

## Shape

- SSA-like, but with explicit basic blocks (closer to `rustc` MIR than
  classic SSA).
- Borrow / move semantics are visible at MIR level — drop schedule is
  computed here.

## Invariants (target)

- Every basic block ends with a terminator.
- Every local has a declared type.
- Borrow checker proof obligations are discharged before lowering to
  codegen.

## Done means (M3+)

TBD; specified together with `mod:codegen` ADR.

## Cross-references

- `mod:types` — input.
- `mod:codegen` — output consumer.
