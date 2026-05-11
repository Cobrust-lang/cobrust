# LC-077 Minimum Ship Capacity (Binary Search on Answer)

**Category**: Binary Search
**Difficulty**: Medium

## Algorithm

A conveyor belt carries packages with given weights, and they must all be
shipped within D days in their original order. Each day's shipment is a
contiguous prefix of the remaining packages. Find the minimum weight capacity
the ship must have to complete all shipments within D days.

This is a "binary search on the answer" pattern. The minimum feasible
capacity is max(weights) (must fit the heaviest package). The maximum is
sum(weights) (all in one day). Binary search for the smallest C in [max, sum]
such that packages can be shipped in <= D days using a greedy day-assignment:
fill each day greedily until adding the next package would exceed capacity.

## Input format

```
Line 1: N D (number of packages, number of days)
Line 2: N space-separated package weights
```

## Oracle

- N=10, D=5, weights=[1,2,3,4,5,6,7,8,9,10] → `15`
- N=6, D=3, weights=[3,2,2,4,1,4] → `6`
- N=1, D=1, weights=[5] → `5`

## Approach hint

Implement a helper `can_ship(weights, N, capacity, D)` that simulates the
greedy assignment: count days needed when each day loads as many packages as
possible without exceeding capacity. Binary search: lo=max_weight,
hi=total_weight, shrink on feasibility.
