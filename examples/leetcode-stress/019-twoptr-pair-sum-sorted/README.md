# LC-019 Two Pointers Pair Sum in Sorted Array

**Category**: Two Pointers
**Difficulty**: Easy

## Algorithm

Given N integers in a sorted (non-decreasing) array and a target sum, find
two 1-based indices i and j (i < j) such that the values at those positions
add up to exactly the target. Exactly one such pair is guaranteed to exist.
Output the two indices one per line.

Because the array is sorted, the two-pointer inward sweep is direct: start
one cursor at index 0 (left) and one at index N-1 (right). If the sum of the
two pointed values equals the target, report and stop. If the sum is too small,
advance the left cursor; if too large, retreat the right cursor.

## Input format

```
Line 1: N
Line 2: N space-separated integers sorted ascending
Line 3: target sum
```

## Oracle

- N=4, values `[2, 7, 11, 15]`, target=9 → `1\n2\n`

## Approach hint

Dual inward cursors; advance left when sum is under target, retreat right when
over; emit 1-based indices on match.
