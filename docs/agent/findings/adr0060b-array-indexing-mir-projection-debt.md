---
doc_kind: finding
finding_id: adr0060b-array-indexing-mir-projection-debt
last_verified_commit: TBD
dependencies: [adr:0060b, adr:0058a]
discovered_by: P9 Phase M sprint 2026-05-19 — llvm_type_08_array_i64 DG verify
severity: P2 (language-surface follow-up; type-emission gap closed, indexing deferred)
status: open (deferred to ADR-0060b array-indexing sub-sprint)
related: [adr:0060b §3.3, adr:0058a §4.1]
---

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
