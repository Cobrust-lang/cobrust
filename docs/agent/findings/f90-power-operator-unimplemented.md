---
finding_id: F90
title: '`**` POWER operator REJECTS at codegen ("unimplemented") — a ubiquitous Python operator the LLM writes constantly (§2.5 training-data-overlap gap)'
date: 2026-06-14
status: resolved
resolved_by: ADR-0102 (2026-06-14)
severity: major
discovered_by: §2.5 LLM-first operator-coverage audit (2026-06-14, F88/F89 sibling)
relates_to: ["claude.md:§2.5", "claude.md:§2.2", "adr-0041:§H3"]
---

# F90 — `**` power operator unimplemented

## What (verified at HEAD 66854e16)

`2 ** 3` (and every `**`) REJECTED at codegen with
`CodegenError::UnimplementedBinOp { op: "**" }` — the honest ADR-0041 §H3
"deferred" surface. The operator parsed and TYPE-checked (it sat in the
arithmetic accept-set of `synth_bin`), but codegen's `(BinOp::Pow, _)` arm
returned the unimplemented error, so `cobrust build` failed.

This was an ADDITIVE gap (a CLEAN reject, NOT a silent miscompile — contrast
F86's `//` truncation): the program did not compile, so no wrong value was
ever produced. The cost was purely first-try failure.

## Why it matters (§2.5 LLM-first)

`**` is one of the most common Python operators an LLM agent writes —
`2 ** n`, `x ** 2`, `base ** exp`, `(hi - lo) ** 0.5`. Its absence is a
direct hit to §2.5's *Maximize-overlap-with-training-data*: the LLM writes
`**` from its Python priors and the build rejects it. A high-frequency
operator gap is worse than a rare one.

## The load-bearing design problem

Python `**`'s result type depends on the exponent SIGN at RUNTIME:

| expr | Python result | type |
|---|---|---|
| `2 ** 3` | `8` | int |
| `2 ** -1` | `0.5` | float |

A static type system CANNOT make `int ** int` be both `int` and `float`.
The fix must PIN one typed result per operand shape and handle the
divergent cases explicitly.

## Resolution (ADR-0102)

Typed result by operand type:

- `int ** int -> int` (i64), via `__cobrust_ipow` (`checked_pow`).
  - A NEGATIVE-LITERAL exponent (`2 ** -1`) is a COMPILE-TIME reject
    (§2.5-A — mirrors F79's negative-literal scalar-index reject), exit 2,
    with a §2.5-B fix-printing diagnostic ("use a float base").
  - Integer OVERFLOW (`2 ** 63`) TRAPS (exit 3) — no silent wrap (§2.2).
  - A runtime-DYNAMIC negative exponent TRAPS (exit 3) — the type checker
    cannot sign-check a non-literal.
- ANY float operand `-> f64` (promote), via libm `pow`
  (`__cobrust_math_pow`). `**` is the ONE arithmetic op that promotes a
  mixed int/float pair (the float exponent makes the result unambiguous).
- CPython identities: `base ** 0 == 1` (incl. `0 ** 0 == 1`),
  `base ** 1 == base`.

The MIR `lower_bin` Pow guard retargets the accepted shapes to the runtime
shims BEFORE codegen's Pow arm (sibling of the `str * int` retarget), so
the single retarget closes every path (the JIT falls back to AOT; there is
no arithmetic const-fold).

## Evidence

- e2e: `crates/cobrust-cli/tests/power_e2e.rs` (8 tests, CPython oracle).
- Typing: `crates/cobrust-types/src/check.rs` `synth_bin` Pow block +
  `TypeError::NegativePowExponent`.
- Lowering: `crates/cobrust-mir/src/lower.rs` `lower_bin` Pow guard.
- Runtime: `crates/cobrust-stdlib/src/math.rs` `__cobrust_ipow`.
- Negative-test flip (F80-style): `crates/cobrust-types/tests/
  python_semantics_corpus.rs::h3_1_pow_codegen_compiles`.
- Sibling findings: F86 (`//` floor div), F78 (str slicing), F79
  (negative-literal index reject — the reject pattern this mirrors).
