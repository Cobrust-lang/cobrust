# LC-010 Array Prefix Product

**Category**: Arrays
**Difficulty**: Medium

## Algorithm

Given N integers, compute for each position i the product of all elements
except the one at position i, and output the results. You must not use
division.

The standard approach builds two auxiliary arrays: a left-product array where
`left[i]` holds the product of all elements to the left of i, and a
right-product array where `right[i]` holds the product of all elements to the
right of i. The answer at position i is `left[i] * right[i]`. Both arrays are
computed in a single pass each, so the total time is O(N).

## Input format

```
Line 1: N
Line 2: N space-separated integers
```

## Oracle

- N=4, values `[1, 2, 3, 4]` → `24\n12\n8\n6\n`

## Approach hint

Two-pass prefix/suffix product accumulation; multiply corresponding entries for
the result at each position.
