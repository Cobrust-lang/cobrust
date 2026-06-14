# `for c in <str>:` — CODEPOINT iteration

> ADR-0101 (closes F88). A `for` loop can now iterate a `str` directly,
> binding each loop variable to **one Unicode codepoint** (a length-1
> `str`) — exactly like Python. §2.5 LLM-first: this is one of the most
> common Python idioms, so the compiler now accepts it ex ante.

## Examples first

```cobrust
fn main() -> i64:
    for c in "hi":
        print(c)          # h
                          # i
    return 0
```

Matches Python: `for c in "hi": print(c)` prints `h` then `i`.

Iterate a variable, not just a literal — and the string stays usable
afterward (it is only **borrowed**, never consumed):

```cobrust
fn main() -> i64:
    let s: str = "abc"
    for c in s:
        print(c)          # a, b, c
    for c in s:
        print(c)          # a, b, c   (s is still usable)
    return 0
```

## Why codepoint, not byte

A `str` iterates by **Unicode codepoint** — a multi-byte character is
**one** iteration, never split into its UTF-8 bytes:

```cobrust
fn main() -> i64:
    for c in "héllo":     # 'é' is 2 UTF-8 bytes, ONE codepoint
        print(c)          # h, é, l, l, o   (FIVE iterations, not six)
    return 0
```

So `"héllo"` yields 5 iterations (h, é, l, l, o), and each `c` is a
fully-formed length-1 `str` you can concatenate, measure, or compare:

```cobrust
fn main() -> i64:
    for c in "xy":
        print(c + "!")    # x!  then  y!
    return 0
```

> **Note — `len(str)` still returns the BYTE count.** This is a separate
> pre-existing divergence from Python (which returns the codepoint count)
> and is **out of scope** for the iteration feature. The *iteration count*
> is codepoint-accurate (`"héllo"` runs the body 5 times); `len("héllo")`
> currently returns 6 (bytes). Don't use `len(s)` to predict the iteration
> count of a non-ASCII string.

## `continue` and `break` work

The string loop reuses the same length-bound index machinery as the list
loop, so `continue` (skip a codepoint) and `break` (stop early) behave
exactly as you'd expect — and the loop always **terminates**:

```cobrust
fn main() -> i64:
    for c in "hello":
        if c == "l":
            continue          # skip the two 'l's
        print(c)              # h, e, o
    return 0
```

## Empty string

`for c in "":` runs the body zero times (no iterations), like Python.

## Memory & ownership

- Each loop variable `c` is a **fresh, owned** length-1 `str` minted that
  iteration. There is **no double-free**: the source string is only read
  (never consumed), and each `c` owns its own copy.
- A 1000-codepoint string iterates cleanly and exits without error.

> A per-iteration allocation of `c` is **leaked** under a separate,
> pre-existing loop-body-drop gap (finding F82) — this is tracked
> independently and does not affect correctness (no crash, no
> double-free). It is named follow-up work, not part of F88.

## Scope: `for` loops only (for now)

String iteration is wired for the **`for` loop**. A `str` is **not yet**
iterable inside a list/set/dict comprehension or on the right of the `in`
operator — both are still **rejected at compile time** (a clean type error,
not a crash):

```cobrust
fn main() -> i64:
    let xs: list[str] = [c for c in "hi"]   # REJECTED at `cobrust check`
    if "e" in "hello":                       # REJECTED at `cobrust check`
        print("x")
    return 0
```

This is deliberate: those forms have no string support in the lower layers
yet, so rejecting them up front (the §2.5-A "catch errors at compile time"
rule) beats a confusing later failure. Use a `for` loop to walk a string's
codepoints today.

## Design notes

- **F88 fix**: before ADR-0101, `for c in "hi":` was a clean compile-time
  rejection (`str` cannot be used in a `for` loop, exit 2) — never a
  silent miscompile. ADR-0101 lifts that deferral for `str`.
- **Runtime**: the loop bound is `__cobrust_str_char_count` (codepoint
  count, NOT byte length) and each value is `__cobrust_str_char_at(s, i)`
  (codepoint-addressed, the same primitive `s[i]` uses — see the
  [str indexing & slicing docs](str-slicing.md)). The two agree
  codepoint-for-codepoint, so a multi-byte char is exactly one iteration.
- This directly serves the §2.5 "LLM-first" principle: an LLM writes
  `for c in s:` from its Python priors; rejecting it forced a non-idiomatic
  index-loop rewrite.
