# LC-025 Hash Map First Unique Character

**Category**: Hash Maps
**Difficulty**: Easy

## Algorithm

Given a string of lowercase letters, find the 0-based index of the first
character that appears exactly once in the string. If no such character exists,
output -1.

A two-pass approach: in the first pass, build a 26-slot frequency count array
(index = char_ord - ord('a'), increment for each character). In the second
pass, scan the string from left to right; return the index of the first
character whose frequency count is exactly 1.

## Input format

```
Line 1: the input string (lowercase letters)
```

## Oracle

- `"abcabd"` → `2` (first unique is 'c' at index 2; a=2, b=2, c=1, d=1)
- `"aabb"` → `-1` (all characters appear at least twice)
- `"z"` → `0`

## Approach hint

26-slot frequency count in one pass; second pass returns index of first
count-1 character.
