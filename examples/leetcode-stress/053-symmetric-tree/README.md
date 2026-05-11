# LC-053 Check if a Binary Tree is Symmetric

**Category**: Binary Tree
**Difficulty**: Easy

## Algorithm

Given a binary tree encoded as parallel arrays, determine whether it is
symmetric around its root — meaning the left subtree is a mirror image of
the right subtree.

Two subtrees rooted at nodes A and B are mirror images if:
- Both are empty (both indices are -1): true.
- Exactly one is empty: false.
- Both non-empty and val[A] == val[B], AND left[A] mirrors right[B],
  AND right[A] mirrors left[B].

A recursive `is_mirror(a, b)` function naturally expresses this. The root
is symmetric iff `is_mirror(left[root], right[root])`.

## Input format

```
Line 1: N   (number of nodes; root is node 0)
Lines 2..N+1: val[i] left[i] right[i]
```

## Oracle

Symmetric tree of 7 nodes → `true`
Asymmetric tree → `false`

## Approach hint

Implement `fn is_mirror(a: i64, b: i64) -> i64` returning 1 for mirror, 0
for not. Base: both -1 → 1; one -1 → 0; then check value equality and
recurse on the cross children.
