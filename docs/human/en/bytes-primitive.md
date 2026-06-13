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

## What you can do

| Form | Result | Notes | Phase |
|---|---|---|---|
| `b"..."` | `bytes` | a byte-string literal (any byte, incl. `\xNN` escapes) | 1 |
| `len(b)` | `int` | the number of bytes | 1 |
| `b[i]` | `int` | the `i`-th byte, `0..255` (matches Python's `b"abc"[0] == 97`); a negative `i` counts from the end (`b"abc"[-1] == 99`); a true out-of-range index traps | 1 |
| `b[lo:hi]` | `bytes` | a slice (a fresh `bytes`); clamps like Python on out-of-range | 2 |
| `b1 + b2` | `bytes` | concatenation (a fresh `bytes`) | 2 |
| `s.encode()` | `bytes` | the UTF-8 bytes of a `str` | 2 |
| `b.decode()` | `str` | decode UTF-8 bytes back to a `str` (see below) | 2 |
| `b.hex()` | `str` | lowercase hex, e.g. `b"\xff\x00".hex() == "ff00"` | 2 |

A `bytes` value behaves like every other Cobrust heap value: it is owned
by your `.cb` scope and freed automatically once, when the scope ends.
You never write a free — and there is no garbage collector. This is the
same ownership discipline `str` and `list` already use. Every operation
above that produces a new `bytes` or `str` (slice / concat / encode /
decode / hex) gives you a **fresh** value your scope owns; the inputs are
only read, never consumed.

## Slicing, concatenation, and the `str` bridge (Phase 2)

```cobrust
fn main() -> i64:
    let b: bytes = b"hello"
    print(len(b[1:4]))       # 3   (b"ell")
    print(len(b + b))        # 10  (b"hellohello")

    # str <-> bytes round-trip
    let s: str = "world"
    let encoded: bytes = s.encode()
    print(len(encoded))      # 5
    print(encoded.decode())  # world

    print(b.hex())           # 68656c6c6f
    return 0
```

Slicing clamps the way Python does — an out-of-range high bound is
trimmed to the length, and a backwards range yields an empty `bytes`
(never an error):

```cobrust
fn main() -> i64:
    let b: bytes = b"abcd"
    print(len(b[1:99]))   # 3   (clamped to b"bcd")
    print(len(b[3:1]))    # 0   (empty)
    return 0
```

## Scalar index `b[i]` — from-end negatives, OOB traps

A scalar index `b[i]` reads the `i`-th byte as an `int` (`0..255`). A
**negative** index counts from the end, exactly like Python — `b[-1]` is
the last byte (ADR-0095, F79 Option B). A **true out-of-range** index, in
either direction (`b[100]` or `b[-100]`), **traps** at runtime (a clean
`bytes index out of range: i=.. len=..` message, exit 3), never the silent
`-1` sentinel it used to return.

```cobrust
fn main() -> i64:
    let b: bytes = b"\x01\x02\xff"
    print(b[-1])     # 255   (last byte)
    print(b[-3])     # 1     (first byte, from the end)
    print(b[0])      # 1
    # print(b[100])  # would TRAP: bytes index out of range: i=100 len=3
    return 0
```

> Before ADR-0095, `b[-1]` silently returned `-1` (the F79 §2.2 bug); an
> interim fix rejected `b[-1]` at compile time. Now it just works, and a
> real out-of-range read traps loudly instead of returning a wrong value.

## Decoding invalid bytes — the no-silent-coercion rule

`b.decode()` reads the bytes as UTF-8. If the bytes are **not** valid
UTF-8, Cobrust does **not** quietly substitute a replacement character
and it does **not** silently truncate — that would be exactly the kind of
silent coercion Cobrust refuses (CLAUDE.md §2.2). Instead it **stops the
program** with a clear diagnostic that names the first bad byte:

```cobrust
fn main() -> i64:
    let b: bytes = b"\xff\xfe"
    let s: str = b.decode()   # stops here
    print(s)                  # never runs
    return 0
```

```
cobrust panic: bytes.decode: invalid utf-8 at byte 0
```

This is the same "stop loudly on a broken precondition" behaviour every
other unrecoverable error in Cobrust uses. A future release will add a
recoverable `Result`-returning form once that style is wired across the
standard library; until then, decoding invalid UTF-8 is a hard stop — but
it is **never** a silent corruption.

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

Phase 2 shipped slicing, concatenation, `.encode()` / `.decode()` /
`.hex()`. These are still **not** here yet — and each one is a **clear
compile error** that tells you the supported form, never a silent wrong
answer:

- **Comparing two `bytes`** (`b1 == b2`, `<`, `>`, …) is a compile error.
  The message tells you to compare `len(a)` with `len(b)`, or to compare
  `a.decode()` with `b.decode()` when both sides are known to be valid
  UTF-8. (Earlier this crashed the compiler; now it is a clean diagnostic.)
- **Negative / open-ended / stepped slices** (`b[1:]`, `b[:3]`, `b[0:4:2]`,
  `b[1:-1]`) are a compile error — only the simple `b[lo:hi]` form (with
  both non-negative bounds present) is supported. The message tells you to
  write both bounds, e.g. `b[1:len(b)]`. (Earlier these silently returned
  the whole buffer; now the compiler stops you with the fix.)
- **Negative-bound / open-ended / stepped SLICES** (`b[-2:]`, `b[1:]`,
  `b[0:4:2]`) stay a compile error — only the simple `b[lo:hi]` scalar-bound
  slice form is supported. (Negative SCALAR indexing `b[-1]` *is* now
  supported — see the next item; only the slice forms remain deferred.)
- A recoverable `Result`-returning `decode()` (today invalid UTF-8 stops
  the program; see above).

The dora stream accessor `event.data_bytes()` / `event.send_output_bytes(...)`
has **landed** (ADR-0076c B-1b) — a robotics node can now read an Arrow
`Binary`/`UInt8` payload as a `bytes` and emit one back, byte-exact. See
`docs/human/en/import-dora.md` for the surface.

## See also

- `docs/agent/adr/0093-bytes-runtime-c-abi.md` — the runtime + C-ABI design.
- `docs/human/en/design-philosophy.md` — why Cobrust drops Python's silent coercions.
