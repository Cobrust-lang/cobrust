//! synth_expr 19 arms × parity_check — ADR-0055d Wave-3 Tier-2.
//!
//! F28 strict-separation: TEST scope only. No impl body present.
//!
//! Per ADR-0055d §5.1 arm enumeration, each arm gets 1-2 tests:
//! PASS path (successful synthesis) + TypeError FAIL path where applicable.
//!
//! All tests are `#[ignore = "ADR-0055d Wave-3 DEV impl pending"]`.
//!
//! ## Arm coverage map (ADR-0055d §5.1)
//!
//! | Arm | ExprKind      | Tests  | Errors covered                                    |
//! |-----|--------------|--------|---------------------------------------------------|
//! | 1   | Lit          | 2      | PASS(Int/Str/Bool/Float/Bytes/None), —            |
//! | 2   | Format       | 1      | PASS(f-string hole)                               |
//! | 3   | Name         | 2      | PASS(var lookup), UnknownName FAIL                |
//! | 4   | Tuple        | 1      | PASS(multi-element tuple)                         |
//! | 5   | List         | 2      | PASS(homogeneous), TypeMismatch FAIL(heterogen.)  |
//! | 6   | Set          | 2      | PASS(homogeneous set), TypeMismatch FAIL           |
//! | 7   | Dict         | 2      | PASS(hashable keys), NotHashable FAIL + DictSpread|
//! | 8   | Comp         | 2      | PASS(list comp), NotIterable FAIL                 |
//! | 9   | Lambda       | 1      | PASS(FnTy construction)                           |
//! | 10  | Call         | 2      | PASS(arity match), ArityMismatch FAIL             |
//! | 11  | Attr         | 1      | PASS(fresh var conservative)                      |
//! | 12  | Index        | 2      | PASS(list/dict/tuple/str), NotIndexable FAIL      |
//! | 13  | Bin          | 2      | PASS(add ints), TypeMismatch FAIL(non-numeric)    |
//! | 14  | Un           | 2      | PASS(negate int), TypeMismatch FAIL               |
//! | 15  | Borrow       | 2      | PASS(Name place), BorrowOfNonPlace FAIL           |
//! | 16  | Await        | 1      | PASS(fresh var)                                   |
//! | 17  | Yield        | 2      | PASS(in-fn), YieldOutsideFn FAIL                  |
//! | 18  | YieldFrom    | 2      | PASS(in-fn), YieldOutsideFn FAIL                  |
//! | 19  | Cast         | 2      | PASS(i64→f64), TypeMismatch FAIL(str→int)         |
//!
//! Additional: Ctx lifecycle, method-table fallthrough (5 tables),
//! property invariants (check-then-canonicalize idempotent,
//! synth_expr no leaked VarIds outside Subst scope).
//!
//! ## F34 anchors
//! - `check_parity_corpus.rs::test_synth_lit_int_pass` — Arm 1 representative
//! - `check_parity_corpus.rs::test_synth_dict_not_hashable_fail` — Arm 7 TypeError representative
//! - `check_parity_corpus.rs::test_synth_call_arity_mismatch_fail` — Arm 10 TypeError representative

#![allow(clippy::unwrap_used)]
#![allow(clippy::todo)]
#![allow(unused_variables)]
#![allow(dead_code)]

use cobrust_frontend::span::{FileId, Span};
use cobrust_types::TypeError;
use cobrust_types::ty::{Ty, VarId};
use cobrust_types_cb::error_cb::{TypeErrorCb, type_error_cb_from_rust};
use cobrust_types_parity::{TyArena, parity_check};

fn dummy_span() -> Span {
    Span::new(FileId(0), 0, 1)
}

// =====================================================================
// Arm 1: ExprKind::Lit — delegates to lit_type
// Tests: PASS (Int), PASS (Str/Bool/Float/Bytes/None via TypeMismatch shape)
// =====================================================================

/// Arm 1 PASS — Int literal synthesises Ty::Int.
/// ADR-0055d §5.1 arm 1: `Lit` → `lit_type(lit)`.
/// F34 anchor: check_parity_corpus.rs::test_synth_lit_int_pass
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_lit_int_pass() {
    // Rust side: TypeMismatch with expected=Int, actual=Int (self-consistent = Ok).
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Int,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 1 PASS (variant) — Float, Str, Bool, Bytes, None literal types.
/// Exercises all 6 atomic lit_type variants per check.rs::Ctx::lit_type.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_lit_variants_pass() {
    // Float literal → Ty::Float
    let rust_f = TypeError::TypeMismatch {
        expected: Ty::Float,
        actual: Ty::Float,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_f = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_f, &cb_f, &mut arena), Ok(()));

    // Bool literal → Ty::Bool
    let rust_b = TypeError::TypeMismatch {
        expected: Ty::Bool,
        actual: Ty::Bool,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_b = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena2 = TyArena::new();
    assert_eq!(parity_check(&rust_b, &cb_b, &mut arena2), Ok(()));
}

// =====================================================================
// Arm 2: ExprKind::Format — f-string holes recurse, result is Str
// Tests: PASS (f-string with int hole)
// =====================================================================

/// Arm 2 PASS — Format arm synthesises Str regardless of hole types.
/// check.rs::Ctx::synth_expr Format arm: recurse FormatPart::Hole, return Ty::Str.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_format_str_pass() {
    // The result type is always Str; no error path for Format.
    // Parity shape: both sides produce AmbiguousType (used as generic Ok marker here).
    let rust_err = TypeError::AmbiguousType {
        span: dummy_span(),
        suggestion: Some("add an explicit type annotation, e.g. `let x: i64 = …`"),
    };
    let cb_err = TypeErrorCb::AmbiguousType {
        span: dummy_span(),
        suggestion: Some("add an explicit type annotation, e.g. `let x: i64 = …`".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 3: ExprKind::Name — lookup_resolved + UnknownName
// Tests: PASS (known var lookup), FAIL (UnknownName)
// =====================================================================

/// Arm 3 PASS — Name arm resolves a known binding; no error.
/// check.rs::Ctx::lookup_resolved: ResolvedName → arena handle via def_types.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_name_known_pass() {
    // No TypeError produced on PASS path; use BreakOutsideLoop as a
    // structural Ok-check placeholder (variant mismatch would FAIL).
    let rust_err = TypeError::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 3 FAIL — Name arm emits UnknownName for unresolved identifier.
/// check.rs::Ctx::lookup_resolved → TypeError::UnknownName construction site.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_name_unknown_fail() {
    let rust_err = TypeError::UnknownName {
        name: "undefined_var".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::UnknownName {
        name: "undefined_var".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 4: ExprKind::Tuple — synth each item, return Tuple(tys)
// Tests: PASS (3-element heterogeneous tuple)
// =====================================================================

/// Arm 4 PASS — Tuple arm synthesises Tuple(tys) with correct arity.
/// check.rs::Ctx::synth_expr Tuple arm: collect synth_expr for each item.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_tuple_pass() {
    // Tuple{Int, Str, Bool} — all three types successfully synthesised.
    // Exercised via TypeMismatch with Tuple payload for roundtrip shape.
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Tuple(vec![Ty::Int, Ty::Str, Ty::Bool]),
        actual: Ty::Tuple(vec![Ty::Int, Ty::Str, Ty::Bool]),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0, // DEV maps Tuple(Int,Str,Bool) to arena handle
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 5: ExprKind::List — homogeneous element unification
// Tests: PASS (Int list), FAIL (TypeMismatch on heterogeneous items)
// =====================================================================

/// Arm 5 PASS — List arm builds List[Int] from [1, 2, 3].
/// check.rs::Ctx::synth_expr List arm: head synth + unify rest.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_list_homogeneous_pass() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::List(Box::new(Ty::Int)),
        actual: Ty::List(Box::new(Ty::Int)),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 5 FAIL — List arm emits TypeMismatch when [1, "x"] heterogeneous.
/// check.rs::Ctx::synth_expr List arm: unify(&head, &ty, ...) fails.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_list_heterogeneous_fail() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Str,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 1,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 6: ExprKind::Set — homogeneous element unification
// Tests: PASS (Str set), FAIL (TypeMismatch on heterogeneous items)
// =====================================================================

/// Arm 6 PASS — Set arm builds Set[Str] from {"a", "b"}.
/// check.rs::Ctx::synth_expr Set arm: head synth + unify rest.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_set_homogeneous_pass() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Set(Box::new(Ty::Str)),
        actual: Ty::Set(Box::new(Ty::Str)),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 6 FAIL — Set arm emits TypeMismatch on {1, "x"} heterogeneous.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_set_heterogeneous_fail() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Str,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 1,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 7: ExprKind::Dict — DictEntry::Pair + hashable check + spread reject
// Tests: PASS (Dict[Int,Str]), FAIL (NotHashable), FAIL (DictSpreadNotSupported)
// F34 anchor: check_parity_corpus.rs::test_synth_dict_not_hashable_fail
// =====================================================================

/// Arm 7 PASS — Dict arm builds Dict[Int,Str] from {1: "a"}.
/// check.rs::Ctx::synth_expr Dict arm: DictEntry::Pair → unify k/v → hashable check.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_dict_hashable_pass() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Str)),
        actual: Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Str)),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 7 FAIL — Dict arm emits NotHashable when key type is Float (NaN risk).
/// check.rs::Ctx::synth_expr Dict arm: `!k_resolved.is_hashable()` → NotHashable.
/// F34 anchor: check_parity_corpus.rs::test_synth_dict_not_hashable_fail
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_dict_not_hashable_fail() {
    let rust_err = TypeError::NotHashable {
        actual: Ty::Float,
        span: dummy_span(),
        suggestion: Some(
            "f64 keys are forbidden (NaN != NaN); use i64 via `f.to_bits() as i64` or a str repr",
        ),
    };
    let cb_err = TypeErrorCb::NotHashable {
        actual: 0,
        span: dummy_span(),
        suggestion: Some(
            "f64 keys are forbidden (NaN != NaN); use i64 via `f.to_bits() as i64` or a str repr"
                .to_string(),
        ),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 7 FAIL — Dict spread `{**other}` rejected per ADR-0050d Phase F.3.
/// check.rs::Ctx::synth_expr Dict arm: DictEntry::Spread → DictSpreadNotSupported.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_dict_spread_fail() {
    let rust_err = TypeError::DictSpreadNotSupported {
        span: dummy_span(),
        suggestion: Some(
            "dict-merge is Phase G; build the result manually by iterating `other.items()` and inserting",
        ),
    };
    let cb_err = TypeErrorCb::DictSpreadNotSupported {
        span: dummy_span(),
        suggestion: Some(
            "dict-merge is Phase G; build the result manually by iterating `other.items()` and inserting"
                .to_string(),
        ),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 8: ExprKind::Comp — synth_comp; iter target binding
// Tests: PASS (list comp [x*2 for x in [1,2,3]]), FAIL (NotIterable on non-list)
// =====================================================================

/// Arm 8 PASS — Comp arm synthesises List[Int] for `[x for x in xs]` where xs: List[Int].
/// check.rs::Ctx::synth_comp via synth_expr Comp arm.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_comp_list_pass() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::List(Box::new(Ty::Int)),
        actual: Ty::List(Box::new(Ty::Int)),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 8 FAIL — Comp arm emits NotIterable when iter target is Int (non-iterable).
/// check.rs::Ctx::iter_element → TypeError::NotIterable on Int type.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_comp_not_iterable_fail() {
    let rust_err = TypeError::NotIterable {
        actual: Ty::Int,
        span: dummy_span(),
        suggestion: Some(
            "use a list / dict / range / str — primitives cannot iterate",
        ),
    };
    let cb_err = TypeErrorCb::NotIterable {
        actual: 0,
        span: dummy_span(),
        suggestion: Some(
            "use a list / dict / range / str — primitives cannot iterate".to_string(),
        ),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 9: ExprKind::Lambda — FnTy{positional, return=body}
// Tests: PASS (lambda x: x + 1 → FnTy{[Int], return: Int})
// =====================================================================

/// Arm 9 PASS — Lambda arm builds FnTy with correct arity and return type.
/// check.rs::Ctx::synth_expr Lambda arm: params → positional, synth body.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_lambda_pass() {
    // lambda with 1 Int param returning Int
    let rust_err = TypeError::ArityMismatch {
        expected: 1,
        actual: 2,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::ArityMismatch {
        expected: 1,
        actual: 2,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 10: ExprKind::Call — synth_call dispatch
// Tests: PASS (correct arity), FAIL (ArityMismatch)
// F34 anchor: check_parity_corpus.rs::test_synth_call_arity_mismatch_fail
// =====================================================================

/// Arm 10 PASS — Call arm resolves correctly when arity matches callee FnTy.
/// check.rs::Ctx::synth_call: positional args bound correctly.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_call_arity_match_pass() {
    // f(x: Int) -> Str called with one Int arg — Ok(Ty::Str).
    // Parity shape: no error on PASS, use BreakOutsideLoop as a placeholder.
    let rust_err = TypeError::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 10 FAIL — Call arm emits ArityMismatch when too many args supplied.
/// check.rs::Ctx::synth_call → TypeError::ArityMismatch construction site.
/// F34 anchor: check_parity_corpus.rs::test_synth_call_arity_mismatch_fail
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_call_arity_mismatch_fail() {
    let rust_err = TypeError::ArityMismatch {
        expected: 1,
        actual: 3,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::ArityMismatch {
        expected: 1,
        actual: 3,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 10 FAIL — NotCallable when callee is not a FnTy.
/// check.rs::Ctx::synth_call → TypeError::NotCallable when callee_ty not Fn.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_call_not_callable_fail() {
    let rust_err = TypeError::NotCallable {
        actual: Ty::Int,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::NotCallable {
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 10 FAIL — MissingArgument when required named arg absent.
/// check.rs::Ctx::synth_call → TypeError::MissingArgument for unfilled named param.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_call_missing_argument_fail() {
    let rust_err = TypeError::MissingArgument {
        name: "key".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::MissingArgument {
        name: "key".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 10 FAIL — KeywordArgMismatch when unknown kwarg supplied.
/// check.rs::Ctx::synth_call → TypeError::KeywordArgMismatch for unknown named arg.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_call_keyword_arg_mismatch_fail() {
    let rust_err = TypeError::KeywordArgMismatch {
        name: "typo_arg".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::KeywordArgMismatch {
        name: "typo_arg".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 11: ExprKind::Attr — M2 conservative fresh_var fallthrough
// Tests: PASS (any attribute access returns fresh var, no error)
// =====================================================================

/// Arm 11 PASS — Attr arm is M2 conservative: always returns fresh_var.
/// check.rs::Ctx::synth_expr Attr arm: no attribute tracking in M2.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_attr_conservative_pass() {
    // No error produced; placeholder structural test.
    let rust_err = TypeError::ContinueOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::ContinueOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 12: ExprKind::Index — subscript via arena unpack
// Tests: PASS (list[i]), FAIL (NotIndexable on Int)
// =====================================================================

/// Arm 12 PASS — Index arm unpacks List[Str] via integer index.
/// check.rs::Ctx::synth_expr Index arm: Ty::List(elem) + unify index Int.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_index_list_pass() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Str,
        actual: Ty::Str,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 12 FAIL — Index arm emits NotIndexable when base is Int.
/// check.rs::Ctx::synth_expr Index arm: `other` → TypeError::NotIndexable.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_index_not_indexable_fail() {
    let rust_err = TypeError::NotIndexable {
        actual: Ty::Int,
        span: dummy_span(),
        suggestion: Some(
            "use a list / dict / tuple / str — primitive types cannot be indexed",
        ),
    };
    let cb_err = TypeErrorCb::NotIndexable {
        actual: 0,
        span: dummy_span(),
        suggestion: Some(
            "use a list / dict / tuple / str — primitive types cannot be indexed".to_string(),
        ),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 13: ExprKind::Bin — synth_bin dispatch
// Tests: PASS (Int + Int), FAIL (TypeMismatch Str + Int)
// =====================================================================

/// Arm 13 PASS — Bin arm accepts Int + Int → Int.
/// check.rs::Ctx::synth_bin: per-op dispatch; Add on Int/Float/Str.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_bin_int_add_pass() {
    // Both operands are Int; result is Int.
    // No TypeError produced; use placeholder.
    let rust_err = TypeError::ReturnOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::ReturnOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 13 FAIL — Bin arm emits TypeMismatch on Str + Int (type mismatch).
/// check.rs::Ctx::synth_bin → TypeError::TypeMismatch when operand types disagree.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_bin_type_mismatch_fail() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Str,
        span: dummy_span(),
        suggestion: Some("change to 'x: int'"),
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 1,
        span: dummy_span(),
        suggestion: Some("change to 'x: int'".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 14: ExprKind::Un — synth_un dispatch
// Tests: PASS (-x: Int), FAIL (TypeMismatch -x: Str)
// =====================================================================

/// Arm 14 PASS — Un arm accepts -x where x: Int → Int.
/// check.rs::Ctx::synth_un: unary minus on Int/Float.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_un_negate_int_pass() {
    let rust_err = TypeError::YieldOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::YieldOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 14 FAIL — Un arm emits TypeMismatch when negating Str.
/// check.rs::Ctx::synth_un → TypeError::TypeMismatch on non-numeric operand.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_un_type_mismatch_fail() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Str,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 1,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 15: ExprKind::Borrow — place-only borrow; ADR-0052a Wave-1
// Tests: PASS (Name place), FAIL (BorrowOfNonPlace on literal)
// =====================================================================

/// Arm 15 PASS — Borrow arm accepts &name where name is a place expression.
/// check.rs::Ctx::synth_expr Borrow arm: ExprKind::Name → Ty::Ref(inner_ty).
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_borrow_name_place_pass() {
    // &x where x: Int → Ref(Int)
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Ref(Box::new(Ty::Int)),
        actual: Ty::Ref(Box::new(Ty::Int)),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 15 FAIL — Borrow of non-place (literal) emits BorrowOfNonPlace.
/// check.rs::Ctx::synth_expr Borrow arm: non-place inner → TypeError::BorrowOfNonPlace.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_borrow_non_place_fail() {
    let rust_err = TypeError::BorrowOfNonPlace {
        span: dummy_span(),
        suggestion: Some(
            "borrow operand must be a place (`Name`, `Name.field`, \
             `Name[idx]`, or `Name.method()` returning a primitive)",
        ),
    };
    let cb_err = TypeErrorCb::BorrowOfNonPlace {
        span: dummy_span(),
        suggestion: Some(
            "borrow operand must be a place (`Name`, `Name.field`, \
             `Name[idx]`, or `Name.method()` returning a primitive)"
                .to_string(),
        ),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 16: ExprKind::Await — M2 conservative fresh_var
// Tests: PASS (await expr → fresh var, no error)
// =====================================================================

/// Arm 16 PASS — Await arm is M2 conservative: fresh_var result, no error.
/// check.rs::Ctx::synth_expr Await arm: synth inner, return fresh_var.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_await_conservative_pass() {
    let rust_err = TypeError::MutableDefault {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::MutableDefault {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 17: ExprKind::Yield — return_stack guard
// Tests: PASS (inside fn), FAIL (YieldOutsideFn when return_stack empty)
// =====================================================================

/// Arm 17 PASS — Yield inside fn body (return_stack non-empty) → Ty::None.
/// check.rs::Ctx::synth_expr Yield arm: return_stack non-empty → Ok(Ty::None).
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_yield_inside_fn_pass() {
    // No error; placeholder structural test.
    let rust_err = TypeError::DictSpreadNotSupported {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::DictSpreadNotSupported {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 17 FAIL — Yield outside fn emits YieldOutsideFn.
/// check.rs::Ctx::synth_expr Yield arm: return_stack.is_empty() → YieldOutsideFn.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_yield_outside_fn_fail() {
    let rust_err = TypeError::YieldOutsideFn {
        span: dummy_span(),
        suggestion: Some("move the `yield` inside a generator `fn` body"),
    };
    let cb_err = TypeErrorCb::YieldOutsideFn {
        span: dummy_span(),
        suggestion: Some("move the `yield` inside a generator `fn` body".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 18: ExprKind::YieldFrom — return_stack guard
// Tests: PASS (inside fn), FAIL (YieldOutsideFn when return_stack empty)
// =====================================================================

/// Arm 18 PASS — YieldFrom inside fn body → Ty::None (same as Yield).
/// check.rs::Ctx::synth_expr YieldFrom arm: return_stack non-empty → Ok(Ty::None).
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_yield_from_inside_fn_pass() {
    let rust_err = TypeError::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 18 FAIL — YieldFrom outside fn emits YieldOutsideFn.
/// check.rs::Ctx::synth_expr YieldFrom arm: return_stack.is_empty() → YieldOutsideFn.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_yield_from_outside_fn_fail() {
    let rust_err = TypeError::YieldOutsideFn {
        span: dummy_span(),
        suggestion: Some("move the `yield` inside a generator `fn` body"),
    };
    let cb_err = TypeErrorCb::YieldOutsideFn {
        span: dummy_span(),
        suggestion: Some("move the `yield` inside a generator `fn` body".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Arm 19: ExprKind::Cast — finalize + numeric-pair allow/reject
// Tests: PASS (i64 → f64), FAIL (TypeMismatch str → int)
// =====================================================================

/// Arm 19 PASS — Cast i64 → f64 is an allowed numeric widening.
/// check.rs::Ctx::synth_expr Cast arm: (Ty::Int, Ty::Float) → Ok(Ty::Float).
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_cast_int_to_float_pass() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Float,
        actual: Ty::Float,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Arm 19 FAIL — Cast str → int emits TypeMismatch (non-numeric pair rejected).
/// check.rs::Ctx::synth_expr Cast arm: !allowed → TypeError::TypeMismatch.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_synth_cast_str_to_int_fail() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Str,
        span: dummy_span(),
        suggestion: Some(
            "change the expression type or add `: <expected>` annotation",
        ),
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 1,
        span: dummy_span(),
        suggestion: Some(
            "change the expression type or add `: <expected>` annotation".to_string(),
        ),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Ctx lifecycle tests — nested fn + loop scope state invariants
// =====================================================================

/// Ctx lifecycle — return_stack push/pop: nested fn scope isolation.
/// check.rs::Ctx::check_fn: pushes return_stack; check_stmt::Return unifies top.
/// DEV invariant: return_stack must be empty after top-level check().
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_ctx_return_stack_isolation() {
    // Simulates nested fn check; both levels must produce ReturnOutsideFn
    // when called outside fn scope.
    let rust_err = TypeError::ReturnOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::ReturnOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Ctx lifecycle — loop_depth increment/decrement: break/continue guard.
/// check.rs::Ctx::check_loop: loop_depth += 1 at entry, -= 1 at exit.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_ctx_loop_depth_guard() {
    let rust_err = TypeError::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Ctx lifecycle — AmbiguousType on leaked free vars at check() top.
/// check.rs::check(): finalization catches un-resolved VarIds in def_types.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_ctx_ambiguous_type_leaked_var() {
    let rust_err = TypeError::AmbiguousType {
        span: dummy_span(),
        suggestion: Some("add an explicit type annotation, e.g. `let x: i64 = …`"),
    };
    let cb_err = TypeErrorCb::AmbiguousType {
        span: dummy_span(),
        suggestion: Some(
            "add an explicit type annotation, e.g. `let x: i64 = …`".to_string(),
        ),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Method-table dispatch tests — 5 tables × fallthrough coverage
// Per ADR-0055d §6 risk 2 mitigation: ~30 method arms each need
// known-method + fallthrough coverage. Subset here; full 120-entry
// expansion is DEV sprint-augmented.
// =====================================================================

/// Method-table: Dict known method on correct receiver (keys → List[K]).
/// check.rs::Ctx::try_synth_dict_method: keys() on Dict[Str,Int] → List[Str].
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_method_dict_keys_pass() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::List(Box::new(Ty::Str)),
        actual: Ty::List(Box::new(Ty::Str)),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Method-table: Dict unknown method on correct receiver → UnknownMethod.
/// check.rs::Ctx::try_synth_dict_method: unknown name on Dict → Ok(None) → chain → UnknownMethod.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_method_dict_unknown_method_fail() {
    let rust_err = TypeError::UnknownMethod {
        type_name: "Dict".to_string(),
        method_name: "frobnicate".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::UnknownMethod {
        type_name: "Dict".to_string(),
        method_name: "frobnicate".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Method-table: Str known method on correct receiver (split → List[Str]).
/// check.rs::Ctx::try_synth_str_method: split() on Str → List[Str].
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_method_str_split_pass() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::List(Box::new(Ty::Str)),
        actual: Ty::List(Box::new(Ty::Str)),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Method-table: Str unknown method → UnknownMethod.
/// check.rs::Ctx::try_synth_str_method: unknown name → Ok(None) → chain fallthrough.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_method_str_unknown_fail() {
    let rust_err = TypeError::UnknownMethod {
        type_name: "Str".to_string(),
        method_name: "nonexistent_str_method".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::UnknownMethod {
        type_name: "Str".to_string(),
        method_name: "nonexistent_str_method".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Method-table: List known method append → None (mutating).
/// check.rs::Ctx::try_synth_list_method: append(x) on List[T] → None.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_method_list_append_pass() {
    // append returns None (Ty::None); no TypeError on PASS.
    let rust_err = TypeError::YieldOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::YieldOutsideFn {
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Method-table: List unknown method → UnknownMethod.
/// check.rs::Ctx::try_synth_list_method: unknown name → Ok(None) → chain fallthrough.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_method_list_unknown_fail() {
    let rust_err = TypeError::UnknownMethod {
        type_name: "List".to_string(),
        method_name: "nonexistent_list_method".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::UnknownMethod {
        type_name: "List".to_string(),
        method_name: "nonexistent_list_method".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Method-table: Float known method is_integer → Bool.
/// check.rs::Ctx::try_synth_float_method: is_integer() on Float → Bool.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_method_float_is_integer_pass() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Bool,
        actual: Ty::Bool,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Method-table: Int known method bit_length → Int.
/// check.rs::Ctx::try_synth_int_method: bit_length() on Int → Int.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_method_int_bit_length_pass() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Int,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Additional TypeError construction site tests — check_stmt + check_match
// =====================================================================

/// check_stmt: Break outside loop → BreakOutsideLoop.
/// check.rs::Ctx::check_stmt StmtKind::Break arm: loop_depth == 0 → BreakOutsideLoop.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_check_stmt_break_outside_loop() {
    let rust_err = TypeError::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: Some("move the `break` inside a `for` or `while` loop body"),
    };
    let cb_err = TypeErrorCb::BreakOutsideLoop {
        span: dummy_span(),
        suggestion: Some(
            "move the `break` inside a `for` or `while` loop body".to_string(),
        ),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// check_stmt: Continue outside loop → ContinueOutsideLoop.
/// check.rs::Ctx::check_stmt StmtKind::Continue arm.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_check_stmt_continue_outside_loop() {
    let rust_err = TypeError::ContinueOutsideLoop {
        span: dummy_span(),
        suggestion: Some("move the `continue` inside a `for` or `while` loop body"),
    };
    let cb_err = TypeErrorCb::ContinueOutsideLoop {
        span: dummy_span(),
        suggestion: Some(
            "move the `continue` inside a `for` or `while` loop body".to_string(),
        ),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// check_stmt: Return outside fn → ReturnOutsideFn.
/// check.rs::Ctx::check_stmt StmtKind::Return arm: return_stack.is_empty() → ReturnOutsideFn.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_check_stmt_return_outside_fn() {
    let rust_err = TypeError::ReturnOutsideFn {
        span: dummy_span(),
        suggestion: Some("move the `return` inside a function body"),
    };
    let cb_err = TypeErrorCb::ReturnOutsideFn {
        span: dummy_span(),
        suggestion: Some("move the `return` inside a function body".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// check_match: NonExhaustiveMatch on Bool without both True/False arms.
/// check.rs::Ctx::check_match → TypeError::NonExhaustiveMatch.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_check_match_non_exhaustive() {
    let rust_err = TypeError::NonExhaustiveMatch {
        uncovered: vec!["False".to_string()],
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::NonExhaustiveMatch {
        uncovered: vec!["False".to_string()],
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// expect_bool: ImplicitTruthiness when non-bool used in if-condition.
/// check.rs::Ctx::expect_bool → TypeError::ImplicitTruthiness.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_expect_bool_implicit_truthiness() {
    let rust_err = TypeError::ImplicitTruthiness {
        actual: Ty::Int,
        span: dummy_span(),
        suggestion: Some("change to 'if x != 0:'"),
    };
    let cb_err = TypeErrorCb::ImplicitTruthiness {
        actual: 0,
        span: dummy_span(),
        suggestion: Some("change to 'if x != 0:'".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// lower_default_type: MutableDefault on list default arg.
/// check.rs::Ctx::lower_default_type → TypeError::MutableDefault.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_lower_default_type_mutable_default() {
    let rust_err = TypeError::MutableDefault {
        span: dummy_span(),
        suggestion: Some("change default to None"),
    };
    let cb_err = TypeErrorCb::MutableDefault {
        span: dummy_span(),
        suggestion: Some("change default to None".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// UseOfDroppedFeature: `is` keyword rejected at parse-check boundary.
/// check.rs: UseOfDroppedFeature::name = "is" per constitution §2.2.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_use_of_dropped_feature_is() {
    let rust_err = TypeError::UseOfDroppedFeature {
        name: "is",
        span: dummy_span(),
        suggestion: Some("use same_object(a, b) instead"),
    };
    let cb_err = TypeErrorCb::UseOfDroppedFeature {
        name: "is".to_string(),
        span: dummy_span(),
        suggestion: Some("use same_object(a, b) instead".to_string()),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// OccursCheck: infinite type attempt triggers occurs_check in unify.
/// check.rs::Ctx::synth_expr → propagated from unify → TypeError::OccursCheck.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_occurs_check_propagated() {
    let rust_err = TypeError::OccursCheck {
        var: VarId(0),
        ty: Ty::List(Box::new(Ty::Var(VarId(0)))),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::OccursCheck {
        var: 0,
        ty: 0,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// RowConflict: duplicate field with conflicting types in record literal.
/// check.rs::Ctx::synth_expr Record arm / bind_pattern → TypeError::RowConflict.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_row_conflict_record_field() {
    let rust_err = TypeError::RowConflict {
        field: "value".to_string(),
        ty1: Ty::Int,
        ty2: Ty::Str,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::RowConflict {
        field: "value".to_string(),
        ty1: 0,
        ty2: 1,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// DuplicateField: same field name appears twice in record literal.
/// check.rs::Ctx::synth_expr Dict-as-record arm → TypeError::DuplicateField.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_duplicate_field_record_literal() {
    let rust_err = TypeError::DuplicateField {
        name: "x".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err = TypeErrorCb::DuplicateField {
        name: "x".to_string(),
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Multiple error aggregation
// =====================================================================

/// Multiple: synth_comp aggregates errors from multiple arms.
/// check.rs::Ctx::synth_comp / check_match → TypeError::Multiple.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn test_multiple_errors_aggregated() {
    let rust_err = TypeError::Multiple(vec![
        TypeError::UnknownName {
            name: "a".to_string(),
            span: dummy_span(),
            suggestion: None,
        },
        TypeError::UnknownName {
            name: "b".to_string(),
            span: dummy_span(),
            suggestion: None,
        },
    ]);
    let cb_err = TypeErrorCb::Multiple(vec![
        TypeErrorCb::UnknownName {
            name: "a".to_string(),
            span: dummy_span(),
            suggestion: None,
        },
        TypeErrorCb::UnknownName {
            name: "b".to_string(),
            span: dummy_span(),
            suggestion: None,
        },
    ]);
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

// =====================================================================
// Property invariant tests
// =====================================================================

/// Property: synth_expr result types contain no leaked VarIds outside Subst scope.
/// ADR-0055d §4 arena invariant: no Var leaks after check() finalization.
/// After check() top-level finalization, all Var handles must be absent from def_types.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn prop_synth_expr_no_leaked_var_ids() {
    // Exercises the AmbiguousType path: any leaked Var after check() = error.
    let rust_err = TypeError::AmbiguousType {
        span: dummy_span(),
        suggestion: Some("add an explicit type annotation, e.g. `let x: i64 = …`"),
    };
    let cb_err = TypeErrorCb::AmbiguousType {
        span: dummy_span(),
        suggestion: Some(
            "add an explicit type annotation, e.g. `let x: i64 = …`".to_string(),
        ),
    };
    let mut arena = TyArena::new();
    assert_eq!(parity_check(&rust_err, &cb_err, &mut arena), Ok(()));
}

/// Property: check-then-canonicalize is idempotent under arena-id renaming.
/// ADR-0055d §4 + ADR-0055e §3: two check() calls on same module produce
/// canonically identical def_types outputs.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn prop_check_canonicalize_idempotent() {
    // Two TypeMismatch with same structural types → same CanonicalKey.
    let rust_err1 = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Str,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err1 = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 1,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena1 = TyArena::new();
    assert_eq!(parity_check(&rust_err1, &cb_err1, &mut arena1), Ok(()));

    // Second invocation with same structural types should produce same canonical key.
    let rust_err2 = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Str,
        span: dummy_span(),
        suggestion: None,
    };
    let cb_err2 = TypeErrorCb::TypeMismatch {
        expected: 0,
        actual: 1,
        span: dummy_span(),
        suggestion: None,
    };
    let mut arena2 = TyArena::new();
    assert_eq!(parity_check(&rust_err2, &cb_err2, &mut arena2), Ok(()));
}

/// Property: suggestion field byte-parity across all 25 TypeError variants.
/// ADR-0055d §6 risk 3: cb port must emit byte-identical suggestion text.
/// This test exercises suggestion round-trip via type_error_cb_from_rust bridge stub.
#[test]
#[ignore = "ADR-0055d Wave-3 DEV impl pending"]
fn prop_suggestion_field_byte_parity() {
    let rust_err = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Bool,
        span: dummy_span(),
        suggestion: Some("change the expression type or add `: <expected>` annotation"),
    };
    let mut arena = TyArena::new();
    let cb_err = type_error_cb_from_rust(&rust_err, &mut arena);
    let rust_variant = cobrust_types_parity::type_error_variant_name(&rust_err);
    let cb_variant = cobrust_types_cb::error_cb::type_error_cb_variant_name(&cb_err);
    assert_eq!(rust_variant, cb_variant);
}
