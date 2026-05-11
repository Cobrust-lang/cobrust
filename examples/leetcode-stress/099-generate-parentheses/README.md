# LC-099 Generate Parentheses

**Category**: Recursion
**Difficulty**: Medium

## Algorithm

Given N pairs of parentheses, enumerate all strings of length 2N that form
valid (fully balanced) bracket sequences. Output each valid string on its own
line in any consistent order.

The standard approach is a recursive backtracking construction. At each step,
the state is the string built so far plus counts of how many open and close
brackets have been appended. Two branching rules maintain validity:

1. Add an open bracket `(` if the count of open brackets used so far is less
   than N.
2. Add a close bracket `)` if the count of close brackets used so far is less
   than the count of open brackets (ensuring we never close more than we open).

When the total length reaches 2N, emit the accumulated string. Because both
branching conditions are tight constraints, only valid sequences are ever
completed — no post-filtering required.

The total count of valid sequences for N pairs is the Nth Catalan number.

## Input format

```
Line 1: N — number of parenthesis pairs (1 ≤ N ≤ 4)
```

## Oracle

- N=1 → `()`
- N=2 → `(())\n()()`  (two lines, one sequence per line, in this order)
- N=3 → six sequences on six lines: `((()))`, `(()())`, `(())()`, `()(())`, `()()()`

## Approach hint

A recursive helper accumulates the current string in a character array (using
list_new + list_set for the character buffer) and recurses with `open+1` or
`close+1` at each decision point. At depth 2N, iterate the buffer to print each
character via `print_no_nl`, then print a newline.
