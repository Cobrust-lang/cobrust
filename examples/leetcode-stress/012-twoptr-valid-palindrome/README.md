# LC-012 Two Pointers Valid Palindrome

**Category**: Two Pointers
**Difficulty**: Easy

## Algorithm

Given a string that may contain letters, digits, and other characters,
determine whether it reads the same forward and backward when only
alphanumeric characters are considered and case is ignored. Output `true` or
`false`.

The two-pointer approach works directly on the original string: place a left
cursor at the start and a right cursor at the end. Skip over any non-
alphanumeric characters from either side. Compare the characters at both
cursors after normalizing to lowercase. If a mismatch is found, return `false`.
If the cursors meet without mismatch, return `true`.

For this problem, treat lowercase letters `a`-`z` and digits `0`-`9` as
alphanumeric. Uppercase `A`-`Z` maps to lowercase `a`-`z` by adding 32 to
the ASCII code.

## Input format

```
Line 1: the input string (may include punctuation and spaces)
```

## Oracle

- `"A man a plan a canal Panama"` → `true`
- `"race a car"` → `false`
- `" "` → `true`

## Approach hint

Left/right cursor inward sweep; skip non-alphanumeric; compare ASCII-normalized
chars; early `false` on mismatch.
