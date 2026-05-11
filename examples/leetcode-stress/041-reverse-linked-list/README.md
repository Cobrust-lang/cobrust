# LC-041 Reverse a Linked List

**Category**: Linked List
**Difficulty**: Easy

## Algorithm

Given a singly linked list represented by parallel arrays — `val[]` holding
node values and `next[]` holding next-node indices with -1 as the null
sentinel — reverse the list and print its values from new head to new tail.

The in-place iterative reversal algorithm uses three cursors: `prev` (starts
at -1, the sentinel), `curr` (starts at head index 0), and `nxt`. In each
step, save `nxt = next[curr]`, point `next[curr] = prev`, advance `prev =
curr`, advance `curr = nxt`. When `curr` reaches -1, `prev` is the new head.

## Input format

```
Line 1: N   (number of nodes)
Lines 2..N+1: val[i] next[i]   (values and next pointers; head is node 0)
```

## Oracle

```
5
1 1
2 2
3 3
4 4
5 -1
```
→ `5 4 3 2 1` (one per line)

## Approach hint

Read all val[] and next[] into two list_new arrays. Run the three-cursor
reversal in-place, then print starting from the new head index.
