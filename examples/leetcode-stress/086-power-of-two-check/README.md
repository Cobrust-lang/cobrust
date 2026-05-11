# LC-086 Power of Two Check

**Category**: Bit Manipulation
**Difficulty**: Easy

## Algorithm

Given an integer N, determine whether it is a power of 2. A power of 2 is
any value of the form 2^k where k >= 0: 1, 2, 4, 8, 16, ... Negative
numbers and zero are not powers of 2.

The classic bit-manipulation insight: a power of 2 has exactly one bit set
in its binary representation. Therefore N > 0 AND (N & (N-1)) == 0 is the
test. N & (N-1) clears the lowest set bit of N; if N was a power of 2, the
result is zero. Equivalently: keep dividing N by 2 while N is even and > 0;
if the final value is 1, it was a power of 2.

## Input format

```
Line 1: N (a single integer)
```

## Oracle

- N=1 → `true`
- N=16 → `true`
- N=3 → `false`
- N=0 → `false`

## Approach hint

Use the repeated-halving approach since Cobrust may not have bitwise AND:
while n > 1 and n mod 2 == 0: n /= 2. After the loop, output "true" if n==1
else "false". Handle n<=0 as false directly.
