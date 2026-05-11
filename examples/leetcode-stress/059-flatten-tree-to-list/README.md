# LC-059 Flatten Binary Tree to Linked List Order

**Category**: Binary Tree
**Difficulty**: Medium

## Algorithm

Given a binary tree encoded as parallel arrays, flatten it into a linked list
following the pre-order traversal sequence (root, left subtree, right subtree)
and print the resulting node values in order.

The in-place approach uses a "Morris-style" right-pointer threading trick:
for each node that has a left child, find the rightmost node of the left
subtree (the pre-order predecessor of the right child). Set its right pointer
to the current node's right child. Then move the left child to the right and
clear the left pointer. Advance to the right child. Repeat until all nodes
are processed.

This runs in O(N) time and O(1) auxiliary space.

## Input format

```
Line 1: N   (number of nodes; root is node 0)
Lines 2..N+1: val[i] left[i] right[i]
```

## Oracle

Tree with pre-order [1,2,3,4,5,6] → `1 2 3 4 5 6` (one per line)

## Approach hint

Implement the rightmost-predecessor scan. After flattening, the tree is a
right-spine: follow right[] from root until -1, printing val[i] at each step.
