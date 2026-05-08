//! MIR data tree — the form `cobrust-codegen` consumes.
//!
//! ADR-0020 §"MIR node families" pins the shape. Six primary
//! families (`Module`, `Body`, `BasicBlock`, `Statement`, `Terminator`,
//! `Place / Rvalue / Operand`) plus supporting types. Spans flow
//! through every node so codegen / borrow-check diagnostics cite the
//! user's source.

use std::fmt;

use cobrust_frontend::span::Span;
use cobrust_hir::DefId;
use cobrust_types::Ty;

// =====================================================================
// IDs
// =====================================================================

/// Identifier for a basic block within a single [`Body`]. Allocated
/// monotonically by the lowering. Block 0 is always the entry block.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct BlockId(pub u32);

/// Identifier for a local within a single [`Body`]. Allocated
/// monotonically by the lowering. Locals 0..param_count are
/// parameters; the dedicated `_return_local` is allocated at index 0
/// (when convention allows) or returned via `Body::return_local`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct LocalId(pub u32);

// =====================================================================
// Module + Body
// =====================================================================

/// A MIR module — one [`Body`] per typed-HIR `Item::Fn` plus one
/// synthetic `Body::Init` for top-level statements.
#[derive(Clone, Debug)]
pub struct Module {
    pub bodies: Vec<Body>,
}

/// Per-function CFG. `locals[0..param_count]` are the parameters;
/// `return_local` stores the function's return value. Every basic
/// block in `blocks` ends in exactly one terminator.
#[derive(Clone, Debug)]
pub struct Body {
    /// `DefId` from HIR — `u32::MAX` for the synthetic init body.
    pub def_id: DefId,
    /// Diagnostic-only function name.
    pub name: String,
    /// Local declarations, ordered by [`LocalId`].
    pub locals: Vec<LocalDecl>,
    /// Basic blocks, ordered by [`BlockId`]. Block 0 is the entry.
    pub blocks: Vec<BasicBlock>,
    /// The local that holds the return value when `Terminator::Return`
    /// fires. Always allocated.
    pub return_local: LocalId,
    /// First `param_count` locals are parameters.
    pub param_count: usize,
    /// Original span (for diagnostics).
    pub span: Span,
}

impl Body {
    /// Total number of locals (params + bindings + temporaries +
    /// return slot).
    #[must_use]
    pub fn local_count(&self) -> usize {
        self.locals.len()
    }

    /// Total number of basic blocks in this body's CFG.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// True if `id` is one of the body's parameters.
    #[must_use]
    pub fn is_param(&self, id: LocalId) -> bool {
        (id.0 as usize) < self.param_count
    }
}

/// One local declaration. The lowering always assigns a fully-resolved
/// [`Ty`] (no inference variables remain at MIR time — ADR-0020
/// invariant).
#[derive(Clone, Debug)]
pub struct LocalDecl {
    pub id: LocalId,
    /// Display name — usually the source name; `_tmpN` for synthetic
    /// temporaries; `_return` for the return slot.
    pub name: String,
    pub ty: Ty,
    /// True if this local was declared mutable. Immutable locals
    /// can still be moved, but cannot be the LHS of an
    /// `Statement::Assign` after the first.
    pub mutable: bool,
    pub span: Span,
}

// =====================================================================
// Basic blocks
// =====================================================================

#[derive(Clone, Debug)]
pub struct BasicBlock {
    pub id: BlockId,
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
    pub span: Span,
}

// =====================================================================
// Statements
// =====================================================================

#[derive(Clone, Debug)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StatementKind {
    /// `place = rvalue;`
    Assign { place: Place, rvalue: Rvalue },
    /// Storage marker — beginning of a local's lifetime.
    StorageLive(LocalId),
    /// Storage marker — end of a local's lifetime (the local goes
    /// out-of-scope; drops were already inserted by the drop pass
    /// if the local is owning).
    StorageDead(LocalId),
    /// No-op (preserves user's `pass`).
    Nop,
}

// =====================================================================
// Terminators
// =====================================================================

#[derive(Clone, Debug)]
pub enum Terminator {
    /// Unconditional jump.
    Goto(BlockId),
    /// Discriminator switch — `operand` matches one of `cases` or
    /// falls through to `otherwise`.
    SwitchInt {
        operand: Operand,
        cases: Vec<(SwitchValue, BlockId)>,
        otherwise: BlockId,
    },
    /// Return from the body. The caller reads `_return_local`.
    Return,
    /// Call a callable. Continues at `target` on normal return; if
    /// `unwind` is `Some`, panics propagate to that block.
    Call {
        func: Operand,
        args: Vec<Operand>,
        destination: Place,
        target: BlockId,
        unwind: Option<BlockId>,
    },
    /// Drop the value at `place` and jump to `target`. Inserted by
    /// the drop-schedule pass; never written by the lowering directly.
    Drop { place: Place, target: BlockId },
    /// Statically-known dead code (e.g. after `raise` or a
    /// `Never`-typed expression).
    Unreachable,
    /// Runtime assert — continues at `target` on success; panics with
    /// `msg` on failure.
    Assert {
        cond: Operand,
        expected: bool,
        msg: AssertKind,
        target: BlockId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwitchValue {
    Bool(bool),
    Int(i64),
    /// ADT discriminant (`AdtId::0` is the inner u32).
    Adt(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssertKind {
    DivisionByZero,
    Overflow,
    BoundsCheck,
    Unreachable,
}

// =====================================================================
// Place + Projection
// =====================================================================

/// Typed access path into the memory rooted at a [`LocalId`].
#[derive(Clone, Debug)]
pub struct Place {
    pub local: LocalId,
    pub projections: Vec<Projection>,
}

impl Place {
    /// Build a bare-local place with no projections.
    #[must_use]
    pub fn local(id: LocalId) -> Self {
        Self {
            local: id,
            projections: Vec::new(),
        }
    }

    /// Append a projection in-place; returns `self` for chaining.
    #[must_use]
    pub fn with_projection(mut self, proj: Projection) -> Self {
        self.projections.push(proj);
        self
    }
}

#[derive(Clone, Debug)]
pub enum Projection {
    /// Tuple index or record field index.
    Field(usize),
    /// `List` / `Dict` / `Str` / `Bytes` index.
    Index(Operand),
    /// Pointer / reference deref.
    Deref,
    /// ADT discriminant peek.
    Discriminant,
}

/// Diagnostic-only stable description of a [`Place`]; used by the
/// error taxonomy to avoid leaking the runtime `Place` type into
/// public errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaceDebug {
    pub local: u32,
    pub projection_chain: String,
}

impl From<&Place> for PlaceDebug {
    fn from(p: &Place) -> Self {
        use std::fmt::Write as _;
        let mut chain = String::new();
        for proj in &p.projections {
            match proj {
                Projection::Field(i) => {
                    let _ = write!(chain, ".{i}");
                }
                Projection::Index(_) => chain.push_str("[?]"),
                Projection::Deref => chain.push('*'),
                Projection::Discriminant => chain.push_str(".discr"),
            }
        }
        Self {
            local: p.local.0,
            projection_chain: chain,
        }
    }
}

// =====================================================================
// Operand + Rvalue
// =====================================================================

/// A value-producer. Statements assign rvalues to places; rvalues
/// consume operands as leaves.
#[derive(Clone, Debug)]
pub enum Rvalue {
    /// Copy- or move-of-place (or load a constant).
    Use(Operand),
    /// Binary arithmetic, comparison, logic, or bitwise op.
    BinaryOp(BinOp, Operand, Operand),
    /// Unary arithmetic / logic / bitwise.
    UnaryOp(UnOp, Operand),
    /// Aggregate construction (tuple / list / set / dict / record / ADT).
    Aggregate(AggregateKind, Vec<Operand>),
    /// Explicit numeric cast — constitution §2.2 rejects implicit;
    /// the lowering only emits this where the user wrote a cast call.
    Cast(CastKind, Operand, Ty),
    /// Take a borrow of `place`.
    Ref(BorrowKind, Place),
    /// Read the ADT discriminant of `place`.
    Discriminant(Place),
    /// Length of `List` / `Str` / `Bytes`.
    Len(Place),
    /// Nullary placeholder — used by codegen for type-driven
    /// constants (size, alignment); not yet emitted in M8 lowering.
    NullaryOp(NullaryOp),
}

#[derive(Clone, Debug)]
pub enum Operand {
    /// Copy a `Copy` type (`Bool`, `Int`, `Float`, `Imag`, tuples-of-Copy).
    Copy(Place),
    /// Move an owning value (`List`, `Set`, `Dict`, `Str` owned, ...).
    Move(Place),
    /// Inline literal.
    Constant(Constant),
}

#[derive(Clone, Debug)]
pub enum Constant {
    Bool(bool),
    Int(i64),
    /// Floats stored as IEEE-754 bit pattern to keep `Eq`/`Hash`
    /// well-defined. The conversion uses `f64::from_bits`.
    Float(u64),
    Imag(u64),
    Str(String),
    Bytes(Vec<u8>),
    None,
    /// Reference to another body in the same module — produced by
    /// `lambda` / decorator desugaring.
    FnRef(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BorrowKind {
    Shared,
    Mut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    MatMul,
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
    /// `e in container` — the lowering may emit a runtime helper call
    /// instead, but for arithmetic-shaped containers this op is
    /// preserved for codegen.
    In,
    NotIn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnOp {
    Plus,
    Neg,
    BitNot,
    Not,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CastKind {
    /// `Int → Float`.
    IntToFloat,
    /// `Float → Int`.
    FloatToInt,
    /// `Bool → Int`.
    BoolToInt,
    /// `Int → Bool` (`v != 0`).
    IntToBool,
    /// String / bytes ↔ — placeholder; M11 stdlib materializes.
    StrToBytes,
    BytesToStr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AggregateKind {
    Tuple,
    List,
    Set,
    Dict,
    Record,
    /// ADT constructor: `(adt_id, variant_index)`.
    Adt(u32, u32),
    /// f-string format runtime helper — the lowering keeps the
    /// template parts as operands (literal chunks become constants;
    /// holes become operands); M11 stdlib materializes.
    FormatString,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NullaryOp {
    SizeOf,
    AlignOf,
}

// =====================================================================
// Display (deterministic — used for golden tests)
// =====================================================================

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for body in &self.bodies {
            writeln!(f, "{body}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "fn {}#{} (params={}) {{",
            self.name, self.def_id.0, self.param_count
        )?;
        for local in &self.locals {
            writeln!(
                f,
                "    let {}: {} = _{}; mut={}",
                local.name, local.ty, local.id.0, local.mutable
            )?;
        }
        for block in &self.blocks {
            writeln!(f, "    bb{}:", block.id.0)?;
            for stmt in &block.statements {
                writeln!(f, "        {stmt}")?;
            }
            writeln!(f, "        => {}", block.terminator)?;
        }
        write!(f, "}}")
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            StatementKind::Assign { place, rvalue } => write!(f, "{place} = {rvalue}"),
            StatementKind::StorageLive(id) => write!(f, "StorageLive(_{})", id.0),
            StatementKind::StorageDead(id) => write!(f, "StorageDead(_{})", id.0),
            StatementKind::Nop => write!(f, "nop"),
        }
    }
}

impl fmt::Display for Terminator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Terminator::Goto(b) => write!(f, "goto bb{}", b.0),
            Terminator::SwitchInt {
                operand,
                cases,
                otherwise,
            } => {
                write!(f, "switch {operand} ")?;
                for (v, b) in cases {
                    write!(f, "[{v:?} -> bb{}] ", b.0)?;
                }
                write!(f, "[_ -> bb{}]", otherwise.0)
            }
            Terminator::Return => write!(f, "return"),
            Terminator::Call {
                func,
                args,
                destination,
                target,
                unwind,
            } => {
                write!(f, "{destination} = call {func}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, ") -> bb{}", target.0)?;
                if let Some(u) = unwind {
                    write!(f, " unwind bb{}", u.0)?;
                }
                Ok(())
            }
            Terminator::Drop { place, target } => {
                write!(f, "drop {place} -> bb{}", target.0)
            }
            Terminator::Unreachable => write!(f, "unreachable"),
            Terminator::Assert {
                cond,
                expected,
                msg,
                target,
            } => write!(f, "assert({cond} == {expected}, {msg:?}) -> bb{}", target.0),
        }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "_{}", self.local.0)?;
        for proj in &self.projections {
            match proj {
                Projection::Field(i) => write!(f, ".{i}")?,
                Projection::Index(_) => write!(f, "[?]")?,
                Projection::Deref => write!(f, ".*")?,
                Projection::Discriminant => write!(f, ".discr")?,
            }
        }
        Ok(())
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Copy(p) => write!(f, "copy {p}"),
            Operand::Move(p) => write!(f, "move {p}"),
            Operand::Constant(c) => write!(f, "{c:?}"),
        }
    }
}

impl fmt::Display for Rvalue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rvalue::Use(op) => write!(f, "{op}"),
            Rvalue::BinaryOp(op, a, b) => write!(f, "{op:?}({a}, {b})"),
            Rvalue::UnaryOp(op, a) => write!(f, "{op:?}({a})"),
            Rvalue::Aggregate(k, items) => {
                write!(f, "agg<{k:?}>(")?;
                for (i, op) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{op}")?;
                }
                write!(f, ")")
            }
            Rvalue::Cast(k, op, ty) => write!(f, "cast<{k:?}>({op} as {ty})"),
            Rvalue::Ref(k, place) => write!(f, "&{k:?} {place}"),
            Rvalue::Discriminant(p) => write!(f, "discr({p})"),
            Rvalue::Len(p) => write!(f, "len({p})"),
            Rvalue::NullaryOp(k) => write!(f, "{k:?}"),
        }
    }
}

// =====================================================================
// Builders / utilities
// =====================================================================

impl Body {
    /// Iterate every block's terminator + a flag for whether it's a
    /// *return* (for the drop-schedule final-flow check).
    pub fn iter_block_targets(&self) -> impl Iterator<Item = (BlockId, &Terminator)> + '_ {
        self.blocks.iter().map(|b| (b.id, &b.terminator))
    }

    /// Successor `BlockId`s of a given block.
    #[must_use]
    pub fn successors(&self, b: BlockId) -> Vec<BlockId> {
        let block = &self.blocks[b.0 as usize];
        match &block.terminator {
            Terminator::Goto(t) | Terminator::Drop { target: t, .. } => vec![*t],
            Terminator::SwitchInt {
                cases, otherwise, ..
            } => {
                let mut v: Vec<BlockId> = cases.iter().map(|(_, b)| *b).collect();
                v.push(*otherwise);
                v
            }
            Terminator::Call { target, unwind, .. } => {
                let mut v = vec![*target];
                if let Some(u) = unwind {
                    v.push(*u);
                }
                v
            }
            Terminator::Assert { target, .. } => vec![*target],
            Terminator::Return | Terminator::Unreachable => vec![],
        }
    }
}
