---
doc_kind: adr
adr_id: 0089
title: Type-preserving `abs(x)` (int->int / float->float) + Python-canonical 1-arg `range(stop)`
status: accepted
date: 2026-06-05
last_verified_commit: 3d13c2c
supersedes: []
superseded_by: []
---

# ADR-0089: Type-preserving `abs(x)` (int->int / float->float) + Python-canonical 1-arg `range(stop)`

## Context

Two of the most common builtins a Python-trained LLM reaches for were
broken on the natural type — both §2.5 (LLM-first) first-try misses, in
the spirit of ADR-0088 (`len(x)` over sized types):

1. **`abs(int)`** — `abs(-5)` raised `type mismatch: expected f64, found
   i64`. The bare `abs` is a PRELUDE stub `abs(x: f64) -> f64`
   (`crates/cobrust-frontend/src/prelude.rs`), so the generic
   stub-unify path unified the arg against `f64` and rejected every
   integer. Python's `abs` is **type-preserving**: `abs(-5) == 5` (an
   `int`), `abs(-5.0) == 5.0` (a `float`). The misleading "expected f64"
   diagnostic is itself a §2.5-B violation — it steered the LLM toward
   wrapping the literal as `abs(-5.0)` (changing the result type).

2. **`range(stop)`** — `range(5)` raised `wrong number of arguments:
   expected 2, got 1`. `range` is a real PRELUDE Cobrust function body
   `range(start: i64, stop: i64) -> list[i64]` (ADR-0050b) that
   materialises `[start, …, stop-1]`; the 2-arg form already drives every
   `for i in range(a, b):`. Python's 1-arg `range(stop) == range(0,
   stop)` simply was never wired.

### Root cause (verified)

- `abs`: identical shape to the ADR-0088 `len` root cause — a single
  narrow `Fn` PRELUDE stub cannot express "int OR float", and the generic
  `synth_call` arg-unify at the `Ty::Fn` arm rejects the non-`f64` arg.
- `range`: NOT a type-checker intrinsic at all — just an ordinary PRELUDE
  fn with arity 2. `range(5)` fails purely on `ArityMismatch`. The
  for-loop desugar (`crates/cobrust-mir/src/lower.rs`, `LoopKind::For`)
  lowers the iter expression `range(…)` to a plain `Call` to that
  2-param body, then index-iterates the returned `list[i64]`.

## Options considered

1. **Widen the PRELUDE stubs to generic/overloaded signatures.** Cobrust
   has no overloading and no numeric-trait surface; one `Fn` stub cannot
   express int|float. Rejected (same as ADR-0088 Option 1).

2. **Special-case the bare calls in the type-checker BEFORE the generic
   stub-unify** (mirrors ADR-0088 `try_synth_len_builtin` and the
   ADR-0064 polymorphic `print`). For `abs`: intercept, resolve the arg,
   return `Int` for an int / `Float` for a float (type-preserving); a
   non-numeric arg unifies against `f64` in-place (preserving the
   canonical `TypeMismatch`, **no new error variant**). For `range`:
   intercept the 1-arg form, return `list[i64]`, and inject `start = 0`
   at MIR-lowering time so the unchanged 2-param body runs. **Chosen.**

3. **Drop the f64-only `abs` / require `.abs()` method-form; keep range
   2-arg-only.** Violates §2.5 (maximize-overlap-with-training-data) —
   Python writes `abs(-5)` and `range(5)`. Rejected.

## Decision

Adopt option 2. Three new type-checker special-cases in
`crates/cobrust-types/src/check.rs`, each running in `synth_call` after
the method-call + `len` dispatch and BEFORE the generic
`synth_expr(callee)` / PRELUDE-stub-unify path:

### `abs(x)` — type-preserving (§3)

`try_synth_abs_builtin` fires for the bare PRELUDE `abs` (its `DefId`
registered in `poly_intrinsic_defs` at `prebind_item`, like `print`; a
user-defined `abs` shadows the def_id and is left to the generic path)
with exactly one positional argument. It resolves the arg (unwrapping one
`Ref` so `abs(&n)` works) and returns:

- **`Int` arg** → `Ty::Int`.
- **`Float` arg** → `Ty::Float`.
- **other / unresolved var** → `unify(f64, arg)` in-place, returns
  `Ty::Float`. For `abs("x")` this raises the canonical
  `TypeError::TypeMismatch { expected: f64, found: str }` — **the
  pre-ADR-0089 behaviour, reusing an existing variant** (no
  error-renderer cascade); for a bare `Ty::Var` it anchors to `f64`.

The PRELUDE `abs` stub is unchanged (the special-case intercepts first).
Distinct from `coil.abs` (the Buffer ufunc) and `math.fabs` (the `import
math` scalar module) — this is the bare scalar `abs(x)`.

### `range(stop)` — 1-arg form (§4)

`try_synth_range_builtin` fires for the bare PRELUDE `range` (its `DefId`
recorded in a **dedicated `range_def` slot** — NOT `poly_intrinsic_defs`,
which would widen the `list[i64]` return's elem to a fresh var and
de-anchor the 2-arg for-loop's `i64` loop var) with exactly one
positional argument. It unifies the arg against `i64` and returns
`list[i64]`. The 2-arg `range(a, b)` form returns `Ok(None)` and stays on
the generic stub path with its `list[i64]` anchored; a 3-arg
`range(a, b, c)` likewise defers, hitting the canonical `ArityMismatch`.

### Per-shape lowering (§5)

- **`abs` int dispatch** — the CLI intrinsic-rewrite
  (`crates/cobrust-cli/src/build/intrinsics.rs`, a NEW dedicated
  `Kind::MathAbs` arm split out of the shared f64-family arm) picks the
  runtime symbol from the arg's resolved type (mirroring `Kind::Print` /
  `Kind::LenPoly`): an `Int` arg → `__cobrust_int_abs` (`i64 -> i64`),
  else → `__cobrust_math_abs` (`f64 -> f64`, the historical path).
- **`abs` int return type** — the PRELUDE `abs` stub declares `-> f64`,
  but `lower_call` overrides the `_callret` destination type to `Ty::Int`
  when the single arg synthesises to `Int`. WITHOUT this the `_callret`
  alloca is a `double` while the int arm calls `__cobrust_int_abs`
  (i64 -> i64), corrupting `print(abs(-5))` (a `double` slot fed to
  `__cobrust_println_int`).
- **Unary-temp typing (the load-bearing sub-fix)** — `abs(-5)`'s arg
  `-5` is `ExprKind::Un { Neg, 5 }`, which lowered to a `Ty::None` `_un`
  temp. Both `synth_expr_ty` AND `lower_un` now type the unary result:
  `-x`/`+x` preserve the operand's numeric type, `~x` is `Int`, `not x`
  is `Bool`. Without it `abs(-5)` fell through to the f64 path and
  re-interpreted the i64 bits as a `double` (→ `NaN`). This is a general
  correctness improvement (a `Ty::None` arithmetic temp was a latent
  bug); all scalar unary results are Copy types, so typing them is
  strictly safer than `Ty::None`.
- **`range` start injection** — `lower_call` prepends
  `Operand::Constant(Constant::Int(0))` when the prelude callee is
  `range` and exactly one positional operand was lowered, so the
  unchanged 2-param body runs.

### Runtime shim (§5)

`__cobrust_int_abs(i64) -> i64` (`crates/cobrust-stdlib/src/math.rs`)
delegates to the pre-existing `abs_i64`, which saturates `i64::MIN` at
`i64::MAX` (Constitution §2.2 forbids silent overflow). Declared in
`crates/cobrust-codegen/src/llvm_backend.rs` alongside the
`__cobrust_math_*_int` shims (DISTINCT `(i64) -> i64` shape vs the
`(f64) -> i64` rounding shims). No new dependency, no new TypeError
variant.

## Consequences

- **Positive**
  - `abs(-5) == 5` (an int, usable in int arithmetic — `abs(-5) + 1 ==
    6`); `abs(-5.0) == 5.0` (float regression); both type-check + lower +
    run (§2.5 first-try wins).
  - `range(5)` / `range(n)` drive for-loops (== `range(0, stop)`); the
    2-arg `range(a, b)` is untouched.
  - The latent `Ty::None` unary-temp bug is closed — any future per-shape
    dispatch over a `-x` / `~x` operand now sees the correct type.
  - NO new error variant — `abs("x")` reuses `TypeMismatch`, 3-arg
    `range` reuses `ArityMismatch`, so the ADR-0088 error-renderer
    cascade (`error.rs` / `fix_safety` / `error_ux` / `cobrust-lsp` /
    `cobrust-types-cb` / `types-parity`) needed NO changes.
- **Negative**
  - Two tests that codified the now-fixed rejections were converted to
    acceptance: `ill_typed.rs::i58_for_range_called_with_one_arg` →
    `..._now_accepted`; `for_range_e2e.rs::f3r24_run_range_one_arg_rejected`
    → `..._now_accepted` (kept as behaviour-change markers). The 3-arg
    rejections (`i59`, `f3r25`) stay correct.
  - The `_un` temp type changed module-wide from `Ty::None` to the
    result type; validated non-regressive across the LC-100 stress corpus
    + f64/coil/math e2es (all scalar Copy types, no drop-schedule impact).
- **Neutral / unknown**
  - cobrust prints whole floats without the trailing `.0`
    (`print(5.0)` → `5`), so `abs(-5.0)` prints `5` (value 5.0; Python
    prints `5.0`). A pre-existing float-format difference, unrelated to
    this ADR.
  - 3-arg `range(a, b, step)` remains deferred (ADR-0050b) — out of
    scope here.

## Evidence

- Repro (pre-fix): `abs(-5)` → `TypeMismatch { expected: Float, actual:
  Int }`; `range(5)` → `ArityMismatch { expected: 2, actual: 1 }`.
- Post-fix e2e (`crates/cobrust-cli/tests/builtins_abs_range_e2e.rs`,
  REAL compile → link → spawn): `abs(-5)`→`5`, `abs(5)`→`5`,
  `abs(-5.0)`→`5`, `abs(0)`→`0`, `abs(-5)+1`→`6`, `abs(n)` var→`7`;
  `range(5)` sum→`10`, `range(0)`→`0`, `range(2,5)` sum→`9`, `range(n)`
  sum→`6`. 10/10 green.
- Type-checker corpus: `well_typed.rs` w221–w229 (abs int literal/param,
  float regression, float literal, borrowed int; range 1-arg, 1-arg zero,
  1-arg var, 2-arg regression) + `ill_typed.rs` i173–i174 (abs str →
  `TypeMismatch`; range 3-arg → `ArityMismatch`) + the converted i58.
  All green (`cobrust-types`: lib 155, well_typed 288 + 1 ignored,
  ill_typed 190 + 6 ignored).
- Regression (all green): LC-100 `intrinsics_input` (101) +
  `lc100_stress_e2e_b1/b2/b3/b4` (30/30/30/10) — range drives every
  for-loop; `for_range_e2e` (36); `math_e2e` (13, abs float),
  `f64_e2e` (33, `abs(-5.5)`), `coil_round_e2e` (8, `coil.abs`),
  `coil_arange_e2e` (7), `method_call_e2e` (5, `.abs()`/`.len()`
  method-form), codegen `llvm_wave3_fmt_iter_math_str` (14, `abs(-7.0)`).
- Differential oracle: python3.11 `abs(-5)`/`abs(5)`/`abs(-5.0)`/`abs(0)`
  = `5`/`5`/`5.0`/`0`; `sum(range(5))`/`sum(range(2,5))` = `10`/`9`.
