# LC-043 Remove the Nth Node from the End

**Category**: Linked List
**Difficulty**: Easy

## Algorithm

Given a singly linked list encoded as parallel arrays and an integer N,
remove the node that is exactly N positions from the tail of the list and
print the resulting list.

The classic one-pass approach uses two cursors separated by a gap of N+1
nodes. Advance the leading cursor N+1 steps ahead of the trailing cursor.
Then advance both together until the leading cursor falls off the end. At
that point the trailing cursor sits at the node just before the one to be
removed. Adjust the next pointer to skip the target node.

## Input format

```
Line 1: L N   (L = list length; N = remove from end, 1-indexed)
Lines 2..L+1: one integer per line (node values, head first)
```

## Oracle

L=5 N=2, [1,2,3,4,5] → `1 2 3 5` (one per line)

## Approach hint

Build val[] and next[] with head=0. Use the two-pointer gap approach.
A dummy head node (value 0, next = real head index 0) simplifies handling
when the head itself must be removed.
