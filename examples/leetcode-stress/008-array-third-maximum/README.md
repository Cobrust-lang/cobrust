# LC-008 Array Third Maximum

**Category**: Arrays
**Difficulty**: Easy

## Algorithm

Given N integers, find the third distinct maximum value. If fewer than three
distinct values exist, output the overall maximum instead.

One approach: track the top three distinct values seen so far (initializing
with a sentinel indicating "not yet seen"). For each input element, update
the tracked set if the element is larger than one of the current top-three
and not already present. After processing all elements, check if a third
maximum was actually found.

## Input format

```
Line 1: N
Line 2: N space-separated integers
```

## Oracle

- N=3, values `[3, 2, 1]` → `1`
- N=4, values `[2, 2, 3, 1]` → `1`
- N=2, values `[1, 2]` → `2` (fewer than 3 distinct, output max)

## Approach hint

Maintain three ranked slots (first, second, third distinct max); scan once and
update slots; fall back to first if third slot is unfilled.
