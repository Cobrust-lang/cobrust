---
doc_kind: finding
finding_id: check-exit-code-borrow-gap
title: "`cobrust check` returns exit 0 instead of 2 for use-after-move at call boundary"
status: accepted_as_honest_debt
date: 2026-05-21
last_verified_commit: ba5bfcb
relates_to: [adr:0052b, test:snap_03_use_after_move_suggestion]
landing_target: Phase H+ (borrow-check widening to cross-statement / cross-call boundary)
---

# check-exit-code-borrow-gap

## Root Cause

`cobrust check` invokes the type-checker + borrow-checker pipeline
but the borrow-checker (as of ADR-0052a Wave-1) is **intra-block only**.
The canonical use-after-move pattern for `snap_03` is:

```cobrust
fn main() -> i64:
    let xs: list[i64] = [1, 2, 3]
    let zs = xs       # move happens here
    let _ = print(xs) # use-after-move — cross-statement
    return 0
```

The use (`print(xs)`) occurs in a subsequent statement after the move
(`let zs = xs`). The intra-block checker does not track cross-statement
liveness; `cobrust check` exits 0 (no error detected) instead of 2
(TYPE_ERROR).

## Gap Surface

| Test | File | Expected | Actual |
|---|---|---|---|
| `snap_03_use_after_move_suggestion` | `crates/cobrust-cli/tests/error_ux_snapshot.rs:157` | exit 2 (TYPE_ERROR) | exit 0 |

## Acceptance Rationale

Pre-existing on main HEAD as of `ba5bfcb`. Not introduced by any recent
branch. The fix requires widening the borrow-checker to cross-statement
liveness tracking — out of scope for ADR-0052a Wave-1 / Phase G.

## Landing Target

Phase H+ borrow-check widening sprint. Tracked via ADR-0052b §3.3
(use-after-move suggestion surface).

## F37 Compliance

Honest-cite per F37. `#[ignore]` annotation in `snap_03_use_after_move_suggestion`
must reference `finding:check-exit-code-borrow-gap`.
