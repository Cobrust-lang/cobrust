# LC-027 Hash Map Longest Substring Without Repeating Characters

**Category**: Hash Maps
**Difficulty**: Medium

## Algorithm

Given a string, find the length of the longest contiguous substring that
contains no repeated characters. Output that maximum length.

The sliding window approach with a character-position tracking table is
canonical: maintain a left boundary of the current window and a right
boundary that advances one character at a time. For each character at the
right boundary, check whether it has been seen before and its last-seen index
is within the current window. If so, advance the left boundary to one past
the duplicate's last-seen position. Update the character's last-seen index.
Track the maximum window size seen.

Emulate the character-to-index map with a 128-slot array (ASCII indexed),
initialized to -1 (meaning "not yet seen or outside current window").

## Input format

```
Line 1: the input string
```

## Oracle

- `"abcabcbb"` → `3` (window `"abc"`)
- `"bbbbb"` → `1`
- `"pwwkew"` → `3` (window `"wke"`)

## Approach hint

Sliding window with 128-slot last-seen-index array; advance left past any
duplicate that falls within the window.
