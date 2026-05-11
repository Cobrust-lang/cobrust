# failure.md — LC-074 peak-element-binary-search

## Status

RUNTIME-FAIL (test corpus error, case 4 only)

## Failing case

Test case 4: input `"4\n1 2 1 3\n"`, expected `"3\n"`, got `"1\n"`.

## Root cause

Test corpus error. The peak-element problem has multiple valid answers:
for array [1, 2, 1, 3], both index 1 (value=2, neighbors 1 and 1) and index 3
(value=3, right edge, neighbor 1) are valid peak elements. The binary search
algorithm finds index 1 (the first peak encountered), while the test corpus
expects index 3. Both answers are algorithmically correct per the problem
definition ("find ANY peak element").

Cases 1–3 and 5 pass (expected 2, 0, 1, 0 — all correct).

## Fix tier

Test corpus correction — update expected to accept any valid peak index, or
change the test to use inputs with a unique peak. No compiler change needed.
