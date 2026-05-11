# LC-054 Path Sum Exists

**Category**: Binary Tree
**Difficulty**: Easy

## Algorithm

Given a binary tree encoded as parallel arrays and a target integer, determine
whether there exists a root-to-leaf path whose node values sum exactly to the
target.

A path is defined as starting at the root and ending at a leaf (a node with
no children). The sum is the accumulated total of `val[]` along the path.

The recursive solution subtracts the current node's value from the remaining
target at each step. At a leaf, return true iff the remaining target is 0.
At an internal node, return true iff either the left or right subtree yields
a path that satisfies the remaining target.

## Input format

```
Line 1: N T   (N nodes, T = target sum; root is node 0)
Lines 2..N+1: val[i] left[i] right[i]
```

## Oracle

Tree with path 5->4->11->2 summing to 22, target=22 → `true`
Same tree, target=5 → `false`

## Approach hint

Recursive fn `has_path(node, remaining)`. If node == -1, return 0. Compute
`r = remaining - val[node]`. If leaf (left == -1 and right == -1), return r
== 0. Else return `has_path(left[node], r)` OR `has_path(right[node], r)`.
