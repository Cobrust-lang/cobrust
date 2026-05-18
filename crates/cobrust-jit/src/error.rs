//! Structured error taxonomy for `cobrust-jit`.
//!
//! Mirrors the shape of `cobrust_codegen::CodegenError` but narrowed
//! to the JIT path's failure modes (no linker, no object emission,
//! finalization is in-process).

use cobrust_mir::BlockId;
use thiserror::Error;

/// Errors emitted by the JIT engine.
///
/// All variants are user-displayable via `Display`. The REPL Session
/// (ADR-0056c) consumes these via the inspector to format friendly
/// diagnostics — never propagated as bare strings.
#[derive(Debug, Error)]
pub enum JitError {
    /// `cranelift-native::builder` failed to detect host ISA. Should
    /// only fire on an exotic host that the cranelift-native author
    /// hasn't covered.
    #[error("failed to detect host ISA: {0}")]
    HostIsaUnavailable(String),

    /// `cranelift-codegen::settings` rejected the configuration.
    /// Programmer error in the engine builder, not user data.
    #[error("cranelift settings error: {0}")]
    Settings(String),

    /// `JITBuilder::with_isa` / `JITModule::new` raised. Wraps the
    /// underlying cranelift-jit error string.
    #[error("JIT module construction failed: {0}")]
    ModuleConstruction(String),

    /// `declare_function` / `define_function` / `finalize_definitions`
    /// returned a `ModuleError`.
    #[error("cranelift module error: {0}")]
    Module(String),

    /// The MIR body referenced a feature this wave-1 JIT path does
    /// not yet support. ADR-0056a §"Decision" pins wave-1 to the
    /// minimal arithmetic subset; everything else surfaces here
    /// for ADR-0056b's broader lowering to claim.
    ///
    /// Carries a free-form description so the REPL Session caller
    /// can route the call to the AOT fallback path (parent §"4
    /// JITModule lifetime" 4-arm signature fallback).
    #[error("MIR feature not yet supported in wave-1 JIT: {feature}")]
    UnsupportedMirFeature { feature: String },

    /// Codegen rejected a type. Carries the offending Cobrust
    /// type description (Ty's Display is opaque to LLM consumers,
    /// so we render it pre-error).
    #[error("unsupported scalar type in wave-1 JIT: {ty}")]
    UnsupportedType { ty: String },

    /// `JitHandle::call` was asked to look up a function name that
    /// the engine never compiled. Always a programmer error in the
    /// caller.
    #[error("no JIT-compiled function named '{name}'")]
    NoSuchFunction { name: String },

    /// The signature the caller transmuted against does NOT match
    /// the compiled body's Cranelift `Signature`. Caught BEFORE
    /// `transmute` in `JitHandle::call` to convert a would-be
    /// SIGSEGV into a typed error.
    ///
    /// Carries both signatures rendered as strings.
    #[error("signature mismatch: caller expected {expected}, JIT compiled {actual}")]
    SignatureMismatch { expected: String, actual: String },

    /// Internal invariant violated. Bug in the JIT crate.
    #[error("JIT internal error: {0}")]
    Internal(String),

    /// Cranelift IR builder rejected the lowering. Carries the
    /// offending MIR block id so the diagnostic points at user-
    /// recognisable source.
    #[error("cranelift IR builder error at block {block_id:?}: {message}")]
    IrBuilder { block_id: BlockId, message: String },
}
