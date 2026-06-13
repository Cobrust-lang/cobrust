# `str * int` repetition — `"ab" * 3 == "ababab"`

> ADR-0097. The `*` operator now repeats a string: `s * n` (and the
> symmetric `n * s`) yields `s` concatenated `n` times — semantics
> identical to Python.

## Examples first

```cobrust
fn main() -> i64:
    print("ab" * 3)        # ababab
    print(3 * "ab")        # ababab   (symmetric — either order works)
    print("=" * 10)        # ==========
    let n: i64 = 2 + 1
    print("ab" * n)        # ababab   (a computed count, not just a literal)
    print(len("ab" * 3))   # 6        (the result is a usable str)
    return 0
```

Matches Python: `"ab" * 3 == "ababab"`, `3 * "ab" == "ababab"`.

## The count rules (CPython 3)

| Expression | Result | Why |
|---|---|---|
| `"x" * 0` | `""` | a zero count yields the empty string |
| `"x" * 1` | `"x"` | a count of 1 is a copy |
| `"x" * -2` | `""` | a **non-positive** count yields the empty string — NEVER a trap |
| `"ab" * 3` | `"ababab"` | the common case |

A zero or negative count is NOT an error in Python, and it is not one in
Cobrust either — the program builds, runs, and exits 0, printing the empty
string.

## Codepoint-faithful

Repetition concatenates WHOLE strings, so a boundary never lands inside a
multi-byte UTF-8 codepoint:

```cobrust
fn main() -> i64:
    print("é" * 2)         # éé      (U+00E9 repeated, never split)
    print(len("é" * 2))    # 4       (len is the BYTE length: "éé" is 4 bytes)
    return 0
```

Matches Python: `"é" * 2 == "éé"`.

## Both operand orders

Python allows the count on either side — `s * n` and `n * s` mean the same
thing. Cobrust normalizes both to "the `str` is the receiver, the `int` is
the count":

```cobrust
let line: str = "-" * 40        # right-int
let line2: str = 40 * "-"       # left-int, same result
```

## What is still a type error

The `*` operator only repeats when exactly ONE operand is a `str` and the
other is an `int`. Everything else is rejected at compile time, exactly
like Python raises a `TypeError`:

- `"a" * "b"` — `str * str` is a type error (you cannot repeat by a string).
- `"a" * 1.5` — `str * float` is a type error (the count must be an `int`).

## Design notes

- **Additive, not a fix**: before ADR-0097, `"ab" * 3` was a CLEAN reject
  (`error[Type]: type mismatch: expected str, found i64`) — it was not a
  silent miscompile. This change makes the common idiom WORK; it does not
  fix a wrong value.
- **Runtime**: `__cobrust_str_repeat(s, n)` builds the result with one
  capacity-reserved allocation (`str::repeat`) and mints a fresh `str`
  buffer (the same mint path `__cobrust_str_slice` uses). The source `s`
  is borrowed (read, not consumed); the fresh result is dropped exactly
  once at its scope.
- This directly serves the §2.5 "LLM-first" principle: `"sep" * n` is an
  idiom an LLM writes constantly (dividers, padding, fixed-width fills).
  Rejecting it would force the LLM to work around its Python priors on a
  very common form.
