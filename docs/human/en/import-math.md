# `import math` — scalar math from Cobrust

> Status: ADR-0083. The FIRST core Python stdlib module (`json` / `re` /
> `datetime` still to come). `math` gives you scalar `f64` math —
> `math.sqrt`, `math.sin`, `math.pi` — the numeric idioms you write
> every day.

## Example first

```python
import math

fn main() -> i64:
    print(math.sqrt(2.0))                                  # 1.4142135623730951
    print(math.pi)                                         # 3.141592653589793
    print(math.pow(2.0, 10.0))                             # 1024
    print(math.hypot(3.0, 4.0))                            # 5
    let h: f64 = math.sqrt(math.pow(3.0, 2.0) + math.pow(4.0, 2.0))
    print(h)                                               # 5
    return 0
```

Build and run it:

```bash
cobrust build prog.cb -o prog
./prog
```

## What you get

### Float functions (18)

- **One argument** (`f64 -> f64`): `math.sqrt`, `math.sin`, `math.cos`,
  `math.tan`, `math.asin`, `math.acos`, `math.atan`, `math.sinh`,
  `math.cosh`, `math.tanh`, `math.exp`, `math.log` (natural log),
  `math.log10`, `math.log2`, `math.fabs`.
- **Two arguments** (`(f64, f64) -> f64`): `math.pow(x, y)`,
  `math.atan2(y, x)`, `math.hypot(x, y)`.

### Rounding to an integer — `floor`, `ceil`, `trunc`

These return an **`i64`** (just like Python, where `math.floor(2.7)` is
the int `2`, not `2.0`). The three round differently, and they only
DIFFER on a negative input:

```python
import math

fn main() -> i64:
    print(math.floor(-1.5))   # -2   round toward -infinity
    print(math.ceil(-1.5))    # -1   round toward +infinity
    print(math.trunc(-1.5))   # -1   round toward zero
    let n: i64 = math.floor(2.7)   # the result is a real i64
    print(n + 1)              # 3    — usable in integer arithmetic
    return 0
```

> `math.floor` (this module) is different from the bare `floor(x)`
> built-in. The built-in `floor(x)` returns a float; `math.floor(x)`
> returns an int. They never clash.

### Testing a number — `isnan`, `isinf`, `isfinite`

These return a **`bool`**, so you use them in a condition:

```python
import math

fn main() -> i64:
    let x: f64 = nan
    if math.isnan(x):
        print(1)              # taken: nan is not-a-number
    else:
        print(0)
    if math.isfinite(inf):    # False — infinity is not finite
        print(1)
    else:
        print(0)
    return 0
```

`math.isnan(nan)` is `True`; `math.isinf(inf)` is `True`;
`math.isfinite(1.0)` is `True` but `math.isfinite(inf)` and
`math.isfinite(nan)` are `False`.

### A few more — `degrees`, `radians`, `copysign`, `fmod`

- `math.degrees(x)` / `math.radians(x)` convert between radians and
  degrees (`f64 -> f64`): `math.degrees(math.pi)` is `180`,
  `math.radians(180.0)` is `math.pi`.
- `math.copysign(x, y)` returns `x` with the sign of `y`
  (`math.copysign(3.0, -1.0)` is `-3`).
- `math.fmod(x, y)` is the C floating remainder
  (`math.fmod(7.0, 3.0)` is `1`).

### Constants (5)

- `math.pi` → `3.141592653589793`
- `math.e` → `2.718281828459045`
- `math.tau` → `6.283185307179586`

Constants are plain attributes — write `math.pi`, never `math.pi()`.

For infinity and not-a-number, write the **bare literals** `inf` and `nan`
(e.g. `let big: f64 = inf`), **not** `math.inf` / `math.nan`: Cobrust's lexer
already tokenizes the words `inf` and `nan` as float literals, so a
`math.`-qualified spelling does not parse. (A `math.inf` form is a deferred
parser follow-up — see ADR-0083.)

### Not yet (a follow-up)

`math.factorial` / `math.gcd` / `math.isqrt` (integer math, no C-library
symbol) are still deferred.

## Two rules to know

### 1. Arguments must be floats — write `2.0`, not `2`

Cobrust never silently turns an integer into a float (constitution
§2.2). `math.sqrt(2)` is a **compile-time error**:

```python
print(math.sqrt(2))    # error: TypeMismatch { expected: Float, actual: Int }
print(math.sqrt(2.0))  # correct
```

This is the same rule the array library `coil` follows (`coil.power(a,
0.0)`), and it means a wrong-type argument is caught while you compile,
not at runtime.

### 2. Out-of-domain inputs return `NaN` / `-inf`, not an error

Python's `math.sqrt(-1)` raises `ValueError`. Cobrust follows the
underlying C math library instead and returns the IEEE value:

```python
print(math.sqrt(-1.0))   # NaN
print(math.log(0.0))     # -inf
```

No exception, no trap, and never a wrong finite number — you get the
honest floating-point result. (This is the declared "numerical-tier"
behaviour; see "Why this design?".)

## Why this design?

- **The kernel is the C math library.** `math.sqrt(x)` compiles to a
  direct `call sqrt(double)` into `libm`, which is already linked. No
  new crate, no wrapper, no dependency — the fastest and simplest path.
- **`math` is scalar; `coil` is arrays.** `coil.sqrt(a)` takes a whole
  buffer and returns a buffer; `math.sqrt(x)` takes one number and
  returns one number. They share nothing and never clash.
- **Numerical tier, stated honestly.** `sqrt` and the constants are
  bit-exact and identical across platforms. The transcendental
  functions (`sin`, `cos`, `atan2`, …) may differ from CPython — and
  between macOS and Linux — in the very last bit, because they use the
  platform's `libm`. The domain behaviour (NaN/-inf vs Python's
  `ValueError`) is the one deliberate divergence we document up front.
- **Constants are free.** `math.pi` is a compile-time number baked
  straight into the program — there is no function call at runtime.

## A note on printing

The float printer shows integer-valued results without a trailing `.0`:
`math.hypot(3.0, 4.0)` prints `5`, not `5.0`; `math.pow(2.0, 10.0)`
prints `1024`. Out-of-domain results print as `NaN` and `-inf`. This is
a display choice, not a value difference — the numbers themselves are
exactly what you expect.
