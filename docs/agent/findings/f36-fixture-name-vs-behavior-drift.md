---
doc_kind: finding
finding_id: f36-fixture-name-vs-behavior-drift
title: "F36: fixture name vs behavior drift"
status: ratified_2026-05-19
date: 2026-05-19
last_verified_commit: 1e57b85
discovered_by: P7 retroactive amend sprint (de6c78d) — 0058a Wave-1 4/5 bypass + 0058b §A3 toy-bench overstatement
severity: P2 (false PASS comfort; named code paths untouched by corpus)
related: [finding:f35-sibling-commit-msg-vs-diff-drift]
cross_refs: [upstream ADSD PR #1 F-pattern catalogue; queued as F41 or next-free slot]
sourced_from: machine-local memory port 2026-05-19 (machine-loss-resilient copy)
---

# F36: Fixture Name vs Behavior Drift

## Pattern

A fixture is renamed or rewritten to work within current language surface, but
the fixture NAME still promises the original shape. PASS count gives false
comfort: the gates verified Cranelift/LLVM emit *something* for the rewritten
body, but never verified the *named* MIR shape.

## Incident (0058a Wave-1, commit `a1d615b`)

At the 0058a Wave-1 fixture-fix sprint (post-`4563731`), 4/5 rewritten fixtures
were name-vs-behavior drift:

| Fixture name | Body actually tests |
|---|---|
| `llvm_type_02_i32` | `i64+i64` (i32 absent from Cobrust `Ty`) |
| `llvm_type_03_i8` | `i64` passthrough |
| `llvm_type_06_void_return` | `-> i64: return 0` (not void at all) |
| `llvm_terminator_02_return_void` | `-> i64: return 0` (not void at all) |
| `llvm_type_09_struct_two_i64` | `tuple(a,b)` without field access verify |

Only `llvm_operand_05_ref_local` (rewritten as `take(&x)`) actually exercised
the named concept.

PASS count of 50/8-ignored gives false comfort. The gates verified Cranelift/LLVM
emit *something* for the rewritten body, but the named code paths in
`llvm_backend.rs` (i32 representation, void calling-conv, struct layout) are
UNTOUCHED by the corpus.

## §A3 honest-scope issue (0058b)

The 5-fixture toy bench (hello 872 / fizzbuzz 1408 / fib 1192 / dot_product 1056 /
nested_branch 1200 cycles) does NOT externally validate `-O3 ≥30%` on production
binaries. LLVM compresses tiny binaries asymmetrically well. ADR-0023 §A3
"RESOLVED" marker should be tightened to:

> "TOY-FIXTURE RESOLVED; production-scale workload (50MB+ binary) pending
> Phase K+ realistic bench"

## Rule

When a fixture cannot be expressed in current language surface, RENAME it to
what it actually tests + queue the original promised shape into a language-surface
gap queue. Do NOT silently rewrite the body while keeping the descriptive name.

## Tier-1 audit gate extension

Audit shape MUST include: **fixture name vs fixture behavior consistency check**.

For each rewritten fixture, audit verifies:
- (a) The fixture body actually exercises the named shape, OR
- (b) The fixture was renamed to match the body AND the original shape was queued
  as a language-surface gap.

## Author-side procedure

When rewriting a fixture to match parser/language constraints:
1. Prefer rename over silent body-replace.
2. If the named shape is intrinsically unrepresentable:
   ```rust
   #[ignore = "language gap: <shape>; deferred to <Phase X.y / ADR-XXXX>"]
   ```
   with the gap queued in a tracked list.

## Open language-surface gap queue (post-0058a Wave-1)

These shapes need actual language work, not fixture-rename:

- `i32` / `i8` narrower int types (Cobrust `Ty` currently only has `Int` = i64)
- `None` keyword as return type (parser KwNone rejection in return-type position;
  codegen has `Ty::None` mapped to i64 fallback per ADR-0058a §14.1)
- `[T; N]` fixed-size array TypeKind (AST gap)
- `&T` in type-annotation position (currently `&` only legal in expression
  position via ADR-0052a borrow)
- anonymous struct literal `struct{i64,i64}` (likely won't add; use tuple/record)

## ADSD catalogue status

F36 RATIFIED 2026-05-19 by retroactive amend sprint at `de6c78d`. Empirically
validated by: (a) 0058a 4 bypass fixture rename + §15 gap queue, (b) 0058b §A3
honest-scope tightening, (c) 0058c proactive catch on 2 DWARF inline tests
pre-merge, (d) Tier-1 audit shape extended with F36 gate. Queued for upstream
ADSD catalogue follow-on PR as F41 (or next-free slot — original PR was F31-F40).
