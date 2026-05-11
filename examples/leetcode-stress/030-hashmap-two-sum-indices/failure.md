# failure.md — LC-030 hashmap-two-sum-indices

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/030-hashmap-two-sum-indices/solution.cb -o /tmp/lc100-030
printf "4\n1 3 5 7\n8\n" | /tmp/lc100-030
```

## Raw stderr

(none — exit code 0, stdout mismatch)

## Actual vs expected

Input: N=4, values=[1,3,5,7], target=8
Expected stdout: "0\n3\n"
Actual stdout:   "1\n2\n"

## Suspected root cause

Test corpus defect. The input [1,3,5,7] with target=8 has TWO valid pairs:
- indices 0,3: 1+7=8
- indices 1,2: 3+5=8

The problem statement guarantees exactly one valid pair, but test case C4 violates
this constraint. The hash-map forward scan approach (O(N) expected) finds pair (1,2)
before (0,3). The test expects (0,3).

Verification: Python shows `[(0,3),(1,2)]` both valid for this input.

The implementation is algorithmically correct for all other test cases (C1,C2,C3
all PASS). The failure is in test case C4 where the "exactly one pair" precondition
is violated by the test data.

## Candidate fix tier

source-level gap (test corpus)

## Notes

Solution compiles and runs correctly. The hash-map two-sum algorithm correctly
finds the complement pair. All test cases except C4 pass. C4 violates the
uniqueness precondition stated in README.md ("Exactly one such pair is guaranteed").
