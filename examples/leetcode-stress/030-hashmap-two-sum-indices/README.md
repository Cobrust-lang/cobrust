# LC-030 Hash Map Two Sum with Index Tracking

**Category**: Hash Maps
**Difficulty**: Medium

## Algorithm

Given N integers and a target sum, find the two 0-based indices i and j
(i < j) such that the values at those indices add up to the target. Exactly
one such pair is guaranteed. Output i then j, one per line.

This variant emphasizes the hash-map lookup pattern: for each element at
position j, compute the complement `target - nums[j]` and check whether the
complement has been seen before. If yes, output the index where the complement
was stored and the current index j. Otherwise, record the current value and
index.

Emulate the value-to-index map with two parallel lists (values and their
indices), using linear search for lookups. This deliberately exposes the
O(N²) cost of emulated hash lookup — a key language-gap signal for the B1
bucket.

## Input format

```
Line 1: N
Line 2: N space-separated integers
Line 3: target sum
```

## Oracle

- N=4, values `[2, 7, 11, 15]`, target=9 → `0\n1\n`
- N=3, values `[3, 2, 4]`, target=6 → `1\n2\n`

## Approach hint

Emulated value-to-index map via parallel lists; for each element check if
complement is already stored; emit both indices on match.
