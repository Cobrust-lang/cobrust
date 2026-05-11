# LC-029 Hash Map Subarray Sum Equals K

**Category**: Hash Maps
**Difficulty**: Medium

## Algorithm

Given N integers and a target value K, count the total number of contiguous
subarrays whose elements sum to exactly K. A subarray is any contiguous portion
of the original array (length >= 1). Output the count.

The efficient approach uses prefix sums and a frequency table: compute the
running prefix sum as you scan left to right. At each position, the number of
subarrays ending here with sum K equals the number of previous prefix sums
equal to `current_prefix_sum - K`. Maintain a count-table of prefix sums seen
so far (initialized with prefix_sum=0 count=1 for the empty prefix).

Emulate the prefix-sum frequency table with parallel value/count lists and
linear search (same approach as majority-element emulation).

## Input format

```
Line 1: N
Line 2: N space-separated integers
Line 3: K (the target sum)
```

## Oracle

- N=5, values `[1, 1, 1, 1, 1]`, K=2 → `4`
- N=4, values `[1, 2, 3, 0]`, K=3 → `3`

## Approach hint

Running prefix sum + emulated frequency table of prefix sums; count of
`(prefix - K)` occurrences at each step.
