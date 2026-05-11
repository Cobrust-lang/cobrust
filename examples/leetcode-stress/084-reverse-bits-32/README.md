# LC-084 Reverse Bits 32

**Category**: Bit Manipulation
**Difficulty**: Easy

## Algorithm

Given a 32-bit non-negative integer, reverse its binary representation and
output the resulting integer. The bit at position 0 (LSB) swaps with the bit
at position 31 (MSB), and so on. For example, 43261596 in binary is
00000010100101000001111010011100, reversed to 00111001011110000010100101000000
= 964176192.

The approach extracts bits one by one from the input using repeated
division-by-2 and reconstruction: result = 0; for 32 iterations: extract LSB
of n (n mod 2), shift result left by 1 (result *= 2), add the extracted bit
to result, shift n right (n /= 2). After 32 steps, result is the reversed
32-bit integer.

## Input format

```
Line 1: N (non-negative 32-bit integer in decimal)
```

## Oracle

- N=43261596 → `964176192`
- N=0 → `0`
- N=1 → `2147483648`

## Approach hint

Use 32-iteration loop. Extract bit = n mod 2. result = result * 2 + bit.
n = n / 2. The result must be treated as unsigned 32-bit; since Cobrust i64
can represent values up to 2^63-1, the unsigned 32-bit result fits without
overflow.
