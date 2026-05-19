---
doc_kind: adr
adr_id: 0060a
parent_adr: 0060
title: "Phase M wave-1 — narrow-int types (i32 + i8) source-level"
status: accepted
date: 2026-05-19
ratified_at: 2026-05-19
last_verified_commit: 2d84de5
supersedes: []
superseded_by: []
relates_to: [adr:0060, adr:0058a, adr:0006, adr:0023]
discovered_by: P10 Phase M sprint per ADR-0058a §15 gaps #1 #2
---

# ADR-0060a: Phase M wave-1 — narrow-int types (i32 + i8)

## 1. Context

ADR-0006 §"Type universe" pins integer arithmetic at single-width
`Ty::Int = i64`. ADR-0058a §15 gaps #1 and #2 identify the missing
`i32` and `i8` source-level narrow-int types needed to honor four F36
fixture rename promises in `codegen_diff_corpus.rs` (lines 443, 457).

The Cranelift backend `lower_ty_wave1` and the LLVM backend `lower_ty`
already emit `i32` / `i8` machine types **internally** (e.g. `bool`
widens via `i8`, the `__cobrust_panic` second arg is `i64` length, the
DI builder declares 8-bit bools). What's missing is a source-level
spelling that flows i32 / i8 through the parser → types → MIR →
codegen pipeline without auto-widening to i64.

## 2. §2.5 LLM-first ROI

§2.5 §D (method-call sugar / training-data overlap) — *very high*:

- `i32` / `i8` are dominant in Rust corpus + C/C++ corpus.
- Python uses `int` (unbounded big-int), so Python-prior is neutral
  here; Rust-prior dominates.
- LLM writes `fn f(x: i32) -> i32` reflexively when the field calls
  for a 32-bit value (file offsets, hash widths, packed columns).

§2.5 §A (compile-time-catch) — adds **`TypeError::NarrowIntOverflow`**
when an integer literal exceeds the target width at type-check time
(e.g. `let x: i8 = 200` ⇒ overflow at parse-time, not runtime).

## 3. Decision

Three additive surface changes — no breakage:

### 3.1 AST: extend `TypeKind` lookup path

`TypeKind::Name(["i32"])` and `TypeKind::Name(["i8"])` already parse
(they're just two-char identifiers). The frontend parser needs **no
new variant** — the named-type lookup in `cobrust-types
::check::lower_named_type` is the resolution site.

### 3.2 Types: add `Ty::IntN(u8)` variant

Add a new variant to the `Ty` enum:

```rust
pub enum Ty {
    // ...
    /// Narrow signed integer: `i8`, `i16`, `i32`. Width in bits.
    /// `Ty::Int` (width 64) remains the canonical big-int spelling.
    IntN(u8),
    // ...
}
```

`width` is one of `{8, 16, 32}` for wave-1. `16` is included as a
future-proof spelling but no parser surface exposes it in wave-1
(the §15 queue lists only #1 + #2, i.e. `i32` + `i8`); the typeck
synthesises i16 only via cast.

Unification rule: `IntN(a)` unifies with `IntN(b)` iff `a == b`. It
does NOT unify with `Ty::Int` directly — narrowing requires an
explicit cast (the wave-1 cast surface is `i64(...)` / `i32(...)` /
`i8(...)`, paralleling the existing `int(...)` / `float(...)` cast
forms in MIR lower `Cast` rvalue, see `cobrust-mir::lower.rs:1615`).

### 3.3 MIR: extend cast surface

`MirLower::lower_cast` already handles `"i64" | "int"` ⇒ `(FloatToInt,
Ty::Int)`. Add three new cast targets:

```rust
"i32" => (CastKind::IntNarrow(32), Ty::IntN(32)),
"i8"  => (CastKind::IntNarrow(8),  Ty::IntN(8)),
"i16" => (CastKind::IntNarrow(16), Ty::IntN(16)),  // internal only
```

`CastKind::IntNarrow(width)` is a new variant in `cobrust-mir::CastKind`.
It is semantically `Ty::Int → Ty::IntN(w)` (truncation) or
`Ty::IntN(w₁) → Ty::IntN(w₂)` (extend / truncate by width compare).

### 3.4 Codegen: extend `lower_ty` / `lower_ty_wave1`

Both backends grow new `IntN(w)` arms:

| width | Cranelift | LLVM (inkwell) |
|---|---|---|
| 8 | `ir::types::I8` | `ctx.i8_type()` |
| 16 | `ir::types::I16` | `ctx.i16_type()` |
| 32 | `ir::types::I32` | `ctx.i32_type()` |

`lower_ty_wave1` returns the Cranelift type unchanged. `lower_ty`
returns `BasicTypeEnum::IntType`. The LLVM backend's DI builder
extends `populate_di_basic_types` to register `i32` / `i8` (`bool`
already at 8-bit per existing code).

### 3.5 Codegen: extend cast lowering

For `CastKind::IntNarrow(w)`:

- **i64 → i8/i16/i32**: `builder.build_int_truncate(src, target_ty, "trunc")`
  (LLVM); `builder.ins().ireduce(target_ty, src)` (Cranelift).
- **i8/i16/i32 → i64**: `builder.build_int_s_extend(src, i64_ty, "sext")`
  (LLVM); `builder.ins().sextend(I64, src)` (Cranelift).
- **i8 → i32 / i32 → i8 etc.**: chain via the appropriate extend or
  truncate primitive.

The narrow types are `Copy` (drop fast-path in
`cobrust-mir::drop.rs:149`) — add `Ty::IntN(_)` to the
`is_copy_scalar` match arm.

### 3.6 Type-check: literal-fit guard

When checking `let x: i8 = <int-lit>`, evaluate the literal's i128
representation and assert it fits in `[i8::MIN, i8::MAX]`. Out-of-range
fires `TypeError::NarrowIntOverflow { width, literal, span,
suggestion: "use a value in [-128, 127]" }`.

## 4. Surface examples

```cobrust
fn count_bytes(x: i32) -> i8:
    return i8(x % 128)

fn pack(a: i32, b: i32) -> i32:
    return (a << 16) | (b & 0xffff)

# Overflow caught at parse-time:
let bad: i8 = 200  # TypeError::NarrowIntOverflow
```

## 5. Acceptance

- 6 unit tests in `cobrust-types`: lookup `i8` / `i16` / `i32`
  produces `Ty::IntN(_)`; literal-fit guard fires on overflow.
- 2 unit tests in `cobrust-mir`: `IntNarrow(8)` and `IntNarrow(32)`
  cast lowering shapes.
- 4 codegen corpus fixtures:
  - `llvm_type_02_i32` (renamed from `llvm_type_02_i64_baseline`)
  - `llvm_type_03_i8` (renamed from `llvm_type_03_i64_passthrough`)
  - `llvm_type_13_i32_narrow_cast` (new)
  - `llvm_type_14_i8_overflow_lit` (new, expects TypeError)

## 6. Anchors

- 0060a-F34: narrow-int type closure canonical
- 0060a-F35: sibling 0060 (frame) + 0060b (trio)
- 0060a-F36: 4 fixture renames now match what they test
- 0060a-F37: zero `#[ignore]` introduced

## 7. Cross-references

- ADR-0060 — Phase M frame
- ADR-0006 §"Type universe" — `Ty::Int` baseline
- ADR-0058a §4.1 — LLVM type table extension
- ADR-0023 — Cranelift backend row extension
- `cobrust-mir::lower.rs:1615` — existing cast surface
- `cobrust-mir::drop.rs:149` — Copy fast-path
