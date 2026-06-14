# `str` comparison — `<` `<=` `>` `>=` (lexicographic)

> ADR-0104 (closes F92). Two strings can be compared with the ordering
> operators `<`, `<=`, `>`, `>=` — **lexicographically by Unicode
> codepoint**, exactly like Python. §2.5 LLM-first: sorting and ordering
> strings is one of the most common things an LLM agent writes, so the
> compiler now accepts it ex ante (before F92 it CRASHED the compiler).

## Examples first

```cobrust
fn main() -> i64:
    print("abc" < "abd")      # True
    print("abc" > "abd")      # False
    print("a" <= "a")         # True
    print("b" >= "a")         # True
    return 0
```

Matches Python exactly: `"abc" < "abd"` is `True`.

A string that is a **prefix** of another is **less** than it, and the
empty string is the smallest:

```cobrust
fn main() -> i64:
    print("ab" < "abc")       # True  (prefix is less)
    print("abc" < "ab")       # False
    print("" < "a")           # True  (empty is the minimum)
    return 0
```

Use it where you'd expect — in an `if`, over variables, when sorting:

```cobrust
fn main() -> i64:
    let a: str = "apple"
    let b: str = "banana"
    if a < b:
        print("a before b")   # a before b
    return 0
```

`==` and `!=` already worked and are unchanged:

```cobrust
fn main() -> i64:
    print("abc" == "abc")     # True
    print("abc" != "abd")     # True
    return 0
```

## Comparison is by codepoint

Strings compare **lexicographically by Unicode codepoint**, the same order
Python uses. A non-ASCII character compares by its codepoint value:

```cobrust
fn main() -> i64:
    print("é" < "f")          # False  (é is U+00E9 = 233, f is U+0066 = 102)
    print("é" > "f")          # True
    return 0
```

This is identical to CPython, where `ord('é') == 233 > ord('f') == 102`.

## What still rejects (cleanly)

- **Mixed types** — comparing a `str` with a number is a **compile-time
  type error** (exit 2), never a crash:

  ```cobrust
  fn main() -> i64:
      print("abc" < 5)        # error[Type]: type mismatch: expected `str`, found `i64`
      return 0
  ```

- **`bytes` ordering** — `b"a" < b"b"` is **not yet supported** and
  rejects at compile time with a fix-printing message (compare `len(a)`
  with `len(b)`, or `.decode()` both sides when they are valid UTF-8). It
  is a clean rejection, never a crash.

## Design notes

- **F92 fix**: before ADR-0104, `"abc" < "abd"` **crashed the compiler**
  (`cobrust build` exited 101 with a codegen panic) — the type checker
  accepted it, then codegen had no path for string operands. This violated
  the rule that the compiler must never panic on type-checked input
  (§5.1). F92 implements the operator instead of rejecting it.
- **Runtime**: each comparison calls `__cobrust_str_cmp(a, b)`, which
  returns -1 / 0 / +1 (the sign of Rust's `str::cmp`). The result is then
  compared against 0 with the matching operator (`a < b` becomes
  `cmp(a, b) < 0`, and so on). The strings are only **borrowed**, never
  consumed, so they stay usable afterward.
- **Byte order = codepoint order**: Rust's `str::cmp` compares the UTF-8
  bytes, but UTF-8 is order-preserving, so byte order equals codepoint
  order for valid text — which is exactly Python's semantics.
- This serves the §2.5 "LLM-first" principle: an LLM writes `s1 < s2` from
  its Python priors (sorting, ordering, binary search); crashing on it
  forced a non-idiomatic rewrite.
