# LC-074 Peak Element via Binary Search

**Category**: Binary Search
**Difficulty**: Medium

## Algorithm

A peak element in an integer array is one that is strictly greater than its
neighbors. Boundary elements are compared to only one neighbor. Given that
adjacent elements are always distinct, find any one peak element's index. The
solution must run in O(log N) time.

The key insight: if nums[mid] < nums[mid+1], the right half must contain a
peak (you can always keep going right until you reach a peak). If
nums[mid] > nums[mid+1], the left half (including mid) must contain a peak.
This allows a binary search to converge to a peak index without checking all
elements.

## Input format

```
Line 1: N (number of elements, N >= 1)
Line 2: N space-separated integers (all adjacent pairs distinct)
```

## Oracle

- N=5, [1,2,3,1,0] → `2` (index of value 3)
- N=4, [1,2,1,3] → `1` or `3` (any valid peak index)
- N=1, [7] → `0`

## Approach hint

Use lo=0, hi=N-1. At each step, if nums[mid] < nums[mid+1], set lo=mid+1,
else set hi=mid. When lo==hi, that is the peak index. Output lo. For the
multi-peak case, the oracle accepts any valid peak index — fix the output
to the deterministic result of this specific binary search algorithm.
