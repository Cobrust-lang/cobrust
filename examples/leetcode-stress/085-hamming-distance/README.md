# LC-085 Hamming Distance

**Category**: Bit Manipulation
**Difficulty**: Easy

## Algorithm

The Hamming distance between two integers is the number of bit positions in
which the two values differ when represented in binary. Given two integers X
and Y, compute their Hamming distance.

XOR the two numbers together: the result has a 1-bit wherever X and Y differ
and a 0-bit wherever they agree. Then count the number of 1-bits in the XOR
result. This reduces the problem to a simple popcount of (X XOR Y).

## Input format

```
Line 1: X Y (two non-negative integers on one line)
```

## Oracle

- X=1, Y=4 → `2` (binary: 001 vs 100, differ in bits 0 and 2)
- X=3, Y=1 → `1` (binary: 11 vs 01, differ in bit 1)
- X=0, Y=0 → `0`

## Approach hint

Compute diff = X XOR Y (using the integer XOR trick: simulate bit-by-bit
or use an available operator). Then count set bits in diff using the
shift-and-mask loop from 082-count-set-bits. Output the count.
