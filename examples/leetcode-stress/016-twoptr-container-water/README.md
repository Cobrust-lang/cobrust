# LC-016 Two Pointers Container With Most Water

**Category**: Two Pointers
**Difficulty**: Medium

## Algorithm

Given N positive integers representing the heights of vertical bars at each
position, find the pair of bars that can trap the most water between them when
treated as walls of a container. The water volume between bars at positions
i and j (i < j) is `(j - i) * min(height[i], height[j])`. Output the maximum
such volume.

The two-pointer strategy is efficient: place one cursor at the leftmost bar and
one at the rightmost. Compute the current container area. Then move the cursor
at the shorter bar inward (since moving the taller bar can never increase the
height). Track the maximum area seen across all positions.

## Input format

```
Line 1: N
Line 2: N space-separated positive integers (bar heights)
```

## Oracle

- N=9, heights `[1, 8, 6, 2, 5, 4, 8, 3, 7]` → `49`

## Approach hint

Dual inward cursors; always advance the side with the shorter bar; track running
maximum area.
