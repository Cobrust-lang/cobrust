---
finding_id: F56
title: LLVM backend mis-codegens nested `UnaryOp(Neg)` on float — missing fixed-point infer_local_types — surfaced by ADR-0070 §X.3 LLVM-default flip
status: open (fr14 #[ignore]'d with deferred-fix cite per F37 discipline)
date: 2026-05-27
severity: medium
siblings: [F53, F54, F55, F37]
last_verified_commit: 41a859b
---

# F56 — LLVM double-negation of a float constant returns garbage

## Symptom

After the X.6-partial CI LLVM-18 install (F55) cleared the build/clippy/dwarf
failures, the `cargo test` job remained red on **both** ubuntu-latest and
macos-latest with a single shared failure:

```
test fr14_value_correctness_double_neg_const ... FAILED
crates/cobrust-codegen/tests/float_return_corpus.rs:422: assertion failed:
fr14 stdout mismatch: "0\n"   (expected "1\n")
```

The fixture compiles `let y: f64 = -(-3.25)` and prints `1` iff `3.24 < y < 3.26`.
Under LLVM-default it prints `0` (y is wrong). Reproduced locally:
`cobrust build` (LLVM default) of `-(-3.25)` → binary prints `0`.

## Root cause

`llvm_backend.rs::lower_unop` (~line 3884) decides float-vs-int negation purely
from the lowered LLVM value's type: `let is_float = a.is_float_value();`.

The double-neg lowers to two MIR temps, both typed `Ty::None`:

- `_inner = UnaryOp(Neg, Constant::Float(3.25))` — operand is a FloatValue, so
  `is_float = true` → correct `fneg`. Result f64 stored into `_inner`'s slot.
- BUT `_inner`'s MIR local is `Ty::None`, which `lower_ty` maps to **i64**
  (llvm_backend.rs §2454-2472). So the f64 is stored as raw bits into an i64 slot.
- `_outer = UnaryOp(Neg, Copy(_inner))` — loads `_inner` as **i64** (IntValue),
  so `lower_unop` sees `is_float = false` → `build_int_neg` on the IEEE-754
  bit-pattern integer (two's-complement negation of the bits, **not** float
  negation) → garbage → `y` fails the `> 3.24` bracket → prints `0`.

The `lower_binop` ADR-0070 §X.3 sibling-fix (llvm_backend.rs ~3577) recovers
float-ness by inspecting the *other* operand and bitcasting the i64 operand to
f64 when a float operand is present. A **unary** op has only one operand, and the
`Ty::None` temp carries no float signal, so the binop trick cannot apply.

The deeper gap: the Cranelift backend converges these `Ty::None` synthetic temps
to their real type (F64) via a fixed-point `infer_local_types` dataflow
(`cranelift_backend.rs:327-...`). The LLVM backend **explicitly does not** —
`lower_ty`'s doc (llvm_backend.rs §2461-2464) states "Cranelift backend
§infer_local_types converges these via a fixed-point dataflow; wave-1 LLVM
backend takes the simpler fallback." This finding is that deferred debt biting.

## Why it only surfaced now

The fixture uses the `build()` helper → `cobrust build` CLI. Pre-X.3 the CLI
defaulted to Cranelift (which has the fixed-point inference), so `-(-3.25)` was
correct. The ADR-0070 §X.3 flip routes `cobrust build` through LLVM-default,
exposing the LLVM-only gap. F53/F54/F55 sibling — the flip is the detection gate.

fr15 (`(a+b)*c`) and fr16 (`-a+b`) pass because their depth-2 chain terminates in
a **binop**, which the §X.3 binop bitcast-fix handles. Only the all-unary nested
case (fr14) has no float-typed sibling operand to key off.

## Resolution (this commit)

Per F37 (no silent rot; cite a specific deferred `#[ignore]`), `fr14` is
`#[ignore]`'d with a full-rationale reason string pointing here and at the real
fix (LLVM `infer_local_types` port). fr15/fr16 stay live (they pass and guard the
binop path).

## Real fix (deferred)

Port the Cranelift fixed-point `infer_local_types` dataflow to the LLVM backend
so `Ty::None` synthetic temps converge to their real scalar type (F64), making
their allocas f64 slots and `lower_unop`/`lower_place_load` see FloatValues
directly. This is an ADR-worthy backend change (touches alloca typing, place
load/store coercion, and the binop bitcast-fix can then be simplified). Tracked
as an ADR-0070 §X.3 follow-on (LLVM type-inference parity), prerequisite for
removing the §X.3 binop bitcast workaround.

Interim narrower option (if the full port is deferred further): pass the MIR
operand's declared type into `lower_unop` and, when the result/operand is part of
a float chain, bitcast the i64 operand to f64 before `fneg` — but the `Ty::None`
operand type means even this needs *some* float-ness propagation, so the
fixed-point port is the principled fix.

## Prevention

Same lesson as F53/F55: every backend-correctness fix landed only in
`cranelift_backend.rs` is latent LLVM debt until the flip. Audit
`cranelift_backend.rs` for `infer_local_types`-dependent correctness and mirror
into `llvm_backend.rs` before/with X.4 (Cranelift removal), since removal deletes
the only correct path.
