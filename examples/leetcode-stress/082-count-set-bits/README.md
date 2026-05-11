# LC-082 Count Set Bits (Hamming Weight)

**Category**: Bit Manipulation
**Difficulty**: Easy

## Algorithm

Given a non-negative 32-bit integer, count how many bits in its binary
representation are set to 1. This quantity is sometimes called the popcount
or Hamming weight of the number.

One approach repeatedly checks the least significant bit and shifts right:
while n > 0, add (n & 1) to the count, then set n = n >> 1. Alternatively,
the Brian Kernighan trick repeatedly clears the lowest set bit with n = n & (n-1)
and counts how many times this is done until n becomes zero. The latter runs
in O(k) time where k is the number of set bits, rather than O(32).

## Input format

```
Line 1: N (a non-negative integer treated as unsigned 32-bit)
```

## Oracle

- N=11 (binary 1011) → `3`
- N=128 (binary 10000000) → `1`
- N=0 → `0`

## Approach hint

Use the shift-and-mask loop: count = 0, while n > 0: count += n & 1, n >>= 1.
In Cobrust with integer division: n_shifted = n / 2, bit = n - (n_shifted * 2).
Use the ADR-0044 surface available. Output the count.
