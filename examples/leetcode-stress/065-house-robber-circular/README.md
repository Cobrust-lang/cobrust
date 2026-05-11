# LC-065 House Robber Circular

**Category**: Dynamic Programming
**Difficulty**: Medium

## Algorithm

This is a variation of the linear house-robber problem where the houses are
arranged in a circle: the first and last house are considered adjacent, so
you cannot rob both. The key insight is to decompose the circular constraint
into two independent linear sub-problems.

Run the linear house-robber algorithm twice: once on the subarray excluding
the last house (indices 0..N-2), and once on the subarray excluding the first
house (indices 1..N-1). Take the maximum of both results. This works because
in any optimal solution, at most one of the first and last house can be chosen.

## Input format

```
Line 1: N (number of houses, N >= 1)
Line 2: N space-separated non-negative integers
```

## Oracle

- N=3, [2,3,2] → `3` (can only take one of the boundary houses)
- N=4, [1,2,3,1] → `4`
- N=1, [5] → `5`

## Approach hint

Write a helper function `rob_linear(list, start, end)` that runs the standard
O(N) linear DP on the subarray from start to end inclusive. Then return
max(rob_linear(0, N-2), rob_linear(1, N-1)). Handle N=1 separately.
