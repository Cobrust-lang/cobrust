# LC-087 Missing Number XOR

**Category**: Bit Manipulation
**Difficulty**: Easy

## Algorithm

Given a list of N distinct integers drawn from the range [0, N] (so the list
has N elements but the range has N+1 possible values), find the one missing
integer. Every integer in [0, N] except one appears exactly once.

Two equivalent solutions exist. The XOR approach: XOR all numbers from 0 to
N together, then XOR all elements in the array. Every element that appears in
both cancels to 0; the remaining value is the missing number. Alternatively,
compute the expected sum 0+1+...+N = N*(N+1)/2 and subtract the actual sum
of the array elements.

## Input format

```
Line 1: N (number of elements, range is [0, N])
Line 2: N space-separated distinct integers from [0, N] with one missing
```

## Oracle

- N=3, [3,0,1] → `2`
- N=1, [0] → `1`
- N=5, [0,1,2,4,5] → `3`

## Approach hint

Use the sum formula approach: expected = N*(N+1)/2. Read all N values and
compute actual_sum. Output expected - actual_sum. This avoids needing XOR
primitives and is equivalent for this problem.
