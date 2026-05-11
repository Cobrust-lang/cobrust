# LC-037 Evaluate Reverse Polish Notation

**Category**: Stack / Queue
**Difficulty**: Medium

## Algorithm

Reverse Polish Notation (RPN) places operators after their operands rather than
between them, eliminating the need for parentheses. For example, `3 4 +` means
`3 + 4 = 7`, and `5 1 2 + 4 * + 3 -` evaluates to `14`.

Evaluation uses a single stack. Scan tokens left to right. If a token is an
integer (possibly negative), push it. If a token is an operator (`+`, `-`,
`*`, `/`), pop two operands — the second-to-top is the left operand, the top
is the right operand — apply the operator, and push the result. Integer
division truncates toward zero. At the end, the stack contains exactly one
element: the answer.

## Input format

```
Line 1: N   (number of tokens)
Lines 2..N+1: one token per line (integer or operator)
```

## Oracle

```
9
5
1
2
+
4
*
+
3
-
```
→ `14`

## Approach hint

Emulate the stack with a list and a top cursor. Distinguish operators with
`str_eq_lit`. For division, implement truncation toward zero using the sign
of the operands.
