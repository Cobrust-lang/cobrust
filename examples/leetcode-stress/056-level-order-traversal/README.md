# LC-056 Level-Order Traversal of a Binary Tree

**Category**: Binary Tree
**Difficulty**: Medium

## Algorithm

Given a binary tree encoded as parallel arrays, perform a breadth-first
(level-order) traversal and print each level's values on a separate line,
space-separated. This is also called a "row by row" or "BFS" traversal.

The algorithm uses a queue. Initially the queue contains just the root.
At each step, record the current queue size (this is the number of nodes
at the current level). Dequeue exactly that many nodes, print their values,
and enqueue their non-null children. Repeat until the queue is empty.

## Input format

```
Line 1: N   (number of nodes; root is node 0)
Lines 2..N+1: val[i] left[i] right[i]
```

## Oracle

Full binary tree of 7 nodes → level 0: `1`, level 1: `2 3`, level 2: `4 5 6 7`

## Approach hint

Emulate the queue with a list of size N and front/back integer cursors.
Before processing each level, save `level_size = back - front`. Dequeue
exactly `level_size` nodes and print them space-separated with a newline
at the end of each level.
