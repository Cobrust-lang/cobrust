---
doc_kind: module
module_id: mod:hir
crate: cobrust-hir
last_verified_commit: TBD
dependencies: [mod:frontend, adr:0005]
---

# Module: hir

## Purpose

High-level intermediate representation: the AST after desugaring +
name resolution. The form the type checker (`mod:types`) consumes.

## Status

- **M2 — delivered.** Lowering covers every form in `adr:0003` and
  follows the desugaring tables pinned by `adr:0005`. 34 golden
  lowering tests green.

## Public surface (M2)

```rust
// Entrypoint.
pub fn lower(module: &ast::Module, sess: &mut Session) -> Result<Module, LoweringError>;

// Session: owns the DefId counter; threaded through downstream
// phases (mod:types, mod:mir).
pub struct Session { pub defs: DefAllocator }

// Top-level HIR root.
pub struct Module { pub docstring: Option<String>, pub items: Vec<Item>, pub span: Span }

// Item, Stmt, Expr, Pattern, Type, ... (see crate::tree)
pub struct Item { pub kind: ItemKind, pub span: Span }
pub enum  ItemKind {
    Import { def_id: DefId, path: Vec<String>, local_name: String, from_name: Option<String> },
    Fn(FnBody),
    Class(ClassBody),
    TypeAlias(TypeAliasBody),
    Decorated { decorators: Vec<Expr>, inner: Box<Item> },
    Let(LetBody),
    ExprStmt(Expr),
}

pub struct FnBody {
    pub def_id: DefId,
    pub name: String,
    pub params: Params,
    pub return_type: Option<Type>,
    pub body: Block,
    pub captures: Vec<CaptureSpec>,
    pub span: Span,
}

pub struct Stmt { pub kind: StmtKind, pub span: Span }
pub enum  StmtKind {
    Let(LetBody),
    Assign { target: Box<Expr>, value: Expr },
    If { arms: Vec<(Expr, Block)>, else_block: Option<Block> },
    Loop(LoopKind),
    Match { scrutinee: Expr, arms: Vec<MatchArm> },
    With { item: WithItem, body: Block },
    Try {
        body: Block,
        handlers: Vec<ExceptHandler>,
        else_block: Option<Block>,
        finally_block: Option<Block>,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    Raise { exc: Option<Expr>, cause: Option<Expr> },
    Pass,
    Expr(Expr),
    Item(Item),
}

pub struct Expr { pub kind: ExprKind, pub span: Span }
pub enum  ExprKind {
    Lit(Lit),
    Format(Vec<FormatPart>),
    Name(ResolvedName),
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    Set(Vec<Expr>),
    Dict(Vec<DictEntry>),
    Comp(Box<Comp>),
    Lambda { params: Params, body: Box<Expr>, captures: Vec<CaptureSpec> },
    Call { callee: Box<Expr>, args: Vec<CallArg> },
    Attr { base: Box<Expr>, name: String },
    Index { base: Box<Expr>, index: Box<IndexKind> },
    Bin { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Un { op: UnaryOp, operand: Box<Expr> },
    Await(Box<Expr>),
    Yield(Option<Box<Expr>>),
    YieldFrom(Box<Expr>),
}

// Resolved-name representation; binding sites carry DefId, name uses
// carry the resolved DefId.
pub struct ResolvedName { pub name: String, pub def_id: DefId, pub kind: DefKind }

// Errors, see adr:0005 §"Error taxonomy".
pub enum LoweringError {
    UnknownName { name: String, span: Span },
    DroppedFeature { name: &'static str, span: Span },
    MutableDefault { span: Span },
    OrPatternBindingMismatch { span: Span },
    DuplicateBinding { name: String, first: Span, second: Span },
    AssignToUnknown { name: String, span: Span },
}
```

## Desugaring scope (M2 — delivered)

Every row of ADR-0005's lowering table is implemented and tested.
The condensed list:

- Comprehensions → `Expr::Comp(Comp { kind, element, clauses })`
- `with a as x, b as y: body` → left-folded nested `With`
- f-strings → `Expr::Format(Vec<FormatPart>)`
- Decorators → `Item::Decorated { decorators, inner }`
- Augmented assignment → desugared to `target = target op rhs`
- `if/elif/else` → `Stmt::If { arms: Vec<(Expr, Block)>, else_block }`
- `for x in xs / while c` unified → `Stmt::Loop(LoopKind::{For,While})`
- `_` (when in a pattern position) → canonicalised to `Wildcard`
  regardless of how the parser tokenised the `_` glyph.
- `from a import x, y` → one `Import` item per target.

## Invariants (M2)

- Every name binding has a unique `DefId`.
- Every `name_expr` use has a resolved `DefId` (or
  `LoweringError::UnknownName`).
- HIR is hygienic — no shadowing ambiguity left over from source.
- Lowering is total: any well-formed AST yields either a well-formed
  HIR or a structured `LoweringError`. No panic paths.
- Spans flow from AST to HIR: every node carries its origin span.

## Done means (M2 — DONE)

- [x] Lowering for every form in the "core 30 forms" suite.
- [x] Name resolution covers module / function / class / loop /
      comprehension / pattern-arm scopes.
- [x] No panics on any AST produced by `mod:frontend`.
- [x] 34 golden lowering tests green
      (`crates/cobrust-hir/tests/lower_forms.rs`).
- [x] `adr:0005` accepted; lowering implementation matches.

## Non-goals

- No type information. That's `mod:types`.
- No optimization. That's `mod:mir`.
- No closure-mode enforcement (`copy` / `ref` / `move`); that lands
  at M3 (constitution §2.2).
- Decorator expansion to chained `Call` is **deferred** — M2
  preserves user intent via `Item::Decorated`. Full expansion lands
  at MIR.

## Cross-references

- `adr:0005` — HIR shape and lowering rules (authoritative).
- `mod:frontend` — input.
- `mod:types` — output consumer.
- Constitution `CLAUDE.md` §2.2 (drops), §7 (M2 done means).
