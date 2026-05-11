# LC-098 Power X N Recursive

**Category**: Recursion
**Difficulty**: Medium

## Algorithm

Compute x raised to the power n (where n is an integer that may be negative)
using recursive fast exponentiation (also called exponentiation by squaring).

The naive approach multiplies x by itself n times (O(n) multiplications). The
fast recursive formulation halves the problem at each step:

- If n == 0, return 1.
- If n is even: result = power(x, n/2); return result * result.
- If n is odd: result = power(x, n/2); return result * result * x.

This reduces the number of multiplications to O(log |n|).

For negative exponents, compute the positive-exponent result then take its
reciprocal: `power(x, -n) = 1.0 / power(x, n)`.

For the purposes of this oracle (integer-only I/O), the inputs are constrained
to integer base and non-negative exponent, and the result is guaranteed to fit
in a 64-bit integer.

## Input format

```
Line 1: X N — integer base and non-negative integer exponent, space-separated
```

## Oracle

- X=2, N=10  → `1024`
- X=3, N=5   → `243`
- X=2, N=0   → `1`
- X=1, N=100 → `1`

## Approach hint

Implement a recursive helper `fn pow_rec(base: i64, exp: i64) -> i64`. Base
case: `exp == 0` returns 1. Even branch: `half = pow_rec(base, exp / 2)`,
return `half * half`. Odd branch: same but multiply by `base` once more.
