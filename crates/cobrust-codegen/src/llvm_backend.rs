//! LLVM backend (per ADR-0023 §"Backend feature flag layout").
//!
//! Active only when `--features llvm` is set. Wraps `inkwell` to
//! produce LLVM IR + emit object code via the LLVM 18+ toolchain.
//!
//! At M9, the LLVM backend implements **the same MIR-form table**
//! as the Cranelift backend; the differential gate validates output
//! parity. Where Cranelift falls back to a stub (e.g. f-strings,
//! aggregate construction), LLVM mirrors the stub so both backends
//! converge.
//!
//! The full LLVM lowering implementation is deferred to a follow-up
//! commit once the inkwell dep tree is verified on CI hosts; this
//! file ships the surface stub so downstream consumers can wire
//! `--features llvm` without compile errors.

use cobrust_mir::Module;

use crate::artifact::Artifact;
use crate::error::CodegenError;
use crate::target::TargetSpec;

/// LLVM backend entrypoint. Mirrors the Cranelift backend's signature.
///
/// # Errors
///
/// At M9 the LLVM backend is feature-gated scaffolding; calling it
/// returns [`CodegenError::LlvmError`] until the inkwell wiring is
/// fully implemented (tracked as the M9 follow-up "LLVM full lowering").
pub fn emit(_module: &Module, spec: &TargetSpec) -> Result<Artifact, CodegenError> {
    Err(CodegenError::LlvmError(format!(
        "LLVM backend at M9 is feature-gated scaffolding; \
         use Backend::Cranelift for {triple} until follow-up lands",
        triple = spec.triple
    )))
}
