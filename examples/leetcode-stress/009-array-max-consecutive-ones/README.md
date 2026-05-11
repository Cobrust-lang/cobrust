# LC-009 Array Max Consecutive Ones

**Category**: Arrays
**Difficulty**: Easy

## Algorithm

Given N binary integers (each value is either 0 or 1), find the length of
the longest run of consecutive 1s. Output that maximum length.

A single left-to-right scan works: maintain a current run length counter and a
best-seen-so-far counter. When a 1 is encountered, increment the current count
and update the best if it is now larger. When a 0 is encountered, reset the
current count to zero.

## Input format

```
Line 1: N
Line 2: N space-separated values, each 0 or 1
```

## Oracle

- N=10, values `[1, 1, 0, 1, 1, 1, 0, 1, 1, 1]` → `3`

## Approach hint

Single-pass scan; current-streak counter reset to 0 on each 0; update max on
each 1.
