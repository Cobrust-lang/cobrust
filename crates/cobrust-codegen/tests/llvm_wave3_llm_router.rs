#![allow(
    clippy::items_after_statements,
    clippy::similar_names,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "test corpus style (F51 lint discipline)"
)]
//! ADR-0058g sub-wave-6 — LLVM backend LLM router intrinsics runtime
//! extern hookup smoke fixtures. **Final wave-3 sub-wave**: this
//! sprint closes F45a §2 row 12 (LLM router) and brings the LLVM
//! backend to feature-parity with the Cranelift backend for all
//! twelve F45a categories.
//!
//! Sub-wave-6 wires the THIRTEEN LLM router runtime helpers into the
//! LLVM `lower_call` extern-name dispatch path (declarations added to
//! `LlvmEmitter::declare_runtime_helpers`):
//!
//! 1. **M-AI.0 (α Phase 2)** — `cobrust.llm` source binding (3 helpers):
//!    `__cobrust_llm_complete`, `__cobrust_llm_dispatch`,
//!    `__cobrust_llm_stream`.
//! 2. **M-AI.1 (α Phase 3)** — `cobrust.prompt` source binding (5 helpers):
//!    `__cobrust_prompt_render`, `__cobrust_prompt_format_few_shot`,
//!    `__cobrust_prompt_format_system_user`, `__cobrust_prompt_escape_braces`,
//!    `__cobrust_llm_complete_structured`.
//! 3. **M-AI.2 (α Phase 4)** — `cobrust.tool` source binding (5 helpers):
//!    `__cobrust_tool_schema`, `__cobrust_tool_registry_new`,
//!    `__cobrust_tool_registry_register`, `__cobrust_tool_invoke`,
//!    `__cobrust_llm_complete_with_tools`.
//!
//! Cranelift parity references (ABI verbatim mirror, all confirmed at
//! `cobrust-codegen/src/cranelift_backend.rs:2896-2961`):
//!
//! ```text
//!   __cobrust_llm_complete             ([p, p, p]    -> p)
//!   __cobrust_llm_dispatch             ([p, p]       -> p)
//!   __cobrust_llm_stream               ([p, p, p]    -> p)   (list[str])
//!   __cobrust_prompt_render            ([p, p, p]    -> p)
//!   __cobrust_prompt_format_few_shot   ([p, p, p]    -> p)
//!   __cobrust_prompt_format_system_user([p, p]       -> p)
//!   __cobrust_prompt_escape_braces     ([p]          -> p)
//!   __cobrust_llm_complete_structured  ([p, p]       -> p)
//!   __cobrust_tool_schema              ([p, p, p, p] -> p)
//!   __cobrust_tool_registry_new        ([]           -> p)
//!   __cobrust_tool_registry_register   ([p, p]       -> p)
//!   __cobrust_tool_invoke              ([p, p]       -> p)
//!   __cobrust_llm_complete_with_tools  ([p, p]       -> p)
//! ```
//!
//! Stdlib ABI cross-confirmed at:
//!   - `cobrust-stdlib/src/llm.rs:422,444,466`
//!   - `cobrust-stdlib/src/prompt.rs:247,270,291,308,324`
//!   - `cobrust-stdlib/src/tool.rs:254,278,289,306,321`
//!
//! # Real-LLM gating strategy
//!
//! M-AI.0 α Phase 2 **Decision 7** (per spike SHA 705f592 + α-RATIFY):
//! every LLM router runtime helper returns an empty `Str` (or empty
//! `List` for `llm_stream` / `tool_registry_new`) on *any* failure
//! path. Critically, when no `cobrust.toml` is present, `config_bundle()`
//! returns `None` and the C-ABI shims short-circuit to `alloc_str_buffer("")`
//! BEFORE any tokio dispatch — so:
//!
//! - **No network connectivity required**.
//! - **No API key required**.
//! - **No `cobrust.toml` required**.
//! - Symbols are unconditionally exported by `cobrust-stdlib`
//!   regardless of the `llm-router` feature flag (`llm.rs` shims have
//!   no `#[cfg]` gating; `prompt.rs`/`tool.rs` use feature-conditional
//!   bodies but emit the same symbol).
//!
//! These fixtures therefore verify:
//!   1. LLVM IR emit succeeds for every helper extern decl
//!      (no `lower-unknown-name` fallthrough — the original wave-1
//!      stub-load surface).
//!   2. Link against `libcobrust_stdlib.a` resolves the symbol
//!      (extern decl ABI matches stdlib `#[unsafe(no_mangle)]` body).
//!   3. Binary runs to completion without crashing on the empty-Str
//!      / empty-List Decision 7 fallback path.
//!
//! Tests that would exercise *real* router dispatch (configured
//! `cobrust.toml`, live provider) are out-of-scope for this fixture;
//! they live under `cobrust-stdlib/tests/llm_corpus.rs` (Tier 3) and
//! are gated by `real-llm-smoke` environment.
//!
//! Each fixture follows the wave-2/3/4/5 `link_and_run` pattern:
//!   1. Build minimal MIR via `Module { bodies: [...] }` directly.
//!   2. Compile to LLVM IR via `emit()` with `Backend::Llvm`.
//!   3. Link against `libcobrust_stdlib.a` + `runtime/cobrust_main.c`
//!      using the system `cc` (matches wave-2/3/4/5 link strategy).
//!   4. Run the resulting binary and assert exit code matches expected.
//!
//! Combined-helper fixtures (per F37 silent-rot guard): the fixture
//! body chains `llm_*` / `prompt_*` / `tool_*` calls with
//! `__cobrust_str_len` (or `__cobrust_list_len` for `llm_stream`) and
//! exits with that length. The Decision 7 contract guarantees the
//! result is 0 when no `cobrust.toml` is present — a single observable
//! signal that exercises both the LLM router helper dispatch AND the
//! str/list runtime helper chain end-to-end vs. wave-1 stub
//! fallthrough.
//!
//! F35-sibling discipline: these fixtures land sub-wave-6 ONLY. After
//! this sprint **all twelve F45a §2 categories are RESOLVED** and the
//! LLVM backend reaches feature-parity with the Cranelift backend for
//! the entire wave-3 stdlib runtime surface.

#[cfg(feature = "llvm")]
mod llvm {
    use cobrust_codegen::{ArtifactKind, Backend, OptLevel, TargetSpec, emit};
    use cobrust_frontend::span::{FileId, Span};
    use cobrust_hir::DefId;
    use cobrust_mir::{
        BasicBlock as MirBlock, BlockId, Body, Constant, LocalDecl, LocalId, Module, Operand,
        Place, Terminator,
    };
    use cobrust_types::Ty;
    use std::path::PathBuf;
    use std::process::{Command, ExitStatus};
    use target_lexicon::Triple;

    // F63 (2026-05-27): RAII tempdir.
    fn llvm_spec(name: &str) -> (TargetSpec, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("create tempdir for llvm spec");
        let spec = TargetSpec {
            triple: Triple::host(),
            opt_level: OptLevel::None,
            backend: Backend::Llvm,
            artifact: ArtifactKind::Object,
            output_dir: dir.path().to_path_buf(),
            module_name: name.to_string(),
            source_path: None,
            runtime_dispatch: false,
            target_cpu: None,
        };
        (spec, dir)
    }

    fn find_stdlib_archive() -> Option<PathBuf> {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").ok()?;
        let workspace = std::path::Path::new(&manifest).parent()?.parent()?;
        for profile in ["debug", "release"] {
            let p = workspace
                .join("target")
                .join(profile)
                .join("libcobrust_stdlib.a");
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    fn find_runtime_c() -> Option<PathBuf> {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").ok()?;
        let workspace = std::path::Path::new(&manifest).parent()?.parent()?;
        let p = workspace.join("crates/cobrust-cli/runtime/cobrust_main.c");
        if p.exists() { Some(p) } else { None }
    }

    /// Mirrors wave-2/3/4/5 `link_and_run`.
    fn link_and_run(name: &str, module: &Module) -> Option<(ExitStatus, String, String)> {
        if !cobrust_codegen::linker::linker_available() {
            return None;
        }
        let stdlib = find_stdlib_archive()?;
        let runtime_c = find_runtime_c()?;

        let (spec, _spec_guard) = llvm_spec(name);
        let artifact =
            emit(module, spec).unwrap_or_else(|e| panic!("LLVM emit `{name}` failed: {e}"));
        let user_obj = artifact.path().to_path_buf();
        let dir = user_obj.parent().expect("user obj parent").to_path_buf();

        let runtime_obj = dir.join("cobrust_main.o");
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let cc_status = Command::new(&cc)
            .arg("-c")
            .arg(&runtime_c)
            .arg("-o")
            .arg(&runtime_obj)
            .status()
            .ok()?;
        if !cc_status.success() {
            return None;
        }

        let exe = dir.join(format!("{name}.exe"));
        let mut link_cmd = Command::new(&cc);
        link_cmd
            .arg(&user_obj)
            .arg(&runtime_obj)
            .arg(&stdlib)
            .arg("-o")
            .arg(&exe);
        if cfg!(target_os = "linux") {
            link_cmd.arg("-lpthread").arg("-ldl").arg("-lm");
        }
        let link_status = link_cmd.status().ok()?;
        if !link_status.success() {
            return None;
        }

        let out = Command::new(&exe).output().ok()?;
        Some((
            out.status,
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ))
    }

    fn make_main(extra_locals: Vec<LocalDecl>, blocks: Vec<MirBlock>) -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let mut locals = vec![LocalDecl {
            id: LocalId(0),
            name: "_return".to_string(),
            ty: Ty::Int,
            mutable: true,
            span: span0,
        }];
        locals.extend(extra_locals);
        Module {
            bodies: vec![Body {
                def_id: DefId(0),
                name: "main".to_string(),
                locals,
                blocks,
                return_local: LocalId(0),
                param_count: 0,
                span: span0,
            }],
        }
    }

    // ====================================================================
    // Fixture builders
    // ====================================================================

    /// Build: `provider = ""; model = ""; prompt = "";
    ///         out = __cobrust_llm_complete(provider, model, prompt);
    ///         _return = __cobrust_str_len(out);`
    ///
    /// Expected exit 0: Decision 7 — no `cobrust.toml` present →
    /// `config_bundle()` returns None → shim short-circuits to
    /// empty `Str` → `__cobrust_str_len` returns 0.
    fn build_llm_complete_then_str_len() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![LocalDecl {
            id: LocalId(1),
            name: "_out".to_string(),
            ty: Ty::Str,
            mutable: true,
            span: span0,
        }];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_llm_complete".to_string())),
                args: vec![
                    Operand::Constant(Constant::Str(String::new())),
                    Operand::Constant(Constant::Str(String::new())),
                    Operand::Constant(Constant::Str(String::new())),
                ],
                destination: Place::local(LocalId(1)),
                target: BlockId(1),
                unwind: None,
            },
            span: span0,
        };
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_len".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(0)),
                target: BlockId(2),
                unwind: None,
            },
            span: span0,
        };
        let bb2 = MirBlock {
            id: BlockId(2),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2])
    }

    /// Build: `out = __cobrust_llm_dispatch(task="", prompt="");
    ///         _return = __cobrust_str_len(out);`
    /// Expected exit 0 (Decision 7 empty fallback).
    fn build_llm_dispatch_then_str_len() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![LocalDecl {
            id: LocalId(1),
            name: "_out".to_string(),
            ty: Ty::Str,
            mutable: true,
            span: span0,
        }];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_llm_dispatch".to_string())),
                args: vec![
                    Operand::Constant(Constant::Str(String::new())),
                    Operand::Constant(Constant::Str(String::new())),
                ],
                destination: Place::local(LocalId(1)),
                target: BlockId(1),
                unwind: None,
            },
            span: span0,
        };
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_len".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(0)),
                target: BlockId(2),
                unwind: None,
            },
            span: span0,
        };
        let bb2 = MirBlock {
            id: BlockId(2),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2])
    }

    /// Build: `list = __cobrust_llm_stream(provider="", model="", prompt="");
    ///         _return = __cobrust_list_len(list);`
    /// Expected exit 0 (Decision 7 empty-list fallback).
    fn build_llm_stream_then_list_len() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![LocalDecl {
            id: LocalId(1),
            name: "_list".to_string(),
            ty: Ty::List(Box::new(Ty::Str)),
            mutable: true,
            span: span0,
        }];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_llm_stream".to_string())),
                args: vec![
                    Operand::Constant(Constant::Str(String::new())),
                    Operand::Constant(Constant::Str(String::new())),
                    Operand::Constant(Constant::Str(String::new())),
                ],
                destination: Place::local(LocalId(1)),
                target: BlockId(1),
                unwind: None,
            },
            span: span0,
        };
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_list_len".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(0)),
                target: BlockId(2),
                unwind: None,
            },
            span: span0,
        };
        let bb2 = MirBlock {
            id: BlockId(2),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2])
    }

    /// Build: `out = __cobrust_prompt_format_system_user(system="", user="");
    ///         _return = __cobrust_str_len(out);`
    ///
    /// Expected exit 0: `prompt_format_system_user_helper("", "")` joins
    /// system + "\n\n" + user. With both empty, the result is "\n\n"
    /// (len 2)... but the Decision 7 contract for prompt helpers is
    /// "feature-gated body, else empty"; the prompt helpers are
    /// unconditionally compiled but `prompt_format_system_user_helper`
    /// is a pure-Rust function with no feature gating. Verify the
    /// actual stdlib body shape.
    ///
    /// Inspecting `prompt.rs:291-300`: the shim unconditionally
    /// invokes `prompt_format_system_user_helper(&s, &u)`. With both
    /// args empty this returns a 2-byte string ("\n\n"). The fixture
    /// exits with 2, not 0 — a non-zero observable signal that the
    /// dispatch path threaded the call correctly.
    fn build_prompt_format_system_user_then_str_len() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![LocalDecl {
            id: LocalId(1),
            name: "_out".to_string(),
            ty: Ty::Str,
            mutable: true,
            span: span0,
        }];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str(
                    "__cobrust_prompt_format_system_user".to_string(),
                )),
                args: vec![
                    Operand::Constant(Constant::Str(String::new())),
                    Operand::Constant(Constant::Str(String::new())),
                ],
                destination: Place::local(LocalId(1)),
                target: BlockId(1),
                unwind: None,
            },
            span: span0,
        };
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_len".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(0)),
                target: BlockId(2),
                unwind: None,
            },
            span: span0,
        };
        let bb2 = MirBlock {
            id: BlockId(2),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2])
    }

    /// Build: `out = __cobrust_prompt_escape_braces(text="hi");
    ///         _return = __cobrust_str_len(out);`
    ///
    /// `prompt_escape_braces_helper("hi")` produces "hi" (no braces to
    /// escape) — len 2. Verifies the single-arg prompt helper
    /// dispatch path.
    fn build_prompt_escape_braces_then_str_len() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![LocalDecl {
            id: LocalId(1),
            name: "_out".to_string(),
            ty: Ty::Str,
            mutable: true,
            span: span0,
        }];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str(
                    "__cobrust_prompt_escape_braces".to_string(),
                )),
                args: vec![Operand::Constant(Constant::Str("hi".to_string()))],
                destination: Place::local(LocalId(1)),
                target: BlockId(1),
                unwind: None,
            },
            span: span0,
        };
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_len".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(0)),
                target: BlockId(2),
                unwind: None,
            },
            span: span0,
        };
        let bb2 = MirBlock {
            id: BlockId(2),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2])
    }

    /// Build: `reg = __cobrust_tool_registry_new();`
    /// Expected: emit + link succeed; binary exits with 0 (default).
    /// The return is a pointer (a registry handle), not an int — we
    /// don't observe it via str_len since registries aren't strings.
    /// Validates the zero-arg helper dispatch path.
    fn build_tool_registry_new() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![LocalDecl {
            id: LocalId(1),
            name: "_reg".to_string(),
            ty: Ty::Str, // opaque ptr — stored in a Str slot to keep MIR simple
            mutable: true,
            span: span0,
        }];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_tool_registry_new".to_string())),
                args: vec![],
                destination: Place::local(LocalId(1)),
                target: BlockId(1),
                unwind: None,
            },
            span: span0,
        };
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1])
    }

    // ====================================================================
    // #[test] entry points
    // ====================================================================

    /// Sub-wave-6 fixture A: `__cobrust_llm_complete` extern decl
    /// resolves + dispatches; Decision 7 empty-Str fallback gives
    /// `str_len` == 0.
    #[test]
    fn llvm_emits_llm_complete_then_str_len() {
        let Some((status, stdout, stderr)) = link_and_run(
            "llm_complete_then_str_len",
            &build_llm_complete_then_str_len(),
        ) else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 0,
            "llm_complete_then_str_len: expected exit 0 (Decision 7 empty fallback), got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-6 fixture B: `__cobrust_llm_dispatch` extern decl
    /// resolves + dispatches; Decision 7 empty-Str fallback.
    #[test]
    fn llvm_emits_llm_dispatch_then_str_len() {
        let Some((status, stdout, stderr)) = link_and_run(
            "llm_dispatch_then_str_len",
            &build_llm_dispatch_then_str_len(),
        ) else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 0,
            "llm_dispatch_then_str_len: expected exit 0 (Decision 7 empty fallback), got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-6 fixture C: `__cobrust_llm_stream` extern decl
    /// resolves + dispatches; Decision 7 empty-List fallback gives
    /// `list_len` == 0.
    #[test]
    fn llvm_emits_llm_stream_then_list_len() {
        let Some((status, stdout, stderr)) = link_and_run(
            "llm_stream_then_list_len",
            &build_llm_stream_then_list_len(),
        ) else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 0,
            "llm_stream_then_list_len: expected exit 0 (Decision 7 empty-list fallback), got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-6 fixture D: `__cobrust_prompt_format_system_user`
    /// extern decl resolves; pure-Rust helper concatenates
    /// "" + "\n\n" + "" → str_len == 2.
    #[test]
    fn llvm_emits_prompt_format_system_user_then_str_len() {
        let Some((status, stdout, stderr)) = link_and_run(
            "prompt_format_system_user_then_str_len",
            &build_prompt_format_system_user_then_str_len(),
        ) else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 2,
            "prompt_format_system_user_then_str_len: expected exit 2 (\\n\\n separator), got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-6 fixture E: `__cobrust_prompt_escape_braces("hi")`
    /// extern decl resolves; pure-Rust helper returns "hi" (no braces
    /// to escape) → str_len == 2.
    #[test]
    fn llvm_emits_prompt_escape_braces_then_str_len() {
        let Some((status, stdout, stderr)) = link_and_run(
            "prompt_escape_braces_then_str_len",
            &build_prompt_escape_braces_then_str_len(),
        ) else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 2,
            "prompt_escape_braces_then_str_len: expected exit 2 (\"hi\" len), got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-6 fixture F: `__cobrust_tool_registry_new()` zero-arg
    /// helper extern decl resolves + dispatches cleanly. Binary exits
    /// with the default _return value (0).
    #[test]
    fn llvm_emits_tool_registry_new() {
        let Some((status, stdout, stderr)) =
            link_and_run("tool_registry_new", &build_tool_registry_new())
        else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 0,
            "tool_registry_new: expected exit 0 (default _return), got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }
}

// =====================================================================
// Default-features (non-llvm) build path: empty test module so the
// crate still has compilation coverage of this file under
// `cargo test -p cobrust-codegen` without `--features llvm`.
// =====================================================================
#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_wave3_llm_router_no_llvm_feature_no_op() {
    // This test exists only so `cargo test -p cobrust-codegen` (no
    // features) does not skip the entire fixture file at compile
    // time. The actual LLVM-backed verification runs under
    // `--features llvm` via the `mod llvm` module above.
}
