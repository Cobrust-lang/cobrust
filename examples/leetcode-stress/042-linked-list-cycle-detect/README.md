# LC-042 Linked List Cycle Detection

**Category**: Linked List
**Difficulty**: Easy

## Algorithm

Given a singly linked list encoded via parallel arrays (`val[]`, `next[]` with
-1 for null), determine whether the list contains a cycle. A cycle means some
node's next pointer refers to a previously-visited node index.

Floyd's tortoise-and-hare algorithm uses two cursors that advance through the
list at different speeds: the slow cursor moves one step per iteration while
the fast cursor moves two steps. If the list is acyclic, the fast cursor
reaches the -1 sentinel first. If a cycle exists, both cursors eventually land
on the same node index, at which point the algorithm terminates and reports
true.

## Input format

```
Line 1: N C   (N nodes; C = cycle target index, or -1 for no cycle)
Lines 2..N+1: val[i]
```
The last node's `next` is set to C (if C != -1) or -1.
All intermediate nodes point to i+1.

## Oracle

- N=4 C=1 → `true` (node 3 → node 1 creates a cycle)
- N=4 C=-1 → `false` (no cycle)

## Approach hint

Build val[] and next[] arrays: node i points to i+1 for i < N-1; last node
points to C. Run the two-pointer algorithm stepping via `next[slow]` and
`next[next[fast]]`. Guard fast step: if fast == -1 or next[fast] == -1 then
no cycle.
