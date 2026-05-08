//! `cobrust-stdlib` — Cobrust's standard library + runtime shim.
//!
//! M11 deliverable. ADR-0025 is the authoritative design document
//! and pins:
//!
//! - **Module surfaces**: io / collections / string / math / panic /
//!   env / fmt — the seven binding modules from ADR-0019 §"M11".
//! - **Runtime ABI**: C-ABI symbols (`__cobrust_print`,
//!   `__cobrust_println`, `__cobrust_panic`, `__cobrust_assert`,
//!   `__cobrust_main_shim`) consumed by codegen-emitted calls.
//! - **Heap allocator**: mimalloc by default (`mimalloc-alloc`
//!   feature); `system-alloc` opts back to libc.
//! - **Error taxonomy**: `Error` enum unifying io / parse / custom;
//!   constitution §2.2 binds `Result<T, E>` as the default error
//!   path.
//!
//! Constitution `CLAUDE.md` §2.2 requirements reflected here:
//!
//! - No implicit truthiness — `List::is_empty` exists; users write
//!   `if list.is_empty()` not `if list`.
//! - No `dyn` in the public surface (constitution §5.1).
//! - Result<T, E> over exceptions.
//!
//! Public surface:
//!
//! - [`io`] — print / println / read_line / read_file / write_file
//!   plus stdin / stdout / stderr handles.
//! - [`collections`] — `List<T>` / `Dict<K, V>` / `Set<T>` newtypes
//!   over Rust's collections.
//! - [`string`] — len / find / replace / split / strip / lower /
//!   upper / format helpers.
//! - [`math`] — sqrt / pow / sin / cos / abs / floor / ceil / round
//!   plus PI / E constants.
//! - [`panic`] — panic / assert; runtime ABI for codegen.
//! - [`env`] — args / var.
//! - [`fmt`] — f-string formatting helpers.
//! - [`runtime`] — heap allocator selection + main shim + error
//!   taxonomy.
//!
//! See `docs/agent/modules/stdlib.md` for the full agent-facing spec
//! and `docs/agent/adr/0025-m11-stdlib-runtime.md` for the design.

#![forbid(unsafe_op_in_unsafe_fn)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::single_match_else)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::result_large_err)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::format_push_string)]
#![allow(clippy::iter_without_into_iter)]
#![allow(clippy::multiple_bound_locations)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::approx_constant)]

pub mod collections;
pub mod env;
pub mod fmt;
pub mod io;
pub mod math;
pub mod panic;
pub mod runtime;
pub mod string;

pub use runtime::{Error, ErrorKind};

// Re-export the seven binding module roots at crate root for
// convenience. Cobrust source-level `import std.X` will project
// onto these paths.
pub use collections::{Dict, List, Set};
