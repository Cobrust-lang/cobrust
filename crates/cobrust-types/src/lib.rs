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
pub mod ecosystem;
pub mod error;
pub mod fix_safety;
pub mod infer;
pub mod refinement;
pub mod ty;

pub use check::{TypeCheckCtx, TypedModule, check, check_incremental};
pub use ecosystem::{
    COIL_BUFFER_ADT, DEN_CONNECTION_ADT, DEN_CURSOR_ADT, DORA_EVENT_ADT, DORA_NODE_ADT, EcoParam,
    EcoSig, HOOD_COMMAND_ADT, MOLT_DATETIME_ADT, PIT_APP_ADT, PIT_REQUEST_ADT, PIT_RESPONSE_ADT,
    PIT_SERVER_HANDLE_ADT, PIT_VALIDATED_BODY_SENTINEL_ADT, PyCompatTier, STRIKE_RESPONSE_ADT,
    coil_buffer_getitem_symbol, coil_buffer_setitem_symbol, coil_buffer_slice_symbol,
    coil_buffer_ty, dora_event_handler_fn_ty, dora_event_ty, dora_node_ty, handle_drop_symbol,
    hood_command_handler_fn_ty, hood_command_ty, is_ecosystem_handle, is_ecosystem_module,
    is_subnamespace, lookup_buffer_binop, lookup_buffer_left_scalar_binop,
    lookup_buffer_scalar_binop, lookup_handle_attr, lookup_handle_method, lookup_module_fn,
    lookup_subnamespace_fn, lookup_validated_body_accessor, pit_app_ty, pit_handler_fn_ty,
    pit_request_ty, pit_response_ty, pit_server_handle_ty, pit_validated_handler_fn_ty,
};
pub use error::TypeError;
pub use fix_safety::{FixSafety, Suggestion, type_error_fix_safety, type_error_suggestion_text};
pub use infer::{Subst, finalize, unify};
pub use refinement::Refinement;
pub use ty::{AdtId, AliasId, FnTy, GenericVar, Record, Ty, VarAllocator, VarId};
