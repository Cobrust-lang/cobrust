//! MIR → Cranelift IR lowering, narrowed to ADR-0056a wave-1.
//!
//! ## Scope
//!
//! Per ADR-0056a §"Decision" the wave-1 JIT path is the minimal
//! arithmetic round-trip: `1 + 2 * 3` evaluated to an `i64`.
//! Concretely this module supports:
//!
//! - Body return type: `i64` (one entry in the 4-arm
//!   `extern "C"` table per parent §4 `JITModule` lifetime
//!   contract).
//! - Body param types: `i64` only.
//! - Statements: `Assign(Place::local, Rvalue::Use|BinaryOp|UnaryOp)`,
//!   `StorageLive` / `StorageDead` / `Nop` (all silently skipped —
//!   no MIR drop schedule in JIT scope).
//! - Operands: `Operand::{Copy(local), Constant(Constant::Int)}`.
//! - BinOps: `Add`, `Sub`, `Mul`.
//! - UnOps: `Neg`.
//! - Terminators: `Return`, `Goto`. **No `SwitchInt` / `Call` /
//!   `Drop` / `Assert`** — those land in ADR-0056b.
//!
//! Any MIR feature outside this surface returns
//! [`JitError::UnsupportedMirFeature`] so the REPL Session can
//! fall back to the AOT one-shot path (ADR-0029 §"Negative").
//!
//! ## Why a separate lowerer
//!
//! ADR-0056a §6 says JIT lowers IDENTICALLY to AOT; the eventual
//! plan in ADR-0056a §3.2 is a `lower_module<M: ClifModule>`
//! helper shared by `cobrust-codegen` and this crate. That
//! refactor lands in ADR-0056b (control-flow + stdlib). For
//! wave-1, isolating the minimal subset here lets us land the
//! crate without disturbing AOT.
//!
//! Reference: `docs/agent/adr/0056a-cranelift-jit-wire.md` §3.2.

use std::collections::HashMap;

use cobrust_mir::{
    BinOp, BlockId, Body, Constant, LocalId, Operand, Place, Rvalue, Statement, StatementKind,
    Terminator, UnOp,
};
use cobrust_types::Ty;

use cranelift_codegen::ir::{self, AbiParam, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};

use crate::error::JitError;

/// Map a Cobrust scalar type to a Cranelift IR type.
///
/// Wave-1 supports only `i64` (with `bool` as a one-off so the
/// 4-arm signature table at parent §4 has at least one
/// `extern "C" fn() -> i64`-shaped friend). Float / pointer / str
/// land later.
pub(crate) fn lower_ty(ty: &Ty) -> Result<ir::Type, JitError> {
    match ty {
        Ty::Int => Ok(ir::types::I64),
        Ty::Bool => Ok(ir::types::I8),
        // Float types — accepted by the 4-arm table at parent §4
        // (() -> f64 row) but no UnOp/BinOp routes for f64 are
        // wired yet. Reject for now so an accidental float param
        // gets a clear error.
        Ty::Float => Err(JitError::UnsupportedType {
            ty: "Float (wave-1 only supports Int)".to_string(),
        }),
        // None as a return type means "return slot only, callable
        // is unit-typed" — accept for the `() -> ()` row.
        Ty::None => Ok(ir::types::INVALID),
        other => Err(JitError::UnsupportedType {
            ty: format!("{other:?}"),
        }),
    }
}

/// Build the `extern "C"` Cranelift `Signature` for a MIR body.
///
/// Wave-1: all params + return must be `Ty::Int`. The MIR
/// convention (lower.rs `BodyBuilder::new`) reserves
/// `locals[0]` as the synthetic `_return` slot of type
/// `Ty::None`; params follow at `locals[1..1 + param_count]`.
/// We respect that convention here.
pub(crate) fn body_signature(body: &Body, call_conv: CallConv) -> Result<Signature, JitError> {
    let mut sig = Signature::new(call_conv);
    let return_local_id = body.return_local;
    let skip = (return_local_id == LocalId(0)) as usize;
    for local in body.locals.iter().skip(skip).take(body.param_count) {
        let ty = lower_ty(&local.ty)?;
        if ty == ir::types::INVALID {
            return Err(JitError::UnsupportedType {
                ty: format!("None-typed param {}", local.name),
            });
        }
        sig.params.push(AbiParam::new(ty));
    }
    // Wave-1 always returns I64; the return local's Ty is None
    // (MIR convention) so we DON'T infer from it. The REPL Session
    // caller validates against the 4-arm extern table.
    // ADR-0056b will widen this to walk the body's writes-to-return-
    // local to derive the actual return type. For wave-1, the dispatch
    // explicitly asks for i64 return tests, so we hardcode the
    // analogue here:
    sig.returns.push(AbiParam::new(ir::types::I64));
    Ok(sig)
}

/// Inferred local type table — wave-1 narrows: every non-return,
/// non-`Ty::None` local must already declare a Cranelift-known
/// scalar (currently `Int`).
fn infer_locals(body: &Body) -> Result<HashMap<LocalId, ir::Type>, JitError> {
    let mut map = HashMap::new();
    for local in &body.locals {
        if local.id == body.return_local && matches!(local.ty, Ty::None) {
            // Return slot: pinned to i64 in wave-1.
            map.insert(local.id, ir::types::I64);
            continue;
        }
        match &local.ty {
            Ty::None => {
                // Synthetic temp without explicit type. The MIR
                // lowering uses these for `_un` / `_bin` temps; in
                // the M9 cranelift backend the chains are walked
                // to infer. For wave-1 we treat them all as I64.
                map.insert(local.id, ir::types::I64);
            }
            other => {
                let ty = lower_ty(other)?;
                if ty == ir::types::INVALID {
                    return Err(JitError::UnsupportedType {
                        ty: format!("INVALID for local {}", local.name),
                    });
                }
                map.insert(local.id, ty);
            }
        }
    }
    Ok(map)
}

/// Lower one MIR `Body` into the Cranelift `Function` already
/// declared in the caller's `JITModule`.
///
/// The caller (engine.rs) is responsible for:
/// - declaring the function in the module (`declare_function`)
/// - building the `Function` shell with the result of
///   `body_signature` above
/// - calling this routine with a fresh `FunctionBuilderContext`
/// - calling `module.define_function(id, &mut Context { func, ... })`
/// - calling `module.finalize_definitions()`
///
/// Returns the populated `Function` on success.
pub(crate) fn lower_body(
    body: &Body,
    call_conv: CallConv,
) -> Result<ir::Function, JitError> {
    let sig = body_signature(body, call_conv)?;
    let mut function = ir::Function::with_name_signature(
        UserFuncName::user(0, body.def_id.0),
        sig,
    );
    let inferred = infer_locals(body)?;

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
                let ty = inferred
                    .get(&local.id)
                    .copied()
                    .unwrap_or(ir::types::I64);
                builder.append_block_param(cl_block, ty);
            }
        }
        block_map.insert(mir_block.id, cl_block);
    }

    // --- declare Cranelift Variables for every local ---
    let mut var_map: HashMap<LocalId, Variable> = HashMap::new();
    for local in &body.locals {
        let ty = inferred
            .get(&local.id)
            .copied()
            .unwrap_or(ir::types::I64);
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
    let param_set: std::collections::HashSet<LocalId> =
        param_local_ids.iter().copied().collect();
    for local in &body.locals {
        if param_set.contains(&local.id) {
            continue;
        }
        let ty = inferred
            .get(&local.id)
            .copied()
            .unwrap_or(ir::types::I64);
        let zero = builder.ins().iconst(ty, 0);
        builder.def_var(var_map[&local.id], zero);
    }

    // --- lower each block's body ---
    for mir_block in &body.blocks {
        let cl_block = block_map[&mir_block.id];
        // If we're already in this block (the entry case), don't
        // re-switch; otherwise switch in.
        if mir_block.id != BlockId(0) {
            builder.switch_to_block(cl_block);
        }

        for stmt in &mir_block.statements {
            lower_statement(&mut builder, &var_map, stmt, mir_block.id)?;
        }
        lower_terminator(
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

fn lower_statement(
    builder: &mut FunctionBuilder<'_>,
    var_map: &HashMap<LocalId, Variable>,
    stmt: &Statement,
    block_id: BlockId,
) -> Result<(), JitError> {
    match &stmt.kind {
        StatementKind::Assign { place, rvalue } => {
            // Wave-1: place must be bare local (no projections).
            if !place.projections.is_empty() {
                return Err(JitError::UnsupportedMirFeature {
                    feature: format!("Place projections at block {block_id:?}"),
                });
            }
            let val = lower_rvalue(builder, var_map, rvalue, block_id)?;
            builder.def_var(var_map[&place.local], val);
            Ok(())
        }
        StatementKind::StorageLive(_)
        | StatementKind::StorageDead(_)
        | StatementKind::Nop => Ok(()),
    }
}

fn lower_rvalue(
    builder: &mut FunctionBuilder<'_>,
    var_map: &HashMap<LocalId, Variable>,
    rvalue: &Rvalue,
    block_id: BlockId,
) -> Result<ir::Value, JitError> {
    match rvalue {
        Rvalue::Use(op) => lower_operand(builder, var_map, op, block_id),
        Rvalue::BinaryOp(op, lhs, rhs) => {
            let l = lower_operand(builder, var_map, lhs, block_id)?;
            let r = lower_operand(builder, var_map, rhs, block_id)?;
            match op {
                BinOp::Add => Ok(builder.ins().iadd(l, r)),
                BinOp::Sub => Ok(builder.ins().isub(l, r)),
                BinOp::Mul => Ok(builder.ins().imul(l, r)),
                other => Err(JitError::UnsupportedMirFeature {
                    feature: format!("BinOp::{other:?} at block {block_id:?}"),
                }),
            }
        }
        Rvalue::UnaryOp(op, val) => {
            let v = lower_operand(builder, var_map, val, block_id)?;
            match op {
                UnOp::Neg => Ok(builder.ins().ineg(v)),
                UnOp::Plus => Ok(v),
                other => Err(JitError::UnsupportedMirFeature {
                    feature: format!("UnOp::{other:?} at block {block_id:?}"),
                }),
            }
        }
        other => Err(JitError::UnsupportedMirFeature {
            feature: format!("Rvalue::{} at block {block_id:?}", rvalue_kind(other)),
        }),
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

fn lower_operand(
    builder: &mut FunctionBuilder<'_>,
    var_map: &HashMap<LocalId, Variable>,
    op: &Operand,
    block_id: BlockId,
) -> Result<ir::Value, JitError> {
    match op {
        Operand::Copy(place) | Operand::Move(place) => {
            lower_place(builder, var_map, place, block_id)
        }
        Operand::Constant(c) => lower_constant(builder, c, block_id),
    }
}

fn lower_place(
    builder: &mut FunctionBuilder<'_>,
    var_map: &HashMap<LocalId, Variable>,
    place: &Place,
    block_id: BlockId,
) -> Result<ir::Value, JitError> {
    if !place.projections.is_empty() {
        return Err(JitError::UnsupportedMirFeature {
            feature: format!("Place projections at block {block_id:?}"),
        });
    }
    let var = var_map.get(&place.local).ok_or_else(|| JitError::Internal(
        format!("place references unknown local {:?}", place.local),
    ))?;
    Ok(builder.use_var(*var))
}

fn lower_constant(
    builder: &mut FunctionBuilder<'_>,
    c: &Constant,
    block_id: BlockId,
) -> Result<ir::Value, JitError> {
    match c {
        Constant::Int(n) => Ok(builder.ins().iconst(ir::types::I64, *n)),
        Constant::Bool(b) => {
            // Bool widens to I8 in wave-1 type lowering, but most
            // arithmetic flows through I64; lift here so binops
            // can compose. Cobrust's M9 codegen does the same.
            Ok(builder.ins().iconst(ir::types::I64, i64::from(*b)))
        }
        other => Err(JitError::UnsupportedMirFeature {
            feature: format!("Constant::{other:?} at block {block_id:?}"),
        }),
    }
}

fn lower_terminator(
    builder: &mut FunctionBuilder<'_>,
    var_map: &HashMap<LocalId, Variable>,
    block_map: &HashMap<BlockId, ir::Block>,
    term: &Terminator,
    block_id: BlockId,
    return_local: LocalId,
) -> Result<(), JitError> {
    match term {
        Terminator::Return => {
            let val = builder.use_var(var_map[&return_local]);
            builder.ins().return_(&[val]);
            Ok(())
        }
        Terminator::Goto(target) => {
            let cl_target = block_map.get(target).ok_or_else(|| JitError::IrBuilder {
                block_id,
                message: format!("Goto target {target:?} not found"),
            })?;
            builder.ins().jump(*cl_target, &[]);
            Ok(())
        }
        Terminator::Unreachable => {
            // TrapCode::user(1) is a constructor; ::user(0) is the
            // reserved sentinel and rejected. 1 is the lowest valid
            // user code and the convention M9 codegen also picks.
            let trap_code = ir::TrapCode::user(1)
                .expect("TrapCode::user(1) is valid (0 is the only reserved sentinel)");
            builder.ins().trap(trap_code);
            Ok(())
        }
        other => Err(JitError::UnsupportedMirFeature {
            feature: format!("Terminator::{} at block {block_id:?}", terminator_kind(other)),
        }),
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
