---
doc_kind: module
module_id: mod:hir
crate: cobrust-hir
last_verified_commit: 62ef6bd
dependencies: [mod:frontend]
---

# Module: hir

## Purpose

High-level intermediate representation: AST after desugaring + name
resolution. The form the type checker (`mod:types`) consumes.

## Status

M0 — empty stub. First delivery at M2.

## Public surface (target — M2)

TBD. Indicative outline:

```rust
pub fn lower(module: &ast::Module, sess: &mut Session) -> Result<Module, LoweringError>;

pub struct Module { /* HIR nodes with resolved names */ }
pub struct Body { /* function/method body */ }
```

## Desugaring scope (target)

- Comprehensions → explicit loops with collector
- `with` → `try`/`finally`-style scope guards
- f-strings → `format` macro calls
- Decorators → explicit calls
- Walrus `:=` → let-binding + use

## Invariants (target — M2)

- Every name binding has a unique `DefId`.
- Every name use has a resolved `DefId` (or a hard error).
- HIR is hygienic — no shadowing ambiguity left over from source.
- Lowering is total: any well-formed AST yields a well-formed HIR.

## Done means (M2)

- [ ] Lowering for every form in the "core 30 forms" suite.
- [ ] Name resolution covers module / function / class / comprehension
      scopes.
- [ ] No panics on any AST produced by `mod:frontend`.

## Non-goals

- No type information. That's `mod:types`.
- No optimization. That's `mod:mir`.

## Cross-references

- `mod:frontend` — input.
- `mod:types` — output consumer.
