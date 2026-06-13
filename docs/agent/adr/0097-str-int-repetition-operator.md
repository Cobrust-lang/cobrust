---
doc_kind: adr
adr_id: 0097
title: "`str * int` / `int * str` REPETITION operator — `\"ab\" * 3 == \"ababab\"` (§2.5 additive idiom)"
status: accepted
date: 2026-06-14
last_verified_commit: 3e973f7
supersedes: []
superseded_by: []
relates_to: [adr:0078, adr:0094, "claude.md:§2.5"]
---

# ADR-0097: `str * int` / `int * str` REPETITION operator

## Context

`"sep" * n` (string repetition) is a Python idiom an LLM writes constantly
— dividers, padding, fixed-width fills. Before this ADR, `"ab" * 3` was a
CLEAN type-mismatch REJECT:

```
error[Type]: type mismatch: expected str, found i64
```

This is purely ADDITIVE — NOT a §2.2 silent-miscompile closure (contrast
ADR-0094's `str` index, which returned the wrong value at exit 0). The
operator simply did not exist; `str` was already in the post-`unify`
arithmetic accept-set, but ONLY for the same-type `str + str` concat path,
so `str * int` (different operand types) failed at `unify` before reaching
any `Str` arm.

The §2.5 win (Maximize-training-data-overlap): make the common idiom WORK
on the first try. The LLM writes `"=" * 40` and gets `"========..."`, not a
type error it must work around.

CPython semantics this implements (`str.__mul__` / `int.__mul__`):

- `"ab" * 3 == "ababab"`; `3 * "ab" == "ababab"` (SYMMETRIC — both orders).
- `"x" * 0 == ""`; `"x" * 1 == "x"`; `"x" * -2 == ""` (a non-positive count
  → the empty str, NEVER a trap).
- codepoint-faithful: `"é" * 2 == "éé"` (repetition concatenates whole
  strings, so a boundary never splits a multi-byte UTF-8 codepoint).

## Options considered

1. **A dedicated `BinOp::Mul` `(Str, Int)` / `(Int, Str)` arm + a fresh
   `__cobrust_str_repeat` runtime shim** (chosen). A single binary operator
   + one runtime fn; mirrors the established `str + str` →
   `__cobrust_str_concat` pipeline (ADR-0078) end-to-end (check → lower →
   stdlib → codegen extern). No core-type restructuring.
2. **A `.repeat(n)` method** (rejected). Python writes `s * n` not
   `s.repeat(n)`; a method form has LOWER training-data overlap (§2.5-D
   names method-sugar for the cases Python USES methods; repetition is an
   operator in Python). Would also not cover the `int * str` order.
3. **Desugar `s * n` to a loop of `s + s + …`** (rejected). O(n) concat
   allocations + O(n) intermediate `str` buffers vs. one capacity-reserved
   `str::repeat`; also re-derives the drop schedule per intermediate. The
   runtime shim is one allocation + total.

## Decision

Add `str` repetition as a `BinOp::Mul` special-case that normalizes BOTH
operand orders to `(str-receiver, int-count)` and lowers to
`__cobrust_str_repeat(s: *mut Str, n: i64) -> *mut Str`. Four pieces,
mirroring the ADR-0078 `str + str` concat pipeline:

- **check.rs `synth_bin` Mul arm**: BEFORE `unify` (a `Str` never unifies
  with an `Int`), `(Ty::Str, Ty::Int) | (Ty::Int, Ty::Str)` → `Ty::Str`.
  Any OTHER pairing (`Str * Str`, `Str * Float`, …) falls through to
  `unify` and rejects as before.
- **lower.rs `lower_bin` Mul guard**: `lhs_is_str ^ rhs_is_str` → normalize
  to `(s_op, n_op)`, evaluating LHS-then-RHS to preserve source-order side
  effects regardless of which side is the `str`. The `str` receiver is
  BORROWED (`upgrade_move_to_copy_handle` — `__cobrust_str_repeat` reads
  but does not consume `s`, so the source `str` survives + drops once); the
  `int` count is a Copy scalar lowered directly. The result is a FRESH
  owned `str` (`Move`-out, dropped once).
- **cobrust-stdlib `__cobrust_str_repeat`**: `str_buf_as_str_local` read +
  `str::repeat` (single capacity-reserved allocation) + `alloc_str_buffer_
  local` mint; `n <= 0 → ""`. The `__cobrust_str_slice` mint discipline.
- **codegen extern decl**: `(ptr, i64) -> ptr` + param-count 2, the
  `__cobrust_str_slice` mirror.

The numeric `Mul` behavior is untouched (the guard fires only when exactly
one operand is `Str`).

## Consequences

- **Positive**
  - `"sep" * n` works on the first try (§2.5 training-data overlap); both
    `s * n` and `n * s` orders accepted (Python symmetry).
  - Zero new core types; reuses the `str + str` borrow + fresh-mint
    drop discipline verbatim — no new ownership reasoning.
  - One allocation per repetition (`str::repeat`), no intermediate buffers.
- **Negative**
  - One more `BinOp::Mul` special-case in both check.rs and lower.rs (the
    arm runs before `unify`, like the coil-Buffer scalar guards).
- **Neutral / unknown**
  - A LOOP minting `"ab" * 3` per iteration is the F82 loop-leak debt —
    NOT closed here. This increment guarantees single-mint drop-balance
    only (`str_mul_e2e_06`).

## Evidence

- `crates/cobrust-cli/tests/str_mul_e2e.rs` — 6 e2e tests, each asserting
  stdout byte-identical to the CPython-3 oracle: basic + symmetric,
  zero/one/negative count, unicode codepoint-faithful, computed count,
  result-is-usable (`len`), single-mint drop-balance.
- Local verify: `cargo test --workspace --locked` green except the known
  `libcoil.a` parallel-build flake (`coil_astype_e2e` 2 reds under
  contention, 7/7 green isolated — not a touched-crate regression).
