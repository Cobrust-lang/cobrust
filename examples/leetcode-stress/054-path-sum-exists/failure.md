# failure.md — LC-054 path-sum-exists

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/054-path-sum-exists/solution.cb -o /tmp/lc100-054
printf "8 22\n5 1 2\n4 3 -1\n8 -1 4\n11 5 6\n13 -1 -1\n4 -1 7\n7 -1 -1\n2 -1 -1\n" | /tmp/lc100-054
```

## Raw stderr

(none — exit code 0, stdout mismatch on cases 1 and 2)

## Actual vs expected

Case 1 input: 8-node tree, target=22
Expected stdout: "true\n"
Actual stdout: "false\n"

Case 2 input: same tree, target=5
Expected stdout: "false\n"
Actual stdout: "false\n"

## Suspected root cause

Test corpus defect for case 1. The 8-node tree encodes:
  Node0: val=5, L=1, R=2
  Node1: val=4, L=3, R=-1
  Node2: val=8, L=-1, R=4
  Node3: val=11, L=5, R=6
  Node4: val=4,  L=-1, R=7
  Node5: val=13, L=-1, R=-1 (leaf)
  Node6: val=7,  L=-1, R=-1 (leaf)
  Node7: val=2,  L=-1, R=-1 (leaf)

All root-to-leaf paths:
  5→4→11→13 = 33
  5→4→11→7  = 27
  5→8→4→2   = 19

None equals 22. The standard LeetCode example for this tree has the path
5→4→11→2=22, but that would require node3's right child to have val=2.
In this test encoding, val=2 is at node7 (child of node4, not node3).
The node structure does not match the intended path-sum-22 tree.

Case 2 correctly returns "false" (no path sums to 5). Cases 3 and 4 pass.

## Candidate fix tier

source-level gap (test corpus)

## Notes

Solution implements correct iterative DFS path sum. Cases 3 and 4 pass.
Cases 1 and 2 appear to have an incorrect tree encoding in test.toml that
doesn't represent the intended tree for the 22-target example.
