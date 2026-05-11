# LC-061 Coin Change Min

**Category**: Dynamic Programming
**Difficulty**: Medium

## Algorithm

Given a set of coin denominations and a target amount, find the fewest number
of coins needed to reach exactly that amount. You may use each denomination
an unlimited number of times. If no combination of coins can sum to the
target, output -1.

The classic bottom-up DP approach builds a table `dp[0..amount]` where each
cell holds the minimum coins to reach that sub-amount. Starting from `dp[0]=0`,
for each amount `a` from 1 to target, try every coin denomination `c`: if
`a >= c` and `dp[a-c]` is reachable, then `dp[a] = min(dp[a], dp[a-c]+1)`.

## Input format

```
Line 1: N (number of coin denominations)
Line 2: N space-separated coin values
Line 3: amount (target sum)
```

## Oracle

- N=3, coins=[1,5,11], amount=15 → `3` (three 5s)
- N=3, coins=[2,5,10], amount=3 → `-1` (unreachable)
- N=1, coins=[1], amount=0 → `0`

## Approach hint

Allocate a dp list of size amount+1. Initialize every slot to amount+1 as
"infinity". Set dp[0]=0. Outer loop over amounts 1..amount; inner loop over
each coin. If amount >= coin and dp[amount-coin]+1 < dp[amount], update.
