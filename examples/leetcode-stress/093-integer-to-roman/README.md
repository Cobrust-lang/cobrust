# LC-093 Integer to Roman

**Category**: Math
**Difficulty**: Medium

## Algorithm

Convert a positive integer in the range 1 to 3999 into its Roman numeral
representation. Roman numerals use seven base symbols (I, V, X, L, C, D, M)
plus six subtractive combinations (IV=4, IX=9, XL=40, XC=90, CD=400, CM=900)
to express values without repetitions of four or more identical symbols in a
row.

A greedy descent strategy works cleanly: maintain a sorted table of value-symbol
pairs from largest (M=1000) down to smallest (I=1). For each entry, repeatedly
append the symbol and subtract the value from the remainder until the remainder
falls below the current entry's value, then advance to the next smaller entry.
This process terminates when the remainder reaches zero.

The thirteen entries needed are, in descending order: 1000, 900, 500, 400, 100,
90, 50, 40, 10, 9, 5, 4, 1 paired with M, CM, D, CD, C, XC, L, XL, X, IX, V,
IV, I.

## Input format

```
Line 1: N — integer in range [1, 3999]
```

## Oracle

- N=3    → `III`
- N=4    → `IV`
- N=9    → `IX`
- N=58   → `LVIII`
- N=1994 → `MCMXCIV`

## Approach hint

Build the symbol table as parallel integer and string arrays of length 13.
Walk from index 0 (largest) to 12 (smallest), looping `while n >= value[i]`:
emit `symbol[i]` via `print_no_nl`, subtract `value[i]` from n. After the loop
over all 13 entries, print a newline.
