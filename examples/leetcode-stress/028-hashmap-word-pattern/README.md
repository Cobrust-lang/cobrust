# LC-028 Hash Map Word Pattern

**Category**: Hash Maps
**Difficulty**: Easy

## Algorithm

Given a pattern string of letters and a sequence of space-separated words
(equal count), determine whether the words follow the same structural pattern
as the letters. That is, each letter maps to exactly one word and no two
distinct letters map to the same word. Output `true` or `false`.

This is a bijective (two-way) mapping problem, analogous to isomorphic strings
but between characters and words. Emulate it with two parallel-list
dictionaries: one mapping each distinct letter to its corresponding word, and
one mapping each distinct word to its corresponding letter. For each position,
check that both mappings are consistent.

## Input format

```
Line 1: pattern string (lowercase letters, no spaces)
Line 2: space-separated words (same count as pattern length)
```

## Oracle

- pattern=`"abba"`, words=`"dog cat cat dog"` → `true`
- pattern=`"abba"`, words=`"dog cat cat fish"` → `false`
- pattern=`"aaaa"`, words=`"dog dog dog dog"` → `true`
- pattern=`"abba"`, words=`"dog dog dog dog"` → `false`

## Approach hint

Bidirectional letter-word and word-letter mapping arrays/lists; linear scan
with consistency check at each position.
