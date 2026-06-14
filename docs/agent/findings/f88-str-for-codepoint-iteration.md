---
doc_kind: finding
finding_id: f88-str-for-codepoint-iteration
last_verified_commit: TBD
discovered_by: §2.5 LLM-first idiom-overlap review
severity: P2
related: f78-str-slice-silent-miscompile (ADR-0094, codepoint-addressed char_at), f79-scalar-negative-index-oob-trap, f82-loop-body-owned-value-drop-leak (per-iter leak debt), f89-continue-in-for-loop-hangs (ADR-0100 increment latch inherited)
status: closed_by_F88
---

# Finding: `for c in <str>:` was rejected — a missing common idiom (§2.5)

## Hypothesis

`for c in "hi":` — iterate a string codepoint-by-codepoint, one of the most
common Python idioms — was REJECTED at type-check with
`error[Type]: \`str\` cannot be used in a \`for\` loop`. This is NOT a silent
miscompile (clean reject, exit 2), so it is a P2 additive gap, not a P0
correctness bug. Per §2.5 (Maximize-overlap-with-training-data) the idiom is
frequent enough in the Python training corpus that the LLM writes it ex ante;
the reject forced a rewrite to an index loop the LLM does NOT prefer.

## Method

`iter_element` in `crates/cobrust-types/src/check.rs` had arms for
`List`/`Set`/`Dict`/`Tuple`/`Var` but no `Ty::Str` arm — it fell through to
the `NotIterable` catch-all. The MIR `LoopKind::For` lowering
(`crates/cobrust-mir/src/lower.rs`) only knew the list layout
(`__cobrust_list_len` + `__cobrust_list_get`).

## Result (resolution — F88 / ADR-0101)

`str` is now an iterable whose loop variable binds a fresh 1-codepoint owned
`str` (CPython semantics).

- **types/check.rs**: `iter_element_for(Ty::Str, allow_str = true) ->
  Ty::Str` at the `for`-loop call site ONLY. The shared `iter_element`
  wrapper (used by comprehension synth + the `in` operator) keeps
  `allow_str = false` so `[c for c in s]` / `x in s` stay clean check-time
  `NotIterable` rejects — their MIR paths (`__cobrust_iter_init` /
  membership) have NO str support, and accepting str there would degrade to
  a codegen-time LLVM-verify / "unimplemented" error (a §2.5 regression
  from the prior clean reject). Caught during F88 verify; cross-path lesson.
- **mir/lower.rs**: STR arm of `LoopKind::For`. Loop bound is the CODEPOINT
  count `__cobrust_str_char_count` (NOT byte len — a multi-byte char is ONE
  iteration). Per-iteration value is `__cobrust_str_char_at(__iter, __idx)`
  (codepoint-addressed, F79/ADR-0094) written directly into the loop var
  (a fresh OWNED handle — no `list_get → str_clone` two-step). The source
  `str` is BORROWED: a bare-`Name` iter is read as `Operand::Copy` (not the
  default `Move`) so `for c in s:` leaves `s` usable after the loop; a
  literal/call-result iter is a fresh temp the loop owns.
- **stdlib/string.rs**: `__cobrust_str_char_count` (`chars().count()`),
  declared in the llvm_backend runtime-helper table.

## Codepoint vs byte (load-bearing)

The loop bound MUST be the codepoint count, not the byte length: `"héllo"`
is 6 UTF-8 bytes but 5 codepoints, so it yields 5 iterations with `c == "é"`
on iteration 1 (not two half-`é` iterations). `__cobrust_str_char_count` and
`__cobrust_str_char_at` both use `chars()`, so they agree codepoint-for-
codepoint with no mid-codepoint split.

## Drop / memory caveat (F82 boundary)

Each loop var is a FRESH owned 1-codepoint `str`. There is NO double-free:
the source `__iter` is only READ via `char_at` (never consumed), and the
loop var owns its own fresh copy. A per-iteration LEAK exists under the
PRE-EXISTING F82 loop-body-drop gap (an owned value minted inside a loop
body is not dropped each iteration); F88 does NOT close F82 and does NOT
claim drop-balance under the loop — only no-double-free + clean exit. F82 is
the separate systemic fix.

## Non-F88 caveat

`len(str)` still returns the BYTE count (a separate pre-existing divergence
from CPython, which returns the codepoint count). The str-for ITERATION
count is codepoint-accurate; `len` is not. Out of F88 scope.

## Regression guard (cross-file, F80/F83 lesson)

Accepting str-iter flips negative tests in OTHER files green→red. Converted:
- `crates/cobrust-types/tests/ill_typed.rs::i55_*` and `i102_*` (were
  `must_reject(NotIterable)` → now assert the program type-checks).
- `crates/cobrust-cli/tests/for_range_e2e.rs::f3r28_*` (was exit-2 reject →
  now `assert_build_run` "h\ne\nl\nl\no\n").

And PINNED the still-rejected cross-paths (the `allow_str` scoping):
- `ill_typed.rs::i102b_str_comprehension_still_rejected_f88` +
  `i102c_str_in_operator_still_rejected_f88` (str in a comprehension / `in`
  operator stays a clean `NotIterable` check-reject).

New corpus: `crates/cobrust-cli/tests/str_for_e2e.rs` (watchdog-guarded;
ASCII, multi-byte `é`/CJK one-codepoint, iteration-count == codepoint count,
empty string 0-iter, loop-var usable, `continue` terminates, 1000-char no
double-free).
