---
doc_kind: adr
adr_id: 0100
title: '`for`-loop `continue` increment latch — `continue` must advance `__idx` (F89 infinite-loop fix)'
status: accepted
date: 2026-06-14
last_verified_commit: 033d21d
supersedes: []
superseded_by: []
---

# ADR-0100: `for`-loop `continue` increment latch

## Context

Finding **F89** (`docs/agent/findings/f89-continue-in-for-loop-hangs.md`)
identified that a `continue` statement inside ANY `for` loop made the loop
SPIN FOREVER — a silent infinite loop (clean compile, no diagnostic, no
exit; worse than a crash):

```
for x in [1, 2, 3, 4]:
    if x == 2:
        continue
    print(x)
# CPython:  1, 3, 4
# pre-F89:  prints 1, then HANGS (exit 142 under an alarm)
```

`for x in xs: if cond: continue; ...` is a textbook filter idiom an LLM
writes constantly (§2.5 maximize-overlap-with-training-data), so this is a
§2.2 + §5.1 severe correctness defect.

### Root cause

The `for` loop lowers (ADR-0050b) to length-bound index iteration:

```
header:  if __idx < __len: goto body  else: goto exit
body:    var = __cobrust_list_get(__iter, __idx)
         [lower body]
         __idx = __idx + 1     ← increment lived ONLY here, in body fall-through
         goto header
exit:    [optional else]
```

The induction-variable increment `__idx += 1` was emitted ONLY in the body
fall-through (`cobrust-mir/src/lower.rs`, after `lower_block(body)`). But
`StmtKind::Continue` emitted `Terminator::Goto(header)` — jumping straight
to the condition check and BYPASSING the increment. So after a `continue`,
`__idx` never advanced, `__idx < __len` stayed true, and the loop never
terminated.

`while` loops were unaffected because the user hand-writes the induction
bump (e.g. `i = i + 1`) inside the body, and the existing
`cli_break_continue_e2e` corpus only exercised `continue` in `while` where
the bump is placed BEFORE the `continue` by the author — so there was ZERO
`continue`-in-`for` coverage, which is how this shipped undetected.

`break` was already correct: it gotos the exit block.

## Decision

Introduce a per-`for`-loop **increment latch** block that is the loop's
`continue` target. The latch performs `__idx += 1` then `Goto(header)`.
BOTH the body fall-through AND `continue` route through the latch, so the
induction variable advances on EVERY path that re-enters the loop:

```
header:  if __idx < __len: goto body  else: goto exit
body:    var = __cobrust_list_get(__iter, __idx)
         [lower body]                 ← `continue` here → goto latch
         goto latch                   ← body fall-through → goto latch
latch:   __idx = __idx + 1            ← runs on EVERY re-entry path
         goto header
exit:    [optional else]
```

### `continue` resolves to the enclosing loop's continue-target

The lowering's `loop_stack` previously stored `(header_block, exit_block)`
and `Continue` gotoed the first field (the header). It now stores
`(continue_target_block, exit_block)`, and `Continue` gotos the
`continue_target`:

- For a **`while`** loop, `continue_target == header` (UNCHANGED behavior —
  the bump is the user's responsibility, as before).
- For a **`for`** loop, `continue_target == latch` (the increment block).

`break` still gotos the second field (`exit_block`), unchanged.

### Nested loops

`loop_stack` is a stack pushed on loop entry and popped on exit, so the
innermost loop's `(continue_target, exit)` is always `last()`. An inner
`continue` targets the inner latch; an outer `continue` targets the outer
latch. Verified by `cli_continue_in_for_e2e::c09_nested_inner_*` /
`c10_nested_outer_*` / `c11_nested_both`.

### Applies to the str `for` arm too

On `main` the `for` loop has only the list arm; the str-`for` arm (F88
redo, branch `fix/f88-redo-str-for`) reuses the SAME `continue`-target
mechanism — it lowers to the same length-bound index iteration, so pointing
its body fall-through and `continue` at the shared latch fixes it for free.

## Consequences

- `continue` inside a `for` loop now SKIPS the element and TERMINATES,
  matching CPython.
- No change to `while`-loop control flow, to `break`, or to straight-line
  `for` iteration. Verified: `cli_break_continue_e2e` (15),
  `break_continue_mir_corpus` (19), `for_range_e2e` (36),
  `leetcode_corpus_e2e` (12) all stay green.
- One extra basic block (the latch) per `for` loop. Negligible; LLVM
  jump-threads the latch→header edge.

### Test: watchdog-guarded corpus (a hang must FAIL, not stall CI)

`crates/cobrust-cli/tests/cli_continue_in_for_e2e.rs` (15 cases) runs every
produced exe through `run_with_timeout`: the exe is spawned and KILLED +
the test FAILED if it does not exit within a 10 s bound. A correct run of
these tiny loops finishes in milliseconds; only a regression to the hang
trips the bound. There is NO unbounded `.wait()` that could stall CI.

Coverage: skip-filter (skip evens → 1,3,5); `continue` on first / last /
every element; `continue` in an `elif`; `continue` + `break` together;
`continue` after a print; nested inner/outer/both `continue`; `continue`
over a `range`-backed list; an accumulator value; plus regression guards
that plain `for` and `break` are unchanged.

## Evidence

- Bug: `cobrust-mir/src/lower.rs` `LoopKind::For` (increment in body
  fall-through) + `StmtKind::Continue` (`Goto(header)`).
- Fix: increment latch block + `loop_stack` continue-target semantics.
- Verification: `cargo test --workspace --locked` (the CI command);
  targeted `cli_continue_in_for_e2e` + the four regression suites above.
