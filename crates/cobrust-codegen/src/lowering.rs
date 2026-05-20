//! Module-generic MIR→Cranelift IR lowering substrate (ADR-0058d).
//!
//! Phase K Strand #4 deliverable. Extracted from
//! `cranelift_backend.rs` to provide a single source of truth for
//! the wave-1 lowering shape consumed by both:
//!
//! - `cobrust-codegen::cranelift_backend` (AOT path, via
//!   `CraneliftCtx::define_body`'s stateful dispatcher — does NOT
//!   call into this module today; the wave-1 helpers exist primarily
//!   to anchor a public contract).
//! - `cobrust-jit::lower` (JIT path, thin wrapper consumer).
//!
//! ## Wave-1 surface (binding per ADR-0058d §2.1)
//!
//! - **Types**: `Ty::Int` → `i64`, `Ty::Bool` → `i8`/widened-to-i64,
//!   `Ty::None` → `i64` return slot.
//! - **Constants**: `Constant::Int`, `Constant::Bool`.
//! - **BinOps**: `Add`, `Sub`, `Mul`.
//! - **UnOps**: `Neg`, `Plus`.
//! - **Place**: `Place::local` only (no projections).
//! - **Operands**: `Copy`, `Move`, `Constant`.
//! - **Statements**: `Assign`, `StorageLive`/`StorageDead`/`Nop`
//!   (no-ops).
//! - **Terminators**: `Return`, `Goto`, `Unreachable`.
//!
//! Any MIR shape outside the wave-1 surface returns
//! [`CodegenError::InvalidMir`] with a `wave1:` prefix so callers
//! can distinguish wave-1-unsupported from genuine MIR invariant
//! violations.
//!
//! ## Public stability
//!
//! Per ADR-0058d §5.1, the wave-1 surface is **stable-for-wave-1**:
//! any signature change requires a sub-ADR. Additive helpers
//! (e.g. a future `lower_constant_float`) are non-breaking.
//!
//! Reference: `docs/agent/adr/0058d-jit-aot-lowering-convergence.md`.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use cobrust_mir::{
    BinOp, BlockId, Body, Constant, LocalId, Operand, Place, Rvalue, Statement, StatementKind,
    Terminator, UnOp,
};
use cobrust_types::Ty;

use cranelift_codegen::ir::{self, AbiParam, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};

use crate::error::CodegenError;

/// Map a Cobrust scalar type to a Cranelift IR type, narrowed to
/// the wave-1 surface.
///
/// Wave-1 supports only `i64` (and `bool` lifted to `i64` in the
/// constant lowering — see [`lower_constant`]). Float / pointer /
/// str return `CodegenError::InvalidMir` with a `wave1:` prefix.
///
/// # Errors
///
/// Returns [`CodegenError::InvalidMir`] for any non-wave-1 scalar
/// type (Float, Imag, Str, etc.).
pub fn lower_ty_wave1(ty: &Ty) -> Result<ir::Type, CodegenError> {
    match ty {
        Ty::Int => Ok(ir::types::I64),
        Ty::Bool => Ok(ir::types::I8),
        Ty::None => Ok(ir::types::INVALID),
        // ADR-0060a — narrow ints map directly to Cranelift's IR widths.
        // The codegen cast surface (CastKind::IntNarrow) inserts
        // `ireduce` / `sextend` ops to bridge `Ty::Int <-> Ty::IntN(w)`.
        Ty::IntN(8) => Ok(ir::types::I8),
        Ty::IntN(16) => Ok(ir::types::I16),
        Ty::IntN(32) => Ok(ir::types::I32),
        Ty::IntN(_) => Ok(ir::types::I64),
        other => Err(CodegenError::InvalidMir(format!(
            "wave1: unsupported type {other:?} (only Int / IntN / Bool / None)"
        ))),
    }
}

/// Build the `extern "C"` Cranelift [`Signature`] for a MIR body
/// under the wave-1 surface.
///
/// The MIR convention (cobrust-mir lower.rs `BodyBuilder::new`)
/// reserves `locals[0]` as the synthetic `_return` slot of type
/// `Ty::None`; params follow at `locals[1..1 + param_count]`. The
/// wave-1 contract pins the return type to `i64` (the REPL Session
/// at the caller side validates against the 4-arm extern table per
/// ADR-0056a §4).
///
/// # Errors
///
/// Returns [`CodegenError::InvalidMir`] if any param has a non-
/// wave-1 type (Float, Str, etc.).
pub fn body_signature_wave1(body: &Body, call_conv: CallConv) -> Result<Signature, CodegenError> {
    let mut sig = Signature::new(call_conv);
    let return_local_id = body.return_local;
    let skip = (return_local_id == LocalId(0)) as usize;
    for local in body.locals.iter().skip(skip).take(body.param_count) {
        let ty = lower_ty_wave1(&local.ty)?;
        if ty == ir::types::INVALID {
            return Err(CodegenError::InvalidMir(format!(
                "wave1: None-typed param {}",
                local.name
            )));
        }
        sig.params.push(AbiParam::new(ty));
    }
    // Wave-1 always returns I64; the return local's Ty is None
    // (MIR convention) so we DON'T infer from it. The caller validates
    // against the 4-arm extern table.
    sig.returns.push(AbiParam::new(ir::types::I64));
    Ok(sig)
}

/// Inferred local type table — wave-1 narrows: every non-return,
/// non-`Ty::None` local must already declare a Cranelift-known
/// scalar (currently `Int`). Synthetic `Ty::None` temps are
/// treated as `i64` (matches the cobrust-codegen AOT path
/// convention for `_un` / `_bin` temps).
fn infer_locals_wave1(body: &Body) -> Result<HashMap<LocalId, ir::Type>, CodegenError> {
    let mut map = HashMap::new();
    for local in &body.locals {
        if local.id == body.return_local && matches!(local.ty, Ty::None) {
            map.insert(local.id, ir::types::I64);
            continue;
        }
        match &local.ty {
            Ty::None => {
                map.insert(local.id, ir::types::I64);
            }
            other => {
                let ty = lower_ty_wave1(other)?;
                if ty == ir::types::INVALID {
                    return Err(CodegenError::InvalidMir(format!(
                        "wave1: INVALID type for local {}",
                        local.name
                    )));
                }
                map.insert(local.id, ty);
            }
        }
    }
    Ok(map)
}

/// Lower a single MIR [`Constant`] under the wave-1 surface.
///
/// Wave-1 supports `Int` and `Bool` (`Bool` is widened to `i64` so
/// downstream `BinOp` arithmetic composes). Float / Str / FnRef /
/// other variants return [`CodegenError::InvalidMir`] with a
/// `wave1:` prefix so JIT callers can distinguish unsupported-by-
/// wave-1 from a genuine bug.
///
/// # Errors
///
/// Returns [`CodegenError::InvalidMir`] for non-wave-1 constant
/// variants.
pub fn lower_constant(
    builder: &mut FunctionBuilder<'_>,
    c: &Constant,
    block_id: BlockId,
) -> Result<ir::Value, CodegenError> {
    match c {
        Constant::Int(n) => Ok(builder.ins().iconst(ir::types::I64, *n)),
        Constant::Bool(b) => {
            // Bool widens to I8 in wave-1 type lowering, but most
            // arithmetic flows through I64; lift here so binops can
            // compose. Mirrors the cobrust-codegen AOT path.
            Ok(builder.ins().iconst(ir::types::I64, i64::from(*b)))
        }
        other => Err(CodegenError::InvalidMir(format!(
            "wave1: Constant::{other:?} at block {block_id:?}"
        ))),
    }
}

/// Lower a [`Place`] read under the wave-1 surface.
///
/// Wave-1 supports bare local reads only — any projection (field
/// access, indexing, deref) is rejected.
///
/// # Errors
///
/// Returns [`CodegenError::InvalidMir`] if the place carries any
/// projection, or [`CodegenError::Internal`] if the local is not
/// in the var_map.
pub fn lower_place<S: BuildHasher>(
    builder: &mut FunctionBuilder<'_>,
    var_map: &HashMap<LocalId, Variable, S>,
    place: &Place,
    block_id: BlockId,
) -> Result<ir::Value, CodegenError> {
    if !place.projections.is_empty() {
        return Err(CodegenError::InvalidMir(format!(
            "wave1: Place projections at block {block_id:?}"
        )));
    }
    let var = var_map.get(&place.local).ok_or_else(|| {
        CodegenError::Internal(format!(
            "wave1: place references unknown local {:?}",
            place.local
        ))
    })?;
    Ok(builder.use_var(*var))
}

/// Lower an [`Operand`] under the wave-1 surface.
///
/// # Errors
///
/// Propagates from [`lower_place`] / [`lower_constant`].
pub fn lower_operand<S: BuildHasher>(
    builder: &mut FunctionBuilder<'_>,
    var_map: &HashMap<LocalId, Variable, S>,
    op: &Operand,
    block_id: BlockId,
) -> Result<ir::Value, CodegenError> {
    match op {
        Operand::Copy(place) | Operand::Move(place) => {
            lower_place(builder, var_map, place, block_id)
        }
        Operand::Constant(c) => lower_constant(builder, c, block_id),
    }
}

/// Lower an [`Rvalue`] under the wave-1 surface.
///
/// Supported: `Use`, `BinaryOp::{Add, Sub, Mul}`, `UnaryOp::{Neg,
/// Plus}`. All other variants return `CodegenError::InvalidMir`.
///
/// # Errors
///
/// Returns [`CodegenError::InvalidMir`] for non-wave-1 rvalue
/// shapes.
pub fn lower_rvalue_wave1<S: BuildHasher>(
    builder: &mut FunctionBuilder<'_>,
    var_map: &HashMap<LocalId, Variable, S>,
    rvalue: &Rvalue,
    block_id: BlockId,
) -> Result<ir::Value, CodegenError> {
    match rvalue {
        Rvalue::Use(op) => lower_operand(builder, var_map, op, block_id),
        Rvalue::BinaryOp(op, lhs, rhs) => {
            let l = lower_operand(builder, var_map, lhs, block_id)?;
            let r = lower_operand(builder, var_map, rhs, block_id)?;
            match op {
                BinOp::Add => Ok(builder.ins().iadd(l, r)),
                BinOp::Sub => Ok(builder.ins().isub(l, r)),
                BinOp::Mul => Ok(builder.ins().imul(l, r)),
                other => Err(CodegenError::InvalidMir(format!(
                    "wave1: BinOp::{other:?} at block {block_id:?}"
                ))),
            }
        }
        Rvalue::UnaryOp(op, val) => {
            let v = lower_operand(builder, var_map, val, block_id)?;
            match op {
                UnOp::Neg => Ok(builder.ins().ineg(v)),
                UnOp::Plus => Ok(v),
                other => Err(CodegenError::InvalidMir(format!(
                    "wave1: UnOp::{other:?} at block {block_id:?}"
                ))),
            }
        }
        other => Err(CodegenError::InvalidMir(format!(
            "wave1: Rvalue::{} at block {block_id:?}",
            rvalue_kind(other)
        ))),
    }
}

fn rvalue_kind(r: &Rvalue) -> &'static str {
    match r {
        Rvalue::Use(_) => "Use",
        Rvalue::BinaryOp(..) => "BinaryOp",
        Rvalue::UnaryOp(..) => "UnaryOp",
        Rvalue::Aggregate(..) => "Aggregate",
        Rvalue::Cast(..) => "Cast",
        Rvalue::Ref(..) => "Ref",
        Rvalue::Discriminant(_) => "Discriminant",
        Rvalue::Len(_) => "Len",
        Rvalue::NullaryOp(_) => "NullaryOp",
    }
}

/// Lower a single MIR [`Statement`] under the wave-1 surface.
///
/// Supported: `Assign { place: Place::local, rvalue }`,
/// `StorageLive`, `StorageDead`, `Nop` (last three are no-ops).
///
/// # Errors
///
/// Returns [`CodegenError::InvalidMir`] if the statement is
/// `Assign` with a projected place, or if the rvalue is non-wave-1.
pub fn lower_statement_wave1<S: BuildHasher>(
    builder: &mut FunctionBuilder<'_>,
    var_map: &HashMap<LocalId, Variable, S>,
    stmt: &Statement,
    block_id: BlockId,
) -> Result<(), CodegenError> {
    match &stmt.kind {
        StatementKind::Assign { place, rvalue } => {
            if !place.projections.is_empty() {
                return Err(CodegenError::InvalidMir(format!(
                    "wave1: Place projections at block {block_id:?}"
                )));
            }
            let val = lower_rvalue_wave1(builder, var_map, rvalue, block_id)?;
            let var = var_map.get(&place.local).ok_or_else(|| {
                CodegenError::Internal(format!(
                    "wave1: assign target unknown local {:?}",
                    place.local
                ))
            })?;
            builder.def_var(*var, val);
            Ok(())
        }
        StatementKind::StorageLive(_) | StatementKind::StorageDead(_) | StatementKind::Nop => {
            Ok(())
        }
    }
}

/// Lower a [`Terminator`] under the wave-1 surface.
///
/// Supported: `Return`, `Goto`, `Unreachable`. `SwitchInt` / `Call` /
/// `Drop` / `Assert` return [`CodegenError::InvalidMir`].
///
/// # Errors
///
/// Returns [`CodegenError::InvalidMir`] for non-wave-1 terminators
/// or invalid Goto target.
pub fn lower_terminator_wave1<S1, S2>(
    builder: &mut FunctionBuilder<'_>,
    var_map: &HashMap<LocalId, Variable, S1>,
    block_map: &HashMap<BlockId, ir::Block, S2>,
    term: &Terminator,
    block_id: BlockId,
    return_local: LocalId,
) -> Result<(), CodegenError>
where
    S1: BuildHasher,
    S2: BuildHasher,
{
    match term {
        Terminator::Return => {
            let var = var_map.get(&return_local).ok_or_else(|| {
                CodegenError::Internal(format!(
                    "wave1: return references unknown local {return_local:?}"
                ))
            })?;
            let val = builder.use_var(*var);
            builder.ins().return_(&[val]);
            Ok(())
        }
        Terminator::Goto(target) => {
            let cl_target = block_map.get(target).ok_or_else(|| {
                CodegenError::InvalidMir(format!(
                    "wave1: Goto target {target:?} not found at block {block_id:?}"
                ))
            })?;
            builder.ins().jump(*cl_target, &[]);
            Ok(())
        }
        Terminator::Unreachable => {
            // TrapCode::user(1) is the lowest valid user trap code;
            // 0 is the reserved sentinel. Mirrors cobrust-codegen's
            // convention.
            let trap_code = ir::TrapCode::user(1)
                .expect("TrapCode::user(1) is valid (0 is the only reserved sentinel)");
            builder.ins().trap(trap_code);
            Ok(())
        }
        other => Err(CodegenError::InvalidMir(format!(
            "wave1: Terminator::{} at block {block_id:?}",
            terminator_kind(other)
        ))),
    }
}

fn terminator_kind(t: &Terminator) -> &'static str {
    match t {
        Terminator::Goto(_) => "Goto",
        Terminator::SwitchInt { .. } => "SwitchInt",
        Terminator::Return => "Return",
        Terminator::Call { .. } => "Call",
        Terminator::Drop { .. } => "Drop",
        Terminator::Unreachable => "Unreachable",
        Terminator::Assert { .. } => "Assert",
    }
}

/// Lower an entire MIR [`Body`] into a fresh Cranelift
/// [`ir::Function`] under the wave-1 surface.
///
/// The caller is responsible for:
/// - declaring the function in their module (`declare_function`)
/// - building the `Function` shell with the result of
///   [`body_signature_wave1`]
/// - calling this routine
/// - calling `module.define_function(id, &mut Context { func, ... })`
/// - calling `module.finalize_definitions()`
///
/// Returns the populated `Function` on success.
///
/// # Errors
///
/// Propagates from any sub-lowering helper.
pub fn lower_body_wave1(body: &Body, call_conv: CallConv) -> Result<ir::Function, CodegenError> {
    let sig = body_signature_wave1(body, call_conv)?;
    let mut function = ir::Function::with_name_signature(UserFuncName::user(0, body.def_id.0), sig);
    let inferred = infer_locals_wave1(body)?;

    let mut builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut builder_ctx);

    // --- Cranelift blocks one-per-MIR-block ---
    let mut block_map: HashMap<BlockId, ir::Block> = HashMap::new();
    for (idx, mir_block) in body.blocks.iter().enumerate() {
        let cl_block = builder.create_block();
        if idx == 0 {
            // Entry block: append params matching the signature.
            let skip = (body.return_local == LocalId(0)) as usize;
            for local in body.locals.iter().skip(skip).take(body.param_count) {
                let ty = inferred.get(&local.id).copied().unwrap_or(ir::types::I64);
                builder.append_block_param(cl_block, ty);
            }
        }
        block_map.insert(mir_block.id, cl_block);
    }

    // --- declare Cranelift Variables for every local ---
    let mut var_map: HashMap<LocalId, Variable> = HashMap::new();
    for local in &body.locals {
        let ty = inferred.get(&local.id).copied().unwrap_or(ir::types::I64);
        let var = builder.declare_var(ty);
        var_map.insert(local.id, var);
    }

    // --- enter the entry block + bind params ---
    let entry = block_map[&BlockId(0)];
    builder.switch_to_block(entry);

    let skip = (body.return_local == LocalId(0)) as usize;
    let param_local_ids: Vec<LocalId> = body
        .locals
        .iter()
        .skip(skip)
        .take(body.param_count)
        .map(|l| l.id)
        .collect();
    for (idx, lid) in param_local_ids.iter().enumerate() {
        let val = builder.block_params(entry)[idx];
        let var = var_map[lid];
        builder.def_var(var, val);
    }

    // --- pre-initialize non-param locals to 0 (matches cobrust-codegen
    //     pattern: guarantees `use_var` is always well-defined on every
    //     path; otherwise Cranelift's verifier rejects). ---
    let param_set: HashSet<LocalId> = param_local_ids.iter().copied().collect();
    for local in &body.locals {
        if param_set.contains(&local.id) {
            continue;
        }
        let ty = inferred.get(&local.id).copied().unwrap_or(ir::types::I64);
        let zero = builder.ins().iconst(ty, 0);
        builder.def_var(var_map[&local.id], zero);
    }

    // --- lower each block's body ---
    for mir_block in &body.blocks {
        let cl_block = block_map[&mir_block.id];
        if mir_block.id != BlockId(0) {
            builder.switch_to_block(cl_block);
        }

        for stmt in &mir_block.statements {
            lower_statement_wave1(&mut builder, &var_map, stmt, mir_block.id)?;
        }
        lower_terminator_wave1(
            &mut builder,
            &var_map,
            &block_map,
            &mir_block.terminator,
            mir_block.id,
            body.return_local,
        )?;
    }

    builder.seal_all_blocks();
    builder.finalize();

    Ok(function)
}

#[cfg(test)]
mod tests {
    //! Direct unit tests for the wave-1 lowering substrate
    //! (ADR-0058d §5.3 risk-3 mitigation: prevent drift after
    //! extraction).

    use super::*;
    use cobrust_frontend::span::{FileId, Span};
    use cobrust_hir::DefId;
    use cobrust_mir::{BasicBlock, LocalDecl};
    use cranelift_codegen::isa::CallConv;

    const SPAN: Span = Span {
        file: FileId::SYNTHETIC,
        start: 0,
        end: 0,
    };

    fn mk_local(id: u32, name: &str, ty: Ty, mutable: bool) -> LocalDecl {
        LocalDecl {
            id: LocalId(id),
            name: name.to_string(),
            ty,
            mutable,
            span: SPAN,
        }
    }

    /// Round-trip `1 + 2` through `lower_body_wave1`.
    #[test]
    fn lower_body_wave1_int_add_round_trip() {
        let return_local = LocalId(0);
        let bin_tmp = LocalId(1);
        let locals = vec![
            mk_local(0, "_return", Ty::None, true),
            mk_local(1, "_bin", Ty::None, false),
        ];

        let statements = vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(bin_tmp),
                    rvalue: Rvalue::BinaryOp(
                        BinOp::Add,
                        Operand::Constant(Constant::Int(1)),
                        Operand::Constant(Constant::Int(2)),
                    ),
                },
                span: SPAN,
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(return_local),
                    rvalue: Rvalue::Use(Operand::Copy(Place::local(bin_tmp))),
                },
                span: SPAN,
            },
        ];

        let body = Body {
            def_id: DefId(1),
            name: "add12".to_string(),
            locals,
            blocks: vec![BasicBlock {
                id: BlockId(0),
                statements,
                terminator: Terminator::Return,
                span: SPAN,
            }],
            return_local,
            param_count: 0,
            span: SPAN,
        };

        let function = lower_body_wave1(&body, CallConv::SystemV).expect("lower ok");
        assert_eq!(function.signature.returns.len(), 1);
        assert_eq!(function.signature.params.len(), 0);
    }

    /// `Constant::Str(_)` is wave-1-unsupported → `InvalidMir`.
    #[test]
    fn lower_constant_str_rejected() {
        // Minimal Function shell to drive lower_constant.
        let sig = Signature::new(CallConv::SystemV);
        let mut function = ir::Function::with_name_signature(UserFuncName::user(0, 0), sig);
        let mut fbctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut function, &mut fbctx);
        let blk = builder.create_block();
        builder.switch_to_block(blk);

        let result = lower_constant(&mut builder, &Constant::Str("hello".into()), BlockId(0));
        match result {
            Err(CodegenError::InvalidMir(msg)) => {
                assert!(
                    msg.starts_with("wave1:"),
                    "expected wave1: prefix; got: {msg}"
                );
            }
            other => panic!("expected InvalidMir; got {other:?}"),
        }
    }
}
