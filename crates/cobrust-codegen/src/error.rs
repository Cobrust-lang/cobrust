//! Codegen error taxonomy. ADR-0023 §"Public surface" pins the
//! variants. Errors are non-exhaustive in spirit (new failure modes
//! can land without breaking match-on-`CodegenError`), but the M9
//! delivery freezes the variant set in the table below.

use thiserror::Error;

use crate::target::Backend;

/// Structured codegen failure modes — every entry maps to a
/// concrete recovery path documented in `docs/agent/modules/codegen.md`.
#[derive(Error, Clone, Debug, PartialEq, Eq)]
pub enum CodegenError {
    /// The selected backend is not compiled into this build.
    /// Mitigation: rebuild `cobrust-codegen` with the matching
    /// cargo feature (`--features llvm`).
    #[error("unsupported backend: {0:?} (rebuild with --features llvm?)")]
    UnsupportedBackend(Backend),

    /// The target triple is not supported by the selected backend.
    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),

    /// The MIR module violates a codegen-time invariant. The
    /// upstream lowering (`cobrust-mir`) should have rejected this;
    /// reaching this branch indicates a bug in the consumer.
    #[error("MIR rejected: {0}")]
    InvalidMir(String),

    /// Cranelift returned an error. The wrapped string is the
    /// `Display` form of the underlying Cranelift error.
    #[error("Cranelift error: {0}")]
    CraneliftError(String),

    /// LLVM (inkwell) returned an error.
    #[error("LLVM error: {0}")]
    LlvmError(String),

    /// `cranelift-object` (or the inkwell object writer) failed to
    /// emit the object file.
    #[error("object emission failed: {0}")]
    ObjectEmission(String),

    /// The system linker (`cc` / `lld`) returned a non-zero exit
    /// code. `stderr` carries the linker's diagnostic text.
    #[error("linker failed (exit {exit_code}): {stderr}")]
    LinkerFailed {
        /// Linker process exit code.
        exit_code: i32,
        /// Captured stderr text (may be lossy if the linker emitted
        /// non-UTF-8 bytes).
        stderr: String,
    },

    /// I/O failure — reading source, writing artifacts, etc.
    #[error("I/O error: {0}")]
    Io(String),

    /// Internal codegen invariant violated. Bug.
    #[error("internal codegen error: {0}")]
    Internal(String),
}

impl From<std::io::Error> for CodegenError {
    fn from(e: std::io::Error) -> Self {
        CodegenError::Io(e.to_string())
    }
}
