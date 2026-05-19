---
doc_kind: module
module_id: mod:jit
crate: cobrust-jit
last_verified_commit: a489016
dependencies: [mod:mir, mod:codegen, adr:0029, adr:0056a, adr:0056b]
---

# Module: jit

## Purpose

Cranelift-backed JIT engine for incremental REPL evaluation. JIT-mode
sibling of `cobrust-codegen`'s AOT object-file backend, sharing the
underlying Cranelift IR + ISA but emitting native code into the
process's address space via `cranelift-jit`'s `JITModule`.

## Status

- **Phase I Wave-1 — delivered** (ADR-0056a). Minimal arithmetic
  round-trip: `i64`-typed entry points, `BinOp::{Add, Sub, Mul}`,
  `Constant::Int`, `Operand::{Copy, Constant}`, function params.
- **Phase K Strand #4 — refactor** (`collapse lower.rs to thin wrapper
  over cobrust-codegen pub fns`): `lower.rs` is now a thin re-export
  so `cobrust-codegen` MIR→Cranelift lowering is the single source of
  truth for both AOT and JIT.

## Cold-start budget (ADR-0029)

`<50ms` per Session — verified at impl time on <self-hosted-runner>.

## Public surface

```rust
pub mod engine;
pub mod error;
pub mod handle;
pub mod lower;  // thin wrapper over cobrust-codegen (Phase K Strand #4)

pub use engine::JitEngine;
pub use error::JitError;
pub use handle::{ArgsList, JitHandle};
```

### `JitEngine`

- `JitEngine::new()` — lazy-init constructor wrapping `JITBuilder` /
  `JITModule`; owns the per-Session JIT page allocator.
- `JitEngine::compile_mir(&mut self, mir: &MirModule) -> Result<JitHandle, JitError>` —
  MIR module → native fn pointers, returning a `JitHandle` keyed by the
  body name.

### `JitHandle`

- `JitHandle::call::<Args, R>(&self, args: Args) -> Result<R, JitError>` —
  invoke the JIT-compiled function with a primitive-typed argument
  tuple (`ArgsList`) and primitive return type `R`.

### `JitError`

Structured error taxonomy covering compile-time + runtime JIT failures.

## Scope progression

- **Wave-1 (ADR-0056a)** — `i64` arithmetic round-trip locked.
- **Wave-2 (ADR-0056b)** — control-flow + stdlib intrinsics +
  dict/list. Architecture set up so 0056b grafts the richer surface
  without a public-API break.

## F34 symbol anchors

- `JitEngine::new` — lazy constructor; cold-start cost measured.
- `JitEngine::compile_mir` — primary MIR→native entrypoint.
- `JitHandle::call` — invocation surface.
- `lower::*` — re-exports from `cobrust-codegen` (Phase K Strand #4).

## Done means

- [x] Cold-start `<50ms` on <self-hosted-runner>.
- [x] `i64` arithmetic round-trip (BinOp Add/Sub/Mul) PASS on DG.
- [x] `lower.rs` collapsed to thin wrapper over `cobrust-codegen` so
      AOT and JIT share a single MIR→Cranelift lowering source of truth.
- [ ] Wave-2 control-flow + intrinsics surface (ADR-0056b, future).
