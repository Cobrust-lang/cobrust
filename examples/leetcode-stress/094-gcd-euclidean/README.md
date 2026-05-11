# LC-094 GCD Euclidean

**Category**: Math
**Difficulty**: Easy

## Algorithm

Given two non-negative integers, compute their greatest common divisor (GCD) —
the largest integer that divides both without remainder.

The Euclidean algorithm is the classical solution: repeatedly replace the larger
value with the remainder of dividing the larger by the smaller. The process
terminates when the remainder reaches zero, at which point the non-zero value is
the GCD. This converges in O(log(min(a, b))) steps.

Formally: `gcd(a, b) = gcd(b, a mod b)`, with base case `gcd(a, 0) = a`.
Both the iterative and recursive formulations are equivalent.

Edge cases: `gcd(0, n) = n` and `gcd(n, n) = n`.

## Input format

```
Line 1: A B — two non-negative integers separated by a space
```

## Oracle

- A=48, B=18  → `6`
- A=100, B=75 → `25`
- A=0, B=5    → `5`
- A=7, B=7    → `7`

## Approach hint

Iterative form: `while b != 0` do `tmp = b; b = a mod b; a = tmp`. Return `a`.
Use `parse_int_tok` to read the two values from the first input line.
