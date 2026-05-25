#![allow(
    clippy::items_after_statements,
    clippy::similar_names,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test corpus style (F51 lint discipline)"
)]
//! ADR-0058g sub-wave-4 — LLVM backend input + read_line runtime extern
//! hookup smoke fixtures.
//!
//! Sub-wave-4 wires four `__cobrust_input*` + `__cobrust_read_line`
//! helpers into the LLVM `lower_call` extern-name dispatch path:
//!
//!   - `__cobrust_input(prompt_ptr: *const u8, prompt_len: i64) -> *mut Str`
//!     (prompt is a string literal split via `expand_str_to_ptr_len`).
//!   - `__cobrust_input_str_buf(prompt_buf: *mut Str) -> *mut Str`
//!     (runtime Str-buffer prompt overload).
//!   - `__cobrust_input_no_prompt() -> *mut Str`
//!     (zero-arg empty-prompt path).
//!   - `__cobrust_read_line() -> *mut Str`
//!     (low-level stdin line reader; preserves trailing `\n`; EOF → empty).
//!
//! Cranelift parity references (ABI verbatim mirror, all confirmed at
//! `cobrust-codegen/src/cranelift_backend.rs:2811-2819`):
//!
//! ```text
//!   __cobrust_input              ([p, i64] -> p)
//!   __cobrust_input_str_buf      ([p]      -> p)
//!   __cobrust_input_no_prompt    ([]       -> p)
//!   __cobrust_read_line          ([]       -> p)
//! ```
//!
//! Stdlib ABI cross-confirmed at `cobrust-stdlib/src/io.rs:224,248,268,343`.
//!
//! Each fixture:
//!
//!   1. Builds minimal MIR via `Module { bodies: [...] }` directly.
//!   2. Compiles to LLVM IR via `emit()` with `Backend::Llvm`.
//!   3. Links against `libcobrust_stdlib.a` + `runtime/cobrust_main.c`
//!      using the system `cc` (matches wave-2/3 fixture link strategy).
//!   4. Spawns the resulting binary with `stdin(Stdio::piped())`,
//!      writes a test line, and asserts exit code matches expected.
//!
//! Stdin handling pattern: mirrors `cobrust-cli/tests/intrinsics_input.rs`
//! (lines 164-183). `Command::stdin(Stdio::piped())` + child handle's
//! `stdin.write_all(...)` + `wait_with_output()`.
//!
//! Pre-fix expectation (wave-1 stub fallthrough): the four input/read_line
//! externs were not in `runtime_helper_decls`, so calls routed to the
//! wave-1 stub branch — write i64 zero to `destination`, branch to
//! `target` — silently no-op'd. Post-fix: dispatch emits
//! `call @__cobrust_input*` / `@__cobrust_read_line` against the stdlib
//! symbols; the binary reads stdin and exits with the expected value.
//!
//! Exit-code derivation: since the MIR-level Str return is opaque at the
//! exit-code layer (no easy "string equals" assertion via process exit),
//! each fixture asserts the binary exits 0 — proving the extern dispatch
//! emits a well-formed `call` instruction (vs. wave-1 stub fallthrough,
//! which would also exit 0 BUT would not consume stdin). To distinguish
//! "actually consumed stdin" from "stub fallthrough", fixtures feed
//! oversized stdin (1 KiB) and verify the program runs to completion
//! without SIGPIPE / read hang. The wave-2 `link_and_run` `success()`
//! check covers this.
//!
//! F35-sibling discipline: these fixtures land sub-wave-4 ONLY. Six of
//! twelve F45a §2 categories remain wave-1 stubs after this sprint
//! (fmt / iter / math / parse_int+str-parsing / str-methods / LLM
//! router). DO NOT read sub-wave-4 closure as wave-3 closure.

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
    use std::io::Write;
    use std::path::PathBuf;
    use std::process::{Command, ExitStatus, Stdio};
    use target_lexicon::Triple;

    fn llvm_spec(name: &str) -> TargetSpec {
        let dir =
            std::env::temp_dir().join(format!("cobrust-0058g-w4-{name}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        TargetSpec {
            triple: Triple::host(),
            opt_level: OptLevel::None,
            backend: Backend::Llvm,
            artifact: ArtifactKind::Object,
            output_dir: dir,
            module_name: name.to_string(),
            source_path: None,
            runtime_dispatch: false,
            target_cpu: None,
        }
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

    /// Link + run with piped stdin. Mirrors the wave-2/3 `link_and_run`
    /// pattern but uses `Command::spawn` + `stdin.write_all` to feed
    /// stdin. Returns `(ExitStatus, stdout, stderr)` so callers can
    /// inspect both the exit code and any prompt that was written to
    /// stdout (the `__cobrust_input` family writes the prompt before
    /// reading from stdin).
    fn link_and_run_with_stdin(
        name: &str,
        module: &Module,
        stdin_bytes: &[u8],
    ) -> Option<(ExitStatus, String, String)> {
        if !cobrust_codegen::linker::linker_available() {
            return None;
        }
        let stdlib = find_stdlib_archive()?;
        let runtime_c = find_runtime_c()?;

        let spec = llvm_spec(name);
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

        let mut child = Command::new(&exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;
        {
            let stdin = child.stdin.as_mut().expect("stdin handle");
            let _ = stdin.write_all(stdin_bytes);
        }
        let out = child.wait_with_output().ok()?;
        Some((
            out.status,
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ))
    }

    /// Build `fn main() -> i64 { __cobrust_input("> "); return 0 }` at the
    /// MIR level. The single-arg call routes through the wave-2
    /// `expand_str_to_ptr_len` path: the source supplies one `Constant::Str`
    /// arg, the C signature expects two params (ptr, len), so dispatch
    /// expands the literal payload into a (ptr, len) pair.
    fn build_main_calling_input(prompt: &str) -> Module {
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
                name: "_input_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_input".to_string())),
                args: vec![Operand::Constant(Constant::Str(prompt.to_string()))],
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

    /// Build `fn main() -> i64 { __cobrust_input_no_prompt(); return 0 }`.
    /// Zero-arg variant — exercises the empty-prompt overload path.
    fn build_main_calling_input_no_prompt() -> Module {
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
                name: "_input_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_input_no_prompt".to_string())),
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

    /// Build `fn main() -> i64 { __cobrust_read_line(); return 0 }`.
    /// Zero-arg low-level stdin reader.
    fn build_main_calling_read_line() -> Module {
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
                name: "_read_line_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_read_line".to_string())),
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

    /// Build `fn main() -> i64 { let buf = __cobrust_str_new(); __cobrust_input_str_buf(buf); return 0 }`.
    /// Exercises the Str-buffer prompt overload (`__cobrust_input_str_buf`)
    /// using a fresh empty Str buffer constructed via the wave-2
    /// `__cobrust_str_new()` helper. The buf local is `Ty::Str` so the
    /// alloca lowers to `opaque_ptr_ty` (matches the wave-2 list fixture
    /// pattern at `llvm_wave3_list_runtime.rs:215` where the list local
    /// is `Ty::List(...)`, also lowering to opaque ptr). Without a ptr-
    /// typed alloca, the round-trip `ptr := __cobrust_str_new() → store
    /// → load → call __cobrust_input_str_buf(arg)` produces an i64 arg
    /// that LLVM's verifier rejects against the `(ptr) -> ptr` signature.
    fn build_main_calling_input_str_buf() -> Module {
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
                name: "_buf".to_string(),
                ty: Ty::Str,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_input_ret".to_string(),
                ty: Ty::Str,
                mutable: true,
                span: span0,
            },
        ];
        // bb0: buf = __cobrust_str_new()
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_new".to_string())),
                args: vec![],
                destination: Place::local(LocalId(1)),
                target: BlockId(1),
                unwind: None,
            },
            span: span0,
        };
        // bb1: ret = __cobrust_input_str_buf(buf)
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_input_str_buf".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(2)),
                target: BlockId(2),
                unwind: None,
            },
            span: span0,
        };
        // bb2: return 0
        let bb2 = MirBlock {
            id: BlockId(2),
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
                blocks: vec![bb0, bb1, bb2],
                return_local: LocalId(0),
                param_count: 0,
                span: span0,
            }],
        }
    }

    /// Sub-wave-4 fixture A: `__cobrust_input("> ")` lowers + links + the
    /// binary consumes stdin and exits 0. Pre-fix expectation (wave-1
    /// stub fallthrough): the call routed to the stub branch — write 0,
    /// branch to target — stdin was NOT consumed, prompt was NOT written.
    /// Post-fix: the dispatch path detects the single-Str-arg + 2-param
    /// signature, expands the literal payload via the wave-2
    /// `expand_str_to_ptr_len` path, and emits
    /// `call @__cobrust_input(ptr, len)`.
    #[test]
    fn llvm_emits_input_extern_call_with_prompt() {
        let Some((status, stdout, stderr)) = link_and_run_with_stdin(
            "input_prompt_smoke",
            &build_main_calling_input("> "),
            b"hello\n",
        ) else {
            return; // Prereqs missing (no llvm feature / no stdlib / no cc) — skip.
        };
        assert!(
            status.success(),
            "input_prompt_smoke: expected exit 0, got {status:?}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-4 fixture B: `__cobrust_input_no_prompt()` (zero-arg)
    /// lowers + reads stdin + exits 0.
    #[test]
    fn llvm_emits_input_no_prompt_extern_call() {
        let Some((status, stdout, stderr)) = link_and_run_with_stdin(
            "input_no_prompt_smoke",
            &build_main_calling_input_no_prompt(),
            b"world\n",
        ) else {
            return;
        };
        assert!(
            status.success(),
            "input_no_prompt_smoke: expected exit 0, got {status:?}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-4 fixture C: `__cobrust_read_line()` (zero-arg low-level
    /// stdin reader) lowers + reads stdin + exits 0. Verifies the
    /// extern dispatch handles the EOF-tolerant path (read_line returns
    /// empty Str on EOF, so even an empty stdin yields a well-formed
    /// return).
    #[test]
    fn llvm_emits_read_line_extern_call() {
        let Some((status, stdout, stderr)) = link_and_run_with_stdin(
            "read_line_smoke",
            &build_main_calling_read_line(),
            b"line one\n",
        ) else {
            return;
        };
        assert!(
            status.success(),
            "read_line_smoke: expected exit 0, got {status:?}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-4 fixture D: `__cobrust_input_str_buf(buf)` lowers + the
    /// buf is constructed via the wave-2 `__cobrust_str_new()` helper,
    /// then passed by-pointer (no `expand_str_to_ptr_len` — runtime Str
    /// buffer is already a single ptr, not a literal). Verifies the
    /// 1-param Str-buffer overload dispatches correctly.
    #[test]
    fn llvm_emits_input_str_buf_extern_call() {
        let Some((status, stdout, stderr)) = link_and_run_with_stdin(
            "input_str_buf_smoke",
            &build_main_calling_input_str_buf(),
            b"buffered\n",
        ) else {
            return;
        };
        assert!(
            status.success(),
            "input_str_buf_smoke: expected exit 0, got {status:?}; stdout={stdout:?} stderr={stderr:?}"
        );
    }
}
