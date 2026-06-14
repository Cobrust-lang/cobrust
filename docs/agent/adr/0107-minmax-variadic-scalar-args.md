---
doc_kind: adr
adr_id: 0107
title: '`min`/`max` VARIADIC scalar args — `max(a, b)` / `min(a, b, c)` (>= 2 args), mixed int/float PROMOTES to float'
status: accepted
date: 2026-06-15
last_verified_commit: f5f8735d
supersedes: []
superseded_by: []
---

# ADR-0107: `min`/`max` variadic scalar args (>= 2), mixed int/float promotes

## Context

Finding **F94** (§2.5 LLM-first): `max(3, 5)` / `min(3, 5, 1)` — the
VARIADIC scalar form — REJECTED at type-check with `ArityMismatch` ("wrong
number of arguments: expected 1"). Only the 1-arg LIST form worked
(`max([3, 1, 5]) == 5`, ADR-0090). Python supports BOTH the iterable form
AND the >=2-arg scalar form. `max(a, b)` / `min(a, b)` is one of the most
common Python idioms an LLM writes (clamping, running maxima), so its
absence was a constant first-try failure — directly against §2.5's
*Maximize-overlap-with-training-data*. This was an ADDITIVE gap (a clean
reject, NOT a silent miscompile), so the fix simply adds the form.

`min`/`max` are OVERLOADED in Python:

| call | meaning |
|---|---|
| `max([3, 1, 5])` | reduce the single iterable arg (ADR-0090) |
| `max(3, 5)` | the largest of the >= 2 scalar args (THIS ADR) |
| `max(5)` | `TypeError` — an int is not iterable |

The load-bearing design choice is the **mixed int/float** policy
(`max(1, 2.0)`): promote to float, or reject for strictness.

## Options considered

1. **Add the >=2-arg scalar form; mixed int/float PROMOTES to `Float`
   (the `Int` operands cast i64→f64 explicitly). Keep the 1-arg list form.
   `min`/`max` only — NOT `sum`. A single non-list arg stays a reject.**
   - Pro: matches CPython exactly (`max(1, 2.0) == 2.0`, a float). Promotion
     is CONSISTENT with Cobrust's existing int+float arithmetic promotion
     (ADR-0102's `**`, the coil buffer-scalar mixed path) — it is NOT a new
     silent-coercion exception to §2.2 because the cast is EXPLICIT
     (`CastKind::IntToFloat` per operand), not a value reinterpretation. The
     lowering REUSES the proven ADR-0090 list-consume shims via a temp list,
     adding zero new runtime symbols. §2.5 *training-data-overlap* closed.
   - Con: a mixed call's result type is `Float` even when the chosen value
     came from an int arg (`max(5, 2.0) == 5.0`). Acceptable — this is
     exactly CPython's behaviour, the surface the LLM expects.

2. **Add the form but REJECT mixed int/float (`max(1, 2.0)` is a compile
   error — all args must be the same scalar type).**
   - Pro: maximally strict (§2.2); no promotion surface to reason about.
   - Con: diverges from CPython (which promotes), so the LLM writing the
     ubiquitous `max(count, ratio * n)` (an int and a float) hits a reject
     it does not expect from its Python priors — a §2.5 deficit. Cobrust
     ALREADY promotes mixed int/float for `**` (ADR-0102); rejecting here
     would be an inconsistent island. Rejected.

3. **Keep rejecting the variadic form (status quo, ADR-0090 "Deferred").**
   - Con: permanent first-try failure on a ubiquitous idiom; the §2.5
     deficit this finding exists to close. Rejected.

## Decision

**Option 1.** Add the variadic scalar form to `min`/`max`:

| call shape | result | lowering |
|---|---|---|
| `min`/`max` 1-arg `list[T]` | `T` (ADR-0090, unchanged) | `__cobrust_{min,max}_{int,float}` over the arg list |
| `min`/`max` `>= 2` all-int args | `int` (i64) | temp `list[int]` → `__cobrust_{min,max}_int` |
| `min`/`max` `>= 2` args, any float | `float` (f64) — PROMOTE | temp `list[float]` (int args cast i64→f64) → `__cobrust_{min,max}_float` |
| `min`/`max` single non-list arg (`max(5)`) | COMPILE error (`NotIterable`) | — (Python: `max(5)` is a `TypeError`) |
| `sum` `>= 2` args | unchanged generic reject | `sum`'s 2nd positional is `start`, NOT an element |

**Typing (`try_synth_reduce_builtin`).** A `>= 2`-positional-arg call whose
name is `min`/`max` takes the variadic arm BEFORE the 1-arg list
destructure. Each arg is synth'd; any `Float` arg promotes the whole call
to `Float`, else `Int`. The args are NOT cross-unified — Cobrust has no
implicit `Int ≡ Float` unification (§2.2), so `unify`-ing the args together
would REJECT `max(1, 2.0)`. Instead each arg is VALIDATED numeric; a
non-numeric arg (`max("a", "b")`) unifies against the target numeric type,
raising the canonical `TypeMismatch` (no new `TypeError` variant). A SINGLE
non-list arg is NOT this arm — it falls through to the 1-arg list form's
`NotIterable` reject (Python parity).

**Lowering (`lower_call`).** Two pieces, both keyed on `min`/`max` with
`>= 2` lowered operands:

- the `callee_return_ty` override re-pins the `_callret` alloca to the
  promoted type (`Float` iff any arg is `Float`, else `Int`), the single
  source of truth the intrinsic-rewrite shim pick reads.
- after `arg_ops` is built, MATERIALISE a `list[T]` temp
  (`Rvalue::Aggregate(AggregateKind::List, …)`) and REPLACE `arg_ops` with
  the single `Operand::Move` list operand, REUSING the ADR-0090
  list-consume path. When the call promotes to `Float`, each `Int` operand
  is cast i64→f64 (`CastKind::IntToFloat`) FIRST so the homogeneous f64-bit
  list matches the `*_float` shim (NOT an f64-arg bitcast of i64 bits — the
  ADR-0089 abs-miscompile lesson). The scalars are `Int`/`Float` (Copy) —
  no element-drop concern; the temp list drops once in its own scope.

The intrinsic-rewrite pass (`Kind::Min`/`Max`) is UNCHANGED: the variadic
form arrives there already shaped as the 1-arg list call (`args.len() == 1`).

**User-shadow gate (the reducer-SHAPE check).** A USER who defines
`fn min(a: f64, b: f64)` (scalar params) is NOT the list-reducer builtin and
MUST keep their own strict signature — without a gate the variadic arm would
intercept their `min(1.0, 2)` and promote it instead of rejecting the i64/f64
mismatch against the user's `(f64, f64)` declaration. So BOTH layers gate on
the reducer SHAPE, not just the name `min`/`max`/`sum`:

- typing: `reduce_defs` registration (`check.rs` `prebind_item`) records a
  `min`/`max`/`sum` def ONLY when its first positional param is a `list`
  (the PRELUDE stubs + test `REDUCE_STUB`s all are). A scalar-param shadow is
  never registered, so `try_synth_reduce_builtin` returns `Ok(None)` and the
  user's fn signature drives the call (the canonical `i80` reject for
  `min(1.0, 2)` is preserved).
- lowering: `lower_call` computes `callee_is_list_reducer` by looking up the
  callee `DefId`'s resolved `Ty::Fn` and checking the first positional is a
  `list`; the reducer return-type override AND the variadic temp-list build
  are gated on it. A valid user `min(1.0, 2.0)` runs the user's body, NOT the
  reducer.

## Consequences

- **Positive**
  - `max(3, 5) == 5`, `min(3, 5, 1) == 1`, `max(2, 8, 4, 1) == 8`,
    `max(1.5, 2.5) == 2.5`, `max(1, 2.0) == 2.0` — `min`/`max` now match
    CPython across the variadic int/float/mixed shapes. §2.5
    *training-data-overlap* deficit closed for the idiom.
  - ZERO new runtime symbols: the temp list bridges to the proven ADR-0090
    shims. ZERO new `TypeError` variants: reuses `NotIterable` (single
    non-list arg) + `TypeMismatch` (non-numeric arg) — no error cascade.
  - The 1-arg list form (`max([list])`) is untouched (`args.len() == 1`
    skips the temp-list build); `list_reduce_e2e` stays green.
- **Negative**
  - A mixed call's result is always `Float` (`max(5, 2.0) == 5.0`), even
    when the chosen value came from an int arg. This is CPython's behaviour
    (the surface the LLM expects), so it is the intended trade, not a defect.
  - `sum` does NOT get the variadic form (Python compatibility — `sum`'s
    2nd positional is `start`). A future ADR could add `sum(iter, start)`.
- **Neutral**
  - Mixed int/float promotion here is consistent with the `**` operator
    (ADR-0102) and the coil buffer-scalar path — a documented, explicit
    promotion (per-operand `IntToFloat` cast), NOT a silent value coercion
    forbidden by §2.2.

## Evidence

- Typing: `crates/cobrust-types/src/check.rs` `try_synth_reduce_builtin`
  (the `args.len() >= 2 && min|max` variadic arm; validate-numeric +
  promote, no cross-unify, no new variant) + the `reduce_defs` registration
  reducer-SHAPE gate in `prebind_item` (first positional is a `list`).
- Lowering: `crates/cobrust-mir/src/lower.rs` `lower_call` — the
  `callee_is_list_reducer` shape gate (looks up the callee `Ty::Fn`'s first
  positional) + the `callee_return_ty` variadic override + the
  temp-`list[T]`-build (`Rvalue::Aggregate(AggregateKind::List)`, with
  per-`Int`-operand `CastKind::IntToFloat` on float-promoted calls).
- Shim reuse (unchanged): `crates/cobrust-cli/src/build/intrinsics.rs`
  `Kind::Min`/`Max` → `__cobrust_{min,max}_{int,float}`.
- e2e oracle corpus: `crates/cobrust-cli/tests/minmax_variadic_e2e.rs`
  (13 tests: 2/3/4-arg int, negative, two/three-arg float, computed vars +
  int arithmetic, mixed int/float promote, list+variadic coexistence,
  nested variadic, + 3 clean-exit-2 rejects: single-int `max(5)` /
  `min(7)`, non-numeric `max("a","b")`).
- Regression: `crates/cobrust-cli/tests/list_reduce_e2e.rs` (14 tests, the
  1-arg list form, all green).
- Builds on: **ADR-0090** (the 1-arg list reducers `min`/`max`/`sum`).
  Sibling §2.5 closures: **ADR-0102** (`**` power, the mixed-promote
  precedent), **ADR-0105** (ternary), **ADR-0104** (`str` ordering).
- Finding: `docs/agent/findings/f94-minmax-variadic-scalar-args.md`
  (status → resolved).
