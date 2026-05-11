# LC-051 Maximum Depth of a Binary Tree

**Category**: Binary Tree
**Difficulty**: Easy

## Algorithm

Given a binary tree encoded as parallel arrays (`val[]`, `left[]`, `right[]`
with -1 as the null/absent sentinel), compute the maximum depth (the number
of nodes along the longest path from the root to any leaf).

Depth is computed recursively: the depth of an empty subtree (index -1) is 0.
The depth of any non-null node is `1 + max(depth(left[node]), depth(right[node]))`.
The recursion naturally handles arbitrary tree shapes.

## Input format

```
Line 1: N   (number of nodes; root is always node 0)
Lines 2..N+1: val[i] left[i] right[i]
```

## Oracle

```
5
1 1 2
2 3 4
3 -1 -1
4 -1 -1
5 -1 -1
```
Tree: root=1, left child=2, right child=3; node 2 has children 4,5.
→ `3`

## Approach hint

Read into three list_new arrays of size N. Write a recursive depth function
that returns 0 for index -1. Call on root (index 0).
