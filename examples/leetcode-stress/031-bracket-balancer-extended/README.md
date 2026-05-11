# LC-031 Bracket-Balancer Extended

**Category**: Stack / Queue
**Difficulty**: Easy

## Algorithm

A classic stack-based bracket matching problem extended beyond the three
standard bracket pairs. Given a string that may contain round brackets `()`,
square brackets `[]`, curly braces `{}`, and angle brackets `<>`, determine
whether every opening symbol is closed by the correct matching symbol in
proper nesting order.

The standard approach pushes each opening symbol onto a stack. When a closing
symbol is encountered, the top of the stack must hold its mirror. Any
mismatch, premature close on an empty stack, or leftover unclosed symbols at
the end produces a "false" result.

## Input format

```
Line 1: the bracket string (may include letters, digits, and bracket chars)
```

## Oracle

- `"({[<>]})"` → `true`
- `"({[}])"` → `false`
- `""` → `true`

## Approach hint

Emulate a stack with a list and a top-of-stack integer. Push opening symbols,
pop and compare on closing symbols, reject on mismatch or empty-stack close.
