//! M12.x `Rvalue::Cast` corpus (per ADR-0027 §3).
//!
//! Each test exercises one row of the conversion table:
//!   i32→i64 / i64→i32 (sext / ireduce)
//!   i32/i64 → f32/f64 (fcvt_from_sint)
//!   f32/f64 → i32/i64 (fcvt_to_sint_sat)
//!   f32 ↔ f64 (fpromote / fdemote)
//!   bool → int (uextend)
//!   int → bool (icmp neq 0)

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
    BasicBlock, BlockId, Body, CastKind, Constant, LocalDecl, LocalId, Module as MirModule,
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

fn assign_float(target: u32, v: f64) -> Statement {
    Statement {
        kind: StatementKind::Assign {
            place: Place::local(LocalId(target)),
            rvalue: Rvalue::Use(Operand::Constant(Constant::Float(v.to_bits()))),
        },
        span: span(),
    }
}

fn assign_bool(target: u32, v: bool) -> Statement {
    Statement {
        kind: StatementKind::Assign {
            place: Place::local(LocalId(target)),
            rvalue: Rvalue::Use(Operand::Constant(Constant::Bool(v))),
        },
        span: span(),
    }
}

fn assign_cast(target: u32, src: u32, kind: CastKind, ty: Ty) -> Statement {
    Statement {
        kind: StatementKind::Assign {
            place: Place::local(LocalId(target)),
            rvalue: Rvalue::Cast(kind, Operand::Copy(Place::local(LocalId(src))), ty),
        },
        span: span(),
    }
}

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
// IntToFloat — i64 → f64
// =====================================================================

#[test]
fn cast_i64_to_f64_zero() {
    let b = body(
        "c01",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Float, true),
        ],
        vec![
            assign_int(1, 0),
            assign_cast(2, 1, CastKind::IntToFloat, Ty::Float),
            assign_int(0, 0),
        ],
    );
    compile("c01", b);
}

#[test]
fn cast_i64_to_f64_pos() {
    let b = body(
        "c02",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Float, true),
        ],
        vec![
            assign_int(1, 42),
            assign_cast(2, 1, CastKind::IntToFloat, Ty::Float),
            assign_int(0, 0),
        ],
    );
    compile("c02", b);
}

#[test]
fn cast_i64_to_f64_neg() {
    let b = body(
        "c03",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Float, true),
        ],
        vec![
            assign_int(1, -7),
            assign_cast(2, 1, CastKind::IntToFloat, Ty::Float),
            assign_int(0, 0),
        ],
    );
    compile("c03", b);
}

#[test]
fn cast_i64_to_f64_max() {
    let b = body(
        "c04",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Float, true),
        ],
        vec![
            assign_int(1, i64::MAX),
            assign_cast(2, 1, CastKind::IntToFloat, Ty::Float),
            assign_int(0, 0),
        ],
    );
    compile("c04", b);
}

#[test]
fn cast_i64_to_f64_min() {
    let b = body(
        "c05",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Float, true),
        ],
        vec![
            assign_int(1, i64::MIN),
            assign_cast(2, 1, CastKind::IntToFloat, Ty::Float),
            assign_int(0, 0),
        ],
    );
    compile("c05", b);
}

// =====================================================================
// FloatToInt — f64 → i64
// =====================================================================

#[test]
fn cast_f64_to_i64_zero() {
    let b = body(
        "c10",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Float, true),
            local(2, "i", Ty::Int, true),
        ],
        vec![
            assign_float(1, 0.0),
            assign_cast(2, 1, CastKind::FloatToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c10", b);
}

#[test]
fn cast_f64_to_i64_int_value() {
    let b = body(
        "c11",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Float, true),
            local(2, "i", Ty::Int, true),
        ],
        vec![
            assign_float(1, 42.0),
            assign_cast(2, 1, CastKind::FloatToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c11", b);
}

#[test]
fn cast_f64_to_i64_fractional_truncates() {
    let b = body(
        "c12",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Float, true),
            local(2, "i", Ty::Int, true),
        ],
        vec![
            assign_float(1, 3.7),
            assign_cast(2, 1, CastKind::FloatToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c12", b);
}

#[test]
fn cast_f64_to_i64_neg() {
    let b = body(
        "c13",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Float, true),
            local(2, "i", Ty::Int, true),
        ],
        vec![
            assign_float(1, -3.5),
            assign_cast(2, 1, CastKind::FloatToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c13", b);
}

#[test]
fn cast_f64_to_i64_nan_saturates() {
    let b = body(
        "c14",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Float, true),
            local(2, "i", Ty::Int, true),
        ],
        vec![
            assign_float(1, f64::NAN),
            assign_cast(2, 1, CastKind::FloatToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c14", b);
}

#[test]
fn cast_f64_to_i64_inf_saturates_max() {
    let b = body(
        "c15",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Float, true),
            local(2, "i", Ty::Int, true),
        ],
        vec![
            assign_float(1, f64::INFINITY),
            assign_cast(2, 1, CastKind::FloatToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c15", b);
}

#[test]
fn cast_f64_to_i64_neg_inf_saturates_min() {
    let b = body(
        "c16",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Float, true),
            local(2, "i", Ty::Int, true),
        ],
        vec![
            assign_float(1, f64::NEG_INFINITY),
            assign_cast(2, 1, CastKind::FloatToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c16", b);
}

// =====================================================================
// BoolToInt — i8 → i64
// =====================================================================

#[test]
fn cast_bool_true_to_int() {
    let b = body(
        "c20",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Bool, true),
            local(2, "i", Ty::Int, true),
        ],
        vec![
            assign_bool(1, true),
            assign_cast(2, 1, CastKind::BoolToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c20", b);
}

#[test]
fn cast_bool_false_to_int() {
    let b = body(
        "c21",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Bool, true),
            local(2, "i", Ty::Int, true),
        ],
        vec![
            assign_bool(1, false),
            assign_cast(2, 1, CastKind::BoolToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c21", b);
}

#[test]
fn cast_bool_then_int_arith() {
    let b = body(
        "c22",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Bool, true),
            local(2, "i", Ty::Int, true),
            local(3, "j", Ty::Int, true),
        ],
        vec![
            assign_bool(1, true),
            assign_cast(2, 1, CastKind::BoolToInt, Ty::Int),
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(3)),
                    rvalue: Rvalue::BinaryOp(
                        cobrust_mir::BinOp::Add,
                        Operand::Copy(Place::local(LocalId(2))),
                        Operand::Constant(Constant::Int(1)),
                    ),
                },
                span: span(),
            },
            assign_int(0, 0),
        ],
    );
    compile("c22", b);
}

// =====================================================================
// IntToBool — i64 → i8
// =====================================================================

#[test]
fn cast_int_zero_to_bool() {
    let b = body(
        "c30",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Bool, true),
        ],
        vec![
            assign_int(1, 0),
            assign_cast(2, 1, CastKind::IntToBool, Ty::Bool),
            assign_int(0, 0),
        ],
    );
    compile("c30", b);
}

#[test]
fn cast_int_pos_to_bool() {
    let b = body(
        "c31",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Bool, true),
        ],
        vec![
            assign_int(1, 42),
            assign_cast(2, 1, CastKind::IntToBool, Ty::Bool),
            assign_int(0, 0),
        ],
    );
    compile("c31", b);
}

#[test]
fn cast_int_neg_to_bool() {
    let b = body(
        "c32",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Bool, true),
        ],
        vec![
            assign_int(1, -7),
            assign_cast(2, 1, CastKind::IntToBool, Ty::Bool),
            assign_int(0, 0),
        ],
    );
    compile("c32", b);
}

// =====================================================================
// Composed casts
// =====================================================================

#[test]
fn cast_int_to_float_to_int_round_trip() {
    let b = body(
        "c40",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Float, true),
            local(3, "y", Ty::Int, true),
        ],
        vec![
            assign_int(1, 42),
            assign_cast(2, 1, CastKind::IntToFloat, Ty::Float),
            assign_cast(3, 2, CastKind::FloatToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c40", b);
}

#[test]
fn cast_bool_to_int_to_bool_round_trip() {
    let b = body(
        "c41",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Bool, true),
            local(2, "i", Ty::Int, true),
            local(3, "g", Ty::Bool, true),
        ],
        vec![
            assign_bool(1, true),
            assign_cast(2, 1, CastKind::BoolToInt, Ty::Int),
            assign_cast(3, 2, CastKind::IntToBool, Ty::Bool),
            assign_int(0, 0),
        ],
    );
    compile("c41", b);
}

#[test]
fn cast_chain_int_float_int_bool() {
    let b = body(
        "c42",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Float, true),
            local(3, "y", Ty::Int, true),
            local(4, "g", Ty::Bool, true),
        ],
        vec![
            assign_int(1, 7),
            assign_cast(2, 1, CastKind::IntToFloat, Ty::Float),
            assign_cast(3, 2, CastKind::FloatToInt, Ty::Int),
            assign_cast(4, 3, CastKind::IntToBool, Ty::Bool),
            assign_int(0, 0),
        ],
    );
    compile("c42", b);
}

// =====================================================================
// Cast in expression contexts
// =====================================================================

#[test]
fn cast_after_binop_int_to_float() {
    let b = body(
        "c50",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "y", Ty::Int, true),
            local(3, "f", Ty::Float, true),
        ],
        vec![
            assign_int(1, 1),
            assign_int(2, 2),
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(1)),
                    rvalue: Rvalue::BinaryOp(
                        cobrust_mir::BinOp::Add,
                        Operand::Copy(Place::local(LocalId(1))),
                        Operand::Copy(Place::local(LocalId(2))),
                    ),
                },
                span: span(),
            },
            assign_cast(3, 1, CastKind::IntToFloat, Ty::Float),
            assign_int(0, 0),
        ],
    );
    compile("c50", b);
}

#[test]
fn cast_in_branch_int_to_float() {
    let mut b = body(
        "c51",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Float, true),
            local(3, "cond", Ty::Bool, true),
        ],
        vec![],
    );
    b.blocks[0].statements = vec![assign_int(1, 5), assign_bool(3, true)];
    b.blocks[0].terminator = Terminator::SwitchInt {
        operand: Operand::Copy(Place::local(LocalId(3))),
        cases: vec![(cobrust_mir::SwitchValue::Bool(true), BlockId(1))],
        otherwise: BlockId(2),
    };
    b.blocks.push(BasicBlock {
        id: BlockId(1),
        statements: vec![assign_cast(2, 1, CastKind::IntToFloat, Ty::Float)],
        terminator: Terminator::Goto(BlockId(2)),
        span: span(),
    });
    b.blocks.push(BasicBlock {
        id: BlockId(2),
        statements: vec![assign_int(0, 0)],
        terminator: Terminator::Return,
        span: span(),
    });
    compile("c51", b);
}

#[test]
fn cast_int_to_float_in_loop_body() {
    let mut b = body(
        "c52",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Float, true),
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
            assign_cast(2, 1, CastKind::IntToFloat, Ty::Float),
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
    compile("c52", b);
}

#[test]
fn cast_float_to_int_then_arith() {
    let b = body(
        "c53",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "f", Ty::Float, true),
            local(2, "i", Ty::Int, true),
            local(3, "j", Ty::Int, true),
        ],
        vec![
            assign_float(1, 5.5),
            assign_cast(2, 1, CastKind::FloatToInt, Ty::Int),
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(3)),
                    rvalue: Rvalue::BinaryOp(
                        cobrust_mir::BinOp::Mul,
                        Operand::Copy(Place::local(LocalId(2))),
                        Operand::Constant(Constant::Int(2)),
                    ),
                },
                span: span(),
            },
            assign_int(0, 0),
        ],
    );
    compile("c53", b);
}

#[test]
fn cast_int_to_bool_for_branch() {
    let mut b = body(
        "c54",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Bool, true),
        ],
        vec![],
    );
    b.blocks[0].statements = vec![
        assign_int(1, 1),
        assign_cast(2, 1, CastKind::IntToBool, Ty::Bool),
    ];
    b.blocks[0].terminator = Terminator::SwitchInt {
        operand: Operand::Copy(Place::local(LocalId(2))),
        cases: vec![(cobrust_mir::SwitchValue::Bool(true), BlockId(1))],
        otherwise: BlockId(1),
    };
    b.blocks.push(BasicBlock {
        id: BlockId(1),
        statements: vec![assign_int(0, 0)],
        terminator: Terminator::Return,
        span: span(),
    });
    compile("c54", b);
}

#[test]
fn cast_two_floats_to_ints_compose() {
    let b = body(
        "c55",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "a", Ty::Float, true),
            local(2, "b", Ty::Float, true),
            local(3, "ai", Ty::Int, true),
            local(4, "bi", Ty::Int, true),
        ],
        vec![
            assign_float(1, 1.5),
            assign_float(2, 2.5),
            assign_cast(3, 1, CastKind::FloatToInt, Ty::Int),
            assign_cast(4, 2, CastKind::FloatToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c55", b);
}

#[test]
fn cast_two_ints_to_floats_compose() {
    let b = body(
        "c56",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "a", Ty::Int, true),
            local(2, "b", Ty::Int, true),
            local(3, "af", Ty::Float, true),
            local(4, "bf", Ty::Float, true),
        ],
        vec![
            assign_int(1, 3),
            assign_int(2, 4),
            assign_cast(3, 1, CastKind::IntToFloat, Ty::Float),
            assign_cast(4, 2, CastKind::IntToFloat, Ty::Float),
            assign_int(0, 0),
        ],
    );
    compile("c56", b);
}

#[test]
fn cast_bool_pair_to_int_pair() {
    let b = body(
        "c57",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "p", Ty::Bool, true),
            local(2, "q", Ty::Bool, true),
            local(3, "pi", Ty::Int, true),
            local(4, "qi", Ty::Int, true),
        ],
        vec![
            assign_bool(1, true),
            assign_bool(2, false),
            assign_cast(3, 1, CastKind::BoolToInt, Ty::Int),
            assign_cast(4, 2, CastKind::BoolToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c57", b);
}

#[test]
fn cast_int_to_float_then_float_arith() {
    let b = body(
        "c58",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Float, true),
            local(3, "g", Ty::Float, true),
        ],
        vec![
            assign_int(1, 5),
            assign_cast(2, 1, CastKind::IntToFloat, Ty::Float),
            assign_float(3, 2.5),
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(3)),
                    rvalue: Rvalue::BinaryOp(
                        cobrust_mir::BinOp::Add,
                        Operand::Copy(Place::local(LocalId(2))),
                        Operand::Copy(Place::local(LocalId(3))),
                    ),
                },
                span: span(),
            },
            assign_int(0, 0),
        ],
    );
    compile("c58", b);
}

#[test]
fn cast_int_to_float_neg_then_back() {
    let b = body(
        "c59",
        vec![
            local(0, "_return", Ty::Int, true),
            local(1, "x", Ty::Int, true),
            local(2, "f", Ty::Float, true),
            local(3, "y", Ty::Int, true),
        ],
        vec![
            assign_int(1, -100),
            assign_cast(2, 1, CastKind::IntToFloat, Ty::Float),
            assign_cast(3, 2, CastKind::FloatToInt, Ty::Int),
            assign_int(0, 0),
        ],
    );
    compile("c59", b);
}
