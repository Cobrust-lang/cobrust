# LC-083 Counting Bits DP

**Category**: Bit Manipulation
**Difficulty**: Easy

## Algorithm

For each integer from 0 to N inclusive, compute the number of 1-bits in
its binary representation, and output all N+1 results. While a naive
approach calls popcount for each integer in O(N log N) total, a DP trick
achieves O(N).

The recurrence is based on the observation that the bit count of an integer
i equals the bit count of i with its lowest set bit removed, plus 1. The
lowest set bit of i is i & (-i). Removing it gives i - (i & (-i)). So:
dp[i] = dp[i - (i & -i)] + 1, with dp[0] = 0. This reduces each step to
a single lookup into already-computed values.

## Input format

```
Line 1: N (upper bound, 0-indexed)
```

## Oracle

- N=4 → `0\n1\n1\n2\n1\n`
- N=0 → `0\n`
- N=3 → `0\n1\n1\n2\n`

## Approach hint

Allocate dp of size N+1. dp[0] = 0. For i in 1..N: compute low_bit = i & (-i)
using integer arithmetic (low_bit = i - ((i / low_power_of_2) * low_power_of_2)).
In simpler form: dp[i] = dp[i/2] + (i mod 2) also works and is equivalent.
Print each dp[i] on its own line.
