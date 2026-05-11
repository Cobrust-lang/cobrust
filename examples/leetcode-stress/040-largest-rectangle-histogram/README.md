# LC-040 Largest Rectangle in a Histogram

**Category**: Stack / Queue
**Difficulty**: Medium

## Algorithm

Given an array of N non-negative integers representing the heights of bars in
a histogram (each bar has width 1), find the area of the largest axis-aligned
rectangle that fits within the histogram.

The classic O(N) solution uses a monotone-increasing stack of indices. When
a bar shorter than the stack top is encountered, bars are popped and their
contribution is computed: for each popped bar at index i with height h, the
rectangle extends from the current stack top + 1 to the current position - 1,
giving width `= current_index - stack_top_after_pop - 1`. An artificial
sentinel bar of height 0 appended at the end flushes all remaining stack
entries.

## Input format

```
Line 1: N
Line 2: N space-separated non-negative integers (bar heights)
```

## Oracle

N=6, [2, 1, 5, 6, 2, 3] → `10`

## Approach hint

Emulate the stack with a list and a top cursor. Store bar indices. After the
loop (including the sentinel flush), track the running maximum area across
all pops.
