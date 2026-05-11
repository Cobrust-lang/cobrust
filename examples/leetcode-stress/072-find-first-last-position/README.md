# LC-072 Find First and Last Position

**Category**: Binary Search
**Difficulty**: Medium

## Algorithm

Given a sorted array of integers that may contain duplicates, find the
starting and ending positions of a given target value. If the target is not
present in the array, output -1 -1. Both bounds must be found in O(log N)
time, requiring two separate binary searches.

The first search finds the leftmost occurrence: after finding target at mid,
continue searching left (hi = mid - 1) to see if an earlier occurrence
exists. The second search finds the rightmost occurrence: after finding
target at mid, continue searching right (lo = mid + 1). Each search runs
standard binary search with a "keep best so far" variable.

## Input format

```
Line 1: N (number of elements)
Line 2: N space-separated integers (sorted)
Line 3: target
```

## Oracle

- N=6, [5,7,7,8,8,10], target=8 → `3 4`
- N=6, [5,7,7,8,8,10], target=6 → `-1 -1`
- N=0, target=0 → `-1 -1`

## Approach hint

Write two helper functions: find_left(nums, n, target) and
find_right(nums, n, target), each performing a binary search that records
the best matching index found. Combine their outputs on one space-separated
line.
