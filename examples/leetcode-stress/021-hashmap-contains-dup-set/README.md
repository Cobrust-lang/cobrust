# LC-021 Hash Map Contains Duplicate via Set Emulation

**Category**: Hash Maps
**Difficulty**: Easy

## Algorithm

Given N integers, determine whether any value appears more than once. Output
`true` if a duplicate exists, `false` otherwise. This is a classic membership-
test problem that a hash set would solve in O(N) expected time.

Because Cobrust does not yet have a built-in hash set, emulate one using a
sorted parallel list of seen values. Before recording each new element, scan
the seen list for a match. If found, output `true` immediately. Otherwise,
append the value. This approach is O(N²) in time but exercises the pattern of
tracking seen values — the key hash-map idiom — using only the available
list primitives. This mismatch between ideal and implemented approach IS the
test signal: it measures the cost of the missing hash primitive.

## Input format

```
Line 1: N
Line 2: N space-separated integers
```

## Oracle

- N=4, values `[1, 2, 3, 1]` → `true`
- N=3, values `[1, 2, 3]` → `false`

## Approach hint

Maintain a "seen" list; linear search before each insert; early `true` on
first hit — this deliberately exercises the cost of missing hash-set support.
