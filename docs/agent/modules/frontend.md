---
doc_kind: module
module_id: mod:frontend
crate: cobrust-frontend
last_verified_commit: 62ef6bd
dependencies: [adr:0003]
---

# Module: frontend

## Purpose

Lex, parse, and represent Cobrust source as an AST. Owns syntax — no
semantic analysis here.

## Status

- **M1 — delivered.** Lexer + parser + AST for the "core 30 forms"
  (see `adr:0003`). Round-trip suite green for every form. Fuzz gate
  satisfied via proptest (`find:m1-fuzz-method`).
- **Follow-ups deferred to later milestones**:
  - Error recovery (partial AST + diagnostics on single-token edits)
    — not gated by M1; tracked for M2/M3.
  - Incremental reparse — tracked for M5+ (IDE story).

## Public surface (M1)

```rust
// Lex / parse entrypoints.
pub fn lex(source: &str, file_id: FileId) -> Result<Vec<Token>, LexError>;
pub fn lex_bytes(bytes: &[u8], file_id: FileId) -> Result<Vec<Token>, LexError>;
pub fn parse(tokens: &[Token]) -> Result<ast::Module, ParseError>;
pub fn parse_str(source: &str, file_id: FileId) -> Result<ast::Module, FrontendError>;
pub fn unparse(module: &ast::Module) -> String;

// Spans.
pub struct FileId(pub u32);
pub struct Span { pub file: FileId, pub start: u32, pub end: u32 }
pub struct Spanned<T> { pub node: T, pub span: Span }

// Tokens.
pub struct Token { pub kind: TokenKind, pub span: Span }
pub enum TokenKind { /* see crate::token for the full enum */ }

// AST root types (see crate::ast for the full enum families).
pub mod ast {
    pub struct Module { pub docstring: Option<String>, pub items: Vec<Stmt>, pub span: Span }
    pub struct Stmt   { pub kind: StmtKind,    pub span: Span }
    pub struct Expr   { pub kind: ExprKind,    pub span: Span }
    pub struct Pattern{ pub kind: PatternKind, pub span: Span }
    pub struct Type   { pub kind: TypeKind,    pub span: Span }
    pub struct Block  { pub stmts: Vec<Stmt>,  pub span: Span }
    pub enum StmtKind  { /* covers ADR-0003 forms 2..19 */ }
    pub enum ExprKind  { /* covers ADR-0003 forms 21..30 */ }
    pub enum PatternKind { /* covers ADR-0003 form 20 */ }
    pub enum TypeKind  { Name, Generic, Union, Fn, Tuple }
}

// Errors.
pub enum LexError      { InvalidUtf8, UnexpectedChar, UnterminatedString, UnterminatedFString, MalformedNumber, InconsistentIndent, InvalidEscape }
pub enum ParseError    { Expected, Syntax, UnexpectedEof, DroppedByConstitution, NonLiteralDefault, IndentError }
pub enum FrontendError { Lex(LexError), Parse(ParseError) }
```

The full enumerations live in `crates/cobrust-frontend/src/`. Stable
re-exports are pinned at the crate root (`lib.rs`).

## Invariants

- **Round-trip property**: `parse(unparse(parse_str(s)?))` equals
  `parse_str(s)?` modulo span normalization, for every program built
  from the 30 forms in `adr:0003`. Verified by
  `tests/round_trip.rs` (one test per form).
- All errors carry source spans `(file_id, byte_start, byte_end)`.
- **No panic** is reachable from any byte input — `lex_bytes` and
  `parse_str` (and the `lex` UTF-8 path) report failures as
  structured errors. Verified by `tests/fuzz_proptest.rs`
  (5 properties × ≥ 4 096 cases default; 100 000 cases under
  `COBRUST_M1_FUZZ_LONG=1`).
- Lexer is deterministic and stream-friendly (input position
  monotonic).
- Parser is recursive-descent + Pratt for expressions; no
  parser-generator dependency.
- Constitution-dropped Python forms (`is`, `del`, `global`,
  `nonlocal`, `async def`, multi-base classes, mutable defaults) are
  rejected with `ParseError::DroppedByConstitution` /
  `NonLiteralDefault`. They never produce a successful AST.

## Preconditions / Postconditions

- `lex(source, file_id)` requires `source` to be valid UTF-8.
  `lex_bytes(bytes, file_id)` accepts arbitrary bytes and surfaces
  `LexError::InvalidUtf8` if not.
- `parse(tokens)` requires `tokens` to end with `TokenKind::Eof`
  (this is what `lex` produces). Mid-stream parser invocation
  on a slice that drops `Eof` is undefined behavior in the moral
  sense (returns `UnexpectedEof`); not gated yet.
- `unparse(module)` is **total**: it never panics on a value-typed
  AST that the parser produced, by construction.

## Done means (M1 — DONE)

- [x] `adr:0003` accepted; the 30-form list is closed.
- [x] Lexer emits `TokenKind::Indent` / `Dedent` / `Newline` / `Eof`
      with byte spans.
- [x] Parser produces a span-bearing `ast::Module`.
- [x] `tests/round_trip.rs` covers all 30 forms (30 tests, all green).
- [x] `tests/fuzz_proptest.rs` panic-free property at ≥ 100 000
      cases per property under `COBRUST_M1_FUZZ_LONG=1`. Method and
      one shrunk panic documented in `find:m1-fuzz-method`.
- [x] Span fidelity: every AST node has a `span` field; the
      round-trip suite normalizes spans before comparing AST shape.
- [x] No panic paths reachable from any byte input.

## Non-goals

- Name resolution, type inference, borrow checking — these live in
  `mod:hir` / `mod:types`.
- Incremental reparse / IDE protocol — deferred to M5+.
- Source-faithful unparser (whitespace / comments preserved) — the
  M1 unparser is canonical, not byte-faithful, by design.
- Error recovery into a partial AST — tracked as a follow-up; the
  M1 parser fails fast.

## ADR-0050a M-F.3.0 — `break` / `continue` (form 16)

| Surface | Anchor |
|---|---|
| Lexer keywords | `KwBreak` + `KwContinue` in `crates/cobrust-frontend/src/token.rs` |
| Reserved word table | `crates/cobrust-frontend/src/lexer.rs` L961, L964 |
| Parser reducer | `crates/cobrust-frontend/src/parser.rs` L205-220 — bumps the keyword token, calls `expect_eos()`, emits `StmtKind::BreakContinue(BreakKind::{Break,Continue})` |
| AST node | `ast::StmtKind::BreakContinue(ast::BreakKind)` + `enum BreakKind { Break, Continue }` (L105-121 of `ast.rs`) |
| Unparser | `crates/cobrust-frontend/src/unparse.rs` L85-93 |
| Test corpus | `crates/cobrust-frontend/tests/break_continue_parse_corpus.rs` — 20 well-typed parse + 5 round-trip + 13 reject; total 38 |

Constraints (ADR-0050a §"Semantics"):
- Bare `break` / `continue` only. No label (`break <ident>` rejected by `expect_eos()` mismatch). No payload (`break 0` / `break "label"` likewise rejected).
- Each keyword stands alone on its own line — the parser does NOT permit `break;` (semicolons unsupported in Cobrust) or `break()` (parses `break` as ident, hits the reserved-word block).

## Cross-references

- `adr:0003` — the 30-form definition this module implements.
- `adr:0050a` — break/continue semantics + contract seal.
- `find:m1-fuzz-method` — fuzz-gate methodology + the one bug it
  caught.
- Constitution `CLAUDE.md` §7 — milestone definition.
- `mod:hir` — primary downstream consumer (M2).
- `mod:cli` — exposes `cobrust lex` / `cobrust parse` (wired in M1
  for downstream tools, exact CLI surface deferred).
