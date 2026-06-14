---
finding_id: F93
title: 'conditional expression (ternary) `<then> if <cond> else <else>` entirely UNIMPLEMENTED — the #1 Python expression idiom FAILS at parse (§2.5 training-data-overlap gap)'
date: 2026-06-15
status: resolved
resolved_by: ADR-0105 (2026-06-15)
severity: major
discovered_by: §2.5 LLM-first expression-coverage audit (2026-06-15, F90/F88/F89 sibling)
relates_to: ["claude.md:§2.5", "claude.md:§2.2", "adr-0105"]
---

# F93 — conditional expression (ternary) unimplemented

## What (verified at HEAD 828f75bd, pre-fix)

`let y = 1 if x < 0 else 2` FAILED at PARSE with "expected end of
statement, found `if`". The Python ternary `<then> if <cond> else <else>`
had NO AST node at all — a `grep` for `Ternary` / `IfExpr` across the
crates found nothing. The parser parsed the `then` expression, then hit a
stray `if` it had no production for, and errored.

This was an ADDITIVE gap (a CLEAN parse-reject, NOT a silent miscompile):
the program did not compile, so no wrong value was ever produced. The cost
was purely first-try failure.

## Why it matters (§2.5 LLM-first)

The ternary is the single most ubiquitous Python EXPRESSION idiom an LLM
agent writes: `a if c else b` appears constantly — a clamped value
(`lo if x < lo else x`), a default fallback (`val if val else default`,
modulo truthiness), a sign pick (`1 if x >= 0 else -1`), a branch inside a
call (`f(a if c else b)`) or `return` (`return a if c else b`). Its
absence is a direct hit to §2.5's *Maximize-overlap-with-training-data*:
the LLM writes the ternary from its Python priors and the build rejects at
parse. A high-frequency form gap is worse than a rare one.

## The load-bearing design problems

1. **Parser disambiguation.** The ternary `if` follows an expression on
   the same line; the statement-level `if cond:` STARTS a statement. The
   two must not collide. Python's ternary also binds more LOOSELY than
   every operator (lower than `or`), and the `else` arm is RIGHT-
   associative — `a if p else b if q else c` ⇒ `a if p else (b if q else
   c)`.
2. **§2.2 no implicit truthiness.** `cond` must be `bool`; `1 if 5 else 2`
   must REJECT (no truthy coercion of the `5`).
3. **Static result type.** Both arms must share a type — `1 if c else "x"`
   (int vs str) must REJECT.

## Resolution (ADR-0105)

A new `ExprKind::IfExpr { cond, then_branch, else_branch }` threads the
full pipeline:

- **Parser** — `parse_expr` parses a full Pratt expression, then a
  trailing `if` opens the ternary (cond Pratt, `expect(else)`, else arm
  via `parse_expr` for right-assoc). The statement-`if` is untouched.
  Comprehension `iter`/guards parse at the Pratt level so `[x for x in xs
  if c]` is not mis-read as a ternary; a comprehension ELEMENT IS a
  ternary.
- **Typing** — `expect_bool(cond)` (§2.5-B `ImplicitTruthiness` fix hint)
  + `unify(then, else)` (canonical `TypeMismatch`). NO new `TypeError`
  variant — the error cascade stays closed.
- **MIR** — value-producing control flow: `cond` SwitchInts to then/else
  blocks, each assigning a fresh result local then `Goto` a join block;
  the expression evaluates to that local. Codegen needs no new arm.

## Verification

- `crates/cobrust-cli/tests/ternary_e2e.rs` (9 tests, CPython-3 oracle):
  let-rhs, call arg, return, nested right-assoc, str, float, loose
  binding, + two clean-exit-2 rejects (non-bool cond; branch mismatch).
- `parser.rs` `ternary_*` unit tests: right-assoc, loose binding, call
  arg, statement-`if` regression, comprehension guard-vs-element.

## Sibling findings

- **F90** (`**` power), **F88** (`for` over str), **F89** (`continue` in
  `for`) — same §2.5 expression/statement-coverage audit family: a
  high-frequency Python form that did not compile first-try.
