---
finding_id: F95
title: '`sorted(xs)` REJECTS — the ubiquitous ascending-sort idiom is unimplemented (§2.5 training-data-overlap gap)'
date: 2026-06-17
status: resolved
resolved_by: ADR-0108 (2026-06-17)
severity: major
discovered_by: §2.5 LLM-first builtin-coverage audit (2026-06-17, F90/F92/F93/F94 sibling)
relates_to: ["claude.md:§2.5", "claude.md:§2.2", "adr-0090", "adr-0104"]
---

# F95 — `sorted(xs)` builtin unimplemented

## What (verified at HEAD 81f90cc0)

`sorted([3, 1, 2])` REJECTED at build (exit 2, unimplemented / `unknown
name`). The most common Python sort idiom did not compile. Python's
`sorted(xs)` returns a NEW ascending-sorted list and does NOT mutate the
source.

This was an ADDITIVE gap (a CLEAN reject, NOT a silent miscompile): the
program did not compile, so no wrong value was ever produced. The cost was
purely first-try failure.

## Why it matters (§2.5 LLM-first)

`sorted(xs)` is one of the most ubiquitous Python idioms an LLM agent
writes — ranking, top-k, dedupe-then-sort, deterministic iteration over a
set/dict. Its absence is a direct hit to §2.5's *Maximize-overlap-with-
training-data*: the LLM writes `sorted(xs)` from its Python priors and the
build rejects it.

## The load-bearing design problem

Python's `sorted` carries TWO semantic commitments that the
implementation must honour:

| commitment | behaviour |
|---|---|
| **VALUE form** (copy) | `sorted(xs)` returns a NEW list; `xs` is NOT mutated (vs `xs.sort()` in place) |
| **type-ordered** | int/float numeric; str LEXICOGRAPHIC (codepoint order) |

A naive implementation that sorts in place (mutating the source) or that
returns the source unchanged (the PRELUDE stub `return xs`) would BOTH be
silent §2.2 miscompiles. The copy + the per-element-type ordering are the
load-bearing correctness requirements.

## Resolution (ADR-0108)

- **Runtime** (`reduce.rs`): three `extern "C"` shims `(ptr) -> ptr`. Each
  BORROWS the source (reads len + each slot — the ADR-0090 borrow-read
  discipline, never `Box::from_raw`) and builds a FRESH `list[T]`. `_str`
  sorts the slot pointers by `__cobrust_str_cmp` (F92 / ADR-0104, UTF-8
  byte order == codepoint order == CPython), then DEEP-COPIES via
  `__cobrust_str_clone` so the fresh list and the source own DISJOINT Str
  allocations (no double-free). Empty / null source → fresh empty list.
- **Typing** (`try_synth_sorted_builtin`): intercepts the bare PRELUDE
  `sorted` call BEFORE the generic path (whose narrow `list[i64]` stub
  param would reject a `list[str]` arg) and returns `list[T]` of the SAME
  element type. A non-list arg → canonical `NotIterable` (§2.5-B
  suggestion). The `sorted_defs` registration gates on the reducer SHAPE
  (first positional is a `list`) so a user `fn sorted` scalar shadow keeps
  its signature. No new `TypeError` variant.
- **Lowering** (`lower_call` + `Kind::Sorted`): the `callee_return_ty`
  override re-pins `_callret` to `list[T]` from the arg's STATIC element
  type — load-bearing for DROP correctness (a `list[str]` dest routes
  `__cobrust_list_drop_elems` + str_drop over the fresh OWNED clones; a
  `list[i64]` dest would LEAK them). The intrinsic-rewrite reads the same
  dest element type to pick the int/float/str symbol. The source operand is
  Copy-at-call (`is_copy_type(Ty::List)`), so the source drops once.
- **Source UNMUTATED**: the e2e fixtures sort `xs`, then read `xs` in its
  original order — proving the copy semantics.

## Deferred (noted in ADR-0108)

- `sorted(xs, reverse=True)` — descending.
- `sorted(xs, key=f)` — key-function projection.
- `xs.sort()` — in-place mutating method.

## Evidence

- e2e: `crates/cobrust-cli/tests/sorted_e2e.rs` (8 tests, CPython oracle:
  int var + source-unmutated, int literal + duplicates, negatives +
  singleton, empty, float, str-lexicographic + source-unmutated, reducer
  regression + str singleton, non-list reject). Regression:
  `list_reduce_e2e.rs` (14) + `leetcode_corpus_e2e.rs` (12), all green.
- Runtime: `crates/cobrust-stdlib/src/reduce.rs`
  `__cobrust_list_sort_{int,float,str}` + 6 unit tests.
- Typing: `crates/cobrust-types/src/check.rs` `try_synth_sorted_builtin`.
- Lowering: `crates/cobrust-mir/src/lower.rs` `lower_call` + the
  `Kind::Sorted` rewrite in `crates/cobrust-cli/src/build/intrinsics.rs`.
- Codegen: `crates/cobrust-codegen/src/llvm_backend.rs` (three `(ptr) ->
  ptr` extern decls).
- Sibling findings: F90 (`**` power), F92 (`str` ordering), F93 (ternary),
  F94 (`min`/`max` variadic) — all §2.5 additive-gap closures. Builds on
  ADR-0090 (list reducers) + ADR-0104 (str ordering).
