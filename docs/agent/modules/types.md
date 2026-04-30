---
doc_kind: module
module_id: mod:types
crate: cobrust-types
last_verified_commit: 62ef6bd
dependencies: [mod:hir]
---

# Module: types

## Purpose

Static structural type system + type checker. M2 ships the static core
(no `dyn`); `dyn` is opt-in and added later.

## Status

M0 — empty stub. First delivery at M2.

## Public surface (target — M2)

```rust
pub fn check(module: &hir::Module, sess: &mut Session) -> Result<TypedModule, TypeError>;

pub struct TypedModule { /* HIR + per-node types */ }
pub enum Ty { /* Int, Float, Str, Tuple, Record, Fn, Generic, ... */ }
```

## Type system shape (target — M2)

- **Structural typing** by default — record types match by field
  signature, not by nominal identity.
- **Algebraic data types** with exhaustive pattern matching.
- **Generics** with explicit type parameters; row polymorphism for
  records (TBD; ADR pending).
- **No `dyn` in M2.** Trait objects arrive in M3+ behind an explicit
  opt-in keyword.
- **No subtyping at value level**; coercions are explicit functions.
- **Inference**: bidirectional (Hindley-Milner-like in the static core,
  then narrowed where annotations exist).

## Invariants (target)

- Type errors never emit a "best guess" type — either inferred or hard
  error.
- The type system is sound for the static core (proof obligation tracked
  in `find:type-soundness-proof`, TBD).
- Compile-time exhaustiveness: every `match` either covers all
  constructors or has a wildcard.

## Done means (M2)

- [ ] Curated suite: ≥ 50 well-typed programs accepted.
- [ ] Curated suite: ≥ 50 ill-typed programs rejected with the right
      error category.
- [ ] All M1 "core 30 forms" type-check with reasonable annotations.

## Non-goals

- No runtime reflection in M2.
- No effect system in M2 (deferred).

## Cross-references

- `mod:hir` — input.
- `mod:mir` — downstream consumer.
- Constitution `CLAUDE.md` §2.2 (drop `is`, drop implicit truthiness, etc.)
