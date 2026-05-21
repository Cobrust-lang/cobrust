---
doc_kind: finding
finding_id: honest-deferred-ignore-audit-2026-05-21
title: "5 honest-deferred #[ignore] audit — 2026-05-21"
status: closed
date: 2026-05-21
last_verified_commit: ba5bfcb
audit_branch: feature/ignore-audit
relates_to: [adr:0052a, adr:0052b, adr:0050c, adr:0062, adr:0058a]
---

# 5 honest-deferred #[ignore] audit — 2026-05-21

## Summary

| # | Test(s) | Disposition | Closing ref |
|---|---|---|---|
| 1 | `borrow_phase_g_e2e` (e0052a_e2e_01..08) | **STALE** — already un-ignored by Cluster A Wave-1 | stale comment removed; 8 tests active |
| 2 | `snap_03_use_after_move_suggestion` | DEFERRED (cited) | `finding:check-exit-code-borrow-gap` |
| 3 | `f3ls22_drop_after_move_use_after_move_rejected` | DEFERRED (cited) | `finding:lc100-str-use-after-move-regression-from-adr0050c` |
| 3b | `f3ls23_partial_iteration_via_early_return_drops_remaining` | DEFERRED (cited) | `finding:lc100-str-use-after-move-regression-from-adr0050c` |
| 3c | `f3ls25_shadowing_rebind_old_list_dropped_before_new_binds` | **CLOSED** — passes at HEAD | `#[ignore]` removed |
| 4 | `s0052b_10_duplicate_field_carries_suggestion` | DEFERRED (cited) | `ADR-0062 §"Cluster B closure"` — record literals Phase G+ |
| 5 | `s0052b_20_use_of_dropped_feature_carries_suggestion` | DEFERRED (cited) | `ADR-0062 §"Cluster B closure"` — Phase J+ FrontendError suggestion |

## Result

- **2 closed**: `borrow_phase_g_e2e` stale comment cleaned; `f3ls25` un-ignored (passes)
- **4 deferred with precise F37 cites**: `snap_03`, `f3ls22`, `f3ls23`, `s0052b_10`, `s0052b_20`

(Note: `borrow_phase_g_e2e` had already been un-ignored by Cluster A prior to this audit;
this audit merely removed the stale module-level comment.)

## Per-test detail

### 1. `borrow_phase_g_e2e` — STALE COMMENT REMOVED

The module-level comment `Pre-DEV-impl status: every e0052a_e2e_* test below is #[ignore]'d
pending Wave-1 DEV merge` was stale. Cluster A (ADR-0052a Wave-1) already removed all
`#[ignore]` markers. All 8 tests pass at HEAD. Stale comment replaced with accurate status.

**Action**: Comment updated, no test changes.

### 2. `snap_03_use_after_move_suggestion` — DEFERRED

**Root cause**: `cobrust check` exits 0 instead of 2 for cross-statement use-after-move.
The intra-block borrow-checker (ADR-0052a Wave-1) does not cover `let zs = xs` then
`print(xs)` in the following statement.

**Finding filed**: `finding:check-exit-code-borrow-gap` (new, 2026-05-21)

**Landing**: Phase H+ borrow-check widening to cross-statement liveness.

**F37 compliance**: `#[ignore]` annotation updated to cite `finding:check-exit-code-borrow-gap`.

### 3. `f3ls22` + `f3ls23` — DEFERRED (f3ls25 CLOSED)

**f3ls22**: Cross-statement use-after-move for `list[str]` passed to a fn.
Same root cause as `snap_03`: intra-block borrow-checker misses call-boundary move.
**Finding**: `finding:lc100-str-use-after-move-regression-from-adr0050c`

**f3ls23**: Partial-iteration drop schedule. `while n > 0:` loop emits
`ImplicitTruthiness { actual: Int }` — the codegen `drop` schedule for
partially-iterated `list[str]` is not yet implemented.
**Finding**: `finding:lc100-str-use-after-move-regression-from-adr0050c`

**f3ls25**: Shadowing rebind drop schedule. **NOW PASSES** at HEAD. The fix
landed as part of the ADR-0050c codegen drop schedule work (exact SHA unknown
but confirmed green at `ba5bfcb`). `#[ignore]` removed.

**F37 compliance**: f3ls22 and f3ls23 annotations updated with `finding:` prefix.

### 4. `s0052b_10_duplicate_field_carries_suggestion` — DEFERRED

`TypeError::DuplicateField` is reserved for record literals (Phase G+). The
dict-literal duplicate-key surface is caught at runtime, not the type checker.
**Cite**: `ADR-0062 §"Cluster B closure"` — precise, no change needed.

### 5. `s0052b_20_use_of_dropped_feature_carries_suggestion` — DEFERRED

The `is` keyword fails at PARSER level (`ParseError::Expected { expected: [Colon],
found: Ident("is") }`). No `suggestion` field on `FrontendError` pre-Phase-J+.
**Cite**: `ADR-0062 §"Cluster B closure"` — precise, no change needed.

## F36/F37/F39 compliance

- **F36**: No fixture renames. All test function names unchanged.
- **F37**: Every `#[ignore]` LEAVE cites a precise `finding:<id>` or `ADR-NNN §section`.
  New finding `check-exit-code-borrow-gap` filed for `snap_03`.
  f3ls22/23 updated to cite `finding:lc100-str-use-after-move-regression-from-adr0050c`.
- **F39**: No device-name leakage. Mac verification results honest-cited below.

## Mac verification

```
cobrust-cli borrow_phase_g_e2e:   8 passed, 0 failed, 0 ignored
cobrust-cli list_str_e2e:        31 passed, 0 failed, 2 ignored (f3ls22, f3ls23)
cobrust-cli error_ux_snapshot:    7 passed, 0 failed, 1 ignored (snap_03)
```

— P7 audit agent, 2026-05-21
