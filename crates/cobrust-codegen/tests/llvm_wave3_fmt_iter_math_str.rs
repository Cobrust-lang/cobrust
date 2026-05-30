#![allow(
    clippy::items_after_statements,
    clippy::similar_names,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "test corpus style (F51 lint discipline)"
)]
//! ADR-0058g sub-wave-5 — LLVM backend fmt / iter / math / parse_int+str-
//! parsing / str-methods runtime extern hookup smoke fixtures.
//!
//! Sub-wave-5 wires the FIVE remaining runtime-helper categories into the
//! LLVM `lower_call` extern-name dispatch path:
//!
//! 1. **fmt** family (ADR-0064 §3.3, ADR-0044 W2, M-F.3.3):
//!    `__cobrust_fmt_int`, `__cobrust_fmt_float`, `__cobrust_fmt_float_prec`,
//!    `__cobrust_fmt_bool`, `__cobrust_fmt_str`, `__cobrust_fmt_repr`,
//!    `__cobrust_str_len`, `__cobrust_str_ptr`, `__cobrust_str_clone`.
//! 2. **iter** family (ADR-0044 W2 Phase 2 for-protocol):
//!    `__cobrust_iter_init`, `__cobrust_iter_next`, `__cobrust_iter_drop`.
//! 3. **math** family (M-F.3.3 gap (b)):
//!    `__cobrust_math_sqrt`, `__cobrust_math_floor`, `__cobrust_math_ceil`,
//!    `__cobrust_math_round`, `__cobrust_math_abs`, `__cobrust_math_sin`,
//!    `__cobrust_math_cos`, `__cobrust_math_tan`, `__cobrust_math_log`,
//!    `__cobrust_math_exp`, `__cobrust_math_pow`.
//! 4. **parse_int + str-parsing** (ADR-0044 W2 Phase 3):
//!    `__cobrust_parse_int`, `__cobrust_str_len_src`, `__cobrust_str_at`,
//!    `__cobrust_str_eq`, `__cobrust_str_eq_lit`, `__cobrust_str_ord`,
//!    `__cobrust_parse_int_tok`, `__cobrust_count_toks`.
//! 5. **str-methods** (M-F.3.5 string stdlib, ADR-0050e):
//!    `__cobrust_str_split`, `__cobrust_str_join`, `__cobrust_str_replace`,
//!    `__cobrust_str_trim`, `__cobrust_str_find`, `__cobrust_str_contains`,
//!    `__cobrust_str_starts_with`, `__cobrust_str_ends_with`,
//!    `__cobrust_str_lower`, `__cobrust_str_upper`.
//!
//! Cranelift parity references (ABI verbatim mirror, all confirmed at
//! `cobrust-codegen/src/cranelift_backend.rs:2765-2894`):
//!
//! ```text
//!   __cobrust_fmt_int            ([p, i64]      -> ())
//!   __cobrust_fmt_float          ([p, f64]      -> ())
//!   __cobrust_fmt_float_prec     ([p, f64, p, i64] -> ())
//!   __cobrust_fmt_bool           ([p, i64]      -> ())
//!   __cobrust_fmt_str            ([p, p, i64]   -> ())
//!   __cobrust_fmt_repr           ([p, p, i64]   -> ())
//!   __cobrust_str_len            ([p]           -> i64)
//!   __cobrust_str_ptr            ([p]           -> p)
//!   __cobrust_str_clone          ([p]           -> p)
//!   __cobrust_iter_init          ([i64]         -> p)
//!   __cobrust_iter_next          ([p]           -> i64)
//!   __cobrust_iter_drop          ([p]           -> ())
//!   __cobrust_math_{sqrt|floor|ceil|round|abs|sin|cos|tan|log|exp}
//!                                ([f64]         -> f64)
//!   __cobrust_math_pow           ([f64, f64]    -> f64)
//!   __cobrust_parse_int          ([p]           -> i64)
//!   __cobrust_str_len_src        ([p]           -> i64)
//!   __cobrust_str_at             ([p, i64]      -> p)
//!   __cobrust_str_eq             ([p, p]        -> i64)
//!   __cobrust_str_eq_lit         ([p, p, i64]   -> i64)
//!   __cobrust_str_ord            ([p]           -> i64)
//!   __cobrust_parse_int_tok      ([p, i64]      -> i64)
//!   __cobrust_count_toks         ([p]           -> i64)
//!   __cobrust_str_split          ([p, p]        -> p)
//!   __cobrust_str_join           ([p, p]        -> p)
//!   __cobrust_str_replace        ([p, p, p]     -> p)
//!   __cobrust_str_trim           ([p]           -> p)
//!   __cobrust_str_find           ([p, p]        -> i64)
//!   __cobrust_str_contains       ([p, p]        -> i64)
//!   __cobrust_str_starts_with    ([p, p]        -> i64)
//!   __cobrust_str_ends_with      ([p, p]        -> i64)
//!   __cobrust_str_lower          ([p]           -> p)
//!   __cobrust_str_upper          ([p]           -> p)
//! ```
//!
//! Stdlib ABI cross-confirmed at:
//!   - `cobrust-stdlib/src/fmt.rs:105,121,143,195,212,230,247,264,306`
//!   - `cobrust-stdlib/src/iter.rs:278,324,349`
//!   - `cobrust-stdlib/src/math.rs:95,101,107,113,119,125,131,137,143,149,155`
//!   - `cobrust-stdlib/src/io.rs:508,524,541,563,590,614,653,673`
//!   - `cobrust-stdlib/src/string.rs:257,288,319,334,347,365,378,391,404,416`
//!
//! Each fixture follows the wave-2/3/4 `link_and_run` pattern:
//!   1. Build minimal MIR via `Module { bodies: [...] }` directly.
//!   2. Compile to LLVM IR via `emit()` with `Backend::Llvm`.
//!   3. Link against `libcobrust_stdlib.a` + `runtime/cobrust_main.c`
//!      using the system `cc` (matches wave-2/3/4 fixture link strategy).
//!   4. Run the resulting binary and assert exit code matches expected.
//!
//! Combined-helper fixtures (per F37 silent-rot guard): when multiple
//! helpers naturally chain (e.g. `__cobrust_str_lower("ABC") → buf` then
//! `__cobrust_str_len(buf) → i64`), the fixture exits with the chained
//! result, providing a single observable signal that exercises both
//! helpers. The dispatch contract is verified end-to-end vs. wave-1
//! stub fallthrough (which would exit zero on every helper that returns
//! a non-zero value through a non-stub codepath).
//!
//! F35-sibling discipline: these fixtures land sub-wave-5 ONLY. ONE of
//! the twelve F45a §2 categories remains wave-1 stub after this sprint
//! (LLM router — `__cobrust_llm_complete` / `__cobrust_llm_dispatch` /
//! `__cobrust_llm_stream`, tracked for sub-wave-6). DO NOT read sub-
//! wave-5 closure as wave-3 closure.

#[cfg(feature = "llvm")]
mod llvm {
    use cobrust_codegen::{ArtifactKind, Backend, OptLevel, TargetSpec, emit};
    use cobrust_frontend::span::{FileId, Span};
    use cobrust_hir::DefId;
    use cobrust_mir::{
        BasicBlock as MirBlock, BlockId, Body, CastKind, Constant, LocalDecl, LocalId, Module,
        Operand, Place, Rvalue, Statement, StatementKind, Terminator,
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

    /// Mirrors wave-2 `link_and_run` at `llvm_wave3_list_runtime.rs:120-168`.
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
            validated_body_of: None,
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
    // fmt family fixtures
    // ====================================================================

    /// Fixture: `buf = __cobrust_str_new(); __cobrust_fmt_int(buf, 42);
    /// _return = __cobrust_str_len(buf)`. Expected exit: 2 (length of "42").
    fn build_fmt_int_then_str_len() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_buf".to_string(),
                ty: Ty::Str,
                mutable: true,
                span: span0,
                validated_body_of: None,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_fmt_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
                validated_body_of: None,
            },
        ];
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
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_fmt_int".to_string())),
                args: vec![
                    Operand::Copy(Place::local(LocalId(1))),
                    Operand::Constant(Constant::Int(42)),
                ],
                destination: Place::local(LocalId(2)),
                target: BlockId(2),
                unwind: None,
            },
            span: span0,
        };
        let bb2 = MirBlock {
            id: BlockId(2),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_len".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(0)),
                target: BlockId(3),
                unwind: None,
            },
            span: span0,
        };
        let bb3 = MirBlock {
            id: BlockId(3),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2, bb3])
    }

    /// Fixture: `buf = __cobrust_str_new(); __cobrust_fmt_bool(buf, 1);
    /// _return = __cobrust_str_len(buf)`. Expected exit: 4 (length of
    /// "True"). Verifies fmt_bool ABI (i64 boolean tag widened from MIR
    /// i1 via the wave-2 widening path at `llvm_backend.rs:2522-2541`).
    fn build_fmt_bool_then_str_len() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_buf".to_string(),
                ty: Ty::Str,
                mutable: true,
                span: span0,
                validated_body_of: None,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_fmt_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
                validated_body_of: None,
            },
        ];
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
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_fmt_bool".to_string())),
                args: vec![
                    Operand::Copy(Place::local(LocalId(1))),
                    Operand::Constant(Constant::Int(1)),
                ],
                destination: Place::local(LocalId(2)),
                target: BlockId(2),
                unwind: None,
            },
            span: span0,
        };
        let bb2 = MirBlock {
            id: BlockId(2),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_len".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(0)),
                target: BlockId(3),
                unwind: None,
            },
            span: span0,
        };
        let bb3 = MirBlock {
            id: BlockId(3),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2, bb3])
    }

    /// Fixture: `buf = __cobrust_str_new(); __cobrust_fmt_str(buf, "hi", 2);
    /// _return = __cobrust_str_len(buf)`. Expected exit: 2. The string
    /// literal arg triggers the wave-2 `expand_trailing_str_len` path
    /// (3-param fmt_str sig, source supplies (buf, "hi") → emit
    /// (buf, ptr, len)). Verifies the trailing-str expansion is wired
    /// for fmt_str specifically (matches fmt-builtin codegen path).
    fn build_fmt_str_then_str_len() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_buf".to_string(),
                ty: Ty::Str,
                mutable: true,
                span: span0,
                validated_body_of: None,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_fmt_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
                validated_body_of: None,
            },
        ];
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
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_fmt_str".to_string())),
                args: vec![
                    Operand::Copy(Place::local(LocalId(1))),
                    Operand::Constant(Constant::Str("hi".to_string())),
                ],
                destination: Place::local(LocalId(2)),
                target: BlockId(2),
                unwind: None,
            },
            span: span0,
        };
        let bb2 = MirBlock {
            id: BlockId(2),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_len".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(0)),
                target: BlockId(3),
                unwind: None,
            },
            span: span0,
        };
        let bb3 = MirBlock {
            id: BlockId(3),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2, bb3])
    }

    // ====================================================================
    // iter family fixture
    // ====================================================================

    /// Fixture: `h = __cobrust_iter_init(0); _return = __cobrust_iter_next(h);
    /// __cobrust_iter_drop(h)`. Per stdlib `iter.rs:501-509`, `iter_init(0)`
    /// is the empty-list sentinel; `iter_next` returns 0 immediately
    /// (end-of-iter). Expected exit: 0. Verifies the three-helper chain
    /// dispatches cleanly through the wave-5 LLVM hookup.
    ///
    /// The handle local is declared `Ty::Str` so the alloca lowers to
    /// `opaque_ptr_ty` (matches wave-4 `input_str_buf` fixture pattern at
    /// `llvm_wave3_input_readline.rs:378-393`: round-trip through a ptr-
    /// typed alloca is required for `__cobrust_iter_next(ptr) -> i64` /
    /// `__cobrust_iter_drop(ptr) -> ()`, both of which expect a ptr arg).
    fn build_iter_init_next_drop_empty() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_handle".to_string(),
                ty: Ty::Str, // ptr-typed alloca (Str lowers to opaque_ptr_ty)
                mutable: true,
                span: span0,
                validated_body_of: None,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_drop_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
                validated_body_of: None,
            },
        ];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_iter_init".to_string())),
                args: vec![Operand::Constant(Constant::Int(0))],
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
                func: Operand::Constant(Constant::Str("__cobrust_iter_next".to_string())),
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
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_iter_drop".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(2)),
                target: BlockId(3),
                unwind: None,
            },
            span: span0,
        };
        let bb3 = MirBlock {
            id: BlockId(3),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2, bb3])
    }

    // ====================================================================
    // math family fixtures
    // ====================================================================

    /// Build `_return = (i64) __cobrust_math_<sym>(<arg_bits>)`. Single-arg
    /// f64 → f64 helper followed by FloatToInt cast to i64.
    fn build_math_unary_chain(sym: &'static str, arg_bits: u64) -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![LocalDecl {
            id: LocalId(1),
            name: "_fval".to_string(),
            ty: Ty::Float,
            mutable: true,
            span: span0,
            validated_body_of: None,
        }];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str(sym.to_string())),
                args: vec![Operand::Constant(Constant::Float(arg_bits))],
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
                    rvalue: Rvalue::Cast(
                        CastKind::FloatToInt,
                        Operand::Copy(Place::local(LocalId(1))),
                        Ty::Int,
                    ),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1])
    }

    /// Build `_return = (i64) __cobrust_math_pow(<base_bits>, <exp_bits>)`.
    fn build_math_pow_chain(base_bits: u64, exp_bits: u64) -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![LocalDecl {
            id: LocalId(1),
            name: "_fval".to_string(),
            ty: Ty::Float,
            mutable: true,
            span: span0,
            validated_body_of: None,
        }];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_math_pow".to_string())),
                args: vec![
                    Operand::Constant(Constant::Float(base_bits)),
                    Operand::Constant(Constant::Float(exp_bits)),
                ],
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
                    rvalue: Rvalue::Cast(
                        CastKind::FloatToInt,
                        Operand::Copy(Place::local(LocalId(1))),
                        Ty::Int,
                    ),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1])
    }

    // ====================================================================
    // parse_int + str-parsing fixtures
    // ====================================================================

    /// Fixture: `_return = __cobrust_parse_int("42")`. The single-arg
    /// Str-literal call routes through the wave-2 `materialize_str_buffer`
    /// path (1-param signature → no expansion, materialize literal as a
    /// Str buffer ptr). Expected exit: 42.
    fn build_parse_int_42() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_parse_int".to_string())),
                args: vec![Operand::Constant(Constant::Str("42".to_string()))],
                destination: Place::local(LocalId(0)),
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
        make_main(vec![], vec![bb0, bb1])
    }

    /// Fixture: `_return = __cobrust_str_ord("A")`. ASCII value of 'A' is
    /// 65. Verifies the str_ord 1-param sig + i64 return path.
    fn build_str_ord_uppercase_a() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_ord".to_string())),
                args: vec![Operand::Constant(Constant::Str("A".to_string()))],
                destination: Place::local(LocalId(0)),
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
        make_main(vec![], vec![bb0, bb1])
    }

    /// Fixture: `_return = __cobrust_count_toks("a b c")`. Three
    /// whitespace-separated tokens. Verifies the count_toks 1-param sig.
    fn build_count_toks_three() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_count_toks".to_string())),
                args: vec![Operand::Constant(Constant::Str("a b c".to_string()))],
                destination: Place::local(LocalId(0)),
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
        make_main(vec![], vec![bb0, bb1])
    }

    // ====================================================================
    // str-method family fixtures
    // ====================================================================

    /// Fixture: `s = __cobrust_str_lower("ABC"); _return = __cobrust_str_len(s)`.
    /// Lower("ABC") → "abc" (3 bytes). Verifies str_lower's 1-param
    /// Str-return ABI + chains into str_len for a single observable exit.
    fn build_str_lower_then_len() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![LocalDecl {
            id: LocalId(1),
            name: "_lowered".to_string(),
            ty: Ty::Str,
            mutable: true,
            span: span0,
            validated_body_of: None,
        }];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_lower".to_string())),
                args: vec![Operand::Constant(Constant::Str("ABC".to_string()))],
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

    /// Fixture: `_return = __cobrust_str_contains("hello", "ell")`. Expected
    /// exit: 1 (substring present). Verifies the 2-param Str-args + i64
    /// return predicate path.
    fn build_str_contains_present() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_contains".to_string())),
                args: vec![
                    Operand::Constant(Constant::Str("hello".to_string())),
                    Operand::Constant(Constant::Str("ell".to_string())),
                ],
                destination: Place::local(LocalId(0)),
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
        make_main(vec![], vec![bb0, bb1])
    }

    /// Fixture: `_return = __cobrust_str_find("hello", "ll")`. Expected
    /// exit: 2 (index of "ll" in "hello"). Verifies find's i64 return
    /// (positive case; -1 sentinel deferred since Unix exit code is
    /// unsigned 0-255).
    fn build_str_find_present() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_find".to_string())),
                args: vec![
                    Operand::Constant(Constant::Str("hello".to_string())),
                    Operand::Constant(Constant::Str("ll".to_string())),
                ],
                destination: Place::local(LocalId(0)),
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
        make_main(vec![], vec![bb0, bb1])
    }

    /// Fixture: `_return = __cobrust_str_starts_with("hello", "he")`.
    /// Expected exit: 1. Verifies the predicate-fn ABI for
    /// starts_with (parallel surface to contains / ends_with).
    fn build_str_starts_with_true() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_starts_with".to_string())),
                args: vec![
                    Operand::Constant(Constant::Str("hello".to_string())),
                    Operand::Constant(Constant::Str("he".to_string())),
                ],
                destination: Place::local(LocalId(0)),
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
        make_main(vec![], vec![bb0, bb1])
    }

    // ====================================================================
    // #[test] entry points
    // ====================================================================

    /// fmt sub-wave-5 fixture A: `fmt_int(buf, 42)` then `str_len(buf) == 2`.
    #[test]
    fn llvm_emits_fmt_int_then_str_len() {
        let Some((status, stdout, stderr)) =
            link_and_run("fmt_int_then_str_len", &build_fmt_int_then_str_len())
        else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 2,
            "fmt_int_then_str_len: expected exit 2, got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// fmt sub-wave-5 fixture B: `fmt_bool(buf, 1)` then `str_len(buf) == 4`
    /// (length of "True").
    #[test]
    fn llvm_emits_fmt_bool_then_str_len() {
        let Some((status, stdout, stderr)) =
            link_and_run("fmt_bool_then_str_len", &build_fmt_bool_then_str_len())
        else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 4,
            "fmt_bool_then_str_len: expected exit 4 (len of \"True\"), got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// fmt sub-wave-5 fixture C: `fmt_str(buf, "hi")` then `str_len(buf) == 2`.
    #[test]
    fn llvm_emits_fmt_str_then_str_len() {
        let Some((status, stdout, stderr)) =
            link_and_run("fmt_str_then_str_len", &build_fmt_str_then_str_len())
        else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 2,
            "fmt_str_then_str_len: expected exit 2, got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// iter sub-wave-5 fixture: `iter_init(0) → iter_next → iter_drop`,
    /// empty-iter chain. Verifies all three iter helpers dispatch
    /// cleanly. Expected exit 0 (end-of-iter sentinel).
    #[test]
    fn llvm_emits_iter_init_next_drop_empty() {
        let Some((status, stdout, stderr)) = link_and_run(
            "iter_init_next_drop_empty",
            &build_iter_init_next_drop_empty(),
        ) else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 0,
            "iter_init_next_drop_empty: expected exit 0 (empty-iter sentinel), got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// math sub-wave-5 fixture A: `(i64) sqrt(16.0) == 4`.
    #[test]
    fn llvm_emits_math_sqrt_16() {
        let Some((status, stdout, stderr)) = link_and_run(
            "math_sqrt_16",
            &build_math_unary_chain("__cobrust_math_sqrt", 16.0_f64.to_bits()),
        ) else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 4,
            "math_sqrt_16: expected exit 4, got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// math sub-wave-5 fixture B: `(i64) abs(-7.0) == 7`. Verifies the
    /// single-arg f64 chain for a different intrinsic than sqrt.
    #[test]
    fn llvm_emits_math_abs_neg7() {
        let Some((status, stdout, stderr)) = link_and_run(
            "math_abs_neg7",
            &build_math_unary_chain("__cobrust_math_abs", (-7.0_f64).to_bits()),
        ) else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 7,
            "math_abs_neg7: expected exit 7, got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// math sub-wave-5 fixture C: `(i64) pow(2.0, 3.0) == 8`. Verifies the
    /// 2-arg f64 × f64 → f64 path.
    #[test]
    fn llvm_emits_math_pow_2_3() {
        let Some((status, stdout, stderr)) = link_and_run(
            "math_pow_2_3",
            &build_math_pow_chain(2.0_f64.to_bits(), 3.0_f64.to_bits()),
        ) else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 8,
            "math_pow_2_3: expected exit 8, got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// parse_int sub-wave-5 fixture: `parse_int("42") == 42`.
    #[test]
    fn llvm_emits_parse_int_42() {
        let Some((status, stdout, stderr)) = link_and_run("parse_int_42", &build_parse_int_42())
        else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 42,
            "parse_int_42: expected exit 42, got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// str-parsing sub-wave-5 fixture: `str_ord("A") == 65`.
    #[test]
    fn llvm_emits_str_ord_uppercase_a() {
        let Some((status, stdout, stderr)) =
            link_and_run("str_ord_uppercase_a", &build_str_ord_uppercase_a())
        else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 65,
            "str_ord_uppercase_a: expected exit 65 (ASCII 'A'), got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// str-parsing sub-wave-5 fixture: `count_toks("a b c") == 3`.
    #[test]
    fn llvm_emits_count_toks_three() {
        let Some((status, stdout, stderr)) =
            link_and_run("count_toks_three", &build_count_toks_three())
        else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 3,
            "count_toks_three: expected exit 3, got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// str-method sub-wave-5 fixture A: `str_len(str_lower("ABC")) == 3`.
    #[test]
    fn llvm_emits_str_lower_then_len() {
        let Some((status, stdout, stderr)) =
            link_and_run("str_lower_then_len", &build_str_lower_then_len())
        else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 3,
            "str_lower_then_len: expected exit 3, got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// str-method sub-wave-5 fixture B: `str_contains("hello", "ell") == 1`.
    #[test]
    fn llvm_emits_str_contains_present() {
        let Some((status, stdout, stderr)) =
            link_and_run("str_contains_present", &build_str_contains_present())
        else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 1,
            "str_contains_present: expected exit 1, got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// str-method sub-wave-5 fixture C: `str_find("hello", "ll") == 2`.
    #[test]
    fn llvm_emits_str_find_present() {
        let Some((status, stdout, stderr)) =
            link_and_run("str_find_present", &build_str_find_present())
        else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 2,
            "str_find_present: expected exit 2, got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// str-method sub-wave-5 fixture D: `str_starts_with("hello", "he") == 1`.
    #[test]
    fn llvm_emits_str_starts_with_true() {
        let Some((status, stdout, stderr)) =
            link_and_run("str_starts_with_true", &build_str_starts_with_true())
        else {
            return;
        };
        let code = status.code().unwrap_or_default();
        assert_eq!(
            code, 1,
            "str_starts_with_true: expected exit 1, got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
    }
}
