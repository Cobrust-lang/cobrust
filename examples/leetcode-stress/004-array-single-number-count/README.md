# LC-004 Array Single Number (Count Method)

**Category**: Arrays
**Difficulty**: Easy

## Algorithm

Given N integers where every value appears exactly twice except for one value
that appears exactly once, find and output that lone value.

Rather than using bitwise tricks, this solution uses a counting approach: for
each candidate value, scan the entire array and count its occurrences. The first
value whose count equals 1 is the answer. This is O(N²) in time but relies
only on basic iteration — no hash structures or bit operations needed.

## Input format

```
Line 1: N (always odd)
Line 2: N space-separated integers
```

## Oracle

- N=5, values `[2, 2, 1, 4, 4]` → `1`
- N=3, values `[3, 1, 3]` → `1`

## Approach hint

Outer scan over each position; inner scan counts occurrences; emit the element
whose count is exactly 1.
