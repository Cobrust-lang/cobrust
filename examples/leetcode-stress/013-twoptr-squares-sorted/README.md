# LC-013 Two Pointers Squares of Sorted Array

**Category**: Two Pointers
**Difficulty**: Easy

## Algorithm

Given N integers sorted in non-decreasing order (which may include negative
values), produce a new array containing the squares of each element, also in
non-decreasing order. Output the result one element per line.

Because the input may contain negative numbers, squaring can make the smallest
(most negative) values become the largest. A two-pointer approach exploits the
sorted property: place one cursor at the left end and one at the right end.
Compare the absolute values of both; the larger square goes into the result
from the rightmost unfilled position. Advance the cursor with the larger
absolute value inward. Repeat until both cursors meet.

## Input format

```
Line 1: N
Line 2: N space-separated integers sorted ascending
```

## Oracle

- N=5, values `[-4, -1, 0, 3, 10]` → `0\n1\n9\n16\n100\n`

## Approach hint

Dual inward cursors fill output right-to-left; compare absolute values to
determine which side contributes next.
