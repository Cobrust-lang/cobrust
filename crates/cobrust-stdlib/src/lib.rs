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
//! M13 amendment (ADR-0028): adds `task` and `sync` modules behind
//! the default-on `tokio-runtime` Cargo feature — structured
//! concurrency primitives (`spawn / JoinHandle / scope / cancel`) +
//! bounded MPSC channels. Constitution §2.2's "no async/sync
//! coloring" is honored: every public function in `task` and `sync`
//! is `fn`, not `async fn`.
//!
//! Constitution `CLAUDE.md` §2.2 requirements reflected here:
//!
//! - No implicit truthiness — `List::is_empty` exists; users write
//!   `if list.is_empty()` not `if list`.
//! - No `dyn` in the public surface (constitution §5.1).
//! - Result<T, E> over exceptions.
//! - No async/sync coloring (M13/ADR-0028).
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
//! - [`re`] (ADR-0084) — `sub` / `findall` / `match` / `search`, the
//!   `regex`-crate-backed stateless subset of Python's `re` module
//!   (str/list[str]/bool returns; Match-object `.group()` deferred).
//!   `@py_compat(semantic)`.
//! - [`random`] (ADR-0086) — `random` / `randint` / `uniform` / `seed`,
//!   the scalar core of Python's `random` over a thread-local
//!   `rand_pcg::Pcg64` module-global RNG (seed-reproducible;
//!   `choice`/`shuffle`/`sample` deferred). `@py_compat(semantic)`.
//! - [`json`] (v0.7.0 Z.5) — `dumps` / `loads` Python-`json`-compatible
//!   encode/decode over `serde_json`. HYBRID surface per the v0.7.0
//!   network-backend roadmap §4.1; `@py_compat(semantic)`.
//! - [`panic`] — panic / assert; runtime ABI for codegen.
//! - [`env`] — args / var.
//! - [`fmt`] — f-string formatting helpers.
//! - [`runtime`] — heap allocator selection + main shim + error
//!   taxonomy.
//! - [`task`] (M13) — `spawn / JoinHandle / scope / cancel`. Gated
//!   by `tokio-runtime` (default-on).
//! - [`sync`] (M13) — bounded MPSC `channel`. Gated by
//!   `tokio-runtime` (default-on).
//!
//! See `docs/agent/modules/stdlib.md` for the full agent-facing spec
//! and `docs/agent/adr/0025-m11-stdlib-runtime.md` (M11) +
//! `docs/agent/adr/0028-m13-concurrency-runtime.md` (M13) for the
//! design.

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
// M12.x ADR-0027 §1: the `*mut u8` ↔ `*mut <Layout>` casts in
// __cobrust_<list|dict|set|tuple>_* + `__cobrust_str_*` + iter
// runtime are intentional FFI layout punning. Cobrust-side
// allocators always return 8-byte-aligned pointers (via
// `Layout::from_size_align(_, 8)` or the global allocator), so the
// stricter alignment is satisfied; clippy can't see that.
#![allow(clippy::cast_ptr_alignment)]

pub mod array;
pub mod collections;
pub mod env;
pub mod fmt;
pub mod io;
pub mod iter;
pub mod json;
pub mod math;
pub mod panic;
pub mod prompt;
// ADR-0086 — `import random` (pseudo-random sampling). A thread-local
// `rand_pcg::Pcg64` module-global RNG: random / randint / uniform / seed.
pub mod random;
// ADR-0084 — `import re` (regular expressions). The `regex`-crate-backed
// stateless subset: sub / findall / match / search.
pub mod re;
pub mod runtime;
pub mod string;
pub mod tool;

// =====================================================================
// M-AI.0 — cobrust.llm source-level binding (ADR-0048 + spike 705f592)
// Gated by the default-on `llm-router` Cargo feature, mirroring how
// M13 modules are gated by `tokio-runtime`.
// =====================================================================
#[cfg(feature = "llm-router")]
pub mod llm;

pub use runtime::{Error, ErrorKind};

// Re-export the seven binding module roots at crate root for
// convenience. Cobrust source-level `import std.X` will project
// onto these paths.
pub use collections::{Dict, List, Set};
pub use iter::{DictIter, Iterator, ListIter, RangeIter, SetIter};

// =====================================================================
// M13 — structured-concurrency runtime (ADR-0028)
// =====================================================================
// Append-only re-exports per the M13 dispatch protocol; M12.x parallel
// edits target a different region of this file (the seven M11 module
// declarations above), so the M13 cut sits at the END to minimize
// merge-time conflict surface.

#[cfg(feature = "tokio-runtime")]
pub mod sync;
#[cfg(feature = "tokio-runtime")]
pub mod task;
