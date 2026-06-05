---
doc_kind: module
module_id: mod:math
crate: none
last_verified_commit: 664a028
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

- **ADR-0083 — delivered.** 18 functions + 5 constants, all libm/clean
  `f64`. 139 `cobrust-types` lib tests + 12 `.cb` e2e tests green.

## Public surface

### Functions (18) — `lookup_module_fn("math", _)`

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

### Constants (5) — `lookup_module_const("math", _) -> Option<f64>`

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

### Deferred (follow-up ADR)

- `floor` / `ceil` / `trunc` — CPython returns **`int`**; needs an
  `fptosi` cast + an `Int`-typed return (not the clean `f64 -> f64`
  shape).
- `factorial` / `gcd` / `isqrt` — integer ops, no libm symbol.

## Lowering (the 5 layers)

1. **Kernel** — none. The kernel IS libm.
2. **cabi** — none. The call goes straight to libm.
3. **`cobrust-types/src/ecosystem.rs`**
   - `is_ecosystem_module("math") == true`.
   - `lookup_module_fn("math", fn)` — the 18 rows, `runtime_symbol` is
     the bare libm name.
   - `lookup_module_const("math", name)` — the 5 constants.
   - `cobrust-types/src/check.rs` — `ExprKind::Attr` synth types
     `math.pi` as `Ty::Float`; an unknown attr on the `math` alias is
     `UnknownName` (§2.5 compile-time-catch).
4. **`cobrust-mir/src/lower.rs`** — NO new mechanism for functions (the
   generic `try_lower_ecosystem_call` Case-1 path -> `emit_ecosystem_call`
   lowers `[Float]`/`[Float,Float]` -> `Float` with a `Constant::Str`
   runtime symbol). Constants: the `lower_expr` Attr arm + `synth_expr_ty`
   Attr arm emit `Constant::Float(v.to_bits())`.
5. **`cobrust-codegen/src/llvm_backend.rs`** — declare the 18 bare libm
   symbols in `runtime_helper_decls`; the existing extern-name dispatch
   in `lower_call` lowers the `f64`-arg / `f64`-return ABI (incl. the
   i64-bits -> f64 `bitcast` for a `Ty::None` binary-op-result arg).

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

- `lookup_module_fn("math", _)` returns the 18 rows; `lookup_module_const`
  the 5 constants. ✅
- `math.sqrt(2.0)` compiles to `call double @sqrt(double)` and prints
  `1.4142135623730951`. ✅
- `math.sqrt(2)` (Int) + `math.phi` (unknown const) are compile-time
  type errors. ✅
- 12 `.cb` e2e tests green. ✅
