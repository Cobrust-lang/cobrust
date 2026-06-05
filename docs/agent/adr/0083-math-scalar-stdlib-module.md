---
doc_kind: adr
adr_id: 0083
title: math — the first core scalar stdlib module (import math) via bare-libm-extern + cobrust-stdlib-shim lowering
status: accepted
date: 2026-06-05
last_verified_commit: e82b780
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

### Scope (part-1 — all libm, all clean f64 scalar)

Functions (18): `sqrt`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
`atan2(y,x)`, `sinh`, `cosh`, `tanh`, `exp`, `log` (natural), `log10`,
`log2`, `pow(x,y)`, `fabs`, `hypot(x,y)`.

Constants (3): `pi`, `e`, `tau`.

### Scope (part-2 — the INT / BOOL / scaling return shapes, shipped)

The functions DEFERRED from part-1 because they leave the clean
`f64 -> f64` libm batch. Both non-`Float` return shapes are ALREADY
PROVEN by `coil` and mirrored EXACTLY here — see §"Part-2: the INT /
BOOL / scaling return shapes" below. Functions (10):

- `floor(x)` / `ceil(x)` / `trunc(x)` — **`[Float] -> Int`** (CPython
  returns `int`). Via NEW `cobrust-stdlib` shims
  `__cobrust_math_floor_int` / `_ceil_int` / `_trunc_int`
  (`x.floor()/.ceil()/.trunc() as i64`). Strict tier.
- `isnan(x)` / `isinf(x)` / `isfinite(x)` — **`[Float] -> Bool`**. Via
  NEW shims `__cobrust_math_isnan` / `_isinf` / `_isfinite`
  (`x.is_nan()/.is_infinite()/.is_finite()`). Strict tier.
- `degrees(x)` / `radians(x)` — `[Float] -> Float`. Via NEW shims
  `__cobrust_math_degrees` / `_radians` (Rust `to_degrees`/`to_radians`,
  the exact `x*180/π` / `x*π/180` scaling). Strict tier.
- `copysign(x,y)` / `fmod(x,y)` — `[Float, Float] -> Float`. BARE libm
  two-arg symbols (NO shim, like part-1's `pow`/`atan2`/`hypot`).
  Both Strict: `copysign` is a sign-bit transplant; `fmod` is the IEEE-754
  floating remainder, an EXACT op (no rounding) → bit-identical across
  conforming libm (unlike the transcendental pow/atan2/hypot — Numerical).

Deferred (a remaining follow-up): (a) **`math.inf` / `math.nan`** —
the lexer (M-F.3.3, `lexer.rs`) tokenizes the bare words `inf` / `nan` as `f64`
literals UNCONDITIONALLY, so `math.inf` does not PARSE (`.` then a `Float("inf")`
token, an `Expected Ident` parse error). The idiomatic Cobrust spelling is
therefore the **bare** `inf` / `nan` (which already work); a `math.`-qualified
form needs the parser to accept the post-`.` `Float("inf")`/`Float("nan")` token
in attribute-name position — a frontend follow-up, NOT shipped here.
(b) `factorial` / `gcd` / `isqrt` (integer ops, no libm symbol).

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

### Part-2: the INT / BOOL / scaling return shapes

Part-2 ships the 10 deferred functions. The two non-`Float` return
shapes are NOT new mechanism — they MIRROR `coil` exactly:

- **`[Float] -> Int`** (`floor`/`ceil`/`trunc`) mirrors `coil.argmin`'s
  `Buffer -> i64`. `runtime_symbol` is a NEW `cobrust-stdlib` shim
  `__cobrust_math_floor_int(x: f64) -> i64 { x.floor() as i64 }` (+
  `_ceil_int` / `_trunc_int`); manifest `ret: Ty::Int`; codegen extern
  `(f64) -> i64`. The `i64` lands in the `.cb` `_ecoret` Int local.
- **`[Float] -> Bool`** (`isnan`/`isinf`/`isfinite`) mirrors
  `coil.any` / `coil.all`'s `Buffer -> bool`. `runtime_symbol` is a NEW
  shim `__cobrust_math_isnan(x: f64) -> bool { x.is_nan() }` (+
  `_isinf` / `_isfinite`); manifest `ret: Ty::Bool`; codegen extern
  `(f64) -> i1` (the Rust C-ABI `-> bool`, declared with
  `bool_type()` EXACTLY as `__cobrust_coil_any`). The `i1` lands in the
  `.cb` `_ecoret` Bool local — usable directly in an `if math.isnan(x):`
  condition (proven by the `.cb` e2e).

There is **NO new MIR arm**: the generic ecosystem-call path
(`emit_ecosystem_call`) declares `_ecoret` with the manifest `ret_ty`
(`Int` / `Bool`) and emits the `Terminator::Call` — it already lowers
`coil`'s `Int` / `Bool` returns, so part-2's shapes cross for free. The
codegen extern-call path drives the arg/return LLVM types off the
declared `callee` signature (`write_place` stores the i64 / i1 result
into the destination, bridging any i1/i8 width gap into the alloca).

The float-returning rows (`degrees`/`radians` via the `to_degrees` /
`to_radians` shims; `copysign` / `fmod` via bare libm) reuse part-1's
`f64 -> f64` / `(f64,f64) -> f64` extern ABI verbatim.

#### `floor`/`ceil`/`trunc` are DISTINCT from the bare-function builtins

Cobrust ALREADY has (a) a bare `floor(x)` / `ceil(x)` PRELUDE builtin
(`check.rs`, `f64 -> f64`) rewritten to (b) the `__cobrust_math_floor` /
`_ceil` shims (`f64 -> f64`) in `cobrust-stdlib`. Those are the
**bare-function** surface and are LEFT UNTOUCHED. `math.floor` is the
PYTHON `math.`-qualified, INT-returning op — a DISTINCT symbol
(`__cobrust_math_floor_int`) + an `Int` return. The two never collide:
different callee name, different return `Ty`. The bare `floor(3.7) as
i64` e2e (`f64_e2e.rs::f64e09`) stays green; a unit test
(`math_part2_int_shims_distinct_from_bare_f64_floor`) pins the
non-collision.

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
- **Part-2: ALL `Strict`.** `floor`/`ceil`/`trunc` return an exact
  integer (`as i64` of `f64::floor`/`ceil`/`trunc` — no last-ULP
  question); `isnan`/`isinf`/`isfinite` are the unambiguous,
  platform-stable IEEE-754 classification; `degrees`/`radians` are the
  exact `x*180/π` / `x*π/180` scaling; `copysign` only transplants the
  sign bit; and `fmod` is the IEEE-754 floating remainder — an EXACT
  operation (result `x - n*y` computed with no rounding), so it is
  bit-identical across conforming libm and to CPython's libm `math.fmod`
  (NOT a last-ULP transcendental — corrected from a first-pass `Numerical`
  label). All six families are **`Strict`**. Oracle
  (python3.11): `floor(-1.5)=-2`, `ceil(-1.5)=-1`, `trunc(-1.5)=-1`,
  `isnan(nan)=True`, `isfinite(inf)=False`, `degrees(pi)=180.0`,
  `radians(180.0)=pi`, `copysign(3,-1)=-3.0`, `fmod(7,3)=1.0`.

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
- **Part-2 (shipped, see §"Part-2")**
  - `floor`/`ceil`/`trunc` (int-returning), `isnan`/`isinf`/`isfinite`
    (bool-returning), `degrees`/`radians`, `copysign`/`fmod` now ship —
    the `[Float] -> Int` / `[Float] -> Bool` shapes mirror
    `coil.argmin` / `coil.any` with NO new MIR mechanism. The
    int-returning `math.floor` is a DISTINCT symbol from the bare
    `floor(x)` builtin (no collision).
- **Neutral / unknown**
  - `factorial`/`gcd`/`isqrt` (non-libm integer ops) + the
    `math.inf`/`math.nan` parser follow-up remain deferred.

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
- **Part-2 live end-to-end** (`cobrust build` → spawn):
  `math.floor(-1.5)` -> `-2`, `math.ceil(-1.5)` -> `-1`,
  `math.trunc(-1.5)` -> `-1` (the three DIVERGE on the negative —
  toward −∞ / +∞ / zero); the Int return flows into i64 arithmetic
  (`floor(2.7) + trunc(1.9)` -> `3`); `if math.isnan(nan):` takes the
  True branch, `if math.isfinite(inf):` takes the False branch (the bool
  return is USED in a condition); `math.degrees(pi)` -> `180`;
  `math.copysign(3.0,-1.0)` -> `-3`; `math.fmod(7.0,3.0)` -> `1`.
  `math.floor(2)` (Int arg) -> compile-time `TypeMismatch`. The bare
  `floor(3.7) as i64` (`f64_e2e.rs::f64e09`) stays green — no collision
  with the new `__cobrust_math_floor_int` symbol.

## Files changed

### Part-1

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

### Part-2

- `crates/cobrust-stdlib/src/math.rs` — NEW shims
  `__cobrust_math_floor_int` / `_ceil_int` / `_trunc_int` (`f64 -> i64`),
  `__cobrust_math_isnan` / `_isinf` / `_isfinite` (`f64 -> bool`),
  `__cobrust_math_degrees` / `_radians` (`f64 -> f64`) + 12 unit tests.
  `copysign` / `fmod` are bare libm — NO shim. (`lib.rs` unchanged — the
  shims are `#[no_mangle] extern "C"`, resolved by symbol, not
  re-exported.)
- `crates/cobrust-types/src/ecosystem.rs` — 10 NEW `("math", _)` rows
  (`floor`/`ceil`/`trunc` `[Float]->Int`; `isnan`/`isinf`/`isfinite`
  `[Float]->Bool`; `degrees`/`radians`/`copysign`/`fmod` `->Float`);
  `floor`/`ceil`/`trunc` removed from the deferred-absent unit test;
  5 NEW part-2 manifest unit tests.
- `crates/cobrust-codegen/src/llvm_backend.rs` — declare the new shim
  externs: `degrees`/`radians` in the single-arg `__cobrust_math_*`
  loop; `_floor_int`/`_ceil_int`/`_trunc_int` as `(f64) -> i64`;
  `_isnan`/`_isinf`/`_isfinite` as `(f64) -> i1`; `copysign`/`fmod`
  added to the bare-libm two-arg loop. NO MIR change (the generic
  ecosystem-call path lowers the `Int`/`Bool` returns).
- `crates/cobrust-cli/tests/math_part2_e2e.rs` — 11 .cb end-to-end tests
  (the floor/ceil/trunc negative-divergence + int-arithmetic, the
  isnan/isinf/isfinite `if`-condition truth tables, degrees/copysign/fmod,
  the Int-arg rejection).
