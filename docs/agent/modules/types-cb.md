---
doc_kind: module
module_id: mod:types-cb
crate: cobrust-types-cb
last_verified_commit: a489016
dependencies: [mod:types, mod:hir, adr:0055, adr:0055a, adr:0055b, adr:0055c, adr:0055d, adr:0055e]
---

# Module: types-cb

## Purpose

Cobrust-cb arena-form **mirror** of `cobrust-types`. The Rust impl at
`crates/cobrust-types/` stays canonical (ADR-0055 §3.1); this crate is
a proof artifact + training-data corpus verifying that the static type
system is expressible in cb without recursive `enum` (Phase 7.5 deferred
per ADR-0055 §3.2).

## Status

- **Phase H Wave-2 — delivered** (ADR-0055a + ADR-0055b).
- **Wave-3 (ADR-0055c + ADR-0055d) — `infer.cb` + `check.cb`** ship as
  READ-ONLY `.cb` proof artifacts; cb-side parity contract is satisfied
  by Wave-2 arena-walking surfaces per the ADR-0055c Reality note.
- Diff-empty against canonical Rust impl on M2 well-typed + ill-typed
  corpus (37/37 Wave-3 + 41/41 Wave-2 — DG-verified).

## Arena-form design (ADR-0055a §3)

- `TyId` / `FnTyId` / `RecordId` = `i64` arena handles (Cobrust ints are
  single-width per ADR-0006 §"Numeric").
- `TyArena` = dense-pack `Vec<TyEntry>` + sibling arenas (`FnTyArena`,
  `RecordArena`) bundled at the API boundary; locked TEST contract per
  ADR-0055a §3 parallel-arenas paragraph.
- `TyEntry` = the Rust `Ty` enum mirrored 1:1 with recursive children
  replaced by `i64` arena handles instead of `Box<Ty>` / `Vec<Ty>`.

## Public surface

```rust
pub type TyId = i64;
pub type FnTyId = i64;
pub type RecordId = i64;

pub enum TyEntry { /* mirrors cobrust_types::Ty under arena-form */ }
pub struct FnTyEntry { /* ... */ }
pub struct RecordEntry { /* ... */ }

pub struct TyArena { /* dense-pack arena */ }
pub struct FnTyArena { /* ... */ }
pub struct RecordArena { /* ... */ }

pub fn ty_cb_arena_from_rust(rust: &Ty) -> (TyId, TyArena);
pub fn record_from_pairs(arena: &mut TyArena, pairs: Vec<(String, TyId)>) -> RecordId;
pub fn fn_ty_arity(arena: &TyArena, fn_id: FnTyId) -> i64;
pub fn is_mutable_container(arena: &TyArena, id: TyId) -> bool;
pub fn is_hashable(arena: &TyArena, id: TyId) -> bool;
pub fn display_ty(/* ... */) -> String;
pub fn clone_into_arena(src_arena: &TyArena, src_id: TyId, dst_arena: &mut TyArena) -> TyId;
pub fn subst_var(arena: &mut TyArena, src_id: TyId, var_id: TyId, replacement_id: TyId) -> TyId;
pub fn free_vars(arena: &TyArena, id: TyId) -> Vec<TyId>;
pub fn canonicalize_arena_root(/* ... */) -> CanonicalKey;

pub mod error_cb;
pub use error_cb::TypeErrorCb as TypeError;
```

## F34 symbol anchors

- `TyEntry::Tuple` — list-of-TyId payload per ADR-0055a §3 table row 1.
- `TyEntry::Ref` — single TyId payload per ADR-0055a §3 table row 9.
- `TyArena::insert` — dense-pack push returning the new `TyId` handle.
- `ty_cb_arena_from_rust` — conversion bridge: post-order Rust→cb walk.
- `display_ty` — byte-equal to Rust `impl Display for Ty`.
- `error_cb::type_error_cb_from_rust` — `TypeError` (Rust) → cb arena form.

## Parity verification

The ADR-0055e harness (crate `cobrust-types-parity`) consumes
`Canonicalize` impls from this crate + canonical Rust impl and asserts
diff-empty under the 5-namespace dense-pack contract (§3). Failures
raise structured `ParityError` with the BLOCK rule cited.

## Re-export surface (mirrors `cobrust-types::lib.rs`)

Every `pub use` in canonical Rust `lib.rs` is reproduced here per
ADR-0055b §4 risk-3 mitigation so Tier-2 ports (`0055c` `infer.rs`,
`0055d` `check.rs`) import from this crate with identical name shapes.

## Done means

- [x] `ty_cb_arena_from_rust` + arena round-trip diff-empty on M2 corpus.
- [x] `display_ty` byte-equal to Rust `impl Display for Ty`.
- [x] `TypeErrorCb` arena form + `Canonicalize for TypeErrorCb` (ADR-0055b).
- [x] Wave-3 `.cb` proof artifacts (`infer.cb`, `check.cb`) READ-ONLY.
- [x] Diff-empty on Wave-3 corpus (37/37) + Wave-2 corpus (41/41) DG-verified.
