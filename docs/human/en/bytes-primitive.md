# `bytes` — a first-class immutable byte buffer

> ADR-0093. `bytes` is now a real runtime value: you can write a `b"..."`
> literal, measure it with `len(b)`, and read a byte with `b[i]`.

## Example first

```cobrust
fn main() -> i64:
    let b: bytes = b"abc"
    print(len(b))   # 3
    print(b[0])     # 97  (the byte value of 'a', an int)
    print(b[1])     # 98
    print(b[2])     # 99
    return 0
```

`bytes` is **"a `str` without the UTF-8 rule"**: an immutable, heap-stored
sequence of raw bytes. Unlike `str` (which is always valid UTF-8), a
`bytes` value can hold any byte — including non-text bytes:

```cobrust
fn main() -> i64:
    let raw: bytes = b"\xff\x00\xfe"
    print(len(raw))   # 3
    print(raw[0])     # 255
    print(raw[1])     # 0
    print(raw[2])     # 254
    return 0
```

## What you can do (Phase 1)

| Form | Result | Notes |
|---|---|---|
| `b"..."` | `bytes` | a byte-string literal (any byte, incl. `\xNN` escapes) |
| `len(b)` | `int` | the number of bytes |
| `b[i]` | `int` | the `i`-th byte, `0..255` (matches Python's `b"abc"[0] == 97`) |

A `bytes` value behaves like every other Cobrust heap value: it is owned
by your `.cb` scope and freed automatically once, when the scope ends.
You never write a free — and there is no garbage collector. This is the
same ownership discipline `str` and `list` already use.

## Why this design?

- **It matches what an LLM writes.** `b"..."`, `len(b)`, and `b[i]` are
  exactly the Python forms. `b[i]` returns an `int` (the byte value),
  not a 1-byte `bytes` — that is Python 3's behaviour, and it is what an
  agent writes on the first try (CLAUDE.md §2.5, the LLM-first north
  star).
- **Bytes stay exact.** Before ADR-0093, a `b"..."` literal was forced
  through the string machinery, which assumes UTF-8 — so a non-text byte
  like `\xff` was silently corrupted. The dedicated `bytes` runtime keeps
  every byte intact.
- **No double-free, no leak.** A `bytes` value is freed exactly once at
  scope exit, even inside a tight loop (the runtime is verified with a
  1000-iteration mint/read/drop stress test).

## What is deferred (honest roadmap)

These are **not** in Phase 1 yet (ADR-0093 Phase 2):

- Slicing `b[lo:hi] -> bytes`
- Concatenation `b1 + b2` and equality `b1 == b2`
- Methods `.hex()`, `.decode()` (bytes → str) and `str.encode()` (str → bytes)
- The dora stream accessor `event.data_bytes()` / `event.send_output_bytes(...)`

You can hold, measure, and index a `bytes` value today; slicing and
concatenation land in a follow-up.

## See also

- `docs/agent/adr/0093-bytes-runtime-c-abi.md` — the runtime + C-ABI design.
- `docs/human/en/design-philosophy.md` — why Cobrust drops Python's silent coercions.
