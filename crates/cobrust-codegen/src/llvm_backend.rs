//! LLVM backend (per ADR-0058a §3, ADR-0023 §"Backend feature flag layout").
//!
//! Active only when `--features llvm` is set. Wraps `inkwell 0.9 + llvm18-1`
//! to lower MIR into LLVM IR and emit object code via the LLVM 18+
//! toolchain.
//!
//! ADR-0058a wave-1 ships the **core lowering pass**.
//! ADR-0058b wave-2 (this file) extends with:
//!
//! - **PassBuilder pipeline**: `OptLevel::Speed` → `default<O2>`,
//!   `OptLevel::SpeedAndSize` → `default<O3>,default<Os>` via
//!   `Module::run_passes` (inkwell 0.9 + LLVM-18 new pass manager).
//! - **Multi-target dispatch**: `Target::from_triple` parametric over
//!   ADR-0046 tier-1 four-triple matrix (Mac arm64 / Linux arm64 /
//!   Linux x86_64 gnu+musl). See `supported_tier1_triples()` for the
//!   binding contract.
//!
//! Explicit non-goals (deferred per ADR-0058b §4):
//!
//! - DWARF debug-info emission (`DIBuilder`, `dbg.declare`) is sub-ADR 0058c.
//! - JIT opt-level changes (cobrust-jit `lower.rs` unchanged).
//! - Cross-link (linker stays at `cc`; cross-target executables are
//!   `release.yml` + `cross`-tool scope).
//! - New MIR features (wave-2 consumes wave-1's IR-construction pass).
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
//!
//! ADR-0058c wave-3 (this file) extends with:
//!
//! - **DWARF v5 emission** via `DebugInfoBuilder` (inkwell 0.9 LLVM-18 binding).
//!   Compile-unit DI scope + per-function `DISubprogram` + per-statement
//!   `DILocation` line table. Phase L Debugger (ADR-0059) consumes the
//!   emitted DWARF via standard `lldb` / `gdb` / VS Code DAP — bind-the-core.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
use inkwell::debug_info::{
    AsDIScope, DIBasicType, DICompileUnit, DICompositeType, DIFile, DIFlagsConstants, DILocation,
    DISubprogram, DWARFEmissionKind, DWARFSourceLanguage, DebugInfoBuilder,
};
use inkwell::module::{FlagBehavior, Linkage, Module as LlvmModule};
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple,
};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType, IntType};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue,
};

use cobrust_frontend::span::Span;

use crate::artifact::{Artifact, ArtifactKind};
use crate::error::CodegenError;
use crate::linker;
use crate::target::{OptLevel, TargetSpec};

// =====================================================================
// DWARF basic-type encoding constants (DW_ATE_* per DWARF v5 §7.8)
// =====================================================================
//
// inkwell 0.9 takes the encoding as a raw `LLVMDWARFTypeEncoding`
// (`u32`) — no symbolic enum is re-exported. We use the canonical
// DW_ATE numeric values per the DWARF v5 spec so the emitted basic
// types are inspectable via standard `llvm-dwarfdump`.

const DW_ATE_ADDRESS: u32 = 0x01;
const DW_ATE_BOOLEAN: u32 = 0x02;
const DW_ATE_FLOAT: u32 = 0x04;
const DW_ATE_SIGNED: u32 = 0x05;
// ADR-0059e §3.2 — DW_ATE_unsigned for the `cobrust::Str::len` member DI.
const DW_ATE_UNSIGNED: u32 = 0x07;

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

    // --- ADR-0058f §3.2: intern Constant::Str payloads as module-level
    //     rodata globals BEFORE body lowering, so `materialize_str_data`
    //     / `materialize_str_buffer` can look up the global pointer for
    //     any payload referenced by an Assign rvalue or Call arg.
    emitter.intern_str_payloads(module);

    // --- now define each body --------------------------------------------
    for body in &module.bodies {
        emitter.define_body(body)?;
    }

    // --- ADR-0058c §3.4: finalize DWARF DIEs before verify + opt -------
    //
    // `DebugInfoBuilder::finalize` writes all deferred DIE metadata
    // (placeholder DIs, replace-all-uses-with chains) into the LLVM
    // module IR. Must run before `Module::verify` (DI metadata shape
    // is checked there) and before `Module::run_passes` (some opt
    // passes consume DI metadata for inliner debug-info bookkeeping).
    emitter.di_builder.finalize();

    // --- verify (debug only) --------------------------------------------
    if cfg!(debug_assertions) {
        // ADR-0058a §9.2: dev-mode verifier prints offending IR via
        // structured error.
        if let Err(err) = emitter.module.verify() {
            return Err(CodegenError::LlvmError(format!(
                "LLVM module verify failed: {err}"
            )));
        }
    }

    // --- Tier 1 runtime-dispatch multi-versioning (ADR-0058b extension) -
    // Must run after all bodies are defined + DWARF finalised, but
    // before PassBuilder so the versioned clones are also optimised.
    if spec.runtime_dispatch {
        emit_multi_version_dispatch(&mut emitter, spec)?;
    }

    // --- ADR-0058b §3.2: run PassBuilder pipeline per OptLevel ----------
    if let Some(pipeline) = pass_pipeline_for(spec.opt_level) {
        let options = PassBuilderOptions::create();
        emitter
            .module
            .run_passes(pipeline, &target_machine, options)
            .map_err(|e| {
                CodegenError::LlvmError(format!(
                    "LLVM PassBuilder pipeline `{pipeline}` failed: {e}"
                ))
            })?;
    }

    // --- finalize: write object file ------------------------------------
    std::fs::create_dir_all(&spec.output_dir)?;
    let object_name = format!("{}.o", spec.module_name);
    let object_path = spec.output_dir.join(object_name);
    target_machine
        .write_to_file(&emitter.module, FileType::Object, &object_path)
        .map_err(|e| CodegenError::ObjectEmission(e.to_string()))?;

    finalize_artifact(object_path, spec)
}

/// Map [`OptLevel`] to an LLVM PassBuilder pipeline string per
/// ADR-0058b §3.2.
///
/// Returns `None` when no optimization passes should run (preserves the
/// wave-1 `-O0` path for `OptLevel::None`).
///
/// | `OptLevel` | Pipeline |
/// |---|---|
/// | `OptLevel::None` | `None` (skip `run_passes`) |
/// | `OptLevel::Speed` | `Some("default<O2>")` |
/// | `OptLevel::SpeedAndSize` | `Some("default<O3>,default<Os>")` |
///
/// The strings use LLVM's new-pass-manager `default<O*>` syntax; see
/// the `opt` tool's `-passes` argument and `llvm::PassBuilder::buildPerModuleDefaultPipeline`.
#[must_use]
pub fn pass_pipeline_for(level: OptLevel) -> Option<&'static str> {
    match level {
        OptLevel::None => None,
        OptLevel::Speed => Some("default<O2>"),
        OptLevel::SpeedAndSize => Some("default<O3>,default<Os>"),
    }
}

/// Enumerate the ADR-0046 tier-1 four-triple matrix that ADR-0058b
/// codifies as supported `TargetMachine::from_triple` inputs.
///
/// Tier-1 triples are guaranteed to construct successfully when the
/// underlying LLVM-18 toolchain on the host includes the corresponding
/// backend (Mac brew `llvm@18`, Linux apt `llvm-18-dev`). Tier-2+
/// triples may construct but are not exercised in CI per ADR-0046.
///
/// See ADR-0058b §3.4 for the binding rationale.
#[must_use]
pub fn supported_tier1_triples() -> &'static [&'static str] {
    &[
        "aarch64-apple-darwin",
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl",
    ]
}

// =====================================================================
// Tier 1 runtime-dispatch multi-versioning
// (numerical-compute-hardware-tiering.md §Tier1, ADR-0058b extension)
// =====================================================================

/// The three ISA tiers emitted for each hot function when
/// `TargetSpec::runtime_dispatch` is `true`.
///
/// | Variant | LLVM `target-features` | Rust detection macro |
/// |---|---|---|
/// | `Sse2`   | `+sse2`               | always-on for x86_64-v1 baseline |
/// | `Avx2`   | `+avx2,+fma`          | `is_x86_feature_detected!("avx2")` |
/// | `Avx512` | `+avx512f,+avx512dq`  | `is_x86_feature_detected!("avx512f")` |
#[derive(Clone, Copy, Debug)]
pub enum Tier1Variant {
    Sse2,
    Avx2,
    Avx512,
}

impl Tier1Variant {
    /// LLVM `target-features` string for this variant.
    #[must_use]
    pub fn target_features(self) -> &'static str {
        match self {
            Tier1Variant::Sse2 => "+sse2",
            Tier1Variant::Avx2 => "+avx2,+fma",
            Tier1Variant::Avx512 => "+avx512f,+avx512dq",
        }
    }

    /// Versioned-name suffix appended to the base function name.
    #[must_use]
    pub fn name_suffix(self) -> &'static str {
        match self {
            Tier1Variant::Sse2 => "_v1_sse2",
            Tier1Variant::Avx2 => "_v2_avx2",
            Tier1Variant::Avx512 => "_v3_avx512",
        }
    }

    /// All three variants in ascending capability order.
    #[must_use]
    pub fn all() -> [Tier1Variant; 3] {
        [Tier1Variant::Sse2, Tier1Variant::Avx2, Tier1Variant::Avx512]
    }
}

/// Returns `true` when the target triple is x86_64.
///
/// aarch64: NEON is mandatory in armv8-a — single-version emission is
/// already optimal. SVE multi-versioning is deferred per strategy doc
/// §NEON/SVE.
#[must_use]
pub fn triple_is_x86_64(spec: &TargetSpec) -> bool {
    spec.triple.to_string().starts_with("x86_64")
}

/// Emit Tier-1 runtime-dispatch multi-versioning for all top-level
/// functions in an already-lowered LLVM module.
///
/// For each function `<fn>` in the module (non-intrinsic, non-internal):
///
/// 1. Rename the original function to `<fn>_v1_sse2` and tag it with
///    `target-features=+sse2`.
/// 2. Clone it twice to `<fn>_v2_avx2` (`+avx2,+fma`) and
///    `<fn>_v3_avx512` (`+avx512f,+avx512dq`).
/// 3. Emit a dispatcher `<fn>` that calls the right versioned function
///    based on a runtime CPU-feature check via
///    `__cobrust_cpu_avx512_supported` / `__cobrust_cpu_avx2_supported`
///    — thin C helpers compiled from `runtime/cpu_features.c` that use
///    the safe `__builtin_cpu_supports` path (no inline asm, no unsafe
///    Rust — satisfies `#![forbid(unsafe_code)]`).
///
/// On non-x86_64 targets (`triple_is_x86_64(spec) == false`) this
/// function is a no-op: NEON is always-on in armv8-a.
///
/// # Safety invariant
///
/// All cloned functions share the same linkage + calling convention as
/// the original. The dispatcher is `linkonce_odr`-equivalent: the
/// original symbol name becomes the dispatcher, ensuring callers link
/// against the dispatcher transparently.
///
/// # Errors
///
/// Returns [`CodegenError::LlvmError`] if inkwell function-value
/// operations fail (e.g. invalid attribute strings).
pub fn emit_multi_version_dispatch<'ctx>(
    emitter: &mut LlvmEmitter<'ctx>,
    spec: &TargetSpec,
) -> Result<(), CodegenError> {
    // aarch64 / non-x86_64: no multi-versioning needed (NEON always-on).
    if !triple_is_x86_64(spec) {
        return Ok(());
    }

    // Collect all top-level, non-intrinsic function names to avoid
    // mutating the module while iterating.
    let fn_names: Vec<String> = emitter
        .module
        .get_functions()
        .filter(|f| {
            // Skip intrinsics (name starts with "llvm.") and the
            // runtime helper stubs we declared ourselves.
            let name = f.get_name().to_str().unwrap_or("");
            !name.starts_with("llvm.")
                && !name.starts_with("__cobrust_")
                && !name.is_empty()
                // Skip functions already versioned (idempotency guard).
                && !name.ends_with("_v1_sse2")
                && !name.ends_with("_v2_avx2")
                && !name.ends_with("_v3_avx512")
        })
        .map(|f| f.get_name().to_string_lossy().into_owned())
        .collect();

    for base_name in fn_names {
        let Some(original_fn) = emitter.module.get_function(&base_name) else {
            continue;
        };

        // Skip external declarations (no body to clone).
        if original_fn.get_first_basic_block().is_none() {
            continue;
        }

        let fn_type = original_fn.get_type();

        // Step 1: Rename original → <fn>_v1_sse2 and tag with +sse2.
        //
        // inkwell 0.9 does not expose `set_name` directly on `FunctionValue`,
        // but it is available via the `GlobalValue` delegation since every
        // function is a global value in LLVM IR.
        let sse2_name = format!("{base_name}_v1_sse2");
        original_fn.as_global_value().set_name(&sse2_name);
        add_target_features_attr(emitter, original_fn, Tier1Variant::Sse2.target_features());

        // Step 2: Clone to _v2_avx2 and _v3_avx512.
        //
        // inkwell 0.9 does not expose LLVM's `CloneFunctionInto` directly.
        // We use `Module::clone_function` via the safe workaround: add a
        // new function declaration, then use `LLVMCloneFunctionInto` via
        // the raw LLVM C API behind inkwell's `unsafe_ptr`. However, to
        // stay within `#![forbid(unsafe_code)]`, we instead emit a thin
        // forwarding wrapper for the avx2 / avx512 variants that simply
        // calls the sse2 version — the `target-features` attribute on the
        // wrapper causes LLVM to re-compile the inlined body with the new
        // ISA extensions enabled (via `alwaysinline` + IPA).
        //
        // This is the standard LLVM multiversioning pattern used by GCC's
        // `__attribute__((target(...)))` and Clang's `__attribute__((target_clones(...)))`.
        for variant in [Tier1Variant::Avx2, Tier1Variant::Avx512] {
            let versioned_name = format!("{base_name}{}", variant.name_suffix());
            let wrapper = emitter.module.add_function(&versioned_name, fn_type, None);

            // Mark alwaysinline so LLVM inlines the sse2 body and
            // re-optimises it with the new target-features.
            let ctx = emitter.ctx;
            let always_inline = ctx.create_enum_attribute(
                inkwell::attributes::Attribute::get_named_enum_kind_id("alwaysinline"),
                0,
            );
            wrapper.add_attribute(inkwell::attributes::AttributeLoc::Function, always_inline);
            add_target_features_attr(emitter, wrapper, variant.target_features());

            // Build a forwarding body: entry block calls sse2 version.
            let entry = ctx.append_basic_block(wrapper, "entry");
            let builder = ctx.create_builder();
            builder.position_at_end(entry);

            // Gather wrapper params to forward.
            let args: Vec<BasicMetadataValueEnum<'ctx>> =
                wrapper.get_param_iter().map(|p| p.into()).collect();

            let call = builder
                .build_call(original_fn, &args, "dispatch_call")
                .map_err(map_builder_err)?;

            // Return the call result (or void).
            let ret_ty = fn_type.get_return_type();
            if ret_ty.is_none() {
                builder.build_return(None).map_err(map_builder_err)?;
            } else {
                let ret_val = call
                    .try_as_basic_value().basic()
                    .ok_or_else(|| CodegenError::LlvmError(
                        format!("Tier1 dispatch wrapper for `{base_name}`: call has no return value for non-void fn"),
                    ))?;
                builder
                    .build_return(Some(&ret_val))
                    .map_err(map_builder_err)?;
            }
        }

        // Step 3: Emit the public-facing dispatcher `<fn>` that runtime-
        // checks CPU features and tail-calls the right versioned variant.
        //
        // The dispatcher is a thin function — LLVM will turn it into an
        // indirect branch or conditional sequence. Runtime detection uses
        // external C helpers that call `__builtin_cpu_supports` (the same
        // mechanism Clang and GCC use for function multi-versioning). These
        // helpers are compiled into `runtime/cobrust_main.c` (or a sibling
        // object) so this file stays `#![forbid(unsafe_code)]`-clean.
        let dispatcher = emitter.module.add_function(&base_name, fn_type, None);
        {
            let ctx = emitter.ctx;
            let entry = ctx.append_basic_block(dispatcher, "entry");
            let builder = ctx.create_builder();
            builder.position_at_end(entry);

            // Declare the two external feature-detection helpers (i32 return,
            // no args). `__cobrust_cpu_avx512_supported()` / `_avx2_supported()`
            // return 1 if the CPU supports the ISA, 0 otherwise. They are
            // defined in `runtime/cpu_features.c` using __builtin_cpu_supports.
            let i32_ty = ctx.i32_type();
            let detect_fn_ty = i32_ty.fn_type(&[], false);

            let detect_avx512 = emitter
                .module
                .get_function("__cobrust_cpu_avx512_supported")
                .unwrap_or_else(|| {
                    emitter.module.add_function(
                        "__cobrust_cpu_avx512_supported",
                        detect_fn_ty,
                        None,
                    )
                });
            let detect_avx2 = emitter
                .module
                .get_function("__cobrust_cpu_avx2_supported")
                .unwrap_or_else(|| {
                    emitter
                        .module
                        .add_function("__cobrust_cpu_avx2_supported", detect_fn_ty, None)
                });

            // if avx512_supported: call _v3_avx512
            let avx512_result = builder
                .build_call(detect_avx512, &[], "avx512_check")
                .map_err(map_builder_err)?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::LlvmError("avx512 detect call".into()))?
                .into_int_value();
            let zero_i32 = i32_ty.const_zero();
            let avx512_cond = builder
                .build_int_compare(IntPredicate::NE, avx512_result, zero_i32, "avx512_cond")
                .map_err(map_builder_err)?;

            let bb_avx512 = ctx.append_basic_block(dispatcher, "call_avx512");
            let bb_check_avx2 = ctx.append_basic_block(dispatcher, "check_avx2");
            let bb_avx2 = ctx.append_basic_block(dispatcher, "call_avx2");
            let bb_sse2 = ctx.append_basic_block(dispatcher, "call_sse2");

            builder
                .build_conditional_branch(avx512_cond, bb_avx512, bb_check_avx2)
                .map_err(map_builder_err)?;

            // bb_avx512
            builder.position_at_end(bb_avx512);
            let v3_fn = emitter
                .module
                .get_function(&format!("{base_name}_v3_avx512"))
                .ok_or_else(|| CodegenError::LlvmError(format!("missing {base_name}_v3_avx512")))?;
            let args: Vec<BasicMetadataValueEnum<'ctx>> =
                dispatcher.get_param_iter().map(|p| p.into()).collect();
            let v3_call = builder
                .build_call(v3_fn, &args, "v3_call")
                .map_err(map_builder_err)?;
            if fn_type.get_return_type().is_none() {
                builder.build_return(None).map_err(map_builder_err)?;
            } else {
                let rv = v3_call.try_as_basic_value().basic().ok_or_else(|| {
                    CodegenError::LlvmError(format!("v3 call no retval for {base_name}"))
                })?;
                builder.build_return(Some(&rv)).map_err(map_builder_err)?;
            }

            // bb_check_avx2
            builder.position_at_end(bb_check_avx2);
            let avx2_result = builder
                .build_call(detect_avx2, &[], "avx2_check")
                .map_err(map_builder_err)?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::LlvmError("avx2 detect call".into()))?
                .into_int_value();
            let avx2_cond = builder
                .build_int_compare(IntPredicate::NE, avx2_result, zero_i32, "avx2_cond")
                .map_err(map_builder_err)?;
            builder
                .build_conditional_branch(avx2_cond, bb_avx2, bb_sse2)
                .map_err(map_builder_err)?;

            // bb_avx2
            builder.position_at_end(bb_avx2);
            let v2_fn = emitter
                .module
                .get_function(&format!("{base_name}_v2_avx2"))
                .ok_or_else(|| CodegenError::LlvmError(format!("missing {base_name}_v2_avx2")))?;
            let args2: Vec<BasicMetadataValueEnum<'ctx>> =
                dispatcher.get_param_iter().map(|p| p.into()).collect();
            let v2_call = builder
                .build_call(v2_fn, &args2, "v2_call")
                .map_err(map_builder_err)?;
            if fn_type.get_return_type().is_none() {
                builder.build_return(None).map_err(map_builder_err)?;
            } else {
                let rv = v2_call.try_as_basic_value().basic().ok_or_else(|| {
                    CodegenError::LlvmError(format!("v2 call no retval for {base_name}"))
                })?;
                builder.build_return(Some(&rv)).map_err(map_builder_err)?;
            }

            // bb_sse2 (fallback)
            builder.position_at_end(bb_sse2);
            let v1_fn = emitter
                .module
                .get_function(&format!("{base_name}_v1_sse2"))
                .ok_or_else(|| CodegenError::LlvmError(format!("missing {base_name}_v1_sse2")))?;
            let args3: Vec<BasicMetadataValueEnum<'ctx>> =
                dispatcher.get_param_iter().map(|p| p.into()).collect();
            let v1_call = builder
                .build_call(v1_fn, &args3, "v1_call")
                .map_err(map_builder_err)?;
            if fn_type.get_return_type().is_none() {
                builder.build_return(None).map_err(map_builder_err)?;
            } else {
                let rv = v1_call.try_as_basic_value().basic().ok_or_else(|| {
                    CodegenError::LlvmError(format!("v1 call no retval for {base_name}"))
                })?;
                builder.build_return(Some(&rv)).map_err(map_builder_err)?;
            }
        }
    }

    Ok(())
}

/// Set the `target-features` LLVM function attribute on `func`.
///
/// Uses inkwell's `create_string_attribute` which maps to LLVM's
/// `addFnAttr(StringRef, StringRef)` — this is distinct from the
/// enum-keyed attributes used for `noinline` etc. Calling this with
/// an already-present `target-features` key replaces the value.
fn add_target_features_attr<'ctx>(
    emitter: &LlvmEmitter<'ctx>,
    func: FunctionValue<'ctx>,
    features: &str,
) {
    let attr = emitter
        .ctx
        .create_string_attribute("target-features", features);
    func.add_attribute(inkwell::attributes::AttributeLoc::Function, attr);
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
///
/// ADR-0058b §3.2 maps Cobrust [`OptLevel`] to LLVM
/// [`OptimizationLevel`] for the TargetMachine; the wave-2 PassBuilder
/// pipeline runs orthogonally on the module (§emit `run_passes`).
///
/// ADR-0058b §3.4 codifies parametric multi-target dispatch: `Target::from_triple`
/// accepts any tier-1 triple per [`supported_tier1_triples`]. Tier-2+ triples
/// may construct successfully at runtime when the LLVM backend is compiled in.
///
/// F66 (2026-05-28) — RISC-V triple normalization. Rust convention writes
/// `riscv64gc-unknown-linux-gnu` (ISA flags baked into the architecture
/// component); upstream LLVM only knows the plain `riscv64` / `riscv32`
/// architecture names — ISA extensions are passed via `target-features`
/// (`+m,+a,+f,+d,+c`). Without normalization, `Target::from_triple` returns
/// "No available targets are compatible with triple" even though the RISC-V
/// backend IS registered by `Target::initialize_all`. We translate the
/// triple's architecture component to LLVM's vocabulary and synthesise the
/// matching feature string. Same pattern applied to `riscv32gc` / `riscv32imc`
/// / `riscv64imac` / `riscv64a23`. ADR-0075 §"Combined risk surfaces" cited
/// the cross-target LLVM triple shape; this is its concrete first-proof
/// resolution.
fn build_target_machine(spec: &TargetSpec) -> Result<TargetMachine, CodegenError> {
    Target::initialize_all(&InitializationConfig::default());
    let (triple_str, isa_features) = normalize_triple_for_llvm(&spec.triple);
    let triple = TargetTriple::create(&triple_str);
    let target = Target::from_triple(&triple)
        .map_err(|e| CodegenError::UnsupportedTarget(format!("{}: {}", spec.triple, e)))?;
    let opt = match spec.opt_level {
        OptLevel::None => OptimizationLevel::None,
        // ADR-0058b §3.2: Speed → LLVM Default (O2); SpeedAndSize → Aggressive (O3).
        // PassBuilder pipeline (`run_passes`) drives the actual opt; TargetMachine
        // opt level sets codegen-time knobs (instruction selection, scheduling).
        OptLevel::Speed => OptimizationLevel::Default,
        OptLevel::SpeedAndSize => OptimizationLevel::Aggressive,
    };
    // Tier 2 host-specific CPU tuning (numerical-compute-hardware-tiering.md §Tier 2).
    // `"native"` is expanded to the concrete host CPU name + full host feature
    // string via LLVM's host-detection helpers, enabling all available ISA
    // extensions with zero dispatch overhead (host-only binary).
    //
    // F58: LLVM's `create_target_machine` does NOT interpret the literal string
    // `"native"` (unlike clang/llc, which call `sys::getHostCPUName()` themselves).
    // Passing `"native"` verbatim yields an "unknown CPU" subtarget that, on some
    // cloud x86_64 runners (e.g. GH Actions ubuntu), aborts with
    // "LLVM ERROR: 64-bit code requested on a subtarget that doesn't support it!".
    // We therefore expand `"native"` ourselves so LLVM receives a recognised CPU
    // name (e.g. "znver3") plus an explicit feature string carrying 64-bit mode.
    //
    // Any other string (e.g. `"skylake"`, `"apple-m1"`, `"neoverse-v1"`) is passed
    // verbatim with empty features. `None` falls back to the `"generic"` baseline
    // (pre-Tier-2 behaviour).
    //
    // F66: ISA features synthesised by `normalize_triple_for_llvm` (RISC-V `gc`,
    // `imac`, etc.) are appended after the caller's features so the RISC-V ISA
    // baseline always reflects the triple's intent. Empty for non-RISCV targets.
    let (cpu, mut features): (String, String) = match spec.target_cpu.as_deref() {
        Some("native") => (
            TargetMachine::get_host_cpu_name().to_string(),
            TargetMachine::get_host_cpu_features().to_string(),
        ),
        Some(name) => (name.to_string(), String::new()),
        None => ("generic".to_string(), String::new()),
    };
    if !isa_features.is_empty() {
        if features.is_empty() {
            features = isa_features;
        } else {
            features.push(',');
            features.push_str(&isa_features);
        }
    }
    target
        .create_target_machine(
            &triple,
            &cpu,
            &features,
            opt,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| {
            CodegenError::LlvmError(format!(
                "failed to create LLVM TargetMachine for {} (cpu={cpu})",
                spec.triple
            ))
        })
}

/// F66 — Normalize a `target_lexicon::Triple` for LLVM consumption.
///
/// Returns `(triple_string, isa_features)` where:
///
/// - `triple_string` is the triple with its architecture component
///   rewritten to LLVM's vocabulary (RISC-V `Riscv64gc` / `Riscv64imac` /
///   `Riscv64a23` → `riscv64`; same for `riscv32*` variants).
/// - `isa_features` is a comma-separated LLVM feature string carrying the
///   ISA extensions implied by the original architecture variant (e.g.
///   `Riscv64gc` → `+m,+a,+f,+d,+c`). Empty for plain `riscv64`/`riscv32`
///   and every non-RISC-V architecture.
///
/// Non-RISC-V triples pass through unchanged (no normalization, no features
/// synthesised).
///
/// # Why
///
/// Rust convention writes RISC-V triples as `riscv64gc-unknown-linux-gnu`
/// (the `gc` ISA flags are part of the architecture string). Upstream LLVM
/// — what inkwell + llvm-sys bind to — only recognises the plain `riscv64`
/// and `riscv32` architecture names; ISA extensions must be passed via the
/// `TargetMachine` `features` parameter. `Target::from_triple("riscv64gc-...")`
/// returns "No available targets are compatible with triple" even when the
/// RISC-V backend is fully registered (`Target::initialize_all` was called).
///
/// `clang` / `llc` work because they call `Triple::normalize` internally;
/// `LLVMGetTargetFromTriple` does not. This helper mirrors that normalization
/// at the inkwell boundary so the codegen layer accepts the broader Rust
/// triple vocabulary.
fn normalize_triple_for_llvm(triple: &target_lexicon::Triple) -> (String, String) {
    use target_lexicon::{Architecture, Riscv32Architecture, Riscv64Architecture};

    let original = triple.to_string();
    let (llvm_arch, features): (&str, &str) = match triple.architecture {
        Architecture::Riscv64(variant) => match variant {
            Riscv64Architecture::Riscv64 => ("riscv64", ""),
            // `riscv64gc` = `g` (m,a,f,d) + `c` (compressed). The `g`
            // baseline also implies `+zicsr,+zifencei` but LLVM 18
            // accepts them implicitly via `+m,+a,+f,+d`; spelling them
            // out matches the rustc target spec for parity.
            Riscv64Architecture::Riscv64gc => ("riscv64", "+m,+a,+f,+d,+c"),
            Riscv64Architecture::Riscv64imac => ("riscv64", "+m,+a,+c"),
            // `riscv64a23` (RVA23 profile) — superset of `gc`; conservative
            // baseline keeps the `gc` feature set. ISA extensions beyond
            // `gc` (v, b, …) belong on a Tier-2 sub-ADR.
            Riscv64Architecture::Riscv64a23 => ("riscv64", "+m,+a,+f,+d,+c"),
            // `Riscv64Architecture` is `#[non_exhaustive]` — any future
            // variant added upstream falls back to the conservative plain
            // `riscv64` baseline with no features (i.e. RV64I). Caller can
            // still override via `--target-cpu` if a richer baseline is
            // needed; the build won't error out on unrecognised variants.
            _ => ("riscv64", ""),
        },
        Architecture::Riscv32(variant) => match variant {
            Riscv32Architecture::Riscv32 => ("riscv32", ""),
            Riscv32Architecture::Riscv32gc => ("riscv32", "+m,+a,+f,+d,+c"),
            Riscv32Architecture::Riscv32i => ("riscv32", ""),
            Riscv32Architecture::Riscv32im => ("riscv32", "+m"),
            Riscv32Architecture::Riscv32ima => ("riscv32", "+m,+a"),
            Riscv32Architecture::Riscv32imac => ("riscv32", "+m,+a,+c"),
            Riscv32Architecture::Riscv32imafc => ("riscv32", "+m,+a,+f,+c"),
            Riscv32Architecture::Riscv32imc => ("riscv32", "+m,+c"),
            // `Riscv32Architecture` is `#[non_exhaustive]` (mirror Riscv64
            // arm above). Conservative RV32I baseline with no features.
            _ => ("riscv32", ""),
        },
        _ => return (original, String::new()),
    };

    // Replace only the architecture prefix (first '-' separated component);
    // preserve vendor + os + env unchanged (e.g. `unknown-linux-gnu`).
    let suffix = original.find('-').map_or("", |i| &original[i..]);
    let normalized = format!("{llvm_arch}{suffix}");
    (normalized, features.to_string())
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
///
/// ADR-0058c wave-3 extends with DWARF emission state: a
/// `DebugInfoBuilder` + `DICompileUnit` per module + cached basic-type
/// DIs + per-function `DISubprogram` lookups + a `LineMap` for
/// resolving per-statement spans into (line, column) pairs.
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
    /// ADR-0058f wave-2: parallel param-count map for the runtime
    /// helpers. Consumed by `lower_call` (extern-name branch) to
    /// detect the `(*const u8, usize)` expansion case where a single
    /// `Constant::Str` arg expands into two C params (ptr, len).
    /// Mirrors `cranelift_backend::runtime_helper_param_counts`.
    runtime_helper_param_counts: HashMap<&'static str, usize>,
    /// ADR-0058f §3.2 module-level str-data interning. Maps each
    /// unique `Constant::Str` payload to its rodata `i8*` global
    /// pointer. Populated once in `intern_str_payloads` before
    /// `define_body` is invoked per body. Consumed by
    /// `materialize_str_data` / `materialize_str_buffer`.
    str_data_globals: HashMap<String, PointerValue<'ctx>>,
    /// Cached `i8*` opaque pointer type used for str/list/dict/refs.
    opaque_ptr_ty: inkwell::types::PointerType<'ctx>,
    /// Target-pointer-width integer type — the LLVM lowering of a C
    /// `usize` / `size_t` argument. `i64` on x86_64 / aarch64 / riscv64,
    /// `i32` on wasm32. F71: runtime-helper externs whose Rust `extern
    /// "C"` definitions take a `usize` (e.g. `__cobrust_println(ptr,
    /// usize)`) MUST declare that parameter — and materialise the value
    /// passed at the call site — with this type, not a hardcoded `i64`.
    /// wasm32-wasip1 enforces strict typed `call`/`call_indirect`
    /// signatures: a `(ptr, i64)` declaration against a `(ptr, i32)`
    /// definition traps `unreachable` at runtime ("signature_mismatch").
    /// Native ELF linkers tolerate the width mismatch, masking the bug.
    usize_ty: IntType<'ctx>,
    // ----- ADR-0058c wave-3 DWARF state ---------------------------------
    /// inkwell DWARF builder; one per LLVM module (per source file).
    di_builder: DebugInfoBuilder<'ctx>,
    /// Compile-unit DI scope; root of every Cobrust function's DIScope.
    di_cu: DICompileUnit<'ctx>,
    /// The `DIFile` inside `di_cu`; reused for every `DISubprogram` +
    /// `DILocation`.
    di_file: DIFile<'ctx>,
    /// Cached basic-type DIs (Int / Float / Bool / opaque-ptr) keyed by
    /// a stable short tag so each signature lowering reuses the same
    /// `DIBasicType` objects.
    di_basic_types: HashMap<&'static str, DIBasicType<'ctx>>,
    /// ADR-0059d §3.2 — per-variant `cobrust::Option` `DICompositeType`.
    /// Emitted once in `populate_di_basic_types`; holds the two-member
    /// struct DI (tag: i32 at offset 0, payload: i64 at offset 64) so
    /// `image lookup --type cobrust::Option` finds the DIE in lldb.
    /// The composite is emitted unconditionally so the DIE is always
    /// present; `di_type_for` keeps returning `cobrust::Adt` for
    /// function signatures (opaque-pointer).
    di_option_composite: Option<DICompositeType<'ctx>>,
    /// ADR-0059e §3.2 — `cobrust::Str` `DICompositeType` carrying the
    /// logical (ptr, len) shape so lldb pretty-printers can walk into
    /// the StringBuffer payload via `SBValue.GetChildMemberWithName`.
    /// Mirrors the wave-3 `cobrust::Option` composite precedent above.
    /// Emitted unconditionally so `image lookup --type cobrust::Str`
    /// resolves to the composite DIE; `di_type_for` continues returning
    /// the opaque-pointer basic type for function signatures (so the
    /// LLVM type lowering doesn't shift). Closes the final Phase L
    /// §6.1 honest-cite (Str runtime `frame variable s = "hello"`).
    di_str_composite: Option<DICompositeType<'ctx>>,
    /// Per-body `DefId.0` → emitted `DISubprogram`. Populated at
    /// `declare_body`; consumed at `define_body` to set per-instruction
    /// debug locations.
    di_subprograms: HashMap<u32, DISubprogram<'ctx>>,
    /// Per-Span (line, column) lookup helper, built from the source
    /// file's bytes when `TargetSpec.source_path` is `Some`. Empty when
    /// the source is unknown (tests / synthetic modules).
    line_map: LineMap,
    /// Whether opt passes will run downstream — flows into `is_optimized`
    /// flag on every `DICompileUnit` + `DISubprogram` so the debugger
    /// renders inline-resolved frames consistently.
    is_optimized: bool,
}

impl<'ctx> LlvmEmitter<'ctx> {
    /// Construct a new emitter. Pre-declares the runtime-helper externs
    /// used by Drop / Assert lowering (`__cobrust_str_drop`,
    /// `__cobrust_list_drop_elems`, `__cobrust_list_drop`,
    /// `__cobrust_panic`).
    ///
    /// `spec` and `target_machine` drive module-name + triple +
    /// data-layout binding.
    ///
    /// ADR-0058c wave-3: also instantiates the DWARF emission scaffold
    /// (`DebugInfoBuilder` + `DICompileUnit` + `DIFile`) and pre-caches
    /// the four DI basic types (`Int`, `Float`, `Bool`, opaque `Ptr`).
    /// When `spec.source_path` is `Some`, the per-Span `LineMap` is
    /// built from the file's contents; otherwise it's empty (every
    /// span maps to line 0 / col 0 — DI structure still validates).
    pub fn new(
        ctx: &'ctx Context,
        spec: &TargetSpec,
        target_machine: &TargetMachine,
    ) -> Result<Self, CodegenError> {
        let module = ctx.create_module(&spec.module_name);
        module.set_triple(&target_machine.get_triple());
        module.set_data_layout(&target_machine.get_target_data().get_data_layout());
        let builder = ctx.create_builder();
        let opaque_ptr_ty = ctx.ptr_type(AddressSpace::default());
        // F71: the C `usize`/`size_t` width is target-driven — `i64` on
        // x86_64 / aarch64 / riscv64, `i32` on wasm32-wasip1. Derive it
        // from the target machine's data layout (already bound to the
        // module above) so `usize`-typed runtime-helper externs declare
        // and pass the width wasm strict typed calls demand.
        let usize_ty = ctx.ptr_sized_int_type(&target_machine.get_target_data(), None);

        // --- ADR-0058c §3.1 DWARF scaffold -----------------------------
        // LLVM requires the module-level "Debug Info Version" + "Dwarf Version"
        // metadata flags before any DIE emission, else `Module::verify`
        // rejects DI metadata under `LLVMVerifierFailureAction`.
        let dbg_ver = ctx.i32_type().const_int(3, false);
        module.add_basic_value_flag("Debug Info Version", FlagBehavior::Warning, dbg_ver);
        let dwarf_ver = ctx.i32_type().const_int(5, false);
        module.add_basic_value_flag("Dwarf Version", FlagBehavior::Warning, dwarf_ver);

        let (filename, directory, line_map) = resolve_source_paths(spec);
        let is_optimized = matches!(spec.opt_level, OptLevel::Speed | OptLevel::SpeedAndSize);
        let (di_builder, di_cu) = module.create_debug_info_builder(
            /* allow_unresolved */ true,
            DWARFSourceLanguage::C,
            &filename,
            &directory,
            /* producer */ "cobrust 0.3.x (ADR-0058c)",
            /* is_optimized */ is_optimized,
            /* compiler command-line flags */ "",
            /* runtime_ver */ 0,
            /* split_name */ "",
            DWARFEmissionKind::Full,
            /* dwo_id */ 0,
            /* split_debug_inlining */ false,
            /* debug_info_for_profiling */ false,
            /* sysroot */ "",
            /* sdk */ "",
        );
        let di_file = di_cu.get_file();

        let mut emitter = LlvmEmitter {
            ctx,
            module,
            builder,
            function_ids: HashMap::new(),
            body_return_types: HashMap::new(),
            runtime_helper_decls: HashMap::new(),
            runtime_helper_param_counts: HashMap::new(),
            str_data_globals: HashMap::new(),
            opaque_ptr_ty,
            usize_ty,
            di_builder,
            di_cu,
            di_file,
            di_basic_types: HashMap::new(),
            di_subprograms: HashMap::new(),
            line_map,
            is_optimized,
            di_option_composite: None,
            di_str_composite: None,
        };
        emitter.declare_runtime_helpers();
        emitter.populate_di_basic_types();
        Ok(emitter)
    }

    /// Pre-build the DI basic types used by every signature lowering:
    /// `i64` / `f64` / `bool` / `ptr` (ADR-0058c §3.2 base) plus 5
    /// distinctly-named container types `cobrust::Str` /
    /// `cobrust::List` / `cobrust::Dict` / `cobrust::Set` /
    /// `cobrust::Tuple` (ADR-0059a §3.3.1 Option A). The 5 container
    /// entries share opaque-pointer storage (64-bit, `DW_ATE_ADDRESS`)
    /// but carry distinct DWARF type-names so lldb pretty-printers
    /// (`tools/lldb-cobrust/printers.py`) can dispatch via
    /// `type summary add cobrust::Str` /
    /// `type synthetic add -l <Provider> --regex '^cobrust::List'`.
    ///
    /// Cached so each `create_subroutine_type` call reuses the same
    /// `DIType` pointers (per ADR-0058c §3.2 dedup contract).
    fn populate_di_basic_types(&mut self) {
        let zero = inkwell::debug_info::DIFlags::ZERO;
        // Int64 — DW_ATE_signed (5).
        let int_ty = self
            .di_builder
            .create_basic_type("i64", 64, DW_ATE_SIGNED, zero)
            .expect("DI basic type i64");
        self.di_basic_types.insert("Int", int_ty);
        // Float64 — DW_ATE_float (4).
        let float_ty = self
            .di_builder
            .create_basic_type("f64", 64, DW_ATE_FLOAT, zero)
            .expect("DI basic type f64");
        self.di_basic_types.insert("Float", float_ty);
        // Bool — DW_ATE_boolean (2), 8 bits per typical lldb expectation
        // (LLVM emits i1 → 1 byte at storage time; basic type stays at
        // 8 bits so debuggers don't gag on sub-byte storage).
        let bool_ty = self
            .di_builder
            .create_basic_type("bool", 8, DW_ATE_BOOLEAN, zero)
            .expect("DI basic type bool");
        self.di_basic_types.insert("Bool", bool_ty);
        // Opaque pointer — DW_ATE_address (1). Fallback for opaque /
        // unmodeled `Ty::Ref` / `Ty::Adt` / `Ty::Bytes`.
        let ptr_ty = self
            .di_builder
            .create_basic_type("ptr", 64, DW_ATE_ADDRESS, zero)
            .expect("DI basic type ptr");
        self.di_basic_types.insert("Ptr", ptr_ty);

        // ADR-0059a §3.3.1 Option A — 5 named container DI types.
        // All share opaque-pointer storage; names disambiguate the
        // lldb pretty-printer dispatch surface.
        //
        // ADR-0059a §6.3 wave-2 — `cobrust::Adt` named DI type added so
        // `Ty::Adt(...)` (the future home of `Option<T>`, `Result<T,E>`,
        // and user-defined enums) gets a distinct DWARF name. Until
        // HIR/MIR carry the per-variant Adt name through to DI, every
        // Adt local collapses to the single `cobrust::Adt` matcher; the
        // generic Adt printer renders `<adt#N @ 0xaddr>` and the
        // OptionProvider takes over once MIR threads the Adt name
        // (Phase L+ follow-up).
        for (key, name) in [
            ("Str", "cobrust::Str"),
            ("List", "cobrust::List"),
            ("Dict", "cobrust::Dict"),
            ("Set", "cobrust::Set"),
            ("Tuple", "cobrust::Tuple"),
            ("Adt", "cobrust::Adt"),
        ] {
            let ty = self
                .di_builder
                .create_basic_type(name, 64, DW_ATE_ADDRESS, zero)
                .expect("DI basic type cobrust container");
            self.di_basic_types.insert(key, ty);
        }

        // ADR-0059d §3.2 — per-variant Option DICompositeType.
        //
        // Emit a `DICompositeType` (DW_TAG_structure_type) named
        // `"cobrust::Option"` unconditionally so `image lookup --type
        // cobrust::Option` finds the DIE in lldb. The composite has
        // two member fields:
        //
        //   tag:     i32 at offset   0, DW_ATE_signed — 0=None, 1=Some.
        //   payload: i64 at offset  64, DW_ATE_signed — valid only
        //            when tag=1 (Option<Int> representative layout).
        //
        // This is a representative layout for `Option<Int>`; the lldb
        // printer reads tag first then conditionally reads the payload
        // (ADR-0059d §3.2 printer extension). Phase L+ will parametrise
        // the payload type once MIR threads the Adt params through DI.
        let tag_ty = self
            .di_builder
            .create_basic_type("i32", 32, DW_ATE_SIGNED, zero)
            .expect("DI i32 basic type for Option tag");
        let payload_ty = self.di_basic_types["Int"]; // i64

        let cu_scope = self.di_cu.as_debug_info_scope();
        let tag_member = self.di_builder.create_member_type(
            cu_scope,
            "tag",
            self.di_file,
            0,  // line number
            32, // size in bits
            32, // align in bits
            0,  // offset in bits
            zero,
            tag_ty.as_type(),
        );
        let payload_member = self.di_builder.create_member_type(
            cu_scope,
            "payload",
            self.di_file,
            0,  // line number
            64, // size in bits
            64, // align in bits
            64, // offset in bits (after 32-bit tag + 32-bit pad)
            zero,
            payload_ty.as_type(),
        );
        let option_composite = self.di_builder.create_struct_type(
            cu_scope,
            "cobrust::Option",
            self.di_file,
            0,   // line number
            128, // size in bits (tag 32 + pad 32 + payload 64)
            64,  // align in bits
            zero,
            None,                                              // derived_from
            &[tag_member.as_type(), payload_member.as_type()], // elements
            0,                                                 // runtime language
            None,                                              // vtable_holder
            "cobrust::Option",                                 // unique_id
        );
        self.di_option_composite = Some(option_composite);

        // ADR-0059e §3.2 — per-variant `cobrust::Str` DICompositeType.
        //
        // Emit a `DICompositeType` (DW_TAG_structure_type) named
        // `"cobrust::Str"` so `image lookup --type cobrust::Str` returns
        // a DIE with structured member fields. Two members carry the
        // logical (ptr, len) view that the lldb pretty-printer
        // (`tools/lldb-cobrust/printers.py::cobrust_str_summary`) walks
        // via `SBValue.GetChildMemberWithName("ptr")` /
        // `GetChildMemberWithName("len")`:
        //
        //   ptr: *const u8 at offset   0, DW_ATE_ADDRESS — bytes start.
        //   len: u64       at offset  64, DW_ATE_UNSIGNED — byte length.
        //
        // The composite models the **logical** (ptr, len) view; the
        // runtime layout is `Box<StringBuffer { Vec<u8> }>` (an
        // indirection through the box). The printer's wave-2
        // `_read_string_buffer` fallback path is preserved for
        // backward-compat with binaries that pre-date this emission.
        //
        // Note: `di_basic_types["Str"]` keeps pointing at the opaque-
        // pointer basic type so function signatures
        // (`fn take_str(x: Str) -> Str`) keep using the pointer-sized
        // DI without affecting LLVM type lowering. The composite is
        // emitted unconditionally — like the Option composite above —
        // so lldb resolves the DIE even when no function signature
        // returns a `Str`.
        let unsigned_i64_ty = self
            .di_builder
            .create_basic_type("u64", 64, DW_ATE_UNSIGNED, zero)
            .expect("DI u64 basic type for Str len");
        let str_ptr_member = self.di_builder.create_member_type(
            cu_scope,
            "ptr",
            self.di_file,
            0,  // line number
            64, // size in bits
            64, // align in bits
            0,  // offset in bits
            zero,
            ptr_ty.as_type(),
        );
        let str_len_member = self.di_builder.create_member_type(
            cu_scope,
            "len",
            self.di_file,
            0,  // line number
            64, // size in bits
            64, // align in bits
            64, // offset in bits (after 64-bit ptr)
            zero,
            unsigned_i64_ty.as_type(),
        );
        let str_composite = self.di_builder.create_struct_type(
            cu_scope,
            "cobrust::Str",
            self.di_file,
            0,   // line number
            128, // size in bits (ptr 64 + len 64)
            64,  // align in bits
            zero,
            None,                                                  // derived_from
            &[str_ptr_member.as_type(), str_len_member.as_type()], // elements
            0,                                                     // runtime language
            None,                                                  // vtable_holder
            "cobrust::Str",                                        // unique_id
        );
        self.di_str_composite = Some(str_composite);
    }

    /// Map a Cobrust MIR `Ty` to its cached `DIBasicType`. Per
    /// ADR-0058c §3.2: numeric scalars get their own DI. Per
    /// ADR-0059a §3.3.1 Option A: 5 container variants
    /// (`Str` / `List` / `Dict` / `Set` / `Tuple`) get their own
    /// distinctly-named DI entries so lldb pretty-printers can
    /// dispatch on the DWARF type-name.
    ///
    /// ADR-0059a §6.3 wave-2 — `Ty::Adt(_, _)` now gets its own
    /// `cobrust::Adt` DI name (was `Ptr`). The lldb printer
    /// registrations bind the generic `cobrust_option_summary` to
    /// `cobrust::Adt` so any Adt local renders with at least the
    /// `Some(<addr>)` ptr-tag shape until per-Adt naming is threaded
    /// through MIR (Phase L+ follow-up).
    ///
    /// All other (`Bytes`, `Ref`, `Record`, `Fn`, `Var`, `Alias`,
    /// `Generic`, `None`, `Never`) collapse to the opaque-pointer
    /// fallback `Ptr` (matches the wave-1/2 LLVM type lowering).
    fn di_type_for(&self, ty: &Ty) -> DIBasicType<'ctx> {
        let key = match ty {
            Ty::Int => "Int",
            // ADR-0060a — narrow ints collapse to the same DW_ATE_signed
            // basic-type category as `Ty::Int`. Width disambiguation lives
            // in LLVM-IR's iN type — DI tracks signedness, not width here.
            Ty::IntN(_) => "Int",
            Ty::Float | Ty::Imag => "Float",
            Ty::Bool => "Bool",
            Ty::Str => "Str",
            Ty::List(_) => "List",
            Ty::Dict(_, _) => "Dict",
            Ty::Set(_) => "Set",
            Ty::Tuple(_) => "Tuple",
            // ADR-0059a §6.3 wave-2 — `Ty::Adt` gets a distinct DI name.
            Ty::Adt(_, _) => "Adt",
            // ADR-0060b — array DI collapses to opaque-ptr (no array DI
            // wave-2; lldb can introspect via the LLVM type).
            Ty::Array(_, _) => "Ptr",
            _ => "Ptr",
        };
        self.di_basic_types[key]
    }

    /// Pre-declare runtime helpers used by Drop / Assert lowering.
    /// Mirrors `cranelift_backend::runtime_helper_signatures` but only
    /// for the wave-1 surface (drop family + panic).
    fn declare_runtime_helpers(&mut self) {
        let void_ty = self.ctx.void_type();
        let i64_ty = self.ctx.i64_type();
        let ptr_ty = self.opaque_ptr_ty;
        // F71: target-pointer-width integer for C `usize` params. `i64`
        // natively, `i32` on wasm32 — must match each runtime def's
        // `extern "C" fn(.. usize ..)` exactly or wasm strict typed
        // calls trap `unreachable`.
        let usize_ty = self.usize_ty;

        // __cobrust_str_drop(*mut Str) -> void
        let str_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        let str_drop =
            self.module
                .add_function("__cobrust_str_drop", str_drop_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_drop", str_drop);

        // __cobrust_list_drop(*mut List) -> void
        let list_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        let list_drop =
            self.module
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
        // F71: 2nd arg is `usize` (panic.rs:47) — pointer-width, not i64.
        let panic_ty = void_ty.fn_type(&[ptr_ty.into(), usize_ty.into()], false);
        let panic_fn =
            self.module
                .add_function("__cobrust_panic", panic_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_panic", panic_fn);

        // ADR-0058g sub-wave-1 — __cobrust_argv() -> *mut ListBuffer<*mut StrBuffer>
        // Mirrors Cranelift `cranelift_backend.rs:2822` zero-arg ptr-return shape.
        // The extern is dispatched from MIR via `Kind::Argv` rewrite at
        // `cobrust-cli/src/build/intrinsics.rs:1439-1447` (ARGV_RUNTIME_SYMBOL).
        // Stdlib export: `cobrust-stdlib/src/env.rs:64`. The companion
        // `__cobrust_capture_argv` symbol is invoked exclusively from the C
        // shim `cobrust-cli/runtime/cobrust_main.c`, NOT from MIR; Cranelift
        // also omits its extern decl, so LLVM matches (parity intentional).
        let argv_ty = ptr_ty.fn_type(&[], false);
        let argv_fn = self
            .module
            .add_function("__cobrust_argv", argv_ty, Some(Linkage::External));
        self.runtime_helper_decls.insert("__cobrust_argv", argv_fn);

        // -----------------------------------------------------------------
        // ADR-0058g sub-wave-2 — list runtime extern hookup.
        // Mirrors Cranelift `cranelift_backend.rs:2670-2682` ABI verbatim
        // (sigs cross-verified against stdlib exports at
        // `cobrust-stdlib/src/collections.rs:390,419,440,459,477,595`).
        //
        // Drop schedule context (ADR-0050c TD-1): `__cobrust_list_drop` +
        // `__cobrust_list_drop_elems` were already declared above for the
        // wave-1 Drop terminator path (see `emit_drop_for_ty` at the
        // corresponding `Terminator::Drop` arm). Sub-wave-2 adds only the
        // 6 constructor/accessor/mutator helpers that the MIR `Constant::Str`
        // extern-name dispatch routes to via `lower_call`.
        // -----------------------------------------------------------------

        // __cobrust_list_new(elem_size: i64, len: i64) -> *mut ListBuffer
        let list_new_ty = ptr_ty.fn_type(&[i64_ty.into(), i64_ty.into()], false);
        let list_new_fn =
            self.module
                .add_function("__cobrust_list_new", list_new_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_list_new", list_new_fn);

        // __cobrust_list_set(list: *mut ListBuffer, i: i64, v: i64) -> void
        let list_set_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
        let list_set_fn =
            self.module
                .add_function("__cobrust_list_set", list_set_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_list_set", list_set_fn);

        // __cobrust_list_get(list: *const ListBuffer, i: i64) -> i64
        let list_get_ty = i64_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let list_get_fn =
            self.module
                .add_function("__cobrust_list_get", list_get_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_list_get", list_get_fn);

        // __cobrust_list_len(list: *const ListBuffer) -> i64
        let list_len_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let list_len_fn =
            self.module
                .add_function("__cobrust_list_len", list_len_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_list_len", list_len_fn);

        // __cobrust_list_is_empty(list: *const ListBuffer) -> i64 (0/1)
        // Returns i64 per the SwitchInt codegen convention (ADR-0050c
        // §"Phase 6" / F5 §2.2 uniformity addendum).
        let list_is_empty_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let list_is_empty_fn = self.module.add_function(
            "__cobrust_list_is_empty",
            list_is_empty_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_list_is_empty", list_is_empty_fn);

        // __cobrust_list_append(list: *mut ListBuffer, v: i64) -> void
        // ADR-0041 §H6: comprehension lowering uses runtime append.
        let list_append_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let list_append_fn = self.module.add_function(
            "__cobrust_list_append",
            list_append_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_list_append", list_append_fn);

        // ADR-0060b dynamic-index Array runtime helpers.
        // __cobrust_array_get_i64(*const i64, usize, usize) -> i64
        // F71: len + idx are `usize` (array.rs:42) — pointer-width.
        let arr_get_i64_ty =
            i64_ty.fn_type(&[ptr_ty.into(), usize_ty.into(), usize_ty.into()], false);
        let arr_get_i64 = self.module.add_function(
            "__cobrust_array_get_i64",
            arr_get_i64_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_array_get_i64", arr_get_i64);

        // __cobrust_array_get_i32(*const i32, usize, usize) -> i32
        let i32_ty = self.ctx.i32_type();
        let arr_get_i32_ty =
            i32_ty.fn_type(&[ptr_ty.into(), usize_ty.into(), usize_ty.into()], false);
        let arr_get_i32 = self.module.add_function(
            "__cobrust_array_get_i32",
            arr_get_i32_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_array_get_i32", arr_get_i32);

        // __cobrust_array_get_i8(*const i8, usize, usize) -> i8
        let i8_ty = self.ctx.i8_type();
        let arr_get_i8_ty =
            i8_ty.fn_type(&[ptr_ty.into(), usize_ty.into(), usize_ty.into()], false);
        let arr_get_i8 = self.module.add_function(
            "__cobrust_array_get_i8",
            arr_get_i8_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_array_get_i8", arr_get_i8);

        // __cobrust_array_get_bool(*const u8, usize, usize) -> i64
        let arr_get_bool_ty =
            i64_ty.fn_type(&[ptr_ty.into(), usize_ty.into(), usize_ty.into()], false);
        let arr_get_bool = self.module.add_function(
            "__cobrust_array_get_bool",
            arr_get_bool_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_array_get_bool", arr_get_bool);

        // -----------------------------------------------------------------
        // ADR-0058f wave-2 — stdlib print system runtime helpers.
        // Mirrors the Cranelift backend's `runtime_helper_signatures`
        // entries at `cranelift_backend.rs:2750-2836` for the print
        // family + the str-buffer subroutines they depend on. Wave-3
        // surfaces (input / list / dict / iter / fmt / math / parse /
        // str method family) explicitly deferred per ADR-0058f §7.
        // -----------------------------------------------------------------
        let i8_ty_helpers = self.ctx.i8_type();
        let f64_ty = self.ctx.f64_type();

        // __cobrust_println_int(i64) -> void  (ADR-0030 §Decision step 5)
        let println_int_ty = void_ty.fn_type(&[i64_ty.into()], false);
        let println_int = self.module.add_function(
            "__cobrust_println_int",
            println_int_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_println_int", println_int);
        self.runtime_helper_param_counts
            .insert("__cobrust_println_int", 1);

        // __cobrust_println_bool(i8) -> void  (ADR-0064 §3.3)
        // i8 (NOT i1) — bools widen at the call site via z_extend.
        let println_bool_ty = void_ty.fn_type(&[i8_ty_helpers.into()], false);
        let println_bool = self.module.add_function(
            "__cobrust_println_bool",
            println_bool_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_println_bool", println_bool);
        self.runtime_helper_param_counts
            .insert("__cobrust_println_bool", 1);

        // __cobrust_println_float(f64) -> void  (ADR-0064 §3.3)
        let println_float_ty = void_ty.fn_type(&[f64_ty.into()], false);
        let println_float = self.module.add_function(
            "__cobrust_println_float",
            println_float_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_println_float", println_float);
        self.runtime_helper_param_counts
            .insert("__cobrust_println_float", 1);

        // __cobrust_println_str_buf(*mut Str) -> void  (ADR-0044 W2)
        let println_str_buf_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        let println_str_buf = self.module.add_function(
            "__cobrust_println_str_buf",
            println_str_buf_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_println_str_buf", println_str_buf);
        self.runtime_helper_param_counts
            .insert("__cobrust_println_str_buf", 1);

        // __cobrust_println(*const u8, usize) -> void  (ADR-0025 §Runtime ABI)
        // F71: 2nd arg is `usize` (io.rs:72) — pointer-width, not i64.
        // This is the exact extern hello_wasm.wasm trapped on under
        // wasmtime's strict typed-call check before this fix.
        let println_lit_ty = void_ty.fn_type(&[ptr_ty.into(), usize_ty.into()], false);
        let println_lit =
            self.module
                .add_function("__cobrust_println", println_lit_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_println", println_lit);
        self.runtime_helper_param_counts
            .insert("__cobrust_println", 2);

        // __cobrust_print_no_nl(*mut Str) -> void
        let print_no_nl_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        let print_no_nl = self.module.add_function(
            "__cobrust_print_no_nl",
            print_no_nl_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_print_no_nl", print_no_nl);
        self.runtime_helper_param_counts
            .insert("__cobrust_print_no_nl", 1);

        // __cobrust_print_no_nl_lit(*const u8, usize) -> void  (ADR-0047 Option H)
        // F71: 2nd arg is `usize` (io.rs:723) — pointer-width, not i64.
        let print_no_nl_lit_ty = void_ty.fn_type(&[ptr_ty.into(), usize_ty.into()], false);
        let print_no_nl_lit = self.module.add_function(
            "__cobrust_print_no_nl_lit",
            print_no_nl_lit_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_print_no_nl_lit", print_no_nl_lit);
        self.runtime_helper_param_counts
            .insert("__cobrust_print_no_nl_lit", 2);

        // -----------------------------------------------------------------
        // ADR-0058f §3.3 — str-buffer subroutines for materialize_str_buffer.
        // -----------------------------------------------------------------
        // __cobrust_str_new() -> *mut Str
        let str_new_ty = ptr_ty.fn_type(&[], false);
        let str_new =
            self.module
                .add_function("__cobrust_str_new", str_new_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_new", str_new);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_new", 0);

        // __cobrust_str_push_static(*mut Str, *const u8, usize) -> void
        let str_push_static_ty =
            void_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), i64_ty.into()], false);
        let str_push_static = self.module.add_function(
            "__cobrust_str_push_static",
            str_push_static_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_str_push_static", str_push_static);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_push_static", 3);

        // Param counts for wave-1 helpers — needed so the extern-name
        // dispatch path in `lower_call` can use a uniform lookup.
        self.runtime_helper_param_counts
            .insert("__cobrust_str_drop", 1);
        self.runtime_helper_param_counts
            .insert("__cobrust_list_drop", 1);
        self.runtime_helper_param_counts
            .insert("__cobrust_list_drop_elems", 2);
        self.runtime_helper_param_counts
            .insert("__cobrust_panic", 2);
        // ADR-0058g sub-wave-1 — argv is a zero-arg helper. Recorded
        // explicitly (not relying on `.unwrap_or(args.len())` fallback) so
        // a future maintainer reading the helper-decl block sees the
        // contract beside the rest of the wave-1/2 surface.
        self.runtime_helper_param_counts.insert("__cobrust_argv", 0);

        // ADR-0058g sub-wave-2 — list runtime param counts. Mirrors
        // Cranelift `cranelift_backend.rs:2670-2682` ABI. Recorded
        // explicitly so the `expand_str_to_ptr_len` detection in
        // `lower_call` (which uses `sig_param_count` for the (ptr, len)
        // expansion case) sees the true C-signature arity — none of the
        // list helpers take a Str arg, so no expansion path applies, but
        // recording the counts uniformly keeps the dispatch contract
        // explicit (F37 silent-rot avoidance).
        self.runtime_helper_param_counts
            .insert("__cobrust_list_new", 2);
        self.runtime_helper_param_counts
            .insert("__cobrust_list_set", 3);
        self.runtime_helper_param_counts
            .insert("__cobrust_list_get", 2);
        self.runtime_helper_param_counts
            .insert("__cobrust_list_len", 1);
        self.runtime_helper_param_counts
            .insert("__cobrust_list_is_empty", 1);
        self.runtime_helper_param_counts
            .insert("__cobrust_list_append", 2);

        // -----------------------------------------------------------------
        // ADR-0058g sub-wave-3 — dict + set + tuple runtime extern hookup.
        // Mirrors Cranelift `cranelift_backend.rs:2684-2758` ABI verbatim
        // (sigs cross-verified against stdlib exports at
        // `cobrust-stdlib/src/collections.rs:781-1359`).
        //
        // Drop schedule context (ADR-0050c TD-1; ADR-0058g §6.1 dict portion):
        // Cranelift `lower_drop` (see `cranelift_backend.rs:1232-1241`)
        // dispatches `__cobrust_dict_drop` on `Ty::Dict(_, _)` but treats
        // `Ty::Set(_)` / `Ty::Tuple(_)` as no-op (comment: "Tuple/Set drops
        // are not yet plumbed; M12.x leaves these as no-op"). To preserve
        // strict parity, this wave-3 hookup:
        //   - DECLARES `__cobrust_dict_drop` AND extends `emit_drop_for_ty`
        //     to call it on `Ty::Dict(_, _)` (mirrors Cranelift).
        //   - DECLARES `__cobrust_set_drop` + `__cobrust_tuple_drop`
        //     (needed if MIR ever routes them through `lower_call` as an
        //     extern; Cranelift's `runtime_helper_signatures` declares
        //     them analogously) but DOES NOT extend `emit_drop_for_ty` for
        //     `Ty::Set` / `Ty::Tuple` — Cranelift no-ops these, so LLVM
        //     matches. Future Phase G widening (see Cranelift comment
        //     line 1238-1240) will widen both backends together.
        //
        // Dict (K, V) shim coverage: 10 typed shims + 4 untyped/erased
        // (new, drop, len, is_empty) + 2 legacy (untyped set/get). Total
        // 16 dict externs declared.
        //
        // Set: 5 externs (new, insert, contains, len, drop).
        //
        // Tuple: 4 externs (new, set, get, drop).
        // -----------------------------------------------------------------

        // --- Dict erased / untyped helpers ----------------------------------

        // __cobrust_dict_new(k_size: i64, v_size: i64, len: i64) -> *mut DictBuffer
        let dict_new_ty = ptr_ty.fn_type(&[i64_ty.into(), i64_ty.into(), i64_ty.into()], false);
        let dict_new_fn =
            self.module
                .add_function("__cobrust_dict_new", dict_new_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_dict_new", dict_new_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_new", 3);

        // __cobrust_dict_drop(*mut DictBuffer) -> void
        let dict_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        let dict_drop_fn =
            self.module
                .add_function("__cobrust_dict_drop", dict_drop_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_dict_drop", dict_drop_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_drop", 1);

        // __cobrust_dict_len(*mut DictBuffer) -> i64
        let dict_len_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let dict_len_fn =
            self.module
                .add_function("__cobrust_dict_len", dict_len_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_dict_len", dict_len_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_len", 1);

        // __cobrust_dict_is_empty(*mut DictBuffer) -> i64 (0/1)
        let dict_is_empty_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let dict_is_empty_fn = self.module.add_function(
            "__cobrust_dict_is_empty",
            dict_is_empty_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_dict_is_empty", dict_is_empty_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_is_empty", 1);

        // --- Dict legacy untyped (i64, i64) shims --------------------------

        // __cobrust_dict_set(p, k: i64, v: i64) -> void
        let dict_set_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
        let dict_set_fn =
            self.module
                .add_function("__cobrust_dict_set", dict_set_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_dict_set", dict_set_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_set", 3);

        // __cobrust_dict_get(p, k: i64) -> i64
        let dict_get_ty = i64_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let dict_get_fn =
            self.module
                .add_function("__cobrust_dict_get", dict_get_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_dict_get", dict_get_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_get", 2);

        // --- Dict typed (K, V) shims — ADR-0050d Decision 7A ---------------

        // (i64, i64) typed shims.
        // __cobrust_dict_set_i64_i64(p, i64, i64) -> void
        let dict_set_i64_i64_ty =
            void_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
        let dict_set_i64_i64_fn = self.module.add_function(
            "__cobrust_dict_set_i64_i64",
            dict_set_i64_i64_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_dict_set_i64_i64", dict_set_i64_i64_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_set_i64_i64", 3);

        // __cobrust_dict_get_i64_i64(p, i64) -> i64
        let dict_get_i64_i64_ty = i64_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let dict_get_i64_i64_fn = self.module.add_function(
            "__cobrust_dict_get_i64_i64",
            dict_get_i64_i64_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_dict_get_i64_i64", dict_get_i64_i64_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_get_i64_i64", 2);

        // __cobrust_dict_contains_i64(p, i64) -> i64 (0/1)
        let dict_contains_i64_ty = i64_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let dict_contains_i64_fn = self.module.add_function(
            "__cobrust_dict_contains_i64",
            dict_contains_i64_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_dict_contains_i64", dict_contains_i64_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_contains_i64", 2);

        // (i64, str) typed shims.
        // __cobrust_dict_set_i64_str(p, i64, *mut Str) -> void
        let dict_set_i64_str_ty =
            void_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), ptr_ty.into()], false);
        let dict_set_i64_str_fn = self.module.add_function(
            "__cobrust_dict_set_i64_str",
            dict_set_i64_str_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_dict_set_i64_str", dict_set_i64_str_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_set_i64_str", 3);

        // __cobrust_dict_get_i64_str(p, i64) -> *mut Str
        let dict_get_i64_str_ty = ptr_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let dict_get_i64_str_fn = self.module.add_function(
            "__cobrust_dict_get_i64_str",
            dict_get_i64_str_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_dict_get_i64_str", dict_get_i64_str_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_get_i64_str", 2);

        // (str, i64) typed shims.
        // __cobrust_dict_set_str_i64(p, *mut Str, i64) -> void
        let dict_set_str_i64_ty =
            void_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), i64_ty.into()], false);
        let dict_set_str_i64_fn = self.module.add_function(
            "__cobrust_dict_set_str_i64",
            dict_set_str_i64_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_dict_set_str_i64", dict_set_str_i64_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_set_str_i64", 3);

        // __cobrust_dict_get_str_i64(p, *mut Str) -> i64
        let dict_get_str_i64_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let dict_get_str_i64_fn = self.module.add_function(
            "__cobrust_dict_get_str_i64",
            dict_get_str_i64_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_dict_get_str_i64", dict_get_str_i64_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_get_str_i64", 2);

        // __cobrust_dict_contains_str(p, *mut Str) -> i64 (0/1)
        let dict_contains_str_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let dict_contains_str_fn = self.module.add_function(
            "__cobrust_dict_contains_str",
            dict_contains_str_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_dict_contains_str", dict_contains_str_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_contains_str", 2);

        // (str, str) typed shims.
        // __cobrust_dict_set_str_str(p, *mut Str, *mut Str) -> void
        let dict_set_str_str_ty =
            void_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        let dict_set_str_str_fn = self.module.add_function(
            "__cobrust_dict_set_str_str",
            dict_set_str_str_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_dict_set_str_str", dict_set_str_str_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_set_str_str", 3);

        // __cobrust_dict_get_str_str(p, *mut Str) -> *mut Str
        let dict_get_str_str_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let dict_get_str_str_fn = self.module.add_function(
            "__cobrust_dict_get_str_str",
            dict_get_str_str_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_dict_get_str_str", dict_get_str_str_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_dict_get_str_str", 2);

        // --- Set<i64> ------------------------------------------------------

        // __cobrust_set_new(elem_size: i64, len: i64) -> *mut SetBuffer
        let set_new_ty = ptr_ty.fn_type(&[i64_ty.into(), i64_ty.into()], false);
        let set_new_fn =
            self.module
                .add_function("__cobrust_set_new", set_new_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_set_new", set_new_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_set_new", 2);

        // __cobrust_set_insert(*mut SetBuffer, i64) -> void
        let set_insert_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let set_insert_fn = self.module.add_function(
            "__cobrust_set_insert",
            set_insert_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_set_insert", set_insert_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_set_insert", 2);

        // __cobrust_set_contains(*mut SetBuffer, i64) -> i64 (0/1)
        let set_contains_ty = i64_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let set_contains_fn = self.module.add_function(
            "__cobrust_set_contains",
            set_contains_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_set_contains", set_contains_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_set_contains", 2);

        // __cobrust_set_len(*mut SetBuffer) -> i64
        let set_len_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let set_len_fn =
            self.module
                .add_function("__cobrust_set_len", set_len_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_set_len", set_len_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_set_len", 1);

        // __cobrust_set_drop(*mut SetBuffer) -> void
        let set_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        let set_drop_fn =
            self.module
                .add_function("__cobrust_set_drop", set_drop_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_set_drop", set_drop_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_set_drop", 1);

        // --- Tuple<i64, ...> -----------------------------------------------

        // __cobrust_tuple_new(n: i64) -> *mut TupleBuffer
        let tuple_new_ty = ptr_ty.fn_type(&[i64_ty.into()], false);
        let tuple_new_fn =
            self.module
                .add_function("__cobrust_tuple_new", tuple_new_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_tuple_new", tuple_new_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_tuple_new", 1);

        // __cobrust_tuple_set(*mut TupleBuffer, i64, i64) -> void
        let tuple_set_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
        let tuple_set_fn =
            self.module
                .add_function("__cobrust_tuple_set", tuple_set_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_tuple_set", tuple_set_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_tuple_set", 3);

        // __cobrust_tuple_get(*mut TupleBuffer, i64) -> i64
        let tuple_get_ty = i64_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let tuple_get_fn =
            self.module
                .add_function("__cobrust_tuple_get", tuple_get_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_tuple_get", tuple_get_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_tuple_get", 2);

        // __cobrust_tuple_drop(*mut TupleBuffer, n: i64) -> void
        // NOTE: Cranelift sig `[p, i64] -> ()` — tuple_drop takes the
        // arity as a second arg, unlike list_drop (single arg).
        let tuple_drop_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let tuple_drop_fn = self.module.add_function(
            "__cobrust_tuple_drop",
            tuple_drop_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_tuple_drop", tuple_drop_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_tuple_drop", 2);

        // -----------------------------------------------------------------
        // ADR-0058g sub-wave-4 — input + read_line runtime extern hookup.
        // Mirrors Cranelift `cranelift_backend.rs:2811-2819` ABI verbatim
        // (sigs cross-verified against stdlib exports at
        // `cobrust-stdlib/src/io.rs:224,248,268,343`).
        //
        // Source-level surface (ADR-0044 W2 Phase 2):
        //   - `input(prompt: str_literal) -> str` lowers to
        //     `__cobrust_input(prompt_ptr, prompt_len)` — the prompt is a
        //     string literal split into (ptr, len) via the wave-2
        //     `expand_str_to_ptr_len` path in `lower_call`.
        //   - `input(prompt: str_buffer) -> str` lowers to
        //     `__cobrust_input_str_buf(prompt_buf)` — the runtime Str
        //     buffer overload (handles non-literal prompts).
        //   - `input_no_prompt() -> str` lowers to
        //     `__cobrust_input_no_prompt()` — zero-arg empty-prompt path.
        //   - `read_line() -> str` lowers to `__cobrust_read_line()` —
        //     low-level stdin line reader (preserves trailing `\n`;
        //     EOF returns empty Str). W2 cap; typed `Result[str, IoError]`
        //     deferred to ADR-0044a.
        //
        // All four helpers return `*mut StrBuffer` (`ptr_ty`); none Drop-
        // schedule at this layer (the str return value is owned by the
        // caller; existing `__cobrust_str_drop` covers the Drop path).
        //
        // F35-sibling discipline: this sub-wave lands input + read_line
        // ONLY. Six of twelve F45a §2 categories will be resolved post-
        // merge (panic + argv + list + dict + set/tuple + input). The
        // remaining 6 (fmt / iter / math / parse_int+str-parsing /
        // str-methods / LLM router) continue as wave-1 stubs.
        // -----------------------------------------------------------------

        // __cobrust_input(prompt_ptr: *const u8, prompt_len: usize) -> *mut Str
        // F71: 2nd arg is `usize` (io.rs `__cobrust_input`) — pointer-width.
        let input_ty = ptr_ty.fn_type(&[ptr_ty.into(), usize_ty.into()], false);
        let input_fn =
            self.module
                .add_function("__cobrust_input", input_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_input", input_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_input", 2);

        // __cobrust_input_str_buf(prompt_buf: *mut Str) -> *mut Str
        let input_str_buf_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let input_str_buf_fn = self.module.add_function(
            "__cobrust_input_str_buf",
            input_str_buf_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_input_str_buf", input_str_buf_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_input_str_buf", 1);

        // __cobrust_input_no_prompt() -> *mut Str
        let input_no_prompt_ty = ptr_ty.fn_type(&[], false);
        let input_no_prompt_fn = self.module.add_function(
            "__cobrust_input_no_prompt",
            input_no_prompt_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_input_no_prompt", input_no_prompt_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_input_no_prompt", 0);

        // __cobrust_read_line() -> *mut Str
        let read_line_ty = ptr_ty.fn_type(&[], false);
        let read_line_fn =
            self.module
                .add_function("__cobrust_read_line", read_line_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_read_line", read_line_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_read_line", 0);

        // -----------------------------------------------------------------
        // ADR-0058g sub-wave-5 — fmt / iter / math / parse_int+str-parsing /
        // str-methods runtime extern hookup. Mirrors Cranelift backend at
        // `cranelift_backend.rs:2765-2894` ABI verbatim. Each helper sig is
        // cross-verified against the stdlib export at
        // `cobrust-stdlib/src/{fmt,iter,math,io,string}.rs`.
        //
        // F35-sibling discipline: this sub-wave lands the FIVE listed
        // categories ONLY. After merge, F45a §2 status becomes 11/12
        // RESOLVED — the LLM router family (sub-wave-6) continues as
        // wave-1 stubs. DO NOT read sub-wave-5 closure as wave-3 closure.
        // -----------------------------------------------------------------

        // -- fmt family (ADR-0064 §3.3 + ADR-0044 W2 + M-F.3.3) -----------
        // __cobrust_fmt_int(buf: *mut Str, v: i64) -> void
        let fmt_int_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let fmt_int_fn =
            self.module
                .add_function("__cobrust_fmt_int", fmt_int_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_fmt_int", fmt_int_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_fmt_int", 2);

        // __cobrust_fmt_float(buf: *mut Str, v: f64) -> void
        let fmt_float_ty = void_ty.fn_type(&[ptr_ty.into(), f64_ty.into()], false);
        let fmt_float_fn =
            self.module
                .add_function("__cobrust_fmt_float", fmt_float_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_fmt_float", fmt_float_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_fmt_float", 2);

        // __cobrust_fmt_float_prec(buf: *mut Str, v: f64, spec_ptr: *const u8, spec_len: i64) -> void
        let fmt_float_prec_ty = void_ty.fn_type(
            &[ptr_ty.into(), f64_ty.into(), ptr_ty.into(), i64_ty.into()],
            false,
        );
        let fmt_float_prec_fn = self.module.add_function(
            "__cobrust_fmt_float_prec",
            fmt_float_prec_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_fmt_float_prec", fmt_float_prec_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_fmt_float_prec", 4);

        // __cobrust_fmt_bool(buf: *mut Str, v: i64) -> void
        let fmt_bool_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let fmt_bool_fn =
            self.module
                .add_function("__cobrust_fmt_bool", fmt_bool_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_fmt_bool", fmt_bool_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_fmt_bool", 2);

        // __cobrust_fmt_str(buf: *mut Str, ptr: *const u8, len: i64) -> void
        let fmt_str_ty = void_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), i64_ty.into()], false);
        let fmt_str_fn =
            self.module
                .add_function("__cobrust_fmt_str", fmt_str_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_fmt_str", fmt_str_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_fmt_str", 3);

        // __cobrust_fmt_repr(buf: *mut Str, _ptr: *mut u8, type_id: i64) -> void
        let fmt_repr_ty = void_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), i64_ty.into()], false);
        let fmt_repr_fn =
            self.module
                .add_function("__cobrust_fmt_repr", fmt_repr_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_fmt_repr", fmt_repr_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_fmt_repr", 3);

        // __cobrust_str_len(buf: *mut Str) -> i64
        let str_len_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let str_len_fn =
            self.module
                .add_function("__cobrust_str_len", str_len_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_len", str_len_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_len", 1);

        // __cobrust_str_ptr(buf: *mut Str) -> *const u8
        let str_ptr_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let str_ptr_fn =
            self.module
                .add_function("__cobrust_str_ptr", str_ptr_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_ptr", str_ptr_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_ptr", 1);

        // __cobrust_str_clone(buf: *mut Str) -> *mut Str  (ADR-0050c §"Phase 3")
        let str_clone_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let str_clone_fn =
            self.module
                .add_function("__cobrust_str_clone", str_clone_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_clone", str_clone_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_clone", 1);

        // -- iter family (for-protocol; ADR-0044 W2 Phase 2) --------------
        // __cobrust_iter_init(iter_val: i64) -> *mut IterHandle
        let iter_init_ty = ptr_ty.fn_type(&[i64_ty.into()], false);
        let iter_init_fn =
            self.module
                .add_function("__cobrust_iter_init", iter_init_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_iter_init", iter_init_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_iter_init", 1);

        // __cobrust_iter_next(handle: *mut IterHandle) -> i64  (0 = end)
        let iter_next_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let iter_next_fn =
            self.module
                .add_function("__cobrust_iter_next", iter_next_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_iter_next", iter_next_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_iter_next", 1);

        // __cobrust_iter_drop(handle: *mut IterHandle) -> void
        let iter_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        let iter_drop_fn =
            self.module
                .add_function("__cobrust_iter_drop", iter_drop_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_iter_drop", iter_drop_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_iter_drop", 1);

        // -- math family (M-F.3.3 gap (b)) --------------------------------
        // Single-arg f64 → f64 shims.
        for sym in [
            "__cobrust_math_sqrt",
            "__cobrust_math_floor",
            "__cobrust_math_ceil",
            "__cobrust_math_round",
            "__cobrust_math_abs",
            "__cobrust_math_sin",
            "__cobrust_math_cos",
            "__cobrust_math_tan",
            "__cobrust_math_log",
            "__cobrust_math_exp",
            // ADR-0083 PART-2: `math.degrees` / `math.radians` (`f64 -> f64`
            // angle-conversion shims, same single-arg ABI as the family
            // above). DISTINCT from the INT-returning `_floor_int` etc.
            // (declared below) and the BOOL-returning `_isnan` etc.
            "__cobrust_math_degrees",
            "__cobrust_math_radians",
        ] {
            let ty = f64_ty.fn_type(&[f64_ty.into()], false);
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, 1);
        }

        // -- ADR-0083 PART-2: INT-returning rounding shims (`(f64) -> i64`)
        // `math.floor` / `math.ceil` / `math.trunc` return CPython `int`.
        // These `__cobrust_math_*_int` symbols are DISTINCT from the
        // f64-returning `__cobrust_math_floor` / `_ceil` above (the
        // bare-function `floor(x)` PRELUDE path) — same arg, different
        // RETURN type. The `(ptr_like f64) -> i64` shape mirrors
        // `coil.argmin`'s `(*Buffer) -> i64`; the i64 lands in the `.cb`
        // `_ecoret` Int local via the generic ecosystem-call path.
        let math_f64_i64_ty = i64_ty.fn_type(&[f64_ty.into()], false);
        for sym in [
            "__cobrust_math_floor_int",
            "__cobrust_math_ceil_int",
            "__cobrust_math_trunc_int",
        ] {
            let f = self
                .module
                .add_function(sym, math_f64_i64_ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, 1);
        }

        // -- ADR-0083 PART-2: BOOL-returning classification shims
        // (`(f64) -> i1`). `math.isnan` / `math.isinf` / `math.isfinite`.
        // The Rust C-ABI `-> bool` is declared as `bool_type()` (LLVM `i1`),
        // mirroring `coil.any`/`coil.all` (`__cobrust_coil_any`'s
        // `bool_type().fn_type(...)`) + `fang.verify_password` EXACTLY; the
        // i1 lands in the `.cb` `_ecoret` Bool local (`write_place` bridges
        // any i1/i8 width gap into the alloca), usable directly in an
        // `if math.isnan(x):` condition.
        let math_f64_bool_ty = self.ctx.bool_type().fn_type(&[f64_ty.into()], false);
        for sym in [
            "__cobrust_math_isnan",
            "__cobrust_math_isinf",
            "__cobrust_math_isfinite",
        ] {
            let f = self
                .module
                .add_function(sym, math_f64_bool_ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, 1);
        }

        // Two-arg f64 × f64 → f64.
        let math_pow_ty = f64_ty.fn_type(&[f64_ty.into(), f64_ty.into()], false);
        let math_pow_fn =
            self.module
                .add_function("__cobrust_math_pow", math_pow_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_math_pow", math_pow_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_math_pow", 2);

        // -- ADR-0083: `import math` (scalar stdlib) — BARE libm externs ---
        // `math.X` lowers (via the ecosystem-call path → `Terminator::Call`
        // onto a `Constant::Str` runtime symbol) to a DIRECT call into the
        // standard C-library `libm`. The symbols are the BARE libm names
        // (`sqrt`, `sin`, `atan2`, `hypot`, …) — NOT `__cobrust_math_*`
        // shims. libm is ALWAYS linked: coil's Rust kernels + the embedded
        // Rust std in `libcobrust_stdlib.a` pull it (macOS: libSystem;
        // Linux: libm via the C runtime). NO new crate, NO cabi, NO
        // ecosystem archive (ADR-0083 §"Lowering"). The extern-name dispatch
        // in `lower_call` already handles the `f64 -> f64` / `(f64,f64) ->
        // f64` ABI (incl. the i64-bits → f64 bitcast for a `Ty::None` binary-
        // op-result arg) and the f64 return — identical to `__cobrust_math_*`.
        //
        // These names are DISTINCT from the `coil.sqrt`/`coil.sin` BUFFER
        // ufuncs (`__cobrust_coil_*`, `Buffer -> Buffer`) — math is scalar.
        // 15 single-arg `f64 -> f64`.
        let libm_f64_f64_ty = f64_ty.fn_type(&[f64_ty.into()], false);
        for sym in [
            "sqrt", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp",
            "log", "log10", "log2", "fabs",
        ] {
            let f = self
                .module
                .add_function(sym, libm_f64_f64_ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, 1);
        }
        // 5 two-arg `(f64, f64) -> f64` — `pow(x,y)`, `atan2(y,x)`,
        // `hypot(x,y)`, plus ADR-0083 PART-2's `copysign(x,y)` /
        // `fmod(x,y)` (also BARE libm two-arg symbols — NO `__cobrust_math_*`
        // shim, exactly like part-1's `pow`/`atan2`/`hypot`).
        let libm_f64f64_f64_ty = f64_ty.fn_type(&[f64_ty.into(), f64_ty.into()], false);
        for sym in ["pow", "atan2", "hypot", "copysign", "fmod"] {
            let f = self
                .module
                .add_function(sym, libm_f64f64_f64_ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, 2);
        }

        // -- parse_int + str-parsing family (ADR-0044 W2 Phase 3) ---------
        // __cobrust_parse_int(s: *mut Str) -> i64
        let parse_int_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let parse_int_fn =
            self.module
                .add_function("__cobrust_parse_int", parse_int_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_parse_int", parse_int_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_parse_int", 1);

        // __cobrust_str_len_src(s: *mut Str) -> i64
        let str_len_src_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let str_len_src_fn = self.module.add_function(
            "__cobrust_str_len_src",
            str_len_src_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_str_len_src", str_len_src_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_len_src", 1);

        // __cobrust_str_at(s: *mut Str, i: i64) -> *mut Str
        let str_at_ty = ptr_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let str_at_fn =
            self.module
                .add_function("__cobrust_str_at", str_at_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_at", str_at_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_at", 2);

        // __cobrust_str_eq(a: *mut Str, b: *mut Str) -> i64
        let str_eq_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let str_eq_fn =
            self.module
                .add_function("__cobrust_str_eq", str_eq_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_eq", str_eq_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_eq", 2);

        // __cobrust_str_concat(a: *mut Str, b: *mut Str) -> *mut Str
        // The runtime target of the `.cb` `str + str` operator (natural
        // concatenation; sibling of `__cobrust_str_eq` for `str == str`).
        // Returns a freshly-allocated Str buffer freed by the Str drop
        // schedule at scope exit.
        let str_concat_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let str_concat_fn = self.module.add_function(
            "__cobrust_str_concat",
            str_concat_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_str_concat", str_concat_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_concat", 2);

        // __cobrust_str_eq_lit(s: *mut Str, lit: *const u8, lit_len: i64) -> i64
        let str_eq_lit_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), i64_ty.into()], false);
        let str_eq_lit_fn = self.module.add_function(
            "__cobrust_str_eq_lit",
            str_eq_lit_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_str_eq_lit", str_eq_lit_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_eq_lit", 3);

        // __cobrust_str_ord(s: *mut Str) -> i64
        let str_ord_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let str_ord_fn =
            self.module
                .add_function("__cobrust_str_ord", str_ord_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_ord", str_ord_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_ord", 1);

        // __cobrust_parse_int_tok(line: *mut Str, i: i64) -> i64
        let parse_int_tok_ty = i64_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let parse_int_tok_fn = self.module.add_function(
            "__cobrust_parse_int_tok",
            parse_int_tok_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_parse_int_tok", parse_int_tok_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_parse_int_tok", 2);

        // __cobrust_count_toks(line: *mut Str) -> i64
        let count_toks_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let count_toks_fn = self.module.add_function(
            "__cobrust_count_toks",
            count_toks_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_count_toks", count_toks_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_count_toks", 1);

        // -- str-method family (M-F.3.5 string stdlib, ADR-0050e) ---------
        // All Str pointers are `ptr_ty`. Predicate fns return i64 (0/1);
        // find returns i64 (-1 sentinel for not-found).
        // __cobrust_str_split(s: *mut Str, sep: *mut Str) -> *mut List
        let str_split_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let str_split_fn =
            self.module
                .add_function("__cobrust_str_split", str_split_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_split", str_split_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_split", 2);

        // __cobrust_str_join(parts: *mut List, sep: *mut Str) -> *mut Str
        let str_join_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let str_join_fn =
            self.module
                .add_function("__cobrust_str_join", str_join_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_join", str_join_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_join", 2);

        // __cobrust_str_replace(s, old, new) -> *mut Str
        let str_replace_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        let str_replace_fn = self.module.add_function(
            "__cobrust_str_replace",
            str_replace_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_str_replace", str_replace_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_replace", 3);

        // __cobrust_str_trim(s: *mut Str) -> *mut Str
        let str_trim_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let str_trim_fn =
            self.module
                .add_function("__cobrust_str_trim", str_trim_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_trim", str_trim_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_trim", 1);

        // __cobrust_str_find(s, needle) -> i64  (-1 sentinel for not-found)
        let str_find_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let str_find_fn =
            self.module
                .add_function("__cobrust_str_find", str_find_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_find", str_find_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_find", 2);

        // __cobrust_str_contains(s, needle) -> i64  (0/1)
        let str_contains_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let str_contains_fn = self.module.add_function(
            "__cobrust_str_contains",
            str_contains_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_str_contains", str_contains_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_contains", 2);

        // __cobrust_str_starts_with(s, prefix) -> i64  (0/1)
        let str_starts_with_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let str_starts_with_fn = self.module.add_function(
            "__cobrust_str_starts_with",
            str_starts_with_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_str_starts_with", str_starts_with_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_starts_with", 2);

        // __cobrust_str_ends_with(s, suffix) -> i64  (0/1)
        let str_ends_with_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let str_ends_with_fn = self.module.add_function(
            "__cobrust_str_ends_with",
            str_ends_with_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_str_ends_with", str_ends_with_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_ends_with", 2);

        // __cobrust_str_lower(s: *mut Str) -> *mut Str
        let str_lower_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let str_lower_fn =
            self.module
                .add_function("__cobrust_str_lower", str_lower_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_lower", str_lower_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_lower", 1);

        // __cobrust_str_upper(s: *mut Str) -> *mut Str
        let str_upper_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let str_upper_fn =
            self.module
                .add_function("__cobrust_str_upper", str_upper_ty, Some(Linkage::External));
        self.runtime_helper_decls
            .insert("__cobrust_str_upper", str_upper_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_str_upper", 1);

        // ===========================================================
        // ADR-0058g sub-wave-6 — LLM router intrinsics (final wave-3
        // category, F45a §2 row 12). Cranelift parity verbatim:
        //   `cranelift_backend.rs:2896-2961` (M-AI.0/AI.1/AI.2).
        //
        // Stdlib ABI cross-confirmed at:
        //   - `cobrust-stdlib/src/llm.rs:422,444,466`
        //   - `cobrust-stdlib/src/prompt.rs:247,270,291,308,324`
        //   - `cobrust-stdlib/src/tool.rs:254,278,289,306,321`
        //
        // All 13 helpers use the `(*mut Str | *mut List) -> *mut Str | *mut List`
        // opaque-pointer ABI. Decision 7 (M-AI.0 α Phase 2): every
        // failure path returns an empty `Str` (or empty List for
        // `llm_stream` / `tool_registry_new`) — symbols are
        // unconditionally exported by `cobrust-stdlib` and resolve
        // cleanly at link time without requiring a configured
        // `cobrust.toml` or live LLM provider.
        // ===========================================================

        // -- M-AI.0 (α Phase 2): cobrust.llm source-level binding -----
        // `llm_complete(provider, model, prompt) -> str`.
        let llm_complete_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        let llm_complete_fn = self.module.add_function(
            "__cobrust_llm_complete",
            llm_complete_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_llm_complete", llm_complete_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_llm_complete", 3);

        // `llm_dispatch(task, prompt) -> str`.
        let llm_dispatch_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let llm_dispatch_fn = self.module.add_function(
            "__cobrust_llm_dispatch",
            llm_dispatch_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_llm_dispatch", llm_dispatch_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_llm_dispatch", 2);

        // `llm_stream(provider, model, prompt) -> list[str]`.
        let llm_stream_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        let llm_stream_fn = self.module.add_function(
            "__cobrust_llm_stream",
            llm_stream_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_llm_stream", llm_stream_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_llm_stream", 3);

        // -- M-AI.1 (α Phase 3): cobrust.prompt source-level binding --
        // `prompt_render(system, user, vars) -> str`.
        let prompt_render_ty =
            ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        let prompt_render_fn = self.module.add_function(
            "__cobrust_prompt_render",
            prompt_render_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_prompt_render", prompt_render_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_prompt_render", 3);

        // `prompt_format_few_shot(examples_in, examples_out, current_input) -> str`.
        let prompt_few_shot_ty =
            ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        let prompt_few_shot_fn = self.module.add_function(
            "__cobrust_prompt_format_few_shot",
            prompt_few_shot_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_prompt_format_few_shot", prompt_few_shot_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_prompt_format_few_shot", 3);

        // `prompt_format_system_user(system, user) -> str`.
        let prompt_sys_user_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let prompt_sys_user_fn = self.module.add_function(
            "__cobrust_prompt_format_system_user",
            prompt_sys_user_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_prompt_format_system_user", prompt_sys_user_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_prompt_format_system_user", 2);

        // `prompt_escape_braces(text) -> str`.
        let prompt_escape_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let prompt_escape_fn = self.module.add_function(
            "__cobrust_prompt_escape_braces",
            prompt_escape_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_prompt_escape_braces", prompt_escape_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_prompt_escape_braces", 1);

        // `llm_complete_structured(prompt, schema_json) -> str`.
        let llm_structured_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let llm_structured_fn = self.module.add_function(
            "__cobrust_llm_complete_structured",
            llm_structured_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_llm_complete_structured", llm_structured_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_llm_complete_structured", 2);

        // -- M-AI.2 (α Phase 4): cobrust.tool source-level binding ---
        // `tool_schema(name, description, params_json, returns_json) -> str`.
        let tool_schema_ty = ptr_ty.fn_type(
            &[ptr_ty.into(), ptr_ty.into(), ptr_ty.into(), ptr_ty.into()],
            false,
        );
        let tool_schema_fn = self.module.add_function(
            "__cobrust_tool_schema",
            tool_schema_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_tool_schema", tool_schema_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_tool_schema", 4);

        // `tool_registry_new() -> *mut Registry`.
        let tool_registry_new_ty = ptr_ty.fn_type(&[], false);
        let tool_registry_new_fn = self.module.add_function(
            "__cobrust_tool_registry_new",
            tool_registry_new_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_tool_registry_new", tool_registry_new_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_tool_registry_new", 0);

        // `tool_registry_register(registry, schema_json) -> *mut Registry`.
        let tool_registry_register_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let tool_registry_register_fn = self.module.add_function(
            "__cobrust_tool_registry_register",
            tool_registry_register_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls.insert(
            "__cobrust_tool_registry_register",
            tool_registry_register_fn,
        );
        self.runtime_helper_param_counts
            .insert("__cobrust_tool_registry_register", 2);

        // `tool_invoke(tool_name, args_json) -> str`.
        let tool_invoke_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let tool_invoke_fn = self.module.add_function(
            "__cobrust_tool_invoke",
            tool_invoke_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_tool_invoke", tool_invoke_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_tool_invoke", 2);

        // `llm_complete_with_tools(prompt, registry_json) -> str`.
        let llm_with_tools_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let llm_with_tools_fn = self.module.add_function(
            "__cobrust_llm_complete_with_tools",
            llm_with_tools_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_llm_complete_with_tools", llm_with_tools_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_llm_complete_with_tools", 2);

        // -- v0.7.0 Stream Z.5: std.json source-level binding --------
        // The `(*mut Str) -> *mut Str` opaque-pointer ABI, mirroring the
        // str->str helpers above. Symbols exported unconditionally by
        // `cobrust-stdlib/src/json.rs` (`__cobrust_json_dumps` /
        // `__cobrust_json_dumps_indent` / `__cobrust_json_loads`); failure
        // paths return a sentinel Str so they resolve at link time without
        // a live config. `json_dumps_indent`'s second param is `i64`
        // (the indent width), marshalled directly by the extern-call path
        // (no str expansion). Closes the Z.5 codegen-wiring gap (the stdlib
        // module + frontend prelude + cli intrinsic-rewrite were landed in
        // the Z.5 merge; this is the missing lowering).
        // `json_dumps(json_input) -> str`.
        let json_dumps_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let json_dumps_fn = self.module.add_function(
            "__cobrust_json_dumps",
            json_dumps_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_json_dumps", json_dumps_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_json_dumps", 1);

        // `json_dumps_indent(json_input, indent: i64) -> str`.
        let json_dumps_indent_ty = ptr_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let json_dumps_indent_fn = self.module.add_function(
            "__cobrust_json_dumps_indent",
            json_dumps_indent_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_json_dumps_indent", json_dumps_indent_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_json_dumps_indent", 2);

        // `json_loads(s) -> str`.
        let json_loads_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let json_loads_fn = self.module.add_function(
            "__cobrust_json_loads",
            json_loads_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_json_loads", json_loads_fn);
        self.runtime_helper_param_counts
            .insert("__cobrust_json_loads", 1);

        // -- M-F.3.6: file IO source-level binding (ADR-0050f) --------
        // F60: these were declared in the deleted cranelift_backend.rs
        // `runtime_helper_signatures` but never in the LLVM backend —
        // latent because `file_io_e2e.rs` is all-#[ignore]'d (pre-impl)
        // and the doc-coverage M-F.3.6 check grepped the Cranelift
        // backend. The §X.4 removal surfaced the asymmetry; restore the
        // codegen scaffolding here (sole AOT backend) so file IO can lower
        // under LLVM. Signatures verbatim from `cranelift_backend.rs`
        // (commit f16bdab): read_file/read_file_lines `(ptr)->ptr`,
        // write/append_file `(ptr,ptr)->i64`, stdin_read_all `()->ptr`,
        // stdout/stderr_write `(ptr)->i64`.
        let ptr_to_ptr_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let ptr_ptr_to_i64_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let ptr_to_i64_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_read_file", ptr_to_ptr_ty, 1usize),
            ("__cobrust_read_file_lines", ptr_to_ptr_ty, 1),
            ("__cobrust_write_file", ptr_ptr_to_i64_ty, 2),
            ("__cobrust_append_file", ptr_ptr_to_i64_ty, 2),
            ("__cobrust_stdin_read_all", ptr_ty.fn_type(&[], false), 0),
            ("__cobrust_stdout_write", ptr_to_i64_ty, 1),
            ("__cobrust_stderr_write", ptr_to_i64_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0072: den ecosystem-module C-ABI binding ------------
        // The first-proof `den` shims over the opaque-pointer ABI
        // (handles are `*mut u8` Boxed pointers; str args/returns are
        // Cobrust Str buffers). Exported by `cobrust-den/src/cabi.rs`,
        // linked as `libden.a` only when the program imports `den`
        // (per-import link in `cobrust-cli/src/build.rs`).
        //
        //   __cobrust_den_connect(path: *mut Str) -> *mut Connection
        //   __cobrust_den_connection_execute(conn, sql: *mut Str) -> *mut Cursor
        //   __cobrust_den_cursor_fetchall(cur) -> *mut Str
        //   __cobrust_den_connection_drop(conn) -> void
        //   __cobrust_den_cursor_drop(cur) -> void
        //
        // The two `*_drop` symbols are emitted by `emit_drop_for_ty` at
        // a handle local's scope exit (the nominal `Ty::Adt` handle is
        // non-Copy → drop-scheduled). All handle/str values cross as
        // opaque pointers; no ptr+len expansion (no string-literal
        // expansion path — `connect`'s param_count is 1, not 2).
        let den_connect_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let den_execute_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let den_fetchall_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let den_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_den_connect", den_connect_ty, 1usize),
            ("__cobrust_den_connection_execute", den_execute_ty, 2),
            ("__cobrust_den_cursor_fetchall", den_fetchall_ty, 1),
            ("__cobrust_den_connection_drop", den_drop_ty, 1),
            ("__cobrust_den_cursor_drop", den_drop_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0072 second-module proof: nest ecosystem-module C-ABI -
        // `nest` (TOML, rebrand of tomli) — pure value-in-value-out
        // (`Str → Str`); no handles, no drops. Exported by
        // `cobrust-nest/src/cabi.rs`, linked as `libnest.a` only when
        // the program imports `nest`.
        //
        //   __cobrust_nest_loads_str(toml: *mut Str) -> *mut Str
        //
        // The returned Str buffer is owned by the caller and dropped by
        // the existing Str drop schedule — no new drop wiring needed.
        // (One symbol today; an array+for like den's above would trip
        // clippy::single_element_loop, so direct binding is used. New
        // nest fns extend this with the same shape.)
        let nest_loads_str_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let nest_loads_str_sym = "__cobrust_nest_loads_str";
        let f = self.module.add_function(
            nest_loads_str_sym,
            nest_loads_str_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls.insert(nest_loads_str_sym, f);
        self.runtime_helper_param_counts
            .insert(nest_loads_str_sym, 1);

        // -- ADR-0072 third-module proof: strike ecosystem-module C-ABI -
        // `strike` (HTTP client, rebrand of requests) — pairs handle
        // pattern (Response, like den's Connection/Cursor) with free-
        // function entrypoints (`get`/`post`, like den's `connect`).
        // Exported by `cobrust-strike/src/cabi.rs`, linked as
        // `libstrike.a` only when the program imports `strike`.
        //
        //   __cobrust_strike_get(url: *mut Str) -> *mut Response
        //   __cobrust_strike_post(url, body: *mut Str) -> *mut Response
        //   __cobrust_strike_response_text(resp) -> *mut Str
        //   __cobrust_strike_response_status_code(resp) -> i64
        //   __cobrust_strike_response_json(resp) -> *mut Str
        //   __cobrust_strike_response_drop(resp) -> void
        //
        // The `_drop` symbol is emitted by `emit_drop_for_ty` at the
        // handle local's scope exit via `handle_drop_symbol(id)` (chain
        // is already general — no new drop wiring needed).
        let strike_get_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let strike_post_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let strike_text_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let strike_status_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let strike_json_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let strike_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_strike_get", strike_get_ty, 1usize),
            ("__cobrust_strike_post", strike_post_ty, 2),
            ("__cobrust_strike_response_text", strike_text_ty, 1),
            ("__cobrust_strike_response_status_code", strike_status_ty, 1),
            ("__cobrust_strike_response_json", strike_json_ty, 1),
            ("__cobrust_strike_response_drop", strike_drop_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0072 fourth-module proof: scale ecosystem-module C-ABI -
        // `scale` (msgpack, rebrand of msgpack-python) — pure value-in-
        // value-out (`Str → Str`); the first-proof shape mirrors nest's
        // (no handles, no drops; the returned `Str` is freed by the
        // existing Str drop schedule). The str→str round-trip carries
        // a JSON string in, returns its msgpack-hex rendering out
        // (`dumps_str`) or accepts hex-rendered msgpack and returns
        // canonical JSON (`loads_str`). A raw bytes ABI is a tracked
        // follow-up. Exported by `cobrust-scale/src/cabi.rs`, linked as
        // `libscale.a` only when the program imports `scale`.
        //
        //   __cobrust_scale_dumps_str(json: *mut Str) -> *mut Str
        //   __cobrust_scale_loads_str(packed: *mut Str) -> *mut Str
        let scale_dumps_str_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let scale_loads_str_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_scale_dumps_str", scale_dumps_str_ty, 1usize),
            ("__cobrust_scale_loads_str", scale_loads_str_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0072 fifth-module proof: molt ecosystem-module C-ABI --
        // `molt` (datetime, rebrand of python-dateutil) — pairs handle
        // pattern (DateTime, like den's Connection/Cursor and strike's
        // Response) with a free-function entrypoint (`now()`, like
        // den's `connect`). Exported by `cobrust-molt/src/cabi.rs`,
        // linked as `libmolt.a` only when the program imports `molt`.
        //
        //   __cobrust_molt_now() -> *mut DateTime
        //   __cobrust_molt_datetime_isoformat(dt) -> *mut Str
        //   __cobrust_molt_datetime_unix_timestamp(dt) -> i64
        //   __cobrust_molt_datetime_drop(dt) -> void
        //
        // The `_drop` symbol is emitted by `emit_drop_for_ty` at the
        // handle local's scope exit via `handle_drop_symbol(id)` (chain
        // is already general — no new drop wiring needed).
        let molt_now_ty = ptr_ty.fn_type(&[], false);
        let molt_iso_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let molt_unix_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let molt_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_molt_now", molt_now_ty, 0usize),
            ("__cobrust_molt_datetime_isoformat", molt_iso_ty, 1),
            ("__cobrust_molt_datetime_unix_timestamp", molt_unix_ty, 1),
            ("__cobrust_molt_datetime_drop", molt_drop_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0073: pit ecosystem-module C-ABI binding ------------
        // `pit` (Flask, web-server). First ecosystem module that takes
        // a CALLBACK in one of its method args (App.route's 4th param).
        // The callback crosses as a raw fn pointer (`*const c_void` at
        // the LLVM IR level it's `ptr`); the trampoline in
        // `cobrust-pit/src/cabi.rs::__cobrust_pit_app_route` transmutes
        // it to the fixed `unsafe extern "C" fn(*mut u8) -> *mut u8`
        // shape and wraps it in a `move |req| { … }` closure satisfying
        // axum's `Arc<dyn Fn + Send + Sync + 'static>` bound.
        //
        //   __cobrust_pit_app_new() -> *mut App
        //   __cobrust_pit_text_response(status: i64, body: *mut Str) -> *mut Response
        //   __cobrust_pit_app_route(
        //       app: *mut App, method: *mut Str, path: *mut Str,
        //       handler: *const c_void
        //   ) -> *mut u8 = null   (Ty::None — discard channel; the
        //                          route() effect is on `app` in place)
        //   __cobrust_pit_app_serve_in_background(
        //       app: *mut App, host: *mut Str, port: i64
        //   ) -> *mut ServerHandle
        //   __cobrust_pit_app_use_cors(app: *mut App) -> *mut u8 = null
        //   __cobrust_pit_app_use_trace(app: *mut App) -> *mut u8 = null
        //   __cobrust_pit_app_use_compression(app: *mut App) -> *mut u8 = null
        //       (ADR-0078 §6.1 — Ty::None discard; the effect is a
        //        middleware flag flipped on `app`, read at serve time)
        //   __cobrust_pit_app_drop(app) -> void
        //   __cobrust_pit_response_drop(resp) -> void
        //   __cobrust_pit_server_handle_drop(handle) -> void
        let pit_app_new_ty = ptr_ty.fn_type(&[], false);
        let pit_text_response_ty = ptr_ty.fn_type(&[i64_ty.into(), ptr_ty.into()], false);
        // ADR-0081 §5.3 Phase-1a — `__cobrust_pit_json_response(status: i64,
        // body: *mut serde_json::Value) -> *mut Response`. SIBLING of
        // `__cobrust_pit_text_response` with the IDENTICAL `[i64, ptr] -> ptr`
        // shape: the 1st arg is the status, the 2nd is the boxed validated
        // body the `route_validated` trampoline owns (re-serialised, BORROWED
        // — the trampoline still frees it once, `cabi.rs:479`).
        let pit_json_response_ty = pit_text_response_ty;
        let pit_app_route_ty = ptr_ty.fn_type(
            &[ptr_ty.into(), ptr_ty.into(), ptr_ty.into(), ptr_ty.into()],
            false,
        );
        // ADR-0080 Phase-1b-ii — `__cobrust_pit_app_route_validated(
        //     app: *mut App, method: *mut Str, path: *mut Str,
        //     handler: *const c_void, schema: *mut Str
        // ) -> *mut u8 = null`. SIBLING of `__cobrust_pit_app_route` with a
        // FIFTH `schema` arg: the validated-body descriptor MIR synthesises
        // from the handler's body-class field table + refinement side-table
        // (ADR-0080 §5.4). The trampoline parses it, validates `req.json()`
        // against it, and dispatches on Ok / synthesises a typed 422 on Err
        // WITHOUT entering the handler (footgun #1 + #2). The handler crosses
        // as the SAME `*const c_void` fn-pointer shape as `route`; the
        // trampoline transmutes it to the 2-arg `fn(*mut u8, *mut u8) ->
        // *mut u8` validated-handler ABI.
        let pit_app_route_validated_ty = ptr_ty.fn_type(
            &[
                ptr_ty.into(),
                ptr_ty.into(),
                ptr_ty.into(),
                ptr_ty.into(),
                ptr_ty.into(),
            ],
            false,
        );
        let pit_serve_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), i64_ty.into()], false);
        // F65 G2 — `__cobrust_pit_app_run(app: *mut App, host: *mut Str, port: i64) -> i64`.
        // Blocking variant of `serve_in_background`; returns 0 on clean
        // shutdown, non-zero on bind/serve error. The App is taken via
        // `mem::take` inside the trampoline (the original `Box<App>` stays
        // live so the `.cb` scope-exit drop frees the empty App cleanly).
        let pit_app_run_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), i64_ty.into()], false);
        // ADR-0078 §6.1 Phase-1 — tower-http middleware setters. Each is
        // `__cobrust_pit_app_use_*(app: *mut App) -> *mut u8 = null`
        // (Ty::None discard channel; the effect is the middleware flag
        // flipped on `app` in place, read at serve time). Shape is the
        // App-receiver / None-return form: one ptr arg, ptr return —
        // identical to `pit_request_body_ty`.
        let pit_app_use_middleware_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        // ADR-0080 Phase-1b-iii — `__cobrust_pit_app_serve_openapi(
        //     app: *mut App, path: *mut Str
        // ) -> *mut u8 = null`. The EXPLICIT OpenAPI-serving opt-in (§5.3):
        // registers a `GET <path>` route serving the OpenAPI doc derived
        // from the App's accumulated `route_validated` schemas (the SAME
        // source the validator reads — footgun #4). Ty::None discard return
        // (the effect is a route registered on `app` in place, mirroring
        // `route`/`use_*`). Shape is two ptr args, ptr return — identical to
        // `pit_request_path_param_ty`.
        let pit_app_serve_openapi_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        // F65 G1 — `__cobrust_pit_request_body(req: *mut Request) -> *mut Str`.
        // Borrow-shim returning a freshly-allocated Cobrust Str. The
        // Request stays Rust-owned (ADR-0073 §2 D6).
        let pit_request_body_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        // F65 G5 enabling — `__cobrust_pit_request_path_param(req: *mut Request,
        // name: *mut Str) -> *mut Str`. Returns the captured path param value
        // or empty Str.
        let pit_request_path_param_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        // ADR-0081 §5.2 Phase-1b — validated-body field READ accessors.
        // `__cobrust_pit_body_get_i64(body: *mut Value, name: *mut Str) -> i64`
        // and `__cobrust_pit_body_get_str(body: *mut Value, name: *mut Str)
        // -> *mut Str` — cloned from the `(ptr, ptr) -> <ret>` `path_param`
        // shape (the str variant is type-identical to it). `body` is the boxed
        // `serde_json::Value` the `route_validated` trampoline left; `name` is
        // the compiler-synthesised field-name Str the MIR retarget passes. The
        // MIR `Attr` sub-arm emits a `Terminator::Call` to these symbols ONLY
        // for a registration-marked validated-body field read (the Q4 gate).
        let pit_body_get_i64_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let pit_body_get_str_ty = pit_request_path_param_ty;
        let pit_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_pit_app_new", pit_app_new_ty, 0usize),
            ("__cobrust_pit_text_response", pit_text_response_ty, 2),
            ("__cobrust_pit_json_response", pit_json_response_ty, 2),
            ("__cobrust_pit_app_route", pit_app_route_ty, 4),
            (
                "__cobrust_pit_app_route_validated",
                pit_app_route_validated_ty,
                5,
            ),
            ("__cobrust_pit_app_serve_in_background", pit_serve_ty, 3),
            ("__cobrust_pit_app_run", pit_app_run_ty, 3),
            ("__cobrust_pit_app_use_cors", pit_app_use_middleware_ty, 1),
            ("__cobrust_pit_app_use_trace", pit_app_use_middleware_ty, 1),
            (
                "__cobrust_pit_app_use_compression",
                pit_app_use_middleware_ty,
                1,
            ),
            (
                "__cobrust_pit_app_serve_openapi",
                pit_app_serve_openapi_ty,
                2,
            ),
            ("__cobrust_pit_request_body", pit_request_body_ty, 1),
            (
                "__cobrust_pit_request_path_param",
                pit_request_path_param_ty,
                2,
            ),
            ("__cobrust_pit_body_get_i64", pit_body_get_i64_ty, 2),
            ("__cobrust_pit_body_get_str", pit_body_get_str_ty, 2),
            ("__cobrust_pit_app_drop", pit_drop_ty, 1),
            ("__cobrust_pit_response_drop", pit_drop_ty, 1),
            ("__cobrust_pit_server_handle_drop", pit_drop_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0073 second proof: hood ecosystem-module C-ABI binding -----
        // `hood` (click, CLI commands). Second module exercising the
        // ADR-0073 cross-boundary callback chain — proves chain
        // generality off the pit "pong" precedent. The callback shape
        // here is `fn() -> i64` (no positional args; i64 return is the
        // user's exit-code intent). At the wire level the callback uses
        // the SAME C-ABI as pit (`unsafe extern "C" fn(*mut u8) -> *mut u8`)
        // per ADR-0073 §5.1 — the trampoline calls the fn-ptr with a
        // null pointer placeholder and discards the return pointer.
        //
        //   __cobrust_hood_command_new(name: *mut Str, help: *mut Str) -> *mut Command
        //   __cobrust_hood_command_handler(
        //       cmd: *mut Command, handler: *const c_void
        //   ) -> i64 = 0   (Ty::Int sentinel — registration is a
        //                   side-effect on the receiver in place)
        //   __cobrust_hood_command_run(cmd: *mut Command) -> i64
        //   __cobrust_hood_command_drop(cmd) -> void
        let hood_command_new_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let hood_command_handler_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let hood_command_run_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let hood_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_hood_command_new", hood_command_new_ty, 2usize),
            ("__cobrust_hood_command_handler", hood_command_handler_ty, 2),
            ("__cobrust_hood_command_run", hood_command_run_ty, 1),
            ("__cobrust_hood_command_drop", hood_drop_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0076 Phase 1: dora ecosystem-module C-ABI binding ---------
        // `dora` (dora-rs robotics dataflow, ninth ecosystem module).
        // Third module exercising the ADR-0073 cross-boundary callback
        // chain (after pit + hood). Phase 1 ships SYNTHETIC runtime:
        // `dora.node(handler)` installs the callback in a process-global
        // slot and `node.run()` mocks one canned ("camera", "frame_001")
        // Event arrival, mirroring F65's synthetic-LLM provider pattern.
        // The callback shape is `fn(dora.Event) -> i64` (Event arg
        // matches pit.Request's borrow shape; i64 return matches hood's
        // exit-code intent). At the wire level the callback uses the
        // SAME C-ABI as pit + hood (`unsafe extern "C" fn(*mut u8) ->
        // *mut u8`) per ADR-0073 §5.1.
        //
        //   __cobrust_dora_node_new(name: *mut Str) -> *mut Node
        //   __cobrust_dora_node_node(
        //       handler: *const c_void
        //   ) -> i64 = 0   (Ty::Int sentinel — registration is a
        //                   side-effect on the global handler slot)
        //   __cobrust_dora_node_run(node: *mut Node) -> i64
        //   __cobrust_dora_node_shutdown(node: *mut Node) -> i64
        //   __cobrust_dora_event_id(event: *mut Event) -> *mut Str
        //   __cobrust_dora_event_data_str(event: *mut Event) -> *mut Str
        //   __cobrust_dora_node_drop(node: *mut Node) -> void
        //   __cobrust_dora_event_drop(event: *mut Event) -> void
        //
        // ADR-0076 Phase 2 — multi-IO declaration + send_output shims:
        //   __cobrust_dora_declare_input(id: *mut Str) -> i64
        //   __cobrust_dora_declare_output(id: *mut Str) -> i64
        //   __cobrust_dora_event_send_output(
        //       event: *mut Event, output_id: *mut Str, payload: *mut Str
        //   ) -> i64   (0 = emitted; -1 = undeclared output id)
        // The two declare shims push the decorator-threaded port ids into
        // process-global slots; `node.run()` then fires the handler once per
        // declared input (falling back to the single canned event when none
        // declared). `event_send_output` validates against the declared
        // outputs + captures the payload for the synthetic-E2E stdout assert.
        let dora_node_new_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let dora_node_node_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let dora_node_run_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let dora_node_shutdown_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let dora_event_id_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let dora_event_data_str_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let dora_declare_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let dora_event_send_output_ty =
            i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        let dora_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_dora_node_new", dora_node_new_ty, 1usize),
            ("__cobrust_dora_node_node", dora_node_node_ty, 1),
            ("__cobrust_dora_node_run", dora_node_run_ty, 1),
            ("__cobrust_dora_node_shutdown", dora_node_shutdown_ty, 1),
            ("__cobrust_dora_event_id", dora_event_id_ty, 1),
            ("__cobrust_dora_event_data_str", dora_event_data_str_ty, 1),
            ("__cobrust_dora_declare_input", dora_declare_ty, 1),
            ("__cobrust_dora_declare_output", dora_declare_ty, 1),
            (
                "__cobrust_dora_event_send_output",
                dora_event_send_output_ty,
                3,
            ),
            ("__cobrust_dora_node_drop", dora_drop_ty, 1),
            ("__cobrust_dora_event_drop", dora_drop_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0078 backend Phase 2: fang ecosystem-module C-ABI -----------
        // `fang` (auth/security, rebrand-of-`argon2`) — pure value pattern
        // (no handles, no drops), the TENTH ecosystem module. Exported by
        // `cobrust-fang/src/cabi.rs`, linked as `libfang.a` only when the
        // program imports `fang`.
        //
        //   __cobrust_fang_hash_password(pw: *mut Str) -> *mut Str
        //   __cobrust_fang_verify_password(pw, hash: *mut Str) -> bool (i1)
        //
        // `verify_password` is the FIRST `-> bool` value-fn return on the
        // chain: the LLVM extern declares an `i1` return that lands in the
        // `_ecoret` bool local (`write_place` -> `coerce_value_to` bridges
        // any i1/i8 width gap into the alloca). The `hash_password`-returned
        // Str buffer is freed by the existing Str drop schedule — no new
        // drop wiring needed.
        let bool_ty = self.ctx.bool_type();
        let fang_hash_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let fang_verify_ty = bool_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        // The JWT surface (HS256-signed JSON Web Tokens) — twins of the
        // hash/verify shape:
        //   __cobrust_fang_jwt_encode(claims_json, secret: *mut Str) -> *mut Str
        //   __cobrust_fang_jwt_verify(token, secret: *mut Str) -> bool (i1)
        //   __cobrust_fang_jwt_decode(token, secret: *mut Str) -> *mut Str
        // encode/decode return a freshly-allocated Str buffer freed by the
        // existing Str drop schedule; verify returns an `i1` that lands in
        // the `_ecoret` bool local exactly like `verify_password`.
        let fang_jwt_strstr_str_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let fang_jwt_verify_ty = bool_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_fang_hash_password", fang_hash_ty, 1usize),
            ("__cobrust_fang_verify_password", fang_verify_ty, 2),
            ("__cobrust_fang_jwt_encode", fang_jwt_strstr_str_ty, 2),
            ("__cobrust_fang_jwt_verify", fang_jwt_verify_ty, 2),
            ("__cobrust_fang_jwt_decode", fang_jwt_strstr_str_ty, 2),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0078 Phase-1c: redis ecosystem-module C-ABI binding ---------
        // `redis` (cache/KV, the redis-py rebrand) — pairs the handle
        // pattern (Client, a den.Connection-shaped stateful resource) with
        // a free-function entrypoint (`connect`, like den's `connect`).
        // Exported by `cobrust-redis/src/cabi.rs`, linked as `libredis.a`
        // only when the program imports `redis`. The sync path means NO
        // async runtime is pulled (ADR-0078 §3.5).
        //
        //   __cobrust_redis_connect(url: *mut Str) -> *mut Client
        //   __cobrust_redis_client_set(c, key, value: *mut Str) -> void
        //   __cobrust_redis_client_get(c, key: *mut Str) -> *mut Str
        //   __cobrust_redis_client_delete(c, key: *mut Str) -> i64
        //   __cobrust_redis_client_exists(c, key: *mut Str) -> bool (i1)
        //   __cobrust_redis_client_drop(c) -> void
        // Phase-B cache/counter/hash verbs (same handle, same shapes):
        //   __cobrust_redis_client_expire(c, key: *mut Str, secs: i64) -> bool (i1)
        //   __cobrust_redis_client_incr(c, key: *mut Str) -> i64
        //   __cobrust_redis_client_incr_by(c, key: *mut Str, delta: i64) -> i64
        //   __cobrust_redis_client_hset(c, key, field, value: *mut Str) -> bool (i1)
        //   __cobrust_redis_client_hget(c, key, field: *mut Str) -> *mut Str
        // Phase-C list/set verbs (same handle, all scalar/str returns):
        //   __cobrust_redis_client_lpush(c, key, value: *mut Str) -> i64
        //   __cobrust_redis_client_rpush(c, key, value: *mut Str) -> i64
        //   __cobrust_redis_client_lpop(c, key: *mut Str) -> *mut Str
        //   __cobrust_redis_client_rpop(c, key: *mut Str) -> *mut Str
        //   __cobrust_redis_client_llen(c, key: *mut Str) -> i64
        //   __cobrust_redis_client_sadd(c, key, member: *mut Str) -> i64
        //   __cobrust_redis_client_srem(c, key, member: *mut Str) -> i64
        //   __cobrust_redis_client_sismember(c, key, member: *mut Str) -> bool (i1)
        //   __cobrust_redis_client_scard(c, key: *mut Str) -> i64
        // Phase-1d LIST-of-str-return verbs (same handle, all return an
        // owned `*mut List<i64>` the `.cb` scope drops via the
        // Ty::List(Str) schedule — the SAME ptr-return shape coil's
        // `__cobrust_coil_buffer_shape -> *mut List<i64>` + the stdlib's
        // `__cobrust_llm_stream -> list[str]` already use; NO new codegen
        // fn-type design — a Ty::List return maps to an LLVM ptr return):
        //   __cobrust_redis_client_lrange(c, key, start, stop: i64) -> *mut List
        //   __cobrust_redis_client_smembers(c, key: *mut Str) -> *mut List
        //   __cobrust_redis_client_hkeys(c, key: *mut Str) -> *mut List
        //   __cobrust_redis_client_hgetall(c, key: *mut Str) -> *mut List
        //     (FLAT [k, v, k, v, ...] — the documented dict-vs-flat-list
        //      divergence, mirroring coil.shape's list-vs-tuple note.)
        //
        // The `_drop` symbol is emitted by `emit_drop_for_ty` at the
        // handle local's scope exit via `handle_drop_symbol(id)` (chain is
        // already general — no new drop wiring needed). `set` returns
        // void (side-effect — the `.cb` `Ty::None` return); `exists` /
        // `expire` / `hset` return an `i1` bool (the fang `verify_password`
        // precedent — `write_place` bridges the i1/alloca width gap);
        // `expire` / `incr_by` carry a trailing `i64` scalar arg.
        let redis_connect_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let redis_set_ty = void_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        let redis_get_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let redis_delete_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let redis_exists_ty = bool_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let redis_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        // Phase-B fn types. `expire`: (ptr, ptr, i64) -> i1; `incr`:
        // (ptr, ptr) -> i64; `incr_by`: (ptr, ptr, i64) -> i64; `hset`:
        // (ptr, ptr, ptr, ptr) -> i1; `hget`: (ptr, ptr, ptr) -> ptr.
        let redis_expire_ty =
            bool_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), i64_ty.into()], false);
        let redis_incr_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let redis_incr_by_ty =
            i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), i64_ty.into()], false);
        let redis_hset_ty = bool_ty.fn_type(
            &[ptr_ty.into(), ptr_ty.into(), ptr_ty.into(), ptr_ty.into()],
            false,
        );
        let redis_hget_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        // Phase-C fn types. `lpush`/`rpush`/`sadd`/`srem`: (ptr, ptr, ptr)
        // -> i64 (the 3-ptr key+value/member shape returning a count);
        // `sismember`: (ptr, ptr, ptr) -> i1. `lpop`/`rpop` reuse
        // `redis_get_ty` ((ptr, ptr) -> ptr); `llen`/`scard` reuse
        // `redis_delete_ty` ((ptr, ptr) -> i64).
        let redis_push_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        let redis_sismember_ty =
            bool_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        // Phase-1d fn types. The single-key LIST-of-str returns
        // (`smembers`/`hkeys`/`hgetall`) reuse `redis_get_ty`
        // ((ptr, ptr) -> ptr — the List return is a ptr, exactly like
        // `get`'s Str return is a ptr); only `lrange` needs the NEW
        // (ptr, ptr, i64, i64) -> ptr shape (receiver + key + start + stop
        // -> the owned List ptr). The `Ty::List(Str)` return + its drop
        // schedule are codegen-generic (no list-specific return wiring).
        let redis_lrange_ty = ptr_ty.fn_type(
            &[ptr_ty.into(), ptr_ty.into(), i64_ty.into(), i64_ty.into()],
            false,
        );
        for (sym, ty, params) in [
            ("__cobrust_redis_connect", redis_connect_ty, 1usize),
            ("__cobrust_redis_client_set", redis_set_ty, 3),
            ("__cobrust_redis_client_get", redis_get_ty, 2),
            ("__cobrust_redis_client_delete", redis_delete_ty, 2),
            ("__cobrust_redis_client_exists", redis_exists_ty, 2),
            ("__cobrust_redis_client_drop", redis_drop_ty, 1),
            ("__cobrust_redis_client_expire", redis_expire_ty, 3),
            ("__cobrust_redis_client_incr", redis_incr_ty, 2),
            ("__cobrust_redis_client_incr_by", redis_incr_by_ty, 3),
            ("__cobrust_redis_client_hset", redis_hset_ty, 4),
            ("__cobrust_redis_client_hget", redis_hget_ty, 3),
            // Phase-C list verbs.
            ("__cobrust_redis_client_lpush", redis_push_ty, 3),
            ("__cobrust_redis_client_rpush", redis_push_ty, 3),
            ("__cobrust_redis_client_lpop", redis_get_ty, 2),
            ("__cobrust_redis_client_rpop", redis_get_ty, 2),
            ("__cobrust_redis_client_llen", redis_delete_ty, 2),
            // Phase-C set verbs.
            ("__cobrust_redis_client_sadd", redis_push_ty, 3),
            ("__cobrust_redis_client_srem", redis_push_ty, 3),
            ("__cobrust_redis_client_sismember", redis_sismember_ty, 3),
            ("__cobrust_redis_client_scard", redis_delete_ty, 2),
            // Phase-1d LIST-of-str-return verbs (the List return is a ptr;
            // smembers/hkeys/hgetall reuse `redis_get_ty`'s (ptr,ptr)->ptr,
            // only lrange takes the (ptr,ptr,i64,i64)->ptr `redis_lrange_ty`).
            ("__cobrust_redis_client_lrange", redis_lrange_ty, 4),
            ("__cobrust_redis_client_smembers", redis_get_ty, 2),
            ("__cobrust_redis_client_hkeys", redis_get_ty, 2),
            ("__cobrust_redis_client_hgetall", redis_get_ty, 2),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0072 8/8 first proof: coil ecosystem-module C-ABI binding ----
        // `coil` (numpy ndarray foundation, ecosystem rebrand of Python's
        // `numpy` library). EIGHTH and final cobra-batch module — completes
        // the workspace-vendored ecosystem. Pure value-handle pattern (no
        // callbacks); chain generality matches den/molt/strike's
        // value-handle precedent. Operator dispatch (`a + b`) + index
        // dispatch (`a[i]`) are explicitly deferred to a sub-ADR per
        // ADR-0072 §"coil deep operator/index" — first proof scope is
        // constructors + repr only.
        //
        //   __cobrust_coil_zeros(n: i64) -> *mut Buffer
        //   __cobrust_coil_ones(n: i64) -> *mut Buffer
        //   __cobrust_coil_eye(n: i64) -> *mut Buffer
        //   __cobrust_coil_print_buffer(b: *mut Buffer) -> i64
        //   __cobrust_coil_buffer_drop(b: *mut Buffer) -> void
        let coil_ctor_ty = ptr_ty.fn_type(&[i64_ty.into()], false);
        let coil_print_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let coil_drop_ty = void_ty.fn_type(&[ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_coil_zeros", coil_ctor_ty, 1usize),
            ("__cobrust_coil_ones", coil_ctor_ty, 1),
            ("__cobrust_coil_eye", coil_ctor_ty, 1),
            // #numpy BATCH 20 — `coil.arange(n) -> Buffer`. The FINAL core
            // numpy constructor (LLMs write `np.arange(n)` constantly). The
            // SAME `(i64) -> ptr` extern shape as `zeros`/`ones`/`eye`
            // (`coil_ctor_ty`, REUSED — no new fn-type); the result is an
            // `Int64` buffer at runtime. The MIR ecosystem-call lowering
            // retargets `coil.arange(n)` onto this Call via the generic
            // `[Int] -> Buffer` path (ZERO batch-specific MIR, like zeros).
            ("__cobrust_coil_arange", coil_ctor_ty, 1),
            ("__cobrust_coil_print_buffer", coil_print_ty, 1),
            ("__cobrust_coil_buffer_drop", coil_drop_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- Stream W P0 增量 (2026-05-29): 8 free functions extending
        // the coil surface toward "basic scientific computing"
        // coverage. Same value-handle pattern as the first proof.
        //
        //   __cobrust_coil_mgrid(start: i64, stop: i64) -> *mut Buffer
        //   __cobrust_coil_ogrid(start: i64, stop: i64) -> *mut Buffer
        //   __cobrust_coil_broadcast_to(a: *mut Buffer, n: i64) -> *mut Buffer
        //   __cobrust_coil_split(a: *mut Buffer, n: i64) -> *mut Buffer
        //   __cobrust_coil_mean / median / std / var (a: *mut Buffer) -> f64
        //   __cobrust_coil_min / max / prod (a: *mut Buffer) -> f64 (BATCH 7)
        let f64_ty = self.ctx.f64_type();
        let coil_grid_ty = ptr_ty.fn_type(&[i64_ty.into(), i64_ty.into()], false);
        let coil_bcast_ty = ptr_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        // #163 BATCH 18 — `__cobrust_coil_reshape(a, rows, cols)`: a
        // `(ptr, i64, i64) -> ptr` shim (the `coil_bcast_ty` Buffer+Int shape
        // + one more Int). NEW fn-type because no prior coil free-function
        // declares a 2-int-arg + Buffer shape in THIS block (the operator
        // block's `coil_slice_ty` is the same shape but a separate `let`; a
        // dedicated name keeps this batch self-contained).
        let coil_reshape_ty = ptr_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
        let coil_agg_ty = f64_ty.fn_type(&[ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_coil_mgrid", coil_grid_ty, 2usize),
            ("__cobrust_coil_ogrid", coil_grid_ty, 2),
            ("__cobrust_coil_broadcast_to", coil_bcast_ty, 2),
            // #163 BATCH 18 — `coil.reshape(a, rows, cols) -> Buffer`. Three
            // params (Buffer, Int, Int) on the new `coil_reshape_ty`; the MIR
            // ecosystem-call lowering retargets `coil.reshape(a, r, c)` onto
            // this Call (generic 3-param path, ZERO batch-specific MIR).
            ("__cobrust_coil_reshape", coil_reshape_ty, 3),
            ("__cobrust_coil_split", coil_bcast_ty, 2),
            ("__cobrust_coil_mean", coil_agg_ty, 1),
            ("__cobrust_coil_median", coil_agg_ty, 1),
            ("__cobrust_coil_std", coil_agg_ty, 1),
            ("__cobrust_coil_var", coil_agg_ty, 1),
            // #145 BATCH 7 — the VALUE reductions `min`/`max`/`prod`. Each
            // is `(ptr) -> f64`, the SAME `coil_agg_ty` shape as `mean`
            // (coil's scalar-reduction convention; every `.cb` Buffer is
            // Float64 so `min`/`max`/`prod -> f64` is numpy-exact). NO new
            // extern type. `min`/`max` `coil_panic` on empty (numpy
            // ValueError); `prod([]) == 1.0`; NaN propagates.
            ("__cobrust_coil_min", coil_agg_ty, 1),
            ("__cobrust_coil_max", coil_agg_ty, 1),
            ("__cobrust_coil_prod", coil_agg_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- #145 statistics gap-closure (2026-06-01): NaN-aware + spread
        // scalar aggregates. `ptp` / `nansum` / `nanmean` / `nanstd` reuse
        // the `coil_agg_ty` (Buffer → f64) shape above; `percentile` takes
        // a trailing f64 quantile (Buffer + f64 → f64, `coil_agg2_ty`):
        //
        //   __cobrust_coil_ptp / nansum / nanmean / nanstd (a: *mut Buffer) -> f64
        //   __cobrust_coil_percentile(a: *mut Buffer, q: f64) -> f64
        let coil_agg2_ty = f64_ty.fn_type(&[ptr_ty.into(), f64_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_coil_ptp", coil_agg_ty, 1usize),
            ("__cobrust_coil_nansum", coil_agg_ty, 1),
            ("__cobrust_coil_nanmean", coil_agg_ty, 1),
            ("__cobrust_coil_nanstd", coil_agg_ty, 1),
            ("__cobrust_coil_percentile", coil_agg2_ty, 2),
            // #163 BATCH 17 — the SCALAR-return linalg reductions `trace` /
            // `norm` ride the SAME `coil_agg_ty` (`(ptr) -> f64`) shape as
            // `mean` / `ptp` (NO new fn-type). `trace` `coil_panic`s on a
            // non-2-D input; `norm` is total — both invisible to codegen
            // (the f64-return ABI is byte-identical). MIR retargets
            // `coil.trace(a)` / `coil.norm(a)` onto these `Call`s via the
            // SAME Buffer→f64 path as `mean` (ZERO batch-specific MIR code).
            ("__cobrust_coil_trace", coil_agg_ty, 1),
            ("__cobrust_coil_norm", coil_agg_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0077 Phase 1: coil Buffer operator / index / attribute
        // C-ABI binding — the FIRST ecosystem-handle operator surface.
        // Because the MIR retarget turns `a + b` / `a[i]` / `a.shape` into
        // `Terminator::Call`s, codegen only needs the extern decls (no
        // `lower_binop` type-switch — ADR-0077 §1.1). Symbols match the
        // `__cobrust_coil_` prefix recognizer in build/intrinsics.rs, so
        // they link from libcoil.a alongside the existing coil surface.
        //
        //   __cobrust_coil_buffer_add/sub/mul(a, b: *mut Buffer) -> *mut Buffer
        //   __cobrust_coil_buffer_getitem(a: *mut Buffer, i: i64) -> f64
        //   __cobrust_coil_buffer_shape(a: *mut Buffer) -> *mut List<i64>
        //   __cobrust_coil_buffer_ndim/size(a: *mut Buffer) -> i64
        let coil_binop_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let coil_getitem_ty = f64_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        let coil_shape_ty = ptr_ty.fn_type(&[ptr_ty.into()], false);
        let coil_attr_i64_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        // -- ADR-0077 Phase 2a: a.dot(b) / a[i]=v / a[lo:hi]. Same MIR-
        // retarget-to-Call discipline (codegen only declares the externs):
        //   __cobrust_coil_buffer_dot(a, b: *mut Buffer) -> f64  (1-D dot)
        //   __cobrust_coil_buffer_setitem(a: *mut Buffer, i: i64, v: f64) -> void
        //   __cobrust_coil_buffer_slice(a: *mut Buffer, lo, hi: i64) -> *mut Buffer
        let coil_dot_ty = f64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let coil_setitem_ty =
            void_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), f64_ty.into()], false);
        let coil_slice_ty = ptr_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
        // -- ADR-0077 Phase-1 completion: `a / b` (true-division) reuses the
        // (ptr, ptr) -> ptr `coil_binop_ty`; the scalar forms `a ⊕ k` take
        // `(ptr, f64) -> ptr` (the python scalar `k` is passed as f64 by the
        // MIR retarget). The MIR-retarget-to-Call discipline holds (codegen
        // only declares the externs):
        //   __cobrust_coil_buffer_div(a, b: *mut Buffer)        -> *mut Buffer
        //   __cobrust_coil_buffer_{add,sub,mul,div}_scalar(a: *mut Buffer,
        //                                                   k: f64) -> *mut Buffer
        let coil_scalar_binop_ty = ptr_ty.fn_type(&[ptr_ty.into(), f64_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_coil_buffer_add", coil_binop_ty, 2usize),
            ("__cobrust_coil_buffer_sub", coil_binop_ty, 2),
            ("__cobrust_coil_buffer_mul", coil_binop_ty, 2),
            ("__cobrust_coil_buffer_div", coil_binop_ty, 2),
            ("__cobrust_coil_buffer_add_scalar", coil_scalar_binop_ty, 2),
            ("__cobrust_coil_buffer_sub_scalar", coil_scalar_binop_ty, 2),
            ("__cobrust_coil_buffer_mul_scalar", coil_scalar_binop_ty, 2),
            ("__cobrust_coil_buffer_div_scalar", coil_scalar_binop_ty, 2),
            // ADR-0077 Phase-2/3 left-scalar `k ⊕ a`: `+`/`*` reuse the
            // commutative `*_scalar` rows above; `-`/`/` need REVERSED
            // shims (`k - a[i]` / `k / a[i]`). Same `(ptr, f64) -> ptr`
            // shape as the right-scalar shims (only operand order flips
            // inside the shim), so they reuse `coil_scalar_binop_ty`.
            ("__cobrust_coil_buffer_rsub_scalar", coil_scalar_binop_ty, 2),
            ("__cobrust_coil_buffer_rdiv_scalar", coil_scalar_binop_ty, 2),
            // ADR-0077 Phase-2/3 buffer-buffer COMPARISON `a cmp b` → a
            // Bool-dtype Buffer. Same `(ptr, ptr) -> ptr` array-array
            // shape as `add`/`sub`/`mul`/`div`, so they reuse
            // `coil_binop_ty`. The MIR retarget (extended
            // `lookup_buffer_binop`) turns `a < b` into a `Terminator::
            // Call` onto these — codegen only declares the externs.
            ("__cobrust_coil_buffer_lt", coil_binop_ty, 2),
            ("__cobrust_coil_buffer_le", coil_binop_ty, 2),
            ("__cobrust_coil_buffer_gt", coil_binop_ty, 2),
            ("__cobrust_coil_buffer_ge", coil_binop_ty, 2),
            ("__cobrust_coil_buffer_eq", coil_binop_ty, 2),
            ("__cobrust_coil_buffer_ne", coil_binop_ty, 2),
            // ADR-0077 §"@-operator" buffer-buffer MATRIX multiply `a @ b`
            // → a `coil.Buffer`. Same `(ptr, ptr) -> ptr` array-array shape
            // as `add`/`sub`/`mul`/`div` + the comparisons, so it reuses
            // `coil_binop_ty`. The MIR retarget (the `lookup_buffer_binop`
            // MatMul arm) turns `a @ b` into a `Terminator::Call` onto this
            // — codegen only declares the extern (no matmul-specific arm).
            ("__cobrust_coil_buffer_matmul", coil_binop_ty, 2),
            ("__cobrust_coil_buffer_getitem", coil_getitem_ty, 2),
            ("__cobrust_coil_buffer_shape", coil_shape_ty, 1),
            ("__cobrust_coil_buffer_ndim", coil_attr_i64_ty, 1),
            ("__cobrust_coil_buffer_size", coil_attr_i64_ty, 1),
            ("__cobrust_coil_buffer_dot", coil_dot_ty, 2),
            ("__cobrust_coil_buffer_setitem", coil_setitem_ty, 3),
            ("__cobrust_coil_buffer_slice", coil_slice_ty, 3),
            // BATCH 19 — `coil.astype(a, dtype) -> Buffer`. The dtype is a
            // `Ty::Str`, which (like dora `event.send_output(output_id: Str,
            // payload: Str)` — `dora_event_send_output_ty` above declares its
            // Str params as `ptr_ty`) crosses the C-ABI as a `*mut Str`
            // buffer POINTER. So at the LLVM ABI level a Buffer-ptr AND a
            // Str-ptr are BOTH `ptr_ty`, and the shape is IDENTICAL to the
            // `(ptr, ptr) -> ptr` `coil_binop_ty` (NO new fn-type). The MIR
            // ecosystem-call lowering retargets `coil.astype(a, "int64")`
            // onto this `Call`; codegen's `lower_call` materialises the Str
            // literal as a stdlib Str buffer + passes its pointer (the
            // send_output path — `lower_call` `else` branch, NEITHER str
            // expansion fires for a 2-arg/2-param Buffer+trailing-Str shape).
            ("__cobrust_coil_astype", coil_binop_ty, 2),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0079 Phase 1: coil.linalg.* sub-namespace + the minimal
        // 2-D / explicit-data constructors that exercise it on NON-identity
        // matrices. The MIR retarget turns `coil.linalg.solve(a, b)` /
        // `coil.array2x2(...)` into `Terminator::Call`s onto these flat
        // `__cobrust_coil_linalg_*` / `__cobrust_coil_array*` symbols
        // (a new prefix sibling already covered by the `__cobrust_coil_`
        // build/intrinsics recognizer), so codegen only declares the
        // externs (no math here — the kernels are `coil::linalg::{solve,
        // det, inv}`, wrapped in cabi.rs). Symbol shapes:
        //   __cobrust_coil_linalg_solve(a, b: *mut Buffer) -> *mut Buffer
        //   __cobrust_coil_linalg_inv(a: *mut Buffer)      -> *mut Buffer
        //   __cobrust_coil_linalg_det(a: *mut Buffer)      -> f64  (0-d→f64)
        //   __cobrust_coil_array2x2(a,b,c,d: f64)          -> *mut Buffer
        //   __cobrust_coil_array2x3(a..f: f64)             -> *mut Buffer
        //   __cobrust_coil_array1d2(a,b: f64)              -> *mut Buffer
        let coil_array2x2_ty = ptr_ty.fn_type(
            &[f64_ty.into(), f64_ty.into(), f64_ty.into(), f64_ty.into()],
            false,
        );
        let coil_array2x3_ty = ptr_ty.fn_type(
            &[
                f64_ty.into(),
                f64_ty.into(),
                f64_ty.into(),
                f64_ty.into(),
                f64_ty.into(),
                f64_ty.into(),
            ],
            false,
        );
        let coil_array1d2_ty = ptr_ty.fn_type(&[f64_ty.into(), f64_ty.into()], false);
        // #145 BATCH 11 — spacing/value CONSTRUCTORS. All-scalar-arg Buffer
        // producers (NO Buffer input). `linspace`/`logspace` are
        // `(f64, f64, i64) -> ptr` (start, stop, num); `full` is
        // `(i64, f64) -> ptr` (n, value). The MIXED-scalar-arg shape rides
        // the SAME generic ecosystem-call lowering — codegen only declares
        // the externs (no batch-specific arm; the flat `__cobrust_coil_*`
        // recognizer prefix). `array2x2`'s 4×f64→ptr proves f64-scalar→ptr;
        // `roll`'s (ptr, i64)→ptr proves the i64-scalar coercion.
        let coil_linspace_ty =
            ptr_ty.fn_type(&[f64_ty.into(), f64_ty.into(), i64_ty.into()], false);
        let coil_full_ty = ptr_ty.fn_type(&[i64_ty.into(), f64_ty.into()], false);
        for (sym, ty, params) in [
            // solve: (ptr, ptr) -> ptr ≡ coil_binop_ty;
            // inv:   (ptr) -> ptr      ≡ coil_shape_ty;
            // det:   (ptr) -> f64      ≡ coil_agg_ty.
            ("__cobrust_coil_linalg_solve", coil_binop_ty, 2usize),
            ("__cobrust_coil_linalg_inv", coil_shape_ty, 1),
            ("__cobrust_coil_linalg_det", coil_agg_ty, 1),
            ("__cobrust_coil_array2x2", coil_array2x2_ty, 4),
            ("__cobrust_coil_array2x3", coil_array2x3_ty, 6),
            ("__cobrust_coil_array1d2", coil_array1d2_ty, 2),
            // #145 BATCH 11 spacing/value ctors: (f64, f64, i64) -> ptr +
            // (i64, f64) -> ptr.
            ("__cobrust_coil_linspace", coil_linspace_ty, 3),
            ("__cobrust_coil_logspace", coil_linspace_ty, 3),
            ("__cobrust_coil_full", coil_full_ty, 2),
            // #145 array-MANIPULATION Buffer-returning ops. The 1-arg
            // reshape ops (`transpose`/`flatten`/`ravel`) are `(ptr) -> ptr`
            // ≡ `coil_shape_ty`; the 2-array combine ops (`concatenate`/
            // `vstack`/`hstack`) are `(ptr, ptr) -> ptr` ≡ `coil_binop_ty`.
            // The MIR ecosystem-call lowering retargets `coil.transpose(a)` /
            // `coil.concatenate(a, b)` onto these `Terminator::Call`s —
            // codegen only declares the externs (no manipulation-specific
            // arm; same flat `__cobrust_coil_*` recognizer prefix).
            ("__cobrust_coil_transpose", coil_shape_ty, 1),
            ("__cobrust_coil_flatten", coil_shape_ty, 1),
            ("__cobrust_coil_ravel", coil_shape_ty, 1),
            ("__cobrust_coil_concatenate", coil_binop_ty, 2),
            ("__cobrust_coil_vstack", coil_binop_ty, 2),
            ("__cobrust_coil_hstack", coil_binop_ty, 2),
            // #163 BATCH 17 — the MATRIX-return linalg op `outer` is `(ptr,
            // ptr) -> ptr` ≡ `coil_binop_ty`, the IDENTICAL extern shape as
            // the combine ops above (NO new fn-type). The `(n, m)` outer
            // product + the dtype-preserving equal-dtype contract live in the
            // Rust kernel (`manipulate.rs`); the handle ABI is byte-identical,
            // so codegen rides the SAME extern + the flat `__cobrust_coil_*`
            // recognizer prefix. MIR retargets `coil.outer(a, b)` onto this
            // `Call` via the SAME generic 2-Buffer-arg path (ZERO new MIR).
            ("__cobrust_coil_outer", coil_binop_ty, 2),
            // #163 elementwise BINARY min/max ufuncs (BATCH 13). The 2-array
            // `maximum`/`minimum`/`fmax`/`fmin` ops are `(ptr, ptr) -> ptr` ≡
            // `coil_binop_ty` — the IDENTICAL extern shape as the 2-array
            // combine ops `concatenate`/`vstack`/`hstack` above (and
            // `linalg.solve`). The NaN split (`maximum`/`minimum` PROPAGATE
            // NaN; `fmax`/`fmin` IGNORE NaN) + the same-shape / same-dtype
            // combine contract live entirely in the Rust kernel
            // (`elementwise.rs`); the handle ABI is byte-identical, so codegen
            // rides the SAME extern shape + the flat `__cobrust_coil_*`
            // recognizer prefix (no batch-specific arm). MIR retargets
            // `coil.maximum(a, b)` onto these `Call`s via the SAME generic
            // 2-Buffer-arg path (ZERO batch-specific MIR code).
            ("__cobrust_coil_maximum", coil_binop_ty, 2),
            ("__cobrust_coil_minimum", coil_binop_ty, 2),
            ("__cobrust_coil_fmax", coil_binop_ty, 2),
            ("__cobrust_coil_fmin", coil_binop_ty, 2),
            // #145 2-Buffer FLOAT ufuncs (BATCH 15). `arctan2`/`hypot`/
            // `logaddexp` are `(ptr, ptr) -> ptr` ≡ `coil_binop_ty` — the
            // IDENTICAL extern shape as the BATCH-13 min/max family + the
            // combine ops above (NO new fn-type). UNLIKE min/max these are
            // FLOAT-PROMOTING (int->f64, f32->f32) + the per-op float math
            // (`arctan2` arg order `(y, x)` Y FIRST; `hypot` OVERFLOW-SAFE;
            // `logaddexp` NUMERICALLY STABLE) — all inside the Rust kernel
            // (`elementwise.rs`). The handle ABI is byte-identical, so codegen
            // rides the SAME extern shape + the flat `__cobrust_coil_*`
            // recognizer prefix (no batch-specific arm). MIR retargets
            // `coil.arctan2(y, x)` onto these `Call`s via the SAME generic
            // 2-Buffer-arg path (ZERO batch-specific MIR code).
            ("__cobrust_coil_arctan2", coil_binop_ty, 2),
            ("__cobrust_coil_hypot", coil_binop_ty, 2),
            ("__cobrust_coil_logaddexp", coil_binop_ty, 2),
            // #145 unary TRANSCENDENTAL Buffer-returning ops. All 1-arg
            // FLOAT-returning ufuncs (`exp`/`log`/`log10`/`sqrt`/`sin`/
            // `cos`/`tan` + optional `exp2`/`log2`/`cbrt`/`sinh`/`cosh`/
            // `tanh`) are `(ptr) -> ptr` ≡ `coil_shape_ty` — the IDENTICAL
            // extern shape as the BATCH-2 reshape ops `transpose`/`flatten`/
            // `ravel` above. The MIR ecosystem-call lowering retargets
            // `coil.exp(a)` onto these `Terminator::Call`s; codegen only
            // declares the externs (no transcendental-specific arm; same
            // flat `__cobrust_coil_*` recognizer prefix).
            ("__cobrust_coil_exp", coil_shape_ty, 1),
            ("__cobrust_coil_log", coil_shape_ty, 1),
            ("__cobrust_coil_log10", coil_shape_ty, 1),
            ("__cobrust_coil_sqrt", coil_shape_ty, 1),
            ("__cobrust_coil_sin", coil_shape_ty, 1),
            ("__cobrust_coil_cos", coil_shape_ty, 1),
            ("__cobrust_coil_tan", coil_shape_ty, 1),
            ("__cobrust_coil_exp2", coil_shape_ty, 1),
            ("__cobrust_coil_log2", coil_shape_ty, 1),
            ("__cobrust_coil_cbrt", coil_shape_ty, 1),
            ("__cobrust_coil_sinh", coil_shape_ty, 1),
            ("__cobrust_coil_cosh", coil_shape_ty, 1),
            ("__cobrust_coil_tanh", coil_shape_ty, 1),
            // #145 unary INVERSE trig / hyperbolic Buffer-returning ops
            // (BATCH 16) — `arcsin`/`arccos`/`arctan`/`arcsinh`/`arccosh`/
            // `arctanh`, COMPLETING the unary transcendental family. All
            // 1-arg FLOAT-returning ufuncs `(ptr) -> ptr` ≡ `coil_shape_ty`,
            // the IDENTICAL extern shape as the BATCH-3 forward
            // transcendentals above. The MIR ecosystem-call lowering
            // retargets `coil.arcsin(a)` onto these `Terminator::Call`s;
            // codegen only declares the externs (no inverse-trig-specific
            // arm; same flat `__cobrust_coil_*` recognizer prefix).
            ("__cobrust_coil_arcsin", coil_shape_ty, 1),
            ("__cobrust_coil_arccos", coil_shape_ty, 1),
            ("__cobrust_coil_arctan", coil_shape_ty, 1),
            ("__cobrust_coil_arcsinh", coil_shape_ty, 1),
            ("__cobrust_coil_arccosh", coil_shape_ty, 1),
            ("__cobrust_coil_arctanh", coil_shape_ty, 1),
            // #145 unary ROUNDING / SIGN Buffer-returning ops (BATCH 4). All
            // 1-arg DTYPE-PRESERVING ufuncs (`abs`/`floor`/`ceil`/`round`/
            // `trunc`/`square`/`sign`) are `(ptr) -> ptr` ≡ `coil_shape_ty`
            // — the IDENTICAL extern shape as the BATCH-3 transcendentals +
            // BATCH-2 reshape ops above. The dtype-preserving rule lives
            // entirely in the Rust kernel (`elementwise.rs`); the ABI is
            // byte-identical, so codegen rides the SAME extern shape + the
            // flat `__cobrust_coil_*` recognizer prefix (no batch-specific
            // arm). MIR retargets `coil.abs(a)` onto these `Call`s.
            ("__cobrust_coil_abs", coil_shape_ty, 1),
            ("__cobrust_coil_floor", coil_shape_ty, 1),
            ("__cobrust_coil_ceil", coil_shape_ty, 1),
            ("__cobrust_coil_round", coil_shape_ty, 1),
            ("__cobrust_coil_trunc", coil_shape_ty, 1),
            ("__cobrust_coil_square", coil_shape_ty, 1),
            ("__cobrust_coil_sign", coil_shape_ty, 1),
            // #163 PREDICATE Buffer-returning ops (BATCH 12). The 1-arg
            // predicate ufuncs `isnan` / `isinf` / `isfinite` are `(ptr) ->
            // ptr` ≡ `coil_shape_ty` — the IDENTICAL extern shape as every
            // other unary ufunc above. UNLIKE the rounding ufuncs the
            // result is a BOOL-dtype Buffer (the per-element MASK,
            // REGARDLESS of input dtype — like `a < b`, but unary), but the
            // opaque `Buffer` handle is dtype-agnostic so the ABI is
            // byte-identical; the bool rule lives entirely in the Rust
            // kernel (`elementwise.rs`). MIR retargets `coil.isnan(a)` onto
            // these `Call`s via the SAME generic Buffer-arg path (ZERO
            // batch-specific MIR code).
            ("__cobrust_coil_isnan", coil_shape_ty, 1),
            ("__cobrust_coil_isinf", coil_shape_ty, 1),
            ("__cobrust_coil_isfinite", coil_shape_ty, 1),
            // #145 REDUCTIONS BATCH 5 — the Buffer-RETURNING cumulative
            // scans `cumsum`/`cumprod` (no-axis FLATTEN to 1-D). `(ptr) ->
            // ptr` ≡ `coil_shape_ty`, the IDENTICAL extern shape as the
            // transcendental / rounding ufuncs + the reshape ops above. The
            // scalar-RETURNING siblings of this batch (`argmin`/`argmax` →
            // i64, `any`/`all` → bool) are declared in the dedicated
            // scalar-extern block below (they need NON-`coil_shape_ty`
            // return types — i64 / i1 — so they cannot ride this `(ptr) ->
            // ptr` loop). MIR retargets `coil.cumsum(a)` onto these `Call`s.
            ("__cobrust_coil_cumsum", coil_shape_ty, 1),
            ("__cobrust_coil_cumprod", coil_shape_ty, 1),
            // #145 SEARCH / ORDER BATCH 9 — the FLAT `sort` / `argsort` /
            // `unique` / `flatnonzero` ops. All 1-arg `(ptr) -> ptr` ≡
            // `coil_shape_ty`, the IDENTICAL extern shape as the reshape
            // ops + unary ufuncs above. The return-DTYPE split (`sort` /
            // `unique` preserve dtype; `argsort` / `flatnonzero` produce an
            // Int64 Buffer) is entirely inside the Rust kernel
            // (`manipulate.rs`); the handle ABI is byte-identical, so
            // codegen rides the SAME extern shape + the flat
            // `__cobrust_coil_*` recognizer prefix (no batch-specific arm).
            // MIR retargets `coil.sort(a)` onto these `Call`s.
            ("__cobrust_coil_sort", coil_shape_ty, 1),
            ("__cobrust_coil_argsort", coil_shape_ty, 1),
            ("__cobrust_coil_unique", coil_shape_ty, 1),
            ("__cobrust_coil_flatnonzero", coil_shape_ty, 1),
            // #145 REARRANGE / REPEAT BATCH 10 — the 1-arg `diff` / `flip`
            // ops (over the C-order FLATTENED array). `(ptr) -> ptr` ≡
            // `coil_shape_ty`, the IDENTICAL extern shape as the reshape
            // ops + unary ufuncs above (DTYPE-PRESERVING, entirely inside
            // the Rust kernel). The i64-SCALAR siblings of this batch
            // (`roll` / `repeat` / `tile`) need a NON-`coil_shape_ty`
            // `(ptr, i64) -> ptr` shape, declared in the dedicated block
            // below. MIR retargets `coil.diff(a)` onto these `Call`s.
            ("__cobrust_coil_diff", coil_shape_ty, 1),
            ("__cobrust_coil_flip", coil_shape_ty, 1),
            // #163 LINALG-EXTRACT BATCH 14 — the 1-arg `diag` / `tril` /
            // `triu` ops. `(ptr) -> ptr` ≡ `coil_shape_ty`, the IDENTICAL
            // extern shape as the reshape ops (`transpose` / `flatten` /
            // `ravel`) + unary ufuncs above. The shape-dependent `diag`
            // (1-D→2-D matrix / 2-D→1-D extract) + the `tril`/`triu`
            // triangle masking + the dtype-preserve rule + the FALLIBLE
            // rank-trap (a disallowed input RANK `coil_panic`s) all live
            // entirely in the Rust kernel (`constructors.rs`) + the shim's
            // `buffer_unary_fallible` body; the opaque `Buffer` handle ABI
            // is byte-identical, so codegen rides the SAME extern shape +
            // the flat `__cobrust_coil_*` recognizer prefix (no batch-
            // specific arm). MIR retargets `coil.diag(a)` onto these
            // `Call`s via the SAME generic 1-Buffer-arg path (ZERO batch-
            // specific MIR code).
            ("__cobrust_coil_diag", coil_shape_ty, 1),
            ("__cobrust_coil_tril", coil_shape_ty, 1),
            ("__cobrust_coil_triu", coil_shape_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- #145 gap-closure BATCH 8 (2026-06-01): `coil.where(cond, a, b)`,
        // the THREE-Buffer elementwise conditional select. The FIRST coil
        // extern with THREE ptr args — a NEW `(ptr, ptr, ptr) -> ptr` shape
        // (`coil_select3_ty`) that EXTENDS the 2-Buffer `coil_binop_ty`
        // (`concatenate` / `vstack` / `hstack` / `linalg.solve`) by one more
        // borrowed handle. The MIR generic ecosystem-call lowering retargets
        // `coil.where(cond, a, b)` onto this `Terminator::Call` (all three
        // Buffer args auto-borrow via `lower_eco_arg`'s Move→Copy upgrade,
        // EXACTLY like the 2-Buffer combine ops — NO batch-specific arm, NO
        // `_=>"any"` gap); codegen only declares the extern (same flat
        // `__cobrust_coil_*` recognizer prefix).
        let coil_select3_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        let where_sym = "__cobrust_coil_where";
        let where_fn =
            self.module
                .add_function(where_sym, coil_select3_ty, Some(Linkage::External));
        self.runtime_helper_decls.insert(where_sym, where_fn);
        self.runtime_helper_param_counts.insert(where_sym, 3usize);

        // -- #145 SCALAR-ARG ufunc gap-closure BATCH 6 (2026-06-01): the
        // Buffer-RETURNING ops taking EXTRA f64 SCALAR args beside the
        // handle. `power(a, p)` is `(ptr, f64) -> ptr` ≡ the `coil_scalar_
        // binop_ty` declared for `a ⊕ k` above (the SAME shape as the
        // scalar-RETURNING `__cobrust_coil_percentile(a, q)` — a Buffer +
        // f64 — except Buffer-returning). `clip(a, lo, hi)` needs a NEW
        // `(ptr, f64, f64) -> ptr` shape (`coil_clip_ty`) — the FIRST coil
        // extern with TWO trailing f64 scalars. The MIR generic ecosystem-
        // call lowering retargets `coil.power(a, p)` / `coil.clip(a, lo, hi)`
        // onto these `Terminator::Call`s (the f64 scalars lower as plain
        // operands via `lower_eco_arg`, like `percentile`'s `q`); codegen
        // only declares the externs (no batch-specific arm; same flat
        // `__cobrust_coil_*` recognizer prefix).
        let coil_clip_ty = ptr_ty.fn_type(&[ptr_ty.into(), f64_ty.into(), f64_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_coil_power", coil_scalar_binop_ty, 2usize),
            ("__cobrust_coil_clip", coil_clip_ty, 3),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- #145 REARRANGE / REPEAT BATCH 10 (2026-06-02): the i64-SCALAR
        // Buffer-RETURNING ops `roll(a, k)` / `repeat(a, n)` / `tile(a, n)`.
        // A NEW `(ptr, i64) -> ptr` shape (`coil_scalar_i64_ty`) — the
        // i64-scalar mirror of the BATCH-6 `(ptr, f64) -> ptr`
        // `coil_scalar_binop_ty` (`power` / `a ⊕ k`): the trailing scalar is
        // an i64 (`shift` / `count`) not an f64. The MIR generic ecosystem-
        // call lowering retargets `coil.roll(a, k)` onto these
        // `Terminator::Call`s — the i64 scalar lowers DIRECTLY (the `EcoSig`
        // param `Ty::Int` lowers the `.cb` int literal as an i64 operand;
        // the extern-call int-width coercion at the `Constant::Str` dispatch
        // forwards it into the i64 param — NO f64 cast, UNLIKE `percentile`'s
        // `q`), so there is NO batch-specific MIR arm; codegen only declares
        // the externs (same flat `__cobrust_coil_*` recognizer prefix).
        let coil_scalar_i64_ty = ptr_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
        for sym in [
            "__cobrust_coil_roll",
            "__cobrust_coil_repeat",
            "__cobrust_coil_tile",
        ] {
            let f = self
                .module
                .add_function(sym, coil_scalar_i64_ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, 2usize);
        }

        // -- #145 REDUCTIONS gap-closure BATCH 5 (2026-06-01): the
        // SCALAR-returning reductions, the NEW extern shapes of this batch.
        // `argmin`/`argmax` return `i64` (the flat C-order index — mirrors
        // `coil.mean`'s scalar return, adapting f64 → i64, i.e. the SAME
        // `(ptr) -> i64` shape as the `coil.Buffer.size`/`.ndim` attribute
        // accessors). `any`/`all` return `bool` — declared as an `i1` LLVM
        // return (the Rust C-ABI `-> bool`), the FIRST coil `-> bool` value
        // fn, mirroring `fang.verify_password`'s `bool_ty.fn_type(...)`; the
        // i1 lands in the `.cb` `_ecoret` Bool local (`write_place` bridges
        // any i1/i8 width gap into the alloca). The MIR generic ecosystem-
        // call lowering drives the return TYPE off the `EcoSig` ret `Ty`
        // (`Ty::Int` / `Ty::Bool`) — NO new MIR arm; codegen only declares
        // the externs.
        //
        //   __cobrust_coil_argmin / argmax (a: *mut Buffer) -> i64
        //   __cobrust_coil_any / all       (a: *mut Buffer) -> bool (i1)
        let coil_arg_i64_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
        let coil_pred_bool_ty = self.ctx.bool_type().fn_type(&[ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_coil_argmin", coil_arg_i64_ty, 1usize),
            ("__cobrust_coil_argmax", coil_arg_i64_ty, 1),
            ("__cobrust_coil_any", coil_pred_bool_ty, 1),
            ("__cobrust_coil_all", coil_pred_bool_ty, 1),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }

        // -- ADR-0084: `import re` (regular expressions) C-ABI binding -------
        // The `regex`-crate-backed stateless subset, exported by
        // `cobrust-stdlib/src/re.rs` (ALWAYS linked — the stdlib staticlib).
        // Three return shapes, all ALREADY PROVEN by string / redis / math
        // (NO new MIR arm — the generic ecosystem-call path drives args +
        // return off the `EcoSig`; codegen only declares the externs):
        //
        //   __cobrust_re_sub(pattern, repl, s: *mut Str) -> *mut Str
        //     — the Str-arg + Str-return shape of `__cobrust_str_replace`
        //       (3 ptr args -> ptr), `redis_hget_ty` ((ptr,ptr,ptr)->ptr).
        //   __cobrust_re_findall(pattern, s: *mut Str) -> *mut List
        //     — the Str-arg + list[str]-return shape of
        //       `__cobrust_redis_client_smembers` (the `Ty::List(Str)` return
        //       is a ptr, exactly like `get`'s Str return; the for-loop /
        //       drop schedule consume it generically), reusing `redis_get_ty`.
        //   __cobrust_re_match / __cobrust_re_search(pattern, s) -> bool (i1)
        //     — the Str-arg + bool-return shape; the FIRST `re` `-> bool`
        //       fns, mirroring `coil.any`/`fang.verify_password`'s
        //       `bool_type().fn_type(...)`. The i1 lands in the `.cb`
        //       `_ecoret` Bool local, usable in `if re.search(...):`.
        //
        // An invalid runtime pattern traps inside the shim (a clean
        // `__cobrust_panic`, non-zero exit) — invisible to codegen.
        let re_sub_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), ptr_ty.into()], false);
        let re_findall_ty = ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let re_bool_ty = self
            .ctx
            .bool_type()
            .fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        for (sym, ty, params) in [
            ("__cobrust_re_sub", re_sub_ty, 3usize),
            ("__cobrust_re_findall", re_findall_ty, 2),
            ("__cobrust_re_match", re_bool_ty, 2),
            ("__cobrust_re_search", re_bool_ty, 2),
        ] {
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
            self.runtime_helper_decls.insert(sym, f);
            self.runtime_helper_param_counts.insert(sym, params);
        }
    }

    /// ADR-0058f §3.2 — module-level `Constant::Str` interning.
    ///
    /// Walks every body's statements + terminator args and registers
    /// each unique `Constant::Str` payload as a private `unnamed_addr`
    /// `[N x i8]` rodata global. The resulting `i8*` pointer is cached
    /// in `str_data_globals` and consumed by `materialize_str_data` /
    /// `materialize_str_buffer` during body lowering.
    ///
    /// Mirrors Cranelift's per-body interning at
    /// `cranelift_backend.rs:873-1013`, but at module scope (LLVM
    /// globals are module-level).
    pub fn intern_str_payloads(&mut self, module: &Module) {
        // Local helper closure can't capture `&mut self` mutably + read
        // it concurrently — collect payloads in a Vec first, then emit.
        let mut payloads: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut push_unique = |p: &str, payloads: &mut Vec<String>| {
            if seen.insert(p.to_string()) {
                payloads.push(p.to_string());
            }
            // F54: f-string precision sentinels arrive as `FMTSPEC:.2f`
            // but `lower_aggregate_format_string` materializes the STRIPPED
            // spec (`.2f`) for `__cobrust_fmt_float_prec`. Intern both so
            // the stripped form resolves in `str_data_ptr_for`.
            if let Some(stripped) = p.strip_prefix("FMTSPEC:") {
                if seen.insert(stripped.to_string()) {
                    payloads.push(stripped.to_string());
                }
            }
        };

        for body in &module.bodies {
            for mir_block in &body.blocks {
                for stmt in &mir_block.statements {
                    if let StatementKind::Assign { rvalue, .. } = &stmt.kind {
                        collect_str_payloads_from_rvalue(rvalue, &mut |p| {
                            push_unique(p, &mut payloads);
                        });
                    }
                }
                if let Terminator::Call { args, .. } = &mir_block.terminator {
                    for arg in args {
                        if let Operand::Constant(Constant::Str(payload)) = arg {
                            push_unique(payload, &mut payloads);
                        }
                        if let Operand::Constant(Constant::Bytes(bytes)) = arg {
                            // Bytes lower through the same str-buffer
                            // path under wave-2 (lossy UTF-8). Intern.
                            if let Ok(s) = std::str::from_utf8(bytes) {
                                push_unique(s, &mut payloads);
                            }
                        }
                    }
                }
            }
        }

        // Emit each unique payload as a private rodata `[N x i8]` global.
        for (idx, payload) in payloads.into_iter().enumerate() {
            let bytes = payload.as_bytes();
            let i8_ty = self.ctx.i8_type();
            let arr_ty = i8_ty.array_type(bytes.len() as u32);
            let const_arr = i8_ty.const_array(
                &bytes
                    .iter()
                    .map(|b| i8_ty.const_int(u64::from(*b), false))
                    .collect::<Vec<_>>(),
            );
            let symbol = format!("__cobrust_str_data_{idx}");
            let global = self.module.add_global(arr_ty, None, &symbol);
            global.set_initializer(&const_arr);
            global.set_constant(true);
            global.set_linkage(Linkage::Private);
            global.set_unnamed_addr(true);
            // Cast the `[N x i8]*` global to `i8*` for the consumer side.
            let ptr_val = self.builder.build_pointer_cast(
                global.as_pointer_value(),
                self.opaque_ptr_ty,
                "str_data_ptr",
            );
            // build_pointer_cast can't fail in practice for opaque-ptr
            // casts; fall back to the raw ptr on the error path.
            let final_ptr = match ptr_val {
                Ok(v) => v,
                Err(_) => global.as_pointer_value(),
            };
            self.str_data_globals.insert(payload, final_ptr);
        }
    }

    // =====================================================================
    // §4.0 — F56: LLVM port of Cranelift's `infer_local_types` fixed-point
    // =====================================================================
    //
    // The MIR lowering spills sub-expressions into synthetic temporaries
    // declared `Ty::None` (e.g. `-(-3.25)` lowers to an inner temp
    // `_inner = UnaryOp(Neg, Float(3.25))` and an outer temp
    // `_outer = UnaryOp(Neg, Copy(_inner))`, both `Ty::None`). `lower_ty`
    // maps `Ty::None → i64`, so without inference the float bits land in
    // an i64 alloca and `lower_unop` sees an `IntValue` → `build_int_neg`
    // on the IEEE bit-pattern (garbage) rather than `build_float_neg`.
    //
    // This is a 1:1 port of `cranelift_backend::{infer_local_types,
    // rvalue_ty, operand_ty}` (the ADR-0033 / ADR-0034 / ADR-0044
    // fixed-point), adapted from `cranelift::ir::Type` to inkwell
    // `BasicTypeEnum`. The resolved-type concept is mapped to LLVM using
    // the SAME mapping `lower_ty` uses, so alloca / store / load types
    // stay consistent. The fixed-point (not a single pass) is what
    // resolves chain depth ≥ 2: `_outer` depends on `_inner`'s inferred
    // type, which may only materialize in a later iteration.
    //
    // Divergence from the Cranelift reference (documented per F56):
    //   * The LLVM emitter has no `runtime_helper_return_types` map (only
    //     `runtime_helper_param_counts`), so the `Constant::Str(helper)`
    //     branch of the Call-destination pre-pass + fixed-point is
    //     omitted. The `Constant::FnRef(known body)` branch (via
    //     `body_return_types`) and the `Assign` fixed-point are retained —
    //     sufficient for the arithmetic-spill case (fr14) and every
    //     candidate that resolves through an Assign rvalue. Runtime-call
    //     destinations of `Ty::None` type keep today's `i64` fallback
    //     (unchanged behavior).
    //   * `Ty::Ref(inner)` is scalar under the LLVM `lower_ty` (transparent
    //     recursion) whereas Cranelift treats it as non-scalar. The LLVM
    //     scalar predicate below mirrors `lower_ty`'s notion of "resolves
    //     to a direct (non-pointer-fallback) LLVM scalar".

    /// LLVM analogue of `cranelift_scalar_ty(..).is_some()`: returns the
    /// resolved scalar `BasicTypeEnum` for a `Ty` that `lower_ty` maps to
    /// a direct scalar (Bool/Int/IntN/Float/Imag, plus transparent
    /// `Ref(scalar)`), or `None` for `Ty::None` and every type that
    /// `lower_ty` lowers to the opaque pointer. Pointer-lowered types are
    /// excluded so they remain *candidates* (their codegen type is the
    /// opaque `i8*`, recovered via the inferred map / the pointer
    /// fallback) — matching the Cranelift posture where indirect types
    /// have `cranelift_scalar_ty == None`.
    fn llvm_scalar_ty(&self, ty: &Ty) -> Option<BasicTypeEnum<'ctx>> {
        match ty {
            Ty::Bool => Some(self.ctx.bool_type().as_basic_type_enum()),
            Ty::Int => Some(self.ctx.i64_type().as_basic_type_enum()),
            Ty::Float | Ty::Imag => Some(self.ctx.f64_type().as_basic_type_enum()),
            Ty::IntN(8) => Some(self.ctx.i8_type().as_basic_type_enum()),
            Ty::IntN(16) => Some(self.ctx.i16_type().as_basic_type_enum()),
            Ty::IntN(32) => Some(self.ctx.i32_type().as_basic_type_enum()),
            Ty::IntN(_) => Some(self.ctx.i64_type().as_basic_type_enum()),
            // `Ref(T)` is transparent in `lower_ty`: scalar iff `T` is.
            Ty::Ref(inner) => self.llvm_scalar_ty(inner),
            // `Ty::None` (placeholder) + all pointer-lowered indirect
            // types are NOT scalars for inference purposes.
            _ => None,
        }
    }

    /// Resolve an rvalue to its codegen `BasicTypeEnum` given the
    /// in-progress `inferred` map (1:1 with `cranelift_backend::rvalue_ty`).
    fn llvm_rvalue_ty(
        &self,
        body: &Body,
        rvalue: &Rvalue,
        inferred: &HashMap<LocalId, BasicTypeEnum<'ctx>>,
    ) -> Option<BasicTypeEnum<'ctx>> {
        match rvalue {
            Rvalue::Use(op) => self.llvm_operand_ty(body, op, inferred),
            Rvalue::BinaryOp(op, a, _b) => match op {
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::LtEq
                | BinOp::Gt
                | BinOp::GtEq
                | BinOp::And
                | BinOp::Or
                | BinOp::In
                | BinOp::NotIn => Some(self.ctx.bool_type().as_basic_type_enum()),
                _ => self.llvm_operand_ty(body, a, inferred),
            },
            Rvalue::UnaryOp(_, a) => self.llvm_operand_ty(body, a, inferred),
            Rvalue::Cast(_, _, ty) => self.llvm_scalar_ty(ty),
            Rvalue::Aggregate(_, _) | Rvalue::Ref(_, _) => {
                Some(self.opaque_ptr_ty.as_basic_type_enum())
            }
            Rvalue::Discriminant(_) | Rvalue::Len(_) | Rvalue::NullaryOp(_) => {
                Some(self.ctx.i64_type().as_basic_type_enum())
            }
        }
    }

    /// Resolve an operand to its codegen `BasicTypeEnum` (1:1 with
    /// `cranelift_backend::operand_ty`). `Copy`/`Move` prefer the
    /// `inferred` map, then fall back to the declared scalar type, then to
    /// the opaque pointer for indirect types.
    fn llvm_operand_ty(
        &self,
        body: &Body,
        op: &Operand,
        inferred: &HashMap<LocalId, BasicTypeEnum<'ctx>>,
    ) -> Option<BasicTypeEnum<'ctx>> {
        match op {
            Operand::Copy(p) | Operand::Move(p) => {
                if let Some(ty) = inferred.get(&p.local) {
                    return Some(*ty);
                }
                body.locals.get(p.local.0 as usize).map(|l| {
                    self.llvm_scalar_ty(&l.ty)
                        .unwrap_or_else(|| self.opaque_ptr_ty.as_basic_type_enum())
                })
            }
            Operand::Constant(c) => Some(match c {
                Constant::Bool(_) | Constant::None => self.ctx.bool_type().as_basic_type_enum(),
                Constant::Int(_) => self.ctx.i64_type().as_basic_type_enum(),
                Constant::Float(_) | Constant::Imag(_) => self.ctx.f64_type().as_basic_type_enum(),
                Constant::Str(_) | Constant::Bytes(_) | Constant::FnRef(_) => {
                    self.opaque_ptr_ty.as_basic_type_enum()
                }
            }),
        }
    }

    /// Fixed-point inference of codegen types for candidate locals
    /// (declared `Ty::None`, or non-scalar per `llvm_scalar_ty`). Returns
    /// a map containing ONLY candidate locals that resolved; callers fall
    /// back to `lower_ty` (i.e. the `Ty::None → i64` default) for absent
    /// entries, preserving today's behavior for genuinely-untyped
    /// pointer / `_callret` slots.
    ///
    /// 1:1 port of `cranelift_backend::infer_local_types` (ADR-0033
    /// fixed-point + ADR-0034 / ADR-0044 Call-destination pre-pass). The
    /// runtime-helper (`Constant::Str`) branch is omitted on the LLVM side
    /// (no `runtime_helper_return_types` map) — see the §4.0 divergence
    /// note.
    fn infer_local_types(&self, body: &Body) -> HashMap<LocalId, BasicTypeEnum<'ctx>> {
        let mut candidates: Vec<LocalId> = Vec::new();
        for local in &body.locals {
            if matches!(local.ty, Ty::None) || self.llvm_scalar_ty(&local.ty).is_none() {
                candidates.push(local.id);
            }
        }

        let mut out: HashMap<LocalId, BasicTypeEnum<'ctx>> = HashMap::new();

        // Call-destination pre-pass: resolve every `Terminator::Call`
        // whose func is a known-body `Constant::FnRef` into `out` before
        // the scan-based fixed-point, so the scan never pins a wrong type
        // for a call destination via iteration order (ADR-0044 rationale).
        for &local_id in &candidates {
            for block in &body.blocks {
                if let Terminator::Call {
                    func, destination, ..
                } = &block.terminator
                {
                    if destination.local != local_id || !destination.projections.is_empty() {
                        continue;
                    }
                    if let Operand::Constant(Constant::FnRef(id)) = func {
                        if let Some(ty) = self.body_return_types.get(id).copied() {
                            out.insert(local_id, ty);
                            break;
                        }
                    }
                }
            }
        }

        // Fixed-point. Terminates because each iteration only adds
        // entries, the candidate set is finite, and an iteration that
        // adds nothing ends the loop. Bound at `candidates.len() + 1`
        // defensively against a malformed self-referential chain.
        let max_iters = candidates.len() + 1;
        for _ in 0..max_iters {
            let before = out.len();
            for &local_id in &candidates {
                if out.contains_key(&local_id) {
                    continue;
                }
                // Call-destination via known-body FnRef (Assign-less local).
                let mut found = false;
                'tscan: for block in &body.blocks {
                    if let Terminator::Call {
                        func, destination, ..
                    } = &block.terminator
                    {
                        if destination.local == local_id && destination.projections.is_empty() {
                            if let Operand::Constant(Constant::FnRef(id)) = func {
                                if let Some(ty) = self.body_return_types.get(id).copied() {
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
                // First Assign to this local that yields a resolvable type
                // given the current `out` snapshot.
                'scan: for block in &body.blocks {
                    for stmt in &block.statements {
                        if let StatementKind::Assign { place, rvalue } = &stmt.kind {
                            if place.local == local_id && place.projections.is_empty() {
                                if let Some(ty) = self.llvm_rvalue_ty(body, rvalue, &out) {
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

    // =====================================================================
    // §4 — MIR Ty → LLVM type mapping
    // =====================================================================

    /// Lower a Cobrust MIR `Ty` to an inkwell `BasicTypeEnum`.
    ///
    /// Per ADR-0058a §4.1 (revised):
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
    /// | `None` | `i64` (mirrors Cranelift's `pointer_type` fallback) |
    /// | `Tuple(...)` / `Record(_)` / `Adt(_,_)` | `i8*` (by pointer at wave-1) |
    /// | other | `i8*` fallback |
    ///
    /// `Ty::None` lowers to `i64` (not `i8`) to match the Cranelift
    /// backend's `cranelift_scalar_ty(...).unwrap_or(pointer_type)`
    /// posture. MIR uses `Ty::None` as a placeholder for "type not yet
    /// inferred" on synthetic temporaries; defaulting to `i64` matches
    /// what those temps hold in practice (recursion return values,
    /// `_callret` slots, etc.). Cranelift backend §"infer_local_types"
    /// converges these via a fixed-point dataflow; wave-1 LLVM backend
    /// takes the simpler fallback. If a real i8 unit value is ever
    /// needed, MIR can be explicit (Ty::Bool widens to i64 at use sites).
    fn lower_ty(&self, ty: &Ty) -> BasicTypeEnum<'ctx> {
        match ty {
            Ty::Bool => self.ctx.bool_type().as_basic_type_enum(),
            Ty::Int => self.ctx.i64_type().as_basic_type_enum(),
            Ty::Float | Ty::Imag => self.ctx.f64_type().as_basic_type_enum(),
            Ty::None => self.ctx.i64_type().as_basic_type_enum(),
            Ty::Ref(inner) => self.lower_ty(inner),
            // ADR-0060a — narrow-int types lower to their native LLVM
            // width via inkwell's `iN_type()` constructors.
            Ty::IntN(8) => self.ctx.i8_type().as_basic_type_enum(),
            Ty::IntN(16) => self.ctx.i16_type().as_basic_type_enum(),
            Ty::IntN(32) => self.ctx.i32_type().as_basic_type_enum(),
            // Unknown narrow width — fall back to i64.
            Ty::IntN(_) => self.ctx.i64_type().as_basic_type_enum(),
            // ADR-0060b — `[T; N]` arrays lower to `[N x T]` at LLVM
            // type level. The MIR `Place::index` projection materializes
            // GEPs against this in-memory layout. Wave-2 supports
            // element types {Int / IntN / Float / Bool / opaque-ptr}.
            Ty::Array(elem, n) => {
                let elem_ty = self.lower_ty(elem);
                let n32 = u32::try_from(*n).unwrap_or(u32::MAX);
                match elem_ty {
                    BasicTypeEnum::IntType(it) => it.array_type(n32).as_basic_type_enum(),
                    BasicTypeEnum::FloatType(ft) => ft.array_type(n32).as_basic_type_enum(),
                    BasicTypeEnum::PointerType(pt) => pt.array_type(n32).as_basic_type_enum(),
                    _ => self.opaque_ptr_ty.as_basic_type_enum(),
                }
            }
            // Owning + container + reference / tuple / record / ADT all
            // lower to opaque pointer at wave-1. Element type stays at
            // MIR level — recovered from per-Place / per-Operand context.
            _ => self.opaque_ptr_ty.as_basic_type_enum(),
        }
    }

    /// Build a function signature given param types + return type.
    fn fn_type_from(
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

        let fn_ty = LlvmEmitter::fn_type_from(&param_tys, ret_ty);
        let func = self
            .module
            .add_function(&name, fn_ty, Some(Linkage::External));

        self.function_ids.insert(body.def_id.0, func);
        self.body_return_types.insert(body.def_id.0, ret_ty);

        // --- ADR-0058c §3.2 per-function DISubprogram ----------------------
        //
        // Build a DISubroutineType from the parameter + return DI basic
        // types, then a DISubprogram rooted at the compile-unit scope.
        // Attach via `FunctionValue::set_subprogram` so the LLVM IR
        // metadata graph wires the function to its DI metadata.
        let ret_di = self.di_type_for(&ret_local.ty);
        let param_di: Vec<inkwell::debug_info::DIType<'ctx>> = param_locals
            .iter()
            .map(|l| self.di_type_for(&l.ty).as_type())
            .collect();
        let subroutine_ty = self.di_builder.create_subroutine_type(
            self.di_file,
            Some(ret_di.as_type()),
            &param_di,
            inkwell::debug_info::DIFlags::ZERO,
        );
        let (line_no, _col) = self.line_map.line_column(body.span.start);
        let subprogram = self.di_builder.create_function(
            self.di_cu.as_debug_info_scope(),
            /* fn name */ &name,
            /* linkage name */ None,
            self.di_file,
            line_no,
            subroutine_ty,
            /* is_local_to_unit */ false,
            /* is_definition */ true,
            /* scope_line */ line_no,
            inkwell::debug_info::DIFlags::ZERO,
            /* is_optimized */ self.is_optimized,
        );
        func.set_subprogram(subprogram);
        self.di_subprograms.insert(body.def_id.0, subprogram);

        Ok(())
    }

    /// Second pass — emit the function body.
    pub fn define_body(&mut self, body: &Body) -> Result<(), CodegenError> {
        let func = *self.function_ids.get(&body.def_id.0).ok_or_else(|| {
            CodegenError::Internal(format!("body {} not declared", body.def_id.0))
        })?;
        let ret_ty = *self.body_return_types.get(&body.def_id.0).ok_or_else(|| {
            CodegenError::Internal(format!("body {} return type missing", body.def_id.0))
        })?;

        // Create one LLVM basic block per MIR block.
        let mut block_map: HashMap<BlockId, BasicBlock<'ctx>> = HashMap::new();
        for mir_block in &body.blocks {
            let label = format!("bb{}", mir_block.id.0);
            let bb = self.ctx.append_basic_block(func, &label);
            block_map.insert(mir_block.id, bb);
        }

        // Capture the per-body DISubprogram (declared in `declare_body`)
        // before the allocas emit — LLVM `Module::verify` rejects any
        // `!dbg` attachment that points at a different subprogram than
        // the function it's attached to, so we must reset the builder's
        // current debug location to *this* subprogram before any alloca
        // / store hits the IR stream (ADR-0058c §3.3).
        let subprogram = self
            .di_subprograms
            .get(&body.def_id.0)
            .copied()
            .ok_or_else(|| {
                CodegenError::Internal(format!("body {} missing DISubprogram", body.def_id.0))
            })?;

        // Entry block sets up allocas + binds parameters. Use a
        // dedicated "allocas" block prepended in front of bb0.
        let entry_bb = block_map[&BlockId(0)];
        let allocas_bb = self.ctx.prepend_basic_block(entry_bb, "allocas");
        self.builder.position_at_end(allocas_bb);

        // ADR-0058c §3.3 multi-fn fix: every body resets the current
        // debug location to a DILocation rooted at *its own*
        // subprogram before alloca/store emission. Without this, the
        // builder leaks the previous body's location into this body's
        // entry block, and `Module::verify` rejects the resulting IR
        // with "!dbg attachment points at wrong subprogram".
        let (init_line, init_col) = self.line_map.line_column(body.span.start);
        let body_entry_loc = self.di_builder.create_debug_location(
            self.ctx,
            init_line,
            init_col,
            subprogram.as_debug_info_scope(),
            None,
        );
        self.builder.set_current_debug_location(body_entry_loc);

        // F56: port of Cranelift's `infer_local_types` fixed-point. Compute
        // the inferred codegen type for every candidate `Ty::None` /
        // non-scalar temp BEFORE the alloca loop, so synthetic float spill
        // temps (e.g. the `_inner` / `_outer` of `-(-3.25)`) get `double`
        // alloca slots instead of the `Ty::None → i64` default — keeping
        // store / load / `lower_unop` on the float path (`build_float_neg`).
        let inferred_local_tys = self.infer_local_types(body);

        let mut local_allocas: HashMap<LocalId, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)> =
            HashMap::new();
        for local in &body.locals {
            // Use the body's return type for the synthetic return slot
            // (parallels Cranelift's inferred_ret).
            let ty: BasicTypeEnum<'ctx> = if local.id == body.return_local {
                ret_ty
            } else if let Some(inferred) = inferred_local_tys.get(&local.id) {
                // Candidate `Ty::None` / non-scalar local whose effective
                // type the fixed-point resolved. Falls through to
                // `lower_ty` only for locals with no inferred entry
                // (genuinely-untyped pointer / `_callret` slots), which
                // keeps the historical `Ty::None → i64` fallback.
                *inferred
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
                .ok_or_else(|| CodegenError::Internal(format!("missing param {idx}")))?;
            let (alloca, _) = local_allocas[&local.id];
            self.builder
                .build_store(alloca, param)
                .map_err(map_builder_err)?;
        }

        // Branch from allocas → entry.
        self.builder
            .build_unconditional_branch(entry_bb)
            .map_err(map_builder_err)?;

        // Lower every MIR block via the per-Body lowerer (subprogram
        // captured above for the entry-block debug-location reset).
        let blocks = body.blocks.clone();
        let mut lowerer = BodyLowerer {
            emitter: self,
            body,
            func,
            block_map: &block_map,
            local_allocas: &local_allocas,
            ret_ty,
            subprogram,
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
///
/// ADR-0058c §3.3: carries the per-body `DISubprogram` so each
/// instruction's debug location can root at the right function scope.
struct BodyLowerer<'a, 'ctx> {
    emitter: &'a mut LlvmEmitter<'ctx>,
    body: &'a Body,
    func: FunctionValue<'ctx>,
    block_map: &'a HashMap<BlockId, BasicBlock<'ctx>>,
    local_allocas: &'a HashMap<LocalId, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    ret_ty: BasicTypeEnum<'ctx>,
    /// Per-body DISubprogram scope (ADR-0058c §3.3).
    subprogram: DISubprogram<'ctx>,
}

impl<'a, 'ctx> BodyLowerer<'a, 'ctx> {
    /// Resolve a `Span` to a `DILocation` rooted at the subprogram
    /// scope, then set it as the builder's current debug location so
    /// every subsequent instruction emission is tagged with it
    /// (ADR-0058c §3.3).
    fn set_debug_loc(&self, span: Span) {
        let (line, col) = self.emitter.line_map.line_column(span.start);
        let loc: DILocation<'ctx> = self.emitter.di_builder.create_debug_location(
            self.emitter.ctx,
            line,
            col,
            self.subprogram.as_debug_info_scope(),
            None,
        );
        self.emitter.builder.set_current_debug_location(loc);
    }

    /// Checked lookup of a local's `(alloca, type)`. Returns
    /// [`CodegenError::InvalidMir`] when the `LocalId` is not declared
    /// in the body (dangling MIR) instead of panicking on a missing
    /// `HashMap` key. ADR-0070 §X.4: with the Cranelift AOT backend
    /// removed, this restores the structured-error contract the
    /// `codegen_ill_formed` regression suite validates (the Cranelift
    /// backend rejected dangling locals via its IR verifier).
    fn local_alloca(
        &self,
        local: LocalId,
    ) -> Result<(PointerValue<'ctx>, BasicTypeEnum<'ctx>), CodegenError> {
        self.local_allocas.get(&local).copied().ok_or_else(|| {
            CodegenError::InvalidMir(format!(
                "reference to undeclared local _{} in body `{}`",
                local.0, self.body.name
            ))
        })
    }

    /// Checked lookup of a basic block. Returns
    /// [`CodegenError::InvalidMir`] when the `BlockId` is not present
    /// (a terminator targeting a non-existent block) instead of
    /// panicking. See [`Self::local_alloca`] for the §X.4 rationale.
    fn block(&self, id: BlockId) -> Result<BasicBlock<'ctx>, CodegenError> {
        self.block_map.get(&id).copied().ok_or_else(|| {
            CodegenError::InvalidMir(format!(
                "terminator targets non-existent block bb{} in body `{}`",
                id.0, self.body.name
            ))
        })
    }
}

impl<'a, 'ctx> BodyLowerer<'a, 'ctx> {
    fn lower_block(&mut self, mir_block: &cobrust_mir::BasicBlock) -> Result<(), CodegenError> {
        let bb = self.block_map[&mir_block.id];
        self.emitter.builder.position_at_end(bb);
        // ADR-0058c §3.3: anchor every block at its own debug location
        // (the block's span). Per-statement locs override on each step.
        self.set_debug_loc(mir_block.span);
        for stmt in &mir_block.statements {
            self.set_debug_loc(stmt.span);
            self.lower_statement(stmt)?;
        }
        // The terminator inherits the block's span (terminators don't
        // carry their own `Span` in MIR — they're keyed to the closing
        // brace of the block).
        self.set_debug_loc(mir_block.span);
        self.lower_terminator(&mir_block.terminator)?;
        Ok(())
    }

    fn lower_statement(&mut self, stmt: &Statement) -> Result<(), CodegenError> {
        match &stmt.kind {
            StatementKind::Assign { place, rvalue } => {
                // ADR-0058f §3.5 cascade fix (mirror of Cranelift's
                // `lower_statement` at `cranelift_backend.rs:1266-1276`):
                // `let v: str = "hello"` lowers to `Assign(v, Use(
                // Constant::Str("hello")))`. The default `lower_constant`
                // path returns a heap StringBuffer pointer (wave-2),
                // but the explicit Str-typed Assign path is hot enough
                // to deserve the direct route — same shape avoids the
                // double-lookup in str_data_globals.
                //
                // F47 fix (2026-05-25): also fire on `_return =
                // Use(Constant::Str(_))` so user-fn `str` returns
                // produce a real `StringBuffer` pointer instead of
                // the M9 stub zero pointer. Mirrors the parallel fix
                // in `cranelift_backend.rs`. Without this, the
                // caller's `let s: str = make_str()` binds null,
                // any downstream `__cobrust_str_ptr(s)` /
                // `__cobrust_str_len(s)` reads zero, producing empty
                // f-string interpolation (`f"got {s}!"` →
                // `"got !"`).
                if let Rvalue::Use(Operand::Constant(Constant::Str(payload))) = rvalue {
                    let dest_ty = self
                        .body
                        .locals
                        .get(place.local.0 as usize)
                        .map(|l| l.ty.clone());
                    let is_str_dest = matches!(dest_ty, Some(Ty::Str));
                    let is_return_slot = place.local == self.body.return_local;
                    if (is_str_dest || is_return_slot) && place.projections.is_empty() {
                        let value = self.materialize_str_buffer(payload)?;
                        return self.write_place(place, value);
                    }
                }
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

    /// ADR-0058f §3.3 — return the rodata `i8*` pointer + i64 byte-length
    /// pair for a str literal. The pointer is the module-level global
    /// interned by `LlvmEmitter::intern_str_payloads`.
    ///
    /// Mirrors `cranelift_backend::EmitCtx::materialize_str_data`
    /// (`cranelift_backend.rs:1130-1145`).
    fn materialize_str_data(
        &mut self,
        payload: &str,
    ) -> Result<(BasicValueEnum<'ctx>, BasicValueEnum<'ctx>), CodegenError> {
        let ptr = self
            .emitter
            .str_data_globals
            .get(payload)
            .copied()
            .ok_or_else(|| {
                CodegenError::Internal(format!(
                    "str payload {payload:?} not interned; intern_str_payloads pre-pass bug"
                ))
            })?;
        let len = self
            .emitter
            .ctx
            .i64_type()
            .const_int(payload.len() as u64, false);
        Ok((ptr.into(), len.into()))
    }

    /// ADR-0058f §3.3 — materialize a source-level string literal as a
    /// Cobrust heap `Str` buffer. Calls `__cobrust_str_new()` then
    /// `__cobrust_str_push_static(buf, ptr, len)` when payload is
    /// non-empty. Returns the resulting `*mut Str` pointer.
    ///
    /// Mirrors `cranelift_backend::EmitCtx::materialize_str_buffer`
    /// (`cranelift_backend.rs:1149-1163`).
    fn materialize_str_buffer(
        &mut self,
        payload: &str,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let str_new = *self
            .emitter
            .runtime_helper_decls
            .get("__cobrust_str_new")
            .ok_or_else(|| {
                CodegenError::Internal(
                    "__cobrust_str_new not declared; declare_runtime_helpers wave-2 bug".into(),
                )
            })?;
        let call = self
            .emitter
            .builder
            .build_call(str_new, &[], "str_new")
            .map_err(map_builder_err)?;
        let buf: BasicValueEnum<'ctx> = call
            .try_as_basic_value()
            .basic()
            .unwrap_or_else(|| self.emitter.opaque_ptr_ty.const_null().into());
        if !payload.is_empty() {
            let (ptr_val, len_val) = self.materialize_str_data(payload)?;
            let push = *self
                .emitter
                .runtime_helper_decls
                .get("__cobrust_str_push_static")
                .ok_or_else(|| {
                    CodegenError::Internal(
                        "__cobrust_str_push_static not declared; wave-2 bug".into(),
                    )
                })?;
            let args: [BasicMetadataValueEnum<'ctx>; 3] =
                [buf.into(), ptr_val.into(), len_val.into()];
            self.emitter
                .builder
                .build_call(push, &args, "str_push")
                .map_err(map_builder_err)?;
        }
        Ok(buf)
    }

    fn lower_terminator(&mut self, term: &Terminator) -> Result<(), CodegenError> {
        match term {
            Terminator::Goto(target) => {
                let blk = self.block(*target)?;
                self.emitter
                    .builder
                    .build_unconditional_branch(blk)
                    .map_err(map_builder_err)?;
                Ok(())
            }
            Terminator::Return => {
                let (alloca, _) = self.local_alloca(self.body.return_local)?;
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
                let target_blk = self.block(*target)?;
                let trap_blk = self
                    .emitter
                    .ctx
                    .append_basic_block(self.func, "assert_trap");
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
                let blk = self.block(*target)?;
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
                    // ADR-0058f §3.5 cascade fix (mirror of Cranelift
                    // `cranelift_backend.rs:1414-1420`): when a user-fn
                    // call passes a `Constant::Str` literal arg into a
                    // Str-typed parameter, the default `lower_operand`
                    // would still work (lower_constant now materializes
                    // a buffer), but going through the explicit path
                    // here keeps parity with the Cranelift surface so
                    // a future double-lookup elision is symmetric.
                    let v = if let Operand::Constant(Constant::Str(payload)) = arg {
                        self.materialize_str_buffer(payload)?
                    } else {
                        self.lower_operand(arg)?
                    };
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
                let blk = self.block(target)?;
                self.emitter
                    .builder
                    .build_unconditional_branch(blk)
                    .map_err(map_builder_err)?;
                return Ok(());
            }
            // Falls through to stub fallthrough below for unknown FnRef
            // ids (lambda placeholder `FnRef(0)`, await `FnRef(u32::MAX)`).
        }

        // ADR-0058f §3.4 — extern-name dispatch. Mirrors Cranelift
        // backend at `cranelift_backend.rs:1439-1521`. When `func` is
        // `Constant::Str(name)`, look up the runtime-helper FunctionValue
        // by name; if found, lower args + emit `build_call`.
        if let Operand::Constant(Constant::Str(name)) = func {
            if let Some(callee) = self
                .emitter
                .runtime_helper_decls
                .get(name.as_str())
                .copied()
            {
                let sig_param_count = self
                    .emitter
                    .runtime_helper_param_counts
                    .get(name.as_str())
                    .copied()
                    .unwrap_or(args.len());
                // Detect the `(*const u8, usize)` expansion case:
                // single source Str arg → two C params (ptr, len).
                let expand_str_to_ptr_len = args.len() == 1
                    && sig_param_count == 2
                    && matches!(args.first(), Some(Operand::Constant(Constant::Str(_))));
                // Detect trailing Str arg expansion: source supplies
                // N args ending in Str, C signature has N+1 params.
                let expand_trailing_str_len = !expand_str_to_ptr_len
                    && args.len() + 1 == sig_param_count
                    && matches!(args.last(), Some(Operand::Constant(Constant::Str(_))));

                let mut call_args: Vec<BasicMetadataValueEnum<'ctx>> =
                    Vec::with_capacity(sig_param_count);
                for (idx, arg) in args.iter().enumerate() {
                    if let Operand::Constant(Constant::Str(payload)) = arg {
                        let is_last = idx + 1 == args.len();
                        if expand_str_to_ptr_len || (expand_trailing_str_len && is_last) {
                            let (ptr, len) = self.materialize_str_data(payload)?;
                            // F71: `materialize_str_data` always returns an
                            // i64 length, but the expanded `usize` C param
                            // (`__cobrust_println`, `__cobrust_panic`, …) is
                            // pointer-width — i32 on wasm32. Coerce the len
                            // to the callee's declared param type so the
                            // value matches the (now target-width) signature.
                            // The len lands at param slot `call_args.len()`
                            // (the ptr was just pushed at the slot before it).
                            call_args.push(ptr.into());
                            let len_slot = call_args.len();
                            let len_val = if let Some(BasicMetadataTypeEnum::IntType(pt)) =
                                callee.get_type().get_param_types().get(len_slot)
                            {
                                self.coerce_value_to(len, (*pt).into())?
                            } else {
                                len
                            };
                            call_args.push(len_val.into());
                        } else {
                            let buf = self.materialize_str_buffer(payload)?;
                            call_args.push(buf.into());
                        }
                    } else {
                        let mut v = self.lower_operand(arg)?;
                        // ADR-0058f §4 bool widening: `__cobrust_println_bool`
                        // takes i8, but MIR `Constant::Bool` lowers to i1.
                        // Detect narrow int args going into wider int
                        // helper params and z_extend them. The callee's
                        // signature param type comes back as
                        // `BasicMetadataTypeEnum`; match on its IntType
                        // variant to get the width.
                        if let BasicValueEnum::IntValue(iv) = v {
                            if let Some(param_ty) = callee.get_type().get_param_types().get(idx) {
                                if let BasicMetadataTypeEnum::IntType(pt) = param_ty {
                                    let pt_width = pt.get_bit_width();
                                    let v_width = iv.get_type().get_bit_width();
                                    if v_width < pt_width {
                                        let widened = self
                                            .emitter
                                            .builder
                                            .build_int_z_extend(iv, *pt, "argext")
                                            .map_err(map_builder_err)?;
                                        v = widened.into();
                                    }
                                } else if matches!(param_ty, BasicMetadataTypeEnum::PointerType(_))
                                {
                                    // ADR-0070 §X.3 sibling-fix (2026-05-26):
                                    // MIR represents list / heap-string values
                                    // as i64 stack-slot encodings of host
                                    // pointers; runtime helpers like
                                    // `__cobrust_list_len`, `__cobrust_list_get`,
                                    // `__cobrust_str_clone` etc. declare their
                                    // first argument as `ptr` (opaque pointer
                                    // in LLVM 15+). When the lowered Operand
                                    // resolves to an IntValue but the callee
                                    // signature expects a PointerType, emit
                                    // an `inttoptr` cast — mirrors the existing
                                    // `Drop`-call coercion at the `Drop`
                                    // terminator handler above.
                                    let ptr_v = self
                                        .emitter
                                        .builder
                                        .build_int_to_ptr(iv, self.emitter.opaque_ptr_ty, "argi2p")
                                        .map_err(map_builder_err)?;
                                    v = ptr_v.into();
                                } else if matches!(param_ty, BasicMetadataTypeEnum::FloatType(_)) {
                                    // ADR-0070 §X.3 sibling-fix (2026-05-26):
                                    // MIR's `Rvalue::BinaryOp` allocates its
                                    // result as `Ty::None` → `i64` (see
                                    // `cobrust-mir/src/lower.rs:1945`), so a
                                    // float arithmetic chain like `(a + b) /
                                    // 2.0` produces an `i64`-typed `_bin` slot
                                    // holding the f64 bit-pattern. When this
                                    // i64 then flows into an `f64 -> f64`
                                    // runtime fn — the bare libm `sqrt` /
                                    // `hypot` / `atan2` / … (ADR-0083 `math`
                                    // module) or the older `__cobrust_math_*`
                                    // prelude shims — we must
                                    // reinterpret the i64 bits as f64 via
                                    // `bitcast`. Matches the Cranelift backend
                                    // tolerance which simply forwards the
                                    // 64-bit value through the ABI register.
                                    let fv = self
                                        .emitter
                                        .builder
                                        .build_bit_cast(iv, self.emitter.ctx.f64_type(), "argi2f")
                                        .map_err(map_builder_err)?;
                                    v = fv;
                                }
                            }
                        }
                        call_args.push(v.into());
                    }
                }
                let call_site = self
                    .emitter
                    .builder
                    .build_call(callee, &call_args, "extern_call")
                    .map_err(map_builder_err)?;
                // ADR-0058g sub-wave-1 — `__cobrust_panic` diverges
                // (`-> !` at the Rust side; stdlib export at
                // `cobrust-stdlib/src/panic.rs:47`). Per ADR-0058g §6.2
                // resolution (Cobrust does NOT use exceptions as the
                // default error path — CLAUDE.md §2.2), we emit
                // `call` + `unreachable` (no `invoke` / EH table). This
                // matches Cranelift's `InstructionData::Unreachable`
                // terminator for the same call site and satisfies LLVM's
                // basic-block terminator constraint without going through
                // the unconditional-branch path below (which would be
                // dead code after a noreturn callee).
                if name.as_str() == "__cobrust_panic" {
                    self.emitter
                        .builder
                        .build_unreachable()
                        .map_err(map_builder_err)?;
                    return Ok(());
                }
                // Many of the print helpers return `void`. Treat
                // absent basic-value return as i64 zero (matches
                // Cranelift `lower_terminator` extern path).
                let ret_val: BasicValueEnum<'ctx> = call_site
                    .try_as_basic_value()
                    .basic()
                    .unwrap_or_else(|| self.emitter.ctx.i64_type().const_zero().into());
                self.write_place(destination, ret_val)?;
                let blk = self.block(target)?;
                self.emitter
                    .builder
                    .build_unconditional_branch(blk)
                    .map_err(map_builder_err)?;
                return Ok(());
            }
            // Unknown extern name — fall through to wave-1 stub. Wave-3
            // surfaces (input / file / list / dict / iter / fmt / math)
            // tracked in ADR-0058f §7.
        }

        // Wave-1 stub fallthrough — write 0 into destination, branch.
        // Unknown FnRef ids (lambda placeholder `FnRef(0)`, await
        // placeholder `FnRef(u32::MAX)`) AND unknown extern names from
        // ADR-0058f §7 wave-3 surfaces both land here. Closes once
        // wave-3 lands (or sooner if a wave-3 helper is dispatched).
        let zero: BasicValueEnum<'ctx> = self.emitter.ctx.i64_type().const_zero().into();
        self.write_place(destination, zero)?;
        let blk = self.block(target)?;
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
        //   - Ty::Dict(_, _) → __cobrust_dict_drop(ptr)   (ADR-0058g sub-wave-3,
        //     ADR-0058g §6.1 dict TD-1 closure; mirrors Cranelift
        //     `cranelift_backend.rs:1232-1237`)
        //   - Ty::Set / Ty::Tuple → no-op (matches Cranelift no-op at
        //     `cranelift_backend.rs:1238-1240`; widening tracked as Phase G
        //     followup per ADR-0050c §"Phase G followup" comment)
        //   - other → no-op
        let helper = match ty {
            Ty::Str => Some("__cobrust_str_drop"),
            Ty::List(elem) if matches!(**elem, Ty::Str) => Some("__cobrust_list_drop_elems"),
            Ty::List(_) => Some("__cobrust_list_drop"),
            Ty::Dict(_, _) => Some("__cobrust_dict_drop"),
            // ADR-0072 §3 / §5 risk 1 — ecosystem nominal handle drop.
            // The reserved-id `Ty::Adt` (e.g. `den.Connection`/`Cursor`)
            // maps to its foreign drop symbol, emitted exactly once at
            // scope exit by the (Str/List-template) drop schedule.
            Ty::Adt(id, _) => cobrust_types::handle_drop_symbol(*id),
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
                    .build_int_to_ptr(val.into_int_value(), self.emitter.opaque_ptr_ty, "drop_arg")
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
        let otherwise_blk = self.block(otherwise)?;
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
                Ok((case_val, self.block(*target)?))
            })
            .collect::<Result<Vec<_>, CodegenError>>()?;
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
            // `Ty::None` lowers to `i64` (see `lower_ty`); a bare
            // `Constant::None` therefore is the i64 zero. The coerce
            // pass at write_place() narrows it to the destination's
            // declared type if different.
            Constant::None => Ok(ctx.i64_type().const_zero().into()),
            Constant::Str(payload) => {
                // ADR-0058f §3.5: materialize the literal as a heap
                // StringBuffer pointer. Wave-1 returned `const_null`
                // which left every Str slot null and broke
                // `print(s)` / `let s: str = "hi"` / `fn f(s: str)`.
                self.materialize_str_buffer(payload)
            }
            Constant::Bytes(payload) => {
                // ADR-0058f wave-2 surface: bytes share the same
                // str-buffer path under lossy UTF-8. Wave-3 may
                // introduce a dedicated `__cobrust_bytes_*` family.
                let s = std::str::from_utf8(payload).unwrap_or("");
                self.materialize_str_buffer(s)
            }
            Constant::FnRef(id) => {
                // ADR-0073 §2 D3 — materialise the user fn pointer as a
                // first-class C-ABI pointer value. Pre-ADR-0073 this arm
                // returned `i64 0` (the ADR-0034-preserved stub) because
                // no MIR consumer materialised a fn pointer as a VALUE
                // operand — `Terminator::Call` short-circuits via the
                // `function_ids` lookup in `lower_call`. ADR-0073 lights
                // up the value-operand path so ecosystem-callback args
                // (`app.route("GET", "/x", handle_ping)`) cross the C
                // ABI as a real fn-pointer value to the runtime
                // trampoline.
                //
                // For an unknown id (lambda placeholder `FnRef(0)`,
                // await placeholder `FnRef(u32::MAX)`) keep the legacy
                // zero stub — those paths are not yet wired through to
                // a real value-use site. The ADR-0073 callback path
                // emits only ids registered in `function_ids` (because
                // the typechecker rejects everything but a top-level
                // fn name).
                if let Some(func) = self.emitter.function_ids.get(id) {
                    Ok(func.as_global_value().as_pointer_value().into())
                } else {
                    Ok(ctx.i64_type().const_zero().into())
                }
            }
        }
    }

    fn lower_place_load(&mut self, place: &Place) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let (alloca, ty) = self.local_alloca(place.local)?;
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
        } else if let [Projection::Index(idx_op)] = place.projections.as_slice() {
            // ADR-0060b finding-closure 2026-05-19:
            // `finding:adr0060b-array-indexing-mir-projection-debt`.
            // `Place::index` on a `Ty::Array(elem, n)` base lowers via
            // the safe `build_extract_value` aggregate-extract path
            // when the index is a constant integer (the §3.4
            // compile-time-catch surface). Dynamic-index Array
            // accesses keep the wave-1 stub-load surface — wave-2
            // ships the constant-index path that exercises real
            // element-type lowering through the LLVM type system.
            //
            // Why no GEP: `cobrust-codegen/src/lib.rs:32`
            // `#![forbid(unsafe_code)]` blocks inkwell's unsafe
            // `build_in_bounds_gep`; the safe `build_extract_value`
            // requires a compile-time `u32` index. ADR-0060b §3.4
            // pivots the wave-2 demonstrable surface onto the literal
            // path which is also the prized compile-time-catch payoff.
            let base_local_ty = self.body.locals.get(place.local.0 as usize).map(|d| &d.ty);
            let base_ty_is_array = base_local_ty
                .map(|t| matches!(t, Ty::Array(_, _)))
                .unwrap_or(false);
            if base_ty_is_array {
                // Detect a compile-time-constant integer index.
                let const_idx: Option<u32> = match idx_op {
                    Operand::Constant(Constant::Int(k)) if *k >= 0 => u32::try_from(*k).ok(),
                    _ => None,
                };
                if let Some(idx_u32) = const_idx {
                    // Load the whole array aggregate, then extract.
                    let agg_val = self
                        .emitter
                        .builder
                        .build_load(ty, alloca, "arr_agg")
                        .map_err(map_builder_err)?;
                    if let BasicValueEnum::ArrayValue(av) = agg_val {
                        let elem = self
                            .emitter
                            .builder
                            .build_extract_value(av, idx_u32, "arr_elem")
                            .map_err(map_builder_err)?;
                        return Ok(elem);
                    }
                }
                // Dynamic index — route through runtime helper per
                // ADR-0060b finding-closure (wave-3 dynamic-index).
                // `#![forbid(unsafe_code)]` in codegen lib.rs blocks GEP;
                // runtime helper gives bounds-checked safe path.
                if let Some(Ty::Array(elem_ty, n)) = base_local_ty {
                    let helper_name = match elem_ty.as_ref() {
                        Ty::Int => "__cobrust_array_get_i64",
                        Ty::IntN(32) => "__cobrust_array_get_i32",
                        Ty::IntN(8) => "__cobrust_array_get_i8",
                        Ty::Bool => "__cobrust_array_get_bool",
                        _ => "__cobrust_array_get_i64", // fallback to i64
                    };
                    if let Some(&helper_fn) = self.emitter.runtime_helper_decls.get(helper_name) {
                        // F71: `len` + `idx` are C `usize` (array.rs) —
                        // pointer-width, i32 on wasm32. Materialise the
                        // static N and coerce the runtime index to
                        // `usize_ty` so both match the declared signature
                        // (wasm strict typed calls reject an i64 here).
                        let usize_ty = self.emitter.usize_ty;
                        // Array base alloca (PointerValue) as opaque ptr arg.
                        let arr_ptr_val: BasicMetadataValueEnum<'ctx> = alloca.into();
                        // Static N as `usize`.
                        let len_val: BasicMetadataValueEnum<'ctx> =
                            usize_ty.const_int(*n as u64, false).into();
                        // Runtime index operand, coerced to `usize`
                        // (zext / trunc as the source width demands).
                        let idx_val = self.lower_operand(idx_op)?;
                        let idx_usize: BasicMetadataValueEnum<'ctx> =
                            self.coerce_value_to(idx_val, usize_ty.into())?.into();
                        let call = self
                            .emitter
                            .builder
                            .build_call(
                                helper_fn,
                                &[arr_ptr_val, len_val, idx_usize],
                                "arr_dyn_get",
                            )
                            .map_err(map_builder_err)?;
                        if let Some(v) = call.try_as_basic_value().basic() {
                            return Ok(v);
                        }
                    }
                }
                // Dynamic index not matched or helper not found
                // — fall through to the wave-1 stub-load surface.
            }
            // Non-Array Index projection — fall through to the stub
            // load (preserves the wave-1 surface for List / Dict /
            // Tuple bases that already lowered via runtime helpers).
            let val = self
                .emitter
                .builder
                .build_load(ty, alloca, "load_proj_stub")
                .map_err(map_builder_err)?;
            Ok(val)
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
        let (alloca, ty) = self.local_alloca(place.local)?;
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
        // ADR-0070 §X.3 sibling-fix (2026-05-26): MIR represents the
        // result of every binop as `Ty::None` (-> i64) per
        // `cobrust-mir/src/lower.rs:1945`. When the binop participates
        // in float arithmetic, one operand may still be a FloatValue
        // (the constant rhs from the source) while the other is an
        // IntValue load (the binop chain's intermediate). Match
        // float-ness by inspecting either operand and, when a mismatch
        // exists, bitcast the i64 operand to f64. This preserves
        // round-trip semantics: the i64 stack slot held the f64
        // bit-pattern produced by an earlier float binop. Matches
        // Cranelift's `Type::F64` widening at the SSA layer.
        let either_float = a.is_float_value() || b.is_float_value();
        let (a, b) = if either_float && (!a.is_float_value() || !b.is_float_value()) {
            let bitcast_to_f64 =
                |v: BasicValueEnum<'ctx>| -> Result<BasicValueEnum<'ctx>, CodegenError> {
                    if let BasicValueEnum::IntValue(iv) = v {
                        let fv = self
                            .emitter
                            .builder
                            .build_bit_cast(iv, self.emitter.ctx.f64_type(), "binop_i2f")
                            .map_err(map_builder_err)?;
                        Ok(fv)
                    } else {
                        Ok(v)
                    }
                };
            (bitcast_to_f64(a)?, bitcast_to_f64(b)?)
        } else {
            (a, b)
        };
        let is_float = either_float;
        let builder = &self.emitter.builder;
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
                .build_int_compare(
                    IntPredicate::EQ,
                    a.into_int_value(),
                    b.into_int_value(),
                    "eq",
                )
                .map_err(map_builder_err)?
                .into(),
            (BinOp::NotEq, false) => builder
                .build_int_compare(
                    IntPredicate::NE,
                    a.into_int_value(),
                    b.into_int_value(),
                    "ne",
                )
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
                // ADR-0070 §X.3 sibling-fix (2026-05-26): use UNE
                // (unordered not-equal) so that `nan != nan` evaluates
                // to `true` per IEEE 754 + Python `==` parity. Matches
                // Cranelift's `FloatCC::NotEqual` which means "UN OR
                // a != b" (see cranelift-codegen docs). The previous
                // `ONE` (ordered not-equal) returned false on NaN
                // operands which broke `f64e16_nan_not_equal_to_itself`
                // under the X.3 LLVM-default flip.
                .build_float_compare(
                    FloatPredicate::UNE,
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
                    note: "matrix multiplication requires cobrust-coil runtime (ADR-0041 §H3, ADR-0071 cobra-rebrand: numpy → coil)",
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
        let builder = &self.emitter.builder;
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
        kind: &AggregateKind,
        operands: &[Operand],
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        // F53 resolution (2026-05-26): the wave-1 stub returned
        // `const_null` for every aggregate kind, which silently broke
        // 36 workspace integration tests once the LLVM backend became
        // reachable through `cobrust build` driver paths (cf.
        // `docs/agent/findings/f53-llvm-default-flip-aggregate-gap.md`).
        //
        // This sprint lands `List` + `FormatString` lowering only;
        // the remaining aggregate kinds (`Dict` / `Set` / `Tuple` /
        // `Record` / `Adt`) keep the wave-1 stub posture pending a
        // follow-up sprint (F53 §3 prerequisite #3). Mirrors
        // Cranelift's `lower_aggregate_list` (cranelift_backend.rs:1674)
        // + `lower_aggregate_format_string` (cranelift_backend.rs:1882).
        match kind {
            AggregateKind::List => self.lower_aggregate_list(operands),
            AggregateKind::FormatString => self.lower_aggregate_format_string(operands),
            // Wave-1 stub fallthrough for kinds not in F53 §3 scope.
            // `Dict` / `Set` / `Tuple` / `Record` / `Adt(_, _)` keep
            // returning `null` until their dedicated sprints land
            // (still need the `__cobrust_dict_new` / `__cobrust_set_new`
            // / `__cobrust_tuple_new` typed-shim dispatch tables —
            // Cranelift's `lower_aggregate_dict_typed` is the reference).
            AggregateKind::Tuple
            | AggregateKind::Dict
            | AggregateKind::Set
            | AggregateKind::Record
            | AggregateKind::Adt(_, _) => Ok(self.emitter.opaque_ptr_ty.const_null().into()),
        }
    }

    /// LLVM mirror of `cranelift_backend::lower_aggregate_list`
    /// (cranelift_backend.rs:1674-1739).
    ///
    /// Lowering pattern:
    ///   1. `__cobrust_list_new(elem_size=8, len)` → buffer ptr.
    ///   2. For each operand, materialise to an i64-encoded value
    ///      (str literal → fresh str buffer; Str-typed local →
    ///      `__cobrust_str_clone`; other → direct lower_operand).
    ///   3. `__cobrust_list_set(buf, idx, val)` to populate the slot.
    ///
    /// Returns the buffer pointer as a `BasicValueEnum` (PointerValue
    /// for the success path, opaque-ptr null when the runtime helpers
    /// are not declared — defensive parity with the str-buf path).
    fn lower_aggregate_list(
        &mut self,
        operands: &[Operand],
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let i64_ty = self.emitter.ctx.i64_type();
        let elem_size = i64_ty.const_int(8, false);
        let len_val = i64_ty.const_int(operands.len() as u64, false);
        // 1. Allocate list buffer.
        let Some(list_new) = self
            .emitter
            .runtime_helper_decls
            .get("__cobrust_list_new")
            .copied()
        else {
            return Ok(self.emitter.opaque_ptr_ty.const_null().into());
        };
        let alloc_args: [BasicMetadataValueEnum<'ctx>; 2] = [elem_size.into(), len_val.into()];
        let alloc_call = self
            .emitter
            .builder
            .build_call(list_new, &alloc_args, "list_new")
            .map_err(map_builder_err)?;
        let buf = alloc_call
            .try_as_basic_value()
            .basic()
            .unwrap_or_else(|| self.emitter.opaque_ptr_ty.const_null().into());
        // 2 + 3. Populate slots via `__cobrust_list_set`.
        if let Some(list_set) = self
            .emitter
            .runtime_helper_decls
            .get("__cobrust_list_set")
            .copied()
        {
            for (idx, op) in operands.iter().enumerate() {
                // Three materialisation cases — verbatim mirror of
                // Cranelift's `lower_aggregate_list` (cf. ADR-0050c
                // Phase 2 + Phase 4 — TD-1 closure):
                //
                //   1. `Constant::Str(payload)` literal → fresh heap
                //      str-buffer via `materialize_str_buffer`.
                //   2. Non-literal Str-typed operand (`Move(p)` or
                //      `Copy(p)` where local p: Ty::Str) → clone the
                //      pointer via `__cobrust_str_clone` so the slot
                //      owns a fresh copy and the source local stays
                //      valid for any subsequent uses (including more
                //      list slots in the same literal — `[s, s, s]`
                //      becomes three independent allocations).
                //   3. Anything else (Int / Bool / nested list pointer)
                //      → `lower_operand` direct (i64 by value).
                let val_raw: BasicValueEnum<'ctx> =
                    if let Operand::Constant(Constant::Str(payload)) = op {
                        self.materialize_str_buffer(payload)?
                    } else {
                        let is_str_operand = match op {
                            Operand::Copy(p) | Operand::Move(p) => self
                                .body
                                .locals
                                .get(p.local.0 as usize)
                                .map(|l| matches!(l.ty, Ty::Str))
                                .unwrap_or(false),
                            Operand::Constant(_) => false,
                        };
                        if is_str_operand {
                            let raw = self.lower_operand(op)?;
                            if let Some(clone_fr) = self
                                .emitter
                                .runtime_helper_decls
                                .get("__cobrust_str_clone")
                                .copied()
                            {
                                let clone_args: [BasicMetadataValueEnum<'ctx>; 1] = [raw.into()];
                                let clone_call = self
                                    .emitter
                                    .builder
                                    .build_call(clone_fr, &clone_args, "str_clone_for_list")
                                    .map_err(map_builder_err)?;
                                clone_call.try_as_basic_value().basic().unwrap_or(raw)
                            } else {
                                raw
                            }
                        } else {
                            self.lower_operand(op)?
                        }
                    };
                // Coerce to i64 for the C-ABI third arg of
                // `__cobrust_list_set(buf, i, v)`. Mirrors Cranelift's
                // `coerce_to_i64` (cranelift_backend.rs:3031-3050).
                let val_i64 = self.coerce_value_to_i64(val_raw)?;
                let idx_val = i64_ty.const_int(idx as u64, false);
                let set_args: [BasicMetadataValueEnum<'ctx>; 3] =
                    [buf.into(), idx_val.into(), val_i64.into()];
                self.emitter
                    .builder
                    .build_call(list_set, &set_args, "list_set")
                    .map_err(map_builder_err)?;
            }
        }
        Ok(buf)
    }

    /// LLVM mirror of `cranelift_backend::lower_aggregate_format_string`
    /// (cranelift_backend.rs:1882-2020).
    ///
    /// f-string lowering: allocate a fresh `__cobrust_str_new()` buffer
    /// then walk operands. Static `Constant::Str` segments map to
    /// `__cobrust_str_push_static`. Non-static "holes" dispatch per the
    /// operand's MIR-declared type (Str → `fmt_str` via str_ptr/str_len;
    /// Float → `fmt_float` or `fmt_float_prec` when followed by an
    /// `FMTSPEC:...` sentinel; Bool/Int → `fmt_bool`/`fmt_int`; else
    /// `fmt_repr`). Returns the buffer pointer.
    fn lower_aggregate_format_string(
        &mut self,
        operands: &[Operand],
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let i64_ty = self.emitter.ctx.i64_type();
        // Allocate buffer via __cobrust_str_new().
        let Some(str_new) = self
            .emitter
            .runtime_helper_decls
            .get("__cobrust_str_new")
            .copied()
        else {
            return Ok(self.emitter.opaque_ptr_ty.const_null().into());
        };
        let new_call = self
            .emitter
            .builder
            .build_call(str_new, &[], "fstr_new")
            .map_err(map_builder_err)?;
        let buf = new_call
            .try_as_basic_value()
            .basic()
            .unwrap_or_else(|| self.emitter.opaque_ptr_ty.const_null().into());

        let mut idx = 0;
        while idx < operands.len() {
            let op = &operands[idx];
            // Static string literal? Push via __cobrust_str_push_static.
            // Skip stray FMTSPEC: sentinels (the preceding float
            // operand's handler consumed it via idx += 2).
            if let Operand::Constant(Constant::Str(payload)) = op {
                if payload.starts_with("FMTSPEC:") {
                    idx += 1;
                    continue;
                }
                if !payload.is_empty() {
                    let (ptr_v, len_v) = self.materialize_str_data(payload)?;
                    if let Some(push_fr) = self
                        .emitter
                        .runtime_helper_decls
                        .get("__cobrust_str_push_static")
                        .copied()
                    {
                        let push_args: [BasicMetadataValueEnum<'ctx>; 3] =
                            [buf.into(), ptr_v.into(), len_v.into()];
                        self.emitter
                            .builder
                            .build_call(push_fr, &push_args, "fstr_push_lit")
                            .map_err(map_builder_err)?;
                    }
                }
                idx += 1;
                continue;
            }
            // Hole — codegen the value + dispatch by MIR-declared type
            // when possible, falling back to the LLVM value type for
            // anonymous operands. ADR-0050c Phase 2 cascade fix mirror:
            // inspect `op`'s MIR-declared Ty FIRST so Str-typed locals
            // (which arrive as opaque-ptr / i64 stack slots) dispatch
            // to `__cobrust_fmt_str` via str_ptr / str_len rather than
            // through `__cobrust_fmt_int` of the raw pointer.
            let mir_ty: Option<Ty> = match op {
                Operand::Copy(p) | Operand::Move(p) => self
                    .body
                    .locals
                    .get(p.local.0 as usize)
                    .map(|l| l.ty.clone()),
                Operand::Constant(_) => None,
            };
            let is_str = matches!(mir_ty, Some(Ty::Str));
            let v = self.lower_operand(op)?;
            // Trailing FMTSPEC: sentinel for float precision specs.
            let maybe_spec: Option<String> = operands.get(idx + 1).and_then(|next| {
                if let Operand::Constant(Constant::Str(s)) = next {
                    s.strip_prefix("FMTSPEC:").map(|s| s.to_string())
                } else {
                    None
                }
            });

            if is_str {
                // `__cobrust_fmt_str(buf, ptr, len)` expects raw bytes;
                // extract (ptr, len) from the StringBuffer via the
                // accessor helpers. f-string precision spec doesn't
                // apply to str holes — only floats.
                let str_ptr_fr = self
                    .emitter
                    .runtime_helper_decls
                    .get("__cobrust_str_ptr")
                    .copied();
                let str_len_fr = self
                    .emitter
                    .runtime_helper_decls
                    .get("__cobrust_str_len")
                    .copied();
                let fmt_str_fr = self
                    .emitter
                    .runtime_helper_decls
                    .get("__cobrust_fmt_str")
                    .copied();
                if let (Some(ptr_fr), Some(len_fr), Some(fmt_fr)) =
                    (str_ptr_fr, str_len_fr, fmt_str_fr)
                {
                    // Coerce IntValue → PointerValue for the str-buffer
                    // arg if MIR encoded the slot as i64. Mirrors
                    // `lower_call`'s extern-call int→ptr coercion at
                    // line ~3088.
                    let v_ptr_arg = self.coerce_value_to_ptr(v)?;
                    let ptr_call = self
                        .emitter
                        .builder
                        .build_call(ptr_fr, &[v_ptr_arg.into()], "fstr_str_ptr")
                        .map_err(map_builder_err)?;
                    let ptr_v_acc = ptr_call
                        .try_as_basic_value()
                        .basic()
                        .unwrap_or_else(|| self.emitter.opaque_ptr_ty.const_null().into());
                    let len_call = self
                        .emitter
                        .builder
                        .build_call(len_fr, &[v_ptr_arg.into()], "fstr_str_len")
                        .map_err(map_builder_err)?;
                    let len_v_acc = len_call
                        .try_as_basic_value()
                        .basic()
                        .unwrap_or_else(|| i64_ty.const_zero().into());
                    let fmt_args: [BasicMetadataValueEnum<'ctx>; 3] =
                        [buf.into(), ptr_v_acc.into(), len_v_acc.into()];
                    self.emitter
                        .builder
                        .build_call(fmt_fr, &fmt_args, "fstr_fmt_str")
                        .map_err(map_builder_err)?;
                }
                idx += 1;
            } else if v.is_float_value() {
                let v_f64 = v.into_float_value();
                if let Some(spec) = maybe_spec {
                    // M-F.3.3 gap (c): route to the precision formatter.
                    if let Some(fr) = self
                        .emitter
                        .runtime_helper_decls
                        .get("__cobrust_fmt_float_prec")
                        .copied()
                    {
                        let (spec_ptr, spec_len) = self.materialize_str_data(&spec)?;
                        let prec_args: [BasicMetadataValueEnum<'ctx>; 4] =
                            [buf.into(), v_f64.into(), spec_ptr.into(), spec_len.into()];
                        self.emitter
                            .builder
                            .build_call(fr, &prec_args, "fstr_fmt_float_prec")
                            .map_err(map_builder_err)?;
                    }
                    idx += 2; // consume value + sentinel
                } else {
                    if let Some(fr) = self
                        .emitter
                        .runtime_helper_decls
                        .get("__cobrust_fmt_float")
                        .copied()
                    {
                        let float_args: [BasicMetadataValueEnum<'ctx>; 2] =
                            [buf.into(), v_f64.into()];
                        self.emitter
                            .builder
                            .build_call(fr, &float_args, "fstr_fmt_float")
                            .map_err(map_builder_err)?;
                    }
                    idx += 1;
                }
            } else if matches!(mir_ty, Some(Ty::Bool))
                || (v.is_int_value() && v.into_int_value().get_type().get_bit_width() == 1)
            {
                // Bool path — widen i1 → i64 for the C ABI.
                let v_int = v.into_int_value();
                let v_i64 = if v_int.get_type().get_bit_width() < 64 {
                    self.emitter
                        .builder
                        .build_int_z_extend(v_int, i64_ty, "fstr_bool_zext")
                        .map_err(map_builder_err)?
                } else {
                    v_int
                };
                if let Some(fr) = self
                    .emitter
                    .runtime_helper_decls
                    .get("__cobrust_fmt_bool")
                    .copied()
                {
                    let bool_args: [BasicMetadataValueEnum<'ctx>; 2] = [buf.into(), v_i64.into()];
                    self.emitter
                        .builder
                        .build_call(fr, &bool_args, "fstr_fmt_bool")
                        .map_err(map_builder_err)?;
                }
                idx += 1;
            } else if v.is_int_value() {
                // Int / unknown-i64 path.
                let v_int = v.into_int_value();
                let v_i64 = if v_int.get_type().get_bit_width() < 64 {
                    self.emitter
                        .builder
                        .build_int_s_extend(v_int, i64_ty, "fstr_int_sext")
                        .map_err(map_builder_err)?
                } else {
                    v_int
                };
                if let Some(fr) = self
                    .emitter
                    .runtime_helper_decls
                    .get("__cobrust_fmt_int")
                    .copied()
                {
                    let int_args: [BasicMetadataValueEnum<'ctx>; 2] = [buf.into(), v_i64.into()];
                    self.emitter
                        .builder
                        .build_call(fr, &int_args, "fstr_fmt_int")
                        .map_err(map_builder_err)?;
                }
                idx += 1;
            } else {
                // Pointer-typed value — assume List/Dict/Set repr.
                if let Some(fr) = self
                    .emitter
                    .runtime_helper_decls
                    .get("__cobrust_fmt_repr")
                    .copied()
                {
                    let v_ptr_arg = self.coerce_value_to_ptr(v)?;
                    let type_id = i64_ty.const_zero();
                    let repr_args: [BasicMetadataValueEnum<'ctx>; 3] =
                        [buf.into(), v_ptr_arg.into(), type_id.into()];
                    self.emitter
                        .builder
                        .build_call(fr, &repr_args, "fstr_fmt_repr")
                        .map_err(map_builder_err)?;
                }
                idx += 1;
            }
        }
        Ok(buf)
    }

    /// Coerce a `BasicValueEnum` to `i64` for runtime-helper i64 args.
    /// Mirrors Cranelift `coerce_to_i64` (cranelift_backend.rs:3031).
    ///
    /// - IntValue 64-bit → unchanged.
    /// - IntValue < 64 bits → s_extend to i64.
    /// - FloatValue f32 → fpromote to f64 then bitcast.
    /// - FloatValue f64 → bitcast i64.
    /// - PointerValue → ptr_to_int via i64.
    /// - Other → defensive fall through to i64 zero.
    fn coerce_value_to_i64(
        &mut self,
        v: BasicValueEnum<'ctx>,
    ) -> Result<IntValue<'ctx>, CodegenError> {
        let i64_ty = self.emitter.ctx.i64_type();
        let f64_ty = self.emitter.ctx.f64_type();
        match v {
            BasicValueEnum::IntValue(iv) => {
                let w = iv.get_type().get_bit_width();
                if w == 64 {
                    Ok(iv)
                } else if w < 64 {
                    self.emitter
                        .builder
                        .build_int_s_extend(iv, i64_ty, "agg_sext_i64")
                        .map_err(map_builder_err)
                } else {
                    self.emitter
                        .builder
                        .build_int_truncate(iv, i64_ty, "agg_trunc_i64")
                        .map_err(map_builder_err)
                }
            }
            BasicValueEnum::FloatValue(fv) => {
                let f_promoted = if fv.get_type() == self.emitter.ctx.f32_type() {
                    self.emitter
                        .builder
                        .build_float_ext(fv, f64_ty, "agg_fpromote")
                        .map_err(map_builder_err)?
                } else {
                    fv
                };
                self.emitter
                    .builder
                    .build_bit_cast(f_promoted, i64_ty, "agg_f2i_bitcast")
                    .map_err(map_builder_err)
                    .map(|bv| bv.into_int_value())
            }
            BasicValueEnum::PointerValue(pv) => self
                .emitter
                .builder
                .build_ptr_to_int(pv, i64_ty, "agg_p2i")
                .map_err(map_builder_err),
            _ => Ok(i64_ty.const_zero()),
        }
    }

    /// Coerce a `BasicValueEnum` to `*ptr` (opaque ptr) for runtime
    /// helpers whose C signature is `*StringBuffer` / `*ListBuffer` /
    /// etc. MIR encodes these as i64 stack-slot pointers; the LLVM
    /// verifier rejects an i64 arg into a `ptr` param, so emit
    /// `inttoptr` defensively. Mirror of `lower_call`'s int→ptr coercion
    /// at line ~3088.
    fn coerce_value_to_ptr(
        &mut self,
        v: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        match v {
            BasicValueEnum::IntValue(iv) => {
                let ptr_v = self
                    .emitter
                    .builder
                    .build_int_to_ptr(iv, self.emitter.opaque_ptr_ty, "agg_i2p")
                    .map_err(map_builder_err)?;
                Ok(ptr_v.into())
            }
            BasicValueEnum::PointerValue(_) => Ok(v),
            _ => Ok(self.emitter.opaque_ptr_ty.const_null().into()),
        }
    }

    fn lower_cast(
        &mut self,
        kind: CastKind,
        op: &Operand,
        _target_ty: &Ty,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let v = self.lower_operand(op)?;
        let builder = &self.emitter.builder;
        let ctx = self.emitter.ctx;
        // ADR-0070 §X.3 sibling-fix (2026-05-26): MIR represents the
        // result of every `Rvalue::BinaryOp` as `Ty::None`-typed local
        // (`_bin` in `cobrust-mir/src/lower.rs:1945`), which maps to
        // LLVM `i64` in `lower_ty`. When the source expression of a
        // `FloatToInt` or `IntToBool` cast traces back through a
        // float-typed binop, the loaded operand is an `IntValue`
        // (i64 stack-slot) holding a float bit-pattern. Mirror
        // Cranelift's defensive fall-through (see
        // `cranelift_backend.rs:lower_cast` 2023-2055) — when the
        // direction of the cast disagrees with the value's LLVM type,
        // re-interpret via `bitcast` rather than panicking with
        // `into_float_value()` / `into_int_value()`.
        let v_was_int = v.is_int_value();
        let v_was_float = v.is_float_value();
        let val: BasicValueEnum<'ctx> = match kind {
            CastKind::IntToFloat => {
                if v_was_float {
                    // Already a float; defensive identity.
                    v
                } else {
                    builder
                        .build_signed_int_to_float(v.into_int_value(), ctx.f64_type(), "i2f")
                        .map_err(map_builder_err)?
                        .into()
                }
            }
            CastKind::FloatToInt => {
                if v_was_int {
                    // Operand is already i64 (binop-result encoded as
                    // Ty::None → i64 per the MIR shape above). Cobrust
                    // float types are 64-bit (`Ty::Float` → `f64`);
                    // bitcast the i64 stack-slot to f64 first, then
                    // emit the proper FloatToInt conversion. Without
                    // this, the cast becomes a silent no-op which is
                    // observably wrong for non-integral float values.
                    let iv = v.into_int_value();
                    let fv = builder
                        .build_bit_cast(iv, ctx.f64_type(), "f2i_reinterp")
                        .map_err(map_builder_err)?;
                    builder
                        .build_float_to_signed_int(fv.into_float_value(), ctx.i64_type(), "f2i")
                        .map_err(map_builder_err)?
                        .into()
                } else {
                    builder
                        .build_float_to_signed_int(v.into_float_value(), ctx.i64_type(), "f2i")
                        .map_err(map_builder_err)?
                        .into()
                }
            }
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
        let (alloca, _) = self.local_alloca(place.local)?;
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

/// ADR-0058f §3.2 — collect every `Constant::Str` payload referenced
/// in an Rvalue's operand tree. Pure traversal (no IR emission); used
/// by `LlvmEmitter::intern_str_payloads` to enumerate the rodata
/// surface during the module-level interning pre-pass.
///
/// Mirrors `cranelift_backend::collect_str_payloads_from_rvalue`
/// (`cranelift_backend.rs:2504-2534`) but with an infallible visitor
/// shape (LLVM-side interning doesn't use the Cranelift Result chain).
fn collect_str_payloads_from_rvalue<F>(rvalue: &Rvalue, visit: &mut F)
where
    F: FnMut(&str),
{
    let visit_operand = |op: &Operand, visit: &mut F| {
        if let Operand::Constant(Constant::Str(payload)) = op {
            visit(payload);
        }
    };
    match rvalue {
        Rvalue::Use(op) | Rvalue::Cast(_, op, _) | Rvalue::UnaryOp(_, op) => {
            visit_operand(op, visit);
        }
        Rvalue::BinaryOp(_, a, b) => {
            visit_operand(a, visit);
            visit_operand(b, visit);
        }
        Rvalue::Aggregate(_, ops) => {
            for op in ops {
                visit_operand(op, visit);
            }
        }
        Rvalue::Ref(_, _) | Rvalue::Discriminant(_) | Rvalue::Len(_) | Rvalue::NullaryOp(_) => {}
    }
}

// =====================================================================
// ADR-0058c §3.3 — LineMap (Span byte-offset → 1-indexed line, column)
// =====================================================================
//
// Inlined here to avoid a `cobrust-lsp` dependency. The algorithm is
// the same as `cobrust-lsp::span_convert::LineMap` (which produces
// UTF-16 columns for LSP); this variant emits **1-indexed** lines +
// columns measured in UTF-8 codepoints, matching DWARF's `DW_LNS_*` line
// table conventions per DWARF v5 §6.2.

/// Byte-offset → (line, column) lookup over a source string.
///
/// `line_starts[i]` is the byte offset of the first character on line
/// `i` (0-indexed internally; DWARF emission adds +1 at lookup time so
/// debuggers see DWARF-conventional 1-indexed lines).
///
/// Empty when constructed via `LineMap::empty()` — every lookup returns
/// `(1, 1)` so the DWARF DIE structure validates but breakpoint
/// resolution collapses to "the first line". Tests + synthetic modules
/// use this path.
#[derive(Clone, Debug, Default)]
struct LineMap {
    line_starts: Vec<u32>,
    source: String,
}

impl LineMap {
    /// Empty `LineMap` — `(1, 1)` for every lookup.
    fn empty() -> Self {
        Self::default()
    }

    /// Build a `LineMap` from a source string.
    fn from_source(source: &str) -> Self {
        let mut line_starts: Vec<u32> = vec![0];
        let bytes = source.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                let next = u32::try_from(i + 1).unwrap_or(u32::MAX);
                line_starts.push(next);
            }
        }
        Self {
            line_starts,
            source: source.to_string(),
        }
    }

    /// Lookup a 1-indexed (line, column) pair for a byte offset.
    ///
    /// Returns `(1, 1)` on empty maps (synthetic-source fallback).
    fn line_column(&self, byte_offset: u32) -> (u32, u32) {
        if self.line_starts.is_empty() {
            return (1, 1);
        }
        let line_idx = match self.line_starts.binary_search(&byte_offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts.get(line_idx).copied().unwrap_or(0);
        let line_byte_off = byte_offset.saturating_sub(line_start) as usize;
        let line_start_us = line_start as usize;
        let bytes = self.source.as_bytes();
        let safe_end = bytes.len().min(line_start_us + line_byte_off);
        let safe_end = clamp_to_char_boundary(&self.source, safe_end);
        let prefix = &self.source[line_start_us..safe_end];
        // Column = codepoint count (DWARF column convention is bytes or
        // codepoints depending on the producer; LLVM emits codepoint
        // counts on most consumers — we follow that convention).
        let col = u32::try_from(prefix.chars().count()).unwrap_or(u32::MAX);
        // DWARF expects 1-indexed lines + columns.
        (u32::try_from(line_idx).unwrap_or(u32::MAX) + 1, col + 1)
    }
}

/// Round `byte_offset` down to the nearest `char` boundary in `source`.
fn clamp_to_char_boundary(source: &str, byte_offset: usize) -> usize {
    let mut off = byte_offset.min(source.len());
    while off > 0 && !source.is_char_boundary(off) {
        off -= 1;
    }
    off
}

/// Resolve DWARF source filename + directory + `LineMap` from a
/// `TargetSpec`. When `source_path` is `Some` and the file is readable,
/// returns `(basename, dirname, LineMap::from_source(file_contents))`.
/// Otherwise falls back to `(spec.module_name, ".", LineMap::empty())`.
fn resolve_source_paths(spec: &TargetSpec) -> (String, String, LineMap) {
    if let Some(src_path) = &spec.source_path {
        if let Ok(source) = std::fs::read_to_string(src_path) {
            let line_map = LineMap::from_source(&source);
            let filename = path_filename(src_path).unwrap_or_else(|| spec.module_name.clone());
            let directory = path_directory(src_path).unwrap_or_else(|| ".".to_string());
            return (filename, directory, line_map);
        }
        // Fall through to synthetic fallback when the file isn't readable
        // (path may be transient; DI structure validates either way).
    }
    (
        format!("{}.cb", spec.module_name),
        ".".to_string(),
        LineMap::empty(),
    )
}

fn path_filename(p: &Path) -> Option<String> {
    p.file_name().and_then(|s| s.to_str()).map(String::from)
}

fn path_directory(p: &Path) -> Option<String> {
    p.parent().and_then(|s| s.to_str()).map(|s| {
        if s.is_empty() {
            ".".to_string()
        } else {
            s.to_string()
        }
    })
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
        let tmp = tempfile::tempdir()
            .expect("invariant: tempdir creation must succeed in test environment");
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
            source_path: None,
            runtime_dispatch: false,
            target_cpu: None,
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
            validated_body_of: None,
        }];
        for (i, ty) in params.iter().enumerate() {
            locals.push(LocalDecl {
                id: LocalId((i + 1) as u32),
                name: format!("p{i}"),
                ty: ty.clone(),
                mutable: false,
                span: span0(),
                validated_body_of: None,
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
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let module = Module { bodies: vec![] };
        let spec = host_spec();
        let result = emit(&module, &spec);
        assert!(
            result.is_ok(),
            "empty module emit failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn smoke_return_42() {
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
                validated_body_of: None,
            },
            LocalDecl {
                id: LocalId(1),
                name: "s".to_string(),
                ty: Ty::Str,
                mutable: false,
                span: span0(),
                validated_body_of: None,
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

    // =================================================================
    // ADR-0058b §3.2 + §3.4 — opt pipeline + multi-target dispatch
    // =================================================================

    /// ADR-0058b §3.2: OptLevel → PassBuilder pipeline string mapping.
    #[test]
    fn pass_pipeline_mapping_matches_spec() {
        assert!(pass_pipeline_for(OptLevel::None).is_none());
        assert_eq!(pass_pipeline_for(OptLevel::Speed), Some("default<O2>"));
        assert_eq!(
            pass_pipeline_for(OptLevel::SpeedAndSize),
            Some("default<O3>,default<Os>")
        );
    }

    /// ADR-0058b §3.4: four tier-1 triples are enumerated in the binding
    /// contract.
    #[test]
    fn tier1_triple_matrix_has_four_entries() {
        let triples = supported_tier1_triples();
        assert_eq!(triples.len(), 4, "tier-1 matrix is ADR-0046 + Strand #5");
        assert!(triples.contains(&"aarch64-apple-darwin"));
        assert!(triples.contains(&"aarch64-unknown-linux-gnu"));
        assert!(triples.contains(&"x86_64-unknown-linux-gnu"));
        assert!(triples.contains(&"x86_64-unknown-linux-musl"));
    }

    /// ADR-0058b §3.4: every tier-1 triple can be parsed by `target-lexicon`
    /// and round-trips through `Triple::host()`-style construction. This
    /// does NOT require backend availability — `Target::from_triple` is
    /// guarded behind the runtime LLVM-18 backend presence and is exercised
    /// at object emission time by the diff corpus.
    #[test]
    fn tier1_triples_parse_via_target_lexicon() {
        use std::str::FromStr;
        for triple_str in supported_tier1_triples() {
            let parsed = target_lexicon::Triple::from_str(triple_str)
                .unwrap_or_else(|e| panic!("triple `{triple_str}` failed to parse: {e}"));
            assert_eq!(parsed.to_string(), *triple_str);
        }
    }

    /// F66 — RISC-V triple normalization: `riscv64gc-...` → `riscv64-...`
    /// with ISA features synthesised into the feature string. Tier-1
    /// triples + non-RISCV cross triples pass through unchanged.
    #[test]
    fn normalize_triple_for_llvm_riscv_and_passthrough() {
        use std::str::FromStr;
        let cases: &[(&str, &str, &str)] = &[
            // (input triple, expected LLVM triple, expected features)
            (
                "riscv64gc-unknown-linux-gnu",
                "riscv64-unknown-linux-gnu",
                "+m,+a,+f,+d,+c",
            ),
            ("riscv64-unknown-linux-gnu", "riscv64-unknown-linux-gnu", ""),
            (
                "riscv64imac-unknown-none-elf",
                "riscv64-unknown-none-elf",
                "+m,+a,+c",
            ),
            (
                "riscv32gc-unknown-linux-gnu",
                "riscv32-unknown-linux-gnu",
                "+m,+a,+f,+d,+c",
            ),
            (
                "riscv32imc-unknown-none-elf",
                "riscv32-unknown-none-elf",
                "+m,+c",
            ),
            // Non-RISCV pass-through (no features synthesised).
            ("x86_64-unknown-linux-gnu", "x86_64-unknown-linux-gnu", ""),
            ("aarch64-apple-darwin", "aarch64-apple-darwin", ""),
            ("wasm32-wasip1", "wasm32-wasip1", ""),
        ];
        for (input, want_triple, want_features) in cases {
            let parsed = target_lexicon::Triple::from_str(input)
                .unwrap_or_else(|e| panic!("triple `{input}` failed to parse: {e}"));
            let (got_triple, got_features) = normalize_triple_for_llvm(&parsed);
            assert_eq!(&got_triple, want_triple, "triple mismatch for `{input}`");
            assert_eq!(
                &got_features, want_features,
                "features mismatch for `{input}`"
            );
        }
    }

    /// ADR-0058b §3.2: `OptLevel::Speed` emit runs without error on a small
    /// fixture. This validates that `default<O2>` pipeline is accepted by
    /// inkwell's `run_passes`.
    #[test]
    fn smoke_opt_speed_pipeline() {
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let body = build_simple_body(
            10,
            "opt_speed",
            vec![],
            Ty::Int,
            Rvalue::Use(Operand::Constant(MirConstant::Int(7))),
        );
        let module = Module { bodies: vec![body] };
        let mut spec = host_spec();
        spec.opt_level = OptLevel::Speed;
        spec.module_name = "opt_speed".to_string();
        let result = emit(&module, &spec);
        assert!(
            result.is_ok(),
            "OptLevel::Speed emit failed: {:?}",
            result.err()
        );
    }

    /// ADR-0058b §3.2 + §A3: `OptLevel::SpeedAndSize` runs the size-overlay
    /// pipeline.
    #[test]
    fn smoke_opt_speed_and_size_pipeline() {
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let body = build_simple_body(
            11,
            "opt_sized",
            vec![],
            Ty::Int,
            Rvalue::Use(Operand::Constant(MirConstant::Int(11))),
        );
        let module = Module { bodies: vec![body] };
        let mut spec = host_spec();
        spec.opt_level = OptLevel::SpeedAndSize;
        spec.module_name = "opt_sized".to_string();
        let result = emit(&module, &spec);
        assert!(
            result.is_ok(),
            "OptLevel::SpeedAndSize emit failed: {:?}",
            result.err()
        );
    }

    // =================================================================
    // ADR-0058c §3.4 — DWARF emission smoke tests
    //
    // Each test compiles a small fixture + asserts the emitted object
    // file contains at least one DWARF section. The acceptance gate
    // (§6) verifies via `llvm-dwarfdump-18` externally; here we use
    // the `object` crate to inspect the section table directly so the
    // smoke tests run with `cargo test` alone.
    // =================================================================

    use object::{Object, ObjectSection};
    use std::fs;

    /// Predicate: object file contains at least one DWARF section.
    ///
    /// ELF on Linux: `.debug_info` / `.debug_line` / etc.
    /// Mach-O on macOS: `__debug_info` / `__debug_line` (note Mach-O
    /// strips the leading dot and prepends `__`).
    ///
    /// We accept any section whose name contains `debug_info`,
    /// `debug_line`, or `debug_abbrev` — those are the three core
    /// DWARF v5 sections LLVM emits per non-empty CU.
    fn object_has_dwarf_sections(path: &std::path::Path) -> bool {
        let data = fs::read(path).expect("read emitted object");
        let obj = object::File::parse(&*data).expect("parse emitted object");
        obj.sections().any(|s| {
            let name = s.name().unwrap_or("");
            name.contains("debug_info")
                || name.contains("debug_line")
                || name.contains("debug_abbrev")
        })
    }

    #[test]
    fn dwarf_empty_module_emits_well_formed_object() {
        // Empty modules emit a `DW_TAG_compile_unit` placeholder in
        // the LLVM IR but LLVM-18's object backend elides the
        // resulting `.debug_*` sections when no symbols reference them
        // (an empty CU has nothing to anchor in `.debug_info`). The
        // contract is "emit() must not panic on an empty module"; the
        // DWARF-content gate is enforced by the non-empty fixtures
        // below.
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let module = Module { bodies: vec![] };
        let mut spec = host_spec();
        spec.module_name = "dwarf_empty".to_string();
        let result = emit(&module, &spec).expect("empty module emit");
        let Artifact::Object(path) = result else {
            panic!("expected Artifact::Object")
        };
        // Object file must exist + parse as an object.
        let bytes = std::fs::read(&path).expect("read object");
        let _ = object::File::parse(&*bytes).expect("parse object");
    }

    #[test]
    fn dwarf_return_42_emits_debug_sections() {
        // `fn answer() -> i64 { return 42 }` — DI emits a
        // DW_TAG_subprogram for the function. We assert the object
        // file is well-formed + carries DWARF sections
        // (section-presence check; subprogram symbol-level check is in
        // dwarf_lldb_smoke.rs::lldb_smoke_hello_world_subprogram_resolves).
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let body = build_simple_body(
            1,
            "dwarf_answer",
            vec![],
            Ty::Int,
            Rvalue::Use(Operand::Constant(MirConstant::Int(42))),
        );
        let module = Module { bodies: vec![body] };
        let mut spec = host_spec();
        spec.module_name = "dwarf_return_42".to_string();
        let result = emit(&module, &spec).expect("return 42 emit");
        let Artifact::Object(path) = result else {
            panic!("expected Artifact::Object")
        };
        assert!(
            object_has_dwarf_sections(&path),
            "return-42 fixture: missing .debug_* sections"
        );
    }

    #[test]
    fn dwarf_multi_fn_module_emits_debug_sections() {
        // Two unrelated user fns share the compile unit; both get
        // their own DISubprogram per §3.2.
        // (Section-presence check; per-fn symbol check in lldb smoke suite.)
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let body_a = build_simple_body(
            10,
            "alpha",
            vec![],
            Ty::Int,
            Rvalue::Use(Operand::Constant(MirConstant::Int(1))),
        );
        let body_b = build_simple_body(
            11,
            "beta",
            vec![],
            Ty::Int,
            Rvalue::Use(Operand::Constant(MirConstant::Int(2))),
        );
        let module = Module {
            bodies: vec![body_a, body_b],
        };
        let mut spec = host_spec();
        spec.module_name = "dwarf_multi_fn".to_string();
        let result = emit(&module, &spec).expect("multi-fn emit");
        let Artifact::Object(path) = result else {
            panic!("expected Artifact::Object")
        };
        assert!(
            object_has_dwarf_sections(&path),
            "multi-fn fixture: missing .debug_* sections"
        );
    }

    #[test]
    fn dwarf_drop_emitting_fn_still_validates() {
        // A function that lowers a `Drop` terminator still emits
        // well-formed DWARF (the Drop helper call gets a debug
        // location like every other instruction).
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let locals = vec![
            LocalDecl {
                id: LocalId(0),
                name: "_return".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0(),
                validated_body_of: None,
            },
            LocalDecl {
                id: LocalId(1),
                name: "s".to_string(),
                ty: Ty::Str,
                mutable: false,
                span: span0(),
                validated_body_of: None,
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
            def_id: DefId(99),
            name: "dwarf_drop_str".to_string(),
            locals,
            blocks: vec![block0, block1],
            return_local: LocalId(0),
            param_count: 1,
            span: span0(),
        };
        let module = Module { bodies: vec![body] };
        let mut spec = host_spec();
        spec.module_name = "dwarf_drop".to_string();
        let result = emit(&module, &spec).expect("drop fn emit");
        let Artifact::Object(path) = result else {
            panic!("expected Artifact::Object")
        };
        assert!(
            object_has_dwarf_sections(&path),
            "drop-fn fixture: missing .debug_* sections"
        );
    }

    #[test]
    fn dwarf_o3_pipeline_preserves_dwarf() {
        // §A3 follow-on: ensure the `-O3,Os` pipeline doesn't strip
        // DWARF sections. Optimization passes consume but preserve
        // debug-info metadata when emit-time `is_optimized` is true.
        let _guard = LLVM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let body = build_simple_body(
            42,
            "dwarf_opt_fn",
            vec![Ty::Int, Ty::Int],
            Ty::Int,
            Rvalue::BinaryOp(
                MirBinOp::Add,
                Operand::Copy(Place::local(LocalId(1))),
                Operand::Copy(Place::local(LocalId(2))),
            ),
        );
        let module = Module { bodies: vec![body] };
        let mut spec = host_spec();
        spec.opt_level = OptLevel::SpeedAndSize;
        spec.module_name = "dwarf_o3".to_string();
        let result = emit(&module, &spec).expect("O3 emit");
        let Artifact::Object(path) = result else {
            panic!("expected Artifact::Object")
        };
        assert!(
            object_has_dwarf_sections(&path),
            "O3 fixture: DWARF sections were stripped by the opt pipeline"
        );
    }

    // =================================================================
    // ADR-0058c §3.3 — LineMap helper unit tests
    // =================================================================

    #[test]
    fn linemap_empty_returns_1_1() {
        let lm = LineMap::empty();
        assert_eq!(lm.line_column(0), (1, 1));
        assert_eq!(lm.line_column(1234), (1, 1));
    }

    #[test]
    fn linemap_ascii_lines() {
        let lm = LineMap::from_source("ab\ncd\nef");
        assert_eq!(lm.line_column(0), (1, 1));
        assert_eq!(lm.line_column(1), (1, 2));
        assert_eq!(lm.line_column(3), (2, 1));
        assert_eq!(lm.line_column(6), (3, 1));
    }
}
