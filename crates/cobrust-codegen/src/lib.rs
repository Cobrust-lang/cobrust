//! `cobrust-codegen` — code generation backend.
//!
//! M9 deliverable. ADR-0023 is the authoritative design document
//! and pins:
//!
//! - **Backend feature flags**: Cranelift is the default;
//!   `--features llvm` opts into the inkwell / LLVM 18+ backend
//!   for `--release` opt quality.
//! - **`extern "Cobrust"` ABI**: System V AMD64 on Linux,
//!   AAPCS64 on macOS — the host's standard C ABI.
//! - **Linker delegation**: invoke `cc` (or `lld` via `--features
//!   lld`); never bundle a linker.
//! - **Target matrix**: `x86_64-unknown-linux-gnu` (ELF) +
//!   `aarch64-apple-darwin` (Mach-O) at M9; expansion-friendly
//!   via `target-lexicon` parsing + Cranelift's `isa::lookup`.
//! - **Differential gate**: every "core 30" form's compiled
//!   output produces identical `stdout` to a hand-written Rust
//!   reference program; LLVM `-O3` ≥ 30% smaller binary on a
//!   representative sample.
//!
//! Public surface:
//!
//! - [`emit`] — MIR module → native artifact.
//! - [`TargetSpec`] — triple + opt-level + backend selection.
//! - [`Artifact`] — emitted file.
//! - [`CodegenError`] — structured error taxonomy.
//! - [`Backend`] / [`OptLevel`] / [`ArtifactKind`] — selectors.
//!
//! See `docs/agent/modules/codegen.md` for the agent-facing spec
//! and `docs/agent/adr/0023-m9-codegen.md` for the design.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::result_large_err)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::single_match_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::needless_for_each)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::unnested_or_patterns)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::elidable_lifetime_names)]

pub mod abi;
pub mod artifact;
pub mod cranelift_backend;
pub mod error;
pub mod linker;
/// Module-generic MIR→Cranelift IR lowering substrate (ADR-0058d).
/// Wave-1 surface; see module docs for the binding scope.
pub mod lowering;
pub mod target;

#[cfg(feature = "llvm")]
pub mod llvm_backend;

pub use artifact::{Artifact, ArtifactKind};
pub use error::CodegenError;
pub use target::{Backend, OptLevel, TargetSpec};

/// Top-level entry — MIR module → native artifact.
///
/// Per ADR-0023, the backend is chosen by `spec.backend`:
///
/// - [`Backend::Cranelift`] always works (pure Rust dep tree).
/// - [`Backend::Llvm`] requires `--features llvm`; otherwise
///   returns [`CodegenError::UnsupportedBackend`].
///
/// On success, the returned [`Artifact`] carries the path to the
/// emitted file (object / executable / dynamic library).
///
/// # Errors
///
/// Returns [`CodegenError`] for any failure mode: unsupported
/// backend / target, MIR rejected, Cranelift / LLVM error,
/// object-emission failure, linker failure, I/O error.
pub fn emit(module: &cobrust_mir::Module, spec: TargetSpec) -> Result<Artifact, CodegenError> {
    match spec.backend {
        Backend::Cranelift => cranelift_backend::emit(module, &spec),
        Backend::Llvm => {
            #[cfg(feature = "llvm")]
            {
                llvm_backend::emit(module, &spec)
            }
            #[cfg(not(feature = "llvm"))]
            {
                let _ = module;
                Err(CodegenError::UnsupportedBackend(Backend::Llvm))
            }
        }
    }
}
