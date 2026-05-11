# LC-015 Two Pointers Remove Element

**Category**: Two Pointers
**Difficulty**: Easy

## Algorithm

Given N integers and a target value to remove, output all elements that are
not equal to the target, in their original relative order. Print the count of
remaining elements first, then each remaining element one per line.

A single write-cursor pass handles this: iterate through the array; whenever
the current element does not equal the target value, write it to the output
position and advance both the read and write cursors. When it does equal the
target, only advance the read cursor.

## Input format

```
Line 1: N
Line 2: N space-separated integers
Line 3: the value to remove
```

## Oracle

- N=4, values `[3, 2, 2, 3]`, remove=3 → count=2, then `2\n2\n`

## Approach hint

Write-cursor compaction; skip elements equal to the target; count remaining.
