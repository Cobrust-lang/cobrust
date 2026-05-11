# LC-036 Daily Temperatures Wait Count

**Category**: Stack / Queue
**Difficulty**: Easy

## Algorithm

Given a list of daily temperature readings, compute for each day how many
days you must wait until a strictly warmer temperature occurs. If no future
day is warmer, the answer for that day is 0.

This is a standard monotone-decreasing-stack problem. Iterate through the
temperatures. The stack holds indices of days whose "wait count" has not yet
been determined. Before processing day i, pop all stack entries whose
temperature is less than today's temperature — for each popped index j,
the answer is `i - j`. Days still in the stack at the end get answer 0.

## Input format

```
Line 1: N
Line 2: N space-separated temperatures (positive integers)
```

## Oracle

N=8, [73, 74, 75, 71, 69, 72, 76, 73] → `1 1 4 2 1 1 0 0`

## Approach hint

Emulate the stack with a list and a top cursor. Store indices. On each step,
pop while stack is non-empty and the temperature at the stack-top index is
less than the current temperature.
