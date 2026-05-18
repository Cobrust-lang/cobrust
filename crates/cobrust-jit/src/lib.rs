//! `cobrust-jit` — Cranelift-backed JIT engine for incremental REPL eval.
//!
//! Phase I wave-1 deliverable (ADR-0056a). This crate is the
//! JIT-mode sibling of `cobrust-codegen`'s AOT object-file backend,
//! sharing the underlying Cranelift IR + ISA but emitting native
//! code into the process's address space via `cranelift-jit`'s
//! `JITModule`.
//!
//! ## Public surface (per ADR-0056a)
//!
//! - [`JitEngine`] — wraps `JITBuilder` / `JITModule`; owns the
//!   per-Session JIT page allocator.
//! - [`JitEngine::new`] — lazy-init constructor; cold-start cost
//!   <50ms per ADR-0029 budget (verified at impl time on DG).
//! - [`JitEngine::compile_mir`] — MIR module → native fn pointers,
//!   returning a [`JitHandle`] keyed by the body name.
//! - [`JitHandle::call`] — invoke the JIT-compiled function with
//!   a primitive-typed argument tuple (`ArgsList`) and primitive
//!   return type `R`.
//! - [`JitError`] — structured error taxonomy.
//!
//! ## Scope at wave-1
//!
//! ADR-0056a §"Decision" pins wave-1 to the minimal arithmetic
//! round-trip: `i64`-typed entry points, `BinOp::{Add, Sub, Mul}`,
//! `Constant::Int`, `Operand::{Copy, Constant}`, function params.
//! Control-flow + stdlib intrinsics + dict/list arrive in
//! ADR-0056b. The architecture is set up so 0056b grafts the
//! richer surface without a public-API break.
//!
//! ## §2.5 (LLM-first design) notes
//!
//! The public surface intentionally mirrors `cobrust-codegen`'s
//! shape: `compile_mir` parallels `emit`, `JitHandle` parallels
//! `Artifact`. The LLM training-data prior for "compile + call"
//! holds across both backends.
//!
//! ## Safety
//!
//! `JitHandle::call` is `unsafe` — calling it with a `R` / `A`
//! pair whose `extern "C"` signature does NOT match the
//! Cranelift `Signature` of the compiled body is undefined
//! behavior (SIGSEGV on the lucky path). Callers MUST validate
//! the signature out-of-band; ADR-0056c §"Signature contract"
//! pins the validation surface for the REPL Session caller.
//!
//! Reference: `docs/agent/adr/0056a-cranelift-jit-wire.md`.

// ADR-0056a §"3.3 JIT module construction" needs raw-fn-pointer
// transmute (`get_finalized_function(id) -> *const u8`); this is
// the load-bearing unsafe surface this crate exists to encapsulate.
#![allow(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::similar_names)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::single_match_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::result_large_err)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::missing_const_for_fn)]

pub mod engine;
pub mod error;
pub mod handle;
pub mod lower;

pub use engine::JitEngine;
pub use error::JitError;
pub use handle::{ArgsList, JitHandle};
