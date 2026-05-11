# LC-067 Partition Equal Subset Sum

**Category**: Dynamic Programming
**Difficulty**: Medium

## Algorithm

Given a list of positive integers, determine whether it can be split into
two non-empty groups that have equal sums. This is equivalent to asking: can
a subset of the array sum to exactly half the total sum? If the total sum is
odd, the answer is immediately false.

This is a 0/1 knapsack variant. Let target = total_sum / 2. Build a boolean
DP array `dp[0..target]` where dp[j] is true if some subset sums to j.
Initialize dp[0] = true. For each number n in the input, iterate j from
target down to n: if dp[j-n] is true, set dp[j] = true. Answer is dp[target].

## Input format

```
Line 1: N (number of integers)
Line 2: N space-separated positive integers
```

## Oracle

- N=4, [1,5,11,5] → `true`
- N=3, [1,2,3] → `true`
- N=3, [1,2,5] → `false`

## Approach hint

Use a boolean-style integer list (0/1 values) of size target+1. Traverse
coins in outer loop, target..coin in inner loop. Output "true" if dp[target]
is 1, else "false". Handle odd-sum early exit with a direct "false" print.
