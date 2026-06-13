---
finding_id: F81
title: "`list` index/slice — `xs[-1]` silently returns 0 (§2.2) + `xs[lo:hi]` is an unimplemented stub → UB crash (§2.2)"
date: 2026-06-13
status: resolved
resolved_date: 2026-06-13
resolution: "ADR-0096 — `__cobrust_list_get` now Python-normalizes a negative index (`len + i`) + TRAPS a true OOB (both directions) via crate::panic::panic (exit 3), no silent-0 sentinel; `__cobrust_list_slice` mints a fresh `list[i64]` for `xs[lo:hi]` (str/bytes-slice mirror, CPython clamp, Move-out drop-once); check.rs gets a `(Ty::List, IndexKind::Slice)` arm returning `Ty::List(elem)` for `lo:hi` + rejecting open/step/negative with TypeError::UnsupportedSliceShape. The str/bytes/list indexing arc is now complete."
severity: critical
relates_to: [adr:0093, adr:0094, adr:0095, adr:0096, "claude.md:§2.2", "claude.md:§2.5", "finding:f78", "finding:f79"]
discovered_by: a verify-the-gap probe (the LIST analogue of the str/bytes indexing arc)
---

# F81 — `list` index/slice correctness (two §2.2 bugs)

## What (verified at HEAD 0974388, the F80 follow-up tree)

The `list` index/slice operator surface had TWO §2.2 bugs — the LIST
analogue of the just-closed `str`/`bytes` arc (F78 slice + F79
negative-index/OOB-trap):

### BUG 1 — `xs[-1]` SILENT MISCOMPILE

```
# [10,20,30][-1]  -> 0   CPython: 30  (last element)    <- SILENT WRONG
# [10,20,30][100] -> 0   CPython: IndexError            <- SILENT WRONG
```

`__cobrust_list_get` (`crates/cobrust-stdlib/src/collections.rs`) did
`if i < 0 || i >= layout.len { return 0; }` — BOTH a negative index AND a
positive OOB returned the silent-`0` sentinel. `xs[-1]` (last element) is
the #1 Python indexing idiom — silently returning `0` is a §2.2
silent-miscompile and a §2.5 first-try trap (an LLM writes `xs[-1]`
constantly). EXACTLY the F79 bug for the str/bytes scalar accessors.

### BUG 2 — `xs[lo:hi]` UB / MEMORY-SAFETY CRASH

```
# let ys: list[i64] = xs[1:3]   builds OK, then CRASHES at runtime:
#   "misaligned pointer dereference"
```

List slicing was an UNIMPLEMENTED STUB. The generic `lower_index` helper
(`crates/cobrust-mir/src/lower.rs`) returned
`IndexKind::Slice { .. } => Ok(Operand::Constant(Constant::Int(0)))` — the
integer `0` used as a list handle → UB. `check.rs` had a
`(Ty::List(elem), IndexKind::Expr)` scalar arm but NO
`(Ty::List, IndexKind::Slice)` arm, so a slice fell through to a `Ty::List`
type but lowered to the UB stub.

## Contrast — str/bytes were already fixed

F78/F79 (ADR-0093/0094/0095) closed the SAME two bug classes for `str`
(codepoint) and `bytes` (byte): a from-end negative scalar index + an
OOB-trap, plus a real `lo:hi` slice with the open/step/negative shapes
rejected at `cobrust check`. F81 extends both to `Ty::List` (element-
addressed — no codepoint concern).

## Fix (ADR-0096)

- **BUG 1**: `__cobrust_list_get` normalizes `idx = if i < 0 { len + i }
  else { i }` then traps `if idx < 0 || idx >= len { crate::panic::panic(
  &format!("list index out of range: i={i} len={len}")) }` — the project
  trap convention (exit 3, clean one-line message, NO path-leaking
  backtrace), NOT a raw `assert!`. The `return 0` sentinel is deleted.
  CRITICAL: this is ALSO the for-loop iteration read path, which is
  strictly in-bounds — the trap never fires there (LC-100 stays green).
- **BUG 2**: `__cobrust_list_slice(list, lo, hi) -> *mut u8` mints a fresh
  `list[i64]` for `[lo, hi)` (the str/bytes-slice mirror; CPython clamp;
  Move-out drops once via the existing `__cobrust_list_drop`). A dedicated
  `IndexKind::Slice` branch in the `Ty::List` lowering arm emits the call;
  a `(Ty::List(elem), IndexKind::Slice {..})` check.rs arm returns
  `Ty::List(elem)` for `lo:hi` + rejects open/step/negative with
  `TypeError::UnsupportedSliceShape` (REUSED — no cascade).

## Verification note

Reproduced independently at the probe tree: `print([10,20,30][-1])`
printed `0` (CPython `30`); `let ys: list[i64] = xs[1:3]` built then
crashed `misaligned pointer dereference`. Post-fix: `xs[-1] == 30`,
`xs[1:3] == [20,30]` (len 2), `xs[100]`/`xs[-100]` trap exit 3 with the
clean message, `xs[1:]`/`xs[0:4:2]`/`xs[1:-1]` reject at build.
Differential e2e: `list_slice_e2e` (6 tests, CPython-3 oracle).

## Out of scope (separate §2.5 gap, NOT a §2.2 bug)

`sorted` / `enumerate` / `zip` are MISSING (`UnknownName`) — a clean
compile error, NOT a silent miscompile / not UB. Tracked as follow-up in
ADR-0096; out of F81 scope. F81 is ONLY the two §2.2 list index/slice
correctness bugs.
