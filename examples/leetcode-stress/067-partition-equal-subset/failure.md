# failure.md — LC-067 partition-equal-subset

## Status

RUNTIME-FAIL (test corpus error, case 5 only)

## Failing case

Test case 5: input `"5\n3 3 3 4 5\n"`, expected `"false\n"`, got `"true\n"`.

## Root cause

Test corpus error. Array [3,3,3,4,5] sums to 18; half = 9. The subset {3,3,3}
sums to 9, so the array CAN be partitioned into equal-sum halves. The correct
answer is `true`. The test corpus expected `false` is incorrect.

Cases 1–4 pass correctly (expected true, true, false, true).

## Fix tier

Test corpus correction — no compiler change needed.
