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
    AssertKind, BasicBlock, BinOp, BlockId, Body, Constant, LocalId, Module, Operand, Place,
    Projection, Rvalue, Statement, StatementKind, SwitchValue, Terminator, UnOp,
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

    let mut ctx = CraneliftCtx {
        pointer_type,
        call_conv,
        function_ids: HashMap::new(),
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
}

impl CraneliftCtx {
    fn body_signature(&self, body: &Body) -> Signature {
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
        let ret_ty = self.infer_return_type(body).unwrap_or(self.pointer_type);
        sig.returns.push(AbiParam::new(ret_ty));
        sig
    }

    /// Infer the actual return type by scanning the body for the
    /// last assignment to the return local. The MIR puts `_return:
    /// Ty::None` regardless of the real signature, so codegen has
    /// to reconstruct the type from the dataflow.
    fn infer_return_type(&self, body: &Body) -> Option<ir::Type> {
        // Walk every block + statement; the return local's RHS type
        // is what we want.
        for block in &body.blocks {
            for stmt in &block.statements {
                if let cobrust_mir::StatementKind::Assign { place, rvalue } = &stmt.kind {
                    if place.local == body.return_local && place.projections.is_empty() {
                        if let Some(ty) = self.rvalue_ty(body, rvalue) {
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

    fn rvalue_ty(&self, body: &Body, rvalue: &cobrust_mir::Rvalue) -> Option<ir::Type> {
        match rvalue {
            cobrust_mir::Rvalue::Use(op) => self.operand_ty(body, op),
            cobrust_mir::Rvalue::BinaryOp(op, a, _b) => {
                use cobrust_mir::BinOp::*;
                match op {
                    Eq | NotEq | Lt | LtEq | Gt | GtEq => Some(ir::types::I8),
                    And | Or => Some(ir::types::I8),
                    In | NotIn => Some(ir::types::I8),
                    _ => self.operand_ty(body, a),
                }
            }
            cobrust_mir::Rvalue::UnaryOp(_, a) => self.operand_ty(body, a),
            cobrust_mir::Rvalue::Cast(_, _, ty) => cranelift_scalar_ty(ty),
            cobrust_mir::Rvalue::Aggregate(_, _) | cobrust_mir::Rvalue::Ref(_, _) => {
                Some(self.pointer_type)
            }
            cobrust_mir::Rvalue::Discriminant(_)
            | cobrust_mir::Rvalue::Len(_)
            | cobrust_mir::Rvalue::NullaryOp(_) => Some(ir::types::I64),
        }
    }

    fn operand_ty(&self, body: &Body, op: &cobrust_mir::Operand) -> Option<ir::Type> {
        use cobrust_mir::{Constant, Operand};
        match op {
            Operand::Copy(p) | Operand::Move(p) => body
                .locals
                .get(p.local.0 as usize)
                .and_then(|l| cranelift_scalar_ty(&l.ty)),
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
    fn infer_local_types(&self, body: &Body) -> HashMap<LocalId, ir::Type> {
        let mut out: HashMap<LocalId, ir::Type> = HashMap::new();
        for local in &body.locals {
            // Only infer for locals where the declared Ty maps unhelpfully
            // (Ty::None and the like). For Ty::Int, Ty::Float, Ty::Bool
            // the declared type is authoritative.
            if cranelift_scalar_ty(&local.ty).is_none() {
                // Will fill below; keep map sentinel so we know we want it.
            } else if !matches!(local.ty, cobrust_types::Ty::None) {
                continue;
            }
            // For each candidate, scan body statements.
            for block in &body.blocks {
                for stmt in &block.statements {
                    if let cobrust_mir::StatementKind::Assign { place, rvalue } = &stmt.kind {
                        if place.local == local.id && place.projections.is_empty() {
                            if let Some(ty) = self.rvalue_ty(body, rvalue) {
                                out.insert(local.id, ty);
                            }
                        }
                    }
                }
            }
        }
        out
    }

    fn declare_body(&mut self, obj: &mut ObjectModule, body: &Body) -> Result<(), CodegenError> {
        let sig = self.body_signature(body);
        let name = if body.name.is_empty() {
            format!("_cobrust_init_{}", body.def_id.0)
        } else {
            body.name.clone()
        };

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

        let sig = self.body_signature(body);
        let mut function = Function::with_name_signature(UserFuncName::user(0, body.def_id.0), sig);

        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_ctx);

        // --- create one Cranelift block per MIR block ----------------
        let mut block_map: HashMap<BlockId, ir::Block> = HashMap::new();
        for (idx, mir_block) in body.blocks.iter().enumerate() {
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
        let inferred_ret = self.infer_return_type(body).unwrap_or(self.pointer_type);
        let inferred_locals = self.infer_local_types(body);
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
        let mut extern_func_ids: HashMap<String, cranelift_module::FuncId> = HashMap::new();
        for mir_block in &body.blocks {
            if let cobrust_mir::Terminator::Call { func, .. } = &mir_block.terminator {
                if let cobrust_mir::Operand::Constant(cobrust_mir::Constant::Str(name)) = func {
                    if extern_func_ids.contains_key(name) {
                        continue;
                    }
                    // M10 runtime-helper signature: void (void). M11
                    // will widen to `(*const u8, usize)` for general
                    // `print(s: str)` lowering.
                    let sig = Signature::new(self.call_conv);
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

        // --- lower each block's statements + terminator --------------
        let mut to_emit: Vec<&BasicBlock> = body.blocks.iter().collect();
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
    /// stored in this map. Used by the M10 hello-world contract
    /// (runtime helper `__cobrust_println_static`).
    extern_funcs: &'a HashMap<String, ir::FuncRef>,
}

impl<'a, 'b> EmitCtx<'a, 'b> {
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
                args: _args,
                destination,
                target,
                unwind: _,
            } => {
                // ADR-0024 amendment: when `func` is `Constant::Str(name)`,
                // emit a real Cranelift `call` to the imported symbol.
                // For all other callee shapes (Constant::FnRef, ...) the
                // M9 stub remains: write a zero placeholder, jump to
                // continuation. M11 will materialize the FnRef path.
                if let Operand::Constant(Constant::Str(name)) = func {
                    if let Some(func_ref) = self.extern_funcs.get(name) {
                        // M10 runtime-helper convention: zero args, void
                        // return. The destination receives a zero (the
                        // call's return value is unused at the M10 scope).
                        self.builder.ins().call(*func_ref, &[]);
                        let zero = self.builder.ins().iconst(ir::types::I64, 0);
                        self.write_place(destination, zero)?;
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
        let mut current_otherwise = otherwise_blk;
        for (i, (val, target)) in cases.iter().enumerate().rev() {
            let case_blk = self.block_id(target)?;
            let test_val = match val {
                SwitchValue::Bool(b) => self.builder.ins().iconst(ir::types::I8, *b as i64),
                SwitchValue::Int(v) => self.builder.ins().iconst(ir::types::I64, *v),
                SwitchValue::Adt(d) => self.builder.ins().iconst(ir::types::I32, *d as i64),
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
            Rvalue::Aggregate(_, _) => {
                // M9 stub: aggregates lower to a zero pointer; full
                // materialization belongs in M11 stdlib.
                Ok(self.builder.ins().iconst(self.pointer_type, 0))
            }
            Rvalue::Cast(_, op, _ty) => {
                // M9 stub: forward the operand without conversion.
                self.lower_operand(op)
            }
            Rvalue::Ref(_, _place) => {
                // M9 stub: a borrow returns a null pointer placeholder.
                Ok(self.builder.ins().iconst(self.pointer_type, 0))
            }
            Rvalue::Discriminant(_) | Rvalue::Len(_) | Rvalue::NullaryOp(_) => {
                Ok(self.builder.ins().iconst(ir::types::I64, 0))
            }
        }
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
            (BinOp::Mod, false) => self.builder.ins().srem(a, b),
            (BinOp::Mod, true) => {
                // No direct Cranelift float remainder; M9 stub.
                self.builder.ins().fsub(a, a)
            }
            (BinOp::BitAnd | BinOp::And, _) => self.builder.ins().band(a, b),
            (BinOp::BitOr | BinOp::Or, _) => self.builder.ins().bor(a, b),
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
            (BinOp::Pow | BinOp::MatMul | BinOp::In | BinOp::NotIn, _) => {
                // M9 stub: defer to M11 runtime helper.
                self.builder.ins().iconst(ir::types::I64, 0)
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

// keep the Arc import noisy-eliminated (used by isa::OwnedTargetIsa internals).
#[allow(dead_code)]
fn _arc_kept() -> Arc<()> {
    Arc::new(())
}
