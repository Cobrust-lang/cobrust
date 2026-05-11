# failure.md — LC-039 decode-nested-depth

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/039-decode-nested-depth/solution.cb -o /tmp/lc100-039
printf "[[][]]\n" | /tmp/lc100-039
```

## Raw stderr

(none — exit code 0, stdout mismatch on case 3 only)

## Actual vs expected

Case 3 input: "[[][]]"
Expected stdout: "3\n"
Actual stdout: "4\n"

## Suspected root cause

Test corpus inconsistency. The README specifies the algorithm as: push 0 on
'['; on ']' pop v, add max(2*v, 1) to new stack top. Under this algorithm:
  - "[[][]]": push 0, push 0, pop 0 → add 1 → [0,1], push 0, pop 0 → add 1 → [0,2], pop 2 → add 4 → [4]. Result=4.

The test.toml expects 3, but no single consistent formula produces 1,2,3,3,4
for all 5 test cases simultaneously:
  - README formula (max(2v,1)) gives: 1,2,4,3,4 — fails case 3.
  - Alternative "[A]→score(A)+1" gives: 1,2,3,3,3 — fails case 5 [[[]]] (gives 3 not 4).

Cases 1,2,4,5 pass with the README formula.

## Candidate fix tier

source-level gap (test corpus)

## Notes

Solution implements the exact algorithm described in README.md. Cases 1,2,4,5
all produce the expected output. Case 3 appears to have an inconsistent
expected value in test.toml.
