---
finding_id: F89
title: '`continue` inside ANY `for` loop HANGS (infinite loop) — the loop-index increment lives in the body fall-through, which `continue` (Goto header) bypasses
date: 2026-06-14
status: resolved
resolved_by: ADR-0100 (2026-06-14)
severity: major
discovered_by: the F88 (`for c in str`) §2.2 adversarial audit
relates_to: ["finding:f88", "claude.md:§2.2", "claude.md:§5.1"]
---

# F89 — `continue` in a `for` loop infinite-loops (pre-existing)

## What (verified at HEAD ea24a63 — F88 branch; reproduces on plain main too)

A `continue` statement inside a `for` loop (list OR str) makes the loop
SPIN FOREVER:

```
for c in "hello":
    if c == "l":
        continue
    print(c)
# CPython: h e o  →  Cobrust: prints h, e then HANGS (exit 142 under an alarm)
```

Confirmed for `list` too: `for x in [1,2,3,4]: if x == 2: continue; print(x)`
prints only `1` then hangs. So it is GENERAL to every `for` loop and
PRE-EXISTING (not introduced by F88; F88 only made `for c in str` reach it
on a common idiom).

## Root cause

The `for` loop's induction-variable increment `__idx += 1` is emitted in
the loop-body FALL-THROUGH (crates/cobrust-mir/src/lower.rs ~:1259-1268,
after `lower_block(body)`). But `StmtKind::Continue` (~lower.rs:528) emits
`Terminator::Goto(header)` — jumping straight to the condition check and
BYPASSING the increment. So after a `continue`, `__idx` never advances,
`__idx < len` stays true, and the loop never terminates.

`while` loops are unaffected because the user hand-writes `i = i + 1` (and
the existing `cli_break_continue_e2e.rs` corpus only tests `continue` in
`while` where the increment is placed BEFORE the `continue` by the author —
so there is ZERO `continue`-in-`for` test coverage anywhere, which is how
this shipped undetected).

## Why it matters (§2.2 + §5.1)

`for x in xs: if cond: continue; ...` is a textbook filter idiom an LLM
writes constantly. It compiles clean then HANGS — a silent infinite loop
(worse than a crash: no diagnostic, no exit). §5.1 + §2.2.

## Fix (the queued increment — F89)

Introduce a per-`for`-loop CONTINUE-TARGET latch block that performs the
`__idx += 1` increment and then `Goto(header)`, and point both the body
fall-through AND `Continue` at that latch (instead of `Continue → header`
directly). I.e. the increment must run on EVERY path that re-enters the
loop — fall-through and `continue` alike. `break` is already correct (it
gotos the exit block). Apply to BOTH the list and str (F88) for-loop
arms. Add a `continue`-in-`for` corpus (list + str): a skip-filter yields
the CPython result and TERMINATES; nested loops; `continue` + `break`
combinations. Run under a watchdog/timeout so a regression to the hang is
caught (a hanging test must FAIL, not stall CI). Verify `cargo test
--workspace --locked`.

## Relation to F88

F88 (`for c in str`) is sound for straight-line + `break` bodies but
inherits this `continue` hang; F88's merge is blocked separately on a
cross-file negative-test regression (for_range_e2e.rs f3r28 asserting
str-for is rejected — must convert to accept, like ill_typed.rs i55).
F88 work is preserved on branch `fix/f88-redo-str-for`; its redo must
(a) convert f3r28, (b) disclose/guard the F89 continue-hang, and
(c) run the FULL `cargo test --workspace --locked` to completion before
commit (the F88 attempt committed while the workspace test was still
compiling — the F83 lesson not fully applied).

## Resolution (ADR-0100, 2026-06-14)

Fixed in `cobrust-mir/src/lower.rs`. The `for` loop now emits an increment
**LATCH** block that performs `__idx += 1` then `Goto(header)`; the
`loop_stack` entry stores `(continue_target, exit)` and `StmtKind::Continue`
gotos the `continue_target`. For a `for` loop that target is the latch; for
a `while` loop it remains the header (unchanged). BOTH the body fall-through
and `continue` route through the latch, so the induction variable advances
on every re-entry path — the hang is gone. `break` (→ exit) and plain
straight-line `for` iteration are unchanged. Nested loops are correct: the
innermost loop's latch is `loop_stack.last()`.

The str-`for` arm (F88 redo, branch `fix/f88-redo-str-for`) inherits the
fix for free — it lowers to the same length-bound index iteration and
reuses the shared continue-target latch.

Regression guard: `crates/cobrust-cli/tests/cli_continue_in_for_e2e.rs`
(15 cases) — a WATCHDOG-guarded corpus that spawns each exe and KILLS +
FAILS it if it does not exit within 10 s, so a future regression to the
hang fails the test instead of stalling CI. Covers skip-filter, first/
last/every-element `continue`, `continue`+`break`, nested inner/outer/both,
`range`-backed lists, and plain-`for`/`break` regression guards. Full
`cargo test --workspace --locked` run to completion before commit.
