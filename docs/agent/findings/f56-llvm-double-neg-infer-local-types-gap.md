---
finding_id: F56
title: LLVM backend mis-codegens nested `UnaryOp(Neg)` on float — missing fixed-point infer_local_types — surfaced by ADR-0070 §X.3 LLVM-default flip
status: RESOLVED (fr14 un-ignored + green; LLVM `infer_local_types` fixed-point ported)
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

The Cranelift fixed-point `infer_local_types` dataflow was **ported to the LLVM
backend** (`llvm_backend.rs`), and `fr14` was un-`#[ignore]`'d. `fr14` now passes;
the whole `cobrust-codegen` suite stays green with no new failures/ignores.

What was added to `llvm_backend.rs` (new §4.0 block, immediately before
`lower_ty`):

- `llvm_scalar_ty(&Ty) -> Option<BasicTypeEnum>` — the LLVM analogue of
  `abi::cranelift_scalar_ty(..).is_some()`. Returns the resolved scalar
  `BasicTypeEnum` for `Bool/Int/IntN/Float/Imag` (and, transparently,
  `Ref(scalar)`); returns `None` for `Ty::None` and every pointer-lowered
  indirect type, so those stay *candidates*.
- `llvm_rvalue_ty` + `llvm_operand_ty` — 1:1 ports of `cranelift_backend::{rvalue_ty,
  operand_ty}`. `UnaryOp(_, a) → operand_ty(a)`; `BinaryOp(cmp/bool/in → i1, else →
  operand_ty(a))`; `Use(op) → operand_ty(op)`; `Copy/Move(p)` prefers the `inferred`
  map then the declared scalar type then the opaque pointer; `Constant` maps
  Bool/None→i1, Int→i64, Float/Imag→f64, Str/Bytes/FnRef→`i8*`.
- `infer_local_types(&Body) -> HashMap<LocalId, BasicTypeEnum>` — the bounded
  fixed-point: a `Constant::FnRef`-destination pre-pass (via `body_return_types`),
  then iterate over candidates resolving each from the first `Assign` rvalue (or
  known-body Call destination) that yields a type under the current partial map,
  until a fixed point or `candidates.len()+1` iterations. **The fixed-point — not a
  single pass — is what resolves the chain-depth-2 case**: `_outer ← Copy(_inner)`
  only resolves to f64 once `_inner` (resolved from `UnaryOp(Neg, Float)` in an
  earlier iteration) is in the map.

Integration (the actual behavior change), in `define_body`'s alloca loop: the
inferred map is computed once before the loop; for each non-return local, the
alloca type is `inferred_local_tys.get(&local.id)` when present, else
`lower_ty(&local.ty)` (preserving the historical `Ty::None → i64` fallback for
genuinely-untyped pointer / `_callret` slots). The return slot keeps `ret_ty`,
and locals with a real declared scalar type keep `lower_ty` (they are not
candidates). With `_inner` / `_outer` now allocated as `double`, store/load
round-trip on the float path and `lower_unop` sees a `FloatValue` →
`build_float_neg` (fneg). fr15/fr16 still pass via the existing binop bitcast-fix
(now redundant for inferred locals, but left intact as belt-and-suspenders).

### Divergence from the Cranelift reference

- **No runtime-helper-return map on the LLVM side.** Cranelift's pre-pass +
  fixed-point also resolve `Constant::Str(helper)` call destinations via
  `runtime_helper_return_types`; the LLVM emitter only caches
  `runtime_helper_param_counts`, so that branch is omitted. The `FnRef(known body)`
  branch (`body_return_types`) and the `Assign` fixed-point are retained — sufficient
  for fr14 and the whole-crate suite. Runtime-call destinations of `Ty::None` type
  keep today's `i64` fallback (unchanged).
- **`Ty::Ref(inner)` is scalar under the LLVM `lower_ty`** (transparent recursion),
  whereas Cranelift treats it as non-scalar. `llvm_scalar_ty` mirrors `lower_ty`'s
  notion so alloca/load/store types stay consistent.

### Note for X.4 (Cranelift removal)

The §X.3 `lower_binop` bitcast-fix is now superseded for inferred locals (their
operands carry real float types), but it is **not** removed in this commit (out of
scope; the whole-crate suite is the only guard and removing it risks regressing a
path the inference doesn't cover, e.g. a runtime-call-fed binop). Revisit during
X.4 when Cranelift parity is the explicit deliverable.

## Prevention

Same lesson as F53/F55: every backend-correctness fix landed only in
`cranelift_backend.rs` is latent LLVM debt until the flip. Audit
`cranelift_backend.rs` for `infer_local_types`-dependent correctness and mirror
into `llvm_backend.rs` before/with X.4 (Cranelift removal), since removal deletes
the only correct path.
