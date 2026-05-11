# LC-007 Array Sorted Intersection

**Category**: Arrays
**Difficulty**: Easy

## Algorithm

Given two sorted integer arrays, find their intersection — all values that
appear in both arrays — and output them in non-decreasing order, with each
common value appearing as many times as it appears in both (the minimum of the
two counts).

Because both arrays are already sorted, a two-pointer merge approach is
efficient: advance the pointer on whichever array has the smaller current value,
and emit when both pointers point to equal values.

## Input format

```
Line 1: M N
Line 2: M space-separated integers (sorted ascending)
Line 3: N space-separated integers (sorted ascending)
```

## Oracle

- M=4 N=5, `[1, 2, 2, 1]` and `[2, 2]` → but as sorted inputs: `[1, 1, 2, 2]` and `[2, 2]` → `2\n2\n`
- M=3 N=4, `[1, 3, 5]` and `[1, 2, 3, 6]` → `1\n3\n`

## Approach hint

Dual-cursor sweep on both sorted arrays; advance the smaller cursor; emit on
equality.
