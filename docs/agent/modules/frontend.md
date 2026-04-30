---
doc_kind: module
module_id: mod:frontend
crate: cobrust-frontend
last_verified_commit: TBD
dependencies: []
---

# Module: frontend

## Purpose

Lex, parse, and represent Cobrust source as an AST. Owns syntax — no
semantic analysis here.

## Status

- M0 — empty stub.
- M1 — first delivery: lexer + parser + AST for the "core 30 forms"
  with 24h fuzz coverage.

## Public surface (target — M1)

Final shape decided in M1 ADR. Indicative outline:

```rust
pub fn lex(source: &str, file_id: FileId) -> Result<Vec<Token>, LexError>;
pub fn parse(tokens: &[Token]) -> Result<ast::Module, ParseError>;
pub fn parse_str(source: &str, file_id: FileId) -> Result<ast::Module, FrontendError>;

pub mod ast {
    // Span-bearing AST nodes.
    pub struct Module { /* ... */ }
    pub struct Stmt { /* ... */ }
    pub struct Expr { /* ... */ }
}
```

## Invariants (target — M1)

- **Round-trip property**: `parse(unparse(ast)) == ast` for any AST
  produced by `parse(source)`.
- All errors carry source spans `(file_id, byte_start, byte_end)`.
- No panic paths reachable from any UTF-8 input.
- Lexer is deterministic and stream-friendly (input position monotonic).
- Parser is recursive-descent + Pratt for expressions; no parser generator
  dependency in M1.

## "Core 30 forms"

Cobrust's M1 surface — the 30 syntactic forms the lexer/parser must
round-trip cleanly. The exact list is finalized in the M1 ADR; expected
shape:

- Module / function / class definitions
- `if` / `elif` / `else` / `match` / `for` / `while` / `with` / `try`
- Expressions: literals, calls, attribute, index, comprehensions, lambdas
- Decorators, type annotations, docstrings
- f-strings (full nesting)

## Done means (M1)

- [ ] "Core 30 forms" round-trip suite (curated programs).
- [ ] 24h `cargo fuzz` with no crashes; coverage report committed.
- [ ] Span fidelity test: every AST node points to its source range.
- [ ] Error recovery: parser produces a partial AST + diagnostics on
      single-token edits.

## Non-goals

- No name resolution, type inference, or borrow checking in this crate.
  Those live in `mod:hir` / `mod:types`.
- No incremental reparse in M1 (deferred).

## Cross-references

- Constitution `CLAUDE.md` §7 — milestone definition.
- `mod:hir` — primary downstream consumer.
- `mod:cli` — exposes `cobrust lex` / `cobrust parse`.
