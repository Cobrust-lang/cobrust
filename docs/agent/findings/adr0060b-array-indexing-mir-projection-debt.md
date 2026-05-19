---
doc_kind: finding
finding_id: adr0060b-array-indexing-mir-projection-debt
last_verified_commit: 9adf6e6
dependencies: [adr:0060b, adr:0058a]
discovered_by: P9 Phase M sprint 2026-05-19 — llvm_type_08_array_i64 DG verify
severity: P2 (language-surface follow-up; type-emission gap closed, indexing deferred)
status: FULLY RESOLVED (compile-time-const + dynamic-index) at 9adf6e6 (2026-05-19 Phase M follow-up sprint, wave-3 dynamic-index sprint)
related: [adr:0060b §3.3, adr:0058a §4.1]
---

## §0b. Dynamic-index Resolution (2026-05-19, commits 43bb500…9adf6e6)

Wave-3 follow-up closes the dynamic-index deferred scope from §0:

1. **`crates/cobrust-stdlib/src/array.rs`** (new) — four C-ABI runtime helpers:
   `__cobrust_array_get_i64`, `__cobrust_array_get_i32`, `__cobrust_array_get_i8`,
   `__cobrust_array_get_bool`. Each uses `slice::get` (safe Rust) and panics via
   `__cobrust_panic` on OOB. No `unsafe_code` relaxation needed.

2. **`crates/cobrust-codegen/src/llvm_backend.rs`** — `declare_runtime_helpers`
   now declares all four array-get symbols. `lower_place_load` Projection::Index path
   routes non-const indices through the appropriate `__cobrust_array_get_<T>` call;
   const-index path (`build_extract_value`) unchanged.

3. **`crates/cobrust-codegen/src/cranelift_backend.rs`** — `runtime_helper_sigs`
   declares the four symbols for Cranelift parity; Index arm comment updated.

4. **`crates/cobrust-codegen/tests/codegen_diff_corpus.rs`** — three new F34
   fixtures: `llvm_array_dyn_index_i64`, `llvm_array_dyn_index_i32`,
   `llvm_array_dyn_index_oob_panic`. All PASS on DG verify (DG 2026-05-19,
   POSTFLIGHT clean POST=0, zero regression).

## §0. Resolution (2026-05-19, commit 981b577)

Three additive edits closed this finding:

1. **`cobrust-types/src/check.rs::synth_expr` IndexAccess arm** — added
   the `(Ty::Array(elem, n), IndexKind::Expr(e))` match arm. Index
   must unify with `Ty::Int`. ADR-0060b §3.4 literal-OOB check fires
   when the index is a constant integer literal whose value falls
   outside `[0, n-1]`; surfaces as `TypeError::NotIndexable` with the
   OOB suggestion.

2. **`cobrust-codegen/src/llvm_backend.rs::lower_place_load`** — added
   the `[Projection::Index(idx_op)]` Array path. When the base
   LocalDecl's type is `Ty::Array(elem, _)` AND the index Operand is
   a compile-time `Constant::Int(k)` with `k >= 0`, the lowering
   emits a safe aggregate-extract:
   - `build_load(array_ty, alloca)` -> ArrayValue
   - `build_extract_value(arr, k as u32)` -> element.

   Why aggregate-extract not GEP: `cobrust-codegen/src/lib.rs:32`
   declares `#![forbid(unsafe_code)]` which blocks inkwell's unsafe
   `build_in_bounds_gep`. The safe `build_extract_value` requires a
   compile-time `u32` index, which exactly matches the ADR-0060b §3.4
   compile-time-catch surface (literal-OOB detection). Dynamic-index
   Array reads originally fell through to the wave-1 stub-load surface.
   **RESOLVED in wave-3 sprint 2026-05-19 (§0b)**: non-const indices now
   route through `__cobrust_array_get_<T>` runtime helpers declared in
   `cobrust-stdlib/src/array.rs`. The `#![forbid(unsafe_code)]` constraint
   is satisfied — no GEP needed, no relaxation of the unsafe policy.

3. **`cobrust-codegen/tests/codegen_diff_corpus.rs`** — F36 rename
   `llvm_type_08_array_i64` -> `llvm_type_08_array_i64_index` with
   the body changed from `return 0` (passthrough) to `return a[0]`
   (real element extract). Added `llvm_type_08b_array_index_literal_oob`
   as a typeck-rejection fixture for the §3.4 compile-time-catch.

Cranelift backend keeps the opaque-pointer wave-1 surface per
ADR-0060b §3.3.

# Finding: `[T; N]` array indexing at source level not yet wired

## §1. Empirical anchor

Phase M codegen corpus fixture:

- `crates/cobrust-codegen/tests/codegen_diff_corpus.rs::llvm_type_08_array_i64`

DG verify at HEAD `27d8416` exposed:

```
type check: NotIndexable {
  actual: Array(Int, 4),
  span: Span { ..., start: 41, end: 45 },
  suggestion: Some("use a list / dict / tuple / str — primitive types cannot be indexed")
}
```

The fixture was authored expecting `return a[0]` to lower to a MIR
`Place::index` projection + LLVM GEP. Today the typeck `NotIndexable`
predicate (the IndexKind synthesis at `check.rs` ~line 1184) rejects
`Ty::Array` from its index-base allow-list.

## §2. Precise root cause

`synth_expr` at the `IndexAccess` arm enumerates indexable bases:

```rust
match (base_ty, idx_kind) {
    (Ty::List(elem), IndexKind::Expr(e)) => {
        unify(&Ty::Int, &it, &mut self.subst, e.span)?;
        Ok(*elem)
    }
    (Ty::Tuple(items), IndexKind::Expr(e)) => { ... }
    (Ty::Dict(k, v), IndexKind::Expr(e)) => { ... }
    (Ty::Str, IndexKind::Expr(e)) => { ... }
    // Ty::Array(elem, n) intentionally NOT yet here
    _ => Err(TypeError::NotIndexable { ... }),
}
```

ADR-0060b §3.3 specifies the value-flow rule (literal-bounds-check at
HIR→MIR + GEP at codegen) but the **typeck dispatch arm** is the
prerequisite. Phase M wave-2 ships the type identity + LLVM type
emission; indexing follows.

## §3. Classification

Additive language-surface debt — fits in the same "value-flow follow-up"
bucket as `finding:adr0060a-binop-on-intn-narrow-int-debt`. The
**type-identity** path Phase M ships is correct (`[i64; 4]` parses,
type-checks, and lowers to `[4 x i64]` at LLVM). The **operational**
path (indexing, mutation, slicing) is the wave-2 follow-up.

## §4. Resolution plan

Three additive code edits:

1. `cobrust-types/src/check.rs::synth_expr` IndexAccess arm: add
   ```rust
   (Ty::Array(elem, n), IndexKind::Expr(e)) => {
       unify(&Ty::Int, &it, &mut self.subst, e.span)?;
       // Per ADR-0060b §3.4 literal-OOB compile-time-catch:
       if let ast::ExprKind::Literal(Lit::Int(s)) = &e.kind {
           if let Ok(k) = s.parse::<i64>() {
               if k < 0 || (k as usize) >= *n {
                   return Err(MirError::ArrayIndexOob { ... });
               }
           }
       }
       Ok(*elem.clone())
   }
   ```

2. `cobrust-mir/src/lower.rs::lower_expr` IndexAccess arm: route
   `Ty::Array` through the same `Place::index` projection as
   `Ty::List`. (The MIR side likely needs no change — the projection
   is type-generic.)

3. `cobrust-codegen/src/{cranelift_backend,llvm_backend}.rs`
   Place::index lowering: when the base local's type is `Ty::Array`,
   emit a GEP `[N x T]*, i64 0, i64 <idx>` instead of the
   list-element runtime helper call. LLVM:
   `builder.build_in_bounds_gep(arr_ty, alloca, &[zero, idx], "elem")`.

## §5. F36 + F37 compliance

- **F36**: `llvm_type_08_array_i64` fixture name accurately describes
  the eventual full-coverage behavior. Wave-2 narrows the body to a
  passthrough that exercises only the type-emission path; the body
  rewrite is captured here.
- **F37**: this finding is the ignore-debt cross-reference for the
  narrowed wave-2 body.

## §6. Cross-references

- ADR-0060b §3.3 §3.4 — array type + literal-OOB rule
- ADR-0058a §4.1 — LLVM array type emission row
- `crates/cobrust-types/src/check.rs::synth_expr` IndexAccess arm
- `crates/cobrust-mir/src/lower.rs::lower_expr` IndexAccess arm
