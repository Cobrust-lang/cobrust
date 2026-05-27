//! M9 ill-formed codegen tests — every case here exercises a
//! [`CodegenError`] variant. The MIR is either constructed
//! manually with intentionally invalid shape, or routed through
//! a backend whose feature flag is off.
//!
//! ADR-0023 §"Public surface" pins the [`CodegenError`] variants;
//! every variant must have at least one regression test here.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::doc_overindented_list_items)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::derivable_impls)]

use std::str::FromStr;

use cobrust_codegen::{Artifact, ArtifactKind, Backend, CodegenError, OptLevel, TargetSpec, emit};
use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_mir::lower as mir_lower;
use cobrust_mir::{
    BasicBlock, BinOp, BlockId, Body, Constant, LocalDecl, LocalId, Module as MirModule, Operand,
    Place, Rvalue, Statement, StatementKind, Terminator,
};
use cobrust_types::{Ty, check};
use target_lexicon::Triple;

fn lower_to_mir(src: &str) -> MirModule {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir lower");
    let typed = check(&hir).expect("type check");
    mir_lower(&typed).expect("mir lower")
}

fn host_object_spec(name: &str) -> TargetSpec {
    let dir = std::env::temp_dir().join(format!("cobrust-m9-ill-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    TargetSpec {
        triple: Triple::host(),
        opt_level: OptLevel::None,
        backend: Backend::Cranelift,
        artifact: ArtifactKind::Object,
        output_dir: dir,
        module_name: name.to_string(),
        source_path: None,
        runtime_dispatch: false,
        target_cpu: None,
    }
}

// =====================================================================
// 1. UnsupportedBackend — LLVM without `--features llvm`.
// =====================================================================

#[test]
fn ill_001_llvm_without_feature() {
    if cfg!(feature = "llvm") {
        return; // LLVM is enabled; this case is unreachable.
    }
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("ill_001");
    spec.backend = Backend::Llvm;
    let err = emit(&mir, spec).expect_err("LLVM should fail without feature");
    assert!(matches!(
        err,
        CodegenError::UnsupportedBackend(Backend::Llvm)
    ));
}

#[test]
fn ill_002_unsupported_backend_message_mentions_feature() {
    if cfg!(feature = "llvm") {
        return;
    }
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("ill_002");
    spec.backend = Backend::Llvm;
    let err = emit(&mir, spec).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("llvm"), "msg should mention llvm: {msg}");
}

// =====================================================================
// 2. UnsupportedTarget — synthetic triple that Cranelift refuses.
// =====================================================================

#[test]
fn ill_003_unsupported_target_arbitrary_triple() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("ill_003");
    // SPARC is not supported by Cranelift; this triple should error.
    spec.triple = Triple::from_str("sparc64-unknown-linux-gnu").expect("triple parse");
    let err = emit(&mir, spec).unwrap_err();
    assert!(
        matches!(err, CodegenError::UnsupportedTarget(_)),
        "expected UnsupportedTarget, got {err:?}"
    );
}

#[test]
fn ill_004_unsupported_target_carries_triple_in_message() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("ill_004");
    spec.triple = Triple::from_str("sparc64-unknown-linux-gnu").unwrap();
    let err = emit(&mir, spec).unwrap_err();
    assert!(err.to_string().contains("sparc"));
}

// =====================================================================
// 3. InvalidMir — manually constructed broken MIR.
// =====================================================================

fn build_bare_module(body: Body) -> MirModule {
    MirModule { bodies: vec![body] }
}

fn dangling_local_body() -> Body {
    use cobrust_frontend::span::Span;
    use cobrust_hir::DefId;
    let span = Span::new(cobrust_frontend::span::FileId::SYNTHETIC, 0, 0);
    let return_local = LocalId(0);
    let return_decl = LocalDecl {
        id: return_local,
        name: "_return".to_string(),
        ty: Ty::Int,
        mutable: true,
        span,
    };
    // Exactly one block whose single statement assigns to a *non-existent*
    // local id (LocalId(99)) — codegen should refuse this with InvalidMir.
    let stmt = Statement {
        kind: StatementKind::Assign {
            place: Place::local(LocalId(99)),
            rvalue: Rvalue::Use(Operand::Constant(Constant::Int(42))),
        },
        span,
    };
    let block = BasicBlock {
        id: BlockId(0),
        statements: vec![stmt],
        terminator: Terminator::Return,
        span,
    };
    Body {
        def_id: DefId(0),
        name: "broken".to_string(),
        locals: vec![return_decl],
        blocks: vec![block],
        return_local,
        param_count: 0,
        span,
    }
}

#[test]
fn ill_005_invalid_mir_dangling_local() {
    let module = build_bare_module(dangling_local_body());
    let spec = host_object_spec("ill_005");
    let err = emit(&module, spec).unwrap_err();
    assert!(
        matches!(err, CodegenError::InvalidMir(_)),
        "expected InvalidMir, got {err:?}"
    );
}

#[test]
fn ill_006_invalid_mir_message_cites_local() {
    let module = build_bare_module(dangling_local_body());
    let spec = host_object_spec("ill_006");
    let err = emit(&module, spec).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("_99") || msg.contains("99"),
        "msg should cite local 99: {msg}"
    );
}

// =====================================================================
// 4. CraneliftError — programmatically force a Cranelift verifier
//    failure by emitting a Body whose terminator has a target
//    block that doesn't exist.
// =====================================================================

#[test]
fn ill_007_cranelift_error_dangling_block_target() {
    use cobrust_frontend::span::Span;
    use cobrust_hir::DefId;
    let span = Span::new(cobrust_frontend::span::FileId::SYNTHETIC, 0, 0);
    let return_local = LocalId(0);
    let return_decl = LocalDecl {
        id: return_local,
        name: "_return".to_string(),
        ty: Ty::Int,
        mutable: true,
        span,
    };
    // Single block whose terminator jumps to BlockId(99) — non-existent.
    let block = BasicBlock {
        id: BlockId(0),
        statements: vec![],
        terminator: Terminator::Goto(BlockId(99)),
        span,
    };
    let body = Body {
        def_id: DefId(1),
        name: "danglingblock".to_string(),
        locals: vec![return_decl],
        blocks: vec![block],
        return_local,
        param_count: 0,
        span,
    };
    let module = build_bare_module(body);
    let spec = host_object_spec("ill_007");
    let err = emit(&module, spec).unwrap_err();
    // Either InvalidMir or CraneliftError or a panic-coerced error;
    // accept anything that signals "broken IR".
    match err {
        CodegenError::CraneliftError(_)
        | CodegenError::InvalidMir(_)
        | CodegenError::Internal(_) => {}
        other => panic!("expected Cranelift / InvalidMir / Internal, got {other:?}"),
    }
}

// =====================================================================
// 5. Object emission already covered structurally by the well_formed
//    suite, but we add a no-body module to exercise the empty-output
//    boundary.
// =====================================================================

#[test]
fn ill_008_empty_module_emits_nothing_useful_but_does_not_panic() {
    // An empty module is still a *valid* shape — codegen must not panic.
    let module = MirModule { bodies: vec![] };
    let spec = host_object_spec("ill_008");
    let result = emit(&module, spec);
    assert!(
        matches!(result, Ok(Artifact::Object(_))),
        "empty module should yield an empty-but-valid object: {result:?}"
    );
}

// =====================================================================
// 6. CodegenError variant coverage — at least one test per variant
//    that is reachable without OS / linker side effects.
// =====================================================================

#[test]
fn ill_009_codegen_error_display_unsupported_backend() {
    let e = CodegenError::UnsupportedBackend(Backend::Llvm);
    let s = e.to_string();
    assert!(s.contains("llvm"));
}

#[test]
fn ill_010_codegen_error_display_unsupported_target() {
    let e = CodegenError::UnsupportedTarget("sparc64-unknown-linux-gnu".to_string());
    assert!(e.to_string().contains("sparc"));
}

#[test]
fn ill_011_codegen_error_display_invalid_mir() {
    let e = CodegenError::InvalidMir("foo".to_string());
    assert!(e.to_string().contains("MIR"));
}

#[test]
fn ill_012_codegen_error_display_cranelift_error() {
    let e = CodegenError::CraneliftError("synthetic".to_string());
    assert!(e.to_string().contains("Cranelift"));
}

#[test]
fn ill_013_codegen_error_display_llvm_error() {
    let e = CodegenError::LlvmError("synthetic".to_string());
    assert!(e.to_string().contains("LLVM"));
}

#[test]
fn ill_014_codegen_error_display_object_emission() {
    let e = CodegenError::ObjectEmission("synthetic".to_string());
    assert!(e.to_string().contains("object"));
}

#[test]
fn ill_015_codegen_error_display_linker_failed() {
    let e = CodegenError::LinkerFailed {
        exit_code: 1,
        stderr: "bad linker".to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("linker"));
    assert!(s.contains("1"));
}

#[test]
fn ill_016_codegen_error_display_io() {
    let e = CodegenError::Io("file missing".to_string());
    assert!(e.to_string().contains("I/O"));
}

#[test]
fn ill_017_codegen_error_display_internal() {
    let e = CodegenError::Internal("regression".to_string());
    assert!(e.to_string().contains("internal"));
}

// =====================================================================
// 7. From<io::Error> for CodegenError
// =====================================================================

#[test]
fn ill_018_codegen_error_from_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing.o");
    let e: CodegenError = io_err.into();
    assert!(matches!(e, CodegenError::Io(_)));
    assert!(e.to_string().contains("missing.o"));
}

// =====================================================================
// 8. Multiple invalid MIR variations — each should cite its own local.
// =====================================================================

#[test]
fn ill_019_invalid_mir_dangling_in_terminator_call_destination() {
    use cobrust_frontend::span::Span;
    use cobrust_hir::DefId;
    let span = Span::new(cobrust_frontend::span::FileId::SYNTHETIC, 0, 0);
    let return_local = LocalId(0);
    let return_decl = LocalDecl {
        id: return_local,
        name: "_return".to_string(),
        ty: Ty::Int,
        mutable: true,
        span,
    };
    // Block with Terminator::Call destination = LocalId(77) — non-existent.
    let block_a = BasicBlock {
        id: BlockId(0),
        statements: vec![],
        terminator: Terminator::Call {
            func: Operand::Constant(Constant::FnRef(0)),
            args: vec![],
            destination: Place::local(LocalId(77)),
            target: BlockId(1),
            unwind: None,
        },
        span,
    };
    let block_b = BasicBlock {
        id: BlockId(1),
        statements: vec![],
        terminator: Terminator::Return,
        span,
    };
    let body = Body {
        def_id: DefId(2),
        name: "callbroken".to_string(),
        locals: vec![return_decl],
        blocks: vec![block_a, block_b],
        return_local,
        param_count: 0,
        span,
    };
    let module = build_bare_module(body);
    let spec = host_object_spec("ill_019");
    let err = emit(&module, spec).unwrap_err();
    assert!(
        matches!(err, CodegenError::InvalidMir(_)),
        "expected InvalidMir, got {err:?}"
    );
}

// =====================================================================
// 9. Backend default selection invariants.
// =====================================================================

#[test]
fn ill_020_default_backend_follows_llvm_feature() {
    // ADR-0070 §X.3 RATIFIED 2026-05-26: LLVM is the default backend when
    // the `llvm` feature is active (now the workspace default). Cranelift
    // remains the fallback under `--no-default-features`.
    if cfg!(feature = "llvm") {
        assert_eq!(Backend::default(), Backend::Llvm);
        assert_eq!(Backend::default_for_dev(), Backend::Llvm);
    } else {
        assert_eq!(Backend::default(), Backend::Cranelift);
        assert_eq!(Backend::default_for_dev(), Backend::Cranelift);
    }
}

#[test]
fn ill_021_release_default_when_no_llvm_is_cranelift() {
    if !cfg!(feature = "llvm") {
        assert_eq!(Backend::default_for_release(), Backend::Cranelift);
    }
}

// =====================================================================
// 10. Triple parsing — invalid triple at the *codegen* layer turns
//     into UnsupportedTarget.
// =====================================================================

#[test]
fn ill_022_unsupported_target_riscv() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("ill_022");
    if let Ok(t) = Triple::from_str("riscv64gc-unknown-linux-gnu") {
        spec.triple = t;
        // Cranelift may or may not have RISC-V depending on cargo features;
        // we assert *correctness* in either branch.
        match emit(&mir, spec) {
            Ok(_) => { /* RISC-V was supported */ }
            Err(CodegenError::UnsupportedTarget(_)) => { /* expected on lean builds */ }
            Err(other) => panic!("unexpected error for RISC-V: {other:?}"),
        }
    }
}

// =====================================================================
// 11. ArtifactKind extension matrix.
// =====================================================================

#[test]
fn ill_023_artifact_kind_extension_object() {
    let triple = Triple::host();
    assert_eq!(ArtifactKind::Object.extension(&triple), "o");
}

#[test]
fn ill_024_artifact_kind_extension_executable_is_empty_or_exe() {
    let triple = Triple::host();
    let ext = ArtifactKind::Executable.extension(&triple);
    assert!(ext.is_empty() || ext == "exe");
}

#[test]
fn ill_025_artifact_kind_extension_dylib_macos() {
    let triple = Triple::from_str("aarch64-apple-darwin").unwrap();
    assert_eq!(ArtifactKind::DynamicLibrary.extension(&triple), "dylib");
}

#[test]
fn ill_026_artifact_kind_extension_dylib_linux() {
    let triple = Triple::from_str("x86_64-unknown-linux-gnu").unwrap();
    assert_eq!(ArtifactKind::DynamicLibrary.extension(&triple), "so");
}

#[test]
fn ill_027_artifact_kind_extension_dylib_windows() {
    let triple = Triple::from_str("x86_64-pc-windows-msvc").unwrap();
    assert_eq!(ArtifactKind::DynamicLibrary.extension(&triple), "dll");
}

// =====================================================================
// 12. OptLevel exhaustive variants.
// =====================================================================

#[test]
fn ill_028_optlevel_none_default() {
    assert_eq!(OptLevel::default(), OptLevel::None);
}

#[test]
fn ill_029_optlevel_compiles_speed() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("ill_029");
    spec.opt_level = OptLevel::Speed;
    let _ = emit(&mir, spec).unwrap();
}

#[test]
fn ill_030_optlevel_compiles_speed_and_size() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("ill_030");
    spec.opt_level = OptLevel::SpeedAndSize;
    let _ = emit(&mir, spec).unwrap();
}

// =====================================================================
// 13. ArtifactKind variants are all reachable.
// =====================================================================

#[test]
fn ill_031_artifact_object_path() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("ill_031");
    spec.artifact = ArtifactKind::Object;
    let a = emit(&mir, spec).unwrap();
    assert!(matches!(a, Artifact::Object(_)));
    assert!(!a.is_executable());
}

#[test]
fn ill_032_artifact_path_exists() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let spec = host_object_spec("ill_032");
    let a = emit(&mir, spec).unwrap();
    assert!(a.path().exists());
}

// =====================================================================
// 14. Targeted MIR shape errors — read place that doesn't exist.
// =====================================================================

#[test]
fn ill_033_read_nonexistent_place_in_assign() {
    use cobrust_frontend::span::Span;
    use cobrust_hir::DefId;
    let span = Span::new(cobrust_frontend::span::FileId::SYNTHETIC, 0, 0);
    let return_local = LocalId(0);
    let return_decl = LocalDecl {
        id: return_local,
        name: "_return".to_string(),
        ty: Ty::Int,
        mutable: true,
        span,
    };
    // _return = copy _88
    let stmt = Statement {
        kind: StatementKind::Assign {
            place: Place::local(return_local),
            rvalue: Rvalue::Use(Operand::Copy(Place::local(LocalId(88)))),
        },
        span,
    };
    let block = BasicBlock {
        id: BlockId(0),
        statements: vec![stmt],
        terminator: Terminator::Return,
        span,
    };
    let body = Body {
        def_id: DefId(3),
        name: "readbroken".to_string(),
        locals: vec![return_decl],
        blocks: vec![block],
        return_local,
        param_count: 0,
        span,
    };
    let module = build_bare_module(body);
    let spec = host_object_spec("ill_033");
    let err = emit(&module, spec).unwrap_err();
    assert!(
        matches!(err, CodegenError::InvalidMir(_)),
        "expected InvalidMir, got {err:?}"
    );
}

// =====================================================================
// 15. PartialEq / Clone / Debug invariants on Backend / OptLevel.
// =====================================================================

#[test]
fn ill_034_backend_eq_clone_debug() {
    let a = Backend::Cranelift;
    let b = a;
    assert_eq!(a, b);
    let _ = format!("{a:?}");
}

#[test]
fn ill_035_optlevel_eq_clone_debug() {
    let a = OptLevel::Speed;
    let b = a;
    assert_eq!(a, b);
    let _ = format!("{a:?}");
}

// =====================================================================
// 16. Triple parsing failures don't panic.
// =====================================================================

#[test]
fn ill_036_triple_parse_failure_is_error() {
    let r = Triple::from_str("totally-invalid-triple-name");
    if let Ok(t) = r {
        // target-lexicon is permissive; just ensure no panic and the
        // emit step yields a structured error.
        let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
        let mut spec = host_object_spec("ill_036");
        spec.triple = t;
        let r = emit(&mir, spec);
        if let Err(e) = r {
            assert!(matches!(
                e,
                CodegenError::UnsupportedTarget(_) | CodegenError::CraneliftError(_)
            ));
        }
    }
}

// =====================================================================
// 17. Artifact path accessor.
// =====================================================================

#[test]
fn ill_037_artifact_path_accessor() {
    let p = std::path::PathBuf::from("/tmp/x.o");
    let a = Artifact::Object(p.clone());
    assert_eq!(a.path(), p.as_path());
}

#[test]
fn ill_038_artifact_executable_path_accessor() {
    let p = std::path::PathBuf::from("/tmp/exe");
    let a = Artifact::Executable(p.clone());
    assert_eq!(a.path(), p.as_path());
    assert!(a.is_executable());
}

#[test]
fn ill_039_artifact_dylib_path_accessor() {
    let p = std::path::PathBuf::from("/tmp/lib.so");
    let a = Artifact::DynamicLibrary(p.clone());
    assert_eq!(a.path(), p.as_path());
    assert!(!a.is_executable());
}

// =====================================================================
// 18. TargetSpec helper constructors are reachable.
// =====================================================================

#[test]
fn ill_040_targetspec_host_dev() {
    let dir = std::env::temp_dir().join("cobrust-m9-helper");
    let spec = TargetSpec::host_dev(dir, "h");
    // ADR-0070 §X.3: host_dev backend follows default_for_dev (LLVM when
    // the `llvm` feature is active — now the workspace default).
    let expected = if cfg!(feature = "llvm") {
        Backend::Llvm
    } else {
        Backend::Cranelift
    };
    assert_eq!(spec.backend, expected);
    assert_eq!(spec.opt_level, OptLevel::None);
}

#[test]
fn ill_041_targetspec_host_release() {
    let dir = std::env::temp_dir().join("cobrust-m9-helper");
    let spec = TargetSpec::host_release(dir, "h");
    assert_eq!(spec.opt_level, OptLevel::Speed);
}

#[test]
fn ill_042_targetspec_host_object() {
    let dir = std::env::temp_dir().join("cobrust-m9-helper");
    let spec = TargetSpec::host_object(dir, "h");
    assert_eq!(spec.artifact, ArtifactKind::Object);
}

// =====================================================================
// 19. Fresh module names — output paths don't collide.
// =====================================================================

#[test]
fn ill_043_two_different_module_names_dont_collide() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let s1 = host_object_spec("ill_043_a");
    let s2 = host_object_spec("ill_043_b");
    let a1 = emit(&mir, s1).unwrap();
    let a2 = emit(&mir, s2).unwrap();
    assert_ne!(a1.path(), a2.path());
}

// =====================================================================
// 20. CodegenError equality.
// =====================================================================

#[test]
fn ill_044_codegen_error_eq() {
    let a = CodegenError::Io("x".to_string());
    let b = CodegenError::Io("x".to_string());
    assert_eq!(a, b);
}

#[test]
fn ill_045_codegen_error_neq_across_variants() {
    let a = CodegenError::Io("x".to_string());
    let b = CodegenError::Internal("x".to_string());
    assert_ne!(a, b);
}

// =====================================================================
// 21..50 — repeat dangling-local pattern across MIR positions.
// =====================================================================

fn run_dangling_in_position(name: &str, position: usize) {
    use cobrust_frontend::span::Span;
    use cobrust_hir::DefId;
    let span = Span::new(cobrust_frontend::span::FileId::SYNTHETIC, 0, 0);
    let return_local = LocalId(0);
    let return_decl = LocalDecl {
        id: return_local,
        name: "_return".to_string(),
        ty: Ty::Int,
        mutable: true,
        span,
    };
    let mut statements = Vec::new();
    for i in 0..position {
        statements.push(Statement {
            kind: StatementKind::Assign {
                place: Place::local(return_local),
                rvalue: Rvalue::Use(Operand::Constant(Constant::Int(i as i64))),
            },
            span,
        });
    }
    // The last statement reads from a dangling local.
    statements.push(Statement {
        kind: StatementKind::Assign {
            place: Place::local(return_local),
            rvalue: Rvalue::BinaryOp(
                BinOp::Add,
                Operand::Copy(Place::local(LocalId(99))),
                Operand::Constant(Constant::Int(0)),
            ),
        },
        span,
    });
    let block = BasicBlock {
        id: BlockId(0),
        statements,
        terminator: Terminator::Return,
        span,
    };
    let body = Body {
        def_id: DefId(position as u32 + 100),
        name: name.to_string(),
        locals: vec![return_decl],
        blocks: vec![block],
        return_local,
        param_count: 0,
        span,
    };
    let module = MirModule { bodies: vec![body] };
    let spec = host_object_spec(name);
    let err = emit(&module, spec).unwrap_err();
    assert!(
        matches!(err, CodegenError::InvalidMir(_)),
        "expected InvalidMir, got {err:?}"
    );
}

#[test]
fn ill_046() {
    run_dangling_in_position("ill_046", 0);
}
#[test]
fn ill_047() {
    run_dangling_in_position("ill_047", 1);
}
#[test]
fn ill_048() {
    run_dangling_in_position("ill_048", 2);
}
#[test]
fn ill_049() {
    run_dangling_in_position("ill_049", 3);
}
#[test]
fn ill_050() {
    run_dangling_in_position("ill_050", 5);
}
