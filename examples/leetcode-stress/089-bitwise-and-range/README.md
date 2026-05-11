# LC-089 Bitwise AND of a Range

**Category**: Bit Manipulation
**Difficulty**: Medium

## Algorithm

Given two non-negative integers left and right (left <= right), compute the
bitwise AND of all integers in the range [left, right] inclusive. Doing this
naively by ANDing every integer in the range would be O(N); the trick allows
O(log N).

The key observation is that if left != right, the last bit position where they
differ will be 0 in the AND of the range (because both 0 and 1 appear at that
position as we traverse the range). Keep right-shifting both numbers until
they are equal; the number of shifts records how many trailing zeros the
answer has. The answer is the common prefix shifted back left.

## Input format

```
Line 1: left right (two non-negative integers)
```

## Oracle

- left=5, right=7 → `4` (binary: 101 AND 110 AND 111 = 100 = 4)
- left=0, right=0 → `0`
- left=1, right=2147483647 → `0`

## Approach hint

shift = 0. While left != right: left = left / 2, right = right / 2,
shift += 1. Result = left * 2^shift (i.e., left shifted left by shift
positions). Compute 2^shift using a loop: pow2 = 1, multiply by 2 shift times.
