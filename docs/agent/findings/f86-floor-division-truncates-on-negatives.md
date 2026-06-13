---
finding_id: F86
title: '`//` integer floor-division TRUNCATES on negatives (should floor toward -∞) — silent miscompile + breaks the div/mod invariant (`%` already floors)
date: 2026-06-14
status: open
severity: major
discovered_by: verify-the-gap idiom probe (2026-06-14, post-F85)
relates_to: ["claude.md:§2.2", "claude.md:§2.1"]
---

# F86 — `//` truncates instead of flooring on negatives

## What (verified at HEAD e0f5e2d vs CPython 3.11)

Cobrust integer `//` TRUNCATES toward zero (C/Rust `/`) instead of
FLOORING toward -∞ (Python `//`):

| expr | Cobrust | CPython | |
|---|---|---|---|
| `-7 // 2` | **-3** | -4 | ✗ |
| `7 // -2` | **-3** | -4 | ✗ |
| `-7 // 3` | **-2** | -3 | ✗ |
| `-7 % 2` | 1 | 1 | ✓ |
| `7 % -2` | -1 | -1 | ✓ |
| `-7 % 3` | 2 | 2 | ✓ |

Positives are fine (`7 // 2 == 3` both). Only NEGATIVE operands (exactly:
when the true quotient is negative and not exact) differ.

## Why it matters (§2.2 + §2.1)

1. **Silent miscompile** (§2.2): `-7 // 2` runs clean and prints a WRONG
   value (`-3` not `-4`). `//` on negatives is common (hashing, grid/index
   math, time arithmetic).
2. **Breaks the div/mod invariant**: `%` ALREADY floors correctly (Python
   semantics — `-7 % 2 == 1`), but `//` truncates, so
   `(a // b) * b + (a % b) != a` for negatives: `(-7 // 2)*2 + (-7 % 2) =
   (-3)*2 + 1 = -5 ≠ -7`. CPython holds the invariant (`(-4)*2 + 1 = -7`).
   The two operators are INCONSISTENT — a correctness bug independent of
   Python compatibility.

## Root cause / fix

The integer `Div` (`//`) lowering/codegen emits a plain truncating
`build_int_signed_div` (LLVM `sdiv`), while `Mod` already emits the
Python-floor `rem` (or an adjusted one). Make `//` FLOOR consistently:
compute the truncated quotient `q = sdiv(a, b)` and a remainder `r`, then
adjust `if (r != 0) && ((a < 0) != (b < 0)) { q -= 1 }` — the standard
floor-division correction. Mirror wherever `Mod`'s floor adjustment lives
(grep the codegen/MIR for the existing `%` floor logic and apply the
symmetric correction to `//`). Division by zero stays a trap (unchanged).

Tests: a div/mod corpus over the sign quadrants (`±a // ±b`, `±a % ±b`)
asserting BOTH the CPython oracle value AND the invariant
`(a // b) * b + (a % b) == a`. Verify `cargo test --workspace --locked`
(F83 blast-radius — this is a codegen/MIR change).
