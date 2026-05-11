# LC-003 Array Find Disappeared Numbers

**Category**: Arrays
**Difficulty**: Easy

## Algorithm

Given N integers where every value is in the range [1, N], some values may
appear multiple times while others appear zero times. Find all values in
[1, N] that do not appear anywhere in the input, and output them in ascending
order.

One straightforward approach: build a boolean presence array of size N+1 (all
false initially), then scan through the input marking each seen value. Finally,
scan positions 1 through N and output each position whose presence flag remains
false.

## Input format

```
Line 1: N
Line 2: N space-separated integers, each in range [1, N]
```

## Oracle

- N=8, values `[4, 3, 2, 7, 8, 2, 3, 1]` → `5\n6\n`

## Approach hint

Boolean presence array of size N+1; two linear scans — mark then report missing.
