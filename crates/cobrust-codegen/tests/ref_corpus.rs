//! M12.x `Rvalue::Ref` corpus (per ADR-0027 §2).
//!
//! Each program lowers a borrow expression. M12.x materializes
//! `Rvalue::Ref` to a `stack_addr` for stack-resident locals;
//! pointer-typed locals pass through. The borrow checker (M8) has
//! already discharged the obligations.

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

use cobrust_codegen::{Artifact, ArtifactKind, Backend, OptLevel, TargetSpec, emit};
use cobrust_frontend::span::{FileId, Span};
use cobrust_hir::DefId;
use cobrust_mir::{
    BasicBlock, BlockId, Body, BorrowKind, Constant, LocalDecl, LocalId, Module as MirModule,
    Operand, Place, Rvalue, Statement, StatementKind, Terminator,
};
use cobrust_types::Ty;
use target_lexicon::Triple;
use tempfile::TempDir;

/// Build a `TargetSpec` rooted in a fresh RAII `TempDir`. F63
/// (2026-05-27): RAII cleanup replaces the legacy
/// `std::env::temp_dir().join(...)` leak.
fn host_object_spec(name: &str) -> (TargetSpec, TempDir) {
    let dir = tempfile::tempdir().expect("create tempdir for target spec");
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

fn span() -> Span {
    Span::new(FileId::SYNTHETIC, 0, 0)
}

/// Build a single-body MIR with `_0 = ret_ty` at index 0, optional
/// extra locals starting at index 1, plus a single block whose
/// statements are `stmts` and terminator is `Return`.
fn body(name: &str, locals: Vec<LocalDecl>, stmts: Vec<Statement>) -> Body {
    Body {
        def_id: DefId(1),
        name: name.to_string(),
        locals,
        blocks: vec![BasicBlock {
            id: BlockId(0),
            statements: stmts,
            terminator: Terminator::Return,
            span: span(),
        }],
        return_local: LocalId(0),
        param_count: 0,
        span: span(),
    }
}

fn local(id: u32, name: &str, ty: Ty, mutable: bool) -> LocalDecl {
    LocalDecl {
        id: LocalId(id),
        name: name.to_string(),
        ty,
        mutable,
        span: span(),
    }
}

fn assign_int(target: u32, v: i64) -> Statement {
    Statement {
        kind: StatementKind::Assign {
            place: Place::local(LocalId(target)),
            rvalue: Rvalue::Use(Operand::Constant(Constant::Int(v))),
        },
        span: span(),
    }
}

fn assign_ref(target: u32, src: u32, kind: BorrowKind) -> Statement {
    Statement {
        kind: StatementKind::Assign {
            place: Place::local(LocalId(target)),
            rvalue: Rvalue::Ref(kind, Place::local(LocalId(src))),
        },
        span: span(),
    }
}

fn compile(name: &str, body: Body) {
    let module = MirModule { bodies: vec![body] };
    let (spec, _guard) = host_object_spec(name);
    let artifact = emit(&module, spec).unwrap_or_else(|e| panic!("emit `{name}`: {e}"));
    let path = artifact.path();
    let meta = std::fs::metadata(path).unwrap();
    assert!(meta.len() > 0, "object file empty for `{name}`");
    assert!(matches!(artifact, Artifact::Object(_)));
}

// =====================================================================
// Shared borrow corpus
// =====================================================================

#[test]
fn ref_shared_int_local() {
    let b = body(
        "ref01",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true), // pointer storage
        ],
        vec![
            assign_int(1, 42),
            assign_ref(2, 1, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref01", b);
}

#[test]
fn ref_shared_two_locals() {
    let b = body(
        "ref02",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "a", Ty::Int, true),
            local(2, "b", Ty::Int, true),
            local(3, "pa", Ty::Int, true),
            local(4, "pb", Ty::Int, true),
        ],
        vec![
            assign_int(1, 10),
            assign_int(2, 20),
            assign_ref(3, 1, BorrowKind::Shared),
            assign_ref(4, 2, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref02", b);
}

#[test]
fn ref_shared_bool_local() {
    let b = body(
        "ref03",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Bool, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(1)),
                    rvalue: Rvalue::Use(Operand::Constant(Constant::Bool(true))),
                },
                span: span(),
            },
            assign_ref(2, 1, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref03", b);
}

#[test]
fn ref_shared_float_local() {
    let b = body(
        "ref04",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Float, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(1)),
                    rvalue: Rvalue::Use(Operand::Constant(Constant::Float(0))),
                },
                span: span(),
            },
            assign_ref(2, 1, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref04", b);
}

#[test]
fn ref_shared_chain_uses() {
    let b = body(
        "ref05",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p1", Ty::Int, true),
            local(3, "p2", Ty::Int, true),
        ],
        vec![
            assign_int(1, 7),
            assign_ref(2, 1, BorrowKind::Shared),
            assign_ref(3, 1, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref05", b);
}

#[test]
fn ref_shared_param_local() {
    let mut b = body(
        "ref06",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, false),
            local(2, "p", Ty::Int, true),
        ],
        vec![assign_ref(2, 1, BorrowKind::Shared), assign_int(0, 0)],
    );
    b.param_count = 1;
    compile("ref06", b);
}

#[test]
fn ref_shared_in_branch_then() {
    // bb0: cond, switch -> bb1 / bb2 (Goto bb3); bb3: Return.
    let mut b = body(
        "ref07",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
            local(3, "cond", Ty::Bool, true),
        ],
        vec![],
    );
    b.blocks[0].statements = vec![
        assign_int(1, 1),
        Statement {
            kind: StatementKind::Assign {
                place: Place::local(LocalId(3)),
                rvalue: Rvalue::Use(Operand::Constant(Constant::Bool(true))),
            },
            span: span(),
        },
    ];
    b.blocks[0].terminator = Terminator::SwitchInt {
        operand: Operand::Copy(Place::local(LocalId(3))),
        cases: vec![(cobrust_mir::SwitchValue::Bool(true), BlockId(1))],
        otherwise: BlockId(2),
    };
    b.blocks.push(BasicBlock {
        id: BlockId(1),
        statements: vec![assign_ref(2, 1, BorrowKind::Shared)],
        terminator: Terminator::Goto(BlockId(2)),
        span: span(),
    });
    b.blocks.push(BasicBlock {
        id: BlockId(2),
        statements: vec![assign_int(0, 0)],
        terminator: Terminator::Return,
        span: span(),
    });
    compile("ref07", b);
}

#[test]
fn ref_shared_repeated_in_loop() {
    // bb0: jump bb1; bb1: ref then jump bb1 once via the SwitchInt edge
    // (bounded by counter).
    let mut b = body(
        "ref08",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
            local(3, "n", Ty::Int, true),
            local(4, "cond", Ty::Bool, true),
        ],
        vec![],
    );
    b.blocks[0].statements = vec![assign_int(1, 5), assign_int(3, 0)];
    b.blocks[0].terminator = Terminator::Goto(BlockId(1));
    b.blocks.push(BasicBlock {
        id: BlockId(1),
        statements: vec![
            assign_ref(2, 1, BorrowKind::Shared),
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(4)),
                    rvalue: Rvalue::BinaryOp(
                        cobrust_mir::BinOp::Lt,
                        Operand::Copy(Place::local(LocalId(3))),
                        Operand::Constant(Constant::Int(3)),
                    ),
                },
                span: span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(3)),
                    rvalue: Rvalue::BinaryOp(
                        cobrust_mir::BinOp::Add,
                        Operand::Copy(Place::local(LocalId(3))),
                        Operand::Constant(Constant::Int(1)),
                    ),
                },
                span: span(),
            },
        ],
        terminator: Terminator::SwitchInt {
            operand: Operand::Copy(Place::local(LocalId(4))),
            cases: vec![(cobrust_mir::SwitchValue::Bool(true), BlockId(1))],
            otherwise: BlockId(2),
        },
        span: span(),
    });
    b.blocks.push(BasicBlock {
        id: BlockId(2),
        statements: vec![assign_int(0, 0)],
        terminator: Terminator::Return,
        span: span(),
    });
    compile("ref08", b);
}

#[test]
fn ref_shared_ten_locals() {
    let mut locals = vec![local(0, "_return", Ty::Int, true)];
    let mut stmts = vec![];
    for i in 1..=10 {
        locals.push(local(i, "x", Ty::Int, true));
        stmts.push(assign_int(i, i as i64));
    }
    locals.push(local(11, "p", Ty::Int, true));
    stmts.push(assign_ref(11, 5, BorrowKind::Shared));
    stmts.push(assign_int(0, 0));
    let b = body("ref09", locals, stmts);
    compile("ref09", b);
}

// =====================================================================
// Mut borrow corpus
// =====================================================================

#[test]
fn ref_mut_int_local() {
    let b = body(
        "ref10",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![
            assign_int(1, 42),
            assign_ref(2, 1, BorrowKind::Mut),
            assign_int(0, 0),
        ],
    );
    compile("ref10", b);
}

#[test]
fn ref_mut_two_disjoint() {
    let b = body(
        "ref11",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "a", Ty::Int, true),
            local(2, "b", Ty::Int, true),
            local(3, "pa", Ty::Int, true),
            local(4, "pb", Ty::Int, true),
        ],
        vec![
            assign_int(1, 10),
            assign_int(2, 20),
            assign_ref(3, 1, BorrowKind::Mut),
            assign_ref(4, 2, BorrowKind::Mut),
            assign_int(0, 0),
        ],
    );
    compile("ref11", b);
}

#[test]
fn ref_mut_after_shared_ok() {
    // Two ref's on same local sequentially — borrow check would catch
    // overlap, but for codegen each ref produces a stack address.
    let b = body(
        "ref12",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p1", Ty::Int, true),
            local(3, "p2", Ty::Int, true),
        ],
        vec![
            assign_int(1, 5),
            assign_ref(2, 1, BorrowKind::Shared),
            assign_ref(3, 1, BorrowKind::Mut),
            assign_int(0, 0),
        ],
    );
    compile("ref12", b);
}

#[test]
fn ref_mut_param_local() {
    let mut b = body(
        "ref13",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![assign_ref(2, 1, BorrowKind::Mut), assign_int(0, 0)],
    );
    b.param_count = 1;
    compile("ref13", b);
}

#[test]
fn ref_mut_chain() {
    let b = body(
        "ref14",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p1", Ty::Int, true),
            local(3, "p2", Ty::Int, true),
            local(4, "p3", Ty::Int, true),
        ],
        vec![
            assign_int(1, 1),
            assign_ref(2, 1, BorrowKind::Mut),
            assign_ref(3, 1, BorrowKind::Mut),
            assign_ref(4, 1, BorrowKind::Mut),
            assign_int(0, 0),
        ],
    );
    compile("ref14", b);
}

// =====================================================================
// Mixed borrow corpus
// =====================================================================

#[test]
fn ref_mixed_shared_mut_disjoint() {
    let b = body(
        "ref15",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "a", Ty::Int, true),
            local(2, "b", Ty::Int, true),
            local(3, "pa_s", Ty::Int, true),
            local(4, "pb_m", Ty::Int, true),
        ],
        vec![
            assign_int(1, 10),
            assign_int(2, 20),
            assign_ref(3, 1, BorrowKind::Shared),
            assign_ref(4, 2, BorrowKind::Mut),
            assign_int(0, 0),
        ],
    );
    compile("ref15", b);
}

#[test]
fn ref_mixed_after_arithmetic() {
    let b = body(
        "ref16",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "y", Ty::Int, true),
            local(3, "p", Ty::Int, true),
        ],
        vec![
            assign_int(1, 5),
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(2)),
                    rvalue: Rvalue::BinaryOp(
                        cobrust_mir::BinOp::Add,
                        Operand::Copy(Place::local(LocalId(1))),
                        Operand::Constant(Constant::Int(1)),
                    ),
                },
                span: span(),
            },
            assign_ref(3, 2, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref16", b);
}

// =====================================================================
// Boundary cases
// =====================================================================

#[test]
fn ref_self_body_is_empty_pre_block() {
    let b = body(
        "ref17",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![
            assign_int(1, 0),
            assign_ref(2, 1, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref17", b);
}

#[test]
fn ref_max_int() {
    let b = body(
        "ref18",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![
            assign_int(1, i64::MAX),
            assign_ref(2, 1, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref18", b);
}

#[test]
fn ref_min_int() {
    let b = body(
        "ref19",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![
            assign_int(1, i64::MIN),
            assign_ref(2, 1, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref19", b);
}

#[test]
fn ref_zero_value() {
    let b = body(
        "ref20",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![
            assign_int(1, 0),
            assign_ref(2, 1, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref20", b);
}

#[test]
fn ref_neg_value() {
    let b = body(
        "ref21",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![
            assign_int(1, -1),
            assign_ref(2, 1, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref21", b);
}

#[test]
fn ref_three_separate_borrows() {
    let b = body(
        "ref22",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "a", Ty::Int, true),
            local(2, "b", Ty::Int, true),
            local(3, "c", Ty::Int, true),
            local(4, "pa", Ty::Int, true),
            local(5, "pb", Ty::Int, true),
            local(6, "pc", Ty::Int, true),
        ],
        vec![
            assign_int(1, 10),
            assign_int(2, 20),
            assign_int(3, 30),
            assign_ref(4, 1, BorrowKind::Shared),
            assign_ref(5, 2, BorrowKind::Shared),
            assign_ref(6, 3, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref22", b);
}

#[test]
fn ref_reuse_pointer_local() {
    let b = body(
        "ref23",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "a", Ty::Int, true),
            local(2, "b", Ty::Int, true),
            local(3, "p", Ty::Int, true),
        ],
        vec![
            assign_int(1, 1),
            assign_int(2, 2),
            assign_ref(3, 1, BorrowKind::Shared),
            assign_ref(3, 2, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref23", b);
}

#[test]
fn ref_after_int_addition() {
    let b = body(
        "ref24",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(1)),
                    rvalue: Rvalue::BinaryOp(
                        cobrust_mir::BinOp::Add,
                        Operand::Constant(Constant::Int(1)),
                        Operand::Constant(Constant::Int(2)),
                    ),
                },
                span: span(),
            },
            assign_ref(2, 1, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref24", b);
}

#[test]
fn ref_two_in_one_block_then_branch() {
    let mut b = body(
        "ref25",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
            local(3, "q", Ty::Int, true),
            local(4, "cond", Ty::Bool, true),
        ],
        vec![],
    );
    b.blocks[0].statements = vec![
        assign_int(1, 7),
        assign_ref(2, 1, BorrowKind::Shared),
        assign_ref(3, 1, BorrowKind::Mut),
        Statement {
            kind: StatementKind::Assign {
                place: Place::local(LocalId(4)),
                rvalue: Rvalue::Use(Operand::Constant(Constant::Bool(true))),
            },
            span: span(),
        },
    ];
    b.blocks[0].terminator = Terminator::SwitchInt {
        operand: Operand::Copy(Place::local(LocalId(4))),
        cases: vec![(cobrust_mir::SwitchValue::Bool(true), BlockId(1))],
        otherwise: BlockId(1),
    };
    b.blocks.push(BasicBlock {
        id: BlockId(1),
        statements: vec![assign_int(0, 0)],
        terminator: Terminator::Return,
        span: span(),
    });
    compile("ref25", b);
}

#[test]
fn ref_int_then_int_use() {
    let b = body(
        "ref26",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
            local(3, "y", Ty::Int, true),
        ],
        vec![
            assign_int(1, 5),
            assign_ref(2, 1, BorrowKind::Shared),
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(3)),
                    rvalue: Rvalue::Use(Operand::Copy(Place::local(LocalId(1)))),
                },
                span: span(),
            },
            assign_int(0, 0),
        ],
    );
    compile("ref26", b);
}

#[test]
fn ref_bool_true_false() {
    let b = body(
        "ref27",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Bool, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(1)),
                    rvalue: Rvalue::Use(Operand::Constant(Constant::Bool(false))),
                },
                span: span(),
            },
            assign_ref(2, 1, BorrowKind::Mut),
            assign_int(0, 0),
        ],
    );
    compile("ref27", b);
}

#[test]
fn ref_in_third_block() {
    let mut b = body(
        "ref28",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![],
    );
    b.blocks[0].statements = vec![assign_int(1, 1)];
    b.blocks[0].terminator = Terminator::Goto(BlockId(1));
    b.blocks.push(BasicBlock {
        id: BlockId(1),
        statements: vec![],
        terminator: Terminator::Goto(BlockId(2)),
        span: span(),
    });
    b.blocks.push(BasicBlock {
        id: BlockId(2),
        statements: vec![assign_ref(2, 1, BorrowKind::Shared), assign_int(0, 0)],
        terminator: Terminator::Return,
        span: span(),
    });
    compile("ref28", b);
}

#[test]
fn ref_pre_post_pattern() {
    let b = body(
        "ref29",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p1", Ty::Int, true),
            local(3, "p2", Ty::Int, true),
            local(4, "y", Ty::Int, true),
        ],
        vec![
            assign_int(1, 1),
            assign_ref(2, 1, BorrowKind::Shared),
            assign_int(4, 99),
            assign_ref(3, 4, BorrowKind::Shared),
            assign_int(0, 0),
        ],
    );
    compile("ref29", b);
}

#[test]
fn ref_into_field_projection() {
    // place._0 borrow — projection chain on Field(0) on a tuple-shaped
    // local. M12.x emits iadd ptr, const_offset.
    let b = body(
        "ref30",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "p", Ty::Int, true),
        ],
        vec![
            assign_int(1, 7),
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(2)),
                    rvalue: Rvalue::Ref(
                        BorrowKind::Shared,
                        Place {
                            local: LocalId(1),
                            projections: vec![cobrust_mir::Projection::Field(0)],
                        },
                    ),
                },
                span: span(),
            },
            assign_int(0, 0),
        ],
    );
    compile("ref30", b);
}
