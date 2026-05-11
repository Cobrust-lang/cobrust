# LC-001 Array Running Sum

**Category**: Arrays
**Difficulty**: Easy

## Algorithm

Given N integers in sequence, produce a new sequence of the same length where
position i holds the sum of all original elements from index 0 through i
inclusive. This is the prefix-sum (also called running sum) transformation.

A single left-to-right pass suffices: maintain a running accumulator, add each
element to it, and record the result. No sorting, no lookups — just one linear
scan.

## Input format

```
Line 1: N
Line 2: N space-separated integers
```

## Oracle

- N=5, values `[1, 2, 3, 4, 5]` → `1\n3\n6\n10\n15\n`

## Approach hint

Linear scan with a running accumulator; emit each accumulated total via
`print_int`.
