# `str` indexing & slicing — CODEPOINT-addressed

> ADR-0094 (fixes F78). The `str` index operator `s[i]` and slice
> operator `s[lo:hi]` now work correctly, with semantics identical to
> Python — **addressed by Unicode codepoint, not by byte**.

## Examples first

```cobrust
fn main() -> i64:
    let s: str = "hello"
    print(s[1:4])         # ell    (no longer the whole "hello")
    print(s[1])           # e      (a single codepoint, a str)
    print(len(s[1:4]))    # 3
    return 0
```

Matches Python: `"hello"[1:4] == "ell"`, `"hello"[1] == "e"`.

## Why codepoint, not byte

This is the key difference between `str` and `bytes`. Python's `str`
indexes by **Unicode codepoint** — a slice NEVER splits a multi-byte
UTF-8 codepoint:

```cobrust
fn main() -> i64:
    let u: str = "héllo"     # 'é' is 2 UTF-8 bytes
    print(u[1:3])            # él    (codepoints [1,3), NOT bytes)
    print(u[1])              # é     (a single codepoint)
    print(u[0:2])            # hé

    let z: str = "你好世界"   # each character is 3 bytes
    print(z[1:3])            # 好世
    return 0
```

A byte-based slice would cut `é` in half in `"héllo"[1:3]`, producing
INVALID UTF-8 — and a Cobrust `str` is always valid UTF-8 (§2.2 forbids
any silent data corruption). With codepoint slicing, a boundary always
lands on a character boundary, so the result is **always valid UTF-8** —
no snap-to-boundary and no mid-slice trap are needed.

> Contrast: `bytes` is indexed **by byte** (`b[i] -> int`, see the
> [bytes docs](bytes-primitive.md)) because each byte is independent;
> `str` is indexed **by codepoint** (`s[i] -> str`, a length-1 string).

## What works now

| Form | Result | Notes |
|---|---|---|
| `s[i]` | `str` | the `i`-th **codepoint**, as a length-1 `str` (matches Python `"héllo"[1] == "é"`, NOT a byte) |
| `s[lo:hi]` | `str` | slice (returns a fresh `str`); codepoint range `[lo, hi)`; Python-style clamp on out-of-bounds |

Slicing uses Python's clamp semantics — an out-of-range high bound is
narrowed to the length, a reversed range yields the empty string (never
an error):

```cobrust
fn main() -> i64:
    let s: str = "hello"
    print(s[1:99])   # ello   (high bound clamped to length)
    print(s[3:1])    # (blank line, reversed range -> "")
    print(s[0:5])    # hello
    return 0
```

Each index / slice op that produces a new `str` gives you a **fresh**
value owned by your scope, freed exactly once at scope exit; the input
`s` is only **borrowed** (read), never consumed — so you can index the
same `s` repeatedly:

```cobrust
fn main() -> i64:
    let s: str = "hello"
    let mid: str = s[1:4]
    print(mid)        # ell
    print(s[0])       # h    (s is still usable)
    print(s[1:3])     # el
    return 0
```

## Slice shapes not yet supported (rejected at compile time)

Only the "both bounds explicit, non-negative, default step" `s[lo:hi]`
form is supported. Every other shape is **rejected** at the
`cobrust check` stage (`UnsupportedSliceShape`) rather than silently
miscompiling as before — this is the §2.5-A "catch errors at compile
time" principle, and the diagnostic prints the correct form
`s[1:len(s)]`:

| Shape | Status |
|---|---|
| `s[1:]` / `s[:3]` / `s[:]` (open-ended) | rejected |
| `s[0:4:2]` (stepped) | rejected |
| `s[1:-1]` (negative bound) | rejected |

Supporting these is named follow-up work in ADR-0094. Until they land,
write explicit non-negative bounds.

## Negative scalar index `s[-1]` (rejected at compile time)

A **negative-literal scalar index** — `s[-1]` (the Python "last
codepoint" idiom), `s[-2]`, etc. — is also **rejected** at `cobrust
check` (`UnsupportedSliceShape`, the same §2.5-A compile-time-catch the
slice shapes use), rather than silently returning the empty string `""`
it produced before (F79). The diagnostic prints the fix: for the last
codepoint, write `s[len(s) - 1]` (a non-negative index).

```cobrust
fn main() -> i64:
    let s: str = "hello"
    # let c: str = s[-1]      # REJECTED — write s[len(s) - 1] instead
    print(s[len(s) - 1])      # o   (the last codepoint, the supported form)
    return 0
```

Only the **literal** negative index is caught. A non-literal index `s[i]`
where `i` is a variable still type-checks (a runtime-negative `i` falls
through to a sentinel — a known divergence). Full Python from-end
indexing (`s[-1] == s[len-1]`) + an out-of-bounds panic are named
Option-B follow-up work in ADR-0094 (F79).

## Design notes

- **F78 fix**: before the fix, `print("hello"[1:4])` silently printed
  `hello` (the whole string) at exit 0, and the `s[i]` scalar index had
  the same bug. Both are now fixed and byte-for-byte aligned with
  CPython 3.
- **Runtime**: `__cobrust_str_char_at` / `__cobrust_str_slice`, mirroring
  the `bytes` Phase-2 slice machinery but codepoint-addressed.
- This directly serves the §2.5 "LLM-first" principle: an LLM writes
  `s[i]` from its Python priors expecting codepoint semantics; byte
  semantics would silently diverge on every non-ASCII string.
