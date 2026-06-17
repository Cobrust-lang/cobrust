---
doc_kind: adr
adr_id: 0108
title: '`sorted(xs)` builtin — a NEW ascending-sorted list (source NOT mutated), int/float numeric + str LEXICOGRAPHIC'
status: accepted
date: 2026-06-17
last_verified_commit: 81f90cc0
supersedes: []
superseded_by: []
---

# ADR-0108: `sorted(xs)` builtin — value form, copy semantics

## Context

Finding **F95** (§2.5 LLM-first): `sorted([3, 1, 2])` REJECTED at build
(exit 2, `unknown name` / unimplemented). `sorted(xs)` is one of the most
ubiquitous Python idioms an LLM writes — ranking, top-k, dedupe-then-sort,
deterministic iteration — so its absence was a constant first-try failure,
directly against §2.5's *Maximize-overlap-with-training-data*. This was an
ADDITIVE gap (a clean reject, NOT a silent miscompile), so the fix simply
adds the form.

Python's `sorted` has TWO load-bearing semantic commitments:

| commitment | Python behaviour |
|---|---|
| **VALUE form** (copy) | `sorted(xs)` returns a NEW list; the SOURCE `xs` is NOT mutated (distinct from `xs.sort()`, which mutates in place) |
| **ascending, type-ordered** | int/float sort numerically; str sorts LEXICOGRAPHICally (codepoint order) |

`sorted(["b","a","c"]) == ["a","b","c"]`; `sorted([]) == []`. The
`reverse=` / `key=` keyword arguments and the in-place `list.sort()` method
are OUT OF SCOPE (deferred follow-ups).

## Options considered

1. **Add the VALUE form `sorted(list[T]) -> list[T]` (T ∈ {int, float,
   str}); BORROW the source + build a FRESH sorted list; reuse the
   ADR-0090 list-consume mechanism + the F92 str ordering. No `reverse=`/
   `key=`/`list.sort()` yet.**
   - Pro: matches CPython exactly for the most common shape. The source is
     untouched (copy semantics — the surface the LLM expects). REUSES the
     proven ADR-0090 borrow-read discipline (`__cobrust_list_len` /
     `__cobrust_list_get`, never `Box::from_raw`) and the F92 / ADR-0104
     `__cobrust_str_cmp` (UTF-8 byte order == codepoint order == CPython).
     ZERO new `TypeError` variants (non-list arg → canonical `NotIterable`).
     §2.5 *training-data-overlap* closed.
   - Con: `reverse=`/`key=`/`list.sort()` remain unimplemented (still a
     clean reject, not a miscompile). Acceptable — bounded follow-ups.

2. **Implement the in-place `list.sort()` mutating form INSTEAD / FIRST.**
   - Con: `sorted(xs)` (the value form) is the more common LLM idiom and is
     SAFER (no aliasing / mutation surprise). The in-place form needs a
     mutable-receiver method-call path that does not yet exist. Deferred,
     not chosen first.

3. **Keep rejecting `sorted` (status quo).**
   - Con: permanent first-try failure on a ubiquitous idiom; the §2.5
     deficit this finding exists to close. Rejected.

## Decision

**Option 1.** Add the value-form `sorted(xs)`:

| call shape | result | lowering |
|---|---|---|
| `sorted(list[int])` | NEW `list[int]` (numeric ascending) | `__cobrust_list_sort_int` |
| `sorted(list[float])` | NEW `list[float]` (numeric, `f64::total_cmp`) | `__cobrust_list_sort_float` |
| `sorted(list[str])` | NEW `list[str]` (LEXICOGRAPHIC) | `__cobrust_list_sort_str` |
| `sorted([])` | NEW empty `list[int]` (elem var anchors to int) | `__cobrust_list_sort_int` |
| `sorted(5)` (non-list) | COMPILE error (`NotIterable`) | — |

**Runtime (`crates/cobrust-stdlib/src/reduce.rs`).** Three `extern "C"`
shims, each `(*mut u8) -> *mut u8`. Each BORROWS the source (reads len +
each slot via `__cobrust_list_len` / `__cobrust_list_get`, never frees it —
exactly the ADR-0090 borrow-read discipline) and builds a FRESH `list[T]`:

- `_int`: numeric `sort_unstable` of the raw i64 slots.
- `_float`: each i64 slot reinterpreted as the stored `f64` bit-pattern
  (`from_bits`); `sort_by(f64::total_cmp)` keeps the sort total (NaN out of
  scope); the fresh list stores the SAME `to_bits()` patterns.
- `_str`: each slot is a `*mut u8` Str pointer; the slot pointers are sorted
  by `__cobrust_str_cmp` (UTF-8 byte order), then DEEP-COPIED via
  `__cobrust_str_clone` into the fresh list. The fresh `list[str]` OWNS its
  clones; the SOURCE keeps its own slots (NOT consumed). Both lists own
  DISJOINT Str allocations — no double-free.

An empty / null source yields a fresh empty list.

**Typing (`try_synth_sorted_builtin`, `check.rs`).** Intercepts a bare
PRELUDE `sorted` call applied to a single `list[T]` arg and returns
`Ty::List(T)` (the SAME element type — copy of the same shape). MUST run
BEFORE the generic path: the PRELUDE stub declares the narrow
`sorted(xs: list[i64]) -> list[i64]`, so the generic stub-unify would
REJECT `sorted(["b","a"])` (a `list[str]` against the `list[i64]` param).
The `sorted_defs` registration gates on the reducer SHAPE (first positional
is a `list`) — a user `fn sorted(x: i64)` scalar shadow is NOT registered
and keeps its strict signature. A non-list arg → canonical `NotIterable`
(no new variant) with the §2.5-B suggestion `"`sorted` takes a single list
argument"`. An unresolved element var (un-annotated `sorted([])`) anchors
to the int path.

**Lowering (`lower.rs` `lower_call` + `intrinsics.rs` `Kind::Sorted`).**
The PRELUDE stub declares `-> list[i64]`, but `sorted` returns the SAME
element type as its arg. The `callee_return_ty` override re-pins the
`_callret` alloca to `list[T]` derived from the arg's STATIC element type
(`synth_expr_ty`, NOT the arg's MIR temp — the ADR-0089/0090 lesson). This
is load-bearing for DROP correctness: a `list[str]` dest routes
`__cobrust_list_drop_elems` + str_drop over the fresh OWNED clones; a
`list[i64]` dest would LEAK the str clones. The intrinsic-rewrite
`Kind::Sorted` pass reads this SAME dest element type to pick the runtime
symbol (int/float/str — the one source of truth). The source list operand
passes UNCHANGED (Copy-at-call per `is_copy_type(Ty::List)` — the shim
BORROWS the source; the `.cb` scope drops the source once). Codegen
declares the three externs as `(ptr) -> ptr`.

## Consequences

- **Positive**
  - `sorted([3,1,2]) == [1,2,3]`, `sorted([5,5,1,3]) == [1,3,5,5]`,
    `sorted(["banana","apple","cherry"]) == ["apple","banana","cherry"]`,
    `sorted([]) == []` — `sorted` now matches CPython across the int / float
    / str / empty / singleton shapes, with the SOURCE unmutated. §2.5
    *training-data-overlap* deficit closed for the idiom.
  - REUSES the ADR-0090 borrow-read mechanism + the F92 / ADR-0104 str
    ordering. THREE new runtime symbols, ONE new `Kind`, ZERO new
    `TypeError` variants (`NotIterable` + the canonical element-unify
    `TypeMismatch`) — no error cascade.
  - The reducers (`min`/`max`/`sum`/`len`) are untouched; `list_reduce_e2e`
    + `leetcode_corpus_e2e` stay green.
- **Negative**
  - `reverse=` / `key=` kwargs and the in-place `list.sort()` are NOT
    implemented (still a clean reject, not a miscompile). Bounded
    follow-ups.
  - `sorted` of a `list[T]` where `T` is not int/float/str (e.g. a nested
    list) is rejected via the element-unify `TypeMismatch` — Python would
    sort tuples/lists structurally; out of scope.
- **Neutral**
  - The VALUE form (copy) was chosen over the in-place `list.sort()` first
    because it is the more common + safer LLM idiom; `list.sort()` is a
    documented follow-up.

## Deferred

- `sorted(xs, reverse=True)` — descending.
- `sorted(xs, key=f)` — key-function projection.
- `xs.sort()` — in-place mutating method (needs the mutable-receiver
  method-call path).
- `sorted` of structurally-orderable element types (tuples, nested lists).

## Evidence

- Runtime: `crates/cobrust-stdlib/src/reduce.rs`
  `__cobrust_list_sort_{int,float,str}` + 6 unit tests (ascending,
  duplicates/negatives/singleton/empty, float, str-lexicographic +
  source-unmutated, str empty/singleton, null-yields-empty).
- Typing: `crates/cobrust-types/src/check.rs` `try_synth_sorted_builtin`
  (intercept-before-generic; `list[T] -> list[T]`; non-list →
  `NotIterable`) + the `sorted_defs` reducer-SHAPE registration gate in
  `prebind_item`.
- Lowering: `crates/cobrust-mir/src/lower.rs` `lower_call` — the
  `sorted_is_intrinsic` shape gate + the `callee_return_ty` `list[T]`
  override (DROP-schedule correctness).
- Rewrite: `crates/cobrust-cli/src/build/intrinsics.rs` `Kind::Sorted` —
  dest-element-type → `__cobrust_list_sort_{int,float,str}`.
- Codegen: `crates/cobrust-codegen/src/llvm_backend.rs` — the three
  `(ptr) -> ptr` extern declarations.
- e2e oracle corpus: `crates/cobrust-cli/tests/sorted_e2e.rs` (8 tests:
  int var + source-unmutated, int literal + duplicates, negatives +
  singleton, empty, float, str-lexicographic + source-unmutated, reducer
  regression + str singleton, + a clean-exit-2 non-list reject).
- Regression: `crates/cobrust-cli/tests/list_reduce_e2e.rs` (14, green) +
  `crates/cobrust-cli/tests/leetcode_corpus_e2e.rs` (12, green).
- Builds on: **ADR-0090** (list reducers — the borrow-read mechanism),
  **ADR-0104** / **F92** (`str` ordering — the lexicographic `str_cmp`).
- Finding: `docs/agent/findings/f95-sorted-builtin.md` (status → resolved).
