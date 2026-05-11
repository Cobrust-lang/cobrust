# LC-096 Jump Game Can Reach

**Category**: Greedy
**Difficulty**: Medium

## Algorithm

Given an array of non-negative integers where each element represents the
maximum number of positions you can jump forward from that index, determine
whether it is possible to reach the last index starting from index 0.

The greedy approach tracks the farthest index reachable at any point during a
left-to-right scan. At each index `i`, if `i` exceeds the current farthest
reachable index, it is unreachable — no future positions can help because
everything between the last reachable point and `i` also had zero or insufficient
reach. Otherwise, update the farthest reach to `max(farthest, i + jumps[i])`.
If `farthest` reaches or passes the last index at any point, return true.

This greedy choice works because the optimal strategy is always to extend reach
as far as possible; the specific path taken does not matter.

## Input format

```
Line 1: N
Line 2: N space-separated jump values
```

## Oracle

- N=5, jumps=[2,3,1,1,4] → `true`  (0→1→4 or 0→2→3→4)
- N=5, jumps=[3,2,1,0,4] → `false` (stuck at index 3 which has jump 0)

## Approach hint

Track `farthest = 0`. For each index `i` from 0 to N-1: if `i > farthest`
return false. Update `farthest = max(farthest, i + jumps[i])`. If farthest
reaches `N-1`, return true. After the loop, return true.
