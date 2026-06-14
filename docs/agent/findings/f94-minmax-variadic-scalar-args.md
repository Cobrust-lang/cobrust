---
finding_id: F94
title: '`max(a, b)` / `min(a, b, c)` (VARIADIC scalar args) REJECT — only the 1-arg list form works; the ubiquitous `max(a, b)` idiom fails first-try (§2.5 training-data-overlap gap)'
date: 2026-06-15
status: resolved
resolved_by: ADR-0107 (2026-06-15)
severity: major
discovered_by: §2.5 LLM-first builtin-coverage audit (2026-06-15, F90/F92/F93 sibling)
relates_to: ["claude.md:§2.5", "claude.md:§2.2", "adr-0090"]
---

# F94 — `min`/`max` variadic scalar args reject

## What (verified at HEAD f5f8735d)

`max(3, 5)` / `min(3, 5, 1)` (the VARIADIC scalar form) REJECTED at
type-check with `ArityMismatch` ("wrong number of arguments: expected 1").
Only the 1-arg LIST form worked: `max([3, 1, 5]) == 5` (ADR-0090). Python
supports BOTH the iterable form (`max([3,1,5])`) AND the variadic-scalar
form (`max(3, 5)`).

This was an ADDITIVE gap (a CLEAN reject, NOT a silent miscompile): the
program did not compile, so no wrong value was ever produced. The cost was
purely first-try failure.

## Why it matters (§2.5 LLM-first)

`max(a, b)` / `min(a, b)` is one of the most common Python idioms an LLM
agent writes — clamping (`max(lo, min(hi, x))`), running maxima
(`best = max(best, candidate)`), pairwise minima. Its absence is a direct
hit to §2.5's *Maximize-overlap-with-training-data*: the LLM writes
`max(a, b)` from its Python priors and the build rejects it. A
high-frequency builtin-shape gap is worse than a rare one.

## The load-bearing design problem

`min`/`max` are OVERLOADED in Python:

| call | meaning |
|---|---|
| `max([3, 1, 5])` | reduce the single iterable arg |
| `max(3, 5)` | the largest of the >= 2 scalar args |
| `max(5)` | TypeError — an int is not iterable |

The fix must add the >=2-arg scalar form WITHOUT breaking the 1-arg list
form, and decide the mixed int/float policy (`max(1, 2.0)`).

## Resolution (ADR-0107)

- **Typing** (`try_synth_reduce_builtin`): a `>= 2`-positional-arg call to
  `min`/`max` (NOT `sum` — Python's `sum`'s 2nd positional is `start`)
  takes the VARIADIC arm. Each arg must be `Int`/`Float`; any `Float` arg
  PROMOTES the whole call to `Float`, else `Int`. The 1-arg list form is
  unchanged. A SINGLE non-list arg (`max(5)`) falls through to the list
  form's `NotIterable` reject (Python parity). No new `TypeError` variant.
- **Lowering** (`lower_call`): the variadic form MATERIALISES a `list[T]`
  temp from the N scalar operands and REUSES the proven ADR-0090
  list-consume path (`__cobrust_{min,max}_{int,float}`). When the call
  promotes to `Float`, each `Int` operand is cast i64→f64
  (`CastKind::IntToFloat`) FIRST so the homogeneous f64-bit list matches
  the `*_float` shim. The scalars are Copy (no element-drop); the temp list
  drops once.
- **Mixed int/float** = PROMOTE to `Float` (consistent with Cobrust's
  existing int+float arithmetic promotion, NOT a silent value coercion —
  the cast is explicit). `max(1, 2.0)` is the float value `2.0` (prints
  `2`, the documented whole-float repr).

## Evidence

- e2e: `crates/cobrust-cli/tests/minmax_variadic_e2e.rs` (13 tests, CPython
  oracle). Regression: `list_reduce_e2e.rs` (14 tests, 1-arg list form).
- Typing: `crates/cobrust-types/src/check.rs` `try_synth_reduce_builtin`
  (variadic arm).
- Lowering: `crates/cobrust-mir/src/lower.rs` `lower_call` (variadic
  return-type override + temp-list build).
- Sibling findings: F90 (`**` power), F92 (`str` ordering), F93 (ternary) —
  all §2.5 additive-gap closures. Builds on ADR-0090 (the 1-arg list
  reducers).
