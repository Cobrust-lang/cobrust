//! Calling convention helpers (per ADR-0023 §"Calling convention details").
//!
//! Cobrust's internal `extern "Cobrust"` ABI matches the host's
//! standard C ABI — System V AMD64 on Linux, AAPCS64 on macOS.
//! This module exposes helpers that map a Cobrust [`Ty`] to its
//! Cranelift / LLVM representation.

use cobrust_types::Ty;
use target_lexicon::{Architecture, OperatingSystem, Triple};

/// Cranelift calling convention for the given target triple.
///
/// Per ADR-0023 §"Calling convention details":
/// - Linux on x86_64 → SystemV
/// - macOS on aarch64 → AppleAarch64
/// - Other Linux / generic → SystemV (best-effort)
#[must_use]
pub fn cranelift_call_conv(triple: &Triple) -> cranelift_codegen::isa::CallConv {
    use cranelift_codegen::isa::CallConv;
    match (triple.architecture, triple.operating_system) {
        (Architecture::Aarch64(_), OperatingSystem::Darwin(_) | OperatingSystem::IOS(_)) => {
            CallConv::AppleAarch64
        }
        (_, OperatingSystem::Windows) => CallConv::WindowsFastcall,
        _ => CallConv::SystemV,
    }
}

/// Map a Cobrust `Ty` to the codegen-time machine type used by
/// the Cranelift backend.
///
/// Returns `None` if the type does not have a direct scalar
/// representation (e.g. owning collections — those are passed
/// by pointer; the caller decides). Tuples + records are
/// always passed by pointer at M9; this helper is for the
/// scalar case only.
#[must_use]
pub fn cranelift_scalar_ty(ty: &Ty) -> Option<cranelift_codegen::ir::Type> {
    use cranelift_codegen::ir::types;
    match ty {
        Ty::Bool => Some(types::I8),
        Ty::Int => Some(types::I64),
        Ty::Float => Some(types::F64),
        Ty::Imag => Some(types::F64), // imaginary stored as a single f64 lane
        Ty::None => Some(types::I8),  // unit-shaped placeholder
        // ADR-0060a — narrow ints map directly to Cranelift's IR widths.
        Ty::IntN(8) => Some(types::I8),
        Ty::IntN(16) => Some(types::I16),
        Ty::IntN(32) => Some(types::I32),
        Ty::IntN(_) => Some(types::I64),
        // Owning + reference + record / tuple / list / dict types are
        // passed by pointer at M9 — return None to signal "indirect".
        _ => None,
    }
}

/// True if a Cobrust `Ty` is "Copy" — fits in a register and can
/// be passed by value without ownership transfer.
#[must_use]
pub fn is_copy_ty(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Bool | Ty::Int | Ty::Float | Ty::Imag | Ty::None | Ty::IntN(_)
    )
}

/// Pointer-width type for the given target. Always `I64` on the
/// M9 delivery scope (x86_64 + aarch64).
#[must_use]
pub fn pointer_ty(triple: &Triple) -> cranelift_codegen::ir::Type {
    use cranelift_codegen::ir::types;
    match triple.architecture.pointer_width() {
        Ok(target_lexicon::PointerWidth::U16) => types::I16,
        Ok(target_lexicon::PointerWidth::U32) => types::I32,
        Ok(target_lexicon::PointerWidth::U64) => types::I64,
        Err(_) => types::I64,
    }
}
