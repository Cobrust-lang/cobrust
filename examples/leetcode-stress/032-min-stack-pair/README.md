# LC-032 Min-Stack via Pair List

**Category**: Stack / Queue
**Difficulty**: Easy

## Algorithm

Design a stack that supports push, pop, and retrieval of the minimum element
stored, all in O(1) time. The key insight is to maintain a parallel auxiliary
stack that tracks the running minimum at each depth level. Whenever an element
is pushed, the new minimum is `min(new_value, current_min)` and is pushed onto
the auxiliary stack in parallel. Pop operations discard the top of both stacks
together.

This is implemented here using two parallel lists (a value list and a min list)
with a shared top-of-stack cursor.

## Input format

```
Line 1: Q   (number of operations)
Lines 2..Q+1: one operation per line:
  "push X"   — push integer X
  "pop"      — pop the top element
  "min"      — print the current minimum
```

## Oracle

```
5
push 3
push 5
min
push 1
min
```
→
```
3
1
```

## Approach hint

Two parallel arrays of size Q. A cursor tracks stack depth. On `push X`,
record X in the value list and `min(X, value_list[cursor-1])` in the min list.
On `min`, print `min_list[cursor-1]`.
