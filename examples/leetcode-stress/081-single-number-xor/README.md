# LC-081 Single Number XOR

**Category**: Bit Manipulation
**Difficulty**: Easy

## Algorithm

Given a list of integers where every element appears exactly twice except for
one element that appears exactly once, find that unique element. The solution
must use O(1) extra space and O(N) time.

XOR has two useful properties: A XOR A = 0 (any number XORed with itself is
zero) and A XOR 0 = A (XOR with zero is identity). Therefore, XORing all
elements of the array causes every pair to cancel to 0, leaving only the
unique element. Initialize result = 0, then XOR every element into it.

## Input format

```
Line 1: N (number of elements, always odd since one appears once, rest twice)
Line 2: N space-separated integers
```

## Oracle

- N=5, [4,1,2,1,2] → `4`
- N=3, [7,7,3] → `3`
- N=1, [99] → `99`

## Approach hint

One variable: result = 0. Loop through all N elements XORing each into
result. Print result. In Cobrust, use the `%` operator for bitwise XOR
if that is the current surface, or use repeated addition/subtraction tricks.
Note: check which bitwise operators ADR-0044 surface currently supports.
