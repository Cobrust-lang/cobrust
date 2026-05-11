# failure.md — LC-061 coin-change-min

## Status

RUNTIME-FAIL (test corpus error, case 5 only)

## Failing case

Test case 5: input `"3\n1 5 10\n27\n"`, expected `"4\n"`, got `"5\n"`.

## Root cause

Test corpus error. The minimum coin count to make amount 27 with coins {1, 5, 10}
is 5 (10+10+5+1+1 = 27 in 5 coins), not 4. No 4-coin combination using {1,5,10}
can sum to 27. The solution correctly computes dp[27]=5.

Verification: DP table over [0..27] with coins {1,5,10}:
- dp[10]=1, dp[20]=2, dp[25]=3 (10+10+5), dp[26]=4, dp[27]=5 (10+10+5+1+1)

Cases 1–4 all pass (expected 3, -1, 0, 6 — all correct).

## Fix tier

Test corpus correction — no compiler change needed.
