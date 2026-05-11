# LC-073 Search Insert Position

**Category**: Binary Search
**Difficulty**: Easy

## Algorithm

Given a sorted array of distinct integers and a target, find the position
where the target is, or the position where it would need to be inserted to
keep the array sorted. No duplicates exist in the array. The answer is always
well-defined: it is the index of the first element >= target.

This is a classic lower-bound binary search. Maintain lo=0, hi=N (note: hi=N
not hi=N-1, to handle insertion at the end). While lo < hi, compute mid. If
nums[mid] < target, move lo = mid+1. Otherwise move hi = mid. At termination,
lo == hi is the insertion point (also the index if the target is present).

## Input format

```
Line 1: N (number of elements)
Line 2: N space-separated integers (sorted, distinct)
Line 3: target
```

## Oracle

- N=4, [1,3,5,6], target=5 → `2`
- N=4, [1,3,5,6], target=2 → `1`
- N=4, [1,3,5,6], target=7 → `4`

## Approach hint

Use the open-right interval variant: lo=0, hi=N. Binary search with the
condition nums[mid] < target to decide direction. When lo==hi the loop ends
and lo is the answer.
