---
doc_kind: adr
adr_id: 0098
title: "`str` arithmetic accept-set guard — `str - str` / `str * str` / `str / str` / `str % str` compile-reject, not compiler-panic (F85, §2.5-A / §5.1)"
status: accepted
date: 2026-06-14
last_verified_commit: a7f8ec0
supersedes: []
superseded_by: []
relates_to: [adr:0093, adr:0094, adr:0097, "finding:f85", "claude.md:§2.5", "claude.md:§5.1"]
---

# ADR-0098: `str` arithmetic accept-set guard (F85)

## Context

`check.rs`'s post-`unify` arithmetic accept-set (`synth_bin`,
`BinOp::Add | Sub | Mul | Div | Mod | Pow` arm) carried `Ty::Str` on the
UNCONDITIONAL accept line:

```rust
Ty::Int | Ty::Float | Ty::Str | Ty::IntN(_) | Ty::Var(_) => Ok(resolved),
```

`Ty::Str` is correct for `str + str` (concat), and `str * int` (repeat,
ADR-0097) is special-cased BEFORE `unify` and returns early — so the only
intent of `Str` on this line was the `+` concat path. But the line is
op-agnostic: a SAME-type `Str <op> Str` for ANY op also `unify`s (both
`Str`) and is admitted, typing the expression as `Str`. Codegen has no
`sub` / `mul` / `div` / `rem` for two `str` `PointerValue`s, so
`"a" - "b"` / `"a" * "b"` / `"a" / "b"` / `"a" % "b"` PANICKED the
`cobrust build` COMPILER in `llvm_backend.rs` (raw `inkwell` "expected
IntValue, found PointerValue", exit 101) — a program the type checker had
ACCEPTED. This is finding **F85**.

This violates two constitutional rules:

- **§2.5-A compile-time-catch**: an LLM/human writing `"a" * "b"` MUST get
  a clean `error[Type]` with the fix, not an internal backtrace.
- **§5.1 no-panic-without-rationale**: the compiler MUST NEVER panic on
  type-checked input. (The panic also leaks internal source paths — the
  same class as F79B's raw-`assert!` leak.)

CPython 3: `"a" - "b"`, `"a" * "b"`, `"a" / "b"`, `"a" % "b"` are all
`TypeError`s.

`Ty::Bytes` was ALREADY guarded correctly on the adjacent line
(`Ty::Bytes if matches!(op, BinOp::Add) => Ok(Ty::Bytes)`, ADR-0093) — so
`b"a" - b"b"` already rejected cleanly. The `Str` line was the lone
unguarded survivor.

## Options considered

1. **Move `Ty::Str` into its own `Add`-only guarded arm, mirroring the
   adjacent `Ty::Bytes` guard EXACTLY** (chosen). One-line structural
   change; the unsupported ops fall to a `Str`-aware reject. No new error
   variant, no error-cascade thread, no codegen touch. The `Str * Int`
   repeat is untouched (it returns BEFORE `unify`).
2. **Add a codegen-side reject for two-`Str` arithmetic** (rejected).
   Catches the bug at the WRONG layer — §2.5-A wants the type checker to
   catch it, and a codegen reject still lets a bad program past `check`,
   confusing the LLM's compile-error feedback loop. Defends the panic but
   not the principle.
3. **A dedicated `TypeError::UnsupportedStrArithmetic` variant** (rejected
   for now). A new variant threads the full `error.rs` / `error_cb.rs` /
   `fix_safety` / `error_ux` / `lsp` / `types-cb-parity` + byte-parity
   cascade (F-class cascade cost). The reused `TypeMismatch` with a better
   `suggestion` already prints the fix at the CLI layer — no cascade,
   strictly cheaper, same §2.5-B outcome.

## Decision

Restructure the accept-set match so `Ty::Str` is `Add`-only and a non-`Add`
`Str` operand gets a dedicated §2.5-B fix-printing reject:

```rust
Ty::Bytes if matches!(op, BinOp::Add) => Ok(Ty::Bytes),
Ty::Str   if matches!(op, BinOp::Add) => Ok(resolved),   // concat only
Ty::Int | Ty::Float | Ty::IntN(_) | Ty::Var(_) => Ok(resolved),
Ty::Str => Err(TypeError::TypeMismatch {
    expected: Ty::Str, actual: Ty::Str, span,
    suggestion: Some(
        "`str` supports `+` (concatenation) and `* int` (repetition, \
         e.g. `\"ab\" * 3`); `-`, `str * str`, `/`, and `%` are not \
         defined on strings",
    ),
}),
other => Err(TypeError::TypeMismatch { expected: Ty::Int, .. }),
```

`str * int` / `int * str` repeat is unaffected (handled pre-`unify`,
ADR-0097). `str + str` concat is unaffected (the new `Add`-only `Str` arm).
The four unsupported ops now reject at compile time (exit 2) with the hint;
NO compiler panic.

## Consequences

- `"a" - "b"` / `"a" * "b"` / `"a" / "b"` / `"a" % "b"` → `error[Type]`
  (exit 2) carrying the §2.5-B hint. Verified end-to-end: `str_mul_e2e_07`.
- `"a" + "b"` concat, `"ab" * 3` / `3 * "ab"` repeat, and all numeric
  arithmetic stay GREEN (str_mul_e2e_01–06 + the arithmetic corpus).
- `b"a" - b"b"` was and stays a clean reject (no change — already guarded).
- No new error variant → no cascade, no parity churn. `TypeError`'s
  `Display` omits the `suggestion`; the hint is rendered by the CLI's
  `error_ux` layer (asserted at the e2e layer, not the types-unit layer).
- Closes finding F85 (status open → resolved).

## Evidence

- `crates/cobrust-types/tests/ill_typed.rs::i08a..i08d` — type-check
  rejects (`Cat::TypeMismatch`) for each of `str - str` / `* str` / `/` /
  `%`.
- `crates/cobrust-cli/tests/str_mul_e2e.rs::str_mul_e2e_07_str_str_arithmetic_rejects_not_panic`
  — end-to-end exit-2 (NOT exit-101 panic, NOT exit-0 silent) + the §2.5-B
  hint substring on stderr, for all four ops.
- `cargo test --workspace --locked` green (F83 blast-radius command).
