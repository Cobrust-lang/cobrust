---
doc_kind: module
module_id: mod:math
crate: none
last_verified_commit: e82b780
dependencies: [mod:types, mod:mir, mod:codegen]
---

# Module: math (scalar stdlib surface)

## Purpose

`import math` — the FIRST core Python stdlib module wired into Cobrust
(per ADR-0083). Scalar `f64` math: `math.sqrt(x)`, `math.pi`, etc.

NOT a crate. There is no `cobrust-math`; `math` is a compiler surface —
a manifest in `cobrust-types` + bare-libm externs in `cobrust-codegen`.
The "kernel" IS the C-library `libm` (already linked via coil + the
embedded Rust std).

DISTINCT from `coil`: `coil.sqrt(a)` is a `Buffer -> Buffer` ufunc;
`math.sqrt(x)` is a scalar `f64 -> f64` op. Different module, signature,
runtime symbol — they never collide.

DISTINCT from the bare-function intrinsic path: `sqrt(x)` (no `math.`
qualifier) is the M-F.3.3 PRELUDE intrinsic -> `__cobrust_math_sqrt`
shim; `math.sqrt(x)` (this module) -> the bare libm `sqrt` symbol.

## Status

- **ADR-0083 part-1 — delivered.** 18 functions + 5 constants, all
  libm/clean `f64`.
- **ADR-0083 part-2 — delivered.** 10 more functions with INT / BOOL /
  scaling return shapes (`floor`/`ceil`/`trunc` -> `Int`;
  `isnan`/`isinf`/`isfinite` -> `Bool`; `degrees`/`radians`/`copysign`/
  `fmod` -> `Float`). 144 `cobrust-types` lib tests + 13 part-1 +
  12 part-2 `.cb` e2e tests + 10 `cobrust-stdlib` shim unit tests green.

## Public surface

### Functions (part-1, 18) — `lookup_module_fn("math", _)`

| `.cb` form | signature | runtime symbol (bare libm) |
|---|---|---|
| `math.sqrt(x)` | `[Float] -> Float` | `sqrt` |
| `math.sin(x)` | `[Float] -> Float` | `sin` |
| `math.cos(x)` | `[Float] -> Float` | `cos` |
| `math.tan(x)` | `[Float] -> Float` | `tan` |
| `math.asin(x)` | `[Float] -> Float` | `asin` |
| `math.acos(x)` | `[Float] -> Float` | `acos` |
| `math.atan(x)` | `[Float] -> Float` | `atan` |
| `math.sinh(x)` | `[Float] -> Float` | `sinh` |
| `math.cosh(x)` | `[Float] -> Float` | `cosh` |
| `math.tanh(x)` | `[Float] -> Float` | `tanh` |
| `math.exp(x)` | `[Float] -> Float` | `exp` |
| `math.log(x)` | `[Float] -> Float` | `log` (natural) |
| `math.log10(x)` | `[Float] -> Float` | `log10` |
| `math.log2(x)` | `[Float] -> Float` | `log2` |
| `math.fabs(x)` | `[Float] -> Float` | `fabs` |
| `math.pow(x, y)` | `[Float, Float] -> Float` | `pow` |
| `math.atan2(y, x)` | `[Float, Float] -> Float` | `atan2` |
| `math.hypot(x, y)` | `[Float, Float] -> Float` | `hypot` |

All tier `PyCompatTier::Numerical`.

### Functions (part-2, 10) — the INT / BOOL / scaling shapes

| `.cb` form | signature | runtime symbol | tier |
|---|---|---|---|
| `math.floor(x)` | `[Float] -> Int` | `__cobrust_math_floor_int` | Strict |
| `math.ceil(x)` | `[Float] -> Int` | `__cobrust_math_ceil_int` | Strict |
| `math.trunc(x)` | `[Float] -> Int` | `__cobrust_math_trunc_int` | Strict |
| `math.isnan(x)` | `[Float] -> Bool` | `__cobrust_math_isnan` | Strict |
| `math.isinf(x)` | `[Float] -> Bool` | `__cobrust_math_isinf` | Strict |
| `math.isfinite(x)` | `[Float] -> Bool` | `__cobrust_math_isfinite` | Strict |
| `math.degrees(x)` | `[Float] -> Float` | `__cobrust_math_degrees` | Strict |
| `math.radians(x)` | `[Float] -> Float` | `__cobrust_math_radians` | Strict |
| `math.copysign(x, y)` | `[Float, Float] -> Float` | `copysign` (bare libm) | Strict |
| `math.fmod(x, y)` | `[Float, Float] -> Float` | `fmod` (bare libm) | Strict |

- **`floor`/`ceil`/`trunc` return `Int`** (CPython `math.floor(2.7) == 2`,
  an int) and DIVERGE on a NEGATIVE input — the load-bearing distinction:
  `floor(-1.5)=-2` (toward −∞), `ceil(-1.5)=-1` (toward +∞),
  `trunc(-1.5)=-1` (toward ZERO). The shapes mirror `coil.argmin`
  (`Buffer -> i64`).
- **`isnan`/`isinf`/`isfinite` return `Bool`**, mirroring `coil.any` /
  `coil.all`. Usable directly in `if math.isnan(x):`. Oracle:
  `isnan(nan)=True`, `isinf(inf)=True`, `isfinite(inf)=False`,
  `isfinite(nan)=False`.
- **`degrees(pi)=180.0`, `radians(180.0)=pi`** (exact scaling via Rust
  `to_degrees`/`to_radians`). `copysign(3.0,-1.0)=-3.0`,
  `fmod(7.0,3.0)=1.0`.

> **`math.floor` is DISTINCT from the bare `floor(x)` builtin.** Cobrust
> also has a bare-function `floor(x)` / `ceil(x)` PRELUDE intrinsic
> (`f64 -> f64`, rewritten to the `__cobrust_math_floor` shim) — the
> M-F.3.3 surface. `math.floor` is the Python `math.`-qualified,
> INT-returning op: a SEPARATE symbol (`__cobrust_math_floor_int`) + an
> `Int` return. No collision (different callee, different return `Ty`).

### Constants (3) — `lookup_module_const("math", _) -> Option<f64>`

| `.cb` form | value | notes |
|---|---|---|
| `math.pi` | `std::f64::consts::PI` = `3.141592653589793` | exact |
| `math.e` | `std::f64::consts::E` = `2.718281828459045` | exact |
| `math.tau` | `std::f64::consts::TAU` = `6.283185307179586` | exact |

Parens-FREE attribute access (`math.pi`, NOT `math.pi()`).

> **`inf` / `nan` are BARE literals, not `math.`-qualified.** The lexer
> tokenizes the words `inf` and `nan` as `f64` literals (M-F.3.3), so
> `math.inf` does **not** parse (`.` then a `Float("inf")` token). Write the
> bare `inf` / `nan` (e.g. `let big: f64 = inf`). A `math.inf` / `math.nan`
> spelling is a deferred parser follow-up — see §Deferred / ADR-0083.

### Deferred (remaining follow-up)

- `factorial` / `gcd` / `isqrt` — integer ops, no libm symbol.
- `math.inf` / `math.nan` — the lexer tokenizes bare `inf`/`nan` as `f64`
  literals, so `math.inf` does not parse; write the bare `inf`/`nan`.

## Lowering (the 5 layers)

1. **Kernel** —
   - Part-1 + `copysign`/`fmod`: none. The kernel IS libm.
   - Part-2 (`floor_int`/`ceil_int`/`trunc_int`/`isnan`/`isinf`/
     `isfinite`/`degrees`/`radians`): `cobrust-stdlib/src/math.rs` shims
     (`x.floor() as i64`, `x.is_nan()`, `x.to_degrees()`, …).
2. **cabi** — none. Part-1 + `copysign`/`fmod` go straight to libm;
   the part-2 shims ARE the C-ABI (`#[no_mangle] extern "C"`).
3. **`cobrust-types/src/ecosystem.rs`**
   - `is_ecosystem_module("math") == true`.
   - `lookup_module_fn("math", fn)` — 18 part-1 rows (bare libm name) +
     10 part-2 rows (the `__cobrust_math_*_int` / `_isnan` shims +
     bare `copysign`/`fmod`).
   - `lookup_module_const("math", name)` — the 5 constants.
   - `cobrust-types/src/check.rs` — `ExprKind::Attr` synth types
     `math.pi` as `Ty::Float`; an unknown attr on the `math` alias is
     `UnknownName` (§2.5 compile-time-catch).
4. **`cobrust-mir/src/lower.rs`** — NO new mechanism for ANY function,
   part-1 OR part-2. The generic `try_lower_ecosystem_call` Case-1 path
   -> `emit_ecosystem_call` declares `_ecoret` with the manifest `ret_ty`
   (`Float` / `Int` / `Bool`) and emits a `Constant::Str` runtime-symbol
   `Terminator::Call`. The `Int`/`Bool` returns ride the SAME path
   `coil.argmin`/`coil.any` already proved. Constants: the `lower_expr`
   Attr arm + `synth_expr_ty` Attr arm emit `Constant::Float(v.to_bits())`.
5. **`cobrust-codegen/src/llvm_backend.rs`** — declare the externs in
   `runtime_helper_decls`:
   - 18 part-1 bare libm `(f64)->f64` / `(f64,f64)->f64`.
   - part-2 `degrees`/`radians` `(f64)->f64` (in the `__cobrust_math_*`
     single-arg loop); `_floor_int`/`_ceil_int`/`_trunc_int` `(f64)->i64`;
     `_isnan`/`_isinf`/`_isfinite` `(f64)->i1` (the Rust C-ABI `-> bool`,
     declared with `bool_type()` EXACTLY as `__cobrust_coil_any`);
     `copysign`/`fmod` added to the bare-libm two-arg loop.
   The existing extern-name dispatch in `lower_call` lowers all of these
   off the declared `callee` signature (incl. the i64-bits -> f64
   `bitcast` for a `Ty::None` binary-op-result f64 arg; `write_place`
   stores the i64 / i1 return into the `_ecoret` destination).

## Invariants

- **arg coercion**: every param is `Ty::Float`. `math.sqrt(2)` (Int) is
  a HARD `TypeMismatch { expected: Float, actual: Int }` — §2.2 no
  silent coercion; write `2.0`. Consistent with coil scalar args.
- **domain errors**: libm semantics — `math.sqrt(-1.0)` -> `NaN`,
  `math.log(0.0)` -> `-inf`. CPython would RAISE `ValueError`; Cobrust
  returns the IEEE value (NO trap, NO silent wrong-finite value). The
  declared Numerical-tier divergence.
- **print repr**: `__cobrust_println_float` (Rust `{}`) drops the `.0`
  for integer-valued floats (`hypot(3,4)` -> `5`) and prints `NaN` /
  `-inf` (capital N). A print-repr divergence from CPython `5.0` /
  `nan`, NOT a value divergence.

## Done means

- `lookup_module_fn("math", _)` returns the 18 part-1 + 10 part-2 rows;
  `lookup_module_const` the 5 constants. ✅
- `math.sqrt(2.0)` compiles to `call double @sqrt(double)` and prints
  `1.4142135623730951`. ✅
- `math.floor(-1.5)` -> `-2` (an Int), `math.trunc(-1.5)` -> `-1`
  (distinct from floor); `if math.isnan(nan):` takes the True branch;
  `math.degrees(pi)` -> `180`; `math.copysign(3.0,-1.0)` -> `-3`. ✅
- `math.sqrt(2)` / `math.floor(2)` (Int arg) + `math.phi` (unknown const)
  are compile-time type errors. ✅
- The bare `floor(3.7) as i64` builtin stays green — no collision with
  `math.floor`'s `__cobrust_math_floor_int`. ✅
- 13 part-1 + 11 part-2 `.cb` e2e tests green. ✅
