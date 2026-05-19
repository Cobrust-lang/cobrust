//! MIR → Cranelift IR lowering — thin wrapper over
//! [`cobrust_codegen::lowering`] (ADR-0058d).
//!
//! ## Phase K Strand #4 (ADR-0058d)
//!
//! Wave-1 lowering lives in `cobrust_codegen::lowering`. This module
//! is a thin adaptation layer: it re-exports the substrate's
//! `lower_body` / `body_signature` under the names cobrust-jit's
//! engine expects, plus a `From<CodegenError> for JitError` bridge.
//!
//! ## Scope (unchanged from ADR-0056a wave-1)
//!
//! Per the substrate at `cobrust_codegen::lowering`:
//!
//! - Body return type: `i64` (one entry in the 4-arm `extern "C"`
//!   table per ADR-0056a §4).
//! - Body param types: `i64` only.
//! - Statements: `Assign(Place::local, Rvalue::Use|BinaryOp|UnaryOp)`,
//!   `StorageLive` / `StorageDead` / `Nop` (no-ops — no MIR drop
//!   schedule in JIT scope).
//! - Operands: `Operand::{Copy(local), Constant(Constant::Int|Bool)}`.
//! - BinOps: `Add`, `Sub`, `Mul`.
//! - UnOps: `Neg`, `Plus`.
//! - Terminators: `Return`, `Goto`, `Unreachable`. **No `SwitchInt` /
//!   `Call` / `Drop` / `Assert`** — those land in ADR-0056b.
//!
//! Any MIR feature outside this surface returns
//! [`JitError::UnsupportedMirFeature`] so the REPL Session can fall
//! back to the AOT one-shot path (ADR-0029 §"Negative").
//!
//! Reference: `docs/agent/adr/0058d-jit-aot-lowering-convergence.md`.

use cobrust_codegen::error::CodegenError;
use cobrust_codegen::lowering;
use cobrust_mir::Body;
use cranelift_codegen::ir;
use cranelift_codegen::ir::Signature;
use cranelift_codegen::isa::CallConv;

use crate::error::JitError;

/// Build the `extern "C"` Cranelift [`Signature`] for a MIR body
/// under the wave-1 surface.
///
/// Thin wrapper over [`lowering::body_signature_wave1`]; converts
/// `CodegenError` → `JitError` via the `From` bridge below.
pub(crate) fn body_signature(body: &Body, call_conv: CallConv) -> Result<Signature, JitError> {
    lowering::body_signature_wave1(body, call_conv).map_err(JitError::from)
}

/// Lower one MIR [`Body`] into the Cranelift `Function`.
///
/// Thin wrapper over [`lowering::lower_body_wave1`]. The caller
/// (engine.rs) is responsible for:
/// - declaring the function in the module (`declare_function`)
/// - building the `Function` shell with the result of
///   [`body_signature`]
/// - calling this routine
/// - calling `module.define_function(id, &mut Context { func, ... })`
/// - calling `module.finalize_definitions()`
///
/// Returns the populated `Function` on success.
pub(crate) fn lower_body(body: &Body, call_conv: CallConv) -> Result<ir::Function, JitError> {
    lowering::lower_body_wave1(body, call_conv).map_err(JitError::from)
}

/// Bridge `cobrust_codegen::CodegenError` → `JitError`.
///
/// The wave-1 substrate emits `CodegenError::InvalidMir` with a
/// `wave1:` prefix for any unsupported MIR shape, and
/// `CodegenError::Internal` for invariant violations. We narrow
/// these to the most-informative `JitError` variant:
///
/// - `InvalidMir("wave1: unsupported type ...")` → `UnsupportedType`
/// - `InvalidMir("wave1: None-typed param ...")` → `UnsupportedType`
/// - `InvalidMir("wave1: ...")` → `UnsupportedMirFeature`
/// - `Internal(msg)` → `Internal(msg)`
/// - other → `Internal(msg)` (defensive — wave-1 substrate should
///   only emit InvalidMir / Internal)
impl From<CodegenError> for JitError {
    fn from(e: CodegenError) -> Self {
        match e {
            CodegenError::InvalidMir(msg) => {
                if msg.contains("unsupported type")
                    || msg.contains("None-typed param")
                    || msg.contains("INVALID type")
                {
                    JitError::UnsupportedType { ty: msg }
                } else {
                    JitError::UnsupportedMirFeature { feature: msg }
                }
            }
            CodegenError::Internal(msg) => JitError::Internal(msg),
            other => JitError::Internal(format!("unexpected CodegenError variant: {other}")),
        }
    }
}
