# LC-071 Search Rotated Sorted Array

**Category**: Binary Search
**Difficulty**: Medium

## Algorithm

A sorted array of distinct integers has been rotated at some unknown pivot
index, so it looks like [4,5,6,7,0,1,2] rather than [0,1,2,4,5,6,7]. Given
such a rotated array and a target value, find the index of the target or
return -1 if not present. The algorithm must run in O(log N) time.

Standard binary search is adapted by observing that at least one half of the
array around any midpoint is always in strictly sorted order. If the left
portion [lo..mid] is sorted (nums[lo] <= nums[mid]), check whether target
falls in that range; if so, narrow to the left half, otherwise search right.
If the right portion is sorted instead, apply the symmetric logic.

## Input format

```
Line 1: N (number of elements)
Line 2: N space-separated integers (the rotated sorted array)
Line 3: target
```

## Oracle

- N=7, [4,5,6,7,0,1,2], target=0 → `4`
- N=7, [4,5,6,7,0,1,2], target=3 → `-1`
- N=1, [1], target=0 → `-1`

## Approach hint

Use lo=0, hi=N-1 loop. Compute mid=(lo+hi)/2. Branch on whether nums[lo]<=nums[mid]
to determine which half is sorted, then narrow the search window to whichever
half the target could lie in. Return mid if nums[mid]==target.
