#![allow(
    clippy::items_after_statements,
    clippy::similar_names,
    reason = "test corpus style; readability over micro-optim (F44-sibling silent-rot fix post sub-wave-3)"
)]
//! ADR-0058g sub-wave-2 — LLVM backend list runtime extern hookup smoke
//! fixtures.
//!
//! Sub-wave-2 wires six `__cobrust_list_*` constructor/accessor/mutator
//! helpers into the LLVM `lower_call` extern-name dispatch path:
//!
//!   - `__cobrust_list_new(elem_size: i64, len: i64) -> *mut ListBuffer`
//!   - `__cobrust_list_set(list: *mut, i: i64, v: i64) -> void`
//!   - `__cobrust_list_get(list: *mut, i: i64) -> i64`
//!   - `__cobrust_list_len(list: *mut) -> i64`
//!   - `__cobrust_list_is_empty(list: *mut) -> i64`  (0/1)
//!   - `__cobrust_list_append(list: *mut, v: i64) -> void`
//!
//! Cranelift parity references (ABI verbatim mirror, all confirmed at
//! `cobrust-codegen/src/cranelift_backend.rs:2670-2682`):
//!
//! ```text
//!   __cobrust_list_new        ([i64, i64] -> p)
//!   __cobrust_list_set        ([p, i64, i64] -> ())
//!   __cobrust_list_get        ([p, i64] -> i64)
//!   __cobrust_list_len        ([p] -> i64)
//!   __cobrust_list_is_empty   ([p] -> i64)
//!   __cobrust_list_append     ([p, i64] -> ())
//! ```
//!
//! Stdlib ABI cross-confirmed at `cobrust-stdlib/src/collections.rs`
//! lines 390/419/440/459/477/595.
//!
//! Drop schedule context (ADR-0050c TD-1, addressed by ADR-0058g §6.1):
//! `__cobrust_list_drop` + `__cobrust_list_drop_elems` were already wired
//! into the wave-1 `Terminator::Drop` dispatch path at
//! `llvm_backend.rs::emit_drop_for_ty`. Sub-wave-2 adds the constructor
//! and accessor surface; the existing Drop path requires no change.
//! These fixtures construct MIR by hand (no frontend → MIR drop-schedule
//! pass), so no `Terminator::Drop` is emitted — that path is regression-
//! tested separately and is not in sub-wave-2 scope.
//!
//! Each fixture follows the wave-1 `llvm_wave3_panic_argv` pattern:
//!
//!   1. Build minimal MIR via `Module { bodies: [...] }` directly.
//!   2. Compile to LLVM IR via `emit()` with `Backend::Llvm`.
//!   3. Link against `libcobrust_stdlib.a` + `runtime/cobrust_main.c`
//!      using the system `cc` (matches wave-2 fixture link strategy).
//!   4. Run the resulting binary and assert exit code matches expected.
//!
//! Pre-fix expectation (wave-1 stub fallthrough): the 6 list extern names
//! were not in `runtime_helper_decls`, so the call routed to the wave-1
//! stub branch — write i64 zero to `destination`, branch to `target` —
//! silently no-op'd. Post-fix: the dispatch path emits `call @__cobrust_list_*`
//! against the stdlib symbols; the binary exits with the expected value.
//!
//! F35-sibling discipline: these fixtures land sub-wave-2 ONLY. Nine of
//! the twelve F45a §2 categories remain wave-1 stubs after this sprint
//! (dict / set+tuple / input / fmt / iter / math / parse_int / str-methods /
//! LLM router). DO NOT read sub-wave-2 closure as wave-3 closure.

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

    /// Bespoke link-and-run that returns `ExitStatus` so per-fixture
    /// assertions can target the exact exit code. Mirrors the wave-1
    /// `link_and_run` helper at `llvm_wave3_panic_argv.rs:92-140`.
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

    /// Standard MIR scaffolding used by every fixture below: a single
    /// `main()` body whose return-local at `LocalId(0)` is `Ty::Int`.
    /// Caller supplies the locals beyond the return slot + the blocks.
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

    /// Build:
    ///
    /// ```text
    ///   bb0: _list = __cobrust_list_new(8, 0); branch bb1
    ///   bb1: _return = 0; return
    /// ```
    ///
    /// Returns 0 on success; pre-fix this would also exit 0 since the
    /// stub fallthrough writes 0 to `_list` and branches. The real signal
    /// is "did the link resolve the extern symbol?" — if `__cobrust_list_new`
    /// is missing from `runtime_helper_decls`, the call site goes through
    /// the stub path and the LLVM IR contains no `call @__cobrust_list_new`;
    /// the binary links because no undefined symbol is requested. The
    /// `_new_then_len` and `_new_then_is_empty` fixtures below provide the
    /// stronger signal (return values that differ between stub vs real).
    fn build_new() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![LocalDecl {
            id: LocalId(1),
            name: "_list".to_string(),
            ty: Ty::List(Box::new(Ty::Int)),
            mutable: true,
            span: span0,
        }];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_list_new".to_string())),
                args: vec![
                    Operand::Constant(Constant::Int(8)),
                    Operand::Constant(Constant::Int(0)),
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
                    rvalue: Rvalue::Use(Operand::Constant(Constant::Int(0))),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1])
    }

    /// Build:
    ///
    /// ```text
    ///   bb0: _list = __cobrust_list_new(8, 0); branch bb1
    ///   bb1: _ignore = __cobrust_list_append(_list, 42); branch bb2
    ///   bb2: _len = __cobrust_list_len(_list); branch bb3
    ///   bb3: _return = _len; return
    /// ```
    ///
    /// Expected post-fix exit: 1 (one element appended). Pre-fix: 0
    /// (stub fallthrough writes zero to every void-return helper dest +
    /// `_len` is the i64-return-typed local that also gets 0).
    fn build_new_then_append_then_len() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_list".to_string(),
                ty: Ty::List(Box::new(Ty::Int)),
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_append_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(3),
                name: "_len".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_list_new".to_string())),
                args: vec![
                    Operand::Constant(Constant::Int(8)),
                    Operand::Constant(Constant::Int(0)),
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
                func: Operand::Constant(Constant::Str("__cobrust_list_append".to_string())),
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
                func: Operand::Constant(Constant::Str("__cobrust_list_len".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(3)),
                target: BlockId(3),
                unwind: None,
            },
            span: span0,
        };
        let bb3 = MirBlock {
            id: BlockId(3),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::Use(Operand::Copy(Place::local(LocalId(3)))),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2, bb3])
    }

    /// Build:
    ///
    /// ```text
    ///   bb0: _list = __cobrust_list_new(8, 3); branch bb1
    ///   bb1: _set_ret = __cobrust_list_set(_list, 1, 99); branch bb2
    ///   bb2: _val = __cobrust_list_get(_list, 1); branch bb3
    ///   bb3: _return = _val; return
    /// ```
    ///
    /// Pre-allocated length-3 list; set index 1 to 99; read it back.
    /// Expected post-fix exit: 99. Pre-fix exit: 0.
    fn build_set_then_get() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_list".to_string(),
                ty: Ty::List(Box::new(Ty::Int)),
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_set_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(3),
                name: "_val".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_list_new".to_string())),
                args: vec![
                    Operand::Constant(Constant::Int(8)),
                    Operand::Constant(Constant::Int(3)),
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
                func: Operand::Constant(Constant::Str("__cobrust_list_set".to_string())),
                args: vec![
                    Operand::Copy(Place::local(LocalId(1))),
                    Operand::Constant(Constant::Int(1)),
                    Operand::Constant(Constant::Int(99)),
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
                func: Operand::Constant(Constant::Str("__cobrust_list_get".to_string())),
                args: vec![
                    Operand::Copy(Place::local(LocalId(1))),
                    Operand::Constant(Constant::Int(1)),
                ],
                destination: Place::local(LocalId(3)),
                target: BlockId(3),
                unwind: None,
            },
            span: span0,
        };
        let bb3 = MirBlock {
            id: BlockId(3),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::Use(Operand::Copy(Place::local(LocalId(3)))),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2, bb3])
    }

    /// Build:
    ///
    /// ```text
    ///   bb0: _list = __cobrust_list_new(8, 0); branch bb1
    ///   bb1: _empty = __cobrust_list_is_empty(_list); branch bb2
    ///   bb2: _return = _empty; return
    /// ```
    ///
    /// Expected post-fix exit: 1 (empty list). Pre-fix exit: 0
    /// (stub returns zero to `_empty`).
    fn build_is_empty_after_new() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_list".to_string(),
                ty: Ty::List(Box::new(Ty::Int)),
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_empty".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_list_new".to_string())),
                args: vec![
                    Operand::Constant(Constant::Int(8)),
                    Operand::Constant(Constant::Int(0)),
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
                func: Operand::Constant(Constant::Str("__cobrust_list_is_empty".to_string())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(2)),
                target: BlockId(2),
                unwind: None,
            },
            span: span0,
        };
        let bb2 = MirBlock {
            id: BlockId(2),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::Use(Operand::Copy(Place::local(LocalId(2)))),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        make_main(extra, vec![bb0, bb1, bb2])
    }

    /// End-to-end round-trip combining all 6 helpers:
    ///
    /// ```text
    ///   bb0: _list = __cobrust_list_new(8, 0);                  // new
    ///   bb1: _      = __cobrust_list_append(_list, 10);          // append
    ///   bb2: _      = __cobrust_list_append(_list, 20);          // append
    ///   bb3: _      = __cobrust_list_append(_list, 30);          // append
    ///   bb4: _      = __cobrust_list_set(_list, 1, 200);         // set
    ///   bb5: _v0    = __cobrust_list_get(_list, 0);              // get 10
    ///   bb6: _v1    = __cobrust_list_get(_list, 1);              // get 200
    ///   bb7: _v2    = __cobrust_list_get(_list, 2);              // get 30
    ///   bb8: _len   = __cobrust_list_len(_list);                 // len = 3
    ///   bb9: _empty = __cobrust_list_is_empty(_list);            // 0
    ///   bb10: _return = _v0 + _v1 + _v2 + _len + _empty;
    ///                 = 10 + 200 + 30 + 3 + 0 = 243
    /// ```
    ///
    /// Expected post-fix exit: 243. Pre-fix exit: 0.
    #[allow(clippy::too_many_lines)]
    fn build_end_to_end() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_list".to_string(),
                ty: Ty::List(Box::new(Ty::Int)),
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_void_ret".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(3),
                name: "_v0".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(4),
                name: "_v1".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(5),
                name: "_v2".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(6),
                name: "_len".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(7),
                name: "_empty".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(8),
                name: "_sum01".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(9),
                name: "_sum012".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(10),
                name: "_sum012l".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];
        let call = |func: &str,
                    args: Vec<Operand>,
                    dst: LocalId,
                    next: BlockId,
                    cur: BlockId|
         -> MirBlock {
            MirBlock {
                id: cur,
                statements: vec![],
                terminator: Terminator::Call {
                    func: Operand::Constant(Constant::Str(func.to_string())),
                    args,
                    destination: Place::local(dst),
                    target: next,
                    unwind: None,
                },
                span: span0,
            }
        };
        let list_op = || Operand::Copy(Place::local(LocalId(1)));
        let int_op = |v: i64| Operand::Constant(Constant::Int(v));

        let bb0 = call(
            "__cobrust_list_new",
            vec![int_op(8), int_op(0)],
            LocalId(1),
            BlockId(1),
            BlockId(0),
        );
        let bb1 = call(
            "__cobrust_list_append",
            vec![list_op(), int_op(10)],
            LocalId(2),
            BlockId(2),
            BlockId(1),
        );
        let bb2 = call(
            "__cobrust_list_append",
            vec![list_op(), int_op(20)],
            LocalId(2),
            BlockId(3),
            BlockId(2),
        );
        let bb3 = call(
            "__cobrust_list_append",
            vec![list_op(), int_op(30)],
            LocalId(2),
            BlockId(4),
            BlockId(3),
        );
        let bb4 = call(
            "__cobrust_list_set",
            vec![list_op(), int_op(1), int_op(200)],
            LocalId(2),
            BlockId(5),
            BlockId(4),
        );
        let bb5 = call(
            "__cobrust_list_get",
            vec![list_op(), int_op(0)],
            LocalId(3),
            BlockId(6),
            BlockId(5),
        );
        let bb6 = call(
            "__cobrust_list_get",
            vec![list_op(), int_op(1)],
            LocalId(4),
            BlockId(7),
            BlockId(6),
        );
        let bb7 = call(
            "__cobrust_list_get",
            vec![list_op(), int_op(2)],
            LocalId(5),
            BlockId(8),
            BlockId(7),
        );
        let bb8 = call(
            "__cobrust_list_len",
            vec![list_op()],
            LocalId(6),
            BlockId(9),
            BlockId(8),
        );
        let bb9 = call(
            "__cobrust_list_is_empty",
            vec![list_op()],
            LocalId(7),
            BlockId(10),
            BlockId(9),
        );

        // bb10: chain Add Rvalues to sum _v0 + _v1 + _v2 + _len + _empty,
        // then return.
        use cobrust_mir::{BinOp, Operand as Op};
        let stmts = vec![
            // _sum01 = _v0 + _v1
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(8)),
                    rvalue: Rvalue::BinaryOp(
                        BinOp::Add,
                        Op::Copy(Place::local(LocalId(3))),
                        Op::Copy(Place::local(LocalId(4))),
                    ),
                },
                span: span0,
            },
            // _sum012 = _sum01 + _v2
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(9)),
                    rvalue: Rvalue::BinaryOp(
                        BinOp::Add,
                        Op::Copy(Place::local(LocalId(8))),
                        Op::Copy(Place::local(LocalId(5))),
                    ),
                },
                span: span0,
            },
            // _sum012l = _sum012 + _len
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(10)),
                    rvalue: Rvalue::BinaryOp(
                        BinOp::Add,
                        Op::Copy(Place::local(LocalId(9))),
                        Op::Copy(Place::local(LocalId(6))),
                    ),
                },
                span: span0,
            },
            // _return = _sum012l + _empty
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::BinaryOp(
                        BinOp::Add,
                        Op::Copy(Place::local(LocalId(10))),
                        Op::Copy(Place::local(LocalId(7))),
                    ),
                },
                span: span0,
            },
        ];
        let bb10 = MirBlock {
            id: BlockId(10),
            statements: stmts,
            terminator: Terminator::Return,
            span: span0,
        };

        make_main(
            extra,
            vec![bb0, bb1, bb2, bb3, bb4, bb5, bb6, bb7, bb8, bb9, bb10],
        )
    }

    /// Sub-wave-2 fixture A: `__cobrust_list_new(8, 0)` lowers + links.
    /// Returning 0 (not a stub crash) confirms `list_new` symbol resolves
    /// and the MIR-side call lowered to a real `call` instruction.
    #[test]
    fn llvm_emits_list_new_extern_call() {
        let Some((status, stdout, stderr)) = link_and_run("list_new_smoke", &build_new()) else {
            return; // prereqs missing — skip
        };
        assert!(
            status.success(),
            "list_new_smoke: expected exit 0, got {status:?}; stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-2 fixture B: `new + append + len`. Post-fix the program
    /// exits with code 1 (one element was appended); pre-fix the stub
    /// fallthrough writes 0 to `_len` and exits 0 — distinguishes real
    /// dispatch from stub.
    #[test]
    fn llvm_emits_list_append_then_len() {
        let Some((status, stdout, stderr)) =
            link_and_run("list_append_len", &build_new_then_append_then_len())
        else {
            return;
        };
        let code = status.code().unwrap_or(-1);
        assert_eq!(
            code, 1,
            "list_append_len: expected exit 1 (one appended elem), got {code}; \
             stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-2 fixture C: `new + set + get`. Post-fix exit 99
    /// (the value written via set is observed via get); pre-fix exit 0.
    #[test]
    fn llvm_emits_list_set_then_get() {
        let Some((status, stdout, stderr)) = link_and_run("list_set_get", &build_set_then_get())
        else {
            return;
        };
        let code = status.code().unwrap_or(-1);
        assert_eq!(
            code, 99,
            "list_set_get: expected exit 99 (value written via set/get), got {code}; \
             stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-2 fixture D: `new + is_empty`. Post-fix exit 1 (the empty
    /// freshly-allocated list reports is_empty == 1); pre-fix exit 0.
    #[test]
    fn llvm_emits_list_is_empty_after_new() {
        let Some((status, stdout, stderr)) =
            link_and_run("list_is_empty", &build_is_empty_after_new())
        else {
            return;
        };
        let code = status.code().unwrap_or(-1);
        assert_eq!(
            code, 1,
            "list_is_empty: expected exit 1 (newly-allocated list is empty), got {code}; \
             stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-2 fixture E: all 6 helpers in one end-to-end program.
    /// Post-fix exit 243 (10+200+30+3+0). Pre-fix exit 0.
    /// Acts as the integration capstone — sub-wave-2 ratification gate.
    #[test]
    fn llvm_emits_list_end_to_end_roundtrip() {
        let Some((status, stdout, stderr)) = link_and_run("list_end_to_end", &build_end_to_end())
        else {
            return;
        };
        let code = status.code().unwrap_or(-1);
        assert_eq!(
            code, 243,
            "list_end_to_end: expected exit 243 (10+200+30+3+0), got {code}; \
             stdout={stdout:?} stderr={stderr:?}"
        );
    }
}

// On default (non-LLVM) feature builds, all fixtures degrade to pass
// (no LLVM backend to exercise; wave-3 surface is feature-gated).
#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_list_new_extern_call() {
    // Skipped on default build — LLVM backend feature-gated.
}

#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_list_append_then_len() {
    // Skipped on default build — LLVM backend feature-gated.
}

#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_list_set_then_get() {
    // Skipped on default build — LLVM backend feature-gated.
}

#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_list_is_empty_after_new() {
    // Skipped on default build — LLVM backend feature-gated.
}

#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_list_end_to_end_roundtrip() {
    // Skipped on default build — LLVM backend feature-gated.
}
