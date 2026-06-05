---
doc_kind: adr
adr_id: 0088
title: Python-canonical free-function `len(x)` over sized types (str | list | dict)
status: accepted
date: 2026-06-05
last_verified_commit: ea9ca8c
supersedes: []
superseded_by: []
---

# ADR-0088: Python-canonical free-function `len(x)` over sized types (str | list | dict)

## Context

The bare free-function `len(x)` is one of the most common operations a
Python program (and therefore a Python-trained LLM) writes — on strings
and lists *constantly*. Cobrust's §2.5 north star ("the language LLM
agents write correctly on the first try") makes this a first-class
surface.

Pre-ADR-0088, the free-function `len(x)` accepted **only `Dict`**:

- `len("abc")` → `type mismatch: expected Dict[?,?], found str`
- `len([1, 2, 3])` → `type mismatch: expected Dict[?,?], found List[...]`
- `len(d)` (dict) → type-checked (the only working path)

The method-form `s.len()` / `xs.len()` *did* work (ADR-0052d-prereq,
`check.rs:2195`/`2357`, 0-arg → `Int`), but that is the Rust spelling;
the Python `len(x)` free-function is what the LLM reaches for first.

### Root cause (verified)

The bare `len` is a PRELUDE intrinsic. Its stub (the literal string in
`crates/cobrust-frontend/src/prelude.rs::PRELUDE`) is:

```
fn len(d: dict[i64, i64]) -> i64:
    return 0
```

— a **dict-only** signature (Phase F.3 / ADR-0050d Decision 5 scope cap).
`len`'s `DefId` is registered in `poly_intrinsic_defs` via
`is_list_polymorphic_intrinsic_name` (`check.rs:~4161`). At a bare call,
`synth_call` widens the stub through `instantiate_list_polymorphic`
(`check.rs:3716`), whose `Ty::Dict(_, _) => Dict[fresh, fresh]` arm
(`check.rs:~3729`) yields `Dict[?, ?] -> i64`. The arg is then unified
against that param at the `Ty::Fn` arm —
`self.unify_call_arg(p, &at, a.span)?` (`check.rs:~3015`, with
`p = Dict[?, ?]`). `str` / `List` do not unify with `Dict`, so both are
rejected. The widening's `Ty::List(_) => List[fresh]` arm only helps when
the stub PARAM is a List — the `len` stub param is a `Dict`, so it never
fires for `len`.

The misleading "expected Dict" diagnostic is itself a §2.5-B error-UX
violation: it leaks the dict-only PRELUDE stub and steers the LLM toward
wrapping the argument in a dict.

## Options considered

1. **Widen the PRELUDE `len` stub to a generic / overloaded signature.**
   Cobrust has no overloading and no `Sized` trait surface yet; a single
   `Fn` stub cannot express "str | list | dict". Rejected — it would
   require a whole trait-bound mechanism for one builtin.

2. **Special-case the bare `len(x)` call in the type-checker BEFORE the
   generic PRELUDE-stub-unify.** Intercept the bare `len` call, synth +
   resolve the arg, accept any SIZED type (`Str` / `List(_)` /
   `Dict(_, _)`) returning `Ty::Int` *without* unifying the arg to
   `Dict`; a non-sized arg raises a clear §2.5-B error. Mirrors the
   existing `print(x)` polymorphic-dispatch precedent (ADR-0064 §3.2:
   `print` is registered polymorphic in the type-checker, and the CLI
   intrinsic-rewrite monomorphizes per arg shape at MIR time). **Chosen.**

3. **Drop the free-function `len` entirely; require `.len()`.** Violates
   §2.5 (maximize-overlap-with-training-data) — Python writes `len(x)`,
   not `x.len()`. Rejected.

## Decision

Adopt option 2. A new type-checker special-case
`try_synth_len_builtin` (`crates/cobrust-types/src/check.rs`) runs in
`synth_call` immediately after the method-call dispatch and BEFORE the
generic `synth_expr(callee)` / PRELUDE-stub-unify path. It fires only for
the bare name `len` whose `DefId` is in `poly_intrinsic_defs` (so a
user-defined fn named `len` is untouched) with exactly one positional
argument. It synthesises the arg, resolves it (`self.subst.apply`,
unwrapping one `Ref` so `len(&s)` works), and:

- **Sized arg** — `Str` | `List(_)` | `Dict(_, _)` → returns `Ty::Int`
  WITHOUT unifying the arg against `Dict`.
- **Non-sized arg** — `len(5)` / `len(3.0)` / `len(true)` → raises the
  new `TypeError::LenArgNotSized { actual, span, suggestion }`, whose
  §2.5-B `#[error(...)]` message NAMES the accepted sized-type set
  (`str` / `list[T]` / `dict[K, V]`) and does NOT say "expected Dict".

The PRELUDE `len` stub is **unchanged** (`prelude.rs` not touched) — the
special-case intercepts before the stub matters. The method-form
`s.len()` / `xs.len()` decision is **kept** (it is the Rust spelling, a
separate path via `try_synth_method_call`).

### Per-shape lowering (ADR-0088 §4)

The CLI intrinsic-rewrite (`crates/cobrust-cli/src/build/intrinsics.rs`,
`Kind::LenPoly` arm) now picks `len`'s runtime symbol from the
argument's resolved `LocalDecl.ty` (mirroring the `Kind::Print`
monomorphization). It resolves the arg's effective type (a
`Constant::Str` literal → `Str`; a `Place` local → `local_ty` lookup,
unwrapping one `Ref`) and dispatches:

| Arg shape   | Runtime symbol           | Notes |
|-------------|--------------------------|-------|
| `Str`       | `__cobrust_str_len_src`  | byte count; the SAME symbol the str method-form `s.len()` rewrites to (via `str_len`) — the two AGREE |
| `List[T]`   | `__cobrust_list_len`     | type-erased over `T` |
| `Dict[K,V]` | `__cobrust_dict_len`     | type-erased over `(K, V)`; the historical Decision-5 symbol; also the `_` fallback for an unresolved (dict-returning) call-return local |

`__cobrust_str_len_src` (`cobrust-stdlib/src/io.rs:524`) delegates to
`__cobrust_str_len` (`cobrust-stdlib/src/fmt.rs:247`), returning
`StringBuffer.bytes.len()` — the **byte** count. For ASCII this equals
the char count (`len("hello") == 5`). Matching the method-form keeps the
two `len` spellings consistent.

### Sized-type scope + deferrals

- **Shipped**: `Str`, `List`, `Dict` — the three types with a `len`
  runtime symbol AND a verified source-level construction path.
- **Deferred**: `Tuple` (no `len` runtime symbol — fixed-size, statically
  known) and `Set` (HAS `__cobrust_set_len` at
  `cobrust-stdlib/src/collections.rs:1265`, but no verified source-level
  set-construction path exists in the e2e harness yet; adding the
  type-checker arm without the lowering+e2e would be an F36
  fixture-vs-behaviour gap). A follow-up wires `Set`'s lowering + e2e and
  extends the special-case + the `LenArgNotSized` message.

## Consequences

- **Positive**
  - `len("abc")`, `len([1,2,3])`, `len(d)` all type-check + lower + run
    (§2.5 first-try win for the single most common Python op).
  - The non-sized error PRINTS THE FIX (§2.5-B) instead of the
    dict-leaking "expected Dict".
  - `len(str)` agrees byte-for-byte with the str method-form `s.len()`.
- **Negative**
  - A new `TypeError` variant (`LenArgNotSized`) forced match-arm updates
    across the workspace (`fix_safety.rs`, `cobrust-lsp/diagnostic.rs`,
    `cobrust-cli/error_ux.rs`, the self-hosting mirror
    `cobrust-types-parity` + `cobrust-types-cb`). This is the §2.5-A
    compile-time-catch working as designed — the compiler forced every
    consumer to handle the new case.
  - The bare `len(list_new(0))` (un-annotated empty list) now surfaces the
    pre-existing list-poly `AmbiguousType` instead of the old
    `expected Dict` rejection — neither ever worked; the fix is the
    natural `let xs: list[i64] = []` annotation.
- **Neutral / unknown**
  - The dict-len RUNTIME count (`len(dict)` returning 0 for a
    literal-initialized dict) is a SEPARATE pre-existing defect
    (`dict_e2e.rs::f3d08_dict_len_returns_count`, `#[ignore]`'d). This ADR
    does not touch it — the `len(dict)` TYPE-CHECK path (the regression
    guarantee) is preserved; the runtime fix is tracked under the
    dict-len `#[ignore]` queue.

## Evidence

- Repro (pre-fix): `len("hello")` →
  `TypeMismatch { expected: Dict(Var, Var), actual: Str }`;
  `len([1,2,3])` → `... actual: List(Int)`.
- Post-fix e2e (`crates/cobrust-cli/tests/len_polymorphic_e2e.rs`, REAL
  compile → link → spawn): `len("hello")` → `5`; `len([1,2,3])` → `3`;
  `len("")`/`len([])` → `0`; `len(5)` → exit 2 (`LenArgNotSized`, no
  "expected Dict"); runtime-str `len(s)` → byte count. 7/7 green.
- Type-checker corpus: `well_typed.rs` w215–w220 (str literal/param,
  list i64/str, dict regression, borrowed str) + `ill_typed.rs`
  i170–i172 (non-sized int/float reject + the §2.5-B no-"expected Dict"
  assertion). 9/9 green.
- Regression (all green): LC-100 `intrinsics_input` (101), pit
  `pit_string_refinement_e2e` (4, the `where len(self) <= n` path),
  `method_call_e2e` (5, the `.len()` method-form), `list_str_e2e` (31),
  `dict_e2e` (5 + 24 ignored).
- Self-hosting parity: `cobrust-types-cb` byte-Display parity test
  `test_display_len_arg_not_sized` + the canonicalize/variant-name/
  fix-safety mirror arms; `cobrust-types-parity` clippy + tests green.
