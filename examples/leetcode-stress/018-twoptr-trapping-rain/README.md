# LC-018 Two Pointers Trapping Rain Water

**Category**: Two Pointers
**Difficulty**: Medium

## Algorithm

Given N non-negative integers representing the elevation of a terrain at each
position, compute the total units of rain water that can be trapped between
the bars after rain. Water at position i is determined by the minimum of the
tallest bar to the left and tallest bar to the right of i, minus the height
at i (if positive).

The two-pointer approach avoids precomputing separate left-max and right-max
arrays: maintain a left cursor at index 0 and a right cursor at N-1, along
with the running left-max and right-max values. If left-max is less than
right-max, the water contribution at the left cursor is `left_max - height[left]`
(guaranteed non-negative since right provides a sufficient right wall). Advance
the left cursor. Otherwise apply the symmetric logic on the right side.

## Input format

```
Line 1: N
Line 2: N space-separated non-negative integers
```

## Oracle

- N=12, heights `[0, 1, 0, 2, 1, 0, 1, 3, 2, 1, 2, 1]` → `6`

## Approach hint

Dual inward cursors with running left-max and right-max; add trapped water at
the side with the smaller max.
