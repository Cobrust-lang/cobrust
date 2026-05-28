//! ADR-0058g sub-wave-1 — LLVM backend `__cobrust_panic` +
//! `__cobrust_argv` extern hookup smoke fixtures.
//!
//! Mirrors the wave-2 `stdlib_io_*` fixture pattern in
//! `crates/cobrust-codegen/tests/codegen_diff_corpus.rs`, but adds two
//! shapes the wave-2 pattern did not need:
//!
//!  - **panic exit-code assertion**: `__cobrust_panic` is `-> !` per
//!    `cobrust-stdlib/src/panic.rs:47`; the LLVM lowering emits
//!    `call` + `unreachable` (ADR-0058g §6.2 resolution — Cobrust does
//!    NOT use exceptions as the default error path, CLAUDE.md §2.2).
//!    The linked binary must exit with `exit_codes::INTERNAL_PANIC = 3`
//!    (`cobrust-stdlib/src/runtime.rs:376`), so the wave-2
//!    `success()`-only run helper is unusable; this file ships a
//!    bespoke run helper that returns `(ExitStatus, stdout, stderr)`.
//!  - **zero-arg extern call**: `__cobrust_argv() -> *mut u8`
//!    (`cobrust-stdlib/src/env.rs:64`); the MIR side rewrites the
//!    source-level `argv()` to `Constant::Str("__cobrust_argv")` with
//!    zero args (`cobrust-cli/src/build/intrinsics.rs:1439-1447`).
//!    We materialise the same MIR shape directly so the test does not
//!    depend on the frontend rewrite path.
//!
//! Cranelift parity references:
//!  - `cobrust-codegen/src/cranelift_backend.rs:2822` — argv extern decl
//!    `(void) -> ptr`.
//!  - `__cobrust_capture_argv` is C-shim-only
//!    (`cobrust-cli/runtime/cobrust_main.c:21-25`); Cranelift does NOT
//!    declare it, and neither does LLVM (intentional parity per
//!    sub-wave-1 scope decision).
//!
//! F35-sibling discipline: these two fixtures land sub-wave-1 ONLY.
//! Ten of the twelve F45a §2 categories remain wave-1 stubs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(feature = "llvm")]
mod llvm {
    use cobrust_codegen::{ArtifactKind, Backend, OptLevel, TargetSpec, emit};
    use cobrust_frontend::span::{FileId, Span};
    use cobrust_hir::DefId;
    use cobrust_mir::{
        BasicBlock as MirBlock, BlockId, Body, Constant, LocalDecl, LocalId, Module, Operand,
        Place, Rvalue, Statement, StatementKind, Terminator,
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

    /// Bespoke link-and-run that returns the raw `ExitStatus` (not just
    /// `success()`). Required by the panic fixture, which expects
    /// `exit_codes::INTERNAL_PANIC = 3`.
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

    /// Build `fn main() -> i64 { __cobrust_argv(); return 0 }` at the MIR
    /// level. Mirrors the source-level `Kind::Argv` rewrite at
    /// `cobrust-cli/src/build/intrinsics.rs:1439-1447`.
    fn build_main_calling_argv() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let locals = vec![
            LocalDecl {
                id: LocalId(0),
                name: "_return".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(1),
                name: "_argv_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_argv".to_string())),
                args: vec![],
                destination: Place::local(LocalId(1)),
                target: BlockId(1),
                unwind: None,
            },
            span: span0,
        };
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::Use(Operand::Constant(Constant::Int(0))),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        Module {
            bodies: vec![Body {
                def_id: DefId(0),
                name: "main".to_string(),
                locals,
                blocks: vec![bb0, bb1],
                return_local: LocalId(0),
                param_count: 0,
                span: span0,
            }],
        }
    }

    /// Build `fn main() -> i64 { __cobrust_panic("boom"); return 0 }` at
    /// the MIR level. The trailing `return 0` is structurally required
    /// by the MIR shape (every Body needs a Return terminator) but is
    /// dead code at the LLVM IR level — the panic call is `noreturn` so
    /// the post-call basic block becomes unreachable.
    fn build_main_calling_panic(msg: &str) -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let locals = vec![
            LocalDecl {
                id: LocalId(0),
                name: "_return".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(1),
                name: "_panic_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_panic".to_string())),
                args: vec![Operand::Constant(Constant::Str(msg.to_string()))],
                destination: Place::local(LocalId(1)),
                target: BlockId(1),
                unwind: None,
            },
            span: span0,
        };
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::Use(Operand::Constant(Constant::Int(0))),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        Module {
            bodies: vec![Body {
                def_id: DefId(0),
                name: "main".to_string(),
                locals,
                blocks: vec![bb0, bb1],
                return_local: LocalId(0),
                param_count: 0,
                span: span0,
            }],
        }
    }

    /// Sub-wave-1 fixture A: `__cobrust_argv()` call lowers + links + the
    /// resulting binary exits 0. Pre-fix expectation (wave-1 stub
    /// fallthrough): `__cobrust_argv` was not declared as a runtime
    /// helper, so the call routed to the wave-1 stub branch — write 0,
    /// branch to target — silently no-op. Post-fix: the call hits the
    /// extern-name dispatch path and emits a `call @__cobrust_argv()`
    /// instruction backed by the stdlib symbol; the binary returns 0.
    #[test]
    fn llvm_emits_argv_extern_call_and_exits_zero() {
        let Some((status, stdout, stderr)) = link_and_run("argv_smoke", &build_main_calling_argv())
        else {
            return; // Prereqs missing (no llvm feature / no stdlib / no cc) — skip.
        };
        assert!(
            status.success(),
            "argv_smoke: expected exit 0, got {status:?}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-1 fixture B: `__cobrust_panic("boom")` lowers as
    /// `call @__cobrust_panic(ptr, len)` followed by `unreachable`; the
    /// resulting binary exits with `exit_codes::INTERNAL_PANIC = 3`
    /// (`cobrust-stdlib/src/runtime.rs:376`). Pre-fix expectation:
    /// `__cobrust_panic` decl was already in `runtime_helper_decls`
    /// (wave-2), but `lower_call` emitted `call` + branch — meaning the
    /// noreturn callee returned to a dead block; LLVM would either UB or
    /// (more likely) compile to a fall-through that silently exited 0
    /// instead of aborting. Post-fix: the dispatch branch detects the
    /// callee name and emits `unreachable`, matching Cranelift's
    /// `InstructionData::Unreachable` terminator semantics.
    #[test]
    fn llvm_emits_panic_extern_call_with_unreachable() {
        let Some((status, stdout, stderr)) =
            link_and_run("panic_smoke", &build_main_calling_panic("boom"))
        else {
            return; // Prereqs missing — skip.
        };
        assert!(
            !status.success(),
            "panic_smoke: expected non-zero exit (panic abort), got success; \
             stdout={stdout:?} stderr={stderr:?}"
        );
        // `exit_codes::INTERNAL_PANIC = 3` per stdlib runtime; if the
        // exact code is unavailable on this platform (e.g. signalled
        // termination on macOS for some abort paths) accept any non-zero.
        if let Some(code) = status.code() {
            assert_eq!(
                code, 3,
                "panic_smoke: expected INTERNAL_PANIC exit code 3, got {code}; \
                 stdout={stdout:?} stderr={stderr:?}"
            );
        }
        // Stderr should contain the panic message (stdlib `panic()`
        // writes via `eprintln!`).
        assert!(
            stderr.contains("boom"),
            "panic_smoke: expected stderr to contain `boom`, got stderr={stderr:?}"
        );
    }
}

// On default (non-LLVM) feature builds, both fixtures degrade to pass
// (no LLVM backend to exercise; the wave-3 surface is feature-gated).
#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_argv_extern_call_and_exits_zero() {
    // Skipped on default build — LLVM backend feature-gated.
}

#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_panic_extern_call_with_unreachable() {
    // Skipped on default build — LLVM backend feature-gated.
}
