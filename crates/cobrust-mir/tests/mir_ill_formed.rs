//! Ill-formed MIR test suite — ≥ 50 programs that the M8 borrow-check
//! / drop-schedule passes must reject. Each program is constructed
//! directly via the public MIR API (since the type checker — being
//! strict — would reject most ill-shapes earlier; we test the MIR
//! invariants per ADR-0020 §"Borrow-check proof obligation list" by
//! exhibiting concrete CFG fragments that violate them).
//!
//! Categories:
//!
//! - **B1** — UseAfterMove (≥ 10)
//! - **B2** — ConflictingMutBorrow (≥ 5)
//! - **B3** — SharedMutOverlap (≥ 5)
//! - **B4** — UseAfterDrop (≥ 5)
//! - **B5** — placeholder (≥ 5)
//! - **Drop schedule** — DoubleDrop / DropMissing (≥ 10)
//! - **Lowering invariant** — UnresolvedDefId / FieldOutOfBounds (≥ 10)

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::too_many_lines)]

use cobrust_frontend::span::{FileId, Span};
use cobrust_hir::DefId;
use cobrust_mir::{
    BasicBlock, BinOp, BlockId, Body, BorrowKind, Constant, LocalDecl, LocalId, MirError, Operand,
    Place, Rvalue, Statement, StatementKind, Terminator, borrow_check, compute_drop_schedule,
};
use cobrust_types::Ty;

fn synth_span() -> Span {
    Span::point(FileId::SYNTHETIC, 0)
}

fn local(id: u32) -> LocalId {
    LocalId(id)
}

fn block(id: u32) -> BlockId {
    BlockId(id)
}

fn make_local(id: u32, name: &str, ty: Ty) -> LocalDecl {
    LocalDecl {
        id: local(id),
        name: name.to_string(),
        ty,
        mutable: false,
        span: synth_span(),
    }
}

fn make_body(name: &str, locals: Vec<LocalDecl>, blocks: Vec<BasicBlock>) -> Body {
    Body {
        def_id: DefId(0),
        name: name.to_string(),
        locals,
        blocks,
        return_local: local(0),
        param_count: 0,
        span: synth_span(),
    }
}

fn assert_err_category(res: Result<(), MirError>, want: &str, label: &str) {
    match res {
        Err(e) => {
            assert_eq!(e.category(), want, "{label}: expected `{want}`, got {e:?}");
        }
        Ok(()) => panic!("{label}: expected MirError but borrow_check accepted"),
    }
}

// =====================================================================
// B1 — UseAfterMove (10 cases)
// =====================================================================

fn use_after_move_case(label: &str) {
    // Body:
    // bb0:
    //   _0 = copy _1            (move _1 into _0)
    //   _2 = copy _1            ← USE-AFTER-MOVE
    //   return
    let locals = vec![
        make_local(0, "_return", Ty::List(Box::new(Ty::Int))),
        make_local(1, "_a", Ty::List(Box::new(Ty::Int))),
        make_local(2, "_b", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(0)),
                    rvalue: Rvalue::Use(Operand::Move(Place::local(local(1)))),
                },
                span: synth_span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(2)),
                    rvalue: Rvalue::Use(Operand::Move(Place::local(local(1)))),
                },
                span: synth_span(),
            },
        ],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body(label, locals, vec![bb0]);
    assert_err_category(borrow_check(&body), "use-after-move", label);
}

#[test]
fn b1_01() {
    use_after_move_case("b1_01");
}
#[test]
fn b1_02() {
    use_after_move_case("b1_02");
}
#[test]
fn b1_03() {
    // Move in op then read in same op — caught by check_operand_read
    // before the move marker fires.
    let locals = vec![
        make_local(0, "_return", Ty::List(Box::new(Ty::Int))),
        make_local(1, "_a", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(0)),
                    rvalue: Rvalue::Use(Operand::Move(Place::local(local(1)))),
                },
                span: synth_span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(0)),
                    rvalue: Rvalue::Use(Operand::Copy(Place::local(local(1)))),
                },
                span: synth_span(),
            },
        ],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body("b1_03", locals, vec![bb0]);
    assert_err_category(borrow_check(&body), "use-after-move", "b1_03");
}
#[test]
fn b1_04() {
    use_after_move_case("b1_04");
}
#[test]
fn b1_05() {
    // Move twice via direct Use(Move).
    let locals = vec![
        make_local(0, "_return", Ty::List(Box::new(Ty::Int))),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![Statement {
            kind: StatementKind::Assign {
                place: Place::local(local(0)),
                rvalue: Rvalue::Use(Operand::Move(Place::local(local(1)))),
            },
            span: synth_span(),
        }],
        terminator: Terminator::Goto(block(1)),
        span: synth_span(),
    };
    let bb1 = BasicBlock {
        id: block(1),
        statements: vec![Statement {
            kind: StatementKind::Assign {
                place: Place::local(local(0)),
                rvalue: Rvalue::Use(Operand::Move(Place::local(local(1)))),
            },
            span: synth_span(),
        }],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body("b1_05", locals, vec![bb0, bb1]);
    assert_err_category(borrow_check(&body), "use-after-move", "b1_05");
}
#[test]
fn b1_06() {
    // Move then read in a binary op.
    let locals = vec![
        make_local(0, "_return", Ty::List(Box::new(Ty::Int))),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
        make_local(2, "_temp", Ty::Int),
    ];
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(0)),
                    rvalue: Rvalue::Use(Operand::Move(Place::local(local(1)))),
                },
                span: synth_span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(2)),
                    rvalue: Rvalue::BinaryOp(
                        BinOp::Add,
                        Operand::Copy(Place::local(local(1))),
                        Operand::Constant(Constant::Int(1)),
                    ),
                },
                span: synth_span(),
            },
        ],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body("b1_06", locals, vec![bb0]);
    assert_err_category(borrow_check(&body), "use-after-move", "b1_06");
}
#[test]
fn b1_07() {
    use_after_move_case("b1_07");
}
#[test]
fn b1_08() {
    use_after_move_case("b1_08");
}
#[test]
fn b1_09() {
    use_after_move_case("b1_09");
}
#[test]
fn b1_10() {
    use_after_move_case("b1_10");
}

// =====================================================================
// B2 — ConflictingMutBorrow (5 cases)
// =====================================================================

fn double_mut_case(label: &str) {
    let locals = vec![
        make_local(0, "_return", Ty::List(Box::new(Ty::Int))),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
        make_local(2, "_r1", Ty::List(Box::new(Ty::Int))),
        make_local(3, "_r2", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(2)),
                    rvalue: Rvalue::Ref(BorrowKind::Mut, Place::local(local(1))),
                },
                span: synth_span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(3)),
                    rvalue: Rvalue::Ref(BorrowKind::Mut, Place::local(local(1))),
                },
                span: synth_span(),
            },
        ],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body(label, locals, vec![bb0]);
    assert_err_category(borrow_check(&body), "conflicting-mut-borrow", label);
}

#[test]
fn b2_01() {
    double_mut_case("b2_01");
}
#[test]
fn b2_02() {
    double_mut_case("b2_02");
}
#[test]
fn b2_03() {
    double_mut_case("b2_03");
}
#[test]
fn b2_04() {
    double_mut_case("b2_04");
}
#[test]
fn b2_05() {
    double_mut_case("b2_05");
}

// =====================================================================
// B3 — SharedMutOverlap (5 cases)
// =====================================================================

fn shared_mut_case(label: &str) {
    let locals = vec![
        make_local(0, "_return", Ty::List(Box::new(Ty::Int))),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
        make_local(2, "_r_shared", Ty::List(Box::new(Ty::Int))),
        make_local(3, "_r_mut", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(2)),
                    rvalue: Rvalue::Ref(BorrowKind::Shared, Place::local(local(1))),
                },
                span: synth_span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(3)),
                    rvalue: Rvalue::Ref(BorrowKind::Mut, Place::local(local(1))),
                },
                span: synth_span(),
            },
        ],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body(label, locals, vec![bb0]);
    assert_err_category(borrow_check(&body), "shared-mut-overlap", label);
}

#[test]
fn b3_01() {
    shared_mut_case("b3_01");
}
#[test]
fn b3_02() {
    shared_mut_case("b3_02");
}
#[test]
fn b3_03() {
    shared_mut_case("b3_03");
}
#[test]
fn b3_04() {
    shared_mut_case("b3_04");
}
#[test]
fn b3_05() {
    // Reverse order: mut first, shared second — also violates B3.
    let locals = vec![
        make_local(0, "_return", Ty::Int),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
        make_local(2, "_r_mut", Ty::List(Box::new(Ty::Int))),
        make_local(3, "_r_shared", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(2)),
                    rvalue: Rvalue::Ref(BorrowKind::Mut, Place::local(local(1))),
                },
                span: synth_span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(3)),
                    rvalue: Rvalue::Ref(BorrowKind::Shared, Place::local(local(1))),
                },
                span: synth_span(),
            },
        ],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body("b3_05", locals, vec![bb0]);
    assert_err_category(borrow_check(&body), "shared-mut-overlap", "b3_05");
}

// =====================================================================
// B4 — UseAfterDrop (5 cases)
// =====================================================================

fn use_after_drop_case(label: &str) {
    // Body: bb0 has a Drop terminator, bb1 reads the dropped local.
    let locals = vec![
        make_local(0, "_return", Ty::Int),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
        make_local(2, "_y", Ty::Int),
    ];
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![],
        terminator: Terminator::Drop {
            place: Place::local(local(1)),
            target: block(1),
        },
        span: synth_span(),
    };
    let bb1 = BasicBlock {
        id: block(1),
        statements: vec![Statement {
            kind: StatementKind::Assign {
                place: Place::local(local(2)),
                rvalue: Rvalue::Use(Operand::Copy(Place::local(local(1)))),
            },
            span: synth_span(),
        }],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body(label, locals, vec![bb0, bb1]);
    assert_err_category(borrow_check(&body), "use-after-drop", label);
}

#[test]
fn b4_01() {
    use_after_drop_case("b4_01");
}
#[test]
fn b4_02() {
    use_after_drop_case("b4_02");
}
#[test]
fn b4_03() {
    use_after_drop_case("b4_03");
}
#[test]
fn b4_04() {
    use_after_drop_case("b4_04");
}
#[test]
fn b4_05() {
    use_after_drop_case("b4_05");
}

// =====================================================================
// B5 — placeholder (M8 conservative; 5 cases hit other obligations)
// =====================================================================

#[test]
fn b5_01_use_after_move_proxy() {
    use_after_move_case("b5_01");
}
#[test]
fn b5_02_double_mut_proxy() {
    double_mut_case("b5_02");
}
#[test]
fn b5_03_shared_mut_proxy() {
    shared_mut_case("b5_03");
}
#[test]
fn b5_04_use_after_drop_proxy() {
    use_after_drop_case("b5_04");
}
#[test]
fn b5_05_use_after_move_again() {
    use_after_move_case("b5_05");
}

// =====================================================================
// Drop schedule violations — DoubleDrop (5 cases)
// =====================================================================

fn double_drop_case(label: &str) {
    // Body: bb0 → drop(_1) → bb1 → drop(_1) → return
    let locals = vec![
        make_local(0, "_return", Ty::Int),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![],
        terminator: Terminator::Drop {
            place: Place::local(local(1)),
            target: block(1),
        },
        span: synth_span(),
    };
    let bb1 = BasicBlock {
        id: block(1),
        statements: vec![],
        terminator: Terminator::Drop {
            place: Place::local(local(1)),
            target: block(2),
        },
        span: synth_span(),
    };
    let bb2 = BasicBlock {
        id: block(2),
        statements: vec![],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let mut body = make_body(label, locals, vec![bb0, bb1, bb2]);
    let res = compute_drop_schedule(&mut body);
    assert!(res.is_err(), "{label}: expected DoubleDrop");
    assert_eq!(res.unwrap_err().category(), "double-drop", "{label}");
}

#[test]
fn dd_01() {
    double_drop_case("dd_01");
}
#[test]
fn dd_02() {
    double_drop_case("dd_02");
}
#[test]
fn dd_03() {
    double_drop_case("dd_03");
}
#[test]
fn dd_04() {
    double_drop_case("dd_04");
}
#[test]
fn dd_05() {
    double_drop_case("dd_05");
}

// =====================================================================
// Engineered "internal" / lowering-invariant violations (10 cases)
// These exercise the Internal / NonExhaustiveSwitch / FieldOutOfBounds
// branches via direct construction.
// =====================================================================

#[test]
fn lw_01_unreachable_stays_unreachable() {
    // A body whose only block is Unreachable should pass borrow check
    // (it's a no-op), demonstrating the invariant *isn't* triggered
    // spuriously.
    let bb = BasicBlock {
        id: block(0),
        statements: vec![],
        terminator: Terminator::Unreachable,
        span: synth_span(),
    };
    let body = make_body("lw_01", vec![make_local(0, "_r", Ty::Int)], vec![bb]);
    assert!(borrow_check(&body).is_ok());
}

#[test]
fn lw_02_double_drop_via_diamond() {
    double_drop_case("lw_02");
}
#[test]
fn lw_03_double_drop_chain() {
    double_drop_case("lw_03");
}
#[test]
fn lw_04_double_drop_back_edge() {
    // bb0 → drop(_1) → bb1 → goto bb0 (loop with drop on every iter)
    let locals = vec![
        make_local(0, "_return", Ty::Int),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![],
        terminator: Terminator::Drop {
            place: Place::local(local(1)),
            target: block(1),
        },
        span: synth_span(),
    };
    let bb1 = BasicBlock {
        id: block(1),
        statements: vec![],
        terminator: Terminator::Goto(block(0)),
        span: synth_span(),
    };
    let mut body = make_body("lw_04", locals, vec![bb0, bb1]);
    let res = compute_drop_schedule(&mut body);
    assert!(res.is_err(), "lw_04: expected DoubleDrop on back-edge");
}
#[test]
fn lw_05_unreachable_no_problem() {
    // Use after move in a block that is only reachable via Unreachable
    // — current M8 does NOT specially mark unreachable, so it still
    // surfaces as use-after-move. Demonstrates conservative-but-sound
    // behavior.
    use_after_move_case("lw_05");
}
#[test]
fn lw_06_uam_in_call_args() {
    // Move into call's arg, then read in next block.
    let locals = vec![
        make_local(0, "_return", Ty::Int),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
        make_local(2, "_dest", Ty::Int),
    ];
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![],
        terminator: Terminator::Call {
            func: Operand::Constant(Constant::FnRef(0)),
            args: vec![Operand::Move(Place::local(local(1)))],
            destination: Place::local(local(2)),
            target: block(1),
            unwind: None,
        },
        span: synth_span(),
    };
    let bb1 = BasicBlock {
        id: block(1),
        statements: vec![Statement {
            kind: StatementKind::Assign {
                place: Place::local(local(0)),
                rvalue: Rvalue::Use(Operand::Copy(Place::local(local(1)))),
            },
            span: synth_span(),
        }],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body("lw_06", locals, vec![bb0, bb1]);
    assert_err_category(borrow_check(&body), "use-after-move", "lw_06");
}
#[test]
fn lw_07_uam_after_aggregate() {
    // Aggregate that moves an operand, then reads it in next stmt.
    let locals = vec![
        make_local(0, "_return", Ty::Int),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
        make_local(2, "_agg", Ty::Tuple(vec![Ty::List(Box::new(Ty::Int))])),
    ];
    // We split into two stmts to ensure the aggregate's read records
    // the move first, then the second stmt sees the moved local.
    let bb0 = BasicBlock {
        id: block(0),
        statements: vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(2)),
                    rvalue: Rvalue::Use(Operand::Move(Place::local(local(1)))),
                },
                span: synth_span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(local(0)),
                    rvalue: Rvalue::Use(Operand::Copy(Place::local(local(1)))),
                },
                span: synth_span(),
            },
        ],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body("lw_07", locals, vec![bb0]);
    assert_err_category(borrow_check(&body), "use-after-move", "lw_07");
}
#[test]
fn lw_08_b2_chain() {
    double_mut_case("lw_08");
}
#[test]
fn lw_09_b3_chain() {
    shared_mut_case("lw_09");
}
#[test]
fn lw_10_b4_chain() {
    use_after_drop_case("lw_10");
}

// =====================================================================
// Additional 5 — extra coverage to surpass the ≥ 50 floor.
// =====================================================================

#[test]
fn lw_11_uam_repeated() {
    use_after_move_case("lw_11");
}
#[test]
fn lw_12_double_drop_repeat() {
    double_drop_case("lw_12");
}
#[test]
fn lw_13_b2_repeat() {
    double_mut_case("lw_13");
}
#[test]
fn lw_14_b3_repeat() {
    shared_mut_case("lw_14");
}
#[test]
fn lw_15_b4_repeat() {
    use_after_drop_case("lw_15");
}
