---
finding_id: F82
title: loop-body owned heap value is never dropped per-iteration — a systemic MIR loop-drop leak (str/bytes/list slice + list literal)
date: 2026-06-13
status: open
severity: major
discovered_by: the F81 list-index/slice §2.2 adversarial audit (RSS measurement on compiled .cb)
relates_to: ["finding:f81", adr:0096, adr:0094, "claude.md:§5.3", "claude.md:§2.2"]
---

# F82 — loop-body owned-value drop leak (systemic, pre-existing)

## What (measured at HEAD 2e9edd3)

An owned heap value bound to a **loop-body local** is NOT dropped at the
end of each iteration — it accumulates on the heap until the loop exits
(or the process ends). Measured via `/usr/bin/time -l`
maximum-resident-set-size on compiled `.cb` programs (NOT a Rust-side
test — the real compiled MIR drop schedule):

| loop body (N iterations) | RSS @5M | per-iter |
|---|---|---|
| `let s = xs[1:4]` (list slice) | 323 MB | ~64 B |
| `let s = xs[1:4]` (list slice) @1M | 67 MB | ~64 B |
| `let s = s2[1:4]` (str slice) | 203 MB | ~40 B |
| `let s = b2[1:4]` (bytes slice) | 203 MB | ~40 B |
| `let s = [1,2,3]` (list **literal**) | 323 MB | ~64 B |
| no-owned-value control loop | 2.9 MB (flat) | 0 |
| 3 slices at **fn scope** (no loop) | 2.9 MB (flat) | 0 |

The leak is **linear in iteration count** and **general** — it is not
specific to slices: a plain loop-body list literal leaks identically. It
is **pre-existing**, predating F81 (str-slice landed in `5cb205b`/F78; the
list-literal machinery is older). F81 (list slice) merely added one more
owned value type that hits the same gap.

## Why it matters (§5.3 efficient / §2.2)

A `while`/`for` loop that constructs any owned heap value per iteration —
extremely common (`for line in lines: let parts = line.split(...)`) —
grows memory unbounded. It is NOT a double-free and NOT UB (values are
correct; the loop-06 e2e proves no double-free), so it is invisible to
exit codes and stdout assertions — only an RSS/leaks measurement catches
it. That is exactly why the F81 `..._drop_balance_in_loop` e2e (renamed to
`..._no_double_free_in_loop`) gave false comfort (F36 fixture-name drift).

## Root cause (hypothesis — to confirm in the fix)

The MIR drop scheduling drops an owned local at its **lexical/function**
scope exit, but a loop-body `let` binding's scope is re-entered each
iteration; the drop is scheduled at the loop's *structural* exit (or the
function epilogue) rather than at the **back-edge** (end of each iteration
body). So N iterations allocate N values and drop them once (or never, if
the binding is shadowed each iteration and only the last is tracked). The
fix is in `cobrust-mir/src/lower.rs` loop lowering: insert a
`Terminator::Drop` (or schedule the drop pass to emit one) for each
loop-body-scoped owned local on the iteration back-edge, before the
condition re-check.

## Fix (the queued increment — F82 sprint)

Loop-body owned locals (any non-`is_copy_type` `Ty`) must drop at the END
of each iteration. Verify with an RSS-bounded e2e (a 1M-iteration
owned-value loop must stay flat, e.g. < 10 MB, not grow linearly) across
list-slice / str-slice / bytes-slice / list-literal. This closes the gap
for ALL loop-body owned values at once, not just slices.

## NOT closed by F81

F81 correctly fixed the two list INDEX/SLICE correctness bugs (xs[-1]
silent-0; xs[lo:hi] UB-stub). It does NOT fix this loop-drop leak — it
inherits it, and ADR-0096 + the renamed e2e now say so honestly (no
drop-balance claim). F82 is the dedicated systemic fix.
