---
doc_kind: adr
adr_id: 0101
title: `for c in <str>:` codepoint iteration
status: accepted
date: 2026-06-14
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0101: `for c in <str>:` codepoint iteration

## Context

`for c in <str>:` (iterate a string codepoint-by-codepoint) was rejected at
type-check with `\`str\` cannot be used in a \`for\` loop` — deferred to
"Phase G" alongside the iter protocol per ADR-0050b §"Iter source type
checking". The reject was clean (exit 2), never a silent miscompile. But the
idiom is among the most common in the Python training corpus, so §2.5
(Maximize-overlap-with-training-data) says the LLM writes it ex ante and the
reject forces a non-preferred rewrite. This ADR lifts the deferral for the
`str` case (the general user-`__iter__` protocol remains Phase G). See
finding `f88-str-for-codepoint-iteration`.

## Options considered

1. **Desugar `for c in s:` to `for i in range(0, len(s)): c = s[i]` at HIR.**
   Reuses the list path verbatim, but `len(s)` returns the BYTE count today
   (a separate divergence) and `s[i]` is codepoint-addressed — the two
   disagree on multi-byte strings, mis-iterating. Rejected: would couple
   F88 to the unrelated `len`-byte-vs-codepoint divergence.
2. **A dedicated MIR STR arm of `LoopKind::For`** bound by a NEW codepoint-
   count primitive `__cobrust_str_char_count`, with the per-iter value from
   the existing codepoint-addressed `__cobrust_str_char_at` (ADR-0094).
   Mirrors the list arm 1:1; the bound and the indexer agree on codepoints.
3. **Full iter-protocol (`__iter__`/`__next__`) for `str`.** Correct
   eventually but Phase-G-sized; over-scoped for one built-in type.

## Decision

Option 2. `iter_element_for(Ty::Str, allow_str = true) -> Ty::Str` (each `c`
is a fresh 1-codepoint owned `str`, CPython semantics), enabled ONLY at the
`for`-loop call site (comprehensions + the `in` operator keep `allow_str =
false`, a clean check-time reject — their MIR paths have no str support). The MIR STR arm bounds the length-bound
index walk by `__cobrust_str_char_count` (codepoint count, NOT byte len) and
writes `__cobrust_str_char_at(__iter, __idx)` directly into the loop var. The
source `str` is BORROWED: a bare-`Name` iter is read as `Operand::Copy` so it
stays usable after the loop; a literal/call-result iter is a fresh temp the
loop owns. The str loop reuses the list arm's F89/ADR-0100 increment latch,
so `continue` advances the codepoint index and terminates for free.

## Consequences

- **Positive**
  - A high-frequency Python idiom now compiles ex ante (§2.5 win).
  - Codepoint-correct: a multi-byte char is ONE iteration (bound and indexer
    both use `chars()`), consistent with the ADR-0094 `s[i]` operator.
  - `continue` over a str loop terminates (inherited F89 latch).
- **Negative**
  - Per-iteration LEAK of the fresh loop-var `str` under the pre-existing
    F82 loop-body-drop gap. F88 guarantees NO double-free (source only read
    via `char_at`; loop var owns its own copy) but does NOT close F82.
  - O(n²) codepoint walk: `__cobrust_str_char_at` re-scans from the start
    each iteration (`chars().nth(i)`). Acceptable for the common short-string
    idiom; a forward-cursor optimization is future work.
  - **Scoped to the `for` loop only.** `iter_element` is shared by the
    comprehension synth + the `in` operator. Their MIR paths
    (`__cobrust_iter_init` / membership) have NO str support, so accepting
    str there would degrade from a clean check-time `NotIterable` reject to
    a codegen-time LLVM-verify / "unimplemented" error (a §2.5 regression).
    The for-loop call site passes `iter_element_for(.., allow_str = true)`;
    the `iter_element` wrapper (comprehension + `in`) keeps `allow_str =
    false`. `[c for c in s]` and `x in s` stay clean check-time rejects.
- **Neutral / unknown**
  - `len(str)` still returns the BYTE count (separate pre-existing
    divergence). The str-for ITERATION count is codepoint-accurate; `len` is
    not. Out of scope here.

## Evidence

- `crates/cobrust-types/src/check.rs` — `iter_element_for(.., allow_str)`;
  the `for`-loop site passes `true`, comprehension + `in` keep `false`.
- `crates/cobrust-mir/src/lower.rs` — `LoopKind::For` STR arm.
- `crates/cobrust-stdlib/src/string.rs` — `__cobrust_str_char_count`.
- `crates/cobrust-codegen/src/llvm_backend.rs` — runtime-helper decl.
- `crates/cobrust-cli/tests/str_for_e2e.rs` — CPython-oracle corpus
  (watchdog-guarded; ASCII, multi-byte one-codepoint, count, empty, usable,
  `continue`, 1000-char no double-free).
- Finding `docs/agent/findings/f88-str-for-codepoint-iteration.md`.
- Prior art: ADR-0094 (F78, codepoint-addressed `char_at`), ADR-0100 (F89,
  for-loop continue increment latch).
