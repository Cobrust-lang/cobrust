# LC-079 Minimum in Rotated Sorted Array

**Category**: Binary Search
**Difficulty**: Easy

## Algorithm

A sorted array of distinct integers has been rotated at some unknown pivot.
Find the minimum element. The minimum element is the only element in the
array whose left neighbor is greater than it (or it is at position 0 if
no rotation occurred).

Binary search can locate the minimum in O(log N). Compare nums[mid] with
nums[hi]. If nums[mid] < nums[hi], the minimum lies in the left half
including mid (hi = mid). If nums[mid] > nums[hi], the minimum lies in the
right half (lo = mid + 1). When lo == hi, that position holds the minimum.

## Input format

```
Line 1: N (number of elements, all distinct)
Line 2: N space-separated integers (rotated sorted)
```

## Oracle

- N=5, [3,4,5,1,2] → `1`
- N=4, [4,5,6,7] → `4` (no rotation)
- N=1, [0] → `0`

## Approach hint

Use lo=0, hi=N-1. Loop while lo < hi. Compute mid=(lo+hi)/2. Compare
nums[mid] with nums[hi] (not nums[lo]) to decide which side the minimum is
on. Converge to lo==hi then return nums[lo].
