# `tuple` indexing — per-position, constant-index

> ADR-0106 (fixes F83). The tuple index operator `t[i]` now works
> correctly, with semantics identical to Python — **addressed by
> position**, and **type-correct per position** (a tuple is
> heterogeneous, so each position can hold a different type). This is the
> final member of the cross-type indexing arc (`str` by codepoint,
> `bytes` by byte, `list` by element, `tuple` by position).

## Examples first

```cobrust
fn main() -> i64:
    let t: (i64, str) = (7, "x")
    print(t[0])          # 7      (no longer the silent 0)
    print(t[1])          # x
    let u: (i64, str, i64) = (1, "a", 2)
    print(u[2])          # 2
    print(u[0] + u[2])   # 3      (real integers, per-position typed)
    print(t[-1])         # x      (last element, Python from-end)
    return 0
```

Matches Python: `(7, "x")[0] == 7`, `(1, "a", 2)[2] == 2`,
`(7, "x")[-1] == "x"`.

## Heterogeneous, per-position typing

A tuple's elements may each be a **different type**. Indexing returns the
**exact type at that position** — `(i64, str)[0]` is an `i64`,
`(i64, str)[1]` is a `str`:

```cobrust
fn main() -> i64:
    let t: (i64, str, i64) = (1, "a", 2)
    print(t[0])   # 1   (i64)
    print(t[1])   # a   (str)
    print(t[2])   # 2   (i64)
    return 0
```

## Negative constant index `t[-1]` (Python from-end)

A **negative** index counts from the end, like Python: `t[-1]` is the
last element, `t[-2]` the second-to-last. Because the index is a
compile-time constant, this is resolved (and bounds-checked) **at
compile time** — no runtime cost, no runtime trap.

```cobrust
fn main() -> i64:
    let t: (i64, i64, i64) = (10, 20, 30)
    print(t[-1])     # 30
    print(t[-2])     # 20
    print(t[-3])     # 10
    return 0
```

## The index must be a CONSTANT

Because a tuple is heterogeneous, the element type is only knowable when
the index is a **compile-time constant**. A **dynamic** index has no
single static type, so it is **rejected at compile time** (the §2.5-A
"catch errors at compile time" principle) — not a silent wrong-type read:

```cobrust
fn main() -> i64:
    let t: (i64, i64) = (1, 2)
    let i: i64 = 1
    print(t[i])      # COMPILE ERROR: a tuple needs a CONSTANT integer index
    return 0
```

> If you need dynamic indexing, use a `list` (homogeneous, element-typed)
> instead — the diagnostic says exactly this.

## Out-of-bounds constant index is rejected at compile time

A constant index that is out of range — in **either** direction
(`(1,2)[5]` past the end, or `(1,2)[-5]` past the start) — is **rejected
at compile time** (unlike `list`/`str`, where a runtime index traps at
run time). The index value is a constant, so the compiler catches it
before the program ever runs:

```cobrust
fn main() -> i64:
    let t: (i64, i64) = (1, 2)
    print(t[5])      # COMPILE ERROR: tuple index out of bounds
    return 0
```

## What works now

| Form | Result | Notes |
|---|---|---|
| `t[i]` (constant `i`) | the i-th element's exact type | per-position; a **negative** constant counts from the end (`t[-1]` is last) |
| `t[i]` (dynamic `i`) | compile error | a tuple needs a constant index — use a `list` for dynamic indexing |
| `t[i]` (constant OOB) | compile error | out of bounds, caught at compile time |

Tuple **slicing** (`t[lo:hi]`) is not yet supported (in Python it returns
a tuple); it is named follow-up work.

## Design notes

- **F83 fix**: before the fix, `(7, "x")[0]` *built fine* and silently
  printed `0` (CPython `7`) — a §2.2 silent miscompile. Two layers were
  stubs: the MIR lowering had no tuple-index branch (it fell through to a
  no-op that read the wrong slot), and the LLVM backend lowered a tuple to
  a null pointer (both construction **and** field reads were
  unimplemented). Now a tuple lowers to a real struct value, constructed
  field-by-field, and `t[i]` reads the i-th field directly.
- This completes the cross-type indexing arc: `str` (by codepoint),
  `bytes` (by byte), `list` (by element), and `tuple` (by position) are
  all index-correct.
- This directly serves the §2.5 "LLM-first" principle: an LLM writes
  `t[0]` / `t[-1]` from its Python priors; a silent `0` would diverge on
  the most basic tuple idiom — and a dynamic `t[i]` is caught at compile
  time with a fix-printing message rather than miscompiling.
