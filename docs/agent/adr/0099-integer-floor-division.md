---
doc_kind: adr
adr_id: 0099
title: '`//` integer floor division (floor toward -∞, not truncate) — ADR-0041 §H1 sibling'
status: accepted
date: 2026-06-14
last_verified_commit: ca68500
supersedes: []
superseded_by: []
---

# ADR-0099: `//` integer floor division (floor toward -∞)

## Context

Finding **F86** (`docs/agent/findings/f86-floor-division-truncates-on-negatives.md`)
identified that integer `//` (FloorDiv) SILENTLY TRUNCATED toward zero on
negative operands instead of FLOORING toward -∞:

| expr | pre-F86 (trunc) | CPython 3 (floor) | |
|---|---|---|---|
| `-7 // 2` | -3 | -4 | ✗ |
| `7 // -2` | -3 | -4 | ✗ |
| `-7 // 3` | -2 | -3 | ✗ |

Root cause: in `cobrust-codegen/src/llvm_backend.rs`, `FloorDiv` SHARED the
truncating `(BinOp::Div | BinOp::FloorDiv, false)` arm, which emits a plain
LLVM `build_int_signed_div` (`sdiv`, truncation toward zero).

This is a §2.2 SILENT MISCOMPILE (clean compile, wrong value, in a common op
— hashing, grid/index math, time arithmetic) AND it broke the div/mod
invariant `(a // b) * b + (a % b) == a`: `%` ALREADY floored (Python
floor-mod, **ADR-0041 §H1**), so the two operators were INCONSISTENT —
`(-7 // 2)*2 + (-7 % 2) = (-3)*2 + 1 = -5 != -7` (CPython: `(-4)*2 + 1 = -7`).

This ADR is the direct SIBLING of ADR-0041 §H1 (`%` floor-mod): the same
trunc→floor correction, applied symmetrically to the quotient.

### `/` vs `//` semantics in Cobrust (DOCUMENTED here)

Cobrust's type checker (`cobrust-types/src/check.rs` `synth_bin`) resolves
BOTH `Div` and `FloorDiv` to the operand type: `int / int -> int`,
`int // int -> int`. Therefore `/` on integers is **C-like TRUNCATING
integer division** (`7 / 2 == 3`, `-7 / 2 == -3`), NOT Python true/float
division. Only `//` (FloorDiv) FLOORS. This matches the pre-existing
Cobrust behavior pinned by the codegen corpus (`7 / 2 == 3`) and the
"`f64 → i64` truncates toward zero (C semantics, not floor)" rule already
documented in `docs/human/*/getting-started.md`. F86 fixes ONLY `//`; `/`
is deliberately left truncating.

## Options considered

1. **Floor only `//` (FloorDiv); leave `/` truncating.** Split FloorDiv
   out of the shared int arm; emit `sdiv` + the trunc→floor correction.
   Pro: restores the div/mod invariant; matches CPython `//`; `/` stays
   the documented C-truncating int division (no churn to existing tests
   `7 / 2 == 3`). Con: `/` and `//` diverge on negatives — but that IS
   Python, and is the point.
2. **Make `/` true/float division (Python `/`).** `int / int -> float`.
   Pro: full Python parity for `/`. Con: a large semantic + type-checker
   change (the result type of `/` flips to `Ty::Float`), breaks every
   existing `int / int` user (LC-100 corpus, codegen corpus `7 / 2 == 3`),
   and is OUT OF SCOPE for F86 (a separate, larger decision). Punts the
   actual bug.
3. **Document the divergence, do not fix.** Mark `//` `@py_compat(none)`.
   Con: permanent silent-miscompile asterisk on a common op; leaves the
   div/mod invariant broken. Non-compliant with §2.2.

## Decision

**Option 1.** Split `FloorDiv` out of the shared `(Div | FloorDiv, false)`
integer arm in `llvm_backend.rs`. For integer `FloorDiv` emit the
FLOOR-adjusted quotient:

```
q   = sdiv(a, b)
rem = srem(a, b)
need_adjust = (rem != 0) && ((a ^ b) < 0)   // remainder non-zero AND signs differ
result = select(need_adjust, q - 1, q)       // branchless trunc→floor correction
```

This is the symmetric twin of ADR-0041 §H1's `%` adjustment (which adds `b`
to `srem` under the SAME `(rem != 0) && (signs differ)` condition), so the
invariant `(a // b) * b + (a % b) == a` holds for every sign quadrant. The
float `//` arm is fixed in parallel: `floor(a / b)` via the same
trunc-then-adjust identity the §H1 float-mod arm already relies on. `/`
(Div) stays truncating. Division-by-zero is UNCHANGED — the MIR-level
`Assert(rhs != 0)` guard (`cobrust-mir/src/lower.rs`) still traps; the floor
adjustment runs only on the success path.

There is exactly ONE integer-division codegen path (the LLVM backend); the
Cranelift JIT (`cobrust-jit`) lowers only `Add`/`Sub`/`Mul` and never sees
`FloorDiv`, and there is NO constant-fold of arithmetic in HIR/MIR — so this
single split closes the bug on every path.

## Consequences

- **Positive**
  - `-7 // 2 == -4`, `7 // -2 == -4` etc. — matches CPython `//` (§2.2
    silent miscompile closed).
  - The div/mod invariant `(a // b) * b + (a % b) == a` holds for all
    sign quadrants — `//` and `%` are now CONSISTENT.
  - float `//` floors too (`-7.0 // 2.0 == -4.0`).
  - §2.5 LLM-first: an LLM writes `(hi + lo) // 2` and negative-index
    floor math expecting Python semantics; it now gets them.
- **Negative**
  - `/` and `//` diverge on negatives (`-7 / 2 == -3` vs `-7 // 2 == -4`).
    This is intentional and Python-faithful, documented in
    getting-started.
  - The integer `//` arm is ~4 extra IR ops (sdiv + srem + xor + icmp +
    and + sub + select) vs a bare `sdiv`. Folds away when `b > 0` and the
    sign is statically known; negligible.
- **Neutral / unknown**
  - Making `/` itself true/float division (Option 2) remains a future,
    separate decision — out of F86's scope.

## Evidence

- Fix: `crates/cobrust-codegen/src/llvm_backend.rs` — `(BinOp::FloorDiv,
  false)` and `(BinOp::FloorDiv, true)` arms split out of the shared
  `Div | FloorDiv` arms.
- `/` semantics confirmation: `crates/cobrust-types/src/check.rs`
  `synth_bin` (`Div`/`FloorDiv` both resolve to the operand type).
- Div-by-zero guard unchanged: `crates/cobrust-mir/src/lower.rs:3455`
  (`needs_div_assert` covers `Div | FloorDiv | Mod`).
- e2e oracle corpus: `crates/cobrust-cli/tests/floor_div_e2e.rs` (7 tests:
  negative-quotient floors, positive/exact unchanged, `%` unchanged, the
  div/mod invariant, `/` stays truncating, float `//` floors, div-by-zero
  traps) — all assert byte-identical to CPython 3.11.
- codegen compile-OK fixture: `crates/cobrust-codegen/tests/
  codegen_diff_corpus.rs::llvm_operand_10b_binop_floordiv_i64`.
- Sibling: **ADR-0041 §H1** (`%` floor-mod) — this ADR applies the
  symmetric correction to `//`.
- Finding: `docs/agent/findings/f86-floor-division-truncates-on-negatives.md`
  (status → resolved).
