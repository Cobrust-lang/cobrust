//! `cobrust-types` — static structural type system + type checker.
//!
//! M2 deliverable. The checker is bidirectional — synthesise where
//! types come from leaves, check where annotations or expected
//! types push down. ADR-0006 is the authoritative design document
//! and pins the proof-obligation list.
//!
//! Public surface:
//!
//! - [`Ty`] — the type universe (no `dyn` at M2).
//! - [`TypeError`] — the structured error taxonomy.
//! - [`check`] — the checker entrypoint:
//!   `check(&hir::Module) → Result<TypedModule, TypeError>`.
//!
//! See `docs/agent/modules/types.md` for the full agent-facing
//! spec, and `docs/agent/adr/0006-type-system.md` for the design.

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

pub mod check;
pub mod error;
pub mod infer;
pub mod ty;

pub use check::{TypedModule, check};
pub use error::TypeError;
pub use infer::{Subst, finalize, unify};
pub use ty::{AdtId, AliasId, FnTy, GenericVar, Record, Ty, VarAllocator, VarId};
