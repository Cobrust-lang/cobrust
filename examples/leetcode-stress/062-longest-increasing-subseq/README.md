# LC-062 Longest Increasing Subsequence

**Category**: Dynamic Programming
**Difficulty**: Medium

## Algorithm

Given a sequence of integers, determine the length of its longest strictly
increasing subsequence. A subsequence preserves relative order but does not
need to be contiguous. The goal is to find the maximum number of elements
that can be picked such that each successive pick is strictly greater than
the previous.

The standard O(n^2) DP approach allocates a table `dp[i]` = length of the
longest increasing subsequence ending at index i. For each i, scan all j < i:
if `nums[j] < nums[i]`, consider extending the subsequence ending at j by
appending nums[i]. Track the global maximum across all positions.

## Input format

```
Line 1: N (number of elements)
Line 2: N space-separated integers
```

## Oracle

- N=6, [3,10,2,1,20,9] → `3` (e.g. 3,10,20 or 1,9,20 etc.)
- N=5, [5,4,3,2,1] → `1` (strictly decreasing)
- N=1, [42] → `1`

## Approach hint

Initialize all dp entries to 1 (every element alone is a subsequence of
length 1). For each i from 1 to N-1, loop j from 0 to i-1: if nums[j] <
nums[i] and dp[j]+1 > dp[i], update dp[i]. Answer is max over all dp[i].
