# LC-044 Middle of a Linked List

**Category**: Linked List
**Difficulty**: Easy

## Algorithm

Given a singly linked list encoded via parallel arrays, find the middle node
and print its value (and all values from the middle to the tail). If the list
has an even number of nodes, the second of the two middle nodes is returned.

The two-pointer approach works without knowing the list length in advance.
A slow cursor advances one step per iteration while a fast cursor advances
two steps. When the fast cursor reaches the end (fast == -1 or next[fast] == -1),
the slow cursor is at the middle node.

## Input format

```
Line 1: N
Lines 2..N+1: one integer per line (node values, head first; each node
              implicitly points to the next, last points to -1)
```

## Oracle

- N=5, [1,2,3,4,5] → `3 4 5` (one per line)
- N=6, [1,2,3,4,5,6] → `4 5 6` (one per line)

## Approach hint

Build val[] and a linear next[]: next[i] = i+1 for i < N-1, next[N-1] = -1.
Run the slow-fast scan. After stopping, traverse from the slow cursor to the
end, printing each value.
