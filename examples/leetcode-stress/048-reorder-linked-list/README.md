# LC-048 Reorder Linked List

**Category**: Linked List
**Difficulty**: Medium

## Algorithm

Given a singly linked list L0 -> L1 -> ... -> Ln-1 -> Ln, reorder it as
L0 -> Ln -> L1 -> Ln-1 -> L2 -> Ln-2 -> ... and print the reordered values.

This combines three well-known linked-list techniques:
1. Find the midpoint using the slow/fast pointer approach.
2. Reverse the second half of the list.
3. Interleave the two halves node by node.

All three steps operate on the parallel-array representation of the list
without allocating extra space.

## Input format

```
Line 1: N
Lines 2..N+1: one integer per line (values head-to-tail)
```

## Oracle

N=5, [1,2,3,4,5] → `1 5 2 4 3` (one per line)
N=4, [1,2,3,4] → `1 4 2 3` (one per line)

## Approach hint

Build val[] and linear next[]. Locate mid with slow/fast, reverse from
mid+1 to end, then walk two cursors (head and reversed-second-head) and
alternate printing one from each.
