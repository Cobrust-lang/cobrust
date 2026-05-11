# LC-002 Array Contains Duplicate

**Category**: Arrays
**Difficulty**: Easy

## Algorithm

Given N integers, determine whether any value appears more than once. Output
`true` if a duplicate exists, `false` if all values are distinct.

The simplest correct approach uses a nested scan: for each element at index i,
check whether the same value appears at any index j > i. If a match is found,
report `true` immediately. If the outer loop finishes with no match, report
`false`. This is O(N²) in time but O(1) in extra space.

## Input format

```
Line 1: N
Line 2: N space-separated integers
```

## Oracle

- N=4, values `[1, 2, 3, 1]` → `true`
- N=4, values `[1, 2, 3, 4]` → `false`

## Approach hint

Nested index scan; early-return `true` on first matching pair, fall through to
`false`.
