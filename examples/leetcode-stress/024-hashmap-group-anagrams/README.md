# LC-024 Hash Map Group Anagrams

**Category**: Hash Maps
**Difficulty**: Medium

## Algorithm

Given M strings (all lowercase), group them such that strings which are
anagrams of each other appear together. Output the groups separated by blank
lines; within each group print one word per line. The ordering of groups and
the ordering within groups both follow first-appearance order.

Because Cobrust lacks a hash map keyed on strings, emulate grouping with
parallel lists: maintain a list of "canonical keys" (each is a frequency-
signature string) and a parallel list of group-start indices into the input.
For each new word, compute its 26-letter frequency signature, search for a
matching signature in the canonical-keys list, and append to that group.

Frequency signature encoding: for word w, create a 26-character string where
position i holds a digit representing how many times the i-th letter appears.
This makes anagram detection exact and order-independent.

## Input format

```
Line 1: M
Lines 2..M+1: one word per line (lowercase)
```

## Oracle

- M=6, words `eat, tea, tan, ate, nat, bat` → groups: `eat tea ate` / `tan nat` / `bat`

## Approach hint

Frequency-signature string as group key; parallel key/group-index lists;
first-seen ordering.
