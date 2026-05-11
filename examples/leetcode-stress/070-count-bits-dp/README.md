# LC-070 Count Bits DP

**Category**: Dynamic Programming
**Difficulty**: Easy

## Algorithm

Given a non-negative integer N, for every integer in the range 0 to N
inclusive, count how many 1-bits its binary representation contains. Output
all N+1 counts in order, one per line.

The key DP insight is that the number of 1-bits in i equals the number of
1-bits in (i >> 1) plus the least significant bit of i. In other words:
dp[i] = dp[i / 2] + (i % 2). Since i/2 was already computed earlier in the
iteration, the table fills in O(N) time without calling any bit-counting
subroutine.

## Input format

```
Line 1: N (0-indexed upper bound, N >= 0)
```

## Oracle

- N=2 → `0\n1\n1\n`
- N=5 → `0\n1\n1\n2\n1\n2\n`
- N=0 → `0\n`

## Approach hint

Allocate a dp list of size N+1. dp[0] = 0. For i from 1 to N:
dp[i] = dp[i/2] + (i mod 2). Print each entry on its own line.
