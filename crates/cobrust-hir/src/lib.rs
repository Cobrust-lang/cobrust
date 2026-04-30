//! `cobrust-hir` — high-level intermediate representation.
//!
//! M2 deliverable. The HIR is the form `cobrust-types` consumes:
//! AST sugar collapsed (comprehensions → loops + collector,
//! decorators → `Item::Decorated`, augmented assignment → desugared
//! read-modify-write, `with` left-folded, walrus reified as
//! let-binding); names resolved to [`DefId`]s; spans preserved.
//!
//! ADR-0005 is the authoritative design document. Every form in
//! ADR-0003 has a per-form lowering rule documented there and a
//! golden test in [`tests/lower_forms.rs`](../tests/lower_forms.rs).
//!
//! Public surface:
//!
//! - [`Module`] — the lowered top-level tree.
//! - [`lower`] — the lowering entrypoint; AST → HIR.
//! - [`LoweringError`] — all failure modes.
//! - [`Session`] — owns the [`DefId`] counter and the diagnostic
//!   accumulator used during lowering.
//!
//! The HIR has no panic paths reachable from any AST that
//! `cobrust-frontend` produces; failures surface as
//! [`LoweringError`].

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::single_match_else)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::only_used_in_recursion)]
#![allow(clippy::self_named_module_files)]
#![allow(clippy::self_only_used_in_recursion)]
#![allow(clippy::unused_self)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::return_self_not_must_use)]

pub mod desugar;
pub mod error;
pub mod lower;
pub mod scope;
pub mod tree;

pub use cobrust_frontend::span::{FileId, Span};
pub use error::LoweringError;
pub use lower::{Session, lower};
pub use scope::{DefId, DefKind, ResolvedName};
pub use tree::{
    BinOp, Block, CallArg, CaptureSpec, ClassBody, Comp, CompClause, CompElem, CompKind, DictEntry,
    ExceptHandler, Expr, ExprKind, FnBody, FormatPart, IndexKind, Item, ItemKind, LetBody, Lit,
    LoopKind, MatchArm, Module, Param, Params, Pattern, PatternKind, Stmt, StmtKind, Type,
    TypeAliasBody, TypeKind, UnaryOp, WithItem,
};
