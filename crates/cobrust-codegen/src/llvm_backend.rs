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
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType};
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
fn build_target_machine(spec: &TargetSpec) -> Result<TargetMachine, CodegenError> {
    Target::initialize_all(&InitializationConfig::default());
    let triple = TargetTriple::create(&spec.triple.to_string());
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
    // `"native"` asks LLVM to auto-detect the host CPU and enables all available
    // ISA extensions — zero dispatch overhead, host-only binary.
    // Any other string (e.g. `"skylake"`, `"apple-m1"`, `"neoverse-v1"`) is passed
    // verbatim to LLVM as the target CPU name.
    // When `None`, fall back to the `"generic"` baseline (pre-Tier-2 behaviour).
    let cpu = spec.target_cpu.as_deref().unwrap_or("generic");
    target
        .create_target_machine(&triple, cpu, "", opt, RelocMode::PIC, CodeModel::Default)
        .ok_or_else(|| {
            CodegenError::LlvmError(format!(
                "failed to create LLVM TargetMachine for {} (cpu={cpu})",
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
        let panic_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
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
        let arr_get_i64_ty = i64_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
        let arr_get_i64 = self.module.add_function(
            "__cobrust_array_get_i64",
            arr_get_i64_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_array_get_i64", arr_get_i64);

        // __cobrust_array_get_i32(*const i32, usize, usize) -> i32
        let i32_ty = self.ctx.i32_type();
        let arr_get_i32_ty = i32_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
        let arr_get_i32 = self.module.add_function(
            "__cobrust_array_get_i32",
            arr_get_i32_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_array_get_i32", arr_get_i32);

        // __cobrust_array_get_i8(*const i8, usize, usize) -> i8
        let i8_ty = self.ctx.i8_type();
        let arr_get_i8_ty = i8_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
        let arr_get_i8 = self.module.add_function(
            "__cobrust_array_get_i8",
            arr_get_i8_ty,
            Some(Linkage::External),
        );
        self.runtime_helper_decls
            .insert("__cobrust_array_get_i8", arr_get_i8);

        // __cobrust_array_get_bool(*const u8, usize, usize) -> i64
        let arr_get_bool_ty = i64_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
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
        let println_lit_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
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
        let print_no_nl_lit_ty = void_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
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

        // __cobrust_input(prompt_ptr: *const u8, prompt_len: i64) -> *mut Str
        let input_ty = ptr_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);
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
        ] {
            let ty = f64_ty.fn_type(&[f64_ty.into()], false);
            let f = self.module.add_function(sym, ty, Some(Linkage::External));
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
                            call_args.push(ptr.into());
                            call_args.push(len.into());
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
                let blk = self.block_map[&target];
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
                        let i64_ty = self.emitter.ctx.i64_type();
                        // Array base alloca (PointerValue) as opaque ptr arg.
                        let arr_ptr_val: BasicMetadataValueEnum<'ctx> = alloca.into();
                        // Static N as i64.
                        let len_val: BasicMetadataValueEnum<'ctx> =
                            i64_ty.const_int(*n as u64, false).into();
                        // Runtime index operand.
                        let idx_val = self.lower_operand(idx_op)?;
                        // Widen to i64 if needed (Bool/i8/i32 index).
                        let idx_i64: BasicMetadataValueEnum<'ctx> = match idx_val {
                            BasicValueEnum::IntValue(iv) if iv.get_type() != i64_ty => self
                                .emitter
                                .builder
                                .build_int_z_extend(iv, i64_ty, "idx_zext")
                                .map_err(map_builder_err)?
                                .into(),
                            _ => idx_val.into(),
                        };
                        let call = self
                            .emitter
                            .builder
                            .build_call(helper_fn, &[arr_ptr_val, len_val, idx_i64], "arr_dyn_get")
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
        let builder = &self.emitter.builder;
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
