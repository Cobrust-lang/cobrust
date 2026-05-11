# LC-091 Happy Number Cycle Detect

**Category**: Math
**Difficulty**: Easy

## Algorithm

A positive integer is called "happy" if repeatedly replacing it with the sum of
squares of its decimal digits eventually reaches 1. If the process never
reaches 1, it enters a cycle that never includes 1, making the number "unhappy".

The key insight is cycle detection: once the running sum revisits a value it has
seen before (and that value is not 1), the sequence will loop forever. A
practical approach is to maintain a small set of previously-seen sums. When the
sum equals 1, return true. When a duplicate sum is encountered, return false.

An equivalent approach (cycle detection without extra storage) applies Floyd's
tortoise-and-hare algorithm: advance one pointer one step at a time and another
two steps at a time; a cycle is confirmed when they meet, and the cycle
contains 1 only when the sequence is happy.

## Input format

```
Line 1: N — the positive integer to test
```

## Oracle

- N=19  → `true`  (19 → 82 → 68 → 100 → 1)
- N=2   → `false` (enters cycle: 4 → 16 → 37 → 58 → 89 → 145 → 42 → 20 → 4)
- N=1   → `true`  (already 1)

## Approach hint

Digit extraction loop: repeatedly take `n mod 10` to get the last digit, square
it, add to the sum, then divide `n` by 10. Maintain a seen-list (parallel
integer array + count) to detect revisits. Return `true` when sum reaches 1,
`false` when a repeated sum is encountered.
