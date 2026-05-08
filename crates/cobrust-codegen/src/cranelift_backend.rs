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
                if let Some(cobrust_mir::Operand::Constant(cobrust_mir::Constant::Str(payload))) =
                    args.first()
                {
                    if str_data_ids.contains_key(payload) {
                        continue;
                    }
                    // Generate a unique symbol name from a payload hash
                    // so identical payloads in different bodies share
                    // the same data symbol (cross-body interning is a
                    // future tweak; for M11 we scope per-body).
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
                // (M11) `_args` is read inside the Constant::Str branch
                // below; keep underscore prefix to match the existing
                // signature shape but use it via the `args.first()`
                // lookup.
                // ADR-0024 amendment: when `func` is `Constant::Str(name)`,
                // emit a real Cranelift `call` to the imported symbol.
                // For all other callee shapes (Constant::FnRef, ...) the
                // M9 stub remains: write a zero placeholder, jump to
                // continuation. M11 will materialize the FnRef path.
                if let Operand::Constant(Constant::Str(name)) = func {
                    // ADR-0027 §4: prefer runtime-helper FuncRef when
                    // the callee name matches the typed-signature
                    // table.
                    if let Some(func_ref) = self.runtime_funcs.get(name.as_str()).copied() {
                        // Lower each arg. For Constant::Str args we
                        // materialize the rodata pointer (push_static
                        // pattern); other operands lower directly.
                        let mut call_args = Vec::with_capacity(args.len());
                        for arg in args {
                            if let Operand::Constant(Constant::Str(payload)) = arg {
                                let (ptr, _len) = self.materialize_str_data(payload)?;
                                call_args.push(ptr);
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
