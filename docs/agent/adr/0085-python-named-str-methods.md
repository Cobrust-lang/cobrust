---
doc_kind: adr
adr_id: 0085
title: Python-named str methods (strip / startswith / endswith / lstrip / rstrip / count)
status: accepted
date: 2026-06-05
last_verified_commit: a4b384d
supersedes: []
superseded_by: []
---

# ADR-0085: Python-named str methods

## Context

Cobrust's `str` method surface (shipped by ADR-0050e M-F.3.5 +
ADR-0052d-prereq method-sugar) is partly Rust-named. `split` / `replace`
/ `find` / `lower` / `upper` already use Python spellings, but the
whitespace-strip and prefix/suffix predicates use Rust names: `trim`,
`starts_with`, `ends_with`. There were no `lstrip` / `rstrip` / `count`
at all.

CLAUDE.md §2.5 (the constitutional north star) states Cobrust is "the
language LLM agents write correctly on the first try" and binds two
selection rules: **maximize-overlap-with-training-data** and
**method-call-sugar priority (Direction D)**. Cobrust is also a Python
successor (§2.1). An LLM writing Python — the dominant training-corpus
spelling — reaches for `s.strip()` / `s.startswith()` / `s.endswith()`.
Today those ERROR:

```
method 'strip' not found on 'str'
hint: str methods: len, split, replace, trim, find, contains,
      starts_with, ends_with, lower, upper
```

This is a direct §2.5 deficit: the first-try Python spelling fails to
compile.

CPython 3.11 oracle (verified via `python3.11`):

- `'  hi  '.strip() == 'hi'` — whitespace, BOTH ends. Identical to Rust
  `str::trim`.
- `'  hi  '.lstrip() == 'hi  '` — LEFT only. Rust `str::trim_start`.
- `'  hi  '.rstrip() == '  hi'` — RIGHT only. Rust `str::trim_end`.
- `'hello'.startswith('he') == True` / `'hello'.endswith('xx') == False`.
- `'banana'.count('a') == 3`; `'aaa'.count('aa') == 1` (NON-overlapping,
  not 2); `'abc'.count('') == 4`. Identical to Rust
  `str::matches(sub).count()`.

## Options considered

1. **Replace the Rust names with the Python names (breaking).** Cleanest
   §5.1 "one way" surface, but breaks every existing `.cb` program, the
   LC corpus, and the M-F.3.5 / method-call e2e tests that spell `trim` /
   `starts_with` / `ends_with`. Rejected — violates the
   non-breaking-published-surface bar.

2. **Add the Python names as aliases; keep the Rust names; deprecate the
   Rust names in docs (chosen).** Python names become the canonical
   documented spelling; the Rust names stay accepted (non-breaking) and
   are marked deprecated-aliases. The §5.1 "one way" resolves toward the
   Python name; a future sweep migrates call-sites.

3. **Ship only the three aliases (strip/startswith/endswith), defer all
   new methods.** Smaller, but leaves `lstrip` / `rstrip` / `count` — all
   common Python idioms — still erroring, an incomplete §2.5 close.
   Rejected in favour of shipping the three new shims too.

## Decision

Add six Python-named str methods. Three are pure ALIASES that reuse the
EXISTING runtime symbol (same semantics, NO new shim):

| Python name | routes to | runtime symbol | return |
|---|---|---|---|
| `strip()` | `trim` | `__cobrust_str_trim` | str |
| `startswith(p)` | `starts_with` | `__cobrust_str_starts_with` | bool |
| `endswith(p)` | `ends_with` | `__cobrust_str_ends_with` | bool |

Three are NEW (new PRELUDE fn + new `cobrust-stdlib/src/string.rs` shim):

| name | shim (ABI) | return | semantics |
|---|---|---|---|
| `lstrip()` | `__cobrust_str_lstrip` `(ptr)->ptr` | str | left-only whitespace; Rust `trim_start` |
| `rstrip()` | `__cobrust_str_rstrip` `(ptr)->ptr` | str | right-only; Rust `trim_end` |
| `count(sub)` | `__cobrust_str_count` `(ptr,ptr)->i64` | int | non-overlapping; Rust `matches(sub).count()` |

The Python names are the CANONICAL spelling (docs teach
`strip`/`startswith`/`endswith`). The Rust names (`trim`/`starts_with`/
`ends_with`) are KEPT WORKING (non-breaking) but documented as
deprecated aliases; the §5.1 "one way to do each thing" resolves toward
the Python name, and a future sweep migrates the existing call-sites.

### Lowering path (where the alias routing hooks)

A str-method call `s.M(...)` flows: type-check in
`cobrust-types/src/check.rs` (arity + return-type `match method_name`),
then `cobrust-mir/src/lower.rs::method_form_rewrite_name` maps `(Str,
M)` to a PRELUDE-fn name. The aliases route to the EXISTING PRELUDE name
(`strip -> trim`, `startswith -> starts_with`, `endswith ->
ends_with`), so they inherit the existing intrinsic-rewrite + runtime
symbol with no further wiring. The three new fns add a PRELUDE stub
(`cobrust-frontend/src/prelude.rs`), an intrinsic `Kind` + symbol
routing (`cobrust-cli/src/build/intrinsics.rs`), a codegen extern
(`cobrust-codegen/src/llvm_backend.rs`), and the shim
(`cobrust-stdlib/src/string.rs`).

### Deferred methods

`join` (as a method form), `title`, `capitalize`, `zfill`,
`splitlines`, `isdigit` are a follow-up. `lstrip` / `rstrip` / `count`
ship now.

## Consequences

- **Positive**
  - The first-try Python spelling (`s.strip()` / `s.startswith()` /
    `s.endswith()`) now compiles + runs — a direct §2.5 close.
  - Three common Python idioms (`lstrip` / `rstrip` / `count`) become
    available with CPython-identical semantics.
  - Zero duplicate shims for the three aliases (one symbol per concept).
- **Negative**
  - Temporary two-spellings-per-concept for strip/startswith/endswith
    until the deprecation sweep migrates call-sites. Mitigated: docs name
    the Python spelling canonical and mark the Rust names deprecated.
- **Neutral / unknown**
  - A chars-argument form (`s.strip(chars)`) is still deferred
    (ADR-0050e Q5); the no-arg whitespace form ships here.

## Evidence

- CPython 3.11 differential semantics, verified via `python3.11` at
  authoring time (values quoted in §Context).
- Unit tests: `crates/cobrust-stdlib/src/string.rs` `#[cfg(test)]`
  (`lstrip_left_only`, `rstrip_right_only`, `lstrip_is_not_rstrip` (F36
  anti-swap), `count_non_overlapping`, `count_absent_and_edge`).
- E2E corpus: `crates/cobrust-cli/tests/str_methods_py_e2e.rs` (8 tests
  — strip both-ends, startswith/endswith true+false, lstrip/rstrip
  one-sided with F36 anti-swap sentinels, count non-overlapping, a
  regression family proving the Rust names still compile+run, and a
  `strip == trim` equivalence proving the alias routes to the same
  symbol).
- Regression (the core str surface stays green):
  `crates/cobrust-cli/tests/method_call_e2e.rs` (5/5),
  `crates/cobrust-cli/tests/string_stdlib_e2e.rs` (0 failed),
  `crates/cobrust-cli/tests/list_str_e2e.rs` (31/0),
  `cobrust-types --lib` (144/0), `cobrust-stdlib --lib` (267/0).
