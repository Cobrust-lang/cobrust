# failure.md — LC-059 flatten-tree-to-list

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/059-flatten-tree-to-list/solution.cb -o /tmp/lc100-059
printf "6\n1 1 2\n2 3 4\n5 -1 -1\n3 -1 -1\n4 -1 -1\n6 -1 -1\n" | /tmp/lc100-059
```

## Raw stderr

(none — exit code 0, stdout mismatch on case 1 only)

## Actual vs expected

Case 1 input: 6-node tree
Expected stdout: "1\n2\n3\n4\n5\n6\n"
Actual stdout: "1\n2\n3\n4\n5\n"

## Suspected root cause

Test corpus defect. The 6-node tree encodes:
  Node0: val=1, L=1, R=2
  Node1: val=2, L=3, R=4
  Node2: val=5, L=-1, R=-1
  Node3: val=3, L=-1, R=-1
  Node4: val=4, L=-1, R=-1
  Node5: val=6, L=-1, R=-1

Node5 (val=6) is not reachable from the root (Node0). No node's left or right
pointer references index 5. Pre-order traversal from root visits nodes: 0,1,3,4,2
(values 1,2,3,4,5) — 5 nodes, not 6.

The expected output of "1\n2\n3\n4\n5\n6\n" requires node5 to be in the tree.
The test.toml likely has a missing pointer: Node2 should have R=5 (encoding:
"5 -1 5") to include node5(val=6) in the right subtree of node2.

Cases 2, 3, 4 all pass correctly.

## Candidate fix tier

source-level gap (test corpus)

## Notes

Solution implements correct iterative pre-order DFS (push right then left).
Pre-order traversal of a well-formed 6-node tree would visit all 6 nodes.
The test.toml encodes an unreachable 6th node, causing case 1 to produce
only 5 values instead of 6.
