# LC-045 Palindrome Linked List

**Category**: Linked List
**Difficulty**: Medium

## Algorithm

Given a singly linked list encoded as parallel arrays, determine whether
the sequence of node values reads the same forwards and backwards.

The approach uses the two-pointer middle-finding technique (LC-044 pattern)
to locate the midpoint, then reverses the second half of the list in-place
(LC-041 pattern). Finally, both halves are walked simultaneously and values
are compared. If all pairs match, the list is a palindrome. As an optional
cleanup step, the second half can be reversed again to restore the original
structure, though correctness of the output does not require it.

## Input format

```
Line 1: N
Lines 2..N+1: one integer per line (node values, head first)
```

## Oracle

- N=5, [1,2,3,2,1] → `true`
- N=5, [1,2,3,4,5] → `false`
- N=4, [1,2,2,1] → `true`

## Approach hint

Build val[] and a linear next[] array. Find the midpoint with slow/fast
pointers. Reverse from the midpoint to the end. Walk from head and from the
new second-half head simultaneously and compare values.
