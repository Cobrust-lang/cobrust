# LC-011 Two Pointers Reverse In Place

**Category**: Two Pointers
**Difficulty**: Easy

## Algorithm

Given N integers in a list, reverse the order of all elements and output the
reversed sequence, one element per line.

The classic two-pointer in-place reversal uses a left cursor starting at index
0 and a right cursor starting at index N-1. Swap the values at both cursors,
then advance left and retreat right. Continue until the two cursors meet or
cross.

## Input format

```
Line 1: N
Line 2: N space-separated integers
```

## Oracle

- N=5, values `[1, 2, 3, 4, 5]` → `5\n4\n3\n2\n1\n`

## Approach hint

Inward two-cursor swap loop; terminate when left >= right.
