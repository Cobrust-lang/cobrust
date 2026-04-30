//! Per-form desugaring helpers. Each helper performs the structural
//! rewrite documented in ADR-0005's "Lowering tables" section. The
//! helpers are pure: they accept already-lowered `tree::*` pieces
//! and emit reshaped pieces. The orchestration (scope management,
//! `DefId` allocation, descent into AST) lives in [`crate::lower`].
//!
//! Conventions:
//!
//! - **Idempotence by construction**: each helper produces exactly
//!   one HIR shape, never branching on previous transformations.
//! - **Span discipline**: helpers preserve the user-source span of
//!   the form they desugar. Synthetic constructs introduced during
//!   desugaring (e.g. the right-hand side of an augmented assignment)
//!   share that span — the type checker's diagnostics still cite the
//!   user's source byte range.

use cobrust_frontend::ast;

use crate::tree::{BinOp, UnaryOp};

/// Translate the AST's [`ast::AssignOp`] into the equivalent HIR
/// [`BinOp`]. ADR-0005 row 8b: `x op= e` → `x = x op e`.
///
/// Returns `None` for `AssignOp::Eq` (plain assignment — no binary
/// operator involved).
#[must_use]
pub fn assign_op_to_bin(op: ast::AssignOp) -> Option<BinOp> {
    use ast::AssignOp::*;
    match op {
        Eq => None,
        PlusEq => Some(BinOp::Add),
        MinusEq => Some(BinOp::Sub),
        StarEq => Some(BinOp::Mul),
        StarStarEq => Some(BinOp::Pow),
        SlashEq => Some(BinOp::Div),
        SlashSlashEq => Some(BinOp::FloorDiv),
        PercentEq => Some(BinOp::Mod),
        AmpEq => Some(BinOp::BitAnd),
        PipeEq => Some(BinOp::BitOr),
        CaretEq => Some(BinOp::BitXor),
        ShlEq => Some(BinOp::Shl),
        ShrEq => Some(BinOp::Shr),
    }
}

/// Identity translation of binary operators. Defined in this module
/// (not just inline in `lower`) so that future changes to the
/// HIR/AST operator set are reviewed in one place.
#[must_use]
pub fn lower_bin_op(op: ast::BinOp) -> BinOp {
    use ast::BinOp::*;
    match op {
        Add => BinOp::Add,
        Sub => BinOp::Sub,
        Mul => BinOp::Mul,
        MatMul => BinOp::MatMul,
        Div => BinOp::Div,
        FloorDiv => BinOp::FloorDiv,
        Mod => BinOp::Mod,
        Pow => BinOp::Pow,
        Shl => BinOp::Shl,
        Shr => BinOp::Shr,
        BitAnd => BinOp::BitAnd,
        BitOr => BinOp::BitOr,
        BitXor => BinOp::BitXor,
        Eq => BinOp::Eq,
        NotEq => BinOp::NotEq,
        Lt => BinOp::Lt,
        LtEq => BinOp::LtEq,
        Gt => BinOp::Gt,
        GtEq => BinOp::GtEq,
        And => BinOp::And,
        Or => BinOp::Or,
        In => BinOp::In,
        NotIn => BinOp::NotIn,
    }
}

/// Identity translation of unary operators.
#[must_use]
pub fn lower_unary_op(op: ast::UnaryOp) -> UnaryOp {
    use ast::UnaryOp::*;
    match op {
        Plus => UnaryOp::Plus,
        Neg => UnaryOp::Neg,
        BitNot => UnaryOp::BitNot,
        Not => UnaryOp::Not,
    }
}

/// Convert an [`ast::Literal`] to its HIR counterpart. Identity
/// rename — the HIR [`crate::tree::Lit`] mirrors the AST literal
/// shape, but with a HIR-local enum.
#[must_use]
pub fn lower_literal(lit: ast::Literal) -> crate::tree::Lit {
    use crate::tree::Lit;
    match lit {
        ast::Literal::Bool(b) => Lit::Bool(b),
        ast::Literal::None => Lit::None,
        ast::Literal::Int(s) => Lit::Int(s),
        ast::Literal::Float(s) => Lit::Float(s),
        ast::Literal::Imag(s) => Lit::Imag(s),
        ast::Literal::Str(s) => Lit::Str(s),
        ast::Literal::Bytes(b) => Lit::Bytes(b),
    }
}
