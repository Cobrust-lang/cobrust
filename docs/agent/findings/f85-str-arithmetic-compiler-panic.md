---
finding_id: F85
title: '`"a" * "b"` / `"a" - "b"` type-check OK then CRASH the compiler in codegen — Str wrongly in the arithmetic accept-set (§2.5-A + §5.1)
date: 2026-06-13
status: open
severity: major
discovered_by: the F84 (str*int) §2.2 adversarial audit
relates_to: ["finding:f84", adr:0097, "claude.md:§2.5", "claude.md:§5.1"]
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
