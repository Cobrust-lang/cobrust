# LC-090 Enumerate Subsets via Bitmask

**Category**: Bit Manipulation
**Difficulty**: Medium

## Algorithm

Given a list of N distinct non-negative single-digit integers (0..9), use
bitmask enumeration to generate and print all non-empty subsets of the list,
one subset per line. For a list of N elements, there are 2^N subsets (including
empty); print 2^N - 1 non-empty ones. Print each subset's elements in their
original order, space-separated.

The bitmask approach: iterate mask from 1 to 2^N - 1. For each mask, bit i
set means element i is included. Print included elements in original index
order. To check if bit i is set in mask: (mask / 2^i) mod 2 == 1.

## Input format

```
Line 1: N (number of elements, 1 <= N <= 4 for tractability)
Line 2: N space-separated single-digit integers
```

## Oracle

- N=2, [1,2] → 3 non-empty subsets: `1\n2\n1 2\n`
- N=3, [1,2,3] → 7 non-empty subsets in mask order
- N=1, [5] → `5\n`

## Approach hint

Pre-compute powers of 2: pow2[i] = 2^i for i in 0..N. Outer loop mask from
1 to pow2[N]-1. Inner loop i from 0..N-1: if (mask / pow2[i]) mod 2 == 1,
this element is in the subset. Print space-separated using print_no_nl for
all but the last, then print the last with print_int.
