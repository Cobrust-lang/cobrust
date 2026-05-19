---
doc_kind: module
module_id: phase-m-language-surface-closure
last_verified_commit: 1ff7921
dependencies: [adr:0058a, adr:0060, adr:0060a, adr:0060b, adr:0060c, adr:0052a, adr:0006]
---

# Phase M — language-surface gap closure

## Scope

Closes ADR-0058a §15 6-gap queue. 5 additive sub-sprints + 1 OOS memo.

| Gap | ADR | Status |
|---|---|---|
| i32 narrow-int | 0060a | landed |
| i8 narrow-int | 0060a | landed |
| -> None return | 0060b | landed |
| `&T` in annotation | 0060b | landed |
| `[T; N]` array type | 0060b | landed (type-emission; indexing deferred) |
| struct{T,U} | 0060c | OUT-OF-SCOPE |

## Public surface delta

### `cobrust_types::Ty`

Added two variants:

```rust
pub enum Ty {
    // ... existing variants ...
    /// ADR-0060a — narrow signed integer (width in bits, {8, 16, 32}).
    IntN(u8),
    /// ADR-0060b — fixed-size homogeneous array `[T; N]`.
    Array(Box<Ty>, usize),
}
```

Unification:

- `IntN(a) ⇔ IntN(b)` iff `a == b` (no cross-width).
- `Array(t1, n1) ⇔ Array(t2, n2)` iff `n1 == n2 ∧ t1 ⇔ t2`.

Display:

- `IntN(8)` → `"i8"`, `IntN(16)` → `"i16"`, `IntN(32)` → `"i32"`.
- `Array(Box::new(Int), 4)` → `"[i64; 4]"`.

Predicates:

- `IntN(_).is_hashable() == true`
- `Array(_, _).is_hashable() == false`
- `IntN(_)` is Copy (`drop.rs::is_copy` arm).

### `cobrust_frontend::ast::TypeKind`

Added two variants:

```rust
pub enum TypeKind {
    // ... existing variants ...
    Ref(Box<Type>),
    Array { elem: Box<Type>, len: usize },
}
```

Parser entry (`parse_type_atom`):

1. `KwNone` → `TypeKind::Name(vec!["None"])` (ADR-0060b §3.1).
2. `Amp` prefix → `TypeKind::Ref(inner)` (ADR-0060b §3.2).
3. `LBracket` prefix → `TypeKind::Array { elem, len }` after parsing
   inner type + `;` + integer literal + `]` (ADR-0060b §3.3).

### `cobrust_hir::TypeKind`

Mirrors AST 1:1. `lower::lower_type` passes new variants through
structurally.

### `cobrust_types::check`

- `lower_named_type`: adds `"i8"` / `"i16"` / `"i32"` arms returning
  `Ty::IntN(width)`.
- `lower_type`: adds `Ref` / `Array` arms.
- `validate_hashable_dict`: recurses into Ref + Array inner.

### `cobrust_codegen`

- `abi.rs::cranelift_scalar_ty`: maps `IntN(8|16|32)` → `types::I8|I16|I32`.
- `abi.rs::is_copy_ty`: includes `IntN(_)`.
- `lowering.rs::lower_ty_wave1`: maps `IntN(8|16|32)` → `I8|I16|I32`.
- `llvm_backend.rs::lower_ty`: adds `IntN` (i8/i16/i32 type) and
  `Array(elem, n)` (`elem_ty.array_type(n as u32)`) arms.
- `llvm_backend.rs::di_type_for`: `IntN` collapses to "Int" DI;
  `Array` collapses to "Ptr" DI.

## Wave-2 deferrals (open findings)

Three F37 findings document operational-flow follow-ups:

1. `finding:adr0060a-binop-on-intn-narrow-int-debt`
   - covers `pm_a03_i8_add_well_typed` + `pm_a04_intn_is_copy`
   - resolution: extend `synth_binop` with `(IntN(w), IntN(w))` arm
     + add `CastKind::IntNarrow(u8)` to MIR + extend `lower_cast`
     token map for `"i32"` / `"i8"` / `"i16"` + extend codegen cast
     arms (`build_int_truncate` / `build_int_s_extend` /
     `ireduce` / `sextend`) + `synth_lit_int` literal-fit guard
     with `TypeError::NarrowIntOverflow`.

2. `finding:adr0060b-array-indexing-mir-projection-debt`
   - covers `llvm_type_08_array_i64` body narrowing
   - resolution: extend `synth_expr` IndexAccess with
     `(Ty::Array(elem, n), IndexKind::Expr(e))` arm + literal-OOB
     compile-time-catch (`MirError::ArrayIndexOob`) + route
     `Place::index` through array layout + LLVM GEP emit
     (`build_in_bounds_gep`).

3. `finding:adr0060b-empty-dict-annotation-k-flow-debt`
   - covers `pm_b06_array_not_hashable` (severity P3)
   - resolution: wire `synth_let` to validate the annotation Ty's
     K-position before binding.

## Verification (DG 1ff7921)

```
codegen_diff_corpus:   52 passed, 0 failed, 6 ignored
phase_m_syntax_corpus: 17 passed, 0 failed, 0 ignored
phase_m_type_corpus:   11 passed, 0 failed, 3 ignored (F37)
```

Zero regression on Phase H/I/J/K/L baselines.

## F34 anchors

- `phase-m-language-surface-closure::scope` — module entry
- `phase_m_syntax_corpus::pm_b01_none_return_type` — parser corpus head
- `phase_m_type_corpus::pm_a01_i32_resolves_to_intn32` — typeck corpus head
- `codegen_diff_corpus::llvm_type_02_i32` — codegen corpus head
- `codegen_diff_corpus::llvm_type_08_array_i64` — array type-emission anchor

## Cross-references

- ADR-0058a §15 — gap queue authority
- ADR-0060 — Phase M frame
- ADR-0060a — narrow-int types
- ADR-0060b — syntax trio
- ADR-0060c — anonymous struct OUT-OF-SCOPE
- ADR-0052a Wave-1 — companion expression-position `&`
- ADR-0006 §"Type universe" — `Ty::Int` baseline
