# LC-097 Gas Station Circular

**Category**: Greedy
**Difficulty**: Medium

## Algorithm

There are N gas stations arranged in a circle. Station i provides `gas[i]`
units of fuel and costs `cost[i]` units to travel from station i to the next.
Starting with an empty tank, find the index of the station from which you can
complete the full circular tour, or report -1 if no such station exists.

Two key observations power the greedy solution:

1. **Global feasibility**: if the total gas across all stations is less than
   the total cost, no starting point can complete the tour. Check this first.

2. **Local reset**: simulate a single greedy pass. Maintain a running tank
   balance (`tank += gas[i] - cost[i]`). When the tank goes negative, the
   current starting point and every station visited since it cannot be the
   answer (because reaching a negative-balance station from any earlier start
   would have the same or worse cumulative deficit). Reset the tank to zero and
   set the candidate start to `i + 1`.

If total gas >= total cost, the greedy candidate found in the pass is guaranteed
to be the unique valid starting station.

## Input format

```
Line 1: N
Line 2: N space-separated gas values
Line 3: N space-separated cost values
```

## Oracle

- N=5, gas=[1,2,3,4,5], cost=[3,4,5,1,2] → `3`
- N=3, gas=[2,3,4], cost=[3,4,3]          → `2`
- N=3, gas=[1,2,3], cost=[3,4,5]          → `-1`

## Approach hint

Two-pass approach: first check if sum(gas) >= sum(cost). If not, output -1. Then
do a second scan with `tank` accumulator and `start` candidate index; when tank
drops below 0, set `start = i + 1` and reset `tank = 0`. Output `start`.
