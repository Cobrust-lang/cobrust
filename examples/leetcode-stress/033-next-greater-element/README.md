# LC-033 Next Greater Element

**Category**: Stack / Queue
**Difficulty**: Easy

## Algorithm

Given an array of N distinct integers, for each position find the first
element to the right that is strictly greater. If no such element exists,
the answer for that position is -1.

A monotone-decreasing stack solves this in O(N) time. Scan left-to-right.
Before pushing the current element, pop all stack entries whose values are
less than the current element — the current element is the "next greater"
for each of those popped entries. After the full scan, any positions still
on the stack have no greater element to the right and get answer -1.

## Input format

```
Line 1: N
Line 2: N space-separated integers
```

## Oracle

N=5, [2, 1, 5, 3, 4] → `5 5 -1 4 -1`

## Approach hint

Emulate the stack using a list and a top cursor. The stack stores indices,
not values, so you can record the answer for each popped index. At the end
of the scan, any remaining indices are assigned -1.
