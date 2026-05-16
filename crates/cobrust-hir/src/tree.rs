//! HIR data tree: span-bearing, name-resolved, sugar-free.
//!
//! Every node carries a [`Span`] from `cobrust-frontend`. Bindings
//! carry a [`DefId`] from [`crate::scope`]. The shape is exactly
//! what `adr:0005` enumerates.

use cobrust_frontend::span::Span;

use crate::scope::{DefId, ResolvedName};

// =====================================================================
// Module + items (forms 1, 2, 3, 4, 5, 6)
// =====================================================================

/// Top-level compilation unit (form 1).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    pub docstring: Option<String>,
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ItemKind {
    /// Form 2 (canonicalised: `from a import x, y` becomes one
    /// `Import` item per target).
    Import {
        def_id: DefId,
        path: Vec<String>,
        local_name: String,
        from_name: Option<String>,
    },
    /// Form 3 (function definition).
    Fn(FnBody),
    /// Form 4 (class definition).
    Class(ClassBody),
    /// Form 6 (type alias).
    TypeAlias(TypeAliasBody),
    /// Form 5 (decorator wrapping `Fn` / `Class` / nested
    /// `Decorated`).
    Decorated {
        decorators: Vec<Expr>,
        inner: Box<Item>,
    },
    /// Top-level `let` (forms 7 at module level).
    Let(LetBody),
    /// Top-level expression statement (form 19).
    ExprStmt(Expr),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FnBody {
    pub def_id: DefId,
    pub name: String,
    pub params: Params,
    pub return_type: Option<Type>,
    pub body: Block,
    pub captures: Vec<CaptureSpec>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassBody {
    pub def_id: DefId,
    pub name: String,
    pub base: Option<Expr>,
    pub traits: Vec<Type>,
    pub members: Vec<Item>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeAliasBody {
    pub def_id: DefId,
    pub name: String,
    pub type_params: Vec<DefId>,
    pub type_param_names: Vec<String>,
    pub value: Type,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LetBody {
    pub def_id: DefId,
    pub pattern: Pattern,
    pub annot: Option<Type>,
    pub value: Expr,
    pub span: Span,
}

/// Closure capture record; M2 retains capture *names* but does not
/// enforce explicit `copy` / `ref` / `move` capture (deferred to
/// M3 per ADR-0005 / ADR-0006).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureSpec {
    pub name: String,
    pub def_id: DefId,
    pub span: Span,
}

// =====================================================================
// Statements (forms 7–19, minus 7 already covered as `LetBody`)
// =====================================================================

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StmtKind {
    /// Form 7.
    Let(LetBody),
    /// Form 8 (plain or augmented assignment, after desugaring).
    Assign {
        target: Box<Expr>,
        value: Expr,
    },
    /// Form 9 — `if` arms + optional `else`.
    If {
        arms: Vec<(Expr, Block)>,
        else_block: Option<Block>,
    },
    /// Forms 10 / 11 unified.
    Loop(LoopKind),
    /// Form 12 — `match` (exhaustiveness checked at type-check).
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
    },
    /// Form 13 — `with` (multi-binding left-folded into nested
    /// `With`s during lowering).
    With {
        item: WithItem,
        body: Block,
    },
    /// Form 14 — `try`.
    Try {
        body: Block,
        handlers: Vec<ExceptHandler>,
        else_block: Option<Block>,
        finally_block: Option<Block>,
    },
    /// Form 15.
    Return(Option<Expr>),
    /// Form 16.
    Break,
    Continue,
    /// Form 17.
    Raise {
        exc: Option<Expr>,
        cause: Option<Expr>,
    },
    /// Form 18.
    Pass,
    /// Form 19.
    Expr(Expr),
    /// Nested item (function-local fn / class / type alias).
    Item(Item),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoopKind {
    While {
        cond: Expr,
        body: Block,
        else_block: Option<Block>,
        span: Span,
    },
    For {
        binding_def_ids: Vec<DefId>,
        pattern: Pattern,
        iter: Expr,
        body: Block,
        else_block: Option<Block>,
        span: Span,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub binding_def_ids: Vec<DefId>,
    pub guard: Option<Expr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithItem {
    pub context: Expr,
    pub binding: Option<(DefId, Pattern)>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExceptHandler {
    pub exc_type: Type,
    pub binding: Option<(DefId, String)>,
    pub body: Block,
    pub span: Span,
}

// =====================================================================
// Parameters
// =====================================================================

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Params {
    pub positional: Vec<Param>,
    pub var_positional: Option<Param>,
    pub keyword_only: Vec<Param>,
    pub var_keyword: Option<Param>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Param {
    pub def_id: DefId,
    pub name: String,
    pub annot: Option<Type>,
    pub default: Option<Lit>,
    pub span: Span,
}

// =====================================================================
// Type annotations (annotation sub-language, identity-lowered)
// =====================================================================

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeKind {
    Name(Vec<String>),
    Generic {
        base: Vec<String>,
        args: Vec<Type>,
    },
    Union(Vec<Type>),
    Fn {
        params: Vec<Type>,
        return_type: Box<Type>,
    },
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
    Wildcard,
    /// `Binding(name, def_id)`. The `def_id` is allocated during
    /// lowering and uniquely identifies this binding site.
    Binding(String, DefId),
    Literal(Lit),
    Sequence {
        items: Vec<Pattern>,
        rest: Option<Box<Pattern>>,
    },
    Mapping {
        entries: Vec<(Expr, Pattern)>,
        rest: Option<(String, DefId)>,
    },
    Class {
        base: Vec<String>,
        positional: Vec<Pattern>,
        keyword: Vec<(String, Pattern)>,
    },
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
    /// Form 21.
    Lit(Lit),
    /// Form 22 — desugared f-string. `parts` interleaves literal
    /// chunks and interpolated holes.
    Format(Vec<FormatPart>),
    /// Form 23 — resolved name.
    Name(ResolvedName),
    /// Form 24a.
    Tuple(Vec<Expr>),
    /// Form 24b.
    List(Vec<Expr>),
    /// Form 24c.
    Set(Vec<Expr>),
    /// Form 24d.
    Dict(Vec<DictEntry>),
    /// Form 25.
    Comp(Box<Comp>),
    /// Form 26.
    Lambda {
        params: Params,
        body: Box<Expr>,
        captures: Vec<CaptureSpec>,
    },
    /// Form 27.
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
    },
    /// Form 28a.
    Attr { base: Box<Expr>, name: String },
    /// Form 28b.
    Index {
        base: Box<Expr>,
        index: Box<IndexKind>,
    },
    /// Form 29 — binary.
    Bin {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// Form 29 — unary.
    Un { op: UnaryOp, operand: Box<Expr> },
    /// Form 30 — `await e`.
    Await(Box<Expr>),
    /// Form 30 — `yield e?`.
    Yield(Option<Box<Expr>>),
    /// Form 30 — `yield from e`.
    YieldFrom(Box<Expr>),
    /// `expr as Type` — explicit numeric cast (M-F.3.3 gap a).
    Cast {
        expr: Box<Expr>,
        target: cobrust_frontend::ast::Type,
    },
}

/// Literal payload — same shape as the AST literal, recapitulated to
/// keep the HIR self-sufficient.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Lit {
    Bool(bool),
    None,
    Int(String),
    Float(String),
    Imag(String),
    Str(String),
    Bytes(Vec<u8>),
}

/// One part of a desugared f-string (form 22).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatPart {
    Lit(String),
    Hole {
        expr: Expr,
        debug_equals: bool,
        format_spec: Option<String>,
    },
}

/// Dict literal entries (form 24d).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DictEntry {
    Pair(Expr, Expr),
    Spread(Expr),
}

/// Comprehension (form 25).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Comp {
    pub kind: CompKind,
    pub element: CompElem,
    pub clauses: Vec<CompClause>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompKind {
    List,
    Set,
    Dict,
    Generator,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompElem {
    Single(Expr),
    KeyValue(Expr, Expr),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompClause {
    pub binding_def_ids: Vec<DefId>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexKind {
    Expr(Expr),
    Slice {
        start: Option<Expr>,
        stop: Option<Expr>,
        step: Option<Expr>,
    },
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
