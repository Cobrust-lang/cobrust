# failure.md — LC-037 reverse-polish-eval

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/037-reverse-polish-eval/solution.cb -o /tmp/lc100-037
printf "5\n10\n6\n9\n3\n/\n" | /tmp/lc100-037
```

## Raw stderr

(none — exit code 0, stdout mismatch on case 4 only)

## Actual vs expected

Case 4 input: N=5, tokens=["10","6","9","3","/"]
Expected stdout: "2\n"
Actual stdout: "10\n"

## Suspected root cause

Test corpus defect. The 5-token sequence ["10","6","9","3","/"] is not a valid
complete RPN expression: with 1 operator and 4 operands, evaluation leaves 3
values on the stack (10, 6, and 9/3=3). No standard RPN evaluation produces 2
from this input. The expression would need at least 2 more operators (e.g.
"10 6 9 3 / - -") to reduce to a single result of 2.

Verification: Cases 1,2,3,5 all pass (correct RPN expressions producing single values).

## Candidate fix tier

source-level gap (test corpus)

## Notes

Solution compiles and runs correctly. All cases except C4 pass. C4's token
sequence is not a valid balanced RPN expression.
