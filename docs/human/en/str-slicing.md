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

## Negative scalar index `s[-1]` (Python from-end indexing)

A **negative scalar index** counts from the end, exactly like Python:
`s[-1]` is the **last codepoint**, `s[-2]` the second-to-last, and so on
(ADR-0095, F79 Option B). This is **codepoint-addressed**, not byte —
`"héllo"[-1] == "o"` and `"héllo"[-4] == "é"` (the multi-byte `é` is one
codepoint).

```cobrust
fn main() -> i64:
    let s: str = "héllo"
    print(s[-1])      # o    (last codepoint)
    print(s[-2])      # l
    print(s[-4])      # é    (codepoint-addressed, not a mid-byte cut)
    return 0
```

Negative indexing works for a variable index too — a runtime-negative `i`
in `s[i]` normalizes from the end the same way.

> **Earlier behavior (superseded).** Before ADR-0095, `s[-1]` silently
> returned the empty string `""` (the F79 §2.2 bug); an interim fix
> (ADR-0094 Option A) rejected `s[-1]` at compile time and asked you to
> write `s[len(s) - 1]`. ADR-0095 makes `s[-1]` *just work* — you no
> longer need the `len(s) - 1` workaround.

## Out-of-range scalar index traps (never a silent value)

A scalar index that is genuinely out of range — in **either** direction
(`s[100]` past the end, or `s[-100]` past the start) — **traps** at
runtime (a clean `str index out of range: i=.. len=..` message, exit 3),
exactly like Rust's own slice-OOB panic. It is **never** a silent empty
string. This also fixes the pre-existing positive-OOB hole (`s[100]` used
to silently return `""` too).

```cobrust
fn main() -> i64:
    let s: str = "hello"
    print(s[100])     # TRAPS: str index out of range: i=100 len=5
    return 0
```

The program *builds* fine — the trap is a **runtime** guard, because the
index value is only known when the program runs.

## Design notes

- **F78 fix**: before the fix, `print("hello"[1:4])` silently printed
  `hello` (the whole string) at exit 0, and the `s[i]` scalar index had
  the same bug. Both are now fixed and byte-for-byte aligned with
  CPython 3.
- **Runtime**: `__cobrust_str_char_at` / `__cobrust_str_slice`, mirroring
  the `bytes` Phase-2 slice machinery but codepoint-addressed. `char_at`
  normalizes a negative index (`len + i`, codepoint count) and traps on a
  true out-of-range read.
- **F79 Option B (ADR-0095)**: `s[-1]` from-end indexing + the OOB trap.
  This replaced the ADR-0094 Option-A interim reject (the runtime is now
  codepoint-correct, so returning the right value beats rejecting), and it
  closed the silent positive-OOB hole (`s[100]`) at the same time.
- This directly serves the §2.5 "LLM-first" principle: an LLM writes
  `s[i]` / `s[-1]` from its Python priors expecting codepoint, from-end
  semantics; byte semantics or a silent sentinel would diverge on the most
  common indexing idioms.
