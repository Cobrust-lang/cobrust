# LC-063 Unique Paths Grid

**Category**: Dynamic Programming
**Difficulty**: Medium

## Algorithm

Consider a rectangular grid with M rows and N columns. A traveler starts at
the top-left corner and must reach the bottom-right corner. The only allowed
moves are one step right or one step down. Count the total number of distinct
routes from start to finish.

The 2D DP table `dp[i][j]` represents the number of ways to reach cell (i,j).
The top row and left column each have exactly 1 way to reach (only one
direction possible). For interior cells: `dp[i][j] = dp[i-1][j] + dp[i][j-1]`
because you arrive either from above or from the left. The answer is dp[M-1][N-1].

## Input format

```
Line 1: M N (rows and columns, 1-indexed dimensions)
```

## Oracle

- M=3, N=7 → `28`
- M=1, N=1 → `1`
- M=2, N=2 → `2`

## Approach hint

The 2D DP table can be space-optimized to a 1D row array. Initialize all
entries to 1 (the first row). For each subsequent row, update left to right:
dp[j] += dp[j-1]. After M-1 row passes, dp[N-1] is the answer.
