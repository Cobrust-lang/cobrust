---
finding_id: F85
title: '`"a" * "b"` / `"a" - "b"` type-check OK then CRASH the compiler in codegen — Str wrongly in the arithmetic accept-set (§2.5-A + §5.1)
date: 2026-06-13
status: resolved
resolved_date: 2026-06-14
resolved_by: adr:0098
severity: major
discovered_by: the F84 (str*int) §2.2 adversarial audit
relates_to: ["finding:f84", adr:0097, adr:0098, "claude.md:§2.5", "claude.md:§5.1"]
---

# F85 — Str-arithmetic compiler panic (pre-existing)

## What (verified at HEAD bccef60)

`"a" * "b"` and `"a" - "b"` (and likely `"a" / "b"`, `"a" % "b"`) PASS the
type checker, then **PANIC the `cobrust build` compiler** in codegen:

```
$ cobrust build  (source: print("a" * "b"))
thread 'main' panicked at crates/cobrust-codegen/src/llvm_backend.rs:6043:34:
... Found PointerValue ... but expected the IntValue variant   (build exits 101)
```

`"a" - "b"` panics identically at `llvm_backend.rs:6035`. This is NOT a
runtime trap of a built program — it is the COMPILER ITSELF crashing on a
program that the type checker ACCEPTED.

## Root cause

`check.rs`'s post-`unify` arithmetic accept-set (`check.rs:~4473`,
`Ty::Int | Ty::Float | Ty::Str | ...`) includes `Ty::Str` — correct for
`+` (str concat) and now `*`-with-Int (F84 repeat), but it ALSO lets
`Str <op> Str` for `-`/`*`/`/`/`%` through: `unify(Str, Str)` succeeds
(both Str), the accept-set admits `Str`, the expr types as `Str`, and
codegen has no `sub`/`mul`/`div` for two Str `PointerValue`s → raw
`inkwell` panic. A `Str * Float` correctly rejects (unify fails); only the
SAME-type `Str <op> Str` for a non-`+`/non-repeat op slips through.

## Why it matters (§2.5-A + §5.1)

§2.5-A compile-time-catch: an LLM (or human) that writes `"a" * "b"` or
`"a" - "b"` should get a clean `error[Type]` with the fix, NOT a compiler
crash with an internal `inkwell` backtrace (which also leaks internal
paths — the same class as F79B's raw-`assert!` leak). §5.1: "no panic
without rationale" — the compiler must never panic on type-checked input.

## Fix (the queued increment — F85 sprint)

Restrict `Ty::Str` in the arithmetic accept-set to the SUPPORTED string
ops only: `Str + Str` (concat) and `Str * Int` / `Int * Str` (repeat,
already special-cased in `synth_bin` before `unify`). For every OTHER
`Str`-operand arithmetic op (`Str - Str`, `Str / Str`, `Str % Str`,
`Str * Str`), emit a clean compile-time `TypeError` with a §2.5-B
fix-printing message (e.g. "`str` supports `+` (concat) and `* int`
(repeat); `-` / `*` `str` / `/` are not defined — did you mean …?").
Mirror the existing bytes / coil-Buffer §2.5-B operator guards. Add an
ill-typed corpus entry per op + confirm `"a"+"b"` concat and `"a"*3`
repeat stay GREEN. Verify with `cargo test --workspace --locked` (F83
lesson — a check.rs/codegen change).

## NOT introduced by F84

The `Str` accept-set + the panic predate F84 (confirmed: `"a" * "b"`
panics at F84's parent commit too). F84 (str*int repeat) is correct and
additive; it only surfaced F85 and (in attempt-1) mis-described it — the
ADR-0097 + check.rs comment claiming "`Str * Str` rejects" were corrected
to name F85 honestly.

## Resolution (2026-06-14 — ADR-0098)

Fixed exactly as the queued increment prescribed: `check.rs`'s arithmetic
accept-set moved `Ty::Str` off the unconditional accept line into its own
`Add`-only guarded arm (mirroring the adjacent `Ty::Bytes` guard), and a
dedicated `Ty::Str => Err(TypeMismatch { .. })` arm gives a §2.5-B
fix-printing reject for every non-`Add` `str` operand:

> `str` supports `+` (concatenation) and `* int` (repetition, e.g.
> `"ab" * 3`); `-`, `str * str`, `/`, and `%` are not defined on strings

`"a" - "b"` / `"a" * "b"` / `"a" / "b"` / `"a" % "b"` now each reject at
COMPILE time (`cobrust build` exit 2, an `error[Type]`) carrying that
hint — NO compiler panic (exit 101), NO silent exit-0. `"a" + "b"` concat
and `"ab" * 3` repeat stay green; `b"a" - b"b"` was and stays a clean
reject (the `Ty::Bytes` guard already covered it — confirmed not a panic).
No new error variant (reused `TypeMismatch` → no cascade).

Tests: `ill_typed::i08a..i08d` (type-check reject + category) +
`str_mul_e2e::str_mul_e2e_07_str_str_arithmetic_rejects_not_panic` (e2e
exit-2 + hint substring, all four ops). Verified `cargo test --workspace
--locked` green (F83 blast-radius command). Status: open → **resolved**.
