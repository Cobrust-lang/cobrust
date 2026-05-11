# LC-023 Hash Map Majority Element

**Category**: Hash Maps
**Difficulty**: Easy

## Algorithm

Given N integers where one value is guaranteed to appear more than N/2 times,
find and output that majority value.

A frequency-count approach emulated with parallel lists: maintain two lists
of equal length — one recording distinct values seen so far, and one recording
their counts. For each input element, search the values list for a match;
if found, increment the corresponding count; if not found, append a new entry
with count 1. After processing all elements, scan the counts list for the
position with the largest count and output the corresponding value.

This exercises the core hash-map "count occurrences" pattern using only list
primitives, surfacing the cost of the missing hash primitive as a language-gap
signal.

## Input format

```
Line 1: N
Line 2: N space-separated integers
```

## Oracle

- N=5, values `[3, 2, 3, 1, 3]` → `3`
- N=3, values `[2, 2, 1]` → `2`

## Approach hint

Parallel value/count lists; linear search + increment pattern; emit the value
with the highest count.
