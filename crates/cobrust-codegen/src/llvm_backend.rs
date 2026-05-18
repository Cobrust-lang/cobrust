//! LLVM backend (per ADR-0058a §3, ADR-0023 §"Backend feature flag layout").
//!
//! Active only when `--features llvm` is set. Wraps `inkwell 0.9 + llvm18-1`
//! to lower MIR into LLVM IR and emit object code via the LLVM 18+
//! toolchain.
//!
//! ADR-0058a wave-1 ships the **core lowering pass** only. Explicit
//! non-goals (deferred per ADR-0058a §8):
//!
//! - Optimization pass pipeline (`OptLevel::Speed` / `SpeedAndSize`)
//!   stays at LLVM `-O0` until sub-ADR 0058b lands.
//! - DWARF debug-info emission (`DIBuilder`, `dbg.declare`) is sub-ADR 0058c.
//! - Multi-target cross-compilation matrix is sub-ADR 0058b.
//!
//! The lowering mirrors `cranelift_backend.rs` semantically:
//!
//! - `LlvmEmitter::declare_body` / `define_body` form the same
//!   two-pass declare-then-define structure.
//! - `Operand` lowering loads from per-local `alloca`s, matching the
//!   Variable-based Cranelift form.
//! - `BinaryOp` / `UnaryOp` dispatch on signed-int vs float.
//! - `Drop` lowers to runtime-helper calls (`__cobrust_str_drop`,
//!   `__cobrust_list_drop_elems`) — same ABI as Cranelift per
//!   ADR-0023 §"Drop-handler ABI".
//! - `Call` honors `Constant::FnRef` (user fns); runtime-helper /
//!   extern-name Call lowering is wave-2.
//!
//! Per-form differences from Cranelift live next to each `lower_*` fn.

use std::collections::HashMap;
use std::path::PathBuf;

use cobrust_mir::{
    AggregateKind, BinOp, BlockId, Body, CastKind, Constant, LocalId, Module, Operand, Place,
    Projection, Rvalue, Statement, StatementKind, SwitchValue, Terminator, UnOp,
};
use cobrust_types::Ty;

use inkwell::AddressSpace;
use inkwell::FloatPredicate;
use inkwell::IntPredicate;
use inkwell::OptimizationLevel;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module as LlvmModule};
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple,
};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue,
};

use crate::artifact::{Artifact, ArtifactKind};
use crate::error::CodegenError;
use crate::linker;
use crate::target::{OptLevel, TargetSpec};

// =====================================================================
// Public entry — `llvm_backend::emit`
// =====================================================================

/// Public LLVM backend entrypoint. Mirrors `cranelift_backend::emit`.
///
/// Lowers `module` into LLVM IR via `inkwell`, writes a relocatable
/// object via `TargetMachine::write_to_file`, then delegates linking
/// to `crate::linker` (same path Cranelift uses).
///
/// # Errors
///
/// Returns [`CodegenError::LlvmError`] / [`CodegenError::ObjectEmission`] /
/// [`CodegenError::UnsupportedTarget`] / [`CodegenError::LinkerFailed`]
/// per ADR-0023's variant table.
pub fn emit(module: &Module, spec: &TargetSpec) -> Result<Artifact, CodegenError> {
    // One Context, scoped to the duration of emit. All inkwell
    // values borrow from this arena — drop ordering enforced by `'ctx`.
    let ctx = Context::create();
    let target_machine = build_target_machine(spec)?;

    // Build emitter (owns Module + Builder via `'ctx` borrowed Context).
    let mut emitter = LlvmEmitter::new(&ctx, spec, &target_machine)?;

    // --- declare every body's signature first ---------------------------
    for body in &module.bodies {
        emitter.declare_body(body)?;
    }

    // --- now define each body --------------------------------------------
    for body in &module.bodies {
        emitter.define_body(body)?;
    }

    // --- finalize: write object file ------------------------------------
    if cfg!(debug_assertions) {
        // ADR-0058a §9.2: dev-mode verifier prints offending IR via
        // structured error.
        if let Err(err) = emitter.module.verify() {
            return Err(CodegenError::LlvmError(format!(
                "LLVM module verify failed: {}",
                err
            )));
        }
    }

    std::fs::create_dir_all(&spec.output_dir)?;
    let object_name = format!("{}.o", spec.module_name);
    let object_path = spec.output_dir.join(object_name);
    target_machine
        .write_to_file(&emitter.module, FileType::Object, &object_path)
        .map_err(|e| CodegenError::ObjectEmission(e.to_string()))?;

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

/// Build a `TargetMachine` for the requested triple + opt level.
fn build_target_machine(spec: &TargetSpec) -> Result<TargetMachine, CodegenError> {
    Target::initialize_all(&InitializationConfig::default());
    let triple = TargetTriple::create(&spec.triple.to_string());
    let target = Target::from_triple(&triple)
        .map_err(|e| CodegenError::UnsupportedTarget(format!("{}: {}", spec.triple, e)))?;
    let opt = match spec.opt_level {
        OptLevel::None => OptimizationLevel::None,
        // ADR-0058a §8 non-goals: opt pipeline stays at -O0 until sub-ADR
        // 0058b. The LLVM backend ignores opt_level beyond None at wave-1.
        OptLevel::Speed | OptLevel::SpeedAndSize => OptimizationLevel::None,
    };
    target
        .create_target_machine(
            &triple,
            "generic",
            "",
            opt,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| {
            CodegenError::LlvmError(format!(
                "failed to create LLVM TargetMachine for {}",
                spec.triple
            ))
        })
}

// =====================================================================
// LlvmEmitter — top-level stateful emitter (parallel to CraneliftCtx)
// =====================================================================

/// Per-emit state. Borrows the inkwell `Context`, `Module`, and
/// `Builder` for the lifetime `'ctx`.
///
/// Mirrors `cranelift_backend::CraneliftCtx`: holds the function-id
/// table + return-type cache so a second-pass `define_body` can
/// resolve forward-declared callees emitted by `declare_body`.
pub struct LlvmEmitter<'ctx> {
    ctx: &'ctx Context,
    /// Owned LLVM module — borrows from `ctx` via `'ctx`.
    pub module: LlvmModule<'ctx>,
    /// Owned LLVM builder — borrows from `ctx` via `'ctx`.
    builder: Builder<'ctx>,
    /// MIR body `def_id` → declared LLVM function.
    function_ids: HashMap<u32, FunctionValue<'ctx>>,
    /// Per-body return type cache (ADR-0034 parallel).
    body_return_types: HashMap<u32, BasicTypeEnum<'ctx>>,
    /// Declared runtime-helper externs (`__cobrust_str_drop`, etc.).
    runtime_helper_decls: HashMap<&'static str, FunctionValue<'ctx>>,
    /// Cached `i8*` opaque pointer type used for str/list/dict/refs.
    opaque_ptr_ty: inkwell::types::PointerType<'ctx>,
}

impl<'ctx> LlvmEmitter<'ctx> {
    /// Construct a new emitter. Pre-declares the runtime-helper externs
    /// used by Drop / Assert lowering (`__cobrust_str_drop`,
    /// `__cobrust_list_drop_elems`, `__cobrust_list_drop`,
    /// `__cobrust_panic`).
    ///
    /// `spec` and `target_machine` drive module-name + triple +
    /// data-layout binding.
    pub fn new(
        ctx: &'ctx Context,
        spec: &TargetSpec,
        target_machine: &TargetMachine,
    ) -> Result<Self, CodegenError> {
        let module = ctx.create_module(&spec.module_name);
        module.set_triple(&target_machine.get_triple());
        module.set_data_layout(&target_machine.get_target_data().get_data_layout());
        let builder = ctx.create_builder();
        let opaque_ptr_ty = ctx.i8_type().ptr_type(AddressSpace::default());

        let mut emitter = LlvmEmitter {
            ctx,
            module,
            builder,
            function_ids: HashMap::new(),
            body_return_types: HashMap::new(),
            runtime_helper_decls: HashMap::new(),
            opaque_ptr_ty,
        };
        emitter.declare_runtime_helpers();
        Ok(emitter)
    }

    /// Pre-declare runtime helpers used by Drop / Assert lowering.
    /// Mirrors `cranelift_backend::runtime_helper_signatures` but only
    /// for the wave-1 surface (drop family + panic).
    fn declare_runtime_helpers(&mut self) {
        let void_ty = self.ctx.void_type();
        let i64_ty = self.ctx.i64_type();
        let ptr_ty = self.opaque_ptr_ty;

        // __cobrust_str_drop(*mut Str) -> void
        let str_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        let str_drop = self
            .module
            .add_function("__cobrust_str_drop", str_drop_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_drop", str_drop);

        // __cobrust_list_drop(*mut List) -> void
        let list_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        let list_drop = self
            .module
            .add_function("__cobrust_list_drop", list_drop_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_list_drop", list_drop);

        // __cobrust_list_drop_elems(*mut List, *mut fn(*mut Str)) -> void
        let list_drop_elems_ty = void_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let list_drop_elems = self.module.add_function(
            "__cobrust_list_drop_elems",
            list_drop_elems_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_list_drop_elems", list_drop_elems);

        // __cobrust_panic(*const u8, usize) -> void (noreturn at runtime)
        let panic_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let panic_fn = self
            .module
            .add_function("__cobrust_panic", panic_ty, Some(Linkage::External));
        self.runtime_helper_decls.insert("__cobrust_panic", panic_fn);
    }

    // =====================================================================
    // §4 — MIR Ty → LLVM type mapping
    // =====================================================================

    /// Lower a Cobrust MIR `Ty` to an inkwell `BasicTypeEnum`.
    ///
    /// Per ADR-0058a §4.1:
    ///
    /// | Ty | LLVM |
    /// |---|---|
    /// | `Bool` | `i1` |
    /// | `Int` | `i64` |
    /// | `Float` | `double` |
    /// | `Imag` | `double` (single-lane stub) |
    /// | `Str` / `Bytes` | `i8*` (opaque pointer) |
    /// | `List` / `Dict` / `Set` | `i8*` (heap-managed opaque) |
    /// | `Ref(T)` | same LLVM repr as `T` (transparent) |
    /// | `None` | `i8` (unit-shaped placeholder) |
    /// | `Tuple(...)` / `Record(_)` / `Adt(_,_)` | `i8*` (by pointer at wave-1) |
    /// | other | `i8*` fallback |
    fn lower_ty(&self, ty: &Ty) -> BasicTypeEnum<'ctx> {
        match ty {
            Ty::Bool => self.ctx.bool_type().as_basic_type_enum(),
            Ty::Int => self.ctx.i64_type().as_basic_type_enum(),
            Ty::Float | Ty::Imag => self.ctx.f64_type().as_basic_type_enum(),
            Ty::None => self.ctx.i8_type().as_basic_type_enum(),
            Ty::Ref(inner) => self.lower_ty(inner),
            // Owning + container + reference / tuple / record / ADT all
            // lower to opaque pointer at wave-1. Element type stays at
            // MIR level — recovered from per-Place / per-Operand context.
            _ => self.opaque_ptr_ty.as_basic_type_enum(),
        }
    }

    /// Build a function signature given param types + return type.
    fn fn_type_from(
        &self,
        params: &[BasicTypeEnum<'ctx>],
        ret: BasicTypeEnum<'ctx>,
    ) -> FunctionType<'ctx> {
        let metadata: Vec<BasicMetadataTypeEnum<'ctx>> =
            params.iter().map(|t| (*t).into()).collect();
        ret.fn_type(&metadata, false)
    }

    // =====================================================================
    // Body declaration + definition (two-pass mirror of CraneliftCtx)
    // =====================================================================

    /// First pass — declare the function symbol so cross-body calls
    /// resolve in the second pass.
    pub fn declare_body(&mut self, body: &Body) -> Result<(), CodegenError> {
        let name = if body.name.is_empty() {
            format!("_cobrust_init_{}", body.def_id.0)
        } else if body.name == "main" {
            // ADR-0025 §G: top-level `main` exported as `_cobrust_user_main`.
            "_cobrust_user_main".to_string()
        } else {
            body.name.clone()
        };

        // Param locals: skip _0 when it's the synthetic return slot.
        let param_locals: Vec<_> = if body.return_local == LocalId(0) {
            body.locals.iter().skip(1).take(body.param_count).collect()
        } else {
            body.locals.iter().take(body.param_count).collect()
        };
        let param_tys: Vec<BasicTypeEnum<'ctx>> =
            param_locals.iter().map(|l| self.lower_ty(&l.ty)).collect();

        // Return type: infer from `_return_local.ty`. For `Ty::None`
        // return locals, fall back to `i64` (matches Cranelift's
        // fallback to pointer_type which is i64 on the M9 scope).
        let ret_local = &body.locals[body.return_local.0 as usize];
        let ret_ty: BasicTypeEnum<'ctx> = if matches!(ret_local.ty, Ty::None) {
            self.ctx.i64_type().as_basic_type_enum()
        } else {
            self.lower_ty(&ret_local.ty)
        };

        let fn_ty = self.fn_type_from(&param_tys, ret_ty);
        let func = self
            .module
            .add_function(&name, fn_ty, Some(Linkage::External));

        self.function_ids.insert(body.def_id.0, func);
        self.body_return_types.insert(body.def_id.0, ret_ty);
        Ok(())
    }

    /// Second pass — emit the function body.
    pub fn define_body(&mut self, body: &Body) -> Result<(), CodegenError> {
        let func = *self
            .function_ids
            .get(&body.def_id.0)
            .ok_or_else(|| CodegenError::Internal(format!("body {} not declared", body.def_id.0)))?;
        let ret_ty = *self
            .body_return_types
            .get(&body.def_id.0)
            .ok_or_else(|| {
                CodegenError::Internal(format!("body {} return type missing", body.def_id.0))
            })?;

        // Create one LLVM basic block per MIR block.
        let mut block_map: HashMap<BlockId, BasicBlock<'ctx>> = HashMap::new();
        for mir_block in &body.blocks {
            let label = format!("bb{}", mir_block.id.0);
            let bb = self.ctx.append_basic_block(func, &label);
            block_map.insert(mir_block.id, bb);
        }

        // Entry block sets up allocas + binds parameters. Use a
        // dedicated "allocas" block prepended in front of bb0.
        let entry_bb = block_map[&BlockId(0)];
        let allocas_bb = self.ctx.prepend_basic_block(entry_bb, "allocas");
        self.builder.position_at_end(allocas_bb);

        let mut local_allocas: HashMap<LocalId, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)> =
            HashMap::new();
        for local in &body.locals {
            // Use the body's return type for the synthetic return slot
            // (parallels Cranelift's inferred_ret).
            let ty: BasicTypeEnum<'ctx> = if local.id == body.return_local {
                ret_ty
            } else {
                self.lower_ty(&local.ty)
            };
            let alloca = self
                .builder
                .build_alloca(ty, &format!("_{}", local.id.0))
                .map_err(map_builder_err)?;
            // Zero-init to keep every local well-defined on every path.
            let zero = zero_of(ty);
            self.builder
                .build_store(alloca, zero)
                .map_err(map_builder_err)?;
            local_allocas.insert(local.id, (alloca, ty));
        }

        // Bind incoming params to their alloca slots.
        let param_locals: Vec<_> = if body.return_local == LocalId(0) {
            body.locals.iter().skip(1).take(body.param_count).collect()
        } else {
            body.locals.iter().take(body.param_count).collect()
        };
        for (idx, local) in param_locals.iter().enumerate() {
            let param = func
                .get_nth_param(idx as u32)
                .ok_or_else(|| CodegenError::Internal(format!("missing param {}", idx)))?;
            let (alloca, _) = local_allocas[&local.id];
            self.builder
                .build_store(alloca, param)
                .map_err(map_builder_err)?;
        }

        // Branch from allocas → entry.
        self.builder
            .build_unconditional_branch(entry_bb)
            .map_err(map_builder_err)?;

        // Lower every MIR block via the per-Body lowerer.
        let blocks = body.blocks.clone();
        let mut lowerer = BodyLowerer {
            emitter: self,
            body,
            func,
            block_map: &block_map,
            local_allocas: &local_allocas,
            ret_ty,
        };
        for mir_block in &blocks {
            lowerer.lower_block(mir_block)?;
        }

        Ok(())
    }
}

// =====================================================================
// BodyLowerer — per-Body lowering pass
// =====================================================================

/// Per-Body lowerer. Borrows the emitter mutably + the body's state
/// tables. The `'a` lifetime scopes the per-body borrow; `'ctx` is the
/// inkwell context arena.
struct BodyLowerer<'a, 'ctx> {
    emitter: &'a mut LlvmEmitter<'ctx>,
    body: &'a Body,
    func: FunctionValue<'ctx>,
    block_map: &'a HashMap<BlockId, BasicBlock<'ctx>>,
    local_allocas: &'a HashMap<LocalId, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    ret_ty: BasicTypeEnum<'ctx>,
}

impl<'a, 'ctx> BodyLowerer<'a, 'ctx> {
    fn lower_block(&mut self, mir_block: &cobrust_mir::BasicBlock) -> Result<(), CodegenError> {
        let bb = self.block_map[&mir_block.id];
        self.emitter.builder.position_at_end(bb);
        for stmt in &mir_block.statements {
            self.lower_statement(stmt)?;
        }
        self.lower_terminator(&mir_block.terminator)?;
        Ok(())
    }

    fn lower_statement(&mut self, stmt: &Statement) -> Result<(), CodegenError> {
        match &stmt.kind {
            StatementKind::Assign { place, rvalue } => {
                let val = self.lower_rvalue(rvalue)?;
                self.write_place(place, val)?;
                Ok(())
            }
            StatementKind::StorageLive(_) | StatementKind::StorageDead(_) | StatementKind::Nop => {
                // Storage markers are MIR-level; LLVM relies on
                // `alloca`-at-entry semantics for stack-frame scope.
                Ok(())
            }
        }
    }

    fn lower_terminator(&mut self, term: &Terminator) -> Result<(), CodegenError> {
        match term {
            Terminator::Goto(target) => {
                let blk = self.block_map[target];
                self.emitter
                    .builder
                    .build_unconditional_branch(blk)
                    .map_err(map_builder_err)?;
                Ok(())
            }
            Terminator::Return => {
                let (alloca, _) = self.local_allocas[&self.body.return_local];
                let val = self
                    .emitter
                    .builder
                    .build_load(self.ret_ty, alloca, "ret")
                    .map_err(map_builder_err)?;
                self.emitter
                    .builder
                    .build_return(Some(&val))
                    .map_err(map_builder_err)?;
                Ok(())
            }
            Terminator::Unreachable => {
                self.emitter
                    .builder
                    .build_unreachable()
                    .map_err(map_builder_err)?;
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
                msg: _,
                target,
            } => {
                let cond_val = self.lower_operand(cond)?.into_int_value();
                let target_blk = self.block_map[target];
                let trap_blk = self.emitter.ctx.append_basic_block(self.func, "assert_trap");
                if *expected {
                    self.emitter
                        .builder
                        .build_conditional_branch(cond_val, target_blk, trap_blk)
                        .map_err(map_builder_err)?;
                } else {
                    self.emitter
                        .builder
                        .build_conditional_branch(cond_val, trap_blk, target_blk)
                        .map_err(map_builder_err)?;
                }
                self.emitter.builder.position_at_end(trap_blk);
                // Wave-1: emit `unreachable` after a conceptual panic.
                // The runtime helper materialises a Python-shaped
                // exception in M11; we keep the trap honest until then.
                self.emitter
                    .builder
                    .build_unreachable()
                    .map_err(map_builder_err)?;
                Ok(())
            }
            Terminator::Drop { place, target } => {
                let local_decl = self.body.locals.get(place.local.0 as usize);
                if let Some(decl) = local_decl {
                    let ty = decl.ty.clone();
                    self.emit_drop_for_ty(place, &ty)?;
                }
                let blk = self.block_map[target];
                self.emitter
                    .builder
                    .build_unconditional_branch(blk)
                    .map_err(map_builder_err)?;
                Ok(())
            }
            Terminator::Call {
                func,
                args,
                destination,
                target,
                unwind: _,
            } => self.lower_call(func, args, destination, *target),
        }
    }

    fn lower_call(
        &mut self,
        func: &Operand,
        args: &[Operand],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), CodegenError> {
        // User-defined fn call via `Constant::FnRef(def_id)`.
        if let Operand::Constant(Constant::FnRef(id)) = func {
            if let Some(callee) = self.emitter.function_ids.get(id).copied() {
                let mut call_args: Vec<BasicMetadataValueEnum<'ctx>> =
                    Vec::with_capacity(args.len());
                for arg in args {
                    let v = self.lower_operand(arg)?;
                    call_args.push(v.into());
                }
                let call_site = self
                    .emitter
                    .builder
                    .build_call(callee, &call_args, "call")
                    .map_err(map_builder_err)?;
                let ret_val: BasicValueEnum<'ctx> = call_site
                    .try_as_basic_value()
                    .basic()
                    .unwrap_or_else(|| self.emitter.ctx.i64_type().const_zero().into());
                self.write_place(destination, ret_val)?;
                let blk = self.block_map[&target];
                self.emitter
                    .builder
                    .build_unconditional_branch(blk)
                    .map_err(map_builder_err)?;
                return Ok(());
            }
            // Falls through to stub fallthrough below for unknown FnRef
            // ids (lambda placeholder `FnRef(0)`, await `FnRef(u32::MAX)`).
        }

        // Wave-1 stub fallthrough — write 0 into destination, branch.
        // Runtime-helper / extern-name Call lowering deferred to wave-2
        // (sub-ADR 0058a-followup or 0058b) per ADR-0058a §8.
        let zero: BasicValueEnum<'ctx> = self.emitter.ctx.i64_type().const_zero().into();
        self.write_place(destination, zero)?;
        let blk = self.block_map[&target];
        self.emitter
            .builder
            .build_unconditional_branch(blk)
            .map_err(map_builder_err)?;
        Ok(())
    }

    fn emit_drop_for_ty(&mut self, place: &Place, ty: &Ty) -> Result<(), CodegenError> {
        // ADR-0050c Phase 2 — TD-1 closure mirror. Dispatch by ty:
        //   - Ty::Str → __cobrust_str_drop(ptr)
        //   - Ty::List(Ty::Str) → __cobrust_list_drop_elems(ptr, str_drop)
        //   - Ty::List(_) → __cobrust_list_drop(ptr)
        //   - other → no-op
        let helper = match ty {
            Ty::Str => Some("__cobrust_str_drop"),
            Ty::List(elem) if matches!(**elem, Ty::Str) => Some("__cobrust_list_drop_elems"),
            Ty::List(_) => Some("__cobrust_list_drop"),
            _ => None,
        };
        if let Some(name) = helper {
            let callee = self.emitter.runtime_helper_decls[name];
            let val = self.lower_place_load(place)?;
            // Helpers expect pointer arg(s); coerce non-pointer values
            // through int→ptr (defensive — the dropped local is
            // expected to be pointer-typed at wave-1).
            let ptr_arg: BasicValueEnum<'ctx> = if val.is_pointer_value() {
                val
            } else if val.is_int_value() {
                self.emitter
                    .builder
                    .build_int_to_ptr(
                        val.into_int_value(),
                        self.emitter.opaque_ptr_ty,
                        "drop_arg",
                    )
                    .map_err(map_builder_err)?
                    .into()
            } else {
                val
            };
            let args: Vec<BasicMetadataValueEnum<'ctx>> = if name == "__cobrust_list_drop_elems" {
                let str_drop = self.emitter.runtime_helper_decls["__cobrust_str_drop"]
                    .as_global_value()
                    .as_pointer_value();
                vec![ptr_arg.into(), str_drop.into()]
            } else {
                vec![ptr_arg.into()]
            };
            self.emitter
                .builder
                .build_call(callee, &args, "drop")
                .map_err(map_builder_err)?;
        }
        Ok(())
    }

    fn lower_switch_int(
        &mut self,
        scrutinee: BasicValueEnum<'ctx>,
        cases: &[(SwitchValue, BlockId)],
        otherwise: BlockId,
    ) -> Result<(), CodegenError> {
        let otherwise_blk = self.block_map[&otherwise];
        if cases.is_empty() {
            self.emitter
                .builder
                .build_unconditional_branch(otherwise_blk)
                .map_err(map_builder_err)?;
            return Ok(());
        }
        let scrutinee_int = scrutinee.into_int_value();
        let scrutinee_ty = scrutinee_int.get_type();
        let case_pairs: Vec<(IntValue<'ctx>, BasicBlock<'ctx>)> = cases
            .iter()
            .map(|(val, target)| {
                let payload = match val {
                    SwitchValue::Bool(b) => i64::from(*b),
                    SwitchValue::Int(v) => *v,
                    SwitchValue::Adt(d) => i64::from(*d),
                };
                let case_val = scrutinee_ty.const_int(payload as u64, true);
                (case_val, self.block_map[target])
            })
            .collect();
        self.emitter
            .builder
            .build_switch(scrutinee_int, otherwise_blk, &case_pairs)
            .map_err(map_builder_err)?;
        Ok(())
    }

    // =====================================================================
    // §5 — Operand + Rvalue lowering
    // =====================================================================

    fn lower_rvalue(&mut self, rvalue: &Rvalue) -> Result<BasicValueEnum<'ctx>, CodegenError> {
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
                // Wave-1 stub — same posture as Cranelift backend at M9.
                Ok(self.emitter.ctx.i64_type().const_zero().into())
            }
        }
    }

    fn lower_operand(&mut self, op: &Operand) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        match op {
            Operand::Copy(place) | Operand::Move(place) => self.lower_place_load(place),
            Operand::Constant(c) => self.lower_constant(c),
        }
    }

    fn lower_constant(&mut self, c: &Constant) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let ctx = self.emitter.ctx;
        match c {
            Constant::Bool(b) => Ok(ctx.bool_type().const_int(u64::from(*b), false).into()),
            Constant::Int(i) => Ok(ctx.i64_type().const_int(*i as u64, true).into()),
            Constant::Float(bits) | Constant::Imag(bits) => {
                let f = f64::from_bits(*bits);
                Ok(ctx.f64_type().const_float(f).into())
            }
            Constant::None => Ok(ctx.i8_type().const_zero().into()),
            Constant::Str(_) | Constant::Bytes(_) => {
                // Wave-1 stub — Cranelift backend M9 emits zero for
                // string literals at most callsites; matching that
                // posture here keeps differential parity. The full
                // str-buffer materialisation is M11 stdlib runtime.
                Ok(self.emitter.opaque_ptr_ty.const_null().into())
            }
            Constant::FnRef(_) => Ok(ctx.i64_type().const_zero().into()),
        }
    }

    fn lower_place_load(&mut self, place: &Place) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let (alloca, ty) = self.local_allocas[&place.local];
        // Wave-1: projections (Field / Index / Deref / Discriminant) are
        // not yet materialised in the LLVM backend — Cranelift backend
        // also only supports a narrow subset of projection paths at M9.
        // Bare-local load is the wave-1 surface.
        if place.projections.is_empty() {
            let val = self
                .emitter
                .builder
                .build_load(ty, alloca, "load")
                .map_err(map_builder_err)?;
            Ok(val)
        } else if matches!(place.projections.as_slice(), [Projection::Deref]) {
            // For deref-of-pointer (the most common wave-1 projection),
            // load the pointer then load through it.
            let ptr_val = self
                .emitter
                .builder
                .build_load(ty, alloca, "ptr_load")
                .map_err(map_builder_err)?;
            if ptr_val.is_pointer_value() {
                let inner = ptr_val.into_pointer_value();
                let loaded = self
                    .emitter
                    .builder
                    .build_load(self.emitter.ctx.i64_type(), inner, "deref")
                    .map_err(map_builder_err)?;
                return Ok(loaded);
            }
            Ok(ptr_val)
        } else {
            // Other projections fall back to bare-local load — wave-2
            // (sub-ADR 0058b) closes Field / Index lowering.
            let val = self
                .emitter
                .builder
                .build_load(ty, alloca, "load_proj_stub")
                .map_err(map_builder_err)?;
            Ok(val)
        }
    }

    fn write_place(
        &mut self,
        place: &Place,
        val: BasicValueEnum<'ctx>,
    ) -> Result<(), CodegenError> {
        let (alloca, ty) = self.local_allocas[&place.local];
        // Cast value to alloca's expected type if needed (handles the
        // i1↔i8↔i64 + zero-fallthrough cases the lowering hands us).
        let val_cast = self.coerce_value_to(val, ty)?;
        self.emitter
            .builder
            .build_store(alloca, val_cast)
            .map_err(map_builder_err)?;
        Ok(())
    }

    /// Coerce a BasicValueEnum to a target BasicTypeEnum via the most
    /// common int/float/ptr conversions. Wave-1 only handles the cases
    /// emit code actually produces (no general-purpose cast pass).
    fn coerce_value_to(
        &mut self,
        val: BasicValueEnum<'ctx>,
        ty: BasicTypeEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        if val.get_type() == ty {
            return Ok(val);
        }
        // Int → Int (zext / trunc).
        if val.is_int_value() {
            if let BasicTypeEnum::IntType(target_int) = ty {
                let iv = val.into_int_value();
                let from_bits = iv.get_type().get_bit_width();
                let to_bits = target_int.get_bit_width();
                let result = if from_bits == to_bits {
                    iv
                } else if from_bits < to_bits {
                    self.emitter
                        .builder
                        .build_int_z_extend(iv, target_int, "zext")
                        .map_err(map_builder_err)?
                } else {
                    self.emitter
                        .builder
                        .build_int_truncate(iv, target_int, "trunc")
                        .map_err(map_builder_err)?
                };
                return Ok(result.into());
            }
        }
        // Pointer ← Int.
        if let BasicTypeEnum::PointerType(target_ptr) = ty {
            if val.is_int_value() {
                let iv = val.into_int_value();
                let casted = self
                    .emitter
                    .builder
                    .build_int_to_ptr(iv, target_ptr, "int2ptr")
                    .map_err(map_builder_err)?;
                return Ok(casted.into());
            }
        }
        // Int ← Pointer.
        if let BasicTypeEnum::IntType(target_int) = ty {
            if val.is_pointer_value() {
                let casted = self
                    .emitter
                    .builder
                    .build_ptr_to_int(val.into_pointer_value(), target_int, "ptr2int")
                    .map_err(map_builder_err)?;
                return Ok(casted.into());
            }
        }
        // Default — return the value unchanged. The dev-mode verifier
        // surfaces any LLVM rejection.
        Ok(val)
    }

    fn lower_binop(
        &mut self,
        op: BinOp,
        a: BasicValueEnum<'ctx>,
        b: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let is_float = a.is_float_value();
        let builder = self.emitter.builder;
        let val: BasicValueEnum<'ctx> = match (op, is_float) {
            (BinOp::Add, false) => builder
                .build_int_add(a.into_int_value(), b.into_int_value(), "add")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Add, true) => builder
                .build_float_add(a.into_float_value(), b.into_float_value(), "fadd")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Sub, false) => builder
                .build_int_sub(a.into_int_value(), b.into_int_value(), "sub")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Sub, true) => builder
                .build_float_sub(a.into_float_value(), b.into_float_value(), "fsub")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Mul, false) => builder
                .build_int_mul(a.into_int_value(), b.into_int_value(), "mul")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Mul, true) => builder
                .build_float_mul(a.into_float_value(), b.into_float_value(), "fmul")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Div | BinOp::FloorDiv, false) => builder
                .build_int_signed_div(a.into_int_value(), b.into_int_value(), "sdiv")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Div | BinOp::FloorDiv, true) => builder
                .build_float_div(a.into_float_value(), b.into_float_value(), "fdiv")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Mod, false) => {
                // ADR-0041 §H1: Python floor-mod, not C remainder.
                // emit `srem`, then `+ b` when rem != 0 && signs differ.
                let av = a.into_int_value();
                let bv = b.into_int_value();
                let ty = av.get_type();
                let rem = builder
                    .build_int_signed_rem(av, bv, "srem")
                    .map_err(map_builder_err)?;
                let zero = ty.const_zero();
                let rem_nonzero = builder
                    .build_int_compare(IntPredicate::NE, rem, zero, "rem_nonzero")
                    .map_err(map_builder_err)?;
                let signs_xor = builder
                    .build_xor(rem, bv, "signs_xor")
                    .map_err(map_builder_err)?;
                let signs_differ = builder
                    .build_int_compare(IntPredicate::SLT, signs_xor, zero, "signs_differ")
                    .map_err(map_builder_err)?;
                let need_adjust = builder
                    .build_and(rem_nonzero, signs_differ, "need_adjust")
                    .map_err(map_builder_err)?;
                let adjusted = builder
                    .build_int_add(rem, bv, "rem_adj")
                    .map_err(map_builder_err)?;
                builder
                    .build_select(need_adjust, adjusted, rem, "mod_result")
                    .map_err(map_builder_err)?
            }
            (BinOp::Mod, true) => {
                // ADR-0041 §H1 float floor-mod (matches Cranelift backend).
                let av = a.into_float_value();
                let bv = b.into_float_value();
                let f_ty = av.get_type();
                let div = builder
                    .build_float_div(av, bv, "fdiv")
                    .map_err(map_builder_err)?;
                let div_i = builder
                    .build_float_to_signed_int(div, self.emitter.ctx.i64_type(), "f2i")
                    .map_err(map_builder_err)?;
                let trunc = builder
                    .build_signed_int_to_float(div_i, f_ty, "i2f")
                    .map_err(map_builder_err)?;
                let prod = builder
                    .build_float_mul(bv, trunc, "fprod")
                    .map_err(map_builder_err)?;
                let rem = builder
                    .build_float_sub(av, prod, "frem")
                    .map_err(map_builder_err)?;
                let fzero = f_ty.const_zero();
                let rem_nonzero = builder
                    .build_float_compare(FloatPredicate::ONE, rem, fzero, "frem_nonzero")
                    .map_err(map_builder_err)?;
                let rem_lt = builder
                    .build_float_compare(FloatPredicate::OLT, rem, fzero, "frem_lt")
                    .map_err(map_builder_err)?;
                let b_lt = builder
                    .build_float_compare(FloatPredicate::OLT, bv, fzero, "fb_lt")
                    .map_err(map_builder_err)?;
                let signs_differ = builder
                    .build_xor(rem_lt, b_lt, "fsigns_differ")
                    .map_err(map_builder_err)?;
                let need_adjust = builder
                    .build_and(rem_nonzero, signs_differ, "fneed_adjust")
                    .map_err(map_builder_err)?;
                let adjusted = builder
                    .build_float_add(rem, bv, "frem_adj")
                    .map_err(map_builder_err)?;
                builder
                    .build_select(need_adjust, adjusted, rem, "fmod_result")
                    .map_err(map_builder_err)?
            }
            (BinOp::BitAnd | BinOp::And, _) => builder
                .build_and(a.into_int_value(), b.into_int_value(), "band")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::BitOr | BinOp::Or, _) => builder
                .build_or(a.into_int_value(), b.into_int_value(), "bor")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::BitXor, _) => builder
                .build_xor(a.into_int_value(), b.into_int_value(), "bxor")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Shl, _) => builder
                .build_left_shift(a.into_int_value(), b.into_int_value(), "shl")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Shr, _) => builder
                .build_right_shift(a.into_int_value(), b.into_int_value(), true, "shr")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Eq, false) => builder
                .build_int_compare(IntPredicate::EQ, a.into_int_value(), b.into_int_value(), "eq")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::NotEq, false) => builder
                .build_int_compare(IntPredicate::NE, a.into_int_value(), b.into_int_value(), "ne")
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Lt, false) => builder
                .build_int_compare(
                    IntPredicate::SLT,
                    a.into_int_value(),
                    b.into_int_value(),
                    "lt",
                )
                .map_err(map_builder_err)?
                .into(),
            (BinOp::LtEq, false) => builder
                .build_int_compare(
                    IntPredicate::SLE,
                    a.into_int_value(),
                    b.into_int_value(),
                    "le",
                )
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Gt, false) => builder
                .build_int_compare(
                    IntPredicate::SGT,
                    a.into_int_value(),
                    b.into_int_value(),
                    "gt",
                )
                .map_err(map_builder_err)?
                .into(),
            (BinOp::GtEq, false) => builder
                .build_int_compare(
                    IntPredicate::SGE,
                    a.into_int_value(),
                    b.into_int_value(),
                    "ge",
                )
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Eq, true) => builder
                .build_float_compare(
                    FloatPredicate::OEQ,
                    a.into_float_value(),
                    b.into_float_value(),
                    "feq",
                )
                .map_err(map_builder_err)?
                .into(),
            (BinOp::NotEq, true) => builder
                .build_float_compare(
                    FloatPredicate::ONE,
                    a.into_float_value(),
                    b.into_float_value(),
                    "fne",
                )
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Lt, true) => builder
                .build_float_compare(
                    FloatPredicate::OLT,
                    a.into_float_value(),
                    b.into_float_value(),
                    "flt",
                )
                .map_err(map_builder_err)?
                .into(),
            (BinOp::LtEq, true) => builder
                .build_float_compare(
                    FloatPredicate::OLE,
                    a.into_float_value(),
                    b.into_float_value(),
                    "fle",
                )
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Gt, true) => builder
                .build_float_compare(
                    FloatPredicate::OGT,
                    a.into_float_value(),
                    b.into_float_value(),
                    "fgt",
                )
                .map_err(map_builder_err)?
                .into(),
            (BinOp::GtEq, true) => builder
                .build_float_compare(
                    FloatPredicate::OGE,
                    a.into_float_value(),
                    b.into_float_value(),
                    "fge",
                )
                .map_err(map_builder_err)?
                .into(),
            (BinOp::Pow, _) => {
                // ADR-0041 §H3 — same honest surface as Cranelift backend.
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

    fn lower_unop(
        &mut self,
        op: UnOp,
        a: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let is_float = a.is_float_value();
        let builder = self.emitter.builder;
        let val: BasicValueEnum<'ctx> = match (op, is_float) {
            (UnOp::Plus, _) => a,
            (UnOp::Neg, false) => builder
                .build_int_neg(a.into_int_value(), "neg")
                .map_err(map_builder_err)?
                .into(),
            (UnOp::Neg, true) => builder
                .build_float_neg(a.into_float_value(), "fneg")
                .map_err(map_builder_err)?
                .into(),
            (UnOp::BitNot | UnOp::Not, _) => builder
                .build_not(a.into_int_value(), "bnot")
                .map_err(map_builder_err)?
                .into(),
        };
        Ok(val)
    }

    fn lower_aggregate(
        &mut self,
        _kind: &AggregateKind,
        _operands: &[Operand],
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        // Wave-1 stub — Aggregate lowering for List/Dict/Set/Tuple/Record
        // requires the stdlib runtime helpers (`__cobrust_list_new`,
        // `__cobrust_dict_new`, etc.) which land in M11 + sub-ADR 0058b.
        // Matches the Cranelift backend's mid-M9 stub posture at the
        // wave-1 ratification SHA.
        Ok(self.emitter.opaque_ptr_ty.const_null().into())
    }

    fn lower_cast(
        &mut self,
        kind: CastKind,
        op: &Operand,
        _target_ty: &Ty,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let v = self.lower_operand(op)?;
        let builder = self.emitter.builder;
        let ctx = self.emitter.ctx;
        let val: BasicValueEnum<'ctx> = match kind {
            CastKind::IntToFloat => builder
                .build_signed_int_to_float(v.into_int_value(), ctx.f64_type(), "i2f")
                .map_err(map_builder_err)?
                .into(),
            CastKind::FloatToInt => builder
                .build_float_to_signed_int(v.into_float_value(), ctx.i64_type(), "f2i")
                .map_err(map_builder_err)?
                .into(),
            CastKind::BoolToInt => builder
                .build_int_z_extend(v.into_int_value(), ctx.i64_type(), "b2i")
                .map_err(map_builder_err)?
                .into(),
            CastKind::IntToBool => builder
                .build_int_compare(
                    IntPredicate::NE,
                    v.into_int_value(),
                    ctx.i64_type().const_zero(),
                    "i2b",
                )
                .map_err(map_builder_err)?
                .into(),
            CastKind::StrToBytes | CastKind::BytesToStr => v,
        };
        Ok(val)
    }

    fn lower_ref(&mut self, place: &Place) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        // Wave-1: Ref of a bare local is the alloca pointer itself.
        let (alloca, _) = self.local_allocas[&place.local];
        Ok(alloca.into())
    }
}

// =====================================================================
// Helpers
// =====================================================================

/// Convert an inkwell builder error into our structured taxonomy.
fn map_builder_err(e: inkwell::builder::BuilderError) -> CodegenError {
    CodegenError::LlvmError(format!("inkwell builder error: {e}"))
}

/// Zero-init value for an arbitrary BasicTypeEnum.
fn zero_of<'ctx>(ty: BasicTypeEnum<'ctx>) -> BasicValueEnum<'ctx> {
    match ty {
        BasicTypeEnum::IntType(t) => t.const_zero().into(),
        BasicTypeEnum::FloatType(t) => t.const_zero().into(),
        BasicTypeEnum::PointerType(t) => t.const_null().into(),
        BasicTypeEnum::ArrayType(t) => t.const_zero().into(),
        BasicTypeEnum::StructType(t) => t.const_zero().into(),
        BasicTypeEnum::VectorType(t) => t.const_zero().into(),
        // LLVM 18+ inkwell exposes scalable vectors as a distinct
        // variant. Wave-1 does not produce scalable-vector locals;
        // panic loudly if MIR somehow surfaces one.
        BasicTypeEnum::ScalableVectorType(t) => t.const_zero().into(),
    }
}

// =====================================================================
// Tests (wave-1 smoke — full 30-fixture corpus lands on TEST branch merge)
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use cobrust_frontend::span::{FileId, Span};
    use cobrust_hir::DefId;
    use cobrust_mir::{
        BasicBlock as MirBlock, BinOp as MirBinOp, BlockId, Body, Constant as MirConstant,
        LocalDecl, LocalId, Module, Operand, Place, Rvalue, Statement, StatementKind, Terminator,
    };
    use cobrust_types::Ty;
    use std::sync::Mutex;

    // Serialize inkwell `Target::initialize_all` + emit across tests —
    // LLVM's target initialisation is process-global; concurrent test
    // runners can race on it.
    static LLVM_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn span0() -> Span {
        Span::new(FileId::SYNTHETIC, 0, 0)
    }

    fn host_spec() -> TargetSpec {
        let tmp = tempfile::tempdir().unwrap();
        // Persist the temp dir for the test's duration via leaking the
        // handle; the directory is cleaned up by the OS at exit. (We
        // do this rather than `into_path()` which is deprecated.)
        let path = tmp.keep();
        TargetSpec {
            triple: target_lexicon::Triple::host(),
            opt_level: OptLevel::None,
            backend: crate::target::Backend::Llvm,
            artifact: ArtifactKind::Object,
            output_dir: path,
            module_name: "smoke".to_string(),
        }
    }

    /// Helper: build a Body with N parameters + a single block doing
    /// `_return = body_rvalue` then Return.
    fn build_simple_body(
        def_id: u32,
        name: &str,
        params: Vec<Ty>,
        ret_ty: Ty,
        body_rvalue: Rvalue,
    ) -> Body {
        // locals: _0 (return slot, declared as the actual ret_ty so the
        // body's return type is unambiguous), _1.._N (params).
        let mut locals = vec![LocalDecl {
            id: LocalId(0),
            name: "_return".to_string(),
            ty: ret_ty,
            mutable: true,
            span: span0(),
        }];
        for (i, ty) in params.iter().enumerate() {
            locals.push(LocalDecl {
                id: LocalId((i + 1) as u32),
                name: format!("p{i}"),
                ty: ty.clone(),
                mutable: false,
                span: span0(),
            });
        }
        let param_count = params.len();

        let block0 = MirBlock {
            id: BlockId(0),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: body_rvalue,
                },
                span: span0(),
            }],
            terminator: Terminator::Return,
            span: span0(),
        };

        Body {
            def_id: DefId(def_id),
            name: name.to_string(),
            locals,
            blocks: vec![block0],
            return_local: LocalId(0),
            param_count,
            span: span0(),
        }
    }

    #[test]
    fn smoke_empty_module() {
        let _guard = LLVM_TEST_LOCK.lock().unwrap();
        let module = Module { bodies: vec![] };
        let spec = host_spec();
        let result = emit(&module, &spec);
        assert!(result.is_ok(), "empty module emit failed: {:?}", result.err());
    }

    #[test]
    fn smoke_return_42() {
        let _guard = LLVM_TEST_LOCK.lock().unwrap();
        let body = build_simple_body(
            1,
            "answer",
            vec![],
            Ty::Int,
            Rvalue::Use(Operand::Constant(MirConstant::Int(42))),
        );
        let module = Module { bodies: vec![body] };
        let spec = host_spec();
        let result = emit(&module, &spec);
        assert!(result.is_ok(), "return 42 emit failed: {:?}", result.err());
    }

    #[test]
    fn smoke_binop_add() {
        let _guard = LLVM_TEST_LOCK.lock().unwrap();
        let body = build_simple_body(
            2,
            "add_i64",
            vec![Ty::Int, Ty::Int],
            Ty::Int,
            Rvalue::BinaryOp(
                MirBinOp::Add,
                Operand::Copy(Place::local(LocalId(1))),
                Operand::Copy(Place::local(LocalId(2))),
            ),
        );
        let module = Module { bodies: vec![body] };
        let spec = host_spec();
        let result = emit(&module, &spec);
        assert!(result.is_ok(), "binop add emit failed: {:?}", result.err());
    }

    #[test]
    fn smoke_unop_neg_float() {
        let _guard = LLVM_TEST_LOCK.lock().unwrap();
        let body = build_simple_body(
            3,
            "neg_f64",
            vec![Ty::Float],
            Ty::Float,
            Rvalue::UnaryOp(UnOp::Neg, Operand::Copy(Place::local(LocalId(1)))),
        );
        let module = Module { bodies: vec![body] };
        let spec = host_spec();
        let result = emit(&module, &spec);
        assert!(result.is_ok(), "unop neg emit failed: {:?}", result.err());
    }

    #[test]
    fn smoke_drop_str_local() {
        let _guard = LLVM_TEST_LOCK.lock().unwrap();
        // fn drop_str(s: Str) -> i64 { _return = 0; drop s; return }
        // We exercise the Drop terminator by inserting an explicit
        // Drop block before Return.
        let locals = vec![
            LocalDecl {
                id: LocalId(0),
                name: "_return".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0(),
            },
            LocalDecl {
                id: LocalId(1),
                name: "s".to_string(),
                ty: Ty::Str,
                mutable: false,
                span: span0(),
            },
        ];
        let block0 = MirBlock {
            id: BlockId(0),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::Use(Operand::Constant(MirConstant::Int(0))),
                },
                span: span0(),
            }],
            terminator: Terminator::Drop {
                place: Place::local(LocalId(1)),
                target: BlockId(1),
            },
            span: span0(),
        };
        let block1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0(),
        };
        let body = Body {
            def_id: DefId(4),
            name: "drop_str".to_string(),
            locals,
            blocks: vec![block0, block1],
            return_local: LocalId(0),
            param_count: 1,
            span: span0(),
        };
        let module = Module { bodies: vec![body] };
        let spec = host_spec();
        let result = emit(&module, &spec);
        assert!(result.is_ok(), "drop_str emit failed: {:?}", result.err());
    }
}
