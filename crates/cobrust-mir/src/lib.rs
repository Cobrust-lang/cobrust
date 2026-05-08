//! `cobrust-mir` — mid-level intermediate representation.
//!
//! M8 deliverable. The MIR is the control-flow-explicit form fed to
//! `cobrust-codegen`. ADR-0020 is the authoritative design document
//! and pins:
//!
//! - **Node families**: `Module`, `Body`, `BasicBlock`, `Statement`,
//!   `Terminator`, `Place` / `Rvalue` / `Operand` (six primary
//!   families plus supporting types).
//! - **Terminator taxonomy**: `Goto / SwitchInt / Return / Call /
//!   Drop / Unreachable / Assert`.
//! - **Drop schedule algorithm**: 5 phases — init, move, end-of-scope,
//!   divergence, verification.
//! - **Borrow-check obligations**: 5 obligations (B1..B5) that
//!   discharge ADR-0006's flow obligations onto MIR-time.
//!
//! Public surface:
//!
//! - [`Module`] / [`Body`] / [`BasicBlock`] / [`Statement`] /
//!   [`Terminator`] / [`Place`] / [`Rvalue`] / [`Operand`] — IR shape.
//! - [`lower`] — typed-HIR → MIR entrypoint:
//!   `lower(&types::TypedModule) → Result<Module, MirError>`.
//! - [`MirError`] — structured error taxonomy.
//!
//! See `docs/agent/modules/mir.md` for the full agent-facing spec
//! and `docs/agent/adr/0020-m8-mir-shape.md` for the design.

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
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::only_used_in_recursion)]
#![allow(clippy::self_only_used_in_recursion)]
#![allow(clippy::unused_self)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::needless_continue)]
#![allow(clippy::if_not_else)]
#![allow(clippy::redundant_else)]
#![allow(clippy::result_large_err)]
#![allow(clippy::iter_over_hash_type)]
#![allow(clippy::for_kv_map)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::single_match)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::needless_for_each)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::unused_unit)]

pub mod borrow;
pub mod drop;
pub mod error;
pub mod lower;
pub mod tree;

pub use borrow::borrow_check;
pub use drop::compute_drop_schedule;
pub use error::MirError;
pub use lower::lower;
pub use tree::{
    AggregateKind, AssertKind, BasicBlock, BinOp, BlockId, Body, BorrowKind, CastKind, Constant,
    LocalDecl, LocalId, Module, NullaryOp, Operand, Place, PlaceDebug, Projection, Rvalue,
    Statement, StatementKind, SwitchValue, Terminator, UnOp,
};
