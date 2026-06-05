---
doc_kind: adr
adr_id: 0083
title: math — the first core scalar stdlib module (import math) via bare-libm-extern lowering
status: accepted
date: 2026-06-05
last_verified_commit: 664a028
supersedes: []
superseded_by: []
---

# ADR-0083: `math` — the first core scalar stdlib module

## Context

The ecosystem surface wires 129 `coil` (numpy/array) ops plus the
app/network modules (`pit`, `strike`, `den`, `hood`, `dora`, `redis`,
`fang`, `nest`, `scale`, `molt`), but ZERO **core** Python stdlib:
`json` / `re` / `math` / `datetime` are all absent. `import math` is THE
most-used scalar-numeric Python module — LLMs and the translation
pipeline (CLAUDE.md §2.5 maximize-overlap-with-training-data) reach for
`math.sqrt` / `math.pi` constantly.

`math` is **distinct** from `coil`:

- `coil.sqrt(a)` is an elementwise **Buffer** ufunc — `Buffer -> Buffer`,
  lowering to `__cobrust_coil_sqrt` (a Rust kernel over a `*mut Buffer`).
- `math.sqrt(x)` is a **scalar** `f64 -> f64` op.

They never collide: a `coil.sqrt` callee resolves against the `coil`
import alias + the `COIL_BUFFER_ADT` argument; a `math.sqrt` callee
resolves against the `math` import alias + an `f64` argument. Different
module, different signature, different runtime symbol.

There is ALSO a pre-existing **bare-function** math intrinsic path
("M-F.3.3 gap (b)"): a PRELUDE stub exposes `sqrt(x)` (no `math.`
qualifier), rewritten in `cobrust-cli/src/build/intrinsics.rs` to
`__cobrust_math_sqrt` (a `cobrust-stdlib` C-ABI shim). That path is
UNTOUCHED by this ADR — it is the bare `sqrt(x)` surface; this ADR adds
the qualified `math.sqrt(x)` module surface.

### Scope (first batch — all libm, all clean f64 scalar)

Functions (18): `sqrt`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
`atan2(y,x)`, `sinh`, `cosh`, `tanh`, `exp`, `log` (natural), `log10`,
`log2`, `pow(x,y)`, `fabs`, `hypot(x,y)`.

Constants (3): `pi`, `e`, `tau`.

Deferred (recorded below, a follow-up ADR): (a) **`math.inf` / `math.nan`** —
the lexer (M-F.3.3, `lexer.rs`) tokenizes the bare words `inf` / `nan` as `f64`
literals UNCONDITIONALLY, so `math.inf` does not PARSE (`.` then a `Float("inf")`
token, an `Expected Ident` parse error). The idiomatic Cobrust spelling is
therefore the **bare** `inf` / `nan` (which already work); a `math.`-qualified
form needs the parser to accept the post-`.` `Float("inf")`/`Float("nan")` token
in attribute-name position — a frontend follow-up, NOT shipped here.
(b) `floor` / `ceil` / `trunc` (CPython returns **`int`** — needs an `fptosi`
cast + an `Int`-typed return, not the clean `f64 -> f64` shape), and
`factorial` / `gcd` / `isqrt` (integer ops, no libm symbol).

## Options considered

1. **Bare-libm extern, ALL 18 (chosen).** Declare the bare C-library
   `libm` symbols (`sqrt`, `sin`, `atan2`, `hypot`, …) as
   `extern "C"` in the LLVM module; `math.X` lowers (via the generic
   ecosystem-call path → `Terminator::Call` onto a `Constant::Str`
   runtime symbol) to a DIRECT `call double @sqrt(double)`. libm is
   ALWAYS linked (coil's Rust kernels + the embedded Rust std in
   `libcobrust_stdlib.a` pull it; macOS resolves bare math symbols via
   libSystem, Linux via libm in the C runtime). NO new crate, NO cabi,
   NO ecosystem archive, NO `cobrust-stdlib` edit.
2. **Reuse the existing `__cobrust_math_*` shims.** Route the 11
   overlapping functions (`sqrt`/`sin`/`cos`/`tan`/`log`/`exp`/`pow` …)
   to the pre-declared `cobrust-stdlib` shims, and add 11 new Rust
   shims for the rest. Rejected: it edits `cobrust-stdlib` (a new
   runtime impl + tests there), splits the mechanism into two halves,
   and adds an indirection (`__cobrust_math_sqrt` → `x.sqrt()` → libm)
   that the bare-libm path skips. The shims compile to the SAME libm
   call anyway.
3. **A `__cobrust_math_*` cabi crate / ecosystem archive.** The
   array-library staging mechanism. Massive overkill for scalar `f64`
   ops — the call goes straight to libm. Rejected.

## Decision

`import math` is registered as an ecosystem-module alias
(`is_ecosystem_module("math")`). The 18 functions are manifest rows in
`lookup_module_fn("math", _)` whose `runtime_symbol` is the **bare libm
name** (`"sqrt"`, `"atan2"`, …), `params` are `[Float]` /
`[Float, Float]`, `ret` is `Float`, tier `Numerical`. Codegen declares
those 18 bare libm symbols in `runtime_helper_decls`; the existing
extern-name dispatch in `lower_call` already lowers the `f64 -> f64` /
`(f64,f64) -> f64` ABI (including the i64-bits → f64 `bitcast` for a
`Ty::None` binary-op-result argument) and the `f64` return. The generic
MIR ecosystem-call path lowers the call with NO MIR change — it iterates
`sig.params` regardless of arity and the `coil` scalar-returning
aggregates (`mean`, `percentile`) already prove the `f64`-arg /
`f64`-return shape crosses.

The 5 constants are a NEW `lookup_module_const("math", _) -> Option<f64>`
seam. A constant is a parens-FREE attribute access (`math.pi`, never
`math.pi()`) — the math-idiomatic surface. The type checker's
`ExprKind::Attr` synth types `math.pi` as `Ty::Float` (and rejects an
unknown attr on the `math` alias as `UnknownName`, §2.5
compile-time-catch — NOT a false-green `fresh_var()`); the MIR `Attr`
lowering emits the value as a pure compile-time `Constant::Float` LLVM
literal (NO runtime call — a constant is just a number).

### Argument-coercion policy (§2.2 — no silent coercion)

Every `math` function parameter is `Ty::Float`. An `Int` argument
(`math.sqrt(2)`) is **REJECTED** at type-check — `unify_call_arg` never
promotes `Int -> Float` (it only unwraps `Ref(T) -> T`), so `Int` fails
to unify with `Float` and surfaces a `TypeMismatch { expected: Float,
actual: Int }`. The fix is to write `math.sqrt(2.0)`. This is the strict
Cobrust choice and is **consistent with coil's scalar-arg convention**
(`coil.power(a, 0.0)`, `coil.percentile(a, 50.0)` — all float literals;
the `x ** 0` case is `power(a, 0.0)`, never `power(a, 0)`).

### Domain-error policy (the @py_compat divergence)

CPython `math.sqrt(-1)` / `math.log(0)` / `math.log(-1)` raise
`ValueError("math domain error")`. libm returns `NaN` / `-inf` / `NaN`.
Cobrust **adopts the libm behaviour** — the documented Numerical-tier
surface: `math.sqrt(-1.0)` returns `NaN`, `math.log(0.0)` returns
`-inf`. This is honest (NO silent wrong-finite value, NO trap) and is
the simplest path that respects "no silent coercion". It is recorded as
the @py_compat divergence below.

### @py_compat tier

- **Functions: `Numerical`.** libm's transcendentals may differ from
  CPython in the LAST ULP (CPython links the same platform libm but its
  argument reduction can differ). `sqrt` is IEEE-correctly-rounded so it
  is bit-exact and platform-stable; `sin`/`cos`/`atan2`/`exp`/`log`/…
  are platform-libm and may vary in the final bit between macOS libm and
  ubuntu glibc. The DOMAIN divergence (NaN/-inf vs ValueError) is the
  declared Numerical-tier divergence.
- **Constants: exact.** `pi`/`e`/`tau` are `std::f64::consts` — the SAME
  `f64` rounding of the mathematical constants CPython uses (verified
  bit-equal against the python3.11 oracle: `3.141592653589793`,
  `2.718281828459045`, `6.283185307179586`). (`inf`/`nan` are the canonical
  IEEE-754 values too, but are written as the BARE `inf`/`nan` literals, not
  `math.inf`/`math.nan` — see §Deferred (a).)

## Consequences

- **Positive**
  - The first core scalar stdlib module ships — `math.sqrt` / `math.pi`
    are the highest-frequency Python numeric idioms (§2.5 training-data
    overlap).
  - Zero new dependency, zero new crate, zero `cobrust-stdlib` edit,
    `Cargo.lock` unchanged — the kernel IS libm, already linked.
  - One uniform mechanism for all 18 functions (no shim/extern split).
  - §2.5 compile-time-catch: bad arg type + unknown constant are both
    hard type errors with FIX-bearing suggestions, never runtime
    surprises.
- **Negative**
  - The float-print surface (`__cobrust_println_float`, Rust `{}`)
    prints integer-valued floats WITHOUT a `.0` (`hypot(3,4)` -> `5`,
    not CPython's `5.0`) and prints `NaN`/`-inf` (capital N) vs
    CPython's `nan`/`-inf`. This is a print-repr divergence, NOT a
    value divergence; the .cb e2e asserts the cobrust form. A
    CPython-parity float repr is a separate (orthogonal) concern.
  - Numerical-tier last-ULP divergence for transcendentals across
    platforms — the .cb e2e asserts identities / `as i64` rounded forms
    for sin/cos/atan2/exp/log, full-precision strings only for sqrt +
    constants + fabs (all exact).
- **Neutral / unknown**
  - `floor`/`ceil`/`trunc` (int-returning) + `factorial`/`gcd`/`isqrt`
    deferred to a follow-up — they need an `fptosi` cast / an
    `Int`-typed return / a non-libm impl, a different shape than this
    clean `f64 -> f64` batch.

## Evidence

- Differential oracle: `/opt/homebrew/bin/python3.11 -c "import math; …"`
  — every asserted value in `cobrust-types`'s `math_*` unit tests +
  `crates/cobrust-cli/tests/math_e2e.rs` is the python3.11 result.
- Live end-to-end (`cobrust build` → spawn): `math.sqrt(2.0)` ->
  `1.4142135623730951`; `math.pi` -> `3.141592653589793`;
  `math.pow(2.0,10.0)` -> `1024`; `math.hypot(3.0,4.0)` -> `5`; the
  Pythagoras chain `sqrt(pow(3,2)+pow(4,2))` -> `5`;
  `math.sqrt(-1.0)` -> `NaN`; `math.log(0.0)` -> `-inf`.
- §2.2 rejections (compile-time): `math.sqrt(2)` ->
  `TypeMismatch { expected: Float, actual: Int }`; `math.sqrt("x")` ->
  `TypeMismatch { expected: Float, actual: Str }`; `math.phi` ->
  `UnknownName { name: "math.phi" }`.
- libm-link probe: bare `sqrt`/`atan2`/`hypot`/`log2`/`fabs` link with
  the default `cc` on macOS with no `-lm` (libSystem); the cobrust link
  uses the same `cc` and already pulls libm via coil + the embedded
  Rust std.

## Files changed

- `crates/cobrust-types/src/ecosystem.rs` — 18 `("math", _)` rows in
  `lookup_module_fn`; `lookup_module_const`; `"math"` in
  `is_ecosystem_module`; unit tests.
- `crates/cobrust-types/src/lib.rs` — re-export `lookup_module_const`.
- `crates/cobrust-types/src/check.rs` — `math.pi` constant typing +
  unknown-attr rejection in the `ExprKind::Attr` synth.
- `crates/cobrust-mir/src/lower.rs` — `math.pi` -> `Constant::Float` in
  the `lower_expr` Attr arm + `synth_expr_ty` Attr arm. NO new lowering
  MECHANISM for the functions (the generic ecosystem-call path lowers
  them).
- `crates/cobrust-codegen/src/llvm_backend.rs` — declare the 18 bare
  libm externs in `runtime_helper_decls`.
- `crates/cobrust-cli/tests/math_e2e.rs` — 13 .cb end-to-end tests.
- Docs: `docs/agent/modules/math.md`, `docs/human/en/import-math.md`,
  `docs/human/zh/import-math.md`.
