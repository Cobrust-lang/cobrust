---
doc_kind: module
module_id: mod:codegen
crate: cobrust-codegen
last_verified_commit: 62ef6bd
dependencies: [mod:mir]
---

# Module: codegen

## Purpose

Lower MIR to native code via LLVM and/or Cranelift.

## Status

M0 — empty stub. First delivery at M3+.

## Backend choice

Pending ADR (TBD). Likely outcome: both backends behind a feature flag.

| Backend | Pros | Cons |
|---|---|---|
| LLVM | Mature, best codegen quality, broad target support | Slow build times, large binary footprint, C++ dependency |
| Cranelift | Fast compile, pure Rust, good for `--debug` and JIT | Less mature optimization |

Anticipated default: Cranelift for `cargo build`, LLVM for
`cargo build --release`. Final decision in ADR before M3.

## Public surface (target)

```rust
pub fn emit(module: &mir::Module, target: TargetSpec) -> Result<Artifact, CodegenError>;

pub struct TargetSpec { /* triple, opt level, backend selection */ }
pub enum Artifact { Object(...), Executable(...), DynamicLibrary(...) }
```

## Done means (M3+)

TBD; co-specified with `mod:mir`.

## Cross-references

- `mod:mir` — input.
- Future ADR — backend selection.
