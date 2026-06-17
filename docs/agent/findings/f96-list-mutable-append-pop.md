---
finding_id: F96
title: '`xs.append(v)` / `xs.pop()` REJECT — core list mutation is unimplemented (§2.5 training-data-overlap gap)'
date: 2026-06-18
status: resolved
resolved_by: ADR-0109 (2026-06-18)
severity: major
discovered_by: §2.5 LLM-first builtin-coverage audit (2026-06-18, F90/F92/F93/F94/F95 sibling)
relates_to: ["claude.md:§2.5", "claude.md:§2.2", "adr-0090", "adr-0108"]
---

# F96 — `list.append(v)` / `list.pop()` unimplemented

## What (verified at HEAD a8836f75)

`xs.append(v)` and `xs.pop()` both REJECTED at build (exit 2,
`method `append`/`pop` not found on `list``). The two most common list
mutation idioms in Python did not compile.

Python semantics:
- `xs.append(v)` mutates `xs` IN PLACE (grows by 1), returns `None`.
- `xs.pop()` removes + RETURNS the LAST element, mutating `xs` (shrinks by
  1); `[].pop()` raises `IndexError`.

This was an ADDITIVE gap (a CLEAN reject, NOT a silent miscompile): the
program did not compile, so no wrong value was ever produced. The cost was
purely first-try failure.

## Why it matters (§2.5 LLM-first)

`xs.append(x)` appears in almost every Python program — accumulate in a
loop, build-then-return, stack/queue, BFS/DFS frontiers. `xs.pop()` is the
canonical "remove + use the last element". Their absence is a direct hit
to §2.5's *Maximize-overlap-with-training-data*: the LLM writes
`xs.append(x)` from its Python priors and the build rejects it. (Cobrust
had a Rust-flavoured `xs.push(v)` alias, but an LLM does not write `push`
for a Python list — §2.5 favours the Python-canonical name.)

## The load-bearing design problem

Two distinct correctness surfaces:

1. **Mutation persistence + drop-once.** The receiver `xs` must stay the
   SAME live handle across the call (append/pop mutate through the
   pointer), and `xs` must still drop EXACTLY once at scope exit. Lists are
   operand-`is_copy_type`, so reading `xs` as `Operand::Copy` (not Move)
   achieves both — the borrow checker does not mark `xs` consumed and the
   drop schedule keeps its single scope-exit drop.

2. **Per-element-type slot encoding.** The list C-ABI is i64-SLOT. A
   `list[float]` stores each element as the f64 bit pattern. A NATIVE f64
   `append` operand must be re-encoded to the slot — handled by a `_float`
   runtime twin (the `min`/`sum`/`sort` `_int`/`_float` ABI pattern), so no
   codegen-side bitcast is needed. `pop` must decode the slot back to the
   element type, and its `_callret` dest must carry the element type (the
   monomorphic PRELUDE `list_pop -> i64` stub would otherwise force every
   pop to the int symbol — a silent f64-as-i64 §2.2 miscompile).

3. **Empty-pop policy.** `[].pop()` must TRAP (exit 3, §2.2 — CPython
   `IndexError` parity), via `crate::panic::panic`, NOT a silent sentinel
   `0` (an in-band wrong value §2.2 forbids) and NOT a raw `assert!` (which
   aborts SIGABRT with a path-leaking backtrace across `extern "C"` — the
   F79B/F92 lesson).

## The owned-element subtlety (deferred)

For `list[str]` / `list[list]` (owned-element lists) the element ownership
must transfer: `append` MOVES the operand INTO the list's `drop_elems`
(the caller must NOT also drop it); `pop` transfers ownership OUT to the
receiving binding (the list must NOT free it — pop removing it from the
length so `drop_elems` won't reach it). Getting this exactly right (no
double-free, no leak) is a separate, audit-worthy surface. F96 SCOPES to
the Copy-scalar element types (`int`/`float`/`bool` — no ownership
transfer) and CLEANLY REJECTS owned-element append/pop at type-check
(exit 2) with a §2.5-B fix-printing message
(`TypeError::UnsupportedListMutate`).

## Resolution

ADR-0109: `append`/`pop` for `list[int]` / `list[float]`; runtime
`__cobrust_list_pop` + `_append_float` / `_pop_float` twins (empty-pop
TRAP via the project panic convention); element-type dispatch from the
value arg (append) / dest type (pop); owned-element §2.5-B reject.

## Follow-ups (noted for the next increment)

- Owned-element `list[str]` / `list[list]` append/pop (ownership transfer).
- `pop(i)` (indexed pop, Python negative-index normalized).
- `insert(i, v)`, `remove(v)`, `extend(other)`, in-place `list.sort()`.
