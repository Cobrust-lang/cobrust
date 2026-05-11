# LC-039 Decode Nested Score by Depth

**Category**: Stack / Queue
**Difficulty**: Easy

## Algorithm

Given a string encoding nested groups with brackets, compute a total score
where the value of each group depends on its nesting depth. Specifically:
- An empty pair of brackets `[]` at depth D contributes 2^D to the score.
- Brackets can be nested arbitrarily.

The algorithm uses a stack of partial scores. Push 0 onto the stack for each
`[`. On `]`, pop the top value: if it is 0 (meaning the group was empty),
add 1 to the new top (representing 2^0 for depth 0; in general, doubling
occurs when the popped value is 0 and we add 1, otherwise we double the
popped value and add to the top). If the top was non-zero, double it and
add to the new top.

## Input format

```
Line 1: the bracket string (only '[' and ']' characters)
```

## Oracle

- `"[]"` → `1`
- `"[[]]"` → `2`
- `"[[][]]"` → `3`

## Approach hint

Use a list as a stack storing partial scores. Push 0 on `[`; on `]` pop v
and add `max(2*v, 1)` to the new top. The final answer is the single
remaining value.
