# LC-092 Palindrome Number Integer

**Category**: Math
**Difficulty**: Easy

## Algorithm

Determine whether a given integer reads the same forwards and backwards without
converting it to a string. Negative integers are never palindromes because the
leading minus sign has no mirror on the right side.

One approach reverses the integer arithmetically: repeatedly peel the last
digit via modulo 10, accumulate it into a reversed number, and divide the
original by 10. After consuming all digits, compare the reversed number to the
original. If they are equal, the number is a palindrome.

A subtlety arises with trailing zeros: any positive integer ending in zero
(other than zero itself) cannot be a palindrome because the leading digit is
never zero. This can be used as an early-exit check.

## Input format

```
Line 1: N — the integer to test (may be negative)
```

## Oracle

- N=121   → `true`
- N=-121  → `false`
- N=10    → `false`
- N=0     → `true`

## Approach hint

Early-reject negatives and non-zero trailing-zero integers. Reverse the digits
arithmetically (`rev = rev * 10 + n % 10`, `n = n / 10`). Compare original to
reversed value.
