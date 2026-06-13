# `list` indexing & slicing — element-addressed

> ADR-0096 (fixes F81). The `list` index operator `xs[i]` and slice
> operator `xs[lo:hi]` now work correctly, with semantics identical to
> Python — **addressed by element** (a list is the element-addressed peer
> of byte-addressed `bytes` and codepoint-addressed `str`).

## Examples first

```cobrust
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30]
    print(xs[0])          # 10
    print(xs[-1])         # 30     (last element, no longer 0)
    let ys: list[i64] = xs[1:3]
    print(len(ys))        # 2      (a fresh [20, 30])
    print(ys[0])          # 20
    print(ys[1])          # 30
    return 0
```

Matches Python: `[10,20,30][-1] == 30`, `[10,20,30][1:3] == [20,30]`.

## Negative scalar index `xs[-1]` (Python from-end indexing)

A **negative scalar index** counts from the end, exactly like Python:
`xs[-1]` is the **last element**, `xs[-2]` the second-to-last, and so on.

```cobrust
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30]
    print(xs[-1])     # 30
    print(xs[-2])     # 20
    print(xs[-3])     # 10
    return 0
```

> **Earlier behavior (superseded).** Before ADR-0096, `xs[-1]` silently
> returned `0` (the F81 §2.2 bug) — every negative index, and every
> positive out-of-range index, quietly produced `0`. It now returns the
> correct element (negatives) or traps loudly (out of range).

## Out-of-range scalar index traps (never a silent value)

A scalar index that is genuinely out of range — in **either** direction
(`xs[100]` past the end, or `xs[-100]` past the start) — **traps** at
runtime (a clean `list index out of range: i=.. len=..` message, exit 3),
exactly like Rust's own slice-OOB panic. It is **never** a silent `0`.

```cobrust
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30]
    print(xs[100])    # TRAPS: list index out of range: i=100 len=3
    return 0
```

The program *builds* fine — the trap is a **runtime** guard, because the
index value is only known when the program runs.

## What works now

| Form | Result | Notes |
|---|---|---|
| `xs[i]` | `T` | the `i`-th element; a **negative** `i` counts from the end (`xs[-1]` is the last) |
| `xs[lo:hi]` | `list[T]` | slice (returns a fresh list); element range `[lo, hi)`; Python-style clamp on out-of-bounds |

Slicing uses Python's clamp semantics — an out-of-range high bound is
narrowed to the length, a reversed range yields the empty list (never an
error):

```cobrust
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30, 40]
    let a: list[i64] = xs[1:99]
    print(len(a))     # 3      (high bound clamped to length -> [20,30,40])
    let b: list[i64] = xs[3:1]
    print(len(b))     # 0      (reversed range -> [])
    return 0
```

Each slice op produces a **fresh** list owned by your scope, freed exactly
once at scope exit; the input `xs` is only **borrowed** (read), never
consumed — so you can slice the same `xs` repeatedly.

## Slice shapes not yet supported (rejected at compile time)

Only the "both bounds explicit, non-negative, default step" `xs[lo:hi]`
form is supported. Every other shape is **rejected** at the
`cobrust check` stage (`UnsupportedSliceShape`) rather than silently
miscompiling — this is the §2.5-A "catch errors at compile time"
principle, and the diagnostic prints the correct form `xs[1:len(xs)]`:

| Shape | Status |
|---|---|
| `xs[1:]` / `xs[:3]` / `xs[:]` (open-ended) | rejected |
| `xs[0:4:2]` (stepped) | rejected |
| `xs[1:-1]` (negative bound) | rejected |

Supporting these is named follow-up work in ADR-0096. Until they land,
write explicit non-negative bounds.

## Design notes

- **F81 fix**: before the fix, `print([10,20,30][-1])` silently printed
  `0` (BUG 1), and `let ys = xs[1:3]` built then **crashed** at runtime
  with `misaligned pointer dereference` — list slicing was an
  unimplemented stub that returned the integer `0` used as a list handle
  (BUG 2, undefined behavior). Both are now fixed and byte-for-byte
  aligned with CPython 3.
- **Runtime**: `__cobrust_list_get` (normalizes a negative index `len + i`,
  traps a true out-of-range read) and `__cobrust_list_slice` (mints a fresh
  list for `[lo, hi)`), mirroring the `str`/`bytes` index/slice machinery
  but **element-addressed**.
- This completes the cross-type indexing arc: `str` (by codepoint),
  `bytes` (by byte), and `list` (by element) are all index/slice-correct
  with from-end negative indexing, out-of-range traps, and bounded
  `lo:hi` slices.
- This directly serves the §2.5 "LLM-first" principle: an LLM writes
  `xs[i]` / `xs[-1]` / `xs[lo:hi]` from its Python priors; a silent `0`
  sentinel or a UB crash would diverge on the most common list idioms.
