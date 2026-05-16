//! Span-bearing AST for the core 30 syntactic forms.
//!
//! See `docs/agent/adr/0003-core-30-forms.md` for the authoritative
//! list. Every node carries a [`Span`].
//!
//! # Conventions
//!
//! - Each top-level family (`Module`, `Stmt`, `Expr`, `Pattern`,
//!   `Type`) has a wrapper struct with a `kind` enum and a `span`.
//! - `Box`-wrapping is used liberally to keep enum sizes bounded.
//! - The AST does **not** preserve trivia (comments / blank lines /
//!   whitespace). Round-tripping is on AST shape, not source bytes.

use crate::span::Span;

// =====================================================================
// Module (form 1) and items
// =====================================================================

/// A whole compilation unit: form #1.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    /// Optional module-level docstring; the leading `expr_stmt`
    /// containing a string literal, if any.
    pub docstring: Option<String>,
    pub items: Vec<Stmt>,
    pub span: Span,
}

// =====================================================================
// Statements (forms 2–19)
// =====================================================================

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StmtKind {
    /// Form 2 — `import a.b as c` / `from a import x as y, z`.
    Import(ImportStmt),
    /// Form 3 — `fn` definition.
    Fn(FnDef),
    /// Form 4 — `class` definition.
    Class(ClassDef),
    /// Form 5 — decorated definition. Decorators wrap an inner
    /// `Stmt` (which is always `Fn` or `Class`).
    Decorated {
        decorators: Vec<Expr>,
        inner: Box<Stmt>,
    },
    /// Form 6 — `type` alias.
    TypeAlias(TypeAlias),
    /// Form 7 — `let` binding.
    Let {
        target: Pattern,
        annot: Option<Type>,
        value: Expr,
    },
    /// Form 8 — assignment / augmented assignment.
    Assign {
        target: Box<Expr>,
        op: AssignOp,
        value: Expr,
    },
    /// Form 9 — `if/elif/else`.
    If {
        cond: Expr,
        then_block: Block,
        elifs: Vec<(Expr, Block)>,
        else_block: Option<Block>,
    },
    /// Form 10 — `while`.
    While {
        cond: Expr,
        body: Block,
        else_block: Option<Block>,
    },
    /// Form 11 — `for`.
    For {
        target: Pattern,
        iter: Expr,
        body: Block,
        else_block: Option<Block>,
    },
    /// Form 12 — `match`.
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
    },
    /// Form 13 — `with`.
    With { items: Vec<WithItem>, body: Block },
    /// Form 14 — `try/except/else/finally`.
    Try {
        body: Block,
        handlers: Vec<ExceptHandler>,
        else_block: Option<Block>,
        finally_block: Option<Block>,
    },
    /// Form 15 — `return`.
    Return(Option<Expr>),
    /// Form 16 — `break` / `continue` (single form, two keywords).
    BreakContinue(BreakKind),
    /// Form 17 — `raise expr (from expr)?`.
    Raise {
        exc: Option<Expr>,
        cause: Option<Expr>,
    },
    /// Form 18 — `pass`.
    Pass,
    /// Form 19 — bare expression statement.
    Expr(Expr),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreakKind {
    Break,
    Continue,
}

/// `import`/`from import` payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportStmt {
    /// `import dotted.name (as alias)?`
    Import {
        path: Vec<String>,
        alias: Option<String>,
    },
    /// `from dotted.name import a (as b), c, ...`
    From {
        path: Vec<String>,
        targets: Vec<ImportTarget>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportTarget {
    pub name: String,
    pub alias: Option<String>,
}

/// Function definition (form 3). `params` may include positional,
/// `*args`, `**kwargs` and literal-only defaults.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FnDef {
    pub name: String,
    pub params: Params,
    pub return_type: Option<Type>,
    pub body: Block,
}

/// Class definition (form 4): single base + trait list, no MRO.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassDef {
    pub name: String,
    pub base: Option<Expr>,
    pub traits: Vec<Type>,
    pub body: Block,
}

/// Type alias (form 6): `type Foo[T] = T | None`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeAlias {
    pub name: String,
    pub type_params: Vec<String>,
    pub value: Type,
}

/// Augmented or plain assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssignOp {
    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    StarStarEq,
    SlashEq,
    SlashSlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,
}

/// One arm of a `match` (form 12).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Block,
}

/// One `with`-item: `expr (as target)?`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithItem {
    pub context: Expr,
    pub target: Option<Pattern>,
}

/// One `except`-handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExceptHandler {
    pub exc_type: Type,
    pub binding: Option<String>,
    pub body: Block,
}

/// A block: an `INDENT`-delimited sequence of statements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

// =====================================================================
// Parameters
// =====================================================================

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Params {
    pub positional: Vec<Param>,
    /// `*args`
    pub var_positional: Option<Param>,
    /// keyword-only after a bare `*` separator
    pub keyword_only: Vec<Param>,
    /// `**kwargs`
    pub var_keyword: Option<Param>,
}

/// A single named parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Param {
    pub name: String,
    pub annot: Option<Type>,
    /// Literal-only default: see ADR-0003 form 3.
    pub default: Option<Literal>,
    pub span: Span,
}

// =====================================================================
// Type annotations (annotation sub-language)
// =====================================================================

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeKind {
    /// Bare or dotted name: `i64`, `collections.OrderedDict`.
    Name(Vec<String>),
    /// Generic application: `List[T]`, `Dict[K, V]`.
    Generic { base: Vec<String>, args: Vec<Type> },
    /// Union: `A | B | C`.
    Union(Vec<Type>),
    /// Function type: `(A, B) -> C`.
    Fn {
        params: Vec<Type>,
        return_type: Box<Type>,
    },
    /// Tuple type: `(A, B)`.
    Tuple(Vec<Type>),
}

// =====================================================================
// Patterns (form 20)
// =====================================================================

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatternKind {
    /// Wildcard `_`.
    Wildcard,
    /// Capture: identifier binds the matched value.
    Binding(String),
    /// Literal: only int / float / string / bytes / bool / None.
    Literal(Literal),
    /// Sequence pattern: `[a, b, *rest]` or `(a, b)`.
    Sequence {
        items: Vec<Pattern>,
        rest: Option<Box<Pattern>>,
    },
    /// Mapping pattern: `{"k": p, **rest}`.
    Mapping {
        entries: Vec<(Expr, Pattern)>,
        rest: Option<String>,
    },
    /// Class pattern: `Point(x=0, y)`.
    Class {
        base: Vec<String>,
        positional: Vec<Pattern>,
        keyword: Vec<(String, Pattern)>,
    },
    /// Or-pattern: `A | B | C`.
    Or(Vec<Pattern>),
}

// =====================================================================
// Expressions (forms 21–30)
// =====================================================================

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExprKind {
    /// Form 21 — int/float/string/bytes/bool/None/imag.
    Literal(Literal),
    /// Form 22 — f-string.
    FString(Vec<FStrPart>),
    /// Form 23 — name reference.
    Name(String),
    /// Form 24 — `(...)`, `[...]`, `{...}` (set or dict).
    Collection(CollectionLit),
    /// Form 25 — list / set / dict / generator comprehension.
    Comprehension(Box<Comprehension>),
    /// Form 26 — `lambda p: e`.
    Lambda {
        params: Params,
        body: Box<Expr>,
    },
    /// Form 27 — call `f(args)`.
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
    },
    /// Form 28 — attribute or index access.
    Access(AccessKind),
    /// Form 29 — binary or unary op (full Pratt table).
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    /// Form 30 — `await e`, `yield e?`, `yield from e`.
    Await(Box<Expr>),
    Yield(Option<Box<Expr>>),
    YieldFrom(Box<Expr>),
    /// `<expr> as <type>` — explicit numeric cast (M-F.3.3 gap a).
    /// Only i64↔f64 and bool→i64 are permitted; the type checker
    /// enforces permitted pairs (constitution §2.2: no silent coercion).
    Cast {
        expr: Box<Expr>,
        target: Type,
    },
}

/// Literal expression payload (form 21).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Literal {
    Int(String),
    Float(String),
    Imag(String),
    Str(String),
    Bytes(Vec<u8>),
    Bool(bool),
    None,
}

/// One part of an f-string body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FStrPart {
    /// Literal text.
    Lit(String),
    /// Interpolation `{expr [=] [: format_spec]}`.
    Expr {
        expr: Box<Expr>,
        debug_equals: bool,
        format_spec: Option<String>,
    },
}

/// `[...] / {...} / (...)` collection literals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CollectionLit {
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    Set(Vec<Expr>),
    Dict(Vec<DictEntry>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DictEntry {
    Pair(Expr, Expr),
    Spread(Expr), // `**rest`
}

/// Comprehension (form 25). `kind` discriminates list/set/dict/gen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Comprehension {
    pub kind: ComprehensionKind,
    pub element: ComprehensionElem,
    pub clauses: Vec<ComprehensionClause>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComprehensionKind {
    List,
    Set,
    Dict,
    Generator,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComprehensionElem {
    /// `x*x` for list/set/gen.
    Single(Expr),
    /// `k: v` for dict.
    KeyValue(Expr, Expr),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComprehensionClause {
    pub target: Pattern,
    pub iter: Expr,
    pub guards: Vec<Expr>,
}

/// Call argument (form 27).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallArg {
    Positional(Expr),
    Keyword(String, Expr),
    StarArgs(Expr),
    StarStarKwargs(Expr),
}

/// Attribute / index access (form 28).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccessKind {
    Attribute {
        base: Box<Expr>,
        name: String,
    },
    Index {
        base: Box<Expr>,
        index: Box<IndexKind>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexKind {
    /// Single expression.
    Expr(Expr),
    /// Slice `start:stop:step`, all optional.
    Slice {
        start: Option<Expr>,
        stop: Option<Expr>,
        step: Option<Expr>,
    },
    /// Tuple of indices: `arr[i, j]`.
    Tuple(Vec<IndexKind>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    MatMul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    In,
    NotIn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnaryOp {
    Plus,
    Neg,
    BitNot,
    Not,
}
