---
finding_id: F87
title: '`print(<inline float binary-op>)` CRASHES the compiler — an f64 binop value is dispatched to `__cobrust_println_int(i64)` → LLVM module-verify fail'
date: 2026-06-14
status: resolved
resolved_by: ADR-0089 §6 (2026-06-14)
severity: major
discovered_by: verify-the-gap idiom probe (2026-06-14, post-F86)
relates_to: ["claude.md:§2.2", "claude.md:§5.1", "ADR-0089"]
---

# F87 — `print(<inline float binop>)` crashes `cobrust build`

## What (verified at HEAD ~04ece86 vs CPython 3.11)

`print(<any INLINE float binary-op expression>)` CRASHES the compiler:
`cobrust build` exits 3 with an LLVM module-verify error
("Call parameter type does not match function signature") because a FLOAT
value is passed to `__cobrust_println_int(i64)`.

| program | result | |
|---|---|---|
| `print(7.0 / 2.0)` | **build-exit 3**, LLVM verify fail (float → println_int) | ✗ (CPython: `3.5`) |
| `print(7.0 + 2.0)` | same crash | ✗ |
| `let x: f64 = 7.0 / 2.0; print(x)` | BUILD-OK, prints `3.5` | ✓ |

The DECLARED-f64 var case dispatches correctly. The bug is ONLY the INLINE
binary-op arg.

## Why it matters (§5.1 + §2.2)

1. **Compiler crash on valid, type-checked input** (§5.1): the type-checker
   ACCEPTS `print(7.0 / 2.0)` — the crash is a backend miscompile, not a
   user error. The compiler MUST NOT crash on valid input.
2. **§2.5 LLM-first**: `print(a / b)` for floats is an extremely common idiom
   an LLM writes on the first try; crashing on it is a sharp footgun.

## Root cause / fix (same class as ADR-0089 abs / unary `_un`)

The print-dispatch monomorphizer (`rewrite_print` in
`crates/cobrust-cli/src/build/intrinsics.rs`) reads the print arg local's
resolved `Ty` to pick `__cobrust_println_int` vs `__cobrust_println_float`.
An INLINE binop is lowered into a `_bin` temp local. That temp was declared
with `Ty::None` in `lower_bin` (`crates/cobrust-mir/src/lower.rs`), and the
print rewrite maps an unresolved `Ty::None` arg local → `Ty::Int` →
`__cobrust_println_int(i64)`. Codegen then hands the f64 binop value to the
i64 shim → LLVM verify fail. A DECLARED-f64 var already worked because the
var local carries `Ty::Float`.

Fix: type the `_bin` temp with the RESOLVED scalar result type instead of
the bug-prone `Ty::None` — exactly the ADR-0089 `lower_un` `_un` fix and the
`abs`/`min`/`max`/`sum` return-type overrides (dispatch on the resolved
DEST type, never a fragile `Ty::None` temp). The result type is computed by
`synth_bin_result_ty(op, lt, rt)`, a helper EXTRACTED from `synth_expr_ty`'s
`Bin` arm so an inline binop arg and a declared-typed var resolve IDENTICALLY
(one source of truth): scalar Int/Float operands → arithmetic Float-if-either-
Float-else-Int, comparisons → Bool, bit/shift → Int. Non-scalar
(Buffer/Str/Dict) operand pairs stay `Ty::None` (resolved by the existing
Buffer/Str-binop paths above), so the change is conservative.

## Resolution (ADR-0089 §6, 2026-06-14)

`lower_bin`'s `_bin` temp is now declared with
`synth_bin_result_ty(op, &lhs_ty, &synth_expr_ty(self, rhs))`. A Float binop
→ `Ty::Float` → `__cobrust_println_float(f64)`; an Int binop still →
`Ty::Int` → `__cobrust_println_int(i64)` (unchanged). Verified vs CPython
3.11: `print(7.0 / 2.0)` == `3.5`; integer-valued float results print
WITHOUT `.0` (`9.0`→`9`, `5.0`→`5`, `6.0`→`6`, `3.0`→`3`) per the existing
`__cobrust_println_float` Rust-`{}` repr (the `math_e2e` convention), NOT
CPython's `9.0`. `+ - * / //` in both operand orders, computed-var operands,
and nested binops all dispatch correctly; int binop / int-var / bool / str
print unchanged.

Corpus: `crates/cobrust-cli/tests/print_float_binop_e2e.rs` (7 tests).
