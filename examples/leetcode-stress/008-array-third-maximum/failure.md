# failure.md — LC-008 array-third-maximum

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/008-array-third-maximum/solution.cb -o /tmp/lc100-008
printf "5\n5 2 5 1 3\n" | /tmp/lc100-008
```

## Raw stderr

(none — exit code 0, stdout mismatch)

## Actual vs expected

Input: N=5, values=[5,2,5,1,3]
Expected stdout: "1\n"
Actual stdout:   "2\n"

## Suspected root cause

Test corpus defect. The input [5,2,5,1,3] has 4 distinct values: {5,3,2,1}.
Sorted descending: 5,3,2,1. The third distinct maximum is 2, not 1.
The expected output of "1\n" would be the fourth distinct maximum.

Verification: Python `sorted(set([5,2,5,1,3]), reverse=True)[2]` = 2.

The solution.cb implementation is algorithmically correct for all other test
cases (C1,C2,C3 all PASS). The failure is in test case C4 where the expected
value is inconsistent with the algorithm description in README.md.

## Candidate fix tier

source-level gap (test corpus)

## Notes

Solution compiles and runs correctly. The algorithm correctly tracks the top-3
distinct maximum values using a sentinel-based 3-slot tracker. All test cases
except C4 pass. C4 appears to have an incorrect expected_stdout in test.toml.
