---
doc_kind: adr
adr_id: 0096
title: "`list` index/slice correctness — `xs[-1]` from-end + OOB-TRAP, `xs[lo:hi]` real slice (F81; the str/bytes indexing arc EXTENDED to `Ty::List`)"
status: accepted
date: 2026-06-13
last_verified_commit: HEAD
supersedes: []
superseded_by: []
relates_to: [adr:0093, adr:0094, adr:0095, finding:f78, finding:f79, finding:f81, "claude.md:§2.2", "claude.md:§2.5"]
---

# ADR-0096: `list` index/slice correctness

## Context

A verify-the-gap probe (2026-06-13) found TWO §2.2 bugs in the `list`
index/slice operator surface — the LIST analogue of the just-closed
`str`/`bytes` indexing arc (F78 slice + F79 negative-index/OOB-trap),
tracked as finding **F81**:

1. **`xs[-1]` SILENT MISCOMPILE.** On `[10,20,30]`, `xs[-1]` printed `0`
   (CPython `30`). The runtime `__cobrust_list_get`
   (`crates/cobrust-stdlib/src/collections.rs`) did
   `if i < 0 || i >= layout.len { return 0; }` — BOTH a negative index AND
   a positive OOB returned the silent-`0` sentinel. An in-band wrong value
   §2.2 forbids; `xs[-1]` (last element) is the #1 Python indexing idiom
   (§2.5 maximize-training-data-overlap). This is EXACTLY the F79 bug for
   the str/bytes scalar accessors.

2. **`xs[lo:hi]` UB / MEMORY-SAFETY CRASH.** `let ys: list[i64] = xs[1:3]`
   built OK then CRASHED at runtime (`misaligned pointer dereference`) —
   list slicing was an UNIMPLEMENTED STUB. The generic `lower_index`
   helper (`crates/cobrust-mir/src/lower.rs`) returned
   `IndexKind::Slice { .. } => Ok(Operand::Constant(Constant::Int(0)))` —
   the integer `0` used as a list handle → UB. `check.rs` had a
   `(Ty::List(elem), IndexKind::Expr)` scalar arm but NO
   `(Ty::List, IndexKind::Slice)` arm.

## Decision

Mirror the F79 (ADR-0095) scalar fix and the F78 (ADR-0094) / ADR-0093
slice fix EXACTLY, extending them to `Ty::List`. ELEMENT-addressed (a list
indexes by element, like `bytes` by byte — no codepoint concern).

### BUG 1 — `__cobrust_list_get` from-end + OOB-trap (mirror F79 Option B)

`__cobrust_list_get(list, i) -> i64` now:

- **normalizes** a negative index Python-style: `idx = if i < 0 { len + i }
  else { i }`, so `[10,20,30][-1] == 30` (the last element);
- **TRAPS** a true OOB (`idx < 0 || idx >= len` — BOTH directions,
  INCLUDING the pre-existing positive-OOB hole) via `crate::panic::panic`
  → exit 3 (INTERNAL_PANIC) with a readable
  `list index out of range: i=.. len=..` diagnostic (§2.5-B);
- the silent `return 0` sentinel is **DELETED**.

This uses the **project trap convention** (`crate::panic::panic`, a `-> !`
fn that calls `std::process::exit(3)`), NOT a raw `assert!` — across the
`extern "C"` boundary a raw panic aborts (SIGABRT / exit 134) with a
path-leaking Rust backtrace, the exact drift the F79B audit caught.

`__cobrust_list_get` is ALSO the for-loop iteration + validated-body read
path (MIR emits `__cobrust_list_get(xs, k)` for `k in 0..len`), which
iterates strictly IN-BOUNDS — the trap never fires there (verified: the
`leetcode_corpus_e2e` LC-100 corpus, a heavy list user, stays 12/12
green).

### BUG 2 — `__cobrust_list_slice` real slice (mirror str/bytes slice)

- **runtime** (`collections.rs`):
  `__cobrust_list_slice(list, lo, hi) -> *mut u8` mints a FRESH owned
  `list[i64]` of length `(hi-lo)` and copies the element range `[lo, hi)`
  via `std::ptr::copy_nonoverlapping`, mirroring `__cobrust_str_slice` /
  `__cobrust_bytes_slice`. CPython slice clamp: bounds clamp to `[0, len]`
  and `hi <= lo` yields an empty list (`xs[1:99]` → tail, `xs[3:1]` → `[]`,
  never an exception — the SAME convention `__cobrust_bytes_slice` uses).
  The new list is `.cb`-owned and scope-exit-drops via the EXISTING
  `__cobrust_list_drop` (no new drop symbol — `i64` elements are Copy
  scalars).
- **lowering** (`lower.rs`, the `Ty::List` rvalue arm): a dedicated
  `IndexKind::Slice { start, stop, step }` branch emits a
  `__cobrust_list_slice(list, lo, hi)` call (base BORROWED via Move→Copy
  upgrade, result MOVED out so the single owner drops it once). The
  `Constant::Int(0)` stub is no longer reachable for the List case; an
  unsupported shape hits a defense-in-depth `MirError` (constitution §6),
  NEVER the silent stub.
- **check.rs**: a new `(Ty::List(elem), IndexKind::Slice {..})` arm returns
  `Ty::List(elem)` for the SUPPORTED contiguous `lo:hi` shape (both bounds
  present, non-negative, default step) and REJECTS every other shape
  (open-ended `xs[1:]`/`xs[:3]`, stepped `xs[0:4:2]`, negative-bound
  `xs[1:-1]`) with `TypeError::UnsupportedSliceShape` (§2.5-A) — the EXACT
  str/bytes reject extended to `Ty::List`. `UnsupportedSliceShape` is
  REUSED (payload-free `{ span, suggestion }`), so NO new error variant →
  NO cascade through error_cb/error_ux/lsp/types-parity (byte-parity
  tripwire stays green). The hint names the supported `xs[1:len(xs)]` form.
- **codegen** (`llvm_backend.rs`): `__cobrust_list_slice` declared as
  `(ptr, i64, i64) -> ptr` in `runtime_helper_decls` + param-count 3,
  beside `__cobrust_bytes_slice` / `__cobrust_list_append`.

## Scope

SUPPORTED: `xs[lo:hi]` with both bounds present + non-negative (consistent
with str/bytes). REJECTED at check: open-ended, stepped, negative-bound.

The cross-type indexing arc is now COMPLETE: `str` (codepoint), `bytes`
(byte), and `list` (element) are ALL index/slice-correct with from-end
negative scalar indexing, OOB-traps (exit 3, both directions, no
sentinel), and bounded `lo:hi` slices (open/step/negative rejected at
check).

## Consequences

- `xs[-1]` returns the last element; `xs[1:3]` returns a fresh `[20,30]`;
  `xs[100]` / `xs[-100]` trap exit 3; `xs[1:]` etc. reject at build.
- For-loop iteration + LC-100 unaffected (strictly in-bounds).
- A future `list[str]` / `list[list[T]]` slice would route the element-drop
  fn through `__cobrust_list_drop_elems` (like `split`); F81 scopes
  `list[i64]` (the Copy-scalar element, a plain `__cobrust_list_drop`).

## Follow-up (out of F81 scope)

`sorted` / `enumerate` / `zip` are MISSING (`UnknownName`) — a SEPARATE
§2.5 gap (a clean compile error, NOT a silent miscompile / not UB), tracked
for a future increment. F81 is ONLY the two §2.2 list index/slice
correctness bugs.

## Verification

- `cobrust-stdlib`: `cabi_list_get_negative_index_from_end`,
  `cabi_list_slice_basic_and_clamp`, `cabi_list_slice_null_is_empty`
  (the OOB-trap is `std::process::exit`, not in-process-testable — pinned
  e2e, mirroring str/bytes).
- `list_slice_e2e` (cli, CPython-3 oracle): slice basic+clamp; negative +
  positive scalar; positive-OOB trap (exit 3); negative-OOB trap (exit 3);
  unsupported-shape reject ×4; slice NO-DOUBLE-FREE in a 1000-iter loop
  (asserts no double-free + value correctness — NOT drop-balance; see the
  loop-leak note below).
- `leetcode_corpus_e2e` 12/12 (LC-100 in-bounds iteration unbroken);
  types + types-cb + types-parity green; fmt + clippy clean.

### Known debt — loop-body owned-value LEAK (F82, PRE-EXISTING)

The F81 audit measured (RSS on a compiled `.cb`) a real per-iteration
LEAK: an owned heap value bound to a LOOP-BODY local (a list/str/bytes
slice, OR even a plain `let s = [1,2,3]` list literal) is NOT dropped each
iteration — it accumulates until loop exit (~64 B/iter for a list-slice
loop). This is a SYSTEMIC MIR loop-body drop-scheduling gap that PREDATES
F81 (back to F78 str-slice + the list-literal machinery) — F81 merely
inherits it for one more value type. It is NOT a double-free / not UB (the
loop-06 e2e proves that). Tracked as finding **F82** for a dedicated
lower.rs fix; F81 does NOT claim drop-balance is verified.

### Runtime-negative slice bound (literal-only reject)

The negative-bound reject at `check` is LITERAL-only (`literal_int_value`).
A runtime/non-literal negative bound (`let n = 0 - 1; xs[1:n]`) passes
check and reaches the runtime, where `lo.clamp(0,len)` / `hi.clamp(0,len)`
yields a SAFE empty/clamped list (no UB) — but this is a clamp, NOT CPython
from-end semantics. Consistent with how str/bytes handle it. Full from-end
negative-bound SLICE support is the same follow-up noted under Scope.
