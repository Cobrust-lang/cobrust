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

## Cascade-completion repair (F83/F92-class lesson)

The F96 feature commit (`6a5c20ad`) was SOUND for its feature surface
(append/pop correct, pop-empty traps exit 3, 5000-iter hammer 0 leaks,
str/owned cleanly rejected) but it did NOT compile as a workspace: the new
`TypeError::UnsupportedListMutate` variant was threaded through
`error_ux.rs`, `fix_safety.rs`, `cobrust-types-parity`, and
`lsp/diagnostic.rs`, but MISSED the `.cb`-mirror crate `cobrust-types-cb`.
Its `type_error_cb_from_rust` match is exhaustive with no wildcard, so the
new variant gave `error[E0004]: non-exhaustive patterns` →
`cobrust-types-cb` failed to build → the WHOLE workspace failed
`cargo build --workspace` and `cargo test --workspace` (no tests ran).
The F96 commit message's `cargo test --workspace --locked exit 0` claim was
therefore false at that commit — verifying only `-p cobrust-cli` masked it.

The repair completes the `cobrust-types-cb` cascade, mirroring the closest
sibling (`UnsupportedSliceShape`) but carrying the two owned `String`s
(`method` + `elem`) cloned from the Rust side per ADR-0055b §6 risk 1:

- `error_cb.rs`: new `TypeErrorCb::UnsupportedListMutate { method, elem,
  span }` variant; a `type_error_cb_from_rust` arm; a `Display` arm
  rendering BYTE-IDENTICALLY to the thiserror `#[error(...)]` attribute on
  the Rust side; payload-free `canonicalize` bucket; variant-name arm.
- `fix_safety_cb.rs`: `LocalEdit` tier, mirroring Rust `fix_safety.rs`.
- `tests/error_display_parity.rs`: two `SuggestionText` arms (returns
  `None` — no `suggestion` field) + a new byte-parity Display test
  `test_display_unsupported_list_mutate`.
- `cobrust-types-parity/src/lib.rs`: `#[allow(clippy::too_many_lines)]` on
  `Canonicalize::canonicalize` — F96's added arm pushed the flat
  per-variant match to 101 lines (the workspace clippy gate, also unrun for
  F96, surfaced this).

Lesson (sibling F83/F92): a new `TypeError` variant's error-cascade MUST
include the `cobrust-types-cb` mirror; the workspace-level
`cargo build --workspace` + `cargo clippy --workspace` are the only gates
that catch a missed mirror — a per-crate `-p cobrust-cli` run is green-blind
to it.
