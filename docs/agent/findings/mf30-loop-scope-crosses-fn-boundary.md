---
finding_id: mf30-loop-scope-crosses-fn-boundary
severity: P2
status: closed_by_cef71f3
discovered_by: P9 M-F.3.0 break/continue corpus authorship 2026-05-16
last_verified_commit: cef71f3
relates_to: [adr:0006, adr:0050a]
---

# mf30-loop-scope-crosses-fn-boundary

## Symptom

A nested `fn` definition placed inside a `while`-loop body could legally
contain `break` / `continue` statements. The type checker accepted them
even though the inner `fn`'s body executes in a separate function scope
with no enclosing loop.

```cobrust
fn outer() -> i64:
    let i: i64 = 0
    while i < 3:
        fn inner() -> i64:
            break          # ← should be TypeError::BreakOutsideLoop
            return 0
        i = i + 1
    return 0
```

Pre-fix at HEAD `30cf2b2`: `cobrust check` returned `ok`. The inner
`fn`'s body was lowered + type-checked under the outer's still-elevated
`loop_depth` counter.

## Trigger surface

`crates/cobrust-types/src/check.rs` — `Ctx::check_fn` (L232-269 in
pre-fix). The function pushed `return_stack` but did **not**
save/reset `loop_depth`. Any nested fn defined inside a loop body
inherited the outer body's loop scope.

Same shape applies to a nested class method, but `check_class`
delegates to `check_item` per member which re-enters `check_fn`, so
the same fix-pattern at `check_fn` closes both cases.

## Root cause

ADR-0006 §"Error taxonomy" lists `BreakOutsideLoop` /
`ContinueOutsideLoop` as required gates but did not pin the
fn-boundary discipline. The type checker grew with `return_stack`
discipline (return-outside-fn correctly rejects at any fn boundary)
but `loop_depth` was kept as a flat counter without sym­metric
save/restore at fn-entry.

## Fix

`check_fn` now saves the current `loop_depth`, sets it to 0 for the
duration of the body's `check_block`, and restores on return:

```rust
let saved_loop_depth = std::mem::take(&mut self.loop_depth);
let _ = self.check_block(&f.body)?;
self.loop_depth = saved_loop_depth;
```

Mirror of the `return_stack.push() / pop()` discipline directly above.

## Verification

`crates/cobrust-types/tests/break_continue_types_corpus.rs` tests
b13 + b14 regression-pin both branches:

- b13 — `break` in nested fn inside outer's while → must reject as
  `TypeError::BreakOutsideLoop`.
- b14 — same shape with `continue` → must reject as
  `TypeError::ContinueOutsideLoop`.

Pre-fix at HEAD `30cf2b2`: both tests panicked with "must reject but
type check passed". Post-fix at HEAD `cef71f3`: both green.

## Impact

P2 because:
- No silent runtime miscompile is reachable — MIR's `lower.rs` L424
  emits `MirError::Internal("break outside loop")` on the no-loop_stack
  fallback, so the case would have failed at MIR layer with an opaque
  internal error.
- Pre-fix path was an inadvertent type-checker false-accept that masked
  the MIR-layer hard error.
- No known production source triggered the bug (closure-fns are rare in
  current Cobrust corpus).

## Cross-references

- ADR-0050a §"Scope discipline" — binding spec.
- ADR-0006 §"Error taxonomy" — error variant list.
- `crates/cobrust-types/src/check.rs` L264-279 — post-fix
  save/reset/restore site.
- `crates/cobrust-mir/src/lower.rs` L424 — defensive MIR fallback
  that would have caught it at a later layer.
