# LC-055 Count Total Nodes in a Binary Tree

**Category**: Binary Tree
**Difficulty**: Easy

## Algorithm

Given a binary tree encoded as parallel arrays, count the total number of
nodes it contains.

The recursive definition is straightforward:
- An empty tree (index -1) has 0 nodes.
- A non-empty tree has `1 + count(left[root]) + count(right[root])` nodes.

While a simple loop over all N entries would also work given the parallel-
array encoding, this problem is designed to exercise recursive tree traversal
as a building block for more complex tree operations.

## Input format

```
Line 1: N   (number of nodes; root is node 0)
Lines 2..N+1: val[i] left[i] right[i]
```

## Oracle

A tree with 7 nodes → `7`

## Approach hint

Recursive fn `count(node)` returning 0 for -1, else
`1 + count(left[node]) + count(right[node])`. Call on root index 0.
