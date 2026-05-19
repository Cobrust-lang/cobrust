//! `JitEngine` — owns the `JITModule`, drives compilation.
//!
//! ADR-0056a §"3.3 JIT module construction" sets out the contract:
//! the engine owns `JITModule` by-value, callers receive a
//! [`JitHandle`] that records the compiled function table.
//!
//! ## Lifetime contract
//!
//! Per ADR-0056a §4 the `JITModule` lifetime is tied to the REPL
//! Session. The wave-1 surface assumes one-shot eval: caller
//! creates a `JitEngine`, compiles a `Module`, calls the
//! resulting `JitHandle`, drops both. ADR-0056c will lift this
//! to a per-Session long-lived `JitEngine` with cross-turn
//! function persistence.
//!
//! ## Safety
//!
//! `get_finalized_function` returns `*const u8`. The unsafe
//! transmute lives in `JitHandle::call`, NOT here — the engine
//! never executes user code, it only emits it.
//!
//! Reference: `docs/agent/adr/0056a-cranelift-jit-wire.md`.

use std::collections::HashMap;
use std::sync::Arc;

use cobrust_mir::Module;
use cranelift_codegen::Context;
use cranelift_codegen::ir::Signature;
use cranelift_codegen::isa::{CallConv, OwnedTargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module as ClifModule, default_libcall_names};

use crate::error::JitError;
use crate::handle::JitHandle;
use crate::lower::{body_signature, lower_body};

/// Cranelift-backed JIT engine.
///
/// ## Public surface (stable across wave-1)
///
/// - [`JitEngine::new`] — construct with host ISA detected via
///   `cranelift-native`.
/// - [`JitEngine::compile_mir`] — compile a MIR module; returns
///   a [`JitHandle`] keyed by body name.
///
/// The `JITModule` is consumed into the returned `JitHandle` —
/// the engine is single-use in wave-1. ADR-0056c will widen to
/// reusable.
pub struct JitEngine {
    module: JITModule,
    isa: Arc<dyn cranelift_codegen::isa::TargetIsa>,
    call_conv: CallConv,
}

impl JitEngine {
    /// Construct a `JitEngine` using the host ISA.
    ///
    /// Cold-start cost on a warm cargo cache: <50ms per ADR-0029
    /// budget. The first call inside a process pays ~150KB of
    /// memmap2 pages per ADR-0056 §8.2.
    pub fn new() -> Result<Self, JitError> {
        let isa = host_isa()?;
        let call_conv = isa.default_call_conv();
        let jit_builder = JITBuilder::with_isa(isa.clone(), default_libcall_names());
        let module = JITModule::new(jit_builder);
        Ok(Self {
            module,
            isa,
            call_conv,
        })
    }

    /// Compile a MIR module. Returns a [`JitHandle`] containing
    /// finalized function pointers keyed by body name.
    ///
    /// ## Two-pass design
    ///
    /// Mirrors `cobrust-codegen`'s AOT path:
    ///
    /// 1. **Declare pass:** every body's signature is declared
    ///    via `module.declare_function`. This populates the
    ///    cross-body FuncId map so wave-N (≥0056b) can lower
    ///    `Constant::FnRef(id)` callees against forward-decls
    ///    without re-ordering bodies.
    /// 2. **Define pass:** each body's IR is lowered via
    ///    [`lower_body`] and submitted via
    ///    `module.define_function`.
    /// 3. **Finalize:** one `finalize_definitions` call commits
    ///    all bodies' code to executable JIT pages.
    ///
    /// On any error the partial state inside `self.module` is
    /// undefined; callers should drop the engine and start fresh.
    pub fn compile_mir(mut self, module: &Module) -> Result<JitHandle, JitError> {
        // --- declare pass -------------------------------------------
        // Map: MIR body name → (FuncId, Signature).
        let mut entries: HashMap<String, (FuncId, Signature)> = HashMap::new();
        for body in &module.bodies {
            let sig = body_signature(body, self.call_conv)?;
            let name = body_export_name(body);
            let fid = self
                .module
                .declare_function(&name, Linkage::Export, &sig)
                .map_err(|e| JitError::Module(e.to_string()))?;
            entries.insert(name, (fid, sig));
        }

        // --- define pass --------------------------------------------
        for body in &module.bodies {
            let name = body_export_name(body);
            let (fid, _sig) = entries
                .get(&name)
                .ok_or_else(|| JitError::Internal(format!("body {name} not declared")))?;
            let function = lower_body(body, self.call_conv)?;
            let mut ctx = Context::for_function(function);
            self.module
                .define_function(*fid, &mut ctx)
                .map_err(|e| JitError::Module(e.to_string()))?;
            // Drop ctx eagerly to free the IR memory; cranelift JIT
            // has already consumed the code.
            self.module.clear_context(&mut ctx);
        }

        // --- finalize -----------------------------------------------
        self.module
            .finalize_definitions()
            .map_err(|e| JitError::Module(e.to_string()))?;

        // --- collect finalized fn pointers --------------------------
        // SAFETY: `finalize_definitions` above committed every FuncId
        // in `entries` to executable memory. `get_finalized_function`
        // returns the raw entry pointer; calling it on a non-finalized
        // FuncId panics, but every FuncId in `entries` IS finalized
        // by construction.
        let mut fn_table = HashMap::with_capacity(entries.len());
        for (name, (fid, sig)) in entries {
            let ptr = self.module.get_finalized_function(fid);
            fn_table.insert(name, (ptr, sig));
        }

        Ok(JitHandle::new(self.module, fn_table))
    }

    /// Inspect the engine's detected ISA. Stable across wave-1;
    /// removed in 0056b when the engine becomes pure-internal.
    pub fn isa(&self) -> &dyn cranelift_codegen::isa::TargetIsa {
        self.isa.as_ref()
    }
}

/// Compute the export name for a MIR body. Mirrors the AOT
/// `cranelift_backend::declare_body` convention so a future
/// `lower_module<M: ClifModule>` factor-out keeps name parity.
fn body_export_name(body: &cobrust_mir::Body) -> String {
    if body.name.is_empty() {
        format!("_cobrust_init_{}", body.def_id.0)
    } else if body.name == "main" {
        "_cobrust_user_main".to_string()
    } else {
        body.name.clone()
    }
}

/// Build the host target ISA. Uses `cranelift-native` to detect
/// the running CPU and enable CPU-specific ISA features.
///
/// Wave-1 keeps PIC on and opt_level at `none` for fastest
/// codegen (REPL latency >> code quality for arithmetic-only
/// expressions). ADR-0056b adds an opt-level switch for stdlib
/// calls.
fn host_isa() -> Result<Arc<dyn cranelift_codegen::isa::TargetIsa>, JitError> {
    let mut shared_builder = settings::builder();
    shared_builder
        .set("opt_level", "none")
        .map_err(|e| JitError::Settings(e.to_string()))?;
    // cranelift-jit explicitly REQUIRES is_pic=false — it generates
    // absolute relocs into anon memory and asserts at module
    // construction time. AOT (cranelift-object) is the opposite: PIC
    // required for ELF/Mach-O linking. The settings divergence is
    // why ADR-0056a §3.2 mandates separate Aot/Jit codegen entries
    // sharing only the MIR-lowering loop.
    shared_builder
        .set("is_pic", "false")
        .map_err(|e| JitError::Settings(e.to_string()))?;
    let shared_flags = settings::Flags::new(shared_builder);

    let isa_builder =
        cranelift_native::builder().map_err(|e| JitError::HostIsaUnavailable(e.to_string()))?;
    let isa: OwnedTargetIsa = isa_builder
        .finish(shared_flags)
        .map_err(|e| JitError::ModuleConstruction(e.to_string()))?;
    Ok(isa)
}
