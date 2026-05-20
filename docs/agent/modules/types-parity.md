---
doc_kind: module
module_id: mod:types-parity
crate: cobrust-types-parity
last_verified_commit: a489016
dependencies: [mod:types, mod:types-cb, adr:0055, adr:0055e]
---

# Module: types-parity

## Purpose

Diff harness contract proving the canonical Rust `cobrust-types` impl
and the arena-form `cobrust-types-cb` mirror produce **identical**
canonical output on the M2 well-typed + ill-typed corpus. Phase H
Wave-1 deliverable per ADR-0055e.

Phase H §1.1 + §8.1 framing: cb mirror is a **proof artifact +
training-data corpus**, not a parallel impl. This harness is the
proof obligation.

## Status

- **Wave-1 — delivered** (ADR-0055e §3 5-namespace dense-pack contract
  + §6 5 BLOCK-rule failure kinds + property-test corpus 25 cases).
- **Wave-2 + Wave-3 — corpus un-ignored** as the cb mirror surface
  lands (`Canonicalize for TyEntry`, `Canonicalize for TypeErrorCb`).

## Scope (F28 strict-separation)

This crate is **TEST scope only**. All `canonicalize` /
`parity_check` *implementations* are DEV scope and live in the
canonical crate (`cobrust-types`) and the mirror (`cobrust-types-cb`).
The trait + corpus + harness types ship here so TEST + DEV are
strictly separated.

## Public surface

```rust
pub struct CanonicalKey {
    /* dense-pack canonical representation of a Ty tree */
}

pub struct TyArena {
    /* stub arena handle passed to Canonicalize implementors */
}

pub trait Canonicalize {
    fn canonicalize(&self, arena: &TyArena, key: &mut CanonicalKey);
}

pub enum ParityError {
    /* 5 BLOCK-rule failure kinds per ADR-0055e §6 */
}

pub fn parity_check<R: Canonicalize, C: Canonicalize>(
    rust: &R, cb: &C,
) -> Result<(), ParityError>;

pub fn type_error_variant_name(err: &TypeError) -> &'static str;
pub fn manual_canonical_key(ty: &Ty) -> CanonicalKey;
```

## Canonicalization namespaces (§3 amendment 2026-05-18)

5 primary arenas, each with an independent dense-pack counter:

- `TyId`
- `AdtId`
- `AliasId`
- `FnTyId`
- `RecordId`

Auxiliary namespaces with the same dense-pack rule:

- `VarId`
- `GenericVar`

## F34 symbol anchors

- `CanonicalKey::push_ty` — append a normalized child handle.
- `parity_check` — diff entrypoint; `Ok(())` iff canonical outputs match.
- `ParityError::TypeErrorVariantMismatch` — BLOCK rule 1.
- `ParityError::CanonicalKeyDiff` — BLOCK rule 5 (catch-all dense diff).

## Done means

- [x] `CanonicalKey` dense-pack contract locked.
- [x] `Canonicalize` impls in canonical + cb crates produce diff-empty
      output on M2 corpus.
- [x] Property-test corpus (25 cases) un-ignored and PASS on DG.
- [x] Wave-2 corpus (38 tests, DG) un-ignored.
- [x] Wave-3 corpus un-ignored as Tier-2 ports land.
