# failure.md — LC-078 koko-eating-speed

## Status

RUNTIME-FAIL (test corpus error, cases 1 and 2)

## Failing cases

Case 1: input `"4 5\n3 6 7 11\n"`, expected `"4\n"`, got `"7\n"`.
- Verification: K=4: ceil(3/4)+ceil(6/4)+ceil(7/4)+ceil(11/4) = 1+2+2+3 = 8 hours > 5.
  K=4 is NOT feasible for 5 hours. Minimum feasible K=7 (hours = 1+1+1+2 = 5).
  The test corpus expected value 4 is incorrect.

Case 2: input `"5 8\n30 11 23 4 20\n"`, expected `"23\n"`, got `"15\n"`.
- Verification: K=15: ceil(30/15)+ceil(11/15)+ceil(23/15)+ceil(4/15)+ceil(20/15)
  = 2+1+2+1+2 = 8 hours <= 8. K=15 is feasible.
  K=14: ceil(30/14)+... = 3+1+2+1+2 = 9 > 8. Not feasible.
  Minimum is K=15, not K=23. The test corpus expected value 23 is incorrect.

Cases 3–5 pass (expected 4, 5, 1 — all correct).

## Fix tier

Test corpus correction — no compiler change needed.
