---
doc_kind: adr
adr_id: 0102
title: '`**` power operator — typed-result by operand type (int**int->int, any-float->f64)'
status: accepted
date: 2026-06-14
last_verified_commit: 66854e16
supersedes: []
superseded_by: []
---

# ADR-0102: `**` power operator (typed result by operand type)

## Context

Finding **F90** (§2.5 LLM-first): the `**` POWER operator REJECTED at
codegen with `CodegenError::UnimplementedBinOp { op: "**" }` (the honest
ADR-0041 §H3 "deferred" surface). `**` is one of the most common Python
operators an LLM agent writes (`2 ** n`, `x ** 2`, `base ** exp`), so its
absence was a constant first-try failure — directly against §2.5's
*Maximize-overlap-with-training-data*. This was an ADDITIVE gap (a clean
reject, NOT a silent miscompile), so the fix simply wires the operator
through.

The load-bearing design problem: **Python `**`'s result type depends on the
exponent SIGN at RUNTIME.** `2 ** 3 == 8` is an `int`, but `2 ** -1 == 0.5`
is a `float`. A static type system cannot make `int ** int` be BOTH `int`
and `float` — Cobrust must PIN one typed result per operand-type shape and
handle the divergent cases explicitly (§2.5 *compile-time-catch* where
possible, a runtime trap where not).

This resolves the `**` portion of **ADR-0041 §H3** (the `** / @ / in / not
in` "unimplemented binop" cluster); `@` / `in` / `not in` remain §H3
deferrals.

## Options considered

1. **`int ** int -> int` (i64), pinned by operand type; reject a
   NEGATIVE-LITERAL exponent at compile time; TRAP on overflow and on a
   runtime-dynamic negative exponent. Any float operand -> f64 (promote).**
   - Pro: the integer result is the overwhelmingly common case (`2 ** n`,
     loop bounds, bit math) and stays an honest `int`. The one case that
     CANNOT be an int — a negative exponent — is caught at COMPILE time for
     the literal form (§2.5-A, the strongest LLM feedback signal; mirrors
     **F79**'s negative-literal scalar-index reject) with a §2.5-B
     fix-printing diagnostic ("use a float base"). Overflow TRAPS (§2.2 — no
     silent wrap). `**` is the ONE arithmetic op that promotes a mixed
     int/float pair, and only because the float exponent makes the result a
     float UNAMBIGUOUSLY.
   - Con: `int ** int` with a runtime-dynamic negative exponent (a variable)
     can only be caught at runtime (a trap), not at compile time. Acceptable
     — it is a trap (no wrong value), and the literal case (the common one)
     IS caught at compile time.

2. **`int ** int -> float` always (CPython's "promote everything to float"
   is NOT what CPython does, but a hypothetical uniform rule).**
   - Pro: one result type, no negative-exponent special case.
   - Con: `2 ** 10` would be a `float` (`1024.0`), diverging from CPython
     (`1024`, an int) and surprising every LLM that writes integer power in
     an integer context (array sizing, bit masks). Forces a `.0`-dropping
     repr everywhere and loses exact large-integer results. Rejected.

3. **Keep rejecting `**` (status quo, ADR-0041 §H3).**
   - Con: permanent first-try failure on a ubiquitous operator; the §2.5
     deficit this finding exists to close. Rejected.

## Decision

**Option 1.** The typed result is pinned by the operand types:

| operand shape | result | lowering |
|---|---|---|
| `int ** int` | `int` (i64) | `__cobrust_ipow(i64, i64) -> i64` |
| `float ** float` | `f64` | `__cobrust_math_pow(f64, f64) -> f64` (libm `pow`) |
| `int ** float` | `f64` (promote) | `__cobrust_math_pow`; int base cast i64→f64 |
| `float ** int` | `f64` (promote) | `__cobrust_math_pow`; int exp cast i64→f64 |

CPython identities preserved: `base ** 0 == 1` (incl. `0 ** 0 == 1`),
`base ** 1 == base`.

**Compile-time reject (§2.5-A).** `int ** int` with a NEGATIVE-LITERAL
exponent (`2 ** -1`, `base ** -3`) is a compile error
(`TypeError::NegativePowExponent`, exit 2). A negative power yields a
non-integer, impossible for the pinned `int`-result. The diagnostic PRINTS
THE FIX (§2.5-B): *"use a float base — write `float(base) ** exp` or make
the base a float literal (e.g. `2.0 ** -1`)"*.

**Runtime traps (§2.2 — never a silent wrong value).** `__cobrust_ipow`:

- **Overflow** (`2 ** 63` and beyond) — `i64::checked_pow` returns `None`
  → `panic` → exit 3. CPython promotes to bignum; Cobrust's i64 has no
  bignum (yet), so a trap is the honest surface, NOT a silent wrap.
- **Runtime-dynamic negative exponent** (a variable the type checker
  cannot sign-check) — `panic` → exit 3 (a `Ty::Int`-result shim cannot
  return a 0.5 without silently changing the value's type).

**Mixed-operand promotion (the `**`-only rule).** This is the SOLE
arithmetic op that promotes a mixed int/float pair. `synth_bin` special-
cases `BinOp::Pow` BEFORE the generic `unify(&lt, &rt)` — that unify would
REJECT `2 ** 3.0` (Int does not unify with Float; Cobrust has NO implicit
numeric coercion for `+`/`-`/`*`/`/`, §2.2). The MIR `lower_bin` Pow guard
casts an int operand i64→f64 (`CastKind::IntToFloat`, mirroring the coil
buffer-scalar mixed-arg path) so the f64 shim receives genuine f64 values,
NOT an i64 bit-pattern the f64 arg-coercion would mis-bitcast.

**Single codegen path.** The MIR `lower_bin` Pow guard retargets the
accepted shapes to the runtime shims BEFORE codegen's `BinOp::Pow` arm is
reached (sibling of the `str * int` / `str + str` retargets). Codegen's Pow
arm stays as a defensive `UnimplementedBinOp` for any shape that somehow
bypasses the guard. The Cranelift JIT (`cobrust-jit`) lowers only
`Add`/`Sub`/`Mul` and falls back to AOT for the Pow `Terminator::Call`
(`wave1: unsupported` → `UnsupportedMirFeature`); there is NO arithmetic
const-fold in HIR/MIR — so this single retarget closes the gap on every
path.

## Consequences

- **Positive**
  - `2 ** 10 == 1024` (an int), `2.0 ** 0.5 == 1.4142135623730951`,
    `2 ** 3.0 == 8.0` — `**` now matches CPython across int/float/mixed
    shapes. §2.5 *training-data-overlap* deficit closed for the operator.
  - The two cases that cannot be a clean int are surfaced HONESTLY: a
    negative LITERAL exponent at COMPILE time (§2.5-A, fix printed), a
    runtime negative exponent + overflow as a TRAP (§2.2, no silent wrong
    value).
- **Negative**
  - `int ** int` overflow + runtime-negative exponent are runtime traps,
    not compile errors (the type checker cannot see a non-literal value).
    Acceptable: a trap is not a wrong value, and the common literal case is
    caught at compile time.
  - No bignum: `2 ** 63` traps rather than promoting (CPython gives a
    bignum). A future bignum tier could revisit; out of F90's scope.
- **Neutral**
  - `**` is the only arithmetic operator that promotes a mixed int/float
    pair. This is a deliberate, documented exception to the "no implicit
    coercion" rule (§2.2), justified because the float exponent makes the
    result type unambiguous.

## Evidence

- Typing: `crates/cobrust-types/src/check.rs` `synth_bin` — `BinOp::Pow`
  block (mixed-promote to Float; `int ** int` negative-literal reject via
  `literal_int_value`).
- New diagnostic: `crates/cobrust-types/src/error.rs`
  `TypeError::NegativePowExponent` (+ `error_ux.rs`, `fix_safety.rs`,
  `cobrust-types-parity`, `cobrust-lsp/diagnostic.rs` arms).
- Lowering: `crates/cobrust-mir/src/lower.rs` `lower_bin` — `HirBinOp::Pow`
  guard (int → `__cobrust_ipow`; any-float → `__cobrust_math_pow` with
  i64→f64 casts on int operands).
- Runtime shim: `crates/cobrust-stdlib/src/math.rs` `__cobrust_ipow`
  (`checked_pow`; trap on overflow / negative exponent). Float path reuses
  the existing `__cobrust_math_pow`.
- Extern decl: `crates/cobrust-codegen/src/llvm_backend.rs`
  `declare_runtime_helpers` — `__cobrust_ipow` `(i64, i64) -> i64`.
- e2e oracle corpus: `crates/cobrust-cli/tests/power_e2e.rs` (8 tests:
  integer-pow matches CPython, `*` not confused with `**`, float-pow
  promotes (all four shapes), float-base negative-exponent OK, integer
  overflow traps exit 3, negative-literal rejects exit 2, runtime-negative
  traps exit 3, negative-base integer parity).
- Negative-test flip (F80-style follow-up): `crates/cobrust-types/tests/
  python_semantics_corpus.rs::h3_1_pow_codegen_compiles` (was
  `h3_1_pow_codegen_error`, which asserted the now-removed reject).
- Resolves: **ADR-0041 §H3** (the `**` portion). Sibling: **ADR-0099**
  (`//` floor div), **ADR-0097** (`str * int`), **ADR-0083** (bare libm
  `pow`). Mirrors **F79** (negative-literal scalar-index reject).
- Finding: `docs/agent/findings/f90-power-operator-unimplemented.md`
  (status → resolved).
