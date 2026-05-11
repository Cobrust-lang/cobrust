# LC-026 Hash Map Isomorphic Strings

**Category**: Hash Maps
**Difficulty**: Easy

## Algorithm

Two strings of equal length are isomorphic if there exists a consistent
one-to-one character mapping from the first string to the second string: each
character in the first maps to exactly one character in the second, and no two
distinct characters in the first map to the same character in the second.
Output `true` if the strings are isomorphic, `false` otherwise.

Emulate the bidirectional mapping with two parallel-list dictionaries (each
with 128 slots for ASCII): one maps characters from string1 to string2, and
one maps characters from string2 to string1. Use the ASCII code as the index
(0-127). Initialize all slots to -1 (meaning "not yet mapped"). For each
position, check consistency in both directions; any conflict means `false`.

## Input format

```
Line 1: first string
Line 2: second string
```

## Oracle

- `"egg"` and `"add"` → `true`
- `"foo"` and `"bar"` → `false`
- `"paper"` and `"title"` → `true`

## Approach hint

Two 128-slot mapping arrays (ASCII indexed); check bidirectional consistency
at each position; early `false` on conflict.
