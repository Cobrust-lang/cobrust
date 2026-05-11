# LC-052 Invert a Binary Tree

**Category**: Binary Tree
**Difficulty**: Easy

## Algorithm

Given a binary tree encoded as parallel arrays, invert it by swapping the
left and right child of every node, and print the resulting tree in
level-order (breadth-first) sequence.

The recursive approach is elegant: to invert the tree rooted at node i,
swap `left[i]` and `right[i]`, then recursively invert both subtrees.
The base case is an empty node (index -1), which is a no-op.

After inverting, perform a breadth-first traversal using a queue (emulated
as a list with front and back cursors) to print node values level by level.

## Input format

```
Line 1: N   (number of nodes; root is node 0)
Lines 2..N+1: val[i] left[i] right[i]
```

## Oracle

```
7
4 1 2
2 3 4
7 5 6
9 -1 -1
6 -1 -1
3 -1 -1
1 -1 -1
```
Original: 4's children are 2 (val=2) and 7 (val=7).
After invert: 4's children are 7 and 2; BFS → `4 7 2 3 6 9 6`... 
simplified example for one BFS output line.

## Approach hint

Recursive invert: swap `left[i]` and `right[i]`, recurse on both. BFS queue
emulated with list + front/back index cursors.
