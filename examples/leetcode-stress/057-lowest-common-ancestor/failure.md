# failure.md — LC-057 lowest-common-ancestor

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/057-lowest-common-ancestor/solution.cb -o /tmp/lc100-057
printf "7 1 4\n6 1 2\n2 3 4\n8 -1 -1\n0 5 6\n7 -1 -1\n4 -1 -1\n5 -1 -1\n" | /tmp/lc100-057
```

## Raw stderr

(none — exit code 0, stdout mismatch on case 2 only)

## Actual vs expected

Case 2 input: same 7-node tree, p=1, q=4
Expected stdout: "6\n"  (root value)
Actual stdout: "2\n"   (node1 value)

## Suspected root cause

Test corpus defect. In the tree:
  Node0: val=6 (root)
  Node1: val=2, L=3, R=4
  Node4: val=7

Node1 is the direct parent of node4. Per the README definition "including either
node being an ancestor of the other", LCA(1,4) = node1 (val=2), because node1
IS an ancestor of node4. The solution correctly returns 2.

The expected "6" (root) would only be correct if the LCA definition requires
a STRICT ancestor (not the node itself). But the README explicitly says:
"the deepest node that is an ancestor of both P and Q (including either node
being an ancestor of the other)."

Cases 1, 3, 4 all pass with the correct LCA algorithm.

## Candidate fix tier

source-level gap (test corpus)

## Notes

Solution implements correct LCA via depth-equalization then simultaneous
ancestor traversal. The algorithm is correct per README. Case 2's expected
output contradicts the README's "including either node" definition.
