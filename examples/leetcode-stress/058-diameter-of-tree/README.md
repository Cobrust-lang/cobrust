# LC-058 Diameter of a Binary Tree

**Category**: Binary Tree
**Difficulty**: Easy

## Algorithm

The diameter of a binary tree is the length of the longest path between any
two nodes, measured in number of edges. The path does not have to pass
through the root.

The key observation is that for each node, the longest path passing through
it equals `height(left_subtree) + height(right_subtree)`. The global diameter
is the maximum such value across all nodes.

An efficient O(N) recursive approach computes height bottom-up while tracking
the running maximum diameter. The `height` function returns the height of a
subtree and also updates a shared maximum whenever `left_height + right_height`
exceeds it.

## Input format

```
Line 1: N   (number of nodes; root is node 0)
Lines 2..N+1: val[i] left[i] right[i]
```

## Oracle

Tree where the longest path has 3 edges → `3`

## Approach hint

Recursive fn `height(node)` returns 0 for -1. For non-null nodes, compute
`lh = height(left)` and `rh = height(right)`. Update global max with
`lh + rh`. Return `1 + max(lh, rh)`. Use a global variable (or a single-
element list as a mutable accumulator) for the max diameter.
