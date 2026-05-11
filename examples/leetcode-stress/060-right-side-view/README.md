# LC-060 Right-Side View of a Binary Tree

**Category**: Binary Tree
**Difficulty**: Medium

## Algorithm

Given a binary tree, imagine standing to the right and looking inward.
For each level of the tree, you see exactly one node — the rightmost node at
that depth. Collect and print these rightmost-visible values from top to
bottom.

The BFS-level-order approach naturally solves this: at each level, when
processing the last node in the level's batch (the rightmost node at that
depth), record its value. The same queue-based level-order traversal from
the level-order problem applies here; only the output differs.

## Input format

```
Line 1: N   (number of nodes; root is node 0)
Lines 2..N+1: val[i] left[i] right[i]
```

## Oracle

Tree with right-side view [1, 3, 4] (root=1, right child=3, right grandchild=4)
→ `1 3 4` (one per line)

## Approach hint

BFS queue as in LC-056. At each level, process `level_size` nodes. The last
node dequeued in each level batch is the rightmost — print its value.
