# LC-057 Lowest Common Ancestor of Two Nodes

**Category**: Binary Tree
**Difficulty**: Medium

## Algorithm

Given a binary tree encoded as parallel arrays and two node indices P and Q,
find the value of their lowest common ancestor (LCA) — the deepest node that
is an ancestor of both P and Q (including either node being an ancestor of
the other).

The recursive approach: at each node, check if the current node is P or Q
(return the current node if so). Recurse into left and right subtrees. If
both sides return non-null results, the current node is the LCA. Otherwise,
return whichever side is non-null.

This single-pass O(N) algorithm works when the tree has no duplicate values
and both nodes are guaranteed to be present.

## Input format

```
Line 1: N P Q   (N nodes; find LCA of node-index P and node-index Q)
Lines 2..N+1: val[i] left[i] right[i]
```

## Oracle

Standard example tree, P=3 Q=5 (indices) → LCA val = val[root ancestor]

## Approach hint

Recursive fn `lca(node, p, q)` returning a node index or -1. At node == -1
return -1. Check if node == p or node == q → return node. Recurse left and
right. Combine results.
