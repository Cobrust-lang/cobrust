# LC-006 Array Plus One

**Category**: Arrays
**Difficulty**: Easy

## Algorithm

A non-negative integer is represented as an array of its decimal digits, most
significant digit first. Add exactly 1 to this number and output the resulting
digit array, one digit per line.

Handle the carry correctly: start from the least significant digit (last
position), add one, propagate carry left as long as a digit overflows from 9
to 0. If all digits overflow (e.g. 999 → 1000), the result needs an extra
leading digit 1.

## Input format

```
Line 1: N
Line 2: N space-separated single digits (0–9), most significant first
```

## Oracle

- N=3, digits `[1, 2, 3]` → `1\n2\n4\n`
- N=3, digits `[9, 9, 9]` → `1\n0\n0\n0\n`

## Approach hint

Scan right-to-left adding carry; if carry remains after index 0, prepend 1 to
the output.
