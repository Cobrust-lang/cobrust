# LC-069 Pascal Triangle Row

**Category**: Dynamic Programming
**Difficulty**: Easy

## Algorithm

Pascal's triangle is constructed by placing 1s on both edges of each row and
filling interior positions as the sum of the two values directly above.
Given a row index R (0-based), output the R-th row of Pascal's triangle.

Rather than building the full triangle, the R-th row can be computed
iteratively. Start with row = [1]. For each step from 1 to R, compute the
next row by inserting the sum of adjacent pairs: new[i] = old[i-1] + old[i]
for 0 < i < len(old), with 1s at both ends. After R steps, output the row.

## Input format

```
Line 1: R (0-based row index, R >= 0)
```

## Oracle

- R=0 → `1`
- R=3 → `1 3 3 1`
- R=5 → `1 5 10 10 5 1`

## Approach hint

Use two alternating lists. For each expansion step, the new list has length
one greater than the old list. All positions except the first and last are
computed as the sum of adjacent elements in the old list. Output all values
on a single space-separated line with a trailing newline.
