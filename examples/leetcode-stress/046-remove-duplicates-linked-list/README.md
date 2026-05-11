# LC-046 Remove Duplicate Values from a Sorted Linked List

**Category**: Linked List
**Difficulty**: Easy

## Algorithm

Given a sorted singly linked list encoded as parallel arrays, remove all
nodes with duplicate values so each value appears at most once.
The output is the deduplicated list printed head to tail.

Because the list is sorted, duplicate values always appear consecutively.
A single forward pass suffices: maintain a cursor at the "current unique"
node. While the next node's value equals the current node's value, advance
a skip pointer. Once the skip pointer points to a node with a different value
(or -1), update current's next to that pointer, then advance current.

## Input format

```
Line 1: N
Lines 2..N+1: one integer per line (sorted values, head first)
```

## Oracle

N=6, [1,1,2,3,3,3] → `1 2 3` (one per line)

## Approach hint

Build val[] and a linear next[]. Walk with a `curr` cursor. While `next[curr]
!= -1` and `val[next[curr]] == val[curr]`, save a `runner` to skip over
duplicates. Set `next[curr] = runner`. Then advance `curr = next[curr]`.
