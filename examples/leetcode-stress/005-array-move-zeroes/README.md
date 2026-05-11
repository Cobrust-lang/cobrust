# LC-005 Array Move Zeroes

**Category**: Arrays
**Difficulty**: Easy

## Algorithm

Given N integers, rearrange them in-place so that all zero values are moved
to the end of the sequence while the relative order of all non-zero values is
preserved. Output the rearranged sequence, one element per line.

The classic two-pointer approach works: maintain a "write cursor" starting at
index 0. Scan from left to right; whenever a non-zero element is found, copy
it to the write cursor position and advance the cursor. After the scan, fill
all remaining positions from the write cursor to N-1 with zeros.

## Input format

```
Line 1: N
Line 2: N space-separated integers
```

## Oracle

- N=5, values `[0, 1, 0, 3, 12]` → `1\n3\n12\n0\n0\n`

## Approach hint

Write-cursor compaction: copy non-zeros forward, then zero-fill the tail.
