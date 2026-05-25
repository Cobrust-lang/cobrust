//! ADR-0058g sub-wave-3 — LLVM backend dict + set + tuple runtime extern
//! hookup smoke fixtures.
//!
//! Sub-wave-3 wires the dict + set + tuple runtime surface into the LLVM
//! `lower_call` extern-name dispatch path AND extends `emit_drop_for_ty`
//! to call `__cobrust_dict_drop` on `Ty::Dict(_, _)` (parity with
//! Cranelift `cranelift_backend.rs:1232-1237`; closes ADR-0058g §6.1
//! TD-1 dict portion):
//!
//! Dict (16 externs total):
//!   - `__cobrust_dict_new(i64, i64, i64) -> *mut`
//!   - `__cobrust_dict_drop(*mut) -> void`     (also fires from `emit_drop_for_ty`)
//!   - `__cobrust_dict_len(*mut) -> i64`
//!   - `__cobrust_dict_is_empty(*mut) -> i64`  (0/1)
//!   - `__cobrust_dict_set` + `__cobrust_dict_get` (legacy untyped i64,i64)
//!   - 10 typed (K, V) shims: `_set_K_V` / `_get_K_V` / `_contains_K`
//!     across {i64, str} × {i64, str}
//!
//! Set<i64> (5 externs):
//!   - `__cobrust_set_new(i64, i64) -> *mut`
//!   - `__cobrust_set_insert(*mut, i64) -> void`
//!   - `__cobrust_set_contains(*mut, i64) -> i64`  (0/1)
//!   - `__cobrust_set_len(*mut) -> i64`
//!   - `__cobrust_set_drop(*mut) -> void`
//!
//! Tuple (4 externs):
//!   - `__cobrust_tuple_new(i64) -> *mut`
//!   - `__cobrust_tuple_set(*mut, i64, i64) -> void`
//!   - `__cobrust_tuple_get(*mut, i64) -> i64`
//!   - `__cobrust_tuple_drop(*mut, i64) -> void`   (note: arity as 2nd arg)
//!
//! Cranelift parity references (ABI verbatim mirror, all confirmed at
//! `cobrust-codegen/src/cranelift_backend.rs:2684-2758`).
//!
//! Stdlib ABI cross-confirmed at `cobrust-stdlib/src/collections.rs`
//! lines 781-1359 (full dict + set + tuple ABI block).
//!
//! Drop schedule context (ADR-0050c TD-1; ADR-0058g §6.1 dict portion):
//! Cranelift `lower_drop` (`cranelift_backend.rs:1232-1241`) dispatches
//! `__cobrust_dict_drop` on `Ty::Dict(_, _)` but explicitly no-ops
//! `Ty::Set(_)` / `Ty::Tuple(_)` ("M12.x leaves these as no-op"). LLVM
//! sub-wave-3 matches: `emit_drop_for_ty` adds the `Ty::Dict` arm but
//! leaves Ty::Set / Ty::Tuple in the `_ => None` fallthrough. Phase G
//! widening lifts both backends together (out-of-scope this sprint).
//!
//! Each fixture follows the sub-wave-2 `llvm_wave3_list_runtime` pattern:
//!
//!   1. Build minimal MIR via `Module { bodies: [...] }` directly.
//!   2. Compile to LLVM IR via `emit()` with `Backend::Llvm`.
//!   3. Link against `libcobrust_stdlib.a` + `runtime/cobrust_main.c`
//!      using the system `cc` (matches sub-wave-2 fixture link strategy).
//!   4. Run the resulting binary and assert exit code matches expected.
//!
//! Pre-fix expectation (wave-1 stub fallthrough): the dict/set/tuple
//! extern names were not in `runtime_helper_decls`, so calls routed to
//! the wave-1 stub branch — write i64 zero to `destination`, branch to
//! `target` — silently no-op'd. Post-fix: each dispatch emits a real
//! `call @__cobrust_<family>_<op>` against stdlib symbols; the binary
//! exits with the expected value.
//!
//! F35-sibling discipline: these fixtures land sub-wave-3 ONLY. After
//! merge:
//!   - 5 of 12 F45a §2 categories RESOLVED (panic, argv, list, dict,
//!     set+tuple combined category).
//!   - 7 categories still wave-1 stub (input / fmt / iter / math /
//!     parse_int+str-parsing / str-methods / LLM-router).
//!   - DO NOT read sub-wave-3 closure as wave-3 closure.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::similar_names
)]

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

    fn llvm_spec(name: &str) -> TargetSpec {
        let dir =
            std::env::temp_dir().join(format!("cobrust-0058g-w3-{name}-{}", std::process::id()));
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

    /// Bespoke link-and-run that returns `ExitStatus` so per-fixture
    /// assertions can target the exact exit code. Mirrors the sub-wave-2
    /// `link_and_run` helper at `llvm_wave3_list_runtime.rs:115-163`.
    fn link_and_run(name: &str, module: &Module) -> Option<(ExitStatus, String, String)> {
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

        let out = Command::new(&exe).output().ok()?;
        Some((
            out.status,
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ))
    }

    /// Standard MIR scaffolding (mirrors sub-wave-2 helper): a single
    /// `main()` body whose return-local at `LocalId(0)` is `Ty::Int`.
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

    /// Convenience: build a `Terminator::Call` block.
    fn call_block(
        cur: BlockId,
        func: &str,
        args: Vec<Operand>,
        dst: LocalId,
        next: BlockId,
    ) -> MirBlock {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
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
    }

    /// Convenience: build a return-block that returns `_return` (assigned
    /// from the named local).
    fn return_from(cur: BlockId, src: LocalId) -> MirBlock {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        MirBlock {
            id: cur,
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::Use(Operand::Copy(Place::local(src))),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        }
    }

    fn int_op(v: i64) -> Operand {
        Operand::Constant(Constant::Int(v))
    }

    fn local_op(id: LocalId) -> Operand {
        Operand::Copy(Place::local(id))
    }

    // -----------------------------------------------------------------
    // Fixture A — `dict_new + dict_len + dict_is_empty` smoke
    // -----------------------------------------------------------------

    /// Build:
    ///
    /// ```text
    ///   bb0: _d     = __cobrust_dict_new(8, 8, 0); branch bb1
    ///   bb1: _len   = __cobrust_dict_len(_d);     branch bb2
    ///   bb2: _empty = __cobrust_dict_is_empty(_d); branch bb3
    ///   bb3: _return = _len + _empty; return     (post-fix: 0+1 = 1)
    /// ```
    ///
    /// Expected post-fix exit: 1 (empty dict reports len=0, is_empty=1).
    /// Pre-fix exit: 0 (stub fallthrough writes zero to every helper dest).
    fn build_dict_new_len_is_empty() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        use cobrust_mir::{BinOp, Operand as Op};

        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_d".to_string(),
                ty: Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Int)),
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_len".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(3),
                name: "_empty".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];

        let bb0 = call_block(
            BlockId(0),
            "__cobrust_dict_new",
            vec![int_op(8), int_op(8), int_op(0)],
            LocalId(1),
            BlockId(1),
        );
        let bb1 = call_block(
            BlockId(1),
            "__cobrust_dict_len",
            vec![local_op(LocalId(1))],
            LocalId(2),
            BlockId(2),
        );
        let bb2 = call_block(
            BlockId(2),
            "__cobrust_dict_is_empty",
            vec![local_op(LocalId(1))],
            LocalId(3),
            BlockId(3),
        );
        // bb3: _return = _len + _empty
        let bb3 = MirBlock {
            id: BlockId(3),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::BinaryOp(
                        BinOp::Add,
                        Op::Copy(Place::local(LocalId(2))),
                        Op::Copy(Place::local(LocalId(3))),
                    ),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };

        make_main(extra, vec![bb0, bb1, bb2, bb3])
    }

    // -----------------------------------------------------------------
    // Fixture B — `dict_set_i64_i64 + dict_get_i64_i64`
    // -----------------------------------------------------------------

    /// Build:
    ///
    /// ```text
    ///   bb0: _d  = __cobrust_dict_new(8, 8, 0);              branch bb1
    ///   bb1: _   = __cobrust_dict_set_i64_i64(_d, 7, 77);    branch bb2
    ///   bb2: _v  = __cobrust_dict_get_i64_i64(_d, 7);        branch bb3
    ///   bb3: _return = _v;                                    return  (77)
    /// ```
    ///
    /// Expected post-fix exit: 77. Pre-fix exit: 0.
    fn build_dict_set_get_i64_i64() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_d".to_string(),
                ty: Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Int)),
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_void".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(3),
                name: "_v".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];

        let bb0 = call_block(
            BlockId(0),
            "__cobrust_dict_new",
            vec![int_op(8), int_op(8), int_op(0)],
            LocalId(1),
            BlockId(1),
        );
        let bb1 = call_block(
            BlockId(1),
            "__cobrust_dict_set_i64_i64",
            vec![local_op(LocalId(1)), int_op(7), int_op(77)],
            LocalId(2),
            BlockId(2),
        );
        let bb2 = call_block(
            BlockId(2),
            "__cobrust_dict_get_i64_i64",
            vec![local_op(LocalId(1)), int_op(7)],
            LocalId(3),
            BlockId(3),
        );
        let bb3 = return_from(BlockId(3), LocalId(3));

        make_main(extra, vec![bb0, bb1, bb2, bb3])
    }

    // -----------------------------------------------------------------
    // Fixture C — `dict_set_i64_i64 + dict_contains_i64`
    // -----------------------------------------------------------------

    /// Build:
    ///
    /// ```text
    ///   bb0: _d  = __cobrust_dict_new(8, 8, 0);              branch bb1
    ///   bb1: _   = __cobrust_dict_set_i64_i64(_d, 42, 99);   branch bb2
    ///   bb2: _c  = __cobrust_dict_contains_i64(_d, 42);      branch bb3
    ///   bb3: _return = _c;                                    return  (1)
    /// ```
    ///
    /// Expected post-fix exit: 1 (the key was inserted). Pre-fix exit: 0.
    fn build_dict_contains() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_d".to_string(),
                ty: Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Int)),
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_void".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(3),
                name: "_c".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];

        let bb0 = call_block(
            BlockId(0),
            "__cobrust_dict_new",
            vec![int_op(8), int_op(8), int_op(0)],
            LocalId(1),
            BlockId(1),
        );
        let bb1 = call_block(
            BlockId(1),
            "__cobrust_dict_set_i64_i64",
            vec![local_op(LocalId(1)), int_op(42), int_op(99)],
            LocalId(2),
            BlockId(2),
        );
        let bb2 = call_block(
            BlockId(2),
            "__cobrust_dict_contains_i64",
            vec![local_op(LocalId(1)), int_op(42)],
            LocalId(3),
            BlockId(3),
        );
        let bb3 = return_from(BlockId(3), LocalId(3));

        make_main(extra, vec![bb0, bb1, bb2, bb3])
    }

    // -----------------------------------------------------------------
    // Fixture D — Set end-to-end: new + insert + contains + len
    // -----------------------------------------------------------------

    /// Build:
    ///
    /// ```text
    ///   bb0: _s  = __cobrust_set_new(8, 0);             branch bb1
    ///   bb1: _   = __cobrust_set_insert(_s, 11);        branch bb2
    ///   bb2: _   = __cobrust_set_insert(_s, 22);        branch bb3
    ///   bb3: _   = __cobrust_set_insert(_s, 11);  (dup) branch bb4
    ///   bb4: _c  = __cobrust_set_contains(_s, 11);      branch bb5
    ///   bb5: _len = __cobrust_set_len(_s);              branch bb6
    ///   bb6: _return = _c + _len;                       return  (1 + 2 = 3)
    /// ```
    ///
    /// Expected post-fix exit: 3 (contains=1, distinct count=2). Pre-fix: 0.
    fn build_set_end_to_end() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        use cobrust_mir::{BinOp, Operand as Op};

        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_s".to_string(),
                ty: Ty::Set(Box::new(Ty::Int)),
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_void".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(3),
                name: "_c".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(4),
                name: "_len".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];

        let bb0 = call_block(
            BlockId(0),
            "__cobrust_set_new",
            vec![int_op(8), int_op(0)],
            LocalId(1),
            BlockId(1),
        );
        let bb1 = call_block(
            BlockId(1),
            "__cobrust_set_insert",
            vec![local_op(LocalId(1)), int_op(11)],
            LocalId(2),
            BlockId(2),
        );
        let bb2 = call_block(
            BlockId(2),
            "__cobrust_set_insert",
            vec![local_op(LocalId(1)), int_op(22)],
            LocalId(2),
            BlockId(3),
        );
        let bb3 = call_block(
            BlockId(3),
            "__cobrust_set_insert",
            vec![local_op(LocalId(1)), int_op(11)],
            LocalId(2),
            BlockId(4),
        );
        let bb4 = call_block(
            BlockId(4),
            "__cobrust_set_contains",
            vec![local_op(LocalId(1)), int_op(11)],
            LocalId(3),
            BlockId(5),
        );
        let bb5 = call_block(
            BlockId(5),
            "__cobrust_set_len",
            vec![local_op(LocalId(1))],
            LocalId(4),
            BlockId(6),
        );
        // bb6: _return = _c + _len
        let bb6 = MirBlock {
            id: BlockId(6),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::BinaryOp(
                        BinOp::Add,
                        Op::Copy(Place::local(LocalId(3))),
                        Op::Copy(Place::local(LocalId(4))),
                    ),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };

        make_main(extra, vec![bb0, bb1, bb2, bb3, bb4, bb5, bb6])
    }

    // -----------------------------------------------------------------
    // Fixture E — Tuple end-to-end: new + set + get + drop
    // -----------------------------------------------------------------

    /// Build:
    ///
    /// ```text
    ///   bb0: _t = __cobrust_tuple_new(3);              branch bb1
    ///   bb1: _ = __cobrust_tuple_set(_t, 0, 100);       branch bb2
    ///   bb2: _ = __cobrust_tuple_set(_t, 1, 200);       branch bb3
    ///   bb3: _ = __cobrust_tuple_set(_t, 2, 50);        branch bb4
    ///   bb4: _v1 = __cobrust_tuple_get(_t, 1);          branch bb5
    ///   bb5: _v2 = __cobrust_tuple_get(_t, 2);          branch bb6
    ///   bb6: _ = __cobrust_tuple_drop(_t, 3);           branch bb7
    ///   bb7: _return = _v1 - _v2;                       return  (200 - 50 = 150)
    /// ```
    ///
    /// Expected post-fix exit: 150. Pre-fix exit: 0.
    fn build_tuple_end_to_end() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        use cobrust_mir::{BinOp, Operand as Op};

        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_t".to_string(),
                ty: Ty::Tuple(vec![Ty::Int, Ty::Int, Ty::Int]),
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_void".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(3),
                name: "_v1".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(4),
                name: "_v2".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];

        let bb0 = call_block(
            BlockId(0),
            "__cobrust_tuple_new",
            vec![int_op(3)],
            LocalId(1),
            BlockId(1),
        );
        let bb1 = call_block(
            BlockId(1),
            "__cobrust_tuple_set",
            vec![local_op(LocalId(1)), int_op(0), int_op(100)],
            LocalId(2),
            BlockId(2),
        );
        let bb2 = call_block(
            BlockId(2),
            "__cobrust_tuple_set",
            vec![local_op(LocalId(1)), int_op(1), int_op(200)],
            LocalId(2),
            BlockId(3),
        );
        let bb3 = call_block(
            BlockId(3),
            "__cobrust_tuple_set",
            vec![local_op(LocalId(1)), int_op(2), int_op(50)],
            LocalId(2),
            BlockId(4),
        );
        let bb4 = call_block(
            BlockId(4),
            "__cobrust_tuple_get",
            vec![local_op(LocalId(1)), int_op(1)],
            LocalId(3),
            BlockId(5),
        );
        let bb5 = call_block(
            BlockId(5),
            "__cobrust_tuple_get",
            vec![local_op(LocalId(1)), int_op(2)],
            LocalId(4),
            BlockId(6),
        );
        let bb6 = call_block(
            BlockId(6),
            "__cobrust_tuple_drop",
            vec![local_op(LocalId(1)), int_op(3)],
            LocalId(2),
            BlockId(7),
        );
        // bb7: _return = _v1 - _v2
        let bb7 = MirBlock {
            id: BlockId(7),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::BinaryOp(
                        BinOp::Sub,
                        Op::Copy(Place::local(LocalId(3))),
                        Op::Copy(Place::local(LocalId(4))),
                    ),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };

        make_main(extra, vec![bb0, bb1, bb2, bb3, bb4, bb5, bb6, bb7])
    }

    // -----------------------------------------------------------------
    // Fixture F — Dict end-to-end capstone: new + set + get + len +
    //             is_empty + contains + (Drop terminator → dict_drop).
    // -----------------------------------------------------------------

    /// Build a dict program that exercises every untyped + typed-i64-i64
    /// helper AND ends with a `Terminator::Drop` on the dict local so
    /// `emit_drop_for_ty`'s new `Ty::Dict(_,_) → __cobrust_dict_drop`
    /// arm fires.
    ///
    /// ```text
    ///   bb0: _d = __cobrust_dict_new(8, 8, 0);          branch bb1
    ///   bb1: _ = __cobrust_dict_set_i64_i64(_d, 1, 10); branch bb2
    ///   bb2: _ = __cobrust_dict_set_i64_i64(_d, 2, 20); branch bb3
    ///   bb3: _v1 = __cobrust_dict_get_i64_i64(_d, 1);   branch bb4
    ///   bb4: _v2 = __cobrust_dict_get_i64_i64(_d, 2);   branch bb5
    ///   bb5: _len = __cobrust_dict_len(_d);             branch bb6
    ///   bb6: _empty = __cobrust_dict_is_empty(_d);      branch bb7
    ///   bb7: _c = __cobrust_dict_contains_i64(_d, 1);   branch bb8
    ///   bb8: _return = _v1 + _v2 + _len + _empty + _c;
    ///                = 10 + 20 + 2 + 0 + 1 = 33
    ///       Drop _d  (Ty::Dict → __cobrust_dict_drop)
    /// ```
    ///
    /// Expected post-fix exit: 33. Pre-fix exit: 0.
    #[allow(clippy::too_many_lines)]
    fn build_dict_end_to_end_with_drop() -> Module {
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        use cobrust_mir::{BinOp, Operand as Op};

        let extra = vec![
            LocalDecl {
                id: LocalId(1),
                name: "_d".to_string(),
                ty: Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Int)),
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_void".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(3),
                name: "_v1".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(4),
                name: "_v2".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(5),
                name: "_len".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(6),
                name: "_empty".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(7),
                name: "_c".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(8),
                name: "_sum1".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(9),
                name: "_sum2".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(10),
                name: "_sum3".to_string(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];

        let bb0 = call_block(
            BlockId(0),
            "__cobrust_dict_new",
            vec![int_op(8), int_op(8), int_op(0)],
            LocalId(1),
            BlockId(1),
        );
        let bb1 = call_block(
            BlockId(1),
            "__cobrust_dict_set_i64_i64",
            vec![local_op(LocalId(1)), int_op(1), int_op(10)],
            LocalId(2),
            BlockId(2),
        );
        let bb2 = call_block(
            BlockId(2),
            "__cobrust_dict_set_i64_i64",
            vec![local_op(LocalId(1)), int_op(2), int_op(20)],
            LocalId(2),
            BlockId(3),
        );
        let bb3 = call_block(
            BlockId(3),
            "__cobrust_dict_get_i64_i64",
            vec![local_op(LocalId(1)), int_op(1)],
            LocalId(3),
            BlockId(4),
        );
        let bb4 = call_block(
            BlockId(4),
            "__cobrust_dict_get_i64_i64",
            vec![local_op(LocalId(1)), int_op(2)],
            LocalId(4),
            BlockId(5),
        );
        let bb5 = call_block(
            BlockId(5),
            "__cobrust_dict_len",
            vec![local_op(LocalId(1))],
            LocalId(5),
            BlockId(6),
        );
        let bb6 = call_block(
            BlockId(6),
            "__cobrust_dict_is_empty",
            vec![local_op(LocalId(1))],
            LocalId(6),
            BlockId(7),
        );
        let bb7 = call_block(
            BlockId(7),
            "__cobrust_dict_contains_i64",
            vec![local_op(LocalId(1)), int_op(1)],
            LocalId(7),
            BlockId(8),
        );

        // bb8: sum chain + Drop _d before Return.
        let bb8 = MirBlock {
            id: BlockId(8),
            statements: vec![
                // _sum1 = _v1 + _v2
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
                // _sum2 = _sum1 + _len
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
                // _sum3 = _sum2 + _empty
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
                // _return = _sum3 + _c
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
            ],
            terminator: Terminator::Drop {
                place: Place::local(LocalId(1)),
                target: BlockId(9),
            },
            span: span0,
        };
        // bb9: return
        let bb9 = MirBlock {
            id: BlockId(9),
            statements: vec![],
            terminator: Terminator::Return,
            span: span0,
        };

        make_main(
            extra,
            vec![bb0, bb1, bb2, bb3, bb4, bb5, bb6, bb7, bb8, bb9],
        )
    }

    // -----------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------

    /// Sub-wave-3 fixture A — dict new + len + is_empty.
    /// Post-fix exit 1 (empty dict: len=0 + is_empty=1).
    #[test]
    fn llvm_emits_dict_new_len_is_empty() {
        let Some((status, stdout, stderr)) =
            link_and_run("dict_new_len_is_empty", &build_dict_new_len_is_empty())
        else {
            return; // prereqs missing — skip
        };
        let code = status.code().unwrap_or(-1);
        assert_eq!(
            code, 1,
            "dict_new_len_is_empty: expected exit 1 (len=0 + is_empty=1), got {code}; \
             stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-3 fixture B — dict typed (i64, i64) set + get round-trip.
    /// Post-fix exit 77 (value written via `_set_i64_i64` observed via `_get_i64_i64`).
    #[test]
    fn llvm_emits_dict_set_then_get_i64_i64() {
        let Some((status, stdout, stderr)) =
            link_and_run("dict_set_get_i64_i64", &build_dict_set_get_i64_i64())
        else {
            return;
        };
        let code = status.code().unwrap_or(-1);
        assert_eq!(
            code, 77,
            "dict_set_get_i64_i64: expected exit 77, got {code}; \
             stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-3 fixture C — dict `_contains_i64` after `_set_i64_i64`.
    /// Post-fix exit 1; pre-fix 0.
    #[test]
    fn llvm_emits_dict_contains_after_set() {
        let Some((status, stdout, stderr)) = link_and_run("dict_contains", &build_dict_contains())
        else {
            return;
        };
        let code = status.code().unwrap_or(-1);
        assert_eq!(
            code, 1,
            "dict_contains: expected exit 1 (key was inserted), got {code}; \
             stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-3 fixture D — set new + insert × 3 (with one duplicate)
    /// + contains + len. Post-fix exit 3 (contains=1 + distinct count=2).
    #[test]
    fn llvm_emits_set_end_to_end() {
        let Some((status, stdout, stderr)) =
            link_and_run("set_end_to_end", &build_set_end_to_end())
        else {
            return;
        };
        let code = status.code().unwrap_or(-1);
        assert_eq!(
            code, 3,
            "set_end_to_end: expected exit 3 (contains=1 + distinct=2), got {code}; \
             stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-3 fixture E — tuple new + set × 3 + get × 2 + drop.
    /// Post-fix exit 150 (200 - 50). Verifies tuple_drop's 2-arg ABI
    /// (`p, n`) lowers correctly.
    #[test]
    fn llvm_emits_tuple_end_to_end() {
        let Some((status, stdout, stderr)) =
            link_and_run("tuple_end_to_end", &build_tuple_end_to_end())
        else {
            return;
        };
        let code = status.code().unwrap_or(-1);
        assert_eq!(
            code, 150,
            "tuple_end_to_end: expected exit 150 (200 - 50), got {code}; \
             stdout={stdout:?} stderr={stderr:?}"
        );
    }

    /// Sub-wave-3 capstone — dict end-to-end exercising every untyped +
    /// typed-i64-i64 helper, terminating with `Terminator::Drop` on the
    /// dict local so `emit_drop_for_ty`'s new `Ty::Dict → dict_drop`
    /// arm fires. Post-fix exit 33 (10+20+2+0+1).
    #[test]
    fn llvm_emits_dict_end_to_end_with_drop() {
        let Some((status, stdout, stderr)) = link_and_run(
            "dict_end_to_end_with_drop",
            &build_dict_end_to_end_with_drop(),
        ) else {
            return;
        };
        let code = status.code().unwrap_or(-1);
        assert_eq!(
            code, 33,
            "dict_end_to_end_with_drop: expected exit 33 (10+20+2+0+1), got {code}; \
             stdout={stdout:?} stderr={stderr:?}"
        );
    }
}

// On default (non-LLVM) feature builds, all fixtures degrade to pass
// (no LLVM backend to exercise; wave-3 surface is feature-gated).
#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_dict_new_len_is_empty() {
    // Skipped on default build — LLVM backend feature-gated.
}

#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_dict_set_then_get_i64_i64() {
    // Skipped on default build — LLVM backend feature-gated.
}

#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_dict_contains_after_set() {
    // Skipped on default build — LLVM backend feature-gated.
}

#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_set_end_to_end() {
    // Skipped on default build — LLVM backend feature-gated.
}

#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_tuple_end_to_end() {
    // Skipped on default build — LLVM backend feature-gated.
}

#[cfg(not(feature = "llvm"))]
#[test]
fn llvm_emits_dict_end_to_end_with_drop() {
    // Skipped on default build — LLVM backend feature-gated.
}
