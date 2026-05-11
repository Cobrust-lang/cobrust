# LC-035 Queue via Two Stacks

**Category**: Stack / Queue
**Difficulty**: Easy

## Algorithm

Implement a first-in first-out queue using only two stacks. The standard
approach uses an "inbox" stack for enqueue operations and an "outbox" stack
for dequeue operations. When a dequeue is requested and the outbox is empty,
move every element from the inbox to the outbox in O(N) time — this
reversal restores FIFO order. Each element crosses between stacks at most
once, giving amortized O(1) per operation.

## Input format

```
Line 1: Q   (number of operations)
Lines 2..Q+1: one of:
  "enqueue X"   — enqueue integer X
  "dequeue"     — print and remove the front element
  "peek"        — print the front element without removing it
```

## Oracle

```
6
enqueue 1
enqueue 2
enqueue 3
dequeue
peek
dequeue
```
→
```
1
2
2
```

## Approach hint

Emulate two stacks with four parallel arrays (inbox_vals[], inbox_top,
outbox_vals[], outbox_top). On dequeue/peek, if outbox is empty, transfer
all inbox elements to outbox before proceeding.
