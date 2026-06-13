---
finding_id: F83
title: "tuple `t[i]` SILENT-0 MISCOMPILE — `(7, \"x\")[0]` builds OK + returns 0 (§2.2; CPython 7)"
date: 2026-06-13
status: resolved
resolved_date: 2026-06-13
resolution: "ADR-0097 — a tuple now lowers to a REAL LLVM struct value (construction via build_insert_value, `t[i]` read via build_extract_value), and the MIR `ExprKind::Index` arm reads `Projection::Field(off)` for a CONSTANT index (Python-negative normalised against the static arity) as the per-position element type. check.rs REJECTS a non-literal / constant-OOB tuple index (TypeError::NotIndexable, §2.5-A compile-time-catch) instead of the head-element / Ty::Never→Int(0) silent fold. The str/bytes/list/tuple indexing arc is now COMPLETE."
severity: critical
relates_to: [adr:0093, adr:0094, adr:0095, adr:0096, adr:0097, "claude.md:§2.2", "claude.md:§2.5", "finding:f78", "finding:f79", "finding:f81"]
discovered_by: a verify-the-gap probe (the TUPLE analogue of the str/bytes/list indexing arc; NO tuple_e2e test acknowledged it)
---

# F83 — tuple `t[i]` SILENT-0 MISCOMPILE (§2.2)

## What (verified at HEAD 89e8627, the F80 follow-up tree)

```
# (7, "x")[0]      -> 0   CPython: 7    <- SILENT WRONG, builds OK, exit 0
# (1, "a", 2)[2]   -> 0   CPython: 2    <- SILENT WRONG
```

`t[i]` on a tuple base BUILT OK with zero warnings and RAN returning `0`.
There was NO `tuple_e2e` test acknowledging it — the last unclosed member of
the cross-type indexing arc (`str` F78/F79, `bytes`, `list` F81).

## Root cause — two stub layers

1. **MIR** (`crates/cobrust-mir/src/lower.rs`). `lower_index` returned
   `IndexKind::Tuple(_) => Constant::Int(0)` (and `Slice => Int(0)`), AND the
   `ExprKind::Index` rvalue lowering had NO `Ty::Tuple` branch (lists/str/bytes
   had theirs) — a tuple index fell through to the generic
   `Projection::Index(Int(0))` no-op.
2. **LLVM backend** (`crates/cobrust-codegen/src/llvm_backend.rs`). A
   `Ty::Tuple` lowered to an opaque-pointer NULL stub. BOTH tuple CONSTRUCTION
   (`lower_aggregate` `Tuple` arm returned `const_null`) AND a
   `Projection::Field` READ (`lower_place_load` fell through to a bare-local
   "stub load" that ignored the projection) were UNIMPLEMENTED. A tuple was a
   null pointer; every `t[i]` re-read field 0's storage / a garbage slot.

The type checker ALREADY typed a LITERAL-index `t[i]` correctly (constant-fold
to the exact per-position element type via `resolve_tuple_index`) — but a
NON-literal index silently fell back to the HEAD element type (a §2.2
miscompile for a mixed tuple), and a constant-OOB index folded to `Ty::Never`
(which collapsed to the MIR `Int(0)` stub).

## Contrast — str/bytes/list were already fixed

F78/F79/F81 (ADR-0093/0094/0095/0096) closed the index-correctness class for
`str` (codepoint), `bytes` (byte), `list` (element). F83 closes it for `tuple`
(per-position constant-index field). A tuple is HETEROGENEOUS — its element
type is only knowable for a COMPILE-TIME constant index, so the fix is a
struct field read for a constant, and a compile-time REJECT for a dynamic /
OOB index (§2.5-A), NOT a runtime trap.

## Fix (ADR-0097)

- **LLVM**: `Ty::Tuple` → a real `struct_type`; `lower_aggregate_tuple` builds
  the struct VALUE via `build_insert_value`; `lower_place_load` gains a
  `[Projection::Field(i)]` arm reading via `build_extract_value` (the safe
  no-GEP Array path). `llvm_scalar_ty(Ty::Tuple)` returns the struct so a
  tuple local keeps its struct alloca; `llvm_operand_ty` resolves a `Field(i)`
  projection to the i-th FIELD's type (else a `Ty::Str` field dest inferred the
  whole `{i64,ptr}` tuple → an LLVM "Call parameter type does not match"
  verify error on `println_str_buf`/`str_drop`).
- **MIR**: a `Ty::Tuple` branch in `ExprKind::Index` reads
  `Projection::Field(off)` for a constant index (`literal_int_value_mir`,
  Python-negative normalised against the static arity) as the resolved element
  type — NOT the `Int(0)` stub; a non-literal / OOB index hits a
  defense-in-depth `MirError`. `synth_expr_ty` gains an `ExprKind::Tuple` arm.
- **check.rs**: the `(Ty::Tuple, IndexKind::Expr)` arm now REJECTS a constant
  OOB index AND a non-literal index with `TypeError::NotIndexable` (§2.5-A,
  §2.5-B fix-printing hints) — REUSED variant, no cascade.

## Verification note

Reproduced independently at the probe tree: `(7, "x")[0]` printed `0`
(CPython `7`). Post-fix: `(7,"x")[0]==7`, `(1,"a",2)[2]==2`, `t[0]+t[2]`
arithmetic, `(10,20,30)[-1]==30`; `(1,2)[5]`/`(1,2)[-5]`/`t[i]` (dynamic)
reject at build with a clean §2.5-B fix-printing diagnostic. A tuple owning a
`str` reads its other field without corrupting / double-freeing the owned
field (leak-or-free-once, never double-free — the existing tuple drop
discipline). Differential e2e: `tuple_e2e` (10 tests, CPython-3 oracle).
LC-100 + full `cobrust-cli` + MIR/types cascade-parity stay green.

## Ownership note (known bounded gap)

A tuple's `str`/`list` field is owned by the tuple but a `t[i]` read returns
it as a borrow (`Operand::Copy`); the tuple drop is a NO-OP (unchanged), so a
tuple-owned str LEAKS rather than double-freeing. A future ADR can wire a
per-field tuple drop (route through `__cobrust_str_drop`/`_list_drop` like
`list[str]`'s `__cobrust_list_drop_elems`). Out of F83 scope — F83 is ONLY the
§2.2 tuple-index SILENT-0 correctness bug.
