---
doc_kind: adr
adr_id: 0003
title: Cobrust core 30 syntactic forms (M1 frontend scope)
status: accepted
date: 2026-04-30
last_verified_commit: 62ef6bd
supersedes: []
superseded_by: []
dependencies: [adr:0001, adr:0002]
---

# ADR-0003: Cobrust core 30 syntactic forms (M1 frontend scope)

## Context

The constitution (`CLAUDE.md` §7) defines M1 as "Lexer + Parser + AST for
Cobrust core syntax. Done means: round-trips the spec's 'core 30 forms';
fuzz-tested 24h." `mod:frontend` (`docs/agent/modules/frontend.md`) defers
the exact list to "the M1 ADR." This is that ADR.

The list must:

- Be **finite, enumerable, and testable** — round-trip property
  (`parse(unparse(ast)) == ast`) requires a closed grammar.
- Be **minimal but complete** — together the 30 forms must let an
  engineer write the smoke-test programs of M1: `hello world`,
  `fibonacci`, a `match` over a `Result`, a class with a decorator,
  a `with` block, a comprehension. If a smoke test cannot be expressed
  via the 30 forms, the list is wrong.
- **Honor `CLAUDE.md` §2** — keep what Python keeps, drop what Python
  drops. Forms that are dropped (`is`, implicit truthy, multiple
  inheritance, metaclass, async-coloring) are *not* in the list and
  must not parse.
- Be **stable surface for `mod:hir`** (M2). Each form lowers cleanly
  to one or more HIR nodes; we do not pick syntax we cannot lower.
- Treat orthogonal concerns as **one form** (e.g. `if/elif/else` is
  one form `if_stmt`, not three) so that the count is meaningful.

## Options considered

1. **No fixed list — "whatever Python 3.12 has"** — the M1 fuzz gate
   would have no oracle of completeness, and dropped Python features
   (`is`, metaclasses) would leak in by default. Rejected.

2. **One form per AST enum variant** — variant counts vary as we
   refactor; the round-trip suite would be unstable. Rejected.

3. **30 syntactic forms, grouped by category, each with a grammar
   sketch and a smoke-test snippet** *(chosen)* — gives the parser a
   bounded surface, gives the fuzz suite a checklist, gives the AST
   designers a concrete invariant to satisfy.

4. **A larger taxonomy (50+) splitting `attribute` vs `index`,
   `binary` vs `unary`, etc.** — the constitution's headline is
   "core 30 forms"; doubling the count to ~60 dilutes M1 and pushes
   work that belongs in M2 / M3 into the frontend. Rejected.

## Decision

Adopt **the 30 forms below** as Cobrust's core syntactic surface for
M1. The lexer accepts the union of token classes implied by these
forms; the parser produces an AST whose nodes round-trip every program
using only these forms; the fuzz harness covers each form with a
seed corpus.

Forms are grouped into six categories. Each row is one form. Subkinds
listed inside a single form are intentionally collapsed — the AST may
distinguish them via a discriminator field, but the round-trip suite
treats them as one form.

### Forms 1–6 — Module & definitions

| # | id | Grammar sketch | Smoke snippet |
|---|---|---|---|
| 1 | `module` | `module := docstring? (stmt NEWLINE)*` | `"""hello world"""` |
| 2 | `import_stmt` | `'import' dotted_name ('as' NAME)?` \| `'from' dotted_name 'import' import_targets` (no `*`) | `from collections import deque as Q` |
| 3 | `fn_def` | `'fn' NAME '(' params ')' ('->' type)? ':' block`; default values restricted to literal expressions | `fn add(x: i64, y: i64 = 0) -> i64: return x + y` |
| 4 | `class_def` | `'class' NAME ('(' base ')')? (':' trait_list)? ':' block`; single base + trait list, no MRO | `class Point(Shape: Drawable): pass` |
| 5 | `decorator` | `('@' expr NEWLINE)+` preceding `fn_def` or `class_def` | `@cached @inline fn pi() -> f64: ...` |
| 6 | `type_alias` | `'type' NAME ('[' type_params ']')? '=' type_expr` | `type Result[T] = Ok[T] \| Err[str]` |

### Forms 7–19 — Statements

| # | id | Grammar sketch | Smoke snippet |
|---|---|---|---|
| 7  | `let_stmt` | `'let' NAME (':' type)? '=' expr` (immutable binding) | `let pi: f64 = 3.14` |
| 8  | `assign_stmt` | `target ('=' \| augop) expr` (`augop` = `+= -= *= /= //= %= **= &= \|= ^= <<= >>=`) | `count += 1` |
| 9  | `if_stmt` | `'if' expr ':' block ('elif' expr ':' block)* ('else' ':' block)?` | `if x > 0: y = 1 elif x == 0: y = 0 else: y = -1` |
| 10 | `while_stmt` | `'while' expr ':' block ('else' ':' block)?` | `while q.is_empty().not(): step()` |
| 11 | `for_stmt` | `'for' target 'in' expr ':' block ('else' ':' block)?` (iter protocol) | `for (k, v) in items: print(k)` |
| 12 | `match_stmt` | `'match' expr ':' (case_clause)+`; see form 20 for patterns | `match r: case Ok(v): ...` |
| 13 | `with_stmt` | `'with' (expr ('as' target)?)** ',' ':' block`; multi-binding | `with open(p) as f, lock(m): use(f)` |
| 14 | `try_stmt` | `'try' ':' block ('except' type ('as' NAME)? ':' block)+ ('else' ':' block)? ('finally' ':' block)?` (reserved for unrecoverable) | `try: parse() except IoError as e: log(e)` |
| 15 | `return_stmt` | `'return' expr?` | `return Ok(value)` |
| 16 | `break_continue_stmt` | `'break' \| 'continue'` (single form, two keywords) | `break` |
| 17 | `raise_stmt` | `'raise' expr ('from' expr)?` | `raise IoError("bad path") from e` |
| 18 | `pass_stmt` | `'pass'` | `pass` |
| 19 | `expr_stmt` | bare expression on its own line; also carries module/fn/class docstrings when the expression is a string literal at the head of the block | `compute(42)` |

### Form 20 — Pattern sub-grammar (used inside `match_stmt`)

| # | id | Grammar sketch | Smoke snippet |
|---|---|---|---|
| 20 | `pattern` | `pattern := literal \| name_binding \| wildcard \| seq_pattern \| mapping_pattern \| class_pattern \| or_pattern ('\|' pattern)+ \| guarded_pattern ('if' expr)?`; the AST exposes one `Pattern` enum, but the round-trip suite covers each subkind | `case Point(x, y) if x == y: ...` |

### Forms 21–30 — Expressions

| # | id | Grammar sketch | Smoke snippet |
|---|---|---|---|
| 21 | `literal_expr` | int (incl. `0x` `0o` `0b` underscores), float (incl. exponent), bool (`True`/`False`), `None`, string, bytes, imaginary | `0xFF_FF`, `1.5e-3j`, `True`, `b"\x00"` |
| 22 | `fstring_expr` | `f"..."` with `{expr (= )? (':' format_spec)?}` interpolation; **arbitrary nesting** of f-strings inside f-strings | `f"x={x:.2f}, nested={f'{y!r}'}"` |
| 23 | `name_expr` | identifier reference (Unicode XID, NFKC-normalized) | `count` |
| 24 | `collection_expr` | tuple `(a, b)`, list `[a, b]`, set `{a, b}`, dict `{k: v, **rest}`; subkind discriminator on AST node | `[1, 2, 3]`, `{1, 2}`, `{"k": v}` |
| 25 | `comprehension_expr` | list / set / dict / generator comprehension; supports `for ... if ...` chains and async-free iteration | `[x*x for x in xs if x > 0]` |
| 26 | `lambda_expr` | `'lambda' params ':' expr`; expression body only, no block | `lambda x: x + 1` |
| 27 | `call_expr` | `expr '(' args ')'` with positional, keyword, `*args`, `**kwargs`; partial application reserved (NYI in M1) | `f(1, 2, key="v", *xs, **kw)` |
| 28 | `access_expr` | attribute `a.b` and indexing `a[b]` (incl. slicing `a[i:j:k]`); subkind discriminator on AST node | `obj.field`, `arr[1:10:2]` |
| 29 | `binary_unary_expr` | full Pratt operator table (arith / bitwise / shift / cmp / `and` / `or` / `not`; **no `is`**); unary `+`, `-`, `~`, `not` | `not (a and b) \| (c << 2)` |
| 30 | `await_yield_expr` | `'await' expr`, `'yield' expr?`, `'yield' 'from' expr`; **single structured-concurrency runtime** — there is no separate `async fn` form, await is allowed in any fn, no two-color problem | `let v = await fetch(u)` |

### Excluded from the 30 (and from the lexer's keyword set)

These Python forms are **dropped** per `CLAUDE.md` §2.2 and must not
parse:

- `is` / `is not` — removed; identity goes through `same_object(a, b)`
- `global` / `nonlocal` — closure capture is explicit (`copy` / `ref` /
  `move` capture), so neither keyword exists
- `async def` / `async for` / `async with` — single runtime; `await`
  works in any fn, no async-coloring
- `del` — bindings are immutable by default; `del` semantics replaced
  by ownership transfer
- multiple-inheritance `class C(A, B):` — single base + trait list only
- `metaclass=` keyword argument on `class_def` — replaced by
  compile-time macros (deferred to a later ADR)
- mutable default arguments — parser accepts them syntactically but
  the type-checker (M2) rejects; M1 semantic behavior is "literal-only
  defaults," enforced by parser

### Lexer scope (token classes implied by the 30 forms)

- Keywords: `and`, `as`, `await`, `break`, `case`, `class`,
  `continue`, `elif`, `else`, `except`, `finally`, `fn`, `for`,
  `from`, `if`, `import`, `in`, `lambda`, `let`, `match`, `not`,
  `or`, `pass`, `raise`, `return`, `try`, `type`, `while`, `with`,
  `yield`
- Soft keywords: `_` (wildcard pattern only)
- Literals: int / float / string / bytes / fstring / bool / none /
  imaginary
- Operators: full Pratt table of `binary_unary_expr` (form 29) +
  augmented assignment operators (form 8)
- Punctuation: `( ) [ ] { } , : ; . -> @ = | & ^`
- Layout tokens: `NEWLINE`, `INDENT`, `DEDENT`, `EOF`; comments
  (`# ...`) and blank lines are skipped before layout tokens are
  emitted
- All tokens carry a span `(file_id, byte_start, byte_end)`

### Round-trip suite

The integration test `crates/cobrust-frontend/tests/round_trip.rs`
contains exactly 30 curated programs, one per form. Each program:

- Exercises the form unambiguously (small enough to be obvious).
- Includes a non-trivial use (not just a bare keyword).
- Compiles to an AST that, when unparsed and re-parsed, equals the
  original AST modulo span normalization.

Spans are normalized in equality checks: the round-trip property is
on AST shape, not byte-exact source layout. Whitespace and comments
are not preserved by the AST and are not part of the round-trip
contract for M1.

### Fuzz seed corpus

The `fuzz/cobrust_frontend/corpus/` seeds derive from the same 30
snippets plus targeted edge inputs (deeply nested f-strings, long
indent chains, malformed UTF-8 sequences). The fuzz target asserts
**no panic on any UTF-8 input** and **no panic on any byte input**
(the lexer must reject non-UTF-8 cleanly with an error, not panic).

## Consequences

- **Positive**
  - Closed surface for M1: the parser is testable to completion, the
    fuzz harness has a meaningful coverage target, the AST shape is
    bounded.
  - Stable input contract for `mod:hir` (M2) and `mod:types` (M2):
    they only need to lower / check the 30 forms, not "all of Python."
  - Explicitly drops Python's bad parts (`is`, async-coloring, etc.)
    by construction — they cannot leak in via the parser.
  - Each form has a smoke snippet, so the round-trip integration
    test is mechanically derivable from this ADR.

- **Negative**
  - 30 is a count we have to defend at every refactor. If a future
    feature genuinely needs a 31st form, that is itself a constitution
    change and requires an ADR superseding this one.
  - Some Python users will miss `is` / `global` / `del` / `async def`.
    The constitution explicitly classifies these as Python defects;
    Cobrust does not chase parity for parity's sake.
  - Pattern sub-grammar (form 20) is a single AST family with five
    subkinds. The round-trip suite covers each subkind, but the
    "30 forms" headline understates the parser's effective surface
    by ~5. This is a deliberate accounting choice consistent with
    the constitution's wording.

- **Neutral / unknown**
  - Soft keywords (e.g. `match`, `case`) require contextual lexing.
    The lexer specification handles this via a "context-sensitive
    keyword" pass; if that pass proves brittle, a follow-up ADR
    will document an alternative.
  - Partial application syntax (`f(1, _, 3)`) is reserved but not
    accepted by M1. If user feedback shows demand, a separate ADR
    will introduce it as a 31st form (and M1's count becomes
    grandfathered as the "M1 surface").
  - F-string format-spec evaluation rules (CPython's PEP 701) are
    deeply intricate; M1 supports the syntax (nesting + `=` debug
    + format spec) but evaluates format specs at runtime in M3+.
    The frontend's job is to parse them, not to evaluate them.

## Evidence

- Constitution `CLAUDE.md` §2.1 (keep), §2.2 (drop), §7 (M1 done means).
- `docs/agent/modules/frontend.md` — defers exact list to this ADR.
- `docs/human/zh/architecture.md` and `docs/human/en/architecture.md` —
  pipeline diagrams place lexer/parser/AST as the entry path.
- ADR-0001 — license under which the implementation ships.
- ADR-0002 — multi-agent topology that delivers M1 via P9 + P8.
- PEP 622 (structural pattern matching), PEP 701 (f-strings),
  Python language reference §6 (expressions) — used as form-count
  cross-checks. Cobrust intentionally diverges from CPython where
  §2.2 of the constitution mandates.
