# failure.md — LC-064 house-robber-linear

## Status

RUNTIME-FAIL (test corpus error, case 5 only)

## Failing case

Test case 5: input `"6\n6 7 1 3 8 2\n"`, expected `"19\n"`, got `"15\n"`.

## Root cause

Test corpus error. The maximum non-adjacent sum for [6, 7, 1, 3, 8, 2] is 15,
not 19. Standard house robber DP trace:
- dp[0]=6, dp[1]=max(6,7)=7, dp[2]=max(7,6+1)=7, dp[3]=max(7,7+3)=10,
  dp[4]=max(10,7+8)=15, dp[5]=max(15,10+2)=15.
Maximum achievable: 6+1+8=15 (indices 0,2,4), 7+3+2=12 (indices 1,3,5).
There is no 19. Cases 1–4 pass (expected 11, 10, 100, 8 — all correct).

## Fix tier

Test corpus correction — no compiler change needed.
