---
doc_kind: adr
adr_id: 0097
title: "tuple `t[i]` index correctness — real LLVM struct lowering + per-position constant-index field read (F83; COMPLETES the str/bytes/list/tuple indexing arc)"
status: accepted
date: 2026-06-13
last_verified_commit: HEAD
supersedes: []
superseded_by: []
relates_to: [adr:0093, adr:0094, adr:0095, adr:0096, finding:f78, finding:f79, finding:f81, finding:f83, "claude.md:§2.2", "claude.md:§2.5"]
---

# ADR-0097: tuple `t[i]` index correctness

## Context

A verify-the-gap probe (2026-06-13) found a §2.2 bug in the TUPLE index
operator surface — the last unclosed member of the cross-type indexing arc
(`str` F78/F79, `bytes`, `list` F81), tracked as finding **F83**:

**`(7, "x")[0]` SILENT-0 MISCOMPILE.** The program BUILT OK with zero
warnings and RAN returning `0` (CPython `7`). There was NO `tuple_e2e` test
acknowledging it. Two stub layers stacked:

1. **MIR** (`crates/cobrust-mir/src/lower.rs`). `lower_index` returned
   `IndexKind::Tuple(_) => Constant::Int(0)` (and `Slice => Int(0)`), AND the
   `ExprKind::Index` rvalue lowering had NO tuple branch (lists/str/bytes had
   theirs) — so a tuple index fell through to the generic
   `Projection::Index(Int(0))` no-op, reading the wrong slot.
2. **LLVM backend** (`crates/cobrust-codegen/src/llvm_backend.rs`). A
   `Ty::Tuple` lowered to an opaque-pointer NULL stub. BOTH tuple
   CONSTRUCTION (`lower_aggregate` `Tuple` arm returned `const_null`) AND a
   `Projection::Field` READ (`lower_place_load` fell through to a bare-local
   "stub load" that ignored the projection) were unimplemented. A tuple was
   effectively a null pointer; every `t[i]` re-read field 0's storage / a
   garbage slot.

The type checker (`crates/cobrust-types/src/check.rs`, the `(Ty::Tuple,
IndexKind::Expr)` arm) ALREADY typed `t[i]` correctly for a literal int index
(constant-fold to the exact per-position element type via
`resolve_tuple_index`) — a tuple is HETEROGENEOUS: `(i64, str)[0]` is `i64`,
`[1]` is `str`. But for a NON-literal index it silently fell back to the HEAD
element type (a §2.2 miscompile for a mixed tuple), and a constant-OOB index
folded to `Ty::Never` (which collapsed to the MIR `Int(0)` stub).

## Decision

Lower a tuple to a REAL LLVM struct VALUE and read a `t[i]` field with a
COMPILE-TIME constant index via the safe no-GEP aggregate path, mirroring the
EXISTING `Projection::Field` discipline that tuple CONSTRUCTION + the
let-destructure path already use. A tuple's element type is only knowable for
a constant index, so a NON-CONSTANT or OOB index is REJECTED at check time
(§2.5-A compile-time-catch), NEVER a silent head-element / `0` miscompile.

### Type checker (`check.rs`, the `(Ty::Tuple, IndexKind::Expr)` arm)

- **LITERAL int index, in range** → constant-fold to the EXACT per-position
  element type (`resolve_tuple_index`, normalising `i<0 -> arity+i`).
- **LITERAL int index, OUT OF BOUNDS** (`(1,2)[5]`, `(1,2)[-5]`) → REJECT with
  `TypeError::NotIndexable` (suggestion: "tuple index out of bounds — use a
  constant in [0, len-1] …"), mirroring the `t.N` tuple-field OOB reject + the
  array-OOB literal reject. Replaces the old silent `Ty::Never` fold.
- **NON-LITERAL index** (`t[i]`) → REJECT with `TypeError::NotIndexable`
  (suggestion: "a tuple needs a CONSTANT integer index (e.g. `t[0]`, `t[-1]`)
  — its elements have heterogeneous types …; use a constant, or convert to a
  list if you need dynamic indexing"). Replaces the head-element fallback (the
  §2.2 silent miscompile for a mixed-type tuple). `NotIndexable` is REUSED
  (existing `{ actual, span, suggestion }` payload) — NO new error variant →
  NO cascade through error_cb / error_ux / lsp / types-parity (the byte-parity
  tripwire stays green).

### MIR (`lower.rs`, the `ExprKind::Index` rvalue lowering)

A dedicated `Ty::Tuple` branch (before the generic `Projection::Index`
fall-through): for a CONSTANT integer index (`literal_int_value_mir`,
Python-negative normalised against the static arity), it materialises the
tuple base in a temp and emits `tuple_place.with_projection(
Projection::Field(off))` read as the per-position element type the checker
resolved — NOT the `Int(0)` stub. A non-literal / OOB index reaching MIR (the
checker bypassed) hits a defense-in-depth `MirError` (constitution §6), NEVER
the silent stub. `synth_expr_ty` gains an `ExprKind::Tuple` arm so a tuple
LITERAL base also routes here.

### LLVM backend (`llvm_backend.rs`)

- **`lower_ty(Ty::Tuple(items))`** → a real `struct_type` of the lowered
  element types (heterogeneous, fixed-arity). A `str`/`list` field is a
  pointer field.
- **`lower_aggregate_tuple`** → builds the struct VALUE field-by-field via
  `build_insert_value` (a `Constant::Str` literal field → a fresh str-buffer;
  a non-literal `Str` field → `__cobrust_str_clone`, mirroring
  `lower_aggregate_list`; else `lower_operand` direct) — NOT `const_null`.
- **`lower_place_load`** gains a `[Projection::Field(i)]` arm: load the struct
  aggregate then `build_extract_value(i)` — the safe no-GEP path the Array
  index uses (codegen `#![forbid(unsafe_code)]`).
- **`llvm_scalar_ty(Ty::Tuple)`** returns the struct type so a tuple local is
  NOT an inference candidate (keeps its `lower_ty` struct alloca instead of
  the `Rvalue::Aggregate → opaque ptr` default that made the tuple temp a
  pointer slot). **`llvm_operand_ty`** resolves a `Field(i)` projection on a
  tuple base to the i-th FIELD's type (not the whole struct) — without this, a
  `Ty::Str` field dest (an inference candidate) inferred the WHOLE `{i64,ptr}`
  tuple struct, producing a struct alloca for a str local + a struct value
  passed to `__cobrust_println_str_buf` / `__cobrust_str_drop` (an LLVM verify
  "Call parameter type does not match" error).

## Scope

SUPPORTED: `t[i]` with a CONSTANT integer index (positive or Python-negative)
in range. REJECTED at check: non-literal index, constant OOB (both
directions). Tuple SLICE (`t[lo:hi]`) is not in F83 scope (CPython returns a
tuple; deferred).

### Ownership / drop (known bounded gap, consistent with prior behavior)

A tuple's `str`/`list` field is OWNED by the tuple but a `t[i]` read returns
the field value as an `Operand::Copy` (a BORROW of the aliased pointer). The
tuple drop is a NO-OP (drop.rs no-ops `Ty::Tuple`, unchanged), so a
tuple-owned str LEAKS rather than double-freeing — the SAME discipline tuples
had before this ADR. Binding an owned field (`let s: str = t[0]`) schedules
`s` for a single str-drop; the tuple's no-op drop means NO double-free. This
is memory-safe (leak-or-free-once, never double-free); a future ADR can wire a
per-field tuple drop (route through `__cobrust_str_drop` / `_list_drop` like
`list[str]`'s `__cobrust_list_drop_elems`).

## Consequences

- `(7, "x")[0] == 7`; `(1, "a", 2)[2] == 2` (per-position typing); `t[0]+t[2]`
  arithmetic; `(10,20,30)[-1] == 30`. `(1,2)[5]` / `(1,2)[-5]` / `t[i]` reject
  at build with a §2.5-B fix-printing diagnostic.
- The cross-type indexing arc is now **COMPLETE**: `str` (codepoint, ADR-0094/
  0095), `bytes` (byte, ADR-0093), `list` (element, ADR-0096), and `tuple`
  (per-position constant-index field, this ADR) are ALL index-correct. Tuples
  additionally gain REAL struct lowering (construction + field read), closing
  the LLVM-backend `Ty::Tuple` opaque-null stub.
- LC-100 (`leetcode_corpus_e2e`) + the full `cobrust-cli` suite stay green;
  the MIR/types cascade-parity suites stay green (no new variant).
- New e2e: `crates/cobrust-cli/tests/tuple_e2e.rs` (10 tests, CPython-3
  oracle), the coverage F83 found MISSING.
