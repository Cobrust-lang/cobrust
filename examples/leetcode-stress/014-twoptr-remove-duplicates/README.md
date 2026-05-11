# LC-014 Two Pointers Remove Duplicates In Place

**Category**: Two Pointers
**Difficulty**: Easy

## Algorithm

Given N integers in a sorted array, remove duplicate values such that each
distinct value appears at most once. Output first the count of unique values
(call it K), then the K unique values in sorted order, one per line.

The two-pointer (or write-cursor) approach works in-place on the sorted array:
maintain a write pointer starting at 1 (the first output slot after the
mandatory first element). Scan with a read pointer from index 1 onward. When
the current element differs from the previous unique element, copy it to the
write position and advance the write pointer.

## Input format

```
Line 1: N
Line 2: N space-separated integers sorted ascending
```

## Oracle

- N=5, values `[1, 1, 2, 2, 3]` → K=3, then `1\n2\n3\n`

## Approach hint

Write-cursor compaction on sorted input; advance write cursor only when the
current element differs from the last written value.
