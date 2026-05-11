# LC-034 Sort a Stack in Ascending Order

**Category**: Stack / Queue
**Difficulty**: Easy

## Algorithm

Given a sequence of integers representing a stack (topmost element last in the
input), sort the stack so the smallest element is at the top using only stack
operations (push, pop, peek) plus one auxiliary stack.

The algorithm repeatedly pops from the input stack and inserts the popped
element into the correct position in the auxiliary stack. To insert element X
into the auxiliary stack: while the auxiliary stack is non-empty and its top
exceeds X, move that top back to the input stack. Then push X. This is O(N^2)
in the worst case but uses only two stacks.

## Input format

```
Line 1: N
Line 2: N space-separated integers (left = bottom, right = top of stack)
```

## Oracle

N=5, [5, 2, 7, 1, 4] → printed top-to-bottom: `1 2 4 5 7`

## Approach hint

Emulate both stacks with parallel list + cursor pairs. After sorting,
print elements from the auxiliary stack top to bottom.
