# LC-022 Hash Map Valid Anagram

**Category**: Hash Maps
**Difficulty**: Easy

## Algorithm

Given two strings, determine whether one is a rearrangement of the other (an
anagram). Two strings are anagrams if and only if they contain exactly the same
multiset of characters. Output `true` or `false`.

The frequency-count approach: build a count array of 26 slots (for lowercase
letters a-z, using ASCII offset). Increment counts for the first string;
decrement counts for the second. If any slot is non-zero at the end, the
strings differ — output `false`. If all slots are zero, output `true`. If the
lengths differ, immediately output `false`.

## Input format

```
Line 1: first string (lowercase letters only)
Line 2: second string (lowercase letters only)
```

## Oracle

- `"anagram"` and `"nagaram"` → `true`
- `"rat"` and `"car"` → `false`

## Approach hint

26-slot frequency array (index = char_ord - ord('a')); increment for string1,
decrement for string2; check all zeros.
