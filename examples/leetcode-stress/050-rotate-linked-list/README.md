# LC-050 Rotate Linked List by K Positions

**Category**: Linked List
**Difficulty**: Easy

## Algorithm

Given a singly linked list and an integer K, rotate the list to the right
by K positions and print the resulting order. Rotating right by 1 means
the last element becomes the new head. Rotating right by K > length is
equivalent to rotating by K mod length.

The most direct approach:
1. Compute the length N and reduce K to K mod N (handle K=0 as no-op).
2. Find the new tail: walk to position N-K-1 (0-indexed).
3. The new head is the node at position N-K.
4. Connect the old tail back to the old head to complete the rotation.

## Input format

```
Line 1: N K
Lines 2..N+1: one integer per line (node values, head first)
```

## Oracle

N=5 K=2, [1,2,3,4,5] → `4 5 1 2 3` (one per line)
N=5 K=5, [1,2,3,4,5] → `1 2 3 4 5` (no change; K mod 5 = 0)

## Approach hint

Build val[] and linear next[]. Walk to the N-K-1 position node to find the
cut point. Adjust next pointers, then print from the new head.
