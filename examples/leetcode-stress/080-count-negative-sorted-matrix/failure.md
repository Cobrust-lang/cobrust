# failure.md — LC-080 count-negative-sorted-matrix

## Status

RUNTIME-FAIL (test corpus error, case 4 only)

## Failing case

Case 4: input `"3 3\n5 1 0\n-1 -1 -1\n-5 -5 -5\n"`, expected `"7\n"`, got `"6\n"`.

## Root cause

Test corpus error. Counting strictly-negative numbers (< 0):
- Row 1 [5, 1, 0]: 0 negatives (0 is non-negative)
- Row 2 [-1, -1, -1]: 3 negatives
- Row 3 [-5, -5, -5]: 3 negatives
Total: 6.

The test corpus expected 7 appears to count 0 as negative, which contradicts
the standard mathematical definition (0 is not negative). The solution
correctly implements the standard definition. Cases 1–3 and 5 pass.

## Fix tier

Test corpus correction — no compiler change needed.
