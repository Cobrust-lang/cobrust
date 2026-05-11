# LC-020 Two Pointers Backspace String Compare

**Category**: Two Pointers
**Difficulty**: Medium

## Algorithm

Given two strings where the character `#` represents a backspace keystroke,
simulate typing and determine whether the final typed results are identical.
Output `true` if both strings resolve to the same text, `false` otherwise.

One approach processes each string forward to produce its final form: maintain
a write cursor starting at index 0. For each character in the string, if it is
not `#`, write it to the output array and advance the cursor; if it is `#` and
the cursor is above 0, retreat the cursor (effectively deleting the previous
character). After processing, compare the two resulting character sequences up
to their final lengths.

## Input format

```
Line 1: first string (may include `#` characters)
Line 2: second string (may include `#` characters)
```

## Oracle

- `"ab#c"` and `"ad#c"` → `true` (both resolve to `"ac"`)
- `"a##c"` and `"#a#c"` → `true` (both resolve to `"c"`)
- `"a#c"` and `"b"` → `false`

## Approach hint

Stack-emulation write-cursor for each string; compare resulting character
arrays by length and content.
