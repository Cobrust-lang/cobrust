# LC-064 House Robber Linear

**Category**: Dynamic Programming
**Difficulty**: Easy

## Algorithm

Given a row of houses where each house contains a certain amount of money,
find the maximum total you can collect under the constraint that you cannot
take from two directly adjacent houses. Any non-adjacent subset of houses
is valid; find the subset with the maximum sum.

The DP recurrence captures the optimal choice at each house: either skip
this house (keeping the best from the previous position) or rob it (adding
its value to the best result two positions back). Let prev2 = best up to
two houses ago, prev1 = best up to one house ago. For each house: new_best =
max(prev1, prev2 + current). Shift prev2 = prev1, prev1 = new_best.

## Input format

```
Line 1: N (number of houses)
Line 2: N space-separated non-negative integers (amounts)
```

## Oracle

- N=4, [2,7,9,3] → `11` (rob index 0 + index 2)
- N=3, [5,1,5] → `10`
- N=1, [100] → `100`

## Approach hint

No array needed; track two rolling variables. Handle N=1 as a special case
(return the single value). For N>=2, initialize prev2=nums[0], prev1=max(nums[0],
nums[1]), then iterate from index 2.
