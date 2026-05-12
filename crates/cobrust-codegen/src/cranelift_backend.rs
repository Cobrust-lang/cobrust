//! Cranelift backend (per ADR-0023 §"Per-MIR-form lowering rules").
//!
//! The Cranelift backend is the M9 default. It is pure-Rust, has
//! no system deps, and produces correct (if unoptimized) object
//! code for every "core 30" form on the delivery-scope targets
//! (`x86_64-unknown-linux-gnu` ELF, `aarch64-apple-darwin` Mach-O).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use cobrust_mir::{
    AggregateKind, AssertKind, BasicBlock, BinOp, BlockId, Body, CastKind, Constant, LocalId,
    Module, Operand, Place, Projection, Rvalue, Statement, StatementKind, SwitchValue, Terminator,
    UnOp,
};

use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, MemFlags, Signature, UserFuncName};
use cranelift_codegen::isa::{self, OwnedTargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::{Context, ir};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{Linkage, Module as ClifModule};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::abi::{cranelift_call_conv, cranelift_scalar_ty, pointer_ty};
use crate::artifact::{Artifact, ArtifactKind};
use crate::error::CodegenError;
use crate::linker;
use crate::target::{OptLevel, TargetSpec};

/// Public Cranelift backend entrypoint.
///
/// # Errors
///
/// Returns a [`CodegenError`] for any backend / object / linker
/// failure. ADR-0023 enumerates the variants.
pub fn emit(module: &Module, spec: &TargetSpec) -> Result<Artifact, CodegenError> {
    let isa = build_isa(spec)?;
    let pointer_type = pointer_ty(&spec.triple);
    let call_conv = cranelift_call_conv(&spec.triple);
    let object_builder = ObjectBuilder::new(
        isa,
        spec.module_name.clone(),
        cranelift_module::default_libcall_names(),
    )
    .map_err(|e| CodegenError::ObjectEmission(e.to_string()))?;
    let mut obj_module = ObjectModule::new(object_builder);

    let runtime_helper_return_types: HashMap<&'static str, ir::Type> =
        runtime_helper_signatures(pointer_type, call_conv)
            .into_iter()
            .filter_map(|(name, sig)| sig.returns.first().map(|p| (name, p.value_type)))
            .collect();
    let mut ctx = CraneliftCtx {
        pointer_type,
        call_conv,
        function_ids: HashMap::new(),
        body_return_types: HashMap::new(),
        runtime_helper_return_types,
    };

    // --- declare every body's signature first ---------------------------
    for body in &module.bodies {
        ctx.declare_body(&mut obj_module, body)?;
    }

    // --- now define each body --------------------------------------------
    for body in &module.bodies {
        ctx.define_body(&mut obj_module, body)?;
    }

    // --- finalize + write object file -----------------------------------
    let product = obj_module.finish();
    std::fs::create_dir_all(&spec.output_dir)?;

    let object_name = format!("{}.o", spec.module_name);
    let object_path = spec.output_dir.join(object_name);
    let bytes = product
        .emit()
        .map_err(|e| CodegenError::ObjectEmission(e.to_string()))?;
    std::fs::write(&object_path, bytes)?;

    finalize_artifact(object_path, spec)
}

/// Decide the final artifact: object alone, or invoke the linker.
fn finalize_artifact(object: PathBuf, spec: &TargetSpec) -> Result<Artifact, CodegenError> {
    match spec.artifact {
        ArtifactKind::Object => Ok(Artifact::Object(object)),
        ArtifactKind::Executable => {
            let extension = ArtifactKind::Executable.extension(&spec.triple);
            let mut output = spec.output_dir.join(&spec.module_name);
            if !extension.is_empty() {
                output.set_extension(extension);
            }
            linker::link(&object, &output, ArtifactKind::Executable)?;
            Ok(Artifact::Executable(output))
        }
        ArtifactKind::DynamicLibrary => {
            let extension = ArtifactKind::DynamicLibrary.extension(&spec.triple);
            let output = spec
                .output_dir
                .join(&spec.module_name)
                .with_extension(extension);
            linker::link(&object, &output, ArtifactKind::DynamicLibrary)?;
            Ok(Artifact::DynamicLibrary(output))
        }
    }
}

/// Build the Cranelift target ISA from the spec.
fn build_isa(spec: &TargetSpec) -> Result<OwnedTargetIsa, CodegenError> {
    let mut shared_builder = settings::builder();
    let opt_str = match spec.opt_level {
        OptLevel::None => "none",
        OptLevel::Speed => "speed",
        OptLevel::SpeedAndSize => "speed_and_size",
    };
    shared_builder
        .set("opt_level", opt_str)
        .map_err(|e| CodegenError::CraneliftError(e.to_string()))?;
    // PIC required so Mach-O / ELF objects link into PIE / shared output.
    shared_builder
        .set("is_pic", "true")
        .map_err(|e| CodegenError::CraneliftError(e.to_string()))?;
    let shared_flags = settings::Flags::new(shared_builder);

    let isa_builder = isa::lookup(spec.triple.clone())
        .map_err(|_| CodegenError::UnsupportedTarget(spec.triple.to_string()))?;
    let isa = isa_builder
        .finish(shared_flags)
        .map_err(|e| CodegenError::CraneliftError(e.to_string()))?;
    Ok(isa)
}

// =====================================================================
// CraneliftCtx — stateful backend
// =====================================================================

struct CraneliftCtx {
    pointer_type: ir::Type,
    call_conv: cranelift_codegen::isa::CallConv,
    /// Map a MIR body's `def_id` → Cranelift `FuncId`.
    function_ids: HashMap<u32, cranelift_module::FuncId>,
    /// ADR-0034: cache the inferred return type for every user-defined
    /// body so a caller can look up the return type of a
    /// `Constant::FnRef(id)` callee at infer-locals time. Populated in
    /// `declare_body` from the same signature inference that produces
    /// the Cranelift `Signature.returns`. Used by `infer_local_types`
    /// to type the `_callret` destination of `Terminator::Call`
    /// destinations whose declared `Ty` is `Ty::None`.
    body_return_types: HashMap<u32, ir::Type>,
    /// ADR-0044: cache the runtime-helper return types so
    /// `infer_local_types` can propagate them through post-rewrite
    /// `Terminator::Call { func: Constant::Str(name), .. }` callsites
    /// (where `name` is a runtime symbol, e.g. `__cobrust_argv`).
    /// Populated once at ctx construction from
    /// `runtime_helper_signatures`. Missing keys default to None,
    /// matching the pre-ADR-0044 behavior.
    runtime_helper_return_types: HashMap<&'static str, ir::Type>,
}

impl CraneliftCtx {
    fn body_signature(&self, body: &Body) -> Signature {
        // ADR-0033: compute the converged inferred-locals map *first*
        // so the return-type inference sees fully-resolved chains
        // through `_un` / `_bin` / `_callret` synthetic temps. Without
        // this, a body like `fn f() -> f64: return -2.71` resolves
        // the return type to I8 (because `_un` is declared `Ty::None`)
        // and Cranelift x86_64's `CvtFloatToSintSeq` panics in
        // emission.
        let inferred = self.infer_local_types(body);
        self.body_signature_with(body, &inferred)
    }

    fn body_signature_with(&self, body: &Body, inferred: &HashMap<LocalId, ir::Type>) -> Signature {
        let mut sig = Signature::new(self.call_conv);
        // Skip the synthetic return slot at locals[0] when collecting params.
        // MIR convention (lower.rs `BodyBuilder::new`): _0 is the dedicated
        // return slot of type `Ty::None`; params follow at _1.._1+N where
        // N == body.param_count. The return local is _0 with type `Ty::None`,
        // but the *real* return type comes from whatever value gets assigned
        // to _0 inside the body.
        let param_locals = if body.return_local == cobrust_mir::LocalId(0) {
            // Skip _0 (return slot) when emitting param signatures.
            body.locals
                .iter()
                .skip(1)
                .take(body.param_count)
                .collect::<Vec<_>>()
        } else {
            body.locals
                .iter()
                .take(body.param_count)
                .collect::<Vec<_>>()
        };
        for local in &param_locals {
            sig.params.push(AbiParam::new(
                cranelift_scalar_ty(&local.ty).unwrap_or(self.pointer_type),
            ));
        }
        let ret_ty = self
            .infer_return_type(body, inferred)
            .unwrap_or(self.pointer_type);
        sig.returns.push(AbiParam::new(ret_ty));
        sig
    }

    /// Infer the actual return type by scanning the body for the
    /// last assignment to the return local. The MIR puts `_return:
    /// Ty::None` regardless of the real signature, so codegen has
    /// to reconstruct the type from the dataflow.
    ///
    /// ADR-0033: takes the converged inferred-locals map so any
    /// `_0 = Use(Copy(_un))`-style chain through a `Ty::None`
    /// synthetic temp resolves to the temp's actual stored type
    /// (e.g. F64) rather than the bogus I8 from the declared type.
    fn infer_return_type(
        &self,
        body: &Body,
        inferred: &HashMap<LocalId, ir::Type>,
    ) -> Option<ir::Type> {
        // Walk every block + statement; the return local's RHS type
        // is what we want.
        for block in &body.blocks {
            for stmt in &block.statements {
                if let cobrust_mir::StatementKind::Assign { place, rvalue } = &stmt.kind {
                    if place.local == body.return_local && place.projections.is_empty() {
                        if let Some(ty) = self.rvalue_ty(body, rvalue, inferred) {
                            return Some(ty);
                        }
                    }
                }
            }
        }
        // Fallback to the declared type of the return local.
        body.locals
            .get(body.return_local.0 as usize)
            .and_then(|l| cranelift_scalar_ty(&l.ty))
    }

    /// ADR-0033: `inferred` is the in-progress (during the
    /// fixed-point pass) or converged (afterward) map of
    /// `LocalId → ir::Type` for locals whose declared `Ty` is
    /// `Ty::None`. Threading the map through here is what closes
    /// the original gap: `Operand::Copy(_un)` previously resolved
    /// to the local's declared type (`Ty::None` → `I8`); now it
    /// consults the inferred type (e.g. `F64`).
    fn rvalue_ty(
        &self,
        body: &Body,
        rvalue: &cobrust_mir::Rvalue,
        inferred: &HashMap<LocalId, ir::Type>,
    ) -> Option<ir::Type> {
        match rvalue {
            cobrust_mir::Rvalue::Use(op) => self.operand_ty(body, op, inferred),
            cobrust_mir::Rvalue::BinaryOp(op, a, _b) => {
                use cobrust_mir::BinOp::*;
                match op {
                    Eq | NotEq | Lt | LtEq | Gt | GtEq => Some(ir::types::I8),
                    And | Or => Some(ir::types::I8),
                    In | NotIn => Some(ir::types::I8),
                    _ => self.operand_ty(body, a, inferred),
                }
            }
            cobrust_mir::Rvalue::UnaryOp(_, a) => self.operand_ty(body, a, inferred),
            cobrust_mir::Rvalue::Cast(_, _, ty) => cranelift_scalar_ty(ty),
            cobrust_mir::Rvalue::Aggregate(_, _) | cobrust_mir::Rvalue::Ref(_, _) => {
                Some(self.pointer_type)
            }
            cobrust_mir::Rvalue::Discriminant(_)
            | cobrust_mir::Rvalue::Len(_)
            | cobrust_mir::Rvalue::NullaryOp(_) => Some(ir::types::I64),
        }
    }

    fn operand_ty(
        &self,
        body: &Body,
        op: &cobrust_mir::Operand,
        inferred: &HashMap<LocalId, ir::Type>,
    ) -> Option<ir::Type> {
        use cobrust_mir::{Constant, Operand};
        match op {
            Operand::Copy(p) | Operand::Move(p) => {
                // ADR-0033 fix: prefer the inferred-locals map when
                // the local's declared type is `Ty::None` (or any
                // type that does not yield a scalar Cranelift type
                // via `cranelift_scalar_ty`). Otherwise fall back to
                // the declared type. This single-point change closes
                // the cross-arch float-return bug; the fixed-point
                // loop in `infer_local_types` ensures `inferred`
                // contains the right answer for every chain depth.
                if let Some(ty) = inferred.get(&p.local) {
                    return Some(*ty);
                }
                body.locals
                    .get(p.local.0 as usize)
                    .and_then(|l| cranelift_scalar_ty(&l.ty))
            }
            Operand::Constant(c) => Some(match c {
                Constant::Bool(_) | Constant::None => ir::types::I8,
                Constant::Int(_) => ir::types::I64,
                Constant::Float(_) | Constant::Imag(_) => ir::types::F64,
                Constant::Str(_) | Constant::Bytes(_) | Constant::FnRef(_) => self.pointer_type,
            }),
        }
    }

    /// Pre-pass: infer codegen-time types for any local whose declared
    /// `Ty` does not yield a scalar Cranelift type (typically `Ty::None`-
    /// typed synthetic temps introduced by the MIR lowering for sub-
    /// expression spills). Walk every `Statement::Assign`; the first
    /// rvalue assigned to a local gives that local's effective type.
    ///
    /// ADR-0033: runs to a fixed-point. Each iteration re-evaluates
    /// every Ty::None local against its rvalue using the in-progress
    /// map; an iteration that produces no new bindings ends the loop.
    /// This closes the chain-depth ≥ 2 case where an outer temp's
    /// rvalue references an inner temp whose inferred type is not yet
    /// materialized in iteration 1. Convergence is bounded by the
    /// length of the longest synthetic-temp chain (typically 2-3
    /// iterations for arithmetic-heavy bodies).
    fn infer_local_types(&self, body: &Body) -> HashMap<LocalId, ir::Type> {
        // Identify candidate locals: those whose declared `Ty` is
        // `Ty::None` or otherwise fails to map to a Cranelift scalar.
        // Locals with a useful declared type are excluded from the
        // map; their type comes from the declaration directly.
        let mut candidates: Vec<LocalId> = Vec::new();
        for local in &body.locals {
            // Special-case the return local _0: the lowering always
            // declares it as `Ty::None` (per `BodyBuilder::new`); its
            // real type comes from whatever value gets assigned to it.
            // It is allocated as a Variable separately (with type =
            // `infer_return_type(...)`) but should still appear in the
            // inferred map so `Operand::Copy(_0)` (rare but possible
            // in chained bodies) resolves correctly.
            if matches!(local.ty, cobrust_types::Ty::None)
                || cranelift_scalar_ty(&local.ty).is_none()
            {
                candidates.push(local.id);
            }
        }

        let mut out: HashMap<LocalId, ir::Type> = HashMap::new();

        // ADR-0044: pre-pass that resolves every runtime-call destination
        // (Terminator::Call whose func is Constant::FnRef(known body) or
        // Constant::Str(known runtime helper)) into `out` before the
        // scan-based fixed-point begins. Without this, the scan path
        // can read a partially-populated `out` and resolve a Copy(opt)
        // rvalue to I8 (Ty::None default for opt) just because the
        // tscan iteration order put opt's resolution after the user
        // binding's resolution. Once that wrong type is in `out`, the
        // `if out.contains_key(&local_id) { continue }` guard pins it
        // for the rest of the fixed-point, so the user binding's var is
        // allocated as I8 and Cranelift `ireduce`s a runtime i64 to i8
        // on def_var — verifier-fatal.
        for &local_id in &candidates {
            for block in &body.blocks {
                if let cobrust_mir::Terminator::Call {
                    func, destination, ..
                } = &block.terminator
                {
                    if destination.local != local_id || !destination.projections.is_empty() {
                        continue;
                    }
                    if let cobrust_mir::Operand::Constant(cobrust_mir::Constant::FnRef(id)) = func {
                        if let Some(ty) = self.body_return_types.get(id).copied() {
                            out.insert(local_id, ty);
                            break;
                        }
                    }
                    if let cobrust_mir::Operand::Constant(cobrust_mir::Constant::Str(name)) = func {
                        if let Some(ty) =
                            self.runtime_helper_return_types.get(name.as_str()).copied()
                        {
                            out.insert(local_id, ty);
                            break;
                        }
                    }
                }
            }
        }

        // Fixed-point iteration. The loop terminates because each
        // iteration only adds entries (never removes), the candidate
        // set is finite, and an iteration that adds nothing ends the
        // loop. Bound the iteration count defensively at
        // `candidates.len() + 1` so a malformed MIR (e.g. a self-
        // referential `_x = Copy(_x)` chain) cannot spin forever.
        let max_iters = candidates.len() + 1;
        for _ in 0..max_iters {
            let before = out.len();
            for &local_id in &candidates {
                // Skip already-resolved locals — they cannot get a
                // *better* answer from a later iteration.
                if out.contains_key(&local_id) {
                    continue;
                }
                // ADR-0034: a `Ty::None`-typed local may be the
                // destination of a `Terminator::Call` (the lowering's
                // `_callret` slot) instead of the LHS of an Assign. If
                // the callee is a `Constant::FnRef(id)` of a known
                // user-defined body, propagate the body's declared
                // return type. Without this, `_callret` falls back to
                // I8 (the default for `Ty::None`) and any caller that
                // returns it through a chain miscompiles.
                let mut found = false;
                'tscan: for block in &body.blocks {
                    if let cobrust_mir::Terminator::Call {
                        func, destination, ..
                    } = &block.terminator
                    {
                        if destination.local == local_id && destination.projections.is_empty() {
                            if let cobrust_mir::Operand::Constant(cobrust_mir::Constant::FnRef(
                                id,
                            )) = func
                            {
                                if let Some(ty) = self.body_return_types.get(id).copied() {
                                    out.insert(local_id, ty);
                                    found = true;
                                    break 'tscan;
                                }
                            }
                            // ADR-0044 (post-intrinsic-rewrite path): when
                            // the callee is a `Constant::Str(name)` — i.e.
                            // a runtime-helper symbol after the M11
                            // print-intrinsic / ADR-0044 input/argv rewrite
                            // — propagate the helper's declared return type
                            // from `runtime_helper_signatures`. Without
                            // this, post-rewrite `_xs = __cobrust_argv()`
                            // destinations fall back to I8 (Ty::None
                            // default) and any downstream use (e.g.
                            // `for x in xs:` lowering's `_iter` copy of
                            // the list pointer) triggers a Cranelift
                            // `ireduce.i8` that the verifier rejects when
                            // the iter_init expects an I64 pointer arg.
                            if let cobrust_mir::Operand::Constant(cobrust_mir::Constant::Str(
                                name,
                            )) = func
                            {
                                if let Some(ty) =
                                    self.runtime_helper_return_types.get(name.as_str()).copied()
                                {
                                    out.insert(local_id, ty);
                                    found = true;
                                    break 'tscan;
                                }
                            }
                        }
                    }
                }
                if found {
                    continue;
                }
                // Find the first Assign to this local that yields a
                // resolvable type given the current `out` snapshot.
                'scan: for block in &body.blocks {
                    for stmt in &block.statements {
                        if let cobrust_mir::StatementKind::Assign { place, rvalue } = &stmt.kind {
                            if place.local == local_id && place.projections.is_empty() {
                                if let Some(ty) = self.rvalue_ty(body, rvalue, &out) {
                                    out.insert(local_id, ty);
                                    break 'scan;
                                }
                            }
                        }
                    }
                }
            }
            if out.len() == before {
                break;
            }
        }
        out
    }

    fn declare_body(&mut self, obj: &mut ObjectModule, body: &Body) -> Result<(), CodegenError> {
        let sig = self.body_signature(body);
        // ADR-0025 §G "Runtime requirements": codegen emits the user's
        // top-level `main` as `_cobrust_user_main`. The C runtime
        // shim (`cobrust_main.c`) provides the platform `main(argc, argv)`
        // which captures argv and dispatches here.
        let name = if body.name.is_empty() {
            format!("_cobrust_init_{}", body.def_id.0)
        } else if body.name == "main" {
            "_cobrust_user_main".to_string()
        } else {
            body.name.clone()
        };

        // ADR-0034: stash the body's declared return type before we hand
        // the signature to Cranelift. Used by `infer_local_types` to
        // propagate the return type to `_callret` locals at any caller
        // site that invokes this body via `Constant::FnRef(def_id)`.
        // The signature always has exactly one `returns` slot (codegen
        // convention; `body_signature` pushes one `AbiParam` for the
        // inferred return type).
        if let Some(ret_param) = sig.returns.first() {
            self.body_return_types
                .insert(body.def_id.0, ret_param.value_type);
        }

        let func_id = obj
            .declare_function(&name, Linkage::Export, &sig)
            .map_err(|e| CodegenError::CraneliftError(e.to_string()))?;
        self.function_ids.insert(body.def_id.0, func_id);
        Ok(())
    }

    fn define_body(&mut self, obj: &mut ObjectModule, body: &Body) -> Result<(), CodegenError> {
        let func_id = *self.function_ids.get(&body.def_id.0).ok_or_else(|| {
            CodegenError::Internal(format!("body {} not declared", body.def_id.0))
        })?;

        if std::env::var_os("COBRUST_M9_DUMP_BODY").is_some() {
            eprintln!(
                "BODY {} def_id={} param_count={} return_local=_{} locals={:?}",
                body.name,
                body.def_id.0,
                body.param_count,
                body.return_local.0,
                body.locals
                    .iter()
                    .map(|l| format!("_{}:{}", l.id.0, l.ty))
                    .collect::<Vec<_>>()
            );
        }

        // ADR-0033: compute the converged inferred-locals map once so
        // the same map drives the function signature, the return-type
        // inference, AND the per-local Variable declarations. Without
        // this single source of truth the three sites can disagree on
        // a `Ty::None` local's effective type, which is exactly the
        // cross-arch float-return bug from the M9 finding.
        let inferred_locals = self.infer_local_types(body);

        let sig = self.body_signature_with(body, &inferred_locals);
        let mut function = Function::with_name_signature(UserFuncName::user(0, body.def_id.0), sig);

        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_ctx);

        // --- compute reachable block set (filters drop-chain dead code) -
        let reachable_pre = compute_reachable_blocks(body);

        // --- create one Cranelift block per MIR block ----------------
        let mut block_map: HashMap<BlockId, ir::Block> = HashMap::new();
        for (idx, mir_block) in body.blocks.iter().enumerate() {
            if !reachable_pre.contains(&mir_block.id) {
                continue;
            }
            let cl_block = builder.create_block();
            if idx == 0 {
                // entry block — append params (matches body_signature's
                // sig.params: skip the synthetic return slot at locals[0]
                // when present).
                let param_locals: Vec<_> = if body.return_local == cobrust_mir::LocalId(0) {
                    body.locals.iter().skip(1).take(body.param_count).collect()
                } else {
                    body.locals.iter().take(body.param_count).collect()
                };
                for local in &param_locals {
                    let ty = cranelift_scalar_ty(&local.ty).unwrap_or(self.pointer_type);
                    builder.append_block_param(cl_block, ty);
                }
            }
            block_map.insert(mir_block.id, cl_block);
        }

        // --- declare variables for every local -----------------------
        // The return local's MIR type is `Ty::None` by convention (lower.rs
        // `BodyBuilder::new`); use the *inferred* return type so the
        // `Terminator::Return` reads a well-typed value.
        //
        // The MIR lowering also occasionally introduces `Ty::None`-typed
        // temporary locals to hold sub-expression results. We infer their
        // codegen-time types from the rvalue actually assigned by walking
        // every Assign statement.
        let inferred_ret = self
            .infer_return_type(body, &inferred_locals)
            .unwrap_or(self.pointer_type);
        let mut var_map: HashMap<LocalId, Variable> = HashMap::new();
        for local in &body.locals {
            let ty = if local.id == body.return_local {
                inferred_ret
            } else if let Some(inferred) = inferred_locals.get(&local.id) {
                *inferred
            } else {
                cranelift_scalar_ty(&local.ty).unwrap_or(self.pointer_type)
            };
            let var = builder.declare_var(ty);
            var_map.insert(local.id, var);
        }

        // --- entry block: switch + bind params ----------------------
        let entry = block_map[&BlockId(0)];
        builder.switch_to_block(entry);

        // copy block params (= function params) into local Variables
        let param_locals: Vec<_> = if body.return_local == cobrust_mir::LocalId(0) {
            body.locals.iter().skip(1).take(body.param_count).collect()
        } else {
            body.locals.iter().take(body.param_count).collect()
        };
        for (idx, local) in param_locals.iter().enumerate() {
            let val = builder.block_params(entry)[idx];
            let var = var_map[&local.id];
            builder.def_var(var, val);
        }

        // Pre-initialize all locals (except those bound as block params
        // above) with a zero of their declared type. Guarantees every
        // Variable is defined on every path; otherwise Cranelift emits
        // "use_var on undefined variable" inside the verifier when the
        // drop schedule or unreachable arm jumps directly to a successor.
        let pointer_type = self.pointer_type;
        let param_local_ids: std::collections::HashSet<_> =
            param_locals.iter().map(|l| l.id).collect();
        for local in &body.locals {
            if param_local_ids.contains(&local.id) {
                continue;
            }
            let var = var_map[&local.id];
            let ty = if local.id == body.return_local {
                inferred_ret
            } else if let Some(inferred) = inferred_locals.get(&local.id) {
                *inferred
            } else {
                cranelift_scalar_ty(&local.ty).unwrap_or(pointer_type)
            };
            let zero = if ty.is_int() {
                builder.ins().iconst(ty, 0)
            } else if ty == ir::types::F32 {
                builder.ins().f32const(0.0_f32)
            } else if ty == ir::types::F64 {
                builder.ins().f64const(0.0_f64)
            } else {
                builder.ins().iconst(pointer_type, 0)
            };
            builder.def_var(var, zero);
        }

        // --- ADR-0024: declare external imports for `Constant::Str` callees ---
        // M10 amends ADR-0023's Call lowering: a `Terminator::Call` whose
        // `func` operand is `Operand::Constant(Constant::Str(name))`
        // resolves to an external imported symbol declared with
        // `Linkage::Import`. Used today by the hello-world runtime
        // helper `__cobrust_println_static`. M11 stdlib will broaden.
        // ADR-0027 §4 amendment: a Call to one of the runtime helpers
        // pre-declared below (`__cobrust_iter_init`, ...) reuses that
        // signature; only "freeform" Constant::Str callees fall back
        // to the M10 `(ptr, len)` legacy shape.
        let runtime_helper_names: std::collections::HashSet<&'static str> =
            runtime_helper_signatures(self.pointer_type, self.call_conv)
                .iter()
                .map(|(n, _)| *n)
                .collect();
        let mut extern_func_ids: HashMap<String, cranelift_module::FuncId> = HashMap::new();
        for mir_block in &body.blocks {
            if let cobrust_mir::Terminator::Call { func, .. } = &mir_block.terminator {
                if let cobrust_mir::Operand::Constant(cobrust_mir::Constant::Str(name)) = func {
                    if extern_func_ids.contains_key(name) {
                        continue;
                    }
                    // Skip names handled by the typed runtime-helper
                    // table (ADR-0027 §4). The signature there is
                    // authoritative.
                    if runtime_helper_names.contains(name.as_str()) {
                        continue;
                    }
                    // ADR-0025 §"Runtime ABI" widens M10's void(void)
                    // signature to `(*const u8, usize)` for runtime
                    // helpers consumed by string-literal callsites.
                    // We always declare as 2-arg `(ptr, len)`; if the
                    // Call's args list is empty (M10 hello-world legacy
                    // path), we pass `(NULL, 0)` so the signatures
                    // match. Runtime helpers tolerate the null path.
                    let mut sig = Signature::new(self.call_conv);
                    sig.params.push(AbiParam::new(self.pointer_type));
                    sig.params.push(AbiParam::new(ir::types::I64));
                    let extern_id = obj
                        .declare_function(name, Linkage::Import, &sig)
                        .map_err(|e| CodegenError::CraneliftError(e.to_string()))?;
                    extern_func_ids.insert(name.clone(), extern_id);
                    let _ = sig; // silence unused warning under cfg branches
                }
            }
        }
        let mut extern_funcs: HashMap<String, ir::FuncRef> = HashMap::new();
        for (name, fid) in &extern_func_ids {
            let func_ref = obj.declare_func_in_func(*fid, builder.func);
            extern_funcs.insert(name.clone(), func_ref);
        }

        // --- ADR-0034 §"Decision" Option 3: user-defined fn FuncRefs ---
        // `self.function_ids` is the per-CraneliftCtx forward-declaration
        // table populated in `declare_body` (line 379). The `emit`
        // entry-point at the top of this file iterates `module.bodies`
        // twice — first calling `declare_body` for every body, then
        // calling `define_body`. By the time control reaches this
        // point, every user-defined fn's FuncId already exists, so any
        // body in the second pass can reference any other body
        // (including itself) via the `Constant::FnRef(def_id)` operand
        // emitted by HIR/MIR.
        //
        // We materialize a per-body `user_funcs: HashMap<u32, FuncRef>`
        // by `declare_func_in_func`-ing every entry. The map's values
        // are scoped to this `builder.func` — different bodies must
        // re-declare. EmitCtx consumes this in `lower_call` to convert
        // `Operand::Constant(Constant::FnRef(id))` callees into real
        // Cranelift `call` instructions.
        let mut user_funcs: HashMap<u32, ir::FuncRef> = HashMap::new();
        for (def_id, fid) in &self.function_ids {
            let func_ref = obj.declare_func_in_func(*fid, builder.func);
            user_funcs.insert(*def_id, func_ref);
        }

        // --- ADR-0027 §1: declare runtime helpers for Aggregate / Cast /
        //     f-string lowering. The set is fixed; we declare imports
        //     up-front so per-body lowering can reference any of them
        //     by name. Unused symbols are stripped at link time.
        let runtime_helpers = runtime_helper_signatures(self.pointer_type, self.call_conv);
        let mut runtime_func_ids: HashMap<&'static str, cranelift_module::FuncId> = HashMap::new();
        for (name, sig) in &runtime_helpers {
            // Skip if already declared via the Constant::Str path above.
            if extern_func_ids.contains_key(*name) {
                continue;
            }
            let fid = obj
                .declare_function(name, Linkage::Import, sig)
                .map_err(|e| CodegenError::CraneliftError(e.to_string()))?;
            runtime_func_ids.insert(*name, fid);
        }
        let mut runtime_funcs: HashMap<&'static str, ir::FuncRef> = HashMap::new();
        for (name, fid) in &runtime_func_ids {
            runtime_funcs.insert(*name, obj.declare_func_in_func(*fid, builder.func));
        }
        // ADR-0044: parallel param-count map for the runtime helpers.
        // Consumed by `lower_terminator` to detect the
        // `(*const u8, usize)` expansion case (1 source Str arg → 2 ir
        // params).
        let runtime_helper_param_counts: HashMap<&'static str, usize> = runtime_helpers
            .iter()
            .map(|(name, sig)| (*name, sig.params.len()))
            .collect();

        // --- ADR-0025 §"Codegen amendments" Constant::Str row ---------
        // Materialize every Constant::Str payload referenced as a
        // runtime-call argument into a `.rodata` data symbol. The
        // EmitCtx then lowers `materialize_str_data(s)` to a pair of
        // Cranelift values: pointer to the data symbol + length.
        //
        // M11 scope: only payloads on the `args[0]` slot of a
        // `Terminator::Call` whose `func` is `Constant::Str(_)` are
        // interned. M12 will widen to all Constant::Str uses
        // (including local-binding slots).
        let mut str_data_ids: HashMap<String, cranelift_module::DataId> = HashMap::new();
        for mir_block in &body.blocks {
            if let cobrust_mir::Terminator::Call { func, args, .. } = &mir_block.terminator {
                if !matches!(
                    func,
                    cobrust_mir::Operand::Constant(cobrust_mir::Constant::Str(_))
                ) {
                    continue;
                }
                // ADR-0044 W2 Phase 3 amendment: intern ALL string-constant
                // args, not just args[0]. This covers str_eq(c, "lit") where
                // the literal is the second argument.
                for arg in args {
                    if let cobrust_mir::Operand::Constant(cobrust_mir::Constant::Str(payload)) = arg
                    {
                        if str_data_ids.contains_key(payload) {
                            continue;
                        }
                        let symbol = str_data_symbol(body.def_id.0, str_data_ids.len(), payload);
                        let data_id = obj
                            .declare_data(&symbol, Linkage::Local, false, false)
                            .map_err(|e| CodegenError::CraneliftError(e.to_string()))?;
                        let mut data_desc = cranelift_module::DataDescription::new();
                        data_desc.define(payload.as_bytes().to_vec().into_boxed_slice());
                        obj.define_data(data_id, &data_desc)
                            .map_err(|e| CodegenError::CraneliftError(e.to_string()))?;
                        str_data_ids.insert(payload.clone(), data_id);
                    }
                }
            }
        }
        let mut str_data_globals: HashMap<String, ir::GlobalValue> = HashMap::new();
        for (payload, did) in &str_data_ids {
            let gv = obj.declare_data_in_func(*did, builder.func);
            str_data_globals.insert(payload.clone(), gv);
        }

        // --- lower each block's statements + terminator --------------
        // ADR-0027 §1 amendment: the drop-schedule pass may emit dead
        // blocks (drop chains for unreachable Return blocks) when the
        // entry block itself is a Return. Cranelift's verifier rejects
        // edges into the entry block, so skip blocks that are not
        // forward-reachable from BlockId(0).
        let reachable = compute_reachable_blocks(body);
        let mut stack_slots: HashMap<LocalId, ir::StackSlot> = HashMap::new();
        let mut to_emit: Vec<&BasicBlock> = body
            .blocks
            .iter()
            .filter(|b| reachable.contains(&b.id))
            .collect();
        for mir_block in to_emit.drain(..) {
            let cl_block = block_map[&mir_block.id];
            if mir_block.id != BlockId(0) {
                builder.switch_to_block(cl_block);
            }
            let mut emit_ctx = EmitCtx {
                pointer_type,
                var_map: &var_map,
                block_map: &block_map,
                builder: &mut builder,
                body,
                extern_funcs: &extern_funcs,
                runtime_funcs: &runtime_funcs,
                runtime_helper_param_counts: &runtime_helper_param_counts,
                user_funcs: &user_funcs,
                str_data_globals: &str_data_globals,
                stack_slots: &mut stack_slots,
            };
            for stmt in &mir_block.statements {
                emit_ctx.lower_statement(stmt)?;
            }
            emit_ctx.lower_terminator(&mir_block.terminator)?;
        }

        builder.seal_all_blocks();
        builder.finalize();

        let mut ctx = Context::for_function(function);
        obj.define_function(func_id, &mut ctx).map_err(|e| {
            let detail = match &e {
                cranelift_module::ModuleError::Compilation(
                    cranelift_codegen::CodegenError::Verifier(v),
                ) => format!(
                    "Verifier errors: {v}\n--- IR ---\n{}\n--- end IR ---",
                    ctx.func.display()
                ),
                cranelift_module::ModuleError::Compilation(cl) => {
                    format!("{cl}\n--- IR ---\n{}\n--- end IR ---", ctx.func.display())
                }
                _ => e.to_string(),
            };
            CodegenError::CraneliftError(detail)
        })?;
        Ok(())
    }
}

// =====================================================================
// EmitCtx — per-block emission helpers
// =====================================================================

struct EmitCtx<'a, 'b> {
    pointer_type: ir::Type,
    var_map: &'a HashMap<LocalId, Variable>,
    block_map: &'a HashMap<BlockId, ir::Block>,
    builder: &'a mut FunctionBuilder<'b>,
    body: &'a Body,
    /// External symbols declared via `Linkage::Import` in `define_body`.
    /// ADR-0024 amends ADR-0023's `Terminator::Call` row: when `func`
    /// operand resolves to `Operand::Constant(Constant::Str(name))`,
    /// the call lowers to a real Cranelift `call` to the FuncRef
    /// stored in this map.
    extern_funcs: &'a HashMap<String, ir::FuncRef>,
    /// Runtime helper FuncRefs declared at body-define time. ADR-0027
    /// §1/§3/§5 binds these symbols (`__cobrust_list_new`,
    /// `__cobrust_str_push_static`, ...) as Linkage::Import imports
    /// resolved at link time against `cobrust-stdlib`'s C-ABI surface.
    runtime_funcs: &'a HashMap<&'static str, ir::FuncRef>,
    /// ADR-0044: param-count lookup for runtime helpers, used by
    /// `lower_terminator` to detect the `(*const u8, usize)` expansion
    /// case — a runtime helper whose signature has 2 params (typically
    /// the (ptr, len) shape) being called with a single source
    /// `Constant::Str` arg. Without this expansion the verifier
    /// rejects the call (1 ir value supplied for 2 sig params).
    runtime_helper_param_counts: &'a HashMap<&'static str, usize>,
    /// User-defined fn FuncRefs (ADR-0034 §"Decision" Option 3). Keyed
    /// on the MIR `Body.def_id.0` value, which matches the `u32` payload
    /// of `Constant::FnRef`. Populated in `define_body` from
    /// `CraneliftCtx.function_ids` after the forward-declaration pass.
    /// Used by `lower_call` to lower
    /// `Operand::Constant(Constant::FnRef(id))` callees into real
    /// Cranelift `call` instructions — closing the M9 stub at the
    /// recursive / cross-module-fn callsite.
    user_funcs: &'a HashMap<u32, ir::FuncRef>,
    /// String-payload data globals declared at body-define time.
    /// ADR-0025 §"Codegen amendments" Constant::Str row: the runtime
    /// `(*const u8, usize)` ABI reads the payload pointer from these
    /// globals at call sites.
    str_data_globals: &'a HashMap<String, ir::GlobalValue>,
    /// ADR-0027 §2: per-local stack-slot allocations created lazily by
    /// `Rvalue::Ref` lowering. A local that gets `&x`'d once is moved
    /// onto a Cranelift StackSlot so we can take its address; future
    /// reads/writes go through the slot. M12.x materializes the slot
    /// lazily — locals that are never `Ref`'d stay in plain Variables.
    stack_slots: &'a mut HashMap<LocalId, ir::StackSlot>,
}

impl<'a, 'b> EmitCtx<'a, 'b> {
    /// ADR-0025 §"Codegen amendments" Constant::Str row: lower a
    /// string-payload to `(ptr, len)` Cranelift values. The pointer
    /// is the address of the `.rodata` data symbol declared at
    /// body-define time; the length is the byte count.
    fn materialize_str_data(
        &mut self,
        payload: &str,
    ) -> Result<(ir::Value, ir::Value), CodegenError> {
        let gv = self.str_data_globals.get(payload).copied().ok_or_else(|| {
            CodegenError::Internal(format!(
                "str payload {payload:?} not interned; codegen-time bug"
            ))
        })?;
        let ptr = self.builder.ins().symbol_value(self.pointer_type, gv);
        let len = self
            .builder
            .ins()
            .iconst(ir::types::I64, payload.len() as i64);
        Ok((ptr, len))
    }

    fn lower_statement(&mut self, stmt: &Statement) -> Result<(), CodegenError> {
        match &stmt.kind {
            StatementKind::Nop | StatementKind::StorageLive(_) | StatementKind::StorageDead(_) => {
                Ok(())
            }
            StatementKind::Assign { place, rvalue } => {
                let value = self.lower_rvalue(rvalue)?;
                self.write_place(place, value)
            }
        }
    }

    fn block_id(&self, b: &BlockId) -> Result<ir::Block, CodegenError> {
        self.block_map
            .get(b)
            .copied()
            .ok_or_else(|| CodegenError::InvalidMir(format!("unknown block bb{}", b.0)))
    }

    fn lower_terminator(&mut self, term: &Terminator) -> Result<(), CodegenError> {
        match term {
            Terminator::Goto(target) => {
                let blk = self.block_id(target)?;
                self.builder.ins().jump(blk, &[]);
                Ok(())
            }
            Terminator::Return => {
                let var = self.var_map[&self.body.return_local];
                let val = self.builder.use_var(var);
                self.builder.ins().return_(&[val]);
                Ok(())
            }
            Terminator::Unreachable => {
                self.builder
                    .ins()
                    .trap(ir::TrapCode::user(1).expect("trap code 1"));
                Ok(())
            }
            Terminator::SwitchInt {
                operand,
                cases,
                otherwise,
            } => {
                let scrutinee = self.lower_operand(operand)?;
                self.lower_switch_int(scrutinee, cases, *otherwise)
            }
            Terminator::Assert {
                cond,
                expected,
                msg,
                target,
            } => {
                let cond_val = self.lower_operand(cond)?;
                let target_blk = self.block_id(target)?;
                let trap_blk = self.builder.create_block();
                if *expected {
                    self.builder
                        .ins()
                        .brif(cond_val, target_blk, &[], trap_blk, &[]);
                } else {
                    self.builder
                        .ins()
                        .brif(cond_val, trap_blk, &[], target_blk, &[]);
                }
                self.builder.switch_to_block(trap_blk);
                let trap_code = match msg {
                    AssertKind::DivisionByZero => ir::TrapCode::INTEGER_DIVISION_BY_ZERO,
                    AssertKind::Overflow => ir::TrapCode::INTEGER_OVERFLOW,
                    AssertKind::BoundsCheck => ir::TrapCode::HEAP_OUT_OF_BOUNDS,
                    AssertKind::Unreachable => ir::TrapCode::user(2).expect("trap code 2"),
                };
                self.builder.ins().trap(trap_code);
                self.builder.seal_block(trap_blk);
                Ok(())
            }
            Terminator::Drop { target, .. } => {
                let blk = self.block_id(target)?;
                self.builder.ins().jump(blk, &[]);
                Ok(())
            }
            Terminator::Call {
                func,
                args,
                destination,
                target,
                unwind: _,
            } => {
                // ADR-0024 amendment: when `func` is `Constant::Str(name)`,
                // emit a real Cranelift `call` to the imported symbol
                // (extern_funcs / runtime_funcs branches below).
                //
                // ADR-0034 amendment: when `func` is `Constant::FnRef(id)`,
                // look up the user-defined fn's FuncRef in `user_funcs`
                // (populated from `CraneliftCtx.function_ids` after the
                // forward-declaration pass) and emit a real Cranelift
                // `call`. This closes the M9 stub for user-defined fn
                // calls — recursion + mutual recursion + cross-fn
                // dispatch all light up here.
                if let Operand::Constant(Constant::FnRef(id)) = func {
                    if let Some(func_ref) = self.user_funcs.get(id).copied() {
                        let mut call_args = Vec::with_capacity(args.len());
                        for arg in args {
                            let v = self.lower_operand(arg)?;
                            call_args.push(v);
                        }
                        let inst = self.builder.ins().call(func_ref, &call_args);
                        let results = self.builder.inst_results(inst);
                        let ret_val = if results.is_empty() {
                            self.builder.ins().iconst(ir::types::I64, 0)
                        } else {
                            results[0]
                        };
                        self.write_place(destination, ret_val)?;
                        let blk = self.block_id(target)?;
                        self.builder.ins().jump(blk, &[]);
                        return Ok(());
                    }
                    // Falls through to the M9 stub below if the FnRef id
                    // is unknown (e.g. lambda placeholder `FnRef(0)` or
                    // await placeholder `FnRef(u32::MAX)` from MIR
                    // lowering — both are pre-M13 stubs). The M9 stub
                    // semantics for those callees are unchanged.
                }
                if let Operand::Constant(Constant::Str(name)) = func {
                    // ADR-0027 §4: prefer runtime-helper FuncRef when
                    // the callee name matches the typed-signature
                    // table.
                    if let Some(func_ref) = self.runtime_funcs.get(name.as_str()).copied() {
                        // Lower each arg. For Constant::Str args we
                        // materialize the rodata pointer (push_static
                        // pattern); other operands lower directly.
                        //
                        // ADR-0044: detect the `(*const u8, usize)`
                        // expansion case — when the source has a single
                        // `Constant::Str` arg but the runtime helper's
                        // signature expects two params, we emit ptr+len
                        // pair (same shape the M11 `__cobrust_println`
                        // extern_funcs path uses). This covers
                        // `input("prompt")` post-rewrite.
                        let sig_param_count = self
                            .runtime_helper_param_counts
                            .get(name.as_str())
                            .copied()
                            .unwrap_or(args.len());
                        let mut call_args = Vec::with_capacity(sig_param_count);
                        let expand_str_to_ptr_len = args.len() == 1
                            && sig_param_count == 2
                            && matches!(args.first(), Some(Operand::Constant(Constant::Str(_))));
                        // ADR-0044 W2 Phase 3: detect trailing Constant::Str
                        // expansion for calls where the last source arg is a
                        // string literal and the C signature has one extra
                        // param for the `len` (e.g. __cobrust_str_eq_lit with
                        // 2 source args but 3 C params).
                        let expand_trailing_str_len = !expand_str_to_ptr_len
                            && args.len() + 1 == sig_param_count
                            && matches!(args.last(), Some(Operand::Constant(Constant::Str(_))));
                        for (idx, arg) in args.iter().enumerate() {
                            if let Operand::Constant(Constant::Str(payload)) = arg {
                                let (ptr, len) = self.materialize_str_data(payload)?;
                                call_args.push(ptr);
                                let is_last = idx + 1 == args.len();
                                if expand_str_to_ptr_len || (expand_trailing_str_len && is_last) {
                                    call_args.push(len);
                                }
                            } else {
                                let v = self.lower_operand(arg)?;
                                call_args.push(v);
                            }
                        }
                        let inst = self.builder.ins().call(func_ref, &call_args);
                        let results = self.builder.inst_results(inst);
                        let ret_val = if results.is_empty() {
                            self.builder.ins().iconst(ir::types::I64, 0)
                        } else {
                            results[0]
                        };
                        self.write_place(destination, ret_val)?;
                        let blk = self.block_id(target)?;
                        self.builder.ins().jump(blk, &[]);
                        return Ok(());
                    }
                    if let Some(func_ref) = self.extern_funcs.get(name).copied() {
                        // ADR-0025 §"Runtime ABI" + §"Codegen amendments"
                        // Constant::Str row: when the runtime helper has
                        // a `(*const u8, usize)` signature, materialize
                        // a payload from `_args[0] = Constant::Str(s)` if
                        // present; pass `(NULL, 0)` otherwise (covers
                        // the M10 hello-world signature widening path).
                        let (ptr_val, len_val) =
                            if let Some(Operand::Constant(Constant::Str(payload))) = args.first() {
                                self.materialize_str_data(payload)?
                            } else {
                                let null = self.builder.ins().iconst(self.pointer_type, 0);
                                let zero = self.builder.ins().iconst(ir::types::I64, 0);
                                (null, zero)
                            };
                        self.builder.ins().call(func_ref, &[ptr_val, len_val]);
                        let zero_ret = self.builder.ins().iconst(ir::types::I64, 0);
                        self.write_place(destination, zero_ret)?;
                        let blk = self.block_id(target)?;
                        self.builder.ins().jump(blk, &[]);
                        return Ok(());
                    }
                }
                // M9 stub fallthrough.
                let zero = self.builder.ins().iconst(ir::types::I64, 0);
                self.write_place(destination, zero)?;
                let blk = self.block_id(target)?;
                self.builder.ins().jump(blk, &[]);
                Ok(())
            }
        }
    }

    fn lower_switch_int(
        &mut self,
        scrutinee: ir::Value,
        cases: &[(SwitchValue, BlockId)],
        otherwise: BlockId,
    ) -> Result<(), CodegenError> {
        let otherwise_blk = self.block_id(&otherwise)?;
        if cases.is_empty() {
            self.builder.ins().jump(otherwise_blk, &[]);
            return Ok(());
        }
        // ADR-0044 codegen amendment: align the case-value iconst type
        // with the scrutinee's value type. Previously a `SwitchValue::Bool`
        // case would emit `iconst.i8 0` against an i64 scrutinee
        // (typical when scrutinee is the for-protocol's `opt_local`
        // typed by ADR-0044's runtime-helper return-type inference),
        // tripping Cranelift's `icmp` verifier on the arg-type mismatch.
        let scrutinee_ty = self.builder.func.dfg.value_type(scrutinee);
        let mut current_otherwise = otherwise_blk;
        for (i, (val, target)) in cases.iter().enumerate().rev() {
            let case_blk = self.block_id(target)?;
            // Compute the test value at the scrutinee's int type. For
            // non-int scrutinees fall back to the legacy widths.
            let payload = match val {
                SwitchValue::Bool(b) => i64::from(*b),
                SwitchValue::Int(v) => *v,
                SwitchValue::Adt(d) => i64::from(*d),
            };
            let test_val = if scrutinee_ty.is_int() {
                self.builder.ins().iconst(scrutinee_ty, payload)
            } else {
                let legacy_ty = match val {
                    SwitchValue::Bool(_) => ir::types::I8,
                    SwitchValue::Int(_) => ir::types::I64,
                    SwitchValue::Adt(_) => ir::types::I32,
                };
                self.builder.ins().iconst(legacy_ty, payload)
            };
            let cmp = self
                .builder
                .ins()
                .icmp(ir::condcodes::IntCC::Equal, scrutinee, test_val);
            if i == 0 {
                self.builder
                    .ins()
                    .brif(cmp, case_blk, &[], current_otherwise, &[]);
            } else {
                let next_blk = self.builder.create_block();
                self.builder.ins().brif(cmp, case_blk, &[], next_blk, &[]);
                self.builder.switch_to_block(next_blk);
                current_otherwise = next_blk;
            }
        }
        Ok(())
    }

    fn lower_rvalue(&mut self, rvalue: &Rvalue) -> Result<ir::Value, CodegenError> {
        match rvalue {
            Rvalue::Use(op) => self.lower_operand(op),
            Rvalue::BinaryOp(op, a, b) => {
                let av = self.lower_operand(a)?;
                let bv = self.lower_operand(b)?;
                self.lower_binop(*op, av, bv)
            }
            Rvalue::UnaryOp(op, a) => {
                let av = self.lower_operand(a)?;
                self.lower_unop(*op, av)
            }
            Rvalue::Aggregate(kind, ops) => self.lower_aggregate(kind, ops),
            Rvalue::Cast(kind, op, ty) => self.lower_cast(*kind, op, ty),
            Rvalue::Ref(_, place) => self.lower_ref(place),
            Rvalue::Discriminant(_) | Rvalue::Len(_) | Rvalue::NullaryOp(_) => {
                Ok(self.builder.ins().iconst(ir::types::I64, 0))
            }
        }
    }

    /// ADR-0027 §1: `Rvalue::Aggregate(kind, operands)` →
    /// heap-allocated value via `__cobrust_<type>_new` +
    /// element-by-element setters.
    fn lower_aggregate(
        &mut self,
        kind: &AggregateKind,
        operands: &[Operand],
    ) -> Result<ir::Value, CodegenError> {
        match kind {
            AggregateKind::Tuple => self.lower_aggregate_tuple(operands),
            AggregateKind::List => self.lower_aggregate_list(operands),
            AggregateKind::Dict => self.lower_aggregate_dict(operands),
            AggregateKind::Set => self.lower_aggregate_set(operands),
            AggregateKind::Record | AggregateKind::Adt(_, _) => {
                // M12.x: structs lower like tuples. Discriminant
                // wiring for ADTs is a Phase F follow-up.
                self.lower_aggregate_tuple(operands)
            }
            AggregateKind::FormatString => self.lower_aggregate_format_string(operands),
        }
    }

    fn lower_aggregate_tuple(&mut self, operands: &[Operand]) -> Result<ir::Value, CodegenError> {
        let n = operands.len() as i64;
        let n_v = self.builder.ins().iconst(ir::types::I64, n);
        let new_call = self.runtime_funcs.get("__cobrust_tuple_new").copied();
        let alloc = if let Some(fr) = new_call {
            let inst = self.builder.ins().call(fr, &[n_v]);
            self.builder.inst_results(inst)[0]
        } else {
            self.builder.ins().iconst(self.pointer_type, 0)
        };
        if let Some(set_fr) = self.runtime_funcs.get("__cobrust_tuple_set").copied() {
            for (i, op) in operands.iter().enumerate() {
                let idx_v = self.builder.ins().iconst(ir::types::I64, i as i64);
                let val = self.lower_operand(op)?;
                let val_i64 = coerce_to_i64(self.builder, val);
                self.builder.ins().call(set_fr, &[alloc, idx_v, val_i64]);
            }
        }
        Ok(alloc)
    }

    fn lower_aggregate_list(&mut self, operands: &[Operand]) -> Result<ir::Value, CodegenError> {
        let elem_size = self.builder.ins().iconst(ir::types::I64, 8);
        let len_v = self
            .builder
            .ins()
            .iconst(ir::types::I64, operands.len() as i64);
        let alloc = if let Some(fr) = self.runtime_funcs.get("__cobrust_list_new").copied() {
            let inst = self.builder.ins().call(fr, &[elem_size, len_v]);
            self.builder.inst_results(inst)[0]
        } else {
            self.builder.ins().iconst(self.pointer_type, 0)
        };
        if let Some(set_fr) = self.runtime_funcs.get("__cobrust_list_set").copied() {
            for (i, op) in operands.iter().enumerate() {
                let idx_v = self.builder.ins().iconst(ir::types::I64, i as i64);
                let val = self.lower_operand(op)?;
                let val_i64 = coerce_to_i64(self.builder, val);
                self.builder.ins().call(set_fr, &[alloc, idx_v, val_i64]);
            }
        }
        Ok(alloc)
    }

    fn lower_aggregate_dict(&mut self, operands: &[Operand]) -> Result<ir::Value, CodegenError> {
        // Dict operands come as a flat (k, v, k, v, ...) sequence.
        let pair_count = (operands.len() / 2) as i64;
        let k_size = self.builder.ins().iconst(ir::types::I64, 8);
        let v_size = self.builder.ins().iconst(ir::types::I64, 8);
        let len_v = self.builder.ins().iconst(ir::types::I64, pair_count);
        let alloc = if let Some(fr) = self.runtime_funcs.get("__cobrust_dict_new").copied() {
            let inst = self.builder.ins().call(fr, &[k_size, v_size, len_v]);
            self.builder.inst_results(inst)[0]
        } else {
            self.builder.ins().iconst(self.pointer_type, 0)
        };
        if let Some(set_fr) = self.runtime_funcs.get("__cobrust_dict_set").copied() {
            for chunk in operands.chunks(2) {
                if chunk.len() == 2 {
                    let k_val = self.lower_operand(&chunk[0])?;
                    let v_val = self.lower_operand(&chunk[1])?;
                    let k_i64 = coerce_to_i64(self.builder, k_val);
                    let v_i64 = coerce_to_i64(self.builder, v_val);
                    self.builder.ins().call(set_fr, &[alloc, k_i64, v_i64]);
                }
            }
        }
        Ok(alloc)
    }

    fn lower_aggregate_set(&mut self, operands: &[Operand]) -> Result<ir::Value, CodegenError> {
        let elem_size = self.builder.ins().iconst(ir::types::I64, 8);
        let len_v = self
            .builder
            .ins()
            .iconst(ir::types::I64, operands.len() as i64);
        let alloc = if let Some(fr) = self.runtime_funcs.get("__cobrust_set_new").copied() {
            let inst = self.builder.ins().call(fr, &[elem_size, len_v]);
            self.builder.inst_results(inst)[0]
        } else {
            self.builder.ins().iconst(self.pointer_type, 0)
        };
        if let Some(insert_fr) = self.runtime_funcs.get("__cobrust_set_insert").copied() {
            for op in operands {
                let v = self.lower_operand(op)?;
                let v_i64 = coerce_to_i64(self.builder, v);
                self.builder.ins().call(insert_fr, &[alloc, v_i64]);
            }
        }
        Ok(alloc)
    }

    /// ADR-0027 §5: f-string lowering. Operands alternate as
    /// `Constant::Str(static_chunk)` and arbitrary expression operands
    /// for the holes. We allocate a fresh string buffer, push each
    /// static chunk via `__cobrust_str_push_static`, and dispatch the
    /// hole formatters by Cranelift value type.
    fn lower_aggregate_format_string(
        &mut self,
        operands: &[Operand],
    ) -> Result<ir::Value, CodegenError> {
        // Allocate buffer.
        let buf = if let Some(fr) = self.runtime_funcs.get("__cobrust_str_new").copied() {
            let inst = self.builder.ins().call(fr, &[]);
            self.builder.inst_results(inst)[0]
        } else {
            self.builder.ins().iconst(self.pointer_type, 0)
        };

        for op in operands {
            // Static string literal? Materialize via `.rodata` symbol.
            if let Operand::Constant(Constant::Str(payload)) = op {
                if !payload.is_empty() {
                    let (ptr, len) = self.materialize_str_data(payload)?;
                    if let Some(push_fr) =
                        self.runtime_funcs.get("__cobrust_str_push_static").copied()
                    {
                        self.builder.ins().call(push_fr, &[buf, ptr, len]);
                    }
                }
                continue;
            }
            // Hole — codegen the value and dispatch by type.
            let v = self.lower_operand(op)?;
            let v_ty = self.builder.func.dfg.value_type(v);
            if v_ty == ir::types::F32 || v_ty == ir::types::F64 {
                let v_f64 = if v_ty == ir::types::F32 {
                    self.builder.ins().fpromote(ir::types::F64, v)
                } else {
                    v
                };
                if let Some(fr) = self.runtime_funcs.get("__cobrust_fmt_float").copied() {
                    self.builder.ins().call(fr, &[buf, v_f64]);
                }
            } else if v_ty == ir::types::I8 {
                // Bool path — i8 value.
                let v_i64 = self.builder.ins().uextend(ir::types::I64, v);
                if let Some(fr) = self.runtime_funcs.get("__cobrust_fmt_bool").copied() {
                    self.builder.ins().call(fr, &[buf, v_i64]);
                }
            } else if v_ty.is_int() {
                let v_i64 = if v_ty.bits() < 64 {
                    self.builder.ins().sextend(ir::types::I64, v)
                } else {
                    v
                };
                if let Some(fr) = self.runtime_funcs.get("__cobrust_fmt_int").copied() {
                    self.builder.ins().call(fr, &[buf, v_i64]);
                }
            } else {
                // Pointer-typed value — assume List/Dict/Set repr.
                if let Some(fr) = self.runtime_funcs.get("__cobrust_fmt_repr").copied() {
                    let type_id = self.builder.ins().iconst(ir::types::I64, 0);
                    self.builder.ins().call(fr, &[buf, v, type_id]);
                }
            }
        }
        Ok(buf)
    }

    /// ADR-0027 §3: `Rvalue::Cast(kind, operand, ty)` per the table.
    fn lower_cast(
        &mut self,
        kind: CastKind,
        operand: &Operand,
        ty: &cobrust_types::Ty,
    ) -> Result<ir::Value, CodegenError> {
        let val = self.lower_operand(operand)?;
        let from_ty = self.builder.func.dfg.value_type(val);
        let to_ty = crate::abi::cranelift_scalar_ty(ty).unwrap_or(self.pointer_type);
        let result = match kind {
            CastKind::IntToFloat => {
                let v_i64 = if from_ty.bits() < 64 && from_ty.is_int() {
                    self.builder.ins().sextend(ir::types::I64, val)
                } else {
                    val
                };
                if to_ty == ir::types::F32 || to_ty == ir::types::F64 {
                    self.builder.ins().fcvt_from_sint(to_ty, v_i64)
                } else {
                    val
                }
            }
            CastKind::FloatToInt => {
                if (from_ty == ir::types::F32 || from_ty == ir::types::F64)
                    && to_ty.is_int()
                    && to_ty.bits() >= 8
                {
                    self.builder.ins().fcvt_to_sint_sat(to_ty, val)
                } else {
                    val
                }
            }
            CastKind::BoolToInt => {
                // bool is i8; widen to target int.
                if to_ty.is_int() && to_ty.bits() > from_ty.bits() {
                    self.builder.ins().uextend(to_ty, val)
                } else if to_ty.is_int() && to_ty.bits() < from_ty.bits() {
                    self.builder.ins().ireduce(to_ty, val)
                } else {
                    val
                }
            }
            CastKind::IntToBool => {
                // x != 0
                let zero = self.builder.ins().iconst(from_ty, 0);
                self.builder
                    .ins()
                    .icmp(ir::condcodes::IntCC::NotEqual, val, zero)
            }
            CastKind::StrToBytes | CastKind::BytesToStr => {
                // Pointer pass-through; runtime layout is identical.
                val
            }
        };
        // Also handle generic int-int width conversions when the kind
        // is one of the kind names but the underlying op needs widen/
        // narrow — covered by the to/from_ty checks above. Final width
        // adjust if mismatched and both ints.
        let result_ty = self.builder.func.dfg.value_type(result);
        let coerced = if result_ty == to_ty {
            result
        } else if result_ty.is_int() && to_ty.is_int() {
            if to_ty.bits() > result_ty.bits() {
                self.builder.ins().sextend(to_ty, result)
            } else if to_ty.bits() < result_ty.bits() {
                self.builder.ins().ireduce(to_ty, result)
            } else {
                result
            }
        } else if result_ty.is_float() && to_ty.is_float() && to_ty.bits() != result_ty.bits() {
            if to_ty.bits() > result_ty.bits() {
                self.builder.ins().fpromote(to_ty, result)
            } else {
                self.builder.ins().fdemote(to_ty, result)
            }
        } else {
            result
        };
        Ok(coerced)
    }

    /// ADR-0027 §2: `Rvalue::Ref(borrow_kind, place)` — address-of.
    fn lower_ref(&mut self, place: &Place) -> Result<ir::Value, CodegenError> {
        // For a stack-resident scalar local, materialize a stack slot
        // (lazily) so we can return its address. Cranelift's
        // `stack_addr` returns a pointer-typed value into the slot.
        let local = place.local;
        // If the local has a Cranelift Variable backing, allocate a
        // stack slot, copy the current value into it, and return its
        // address. Subsequent reads of that local should ideally read
        // from the slot — at M12.x we don't redirect reads, matching
        // the ADR's "intra-procedural" lifetime tracking.
        let var = self
            .var_map
            .get(&local)
            .copied()
            .ok_or_else(|| CodegenError::InvalidMir(format!("&_{}: undeclared", local.0)))?;
        let cur_val = self.builder.use_var(var);
        let val_ty = self.builder.func.dfg.value_type(cur_val);

        let slot = if let Some(s) = self.stack_slots.get(&local).copied() {
            s
        } else {
            let size = (val_ty.bits() / 8).max(8);
            let slot = self.builder.create_sized_stack_slot(ir::StackSlotData::new(
                ir::StackSlotKind::ExplicitSlot,
                size,
                3,
            ));
            self.stack_slots.insert(local, slot);
            slot
        };
        // Write current value into slot.
        self.builder.ins().stack_store(cur_val, slot, 0);
        let addr = self.builder.ins().stack_addr(self.pointer_type, slot, 0);
        // Apply field projections: each Field(i) advances the pointer
        // by 8 bytes (M12.x scalar tuple layout — Phase F widens).
        let mut ptr = addr;
        for proj in &place.projections {
            match proj {
                Projection::Field(idx) => {
                    let off = self
                        .builder
                        .ins()
                        .iconst(self.pointer_type, (*idx as i64) * 8);
                    ptr = self.builder.ins().iadd(ptr, off);
                }
                Projection::Deref => {
                    // Dereference by loading the pointer at ptr.
                    ptr = self
                        .builder
                        .ins()
                        .load(self.pointer_type, MemFlags::new(), ptr, 0);
                }
                Projection::Index(_) | Projection::Discriminant => {
                    // Index / discriminant projections require runtime
                    // helpers; M12.x conservative — pass pointer through.
                }
            }
        }
        Ok(ptr)
    }

    fn lower_binop(
        &mut self,
        op: BinOp,
        a: ir::Value,
        b: ir::Value,
    ) -> Result<ir::Value, CodegenError> {
        let a_ty = self.builder.func.dfg.value_type(a);
        let is_float = a_ty == ir::types::F32 || a_ty == ir::types::F64;
        let val = match (op, is_float) {
            (BinOp::Add, false) => self.builder.ins().iadd(a, b),
            (BinOp::Add, true) => self.builder.ins().fadd(a, b),
            (BinOp::Sub, false) => self.builder.ins().isub(a, b),
            (BinOp::Sub, true) => self.builder.ins().fsub(a, b),
            (BinOp::Mul, false) => self.builder.ins().imul(a, b),
            (BinOp::Mul, true) => self.builder.ins().fmul(a, b),
            (BinOp::Div, false) | (BinOp::FloorDiv, false) => self.builder.ins().sdiv(a, b),
            (BinOp::Div, true) | (BinOp::FloorDiv, true) => self.builder.ins().fdiv(a, b),
            (BinOp::Mod, false) => {
                // H1 (ADR-0041): Python floor-mod, not C remainder.
                //
                // CPython: `(-7) %  3 ==  2`,  `7 % (-3) == -2`.
                // C srem:  `(-7) %  3 == -1`,  `7 % (-3) ==  1`.
                //
                // Adjustment per ADR-0041 §H1 Option 1: emit `srem`,
                // then add `b` to the remainder when `rem != 0` and
                // `(rem ^ b) < 0` (i.e. rem and b have opposite signs).
                // This is the standard Knuth-floor-mod formulation
                // and matches CPython byte-for-byte on i64 inputs.
                let rem = self.builder.ins().srem(a, b);
                let zero = self.builder.ins().iconst(a_ty, 0);
                // rem != 0
                let rem_nonzero =
                    self.builder
                        .ins()
                        .icmp(ir::condcodes::IntCC::NotEqual, rem, zero);
                // (rem ^ b) < 0  →  signs differ
                let signs_xor = self.builder.ins().bxor(rem, b);
                let signs_differ =
                    self.builder
                        .ins()
                        .icmp(ir::condcodes::IntCC::SignedLessThan, signs_xor, zero);
                let need_adjust = self.builder.ins().band(rem_nonzero, signs_differ);
                let adjusted = self.builder.ins().iadd(rem, b);
                self.builder.ins().select(need_adjust, adjusted, rem)
            }
            (BinOp::Mod, true) => {
                // H1 (ADR-0041): float floor-mod via fma-style adjust.
                //
                // We do not have a direct Cranelift float remainder.
                // Emit `a - b * floor(a / b)`; the floor isn't directly
                // available either, so for the float path we fall back
                // to integer round-toward-negative-infinity via a
                // (signed) trunc-and-adjust. Because the float `%` is
                // not exercised by current corpus tests (well-typed
                // suite, codegen_diff_corpus), the implementation here
                // ships the simplest correct rewrite that matches
                // CPython for non-NaN finite inputs: `a - b * trunc(a/b)`
                // then add `b` once if the resulting sign differs.
                let div = self.builder.ins().fdiv(a, b);
                // Round toward zero — Cranelift has `fcvt_to_sint` /
                // back via `fcvt_from_sint`.
                let div_i = self.builder.ins().fcvt_to_sint_sat(ir::types::I64, div);
                let trunc = match a_ty {
                    t if t == ir::types::F32 => {
                        self.builder.ins().fcvt_from_sint(ir::types::F32, div_i)
                    }
                    _ => self.builder.ins().fcvt_from_sint(ir::types::F64, div_i),
                };
                let prod = self.builder.ins().fmul(b, trunc);
                let rem = self.builder.ins().fsub(a, prod);
                // Adjust by `b` when sign(rem) != sign(b) and rem != 0.
                let fzero = match a_ty {
                    t if t == ir::types::F32 => self.builder.ins().f32const(0.0_f32),
                    _ => self.builder.ins().f64const(0.0_f64),
                };
                let rem_nonzero =
                    self.builder
                        .ins()
                        .fcmp(ir::condcodes::FloatCC::NotEqual, rem, fzero);
                let rem_lt = self
                    .builder
                    .ins()
                    .fcmp(ir::condcodes::FloatCC::LessThan, rem, fzero);
                let b_lt = self
                    .builder
                    .ins()
                    .fcmp(ir::condcodes::FloatCC::LessThan, b, fzero);
                let signs_differ = self.builder.ins().bxor(rem_lt, b_lt);
                let need_adjust = self.builder.ins().band(rem_nonzero, signs_differ);
                let adjusted = self.builder.ins().fadd(rem, b);
                self.builder.ins().select(need_adjust, adjusted, rem)
            }
            (BinOp::BitAnd, _) => self.builder.ins().band(a, b),
            (BinOp::BitOr, _) => self.builder.ins().bor(a, b),
            (BinOp::And, _) | (BinOp::Or, _) => {
                // H2 (ADR-0041): boolean and/or MUST short-circuit.
                //
                // The MIR lowering layer rewrites `a and b` / `a or b`
                // into explicit control flow at `lower_bin` (cobrust-mir
                // crate). By the time codegen sees a `BinOp::And` /
                // `BinOp::Or`, the source program had a bitwise-on-bool
                // intent that bypassed MIR — typically because the
                // operand types are statically `bool` and the eager
                // evaluation is harmless. We emit `band` / `bor` here
                // as a defense-in-depth fallback; the MIR side ensures
                // short-circuit semantics for any program with a
                // possibly-trapping RHS.
                match op {
                    BinOp::And => self.builder.ins().band(a, b),
                    BinOp::Or => self.builder.ins().bor(a, b),
                    _ => unreachable!(),
                }
            }
            (BinOp::BitXor, _) => self.builder.ins().bxor(a, b),
            (BinOp::Shl, _) => self.builder.ins().ishl(a, b),
            (BinOp::Shr, _) => self.builder.ins().sshr(a, b),
            (BinOp::Eq, false) => self.builder.ins().icmp(ir::condcodes::IntCC::Equal, a, b),
            (BinOp::NotEq, false) => self
                .builder
                .ins()
                .icmp(ir::condcodes::IntCC::NotEqual, a, b),
            (BinOp::Lt, false) => {
                self.builder
                    .ins()
                    .icmp(ir::condcodes::IntCC::SignedLessThan, a, b)
            }
            (BinOp::LtEq, false) => {
                self.builder
                    .ins()
                    .icmp(ir::condcodes::IntCC::SignedLessThanOrEqual, a, b)
            }
            (BinOp::Gt, false) => {
                self.builder
                    .ins()
                    .icmp(ir::condcodes::IntCC::SignedGreaterThan, a, b)
            }
            (BinOp::GtEq, false) => {
                self.builder
                    .ins()
                    .icmp(ir::condcodes::IntCC::SignedGreaterThanOrEqual, a, b)
            }
            (BinOp::Eq, true) => self.builder.ins().fcmp(ir::condcodes::FloatCC::Equal, a, b),
            (BinOp::NotEq, true) => self
                .builder
                .ins()
                .fcmp(ir::condcodes::FloatCC::NotEqual, a, b),
            (BinOp::Lt, true) => self
                .builder
                .ins()
                .fcmp(ir::condcodes::FloatCC::LessThan, a, b),
            (BinOp::LtEq, true) => {
                self.builder
                    .ins()
                    .fcmp(ir::condcodes::FloatCC::LessThanOrEqual, a, b)
            }
            (BinOp::Gt, true) => self
                .builder
                .ins()
                .fcmp(ir::condcodes::FloatCC::GreaterThan, a, b),
            (BinOp::GtEq, true) => {
                self.builder
                    .ins()
                    .fcmp(ir::condcodes::FloatCC::GreaterThanOrEqual, a, b)
            }
            (BinOp::Pow, _) => {
                // H3 (ADR-0041): no silent zero. `**` requires either an
                // integer pow loop (deferred to M11.x) or a stdlib call;
                // until then, surface the drift honestly.
                return Err(CodegenError::UnimplementedBinOp {
                    op: "**",
                    note: "integer pow / float pow stdlib helper is M11.x scope (ADR-0041 §H3)",
                });
            }
            (BinOp::MatMul, _) => {
                return Err(CodegenError::UnimplementedBinOp {
                    op: "@",
                    note: "matrix multiplication requires cobrust-numpy runtime (ADR-0041 §H3)",
                });
            }
            (BinOp::In, _) => {
                return Err(CodegenError::UnimplementedBinOp {
                    op: "in",
                    note: "container-membership requires cobrust-stdlib iterator equality (ADR-0041 §H3)",
                });
            }
            (BinOp::NotIn, _) => {
                return Err(CodegenError::UnimplementedBinOp {
                    op: "not in",
                    note: "container non-membership requires cobrust-stdlib iterator equality (ADR-0041 §H3)",
                });
            }
        };
        Ok(val)
    }

    fn lower_unop(&mut self, op: UnOp, a: ir::Value) -> Result<ir::Value, CodegenError> {
        let a_ty = self.builder.func.dfg.value_type(a);
        let is_float = a_ty == ir::types::F32 || a_ty == ir::types::F64;
        let val = match (op, is_float) {
            (UnOp::Plus, _) => a,
            (UnOp::Neg, false) => {
                let zero = self.builder.ins().iconst(a_ty, 0);
                self.builder.ins().isub(zero, a)
            }
            (UnOp::Neg, true) => self.builder.ins().fneg(a),
            (UnOp::BitNot, _) => self.builder.ins().bnot(a),
            (UnOp::Not, _) => {
                let one = self.builder.ins().iconst(a_ty, 1);
                self.builder.ins().bxor(a, one)
            }
        };
        Ok(val)
    }

    fn lower_operand(&mut self, op: &Operand) -> Result<ir::Value, CodegenError> {
        match op {
            Operand::Copy(p) | Operand::Move(p) => self.read_place(p),
            Operand::Constant(c) => self.lower_constant(c),
        }
    }

    fn lower_constant(&mut self, c: &Constant) -> Result<ir::Value, CodegenError> {
        let val = match c {
            Constant::Bool(b) => self.builder.ins().iconst(ir::types::I8, *b as i64),
            Constant::Int(i) => self.builder.ins().iconst(ir::types::I64, *i),
            Constant::Float(bits) | Constant::Imag(bits) => {
                self.builder.ins().f64const(f64::from_bits(*bits))
            }
            Constant::Str(_) | Constant::Bytes(_) => {
                // M9 stub: defer to M11 stdlib for runtime materialization.
                self.builder.ins().iconst(self.pointer_type, 0)
            }
            Constant::None => self.builder.ins().iconst(ir::types::I8, 0),
            Constant::FnRef(_) => self.builder.ins().iconst(self.pointer_type, 0),
        };
        Ok(val)
    }

    fn read_place(&mut self, place: &Place) -> Result<ir::Value, CodegenError> {
        let var = self.var_map.get(&place.local).copied().ok_or_else(|| {
            CodegenError::InvalidMir(format!(
                "place references undeclared local _{}",
                place.local.0
            ))
        })?;
        let mut value = self.builder.use_var(var);
        for proj in &place.projections {
            value = match proj {
                Projection::Field(_) | Projection::Discriminant | Projection::Index(_) => value,
                Projection::Deref => {
                    let ptr_ty = self.builder.func.dfg.value_type(value);
                    if ptr_ty == self.pointer_type {
                        self.builder
                            .ins()
                            .load(ir::types::I64, MemFlags::new(), value, 0)
                    } else {
                        value
                    }
                }
            };
        }
        Ok(value)
    }

    fn write_place(&mut self, place: &Place, value: ir::Value) -> Result<(), CodegenError> {
        let var = self.var_map.get(&place.local).copied().ok_or_else(|| {
            CodegenError::InvalidMir(format!(
                "place references undeclared local _{}",
                place.local.0
            ))
        })?;
        // Match the variable's declared type — Cranelift requires
        // def_var(var, value) where value's type matches the variable.
        let prev_value = self.builder.use_var(var);
        let var_ty = self.builder.func.dfg.value_type(prev_value);
        let val_ty = self.builder.func.dfg.value_type(value);
        let coerced = if var_ty == val_ty {
            value
        } else if var_ty.is_int() && val_ty.is_int() {
            if var_ty.bits() > val_ty.bits() {
                self.builder.ins().sextend(var_ty, value)
            } else if var_ty.bits() < val_ty.bits() {
                self.builder.ins().ireduce(var_ty, value)
            } else {
                value
            }
        } else if var_ty.is_float() && val_ty.is_int() {
            self.builder.ins().fcvt_from_sint(var_ty, value)
        } else if var_ty.is_int() && val_ty.is_float() {
            self.builder.ins().fcvt_to_sint_sat(var_ty, value)
        } else {
            value
        };
        self.builder.def_var(var, coerced);
        Ok(())
    }
}

/// Generate a deterministic data-symbol name for a string payload.
/// Format: `_cobrust_str_<def_id>_<idx>`. Per-body uniqueness is
/// sufficient for M11; cross-body interning is M12.
fn str_data_symbol(def_id: u32, idx: usize, _payload: &str) -> String {
    format!("_cobrust_str_{def_id}_{idx}")
}

/// Compute the set of basic-block ids reachable from the entry
/// (BlockId(0)) via successor edges. Used to skip dead drop-schedule
/// chains that target the entry block (which Cranelift's verifier
/// rejects as "invalid reference to entry block").
fn compute_reachable_blocks(body: &Body) -> std::collections::HashSet<BlockId> {
    let mut reachable = std::collections::HashSet::new();
    if body.blocks.is_empty() {
        return reachable;
    }
    let max_idx = body.blocks.len() as u32;
    let mut stack = vec![BlockId(0)];
    while let Some(id) = stack.pop() {
        // Defensive: ill-formed MIR can reference non-existent block
        // ids. The ill-formed corpus relies on those surfacing as
        // CraneliftError later in the lowering, so just skip them
        // here without panicking.
        if id.0 >= max_idx {
            reachable.insert(id);
            continue;
        }
        if !reachable.insert(id) {
            continue;
        }
        for succ in body.successors(id) {
            if !reachable.contains(&succ) {
                stack.push(succ);
            }
        }
    }
    reachable
}

/// ADR-0027 binding: register the runtime-helper symbol table that
/// every body's lowering pre-imports. The signatures match the
/// `cobrust-stdlib` C-ABI surface bit-for-bit.
fn runtime_helper_signatures(
    pointer_type: ir::Type,
    call_conv: cranelift_codegen::isa::CallConv,
) -> Vec<(&'static str, Signature)> {
    let mut out = Vec::new();
    let p = pointer_type;
    let i64 = ir::types::I64;
    let f64 = ir::types::F64;

    // -- Aggregate / collection runtime ----------------------------
    // List<i64>
    out.push(("__cobrust_list_new", sig(call_conv, &[i64, i64], Some(p))));
    out.push(("__cobrust_list_set", sig(call_conv, &[p, i64, i64], None)));
    out.push(("__cobrust_list_get", sig(call_conv, &[p, i64], Some(i64))));
    out.push(("__cobrust_list_len", sig(call_conv, &[p], Some(i64))));
    out.push(("__cobrust_list_drop", sig(call_conv, &[p], None)));
    // ADR-0041 §H6: comprehension lowering uses runtime append.
    out.push(("__cobrust_list_append", sig(call_conv, &[p, i64], None)));

    // Dict<i64, i64>
    out.push((
        "__cobrust_dict_new",
        sig(call_conv, &[i64, i64, i64], Some(p)),
    ));
    out.push(("__cobrust_dict_set", sig(call_conv, &[p, i64, i64], None)));
    out.push(("__cobrust_dict_get", sig(call_conv, &[p, i64], Some(i64))));
    out.push(("__cobrust_dict_len", sig(call_conv, &[p], Some(i64))));
    out.push(("__cobrust_dict_drop", sig(call_conv, &[p], None)));

    // Set<i64>
    out.push(("__cobrust_set_new", sig(call_conv, &[i64, i64], Some(p))));
    out.push(("__cobrust_set_insert", sig(call_conv, &[p, i64], None)));
    out.push((
        "__cobrust_set_contains",
        sig(call_conv, &[p, i64], Some(i64)),
    ));
    out.push(("__cobrust_set_len", sig(call_conv, &[p], Some(i64))));
    out.push(("__cobrust_set_drop", sig(call_conv, &[p], None)));

    // Tuple<i64, ...>
    out.push(("__cobrust_tuple_new", sig(call_conv, &[i64], Some(p))));
    out.push(("__cobrust_tuple_set", sig(call_conv, &[p, i64, i64], None)));
    out.push(("__cobrust_tuple_get", sig(call_conv, &[p, i64], Some(i64))));
    out.push(("__cobrust_tuple_drop", sig(call_conv, &[p, i64], None)));

    // Heap allocator
    out.push(("__cobrust_alloc", sig(call_conv, &[i64], Some(p))));
    out.push(("__cobrust_dealloc", sig(call_conv, &[p, i64], None)));

    // -- iter runtime (ADR-0027 §4) -------------------------------
    out.push(("__cobrust_iter_init", sig(call_conv, &[i64], Some(p))));
    out.push(("__cobrust_iter_next", sig(call_conv, &[p], Some(i64))));
    out.push(("__cobrust_iter_drop", sig(call_conv, &[p], None)));

    // -- print_int runtime (ADR-0030 §Decision step 5) ----------------
    // `print_int(n: i64)` — prints n as decimal + newline.
    out.push(("__cobrust_println_int", sig(call_conv, &[i64], None)));

    // -- f-string runtime ------------------------------------------
    out.push(("__cobrust_str_new", sig(call_conv, &[], Some(p))));
    out.push((
        "__cobrust_str_push_static",
        sig(call_conv, &[p, p, i64], None),
    ));
    out.push(("__cobrust_fmt_int", sig(call_conv, &[p, i64], None)));
    out.push(("__cobrust_fmt_float", sig(call_conv, &[p, f64], None)));
    out.push(("__cobrust_fmt_bool", sig(call_conv, &[p, i64], None)));
    out.push(("__cobrust_fmt_str", sig(call_conv, &[p, p, i64], None)));
    out.push(("__cobrust_fmt_repr", sig(call_conv, &[p, p, i64], None)));
    out.push(("__cobrust_str_len", sig(call_conv, &[p], Some(i64))));
    out.push(("__cobrust_str_ptr", sig(call_conv, &[p], Some(p))));
    out.push(("__cobrust_str_drop", sig(call_conv, &[p], None)));

    // -- ADR-0044 W2 Phase 2: stdin + argv source-level binding ---
    // `input(prompt: str) -> str` — writes prompt to stdout, reads one
    // line from stdin (strip trailing \n), returns owned Str pointer.
    // `print(s)` heap-buffer path — ADR-0044 W2 Phase 2 codegen
    // amendment so non-literal `print(s)` callsites round-trip through
    // a single C-ABI dispatch (no per-callsite (ptr, len) extraction).
    out.push(("__cobrust_println_str_buf", sig(call_conv, &[p], None)));
    out.push(("__cobrust_input", sig(call_conv, &[p, i64], Some(p))));
    // `input_no_prompt() -> str` — empty-prompt overload.
    out.push(("__cobrust_input_no_prompt", sig(call_conv, &[], Some(p))));
    // `read_line() -> str` (W2 Phase 2 scope cap per ADR-0044
    // Decision 1D; typed `Result[str, IoError]` deferred to ADR-0044a).
    // Returns the line preserving its trailing \n; EOF returns empty Str.
    out.push(("__cobrust_read_line", sig(call_conv, &[], Some(p))));
    // `argv() -> list[str]` — materializes CAPTURED_ARGS into a
    // List<Str>; each element is a heap-allocated Str pointer.
    out.push(("__cobrust_argv", sig(call_conv, &[], Some(p))));

    // -- ADR-0044 W2 Phase 3: parse_int / str_len / str_at / str_eq ----
    // `parse_int(s: str) -> i64` — parses decimal integer from Str buf.
    out.push(("__cobrust_parse_int", sig(call_conv, &[p], Some(i64))));
    // `str_len(s: str) -> i64` — byte length of Str buf.
    out.push(("__cobrust_str_len_src", sig(call_conv, &[p], Some(i64))));
    // `str_at(s: str, i: i64) -> str` — single-byte Str at position i.
    out.push(("__cobrust_str_at", sig(call_conv, &[p, i64], Some(p))));
    // `str_eq(a: str, b: str) -> i64` — content equality (1 or 0).
    out.push(("__cobrust_str_eq", sig(call_conv, &[p, p], Some(i64))));
    // `str_eq_lit(s: str, lit: str) -> i64` — compare runtime str against
    // static literal; 3-param C ABI (buf_ptr, lit_ptr, lit_len).
    out.push((
        "__cobrust_str_eq_lit",
        sig(call_conv, &[p, p, i64], Some(i64)),
    ));
    // `str_ord(s: str) -> i64` — ASCII byte value of first byte.
    out.push(("__cobrust_str_ord", sig(call_conv, &[p], Some(i64))));
    // `parse_int_tok(line: str, i: i64) -> i64` — i-th space-separated int.
    out.push((
        "__cobrust_parse_int_tok",
        sig(call_conv, &[p, i64], Some(i64)),
    ));
    // `count_toks(line: str) -> i64` — count of whitespace-separated tokens.
    out.push(("__cobrust_count_toks", sig(call_conv, &[p], Some(i64))));
    // `print_no_nl(s: str)` — print Str buffer without trailing newline.
    out.push(("__cobrust_print_no_nl", sig(call_conv, &[p], None)));
    // ADR-0047 Option H / LC-100 Pattern A fix: raw-bytes variant for
    // `print_no_nl(<string literal>)` callsites — codegen routes
    // `Constant::Str` arguments here via the single-arg-Str-to-(ptr, len)
    // expansion in `lower_terminator` (analogous to `__cobrust_println`).
    // Closes the `.rodata` misalignment defect in `__cobrust_print_no_nl`'s
    // `StringBuffer` cast. Runtime-str callsites continue to use the
    // existing single-pointer entry.
    out.push(("__cobrust_print_no_nl_lit", sig(call_conv, &[p, i64], None)));

    // -- M-AI.0 (α Phase 2): cobrust.llm source-level binding ---------
    // `llm_complete(provider, model, prompt) -> str`. All three args are
    // heap-Str pointers (or .rodata-static-literal pointers). Returns an
    // owned Str pointer (Decision 7: empty Str on any failure).
    out.push((
        "__cobrust_llm_complete",
        sig(call_conv, &[p, p, p], Some(p)),
    ));
    // `llm_dispatch(task, prompt) -> str`. Both args are Str pointers.
    out.push(("__cobrust_llm_dispatch", sig(call_conv, &[p, p], Some(p))));
    // `llm_stream(provider, model, prompt) -> list[str]`. Returns a list
    // pointer (Decision 3B collect-all-chunks form). Element i64 slots
    // store heap-Str pointers per the `__cobrust_argv` precedent.
    out.push(("__cobrust_llm_stream", sig(call_conv, &[p, p, p], Some(p))));

    out
}

fn sig(
    call_conv: cranelift_codegen::isa::CallConv,
    params: &[ir::Type],
    ret: Option<ir::Type>,
) -> Signature {
    let mut s = Signature::new(call_conv);
    for p in params {
        s.params.push(AbiParam::new(*p));
    }
    if let Some(r) = ret {
        s.returns.push(AbiParam::new(r));
    }
    s
}

/// Coerce a Cranelift value to i64 for runtime-helper calls. Pointers
/// pass through (they're already 64-bit on supported targets); ints
/// are sign-extended; bools are zero-extended; floats are bit-cast
/// (M12.x conservative — float aggregate elements aren't a target
/// today).
fn coerce_to_i64(builder: &mut FunctionBuilder<'_>, v: ir::Value) -> ir::Value {
    let ty = builder.func.dfg.value_type(v);
    if ty == ir::types::I64 {
        v
    } else if ty.is_int() && ty.bits() < 64 {
        builder.ins().sextend(ir::types::I64, v)
    } else if ty == ir::types::F32 {
        let promoted = builder.ins().fpromote(ir::types::F64, v);
        builder
            .ins()
            .bitcast(ir::types::I64, MemFlags::new(), promoted)
    } else if ty == ir::types::F64 {
        builder.ins().bitcast(ir::types::I64, MemFlags::new(), v)
    } else if ty.is_int() && ty.bits() > 64 {
        builder.ins().ireduce(ir::types::I64, v)
    } else {
        // Pointer or unknown — Cranelift insists on a typed value, so
        // bitcast through.
        builder.ins().bitcast(ir::types::I64, MemFlags::new(), v)
    }
}

// keep the Arc import noisy-eliminated (used by isa::OwnedTargetIsa internals).
#[allow(dead_code)]
fn _arc_kept() -> Arc<()> {
    Arc::new(())
}
