---
doc_kind: adr
adr_id: 0105
title: 'conditional expression (ternary) `<then> if <cond> else <else>` — full pipeline'
status: accepted
date: 2026-06-15
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0105: conditional expression (ternary)

## Context

Finding **F93** (§2.5 LLM-first): the Python CONDITIONAL EXPRESSION
(ternary) `<then> if <cond> else <else>` was entirely UNIMPLEMENTED.
`let y = 1 if x < 0 else 2` FAILED at PARSE ("expected end of statement,
found `if`") — there was no AST node (a `grep` for `Ternary`/`IfExpr`
found nothing). It is the single most ubiquitous Python EXPRESSION idiom
an LLM agent writes (`a if c else b` appears constantly: a clamped value,
a default fallback, a sign pick). Its absence was a constant first-try
failure, directly against §2.5's *Maximize-overlap-with-training-data*.

This was an ADDITIVE gap (a clean parse-reject, NOT a silent miscompile),
so the work simply wires the form through the full pipeline.

The load-bearing design problems:

- **Parser disambiguation.** The ternary `if` appears AFTER an expression
  on the same line; the statement-level `if cond:` block STARTS a
  statement. The two must not collide. Python's ternary also binds more
  LOOSELY than every operator (lower than `or`), and the `else` arm is
  RIGHT-associative.
- **§2.2 no implicit truthiness.** `cond` must be `bool`; a non-bool cond
  must REJECT (no truthy coercion).
- **Result type.** Both arms must share a type — a static type system
  cannot give the expression two types.

## Options considered

1. **Parse the ternary as a top-level wrapper around the Pratt
   expression; type as `expect_bool(cond)` + `unify(then, else)`; lower
   in MIR as value-producing control flow reusing the `if`-statement
   block machinery.**
   - Pro: the loose binding falls out naturally — `parse_expr` first
     parses a full Pratt expression (the `then` arm), then, if an `if`
     follows, opens the ternary. The `else` arm recurses through
     `parse_expr`, giving right-associativity for free. Reuses the
     existing `expect_bool` (§2.5-B `ImplicitTruthiness` fix hint) and
     `unify` (canonical `TypeMismatch`) — NO new `TypeError` variant, so
     the error cascade stays closed. MIR reuses the `lower_condition` +
     `SwitchInt` + join-block primitive the `if` statement already uses;
     codegen needs NO new arm (standard blocks + a result local).
   - Con: requires care that comprehension `iter`/guard positions do NOT
     consume the ternary `if` (they parse at the Pratt level, reserving
     the trailing `if`/`else` for the comprehension's own clauses).

2. **Add a dedicated ternary precedence level inside the Pratt loop.**
   - Con: the ternary is not a left/right binary operator (it has TWO
     operators `if`/`else` with a nested expression between them); shoe-
     horning it into `peek_binop` distorts the table and complicates the
     `else`-arm right-associativity. Rejected for a clean top-level
     wrapper.

3. **Keep rejecting (status quo).**
   - Con: permanent first-try failure on the most common Python
     expression form; the §2.5 deficit this finding exists to close.
     Rejected.

## Decision

**Option 1.** A new `ExprKind::IfExpr { cond, then_branch, else_branch }`
threads the full pipeline (AST → HIR → typing → MIR → codegen):

- **Parser** (`parse_expr`): after parsing a full Pratt expression in
  EXPRESSION position, a trailing `if` opens the ternary — consume `if`,
  parse `<cond>` (Pratt), `expect(else)`, then the `<else>` arm via
  `parse_expr` (right-assoc). The statement-level `if cond:` is untouched
  (dispatched at `parse_stmt` before any expression is parsed).
  Comprehension `iter`/guard positions parse at the Pratt level so a bare
  `[x for x in xs if c]` guard is NOT mis-read as a ternary; a
  comprehension ELEMENT (`[a if c else b for ...]`) IS a ternary.
- **Typing** (`synth_expr`): `expect_bool(cond)` (§2.2 — emits the §2.5-B
  `ImplicitTruthiness` fix hint on a non-bool cond) + `unify(then, else)`
  (canonical `TypeMismatch` on a branch mismatch). Result type =
  `then`'s resolved type. **No new `TypeError` variant.**
- **MIR** (`lower_expr`): eval `cond` in the current block → `SwitchInt`
  to then/else blocks, each assigning a fresh `_tern` result local then
  `Goto` a join block. The expression evaluates to the result local. The
  result local's type is `then`'s synth type, so a non-Copy ternary (a
  `str`/`list` ternary) participates in drop dispatch.
- **Codegen**: no new arm — the MIR is standard basic blocks + a result
  local.

**Precedence / associativity (CPython-identical):**

- `a or b if c else d` ⇒ `(a or b) if c else d` (binds looser than `or`).
- `a if p else b if q else c` ⇒ `a if p else (b if q else c)` (the `else`
  arm is right-associative).

## Consequences

- **Positive**
  - The #1 Python expression idiom now compiles first-try (§2.5
    *Maximize-overlap-with-training-data*).
  - Non-bool cond + branch-type mismatch are caught at COMPILE time
    (§2.5-A *compile-time-catch*) with §2.5-B fix-printing diagnostics,
    reusing existing variants (no cascade churn).
  - Works in every expression position: let-rhs, call arg, return value,
    nested chain, comprehension element. `str` / `float` arms work.
- **Negative**
  - A ternary in a comprehension `iter`/guard position must be
    parenthesised (Python's `or_test` grammar — same restriction CPython
    has). Documented.
- **Neutral / unknown**
  - The self-hosting `crates/cobrust-types-cb/src/check.cb` mirror gains
    an Arm-20 `IfExpr` case for parity (not yet compiled; Phase M5+).

## Evidence

- E2E corpus `crates/cobrust-cli/tests/ternary_e2e.rs` (9 tests) — each
  asserts stdout byte-identical to the CPython-3 oracle (let-rhs, call
  arg, return, nested right-assoc, str, float, loose-binding) + two
  clean-exit-2 REJECT tests (non-bool cond; branch type mismatch).
- Parser unit tests in `parser.rs` (`ternary_*`) — AST-shape assertions
  for right-assoc, loose binding, call-arg, the statement-`if`
  regression, and the comprehension guard-vs-element distinction.
- Finding `docs/agent/findings/f93-conditional-expression-ternary.md`.
