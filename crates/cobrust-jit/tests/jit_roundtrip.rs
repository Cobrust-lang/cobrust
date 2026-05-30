//! ADR-0056a wave-1 round-trip tests.
//!
//! These tests construct MIR bodies by hand and drive the
//! `JitEngine` → `JitHandle::call` round trip. They are the
//! authoritative validation surface for ADR-0056a §3.3 + §5
//! ("JIT module construction + finalize" / "Mode dispatch +
//! signature contract").
//!
//! Test surface (10 tests):
//! 1. const_return — `fn() -> i64 { 42 }`
//! 2. add_two_params — `fn(i64, i64) -> i64 { a + b }`
//! 3. sub_two_params — `fn(i64, i64) -> i64 { a - b }`
//! 4. mul_two_params — `fn(i64, i64) -> i64 { a * b }`
//! 5. constant_arith — `fn() -> i64 { 1 + 2 * 3 }` (the canonical
//!    ADR-0056a §1 round-trip example)
//! 6. unary_neg — `fn(i64) -> i64 { -x }`
//! 7. three_param_sum — `fn(i64, i64, i64) -> i64 { a + b + c }`
//! 8. signature_mismatch_caught — caller asks for `(i64,) -> i64`
//!    on a fn declared `() -> i64`; expect typed error not SIGSEGV
//! 9. unknown_fn_caught — `JitHandle::call("nope", ())` → typed
//!    error
//! 10. unsupported_mir_feature — body uses Terminator::Call which
//!     is wave-2+; expect typed error
//!
//! Tests construct MIR bodies via `mk_body` helper that emits the
//! single-block shape `block0: stmts; return _ret`.

// Test files: allow unwrap/expect noise per CTO runbook (matches
// the pedantic-on-tests stall pattern in MEMORY.md — module-level
// allow, not per-call-site).
#![allow(clippy::unwrap_used)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::items_after_statements)]

use cobrust_frontend::span::{FileId, Span};
use cobrust_hir::DefId;
use cobrust_jit::{JitEngine, JitError};
use cobrust_mir::{
    BasicBlock, BinOp, BlockId, Body, Constant, LocalDecl, LocalId, Module, Operand, Place, Rvalue,
    Statement, StatementKind, Terminator, UnOp,
};
use cobrust_types::Ty;

const FILE: FileId = FileId::SYNTHETIC;
const SPAN: Span = Span {
    file: FileId::SYNTHETIC,
    start: 0,
    end: 0,
};

/// Build a single-body `Module` for one `fn name(params: [Ty::Int; n]) -> Ty::Int { ...stmts; return }`.
///
/// `body_stmts` is a closure that, given the param LocalIds (in
/// order) and the return LocalId, emits the `Vec<Statement>` that
/// computes the return value (assigning it to the return local).
fn mk_module<F>(name: &str, param_count: usize, body_stmts: F) -> Module
where
    F: FnOnce(&[LocalId], LocalId) -> Vec<Statement>,
{
    // MIR convention (lower.rs `BodyBuilder::new`): locals[0] is the
    // synthetic return slot, params follow at locals[1..]. We mirror
    // that so the JIT signature builder reads the convention right.
    let return_local = LocalId(0);
    let mut locals = vec![LocalDecl {
        id: return_local,
        name: "_return".to_string(),
        ty: Ty::None, // MIR return slot is always None; JIT pins to I64
        mutable: true,
        span: SPAN,
        validated_body_of: None,
    }];
    let mut param_ids = Vec::with_capacity(param_count);
    for i in 0..param_count {
        let id = LocalId((i + 1) as u32);
        locals.push(LocalDecl {
            id,
            name: format!("a{i}"),
            ty: Ty::Int,
            mutable: false,
            span: SPAN,
            validated_body_of: None,
        });
        param_ids.push(id);
    }

    let statements = body_stmts(&param_ids, return_local);

    let block = BasicBlock {
        id: BlockId(0),
        statements,
        terminator: Terminator::Return,
        span: SPAN,
    };

    let body = Body {
        def_id: DefId(1),
        name: name.to_string(),
        locals,
        blocks: vec![block],
        return_local,
        param_count,
        span: SPAN,
    };

    Module { bodies: vec![body] }
}

fn assign(local: LocalId, rvalue: Rvalue) -> Statement {
    Statement {
        kind: StatementKind::Assign {
            place: Place::local(local),
            rvalue,
        },
        span: SPAN,
    }
}

#[test]
fn const_return_42() {
    // fn ret42() -> i64 { return 42 }
    let module = mk_module("ret42", 0, |_params, ret| {
        vec![assign(
            ret,
            Rvalue::Use(Operand::Constant(Constant::Int(42))),
        )]
    });
    let engine = JitEngine::new().expect("JitEngine::new should succeed on host ISA");
    let handle = engine.compile_mir(&module).expect("compile should succeed");

    // SAFETY: we just compiled `ret42` with signature `() -> i64`;
    // calling with `()` args / `i64` return matches.
    let result: i64 = unsafe { handle.call("ret42", ()).expect("call should succeed") };
    assert_eq!(result, 42);
}

#[test]
fn add_two_params() {
    // fn add(a: i64, b: i64) -> i64 { return a + b }
    let module = mk_module("add", 2, |params, ret| {
        let a = params[0];
        let b = params[1];
        vec![assign(
            ret,
            Rvalue::BinaryOp(
                BinOp::Add,
                Operand::Copy(Place::local(a)),
                Operand::Copy(Place::local(b)),
            ),
        )]
    });
    let engine = JitEngine::new().unwrap();
    let handle = engine.compile_mir(&module).unwrap();

    let result: i64 = unsafe { handle.call("add", (3i64, 4i64)).unwrap() };
    assert_eq!(result, 7);

    let result: i64 = unsafe { handle.call("add", (-100i64, 50i64)).unwrap() };
    assert_eq!(result, -50);
}

#[test]
fn sub_two_params() {
    let module = mk_module("sub", 2, |params, ret| {
        vec![assign(
            ret,
            Rvalue::BinaryOp(
                BinOp::Sub,
                Operand::Copy(Place::local(params[0])),
                Operand::Copy(Place::local(params[1])),
            ),
        )]
    });
    let engine = JitEngine::new().unwrap();
    let handle = engine.compile_mir(&module).unwrap();
    let result: i64 = unsafe { handle.call("sub", (10i64, 3i64)).unwrap() };
    assert_eq!(result, 7);
}

#[test]
fn mul_two_params() {
    let module = mk_module("mul", 2, |params, ret| {
        vec![assign(
            ret,
            Rvalue::BinaryOp(
                BinOp::Mul,
                Operand::Copy(Place::local(params[0])),
                Operand::Copy(Place::local(params[1])),
            ),
        )]
    });
    let engine = JitEngine::new().unwrap();
    let handle = engine.compile_mir(&module).unwrap();
    let result: i64 = unsafe { handle.call("mul", (6i64, 7i64)).unwrap() };
    assert_eq!(result, 42);
}

#[test]
fn constant_arith_one_plus_two_times_three() {
    // The canonical ADR-0056a §1 example: `1 + 2 * 3` → 7.
    //
    // MIR shape (with associative-left multiplication first):
    //   _tmp = 2 * 3
    //   _ret = 1 + _tmp
    //
    // Locals: _0 = return; _1 = _tmp.
    let return_local = LocalId(0);
    let tmp = LocalId(1);
    let module = Module {
        bodies: vec![Body {
            def_id: DefId(1),
            name: "main".to_string(),
            locals: vec![
                LocalDecl {
                    id: return_local,
                    name: "_return".to_string(),
                    ty: Ty::None,
                    mutable: true,
                    span: SPAN,
                    validated_body_of: None,
                },
                LocalDecl {
                    id: tmp,
                    name: "_tmp".to_string(),
                    ty: Ty::Int,
                    mutable: false,
                    span: SPAN,
                    validated_body_of: None,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                statements: vec![
                    assign(
                        tmp,
                        Rvalue::BinaryOp(
                            BinOp::Mul,
                            Operand::Constant(Constant::Int(2)),
                            Operand::Constant(Constant::Int(3)),
                        ),
                    ),
                    assign(
                        return_local,
                        Rvalue::BinaryOp(
                            BinOp::Add,
                            Operand::Constant(Constant::Int(1)),
                            Operand::Copy(Place::local(tmp)),
                        ),
                    ),
                ],
                terminator: Terminator::Return,
                span: SPAN,
            }],
            return_local,
            param_count: 0,
            span: SPAN,
        }],
    };

    let engine = JitEngine::new().unwrap();
    let handle = engine.compile_mir(&module).unwrap();
    // Body name was "main" → AOT export-name convention renames to
    // "_cobrust_user_main"; JitHandle::function_names() reflects this.
    let names = handle.function_names();
    assert!(
        names.contains(&"_cobrust_user_main"),
        "expected _cobrust_user_main, got {names:?}"
    );
    let result: i64 = unsafe { handle.call("_cobrust_user_main", ()).unwrap() };
    assert_eq!(result, 7, "1 + 2 * 3 should JIT-evaluate to 7");
}

#[test]
fn unary_neg() {
    let module = mk_module("negate", 1, |params, ret| {
        vec![assign(
            ret,
            Rvalue::UnaryOp(UnOp::Neg, Operand::Copy(Place::local(params[0]))),
        )]
    });
    let engine = JitEngine::new().unwrap();
    let handle = engine.compile_mir(&module).unwrap();
    let result: i64 = unsafe { handle.call("negate", (5i64,)).unwrap() };
    assert_eq!(result, -5);
    let result: i64 = unsafe { handle.call("negate", (-7i64,)).unwrap() };
    assert_eq!(result, 7);
}

#[test]
fn three_param_sum() {
    // fn sum3(a, b, c) -> i64 { _tmp = a + b; return _tmp + c }
    let return_local = LocalId(0);
    let a = LocalId(1);
    let b = LocalId(2);
    let c = LocalId(3);
    let tmp = LocalId(4);
    let module = Module {
        bodies: vec![Body {
            def_id: DefId(1),
            name: "sum3".to_string(),
            locals: vec![
                LocalDecl {
                    id: return_local,
                    name: "_return".into(),
                    ty: Ty::None,
                    mutable: true,
                    span: SPAN,
                    validated_body_of: None,
                },
                LocalDecl {
                    id: a,
                    name: "a".into(),
                    ty: Ty::Int,
                    mutable: false,
                    span: SPAN,
                    validated_body_of: None,
                },
                LocalDecl {
                    id: b,
                    name: "b".into(),
                    ty: Ty::Int,
                    mutable: false,
                    span: SPAN,
                    validated_body_of: None,
                },
                LocalDecl {
                    id: c,
                    name: "c".into(),
                    ty: Ty::Int,
                    mutable: false,
                    span: SPAN,
                    validated_body_of: None,
                },
                LocalDecl {
                    id: tmp,
                    name: "_tmp".into(),
                    ty: Ty::Int,
                    mutable: false,
                    span: SPAN,
                    validated_body_of: None,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                statements: vec![
                    assign(
                        tmp,
                        Rvalue::BinaryOp(
                            BinOp::Add,
                            Operand::Copy(Place::local(a)),
                            Operand::Copy(Place::local(b)),
                        ),
                    ),
                    assign(
                        return_local,
                        Rvalue::BinaryOp(
                            BinOp::Add,
                            Operand::Copy(Place::local(tmp)),
                            Operand::Copy(Place::local(c)),
                        ),
                    ),
                ],
                terminator: Terminator::Return,
                span: SPAN,
            }],
            return_local,
            param_count: 3,
            span: SPAN,
        }],
    };

    let engine = JitEngine::new().unwrap();
    let handle = engine.compile_mir(&module).unwrap();
    let result: i64 = unsafe { handle.call("sum3", (10i64, 20i64, 30i64)).unwrap() };
    assert_eq!(result, 60);
}

#[test]
fn signature_mismatch_caught_before_transmute() {
    // Compiled fn is () -> i64. Caller asks for (i64,) -> i64. Must
    // surface JitError::SignatureMismatch, NOT segfault.
    let module = mk_module("nullary", 0, |_p, ret| {
        vec![assign(
            ret,
            Rvalue::Use(Operand::Constant(Constant::Int(99))),
        )]
    });
    let engine = JitEngine::new().unwrap();
    let handle = engine.compile_mir(&module).unwrap();

    // Wrong arg count: function takes (), caller passes (i64,).
    let result: Result<i64, JitError> = unsafe { handle.call("nullary", (1i64,)) };
    match result {
        Err(JitError::SignatureMismatch { expected, actual }) => {
            // expected is the caller's claim ("[I64]"), actual is the
            // compiled signature ("[]"); the trait shape is wired
            // such that caller params show as `expected`.
            assert!(
                expected.contains("I64"),
                "expected message should reference I64: {expected}"
            );
            assert!(
                !actual.contains("I64"),
                "actual (compiled) should be param-less: {actual}"
            );
        }
        other => panic!("expected SignatureMismatch, got {other:?}"),
    }
}

#[test]
fn unknown_function_name_caught() {
    let module = mk_module("known", 0, |_p, ret| {
        vec![assign(
            ret,
            Rvalue::Use(Operand::Constant(Constant::Int(0))),
        )]
    });
    let engine = JitEngine::new().unwrap();
    let handle = engine.compile_mir(&module).unwrap();
    let result: Result<i64, JitError> = unsafe { handle.call("does_not_exist", ()) };
    match result {
        Err(JitError::NoSuchFunction { name }) => assert_eq!(name, "does_not_exist"),
        other => panic!("expected NoSuchFunction, got {other:?}"),
    }
}

#[test]
fn unsupported_mir_feature_call_terminator() {
    // Body uses Terminator::Call (wave-2+); compile_mir must reject
    // with a typed error so the REPL Session can route to AOT one-shot.
    //
    // We construct a 2-block body: block0 calls some_fn(); block1
    // returns. Since wave-1 doesn't lower Terminator::Call, compile
    // should error.
    let return_local = LocalId(0);
    let tmp = LocalId(1);
    let module = Module {
        bodies: vec![Body {
            def_id: DefId(1),
            name: "uses_call".to_string(),
            locals: vec![
                LocalDecl {
                    id: return_local,
                    name: "_return".into(),
                    ty: Ty::None,
                    mutable: true,
                    span: SPAN,
                    validated_body_of: None,
                },
                LocalDecl {
                    id: tmp,
                    name: "_callret".into(),
                    ty: Ty::Int,
                    mutable: false,
                    span: SPAN,
                    validated_body_of: None,
                },
            ],
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Terminator::Call {
                        func: Operand::Constant(Constant::Str("some_fn".into())),
                        args: vec![],
                        destination: Place::local(tmp),
                        target: BlockId(1),
                        unwind: None,
                    },
                    span: SPAN,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![assign(
                        return_local,
                        Rvalue::Use(Operand::Copy(Place::local(tmp))),
                    )],
                    terminator: Terminator::Return,
                    span: SPAN,
                },
            ],
            return_local,
            param_count: 0,
            span: SPAN,
        }],
    };

    let engine = JitEngine::new().unwrap();
    let result = engine.compile_mir(&module);
    match result {
        Err(JitError::UnsupportedMirFeature { feature }) => {
            assert!(
                feature.contains("Call"),
                "feature should mention Call: {feature}"
            );
        }
        Ok(_) => panic!("expected UnsupportedMirFeature for Terminator::Call in wave-1"),
        Err(other) => panic!("expected UnsupportedMirFeature, got {other:?}"),
    }
}

#[test]
fn function_names_introspection() {
    // Two functions in one module; function_names() should list both
    // (in some order — HashMap iteration is unordered).
    let return_local = LocalId(0);
    let module = Module {
        bodies: vec![
            Body {
                def_id: DefId(1),
                name: "fn_a".into(),
                locals: vec![LocalDecl {
                    id: return_local,
                    name: "_return".into(),
                    ty: Ty::None,
                    mutable: true,
                    span: SPAN,
                    validated_body_of: None,
                }],
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    statements: vec![assign(
                        return_local,
                        Rvalue::Use(Operand::Constant(Constant::Int(1))),
                    )],
                    terminator: Terminator::Return,
                    span: SPAN,
                }],
                return_local,
                param_count: 0,
                span: SPAN,
            },
            Body {
                def_id: DefId(2),
                name: "fn_b".into(),
                locals: vec![LocalDecl {
                    id: return_local,
                    name: "_return".into(),
                    ty: Ty::None,
                    mutable: true,
                    span: SPAN,
                    validated_body_of: None,
                }],
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    statements: vec![assign(
                        return_local,
                        Rvalue::Use(Operand::Constant(Constant::Int(2))),
                    )],
                    terminator: Terminator::Return,
                    span: SPAN,
                }],
                return_local,
                param_count: 0,
                span: SPAN,
            },
        ],
    };
    let engine = JitEngine::new().unwrap();
    let handle = engine.compile_mir(&module).unwrap();
    let names = handle.function_names();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"fn_a"));
    assert!(names.contains(&"fn_b"));

    let a: i64 = unsafe { handle.call("fn_a", ()).unwrap() };
    let b: i64 = unsafe { handle.call("fn_b", ()).unwrap() };
    assert_eq!(a, 1);
    assert_eq!(b, 2);
}

#[allow(dead_code)]
fn _force_imports_used(_: FileId) {
    // FILE binding is used at module level via SPAN.
    let _ = FILE;
}
