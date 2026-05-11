# LC-017 Two Pointers Sort Colors (Three-Way Partition)

**Category**: Two Pointers
**Difficulty**: Medium

## Algorithm

Given N integers where each value is 0, 1, or 2 (representing three categories,
often called "red", "white", and "blue"), sort the array so all 0s come first,
then all 1s, then all 2s. Output the sorted sequence one element per line.

The classic three-way partition (Dutch National Flag algorithm) uses three
pointers: `lo` tracks the boundary of 0s (left), `hi` tracks the boundary of
2s (right), and `mid` is the current element under inspection. Examine the
element at `mid`: if it is 0, swap with `lo` and advance both `lo` and `mid`;
if it is 2, swap with `hi` and decrement `hi` (do not advance `mid` — the
swapped element is unexamined); if it is 1, just advance `mid`.

## Input format

```
Line 1: N
Line 2: N space-separated integers, each 0, 1, or 2
```

## Oracle

- N=6, values `[2, 0, 2, 1, 1, 0]` → `0\n0\n1\n1\n2\n2\n`

## Approach hint

Three-way partition with `lo`, `mid`, `hi` cursors; terminate when `mid > hi`.
