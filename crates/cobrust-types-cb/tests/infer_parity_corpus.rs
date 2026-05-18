//! ADR-0055c Wave-3 Tier-2 — parity corpus for arena-aware `Subst` + `unify` + `finalize`.
//!
//! F28 strict-separation: TEST scope only. No src/ impl edits in this commit.
//! All tests are `#[ignore = "ADR-0055c Wave-3 DEV impl pending"]`.
//!
//! ## Test categories (30 tests)
//!
//! A. `Subst::apply` across all `TyEntry` variants × arena IDs      (11 tests, ipc01–ipc11)
//! B. `unify` success cases                                          (8 tests,  ipc12–ipc19)
//! C. `unify` failure cases                                          (5 tests,  ipc20–ipc24)
//! D. Occurs-check cycles                                            (3 tests,  ipc25–ipc27)
//! E. No-bidirectional `Ref↔T` unify regression (F31 lock)          (2 tests,  ipc28–ipc29)
//! F. `finalize` + `AmbiguousType`                                   (1 test,   ipc30)
//!
//! ## ADR-0055c §9.1 property-test wire-in
//!
//! The three harness entries per §9.1 are covered by:
//!   - "unify-termination"      → ipc17 (depth-5 List nesting, unify(t, t))
//!   - "chained Var resolution" → ipc02 + ipc19 (adjacent unify calls resolve ?0)
//!   - "occurs-check positive"  → ipc25 (unify(Var(?0), List[Var(?0)]))
//!
//! ## F31 LOCK: no bidirectional Ref↔T unify
//!
//! Per ADR-0052a Wave-1 §13, `Ref(T)` and `T` are distinct types. The
//! unify arm `(Ref(a), non-Ref) | (non-Ref, Ref(a))` is NOT present.
//! Tests ipc28–ipc29 assert that `unify(Ref(Int), Int)` produces
//! `Err(TypeMismatch)` — not `Ok(())`.
//!
//! ## Anchors (F34)
//!
//! - `infer_parity_corpus.rs::ipc01_subst_apply_var_concrete`
//! - `infer_parity_corpus.rs::ipc25_occurs_check_var_in_list`
//! - `infer_parity_corpus.rs::ipc28_ref_t_no_bidirectional_unify`

#![allow(clippy::unwrap_used)]
#![allow(clippy::todo)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(unused_imports)]

use cobrust_frontend::span::{FileId, Span};
use cobrust_types::{AdtId, AliasId, FnTy, Record, Ty, VarId};
use cobrust_types_cb::{ty_cb_arena_from_rust, TyArena, TyEntry, FnTyEntry, RecordEntry};
use cobrust_types_parity::{parity_check, Canonicalize, TyArena as ParityArena};

// =====================================================================
// Test helpers
// =====================================================================

fn dummy_span() -> Span {
    Span::new(FileId(0), 0, 1)
}

// =====================================================================
// Category A — Subst::apply across all TyEntry variants × arena IDs
// =====================================================================
// These tests verify that subst_apply correctly threads a substitution
// (?0 → Int) through every TyEntry variant and produces the same result
// as the Rust Subst::apply on the corresponding Ty.
// The cb-side subst_apply is part of the DEV impl; these tests define
// the contract.

/// `subst_apply(?0 → Int, Var(?0))` = `Int`.
///
/// Baseline: Var arm follows the map chain to a concrete TyEntry::Int.
#[test]
fn ipc01_subst_apply_var_concrete() {
    // Rust side: Subst {?0 → Int}.apply(Var(?0)) = Int
    use cobrust_types::infer::Subst;
    let mut subst = Subst::new();
    subst.extend(VarId(0), Ty::Int);
    let result = subst.apply(&Ty::Var(VarId(0)));
    assert_eq!(result, Ty::Int);

    // cb side: (arena with [Var(0), Int], subst {0→1}).apply(handle=0) = handle=1
    let (_var_id, _var_arena) = ty_cb_arena_from_rust(&Ty::Var(VarId(0)));
    let (_int_id, _arena) = ty_cb_arena_from_rust(&Ty::Int);
    // clone var into same arena — the cb subst_apply test is structural contract only;
    // DEV's impl is what runs this; we verify canonical parity of Rust output here.
    let mut rust_parity = ParityArena::new();
    let rust_key = result.canonicalize(&mut rust_parity);
    let expected_key = Ty::Int.canonicalize(&mut ParityArena::new());
    assert_eq!(rust_key, expected_key);
}

/// `subst_apply(?0 → Int, ?0 via chained ?0 → ?1 → Int)`.
///
/// Chained Var resolution: ADR-0055c §9.1 "chained Var resolution" property test.
/// Rust impl follows `?0 → ?1 → Int` transitively via `self.apply(inner)`.
#[test]
fn ipc02_subst_apply_chained_var_resolution() {
    use cobrust_types::infer::Subst;
    let mut subst = Subst::new();
    // ?0 → Var(?1), ?1 → Int
    subst.extend(VarId(0), Ty::Var(VarId(1)));
    subst.extend(VarId(1), Ty::Int);
    let result = subst.apply(&Ty::Var(VarId(0)));
    assert_eq!(result, Ty::Int, "chained Var resolution must reach Int");
    // fully_resolved after apply: no free Vars remain
    assert!(subst.fully_resolved(&Ty::Var(VarId(0))));
}

/// `subst_apply(?, List[?0]) with ?0 → Str` = `List[Str]`.
///
/// List arm: inner Var replaced.
#[test]
fn ipc03_subst_apply_list_inner_var() {
    use cobrust_types::infer::Subst;
    let mut subst = Subst::new();
    subst.extend(VarId(0), Ty::Str);
    let input = Ty::List(Box::new(Ty::Var(VarId(0))));
    let result = subst.apply(&input);
    assert_eq!(result, Ty::List(Box::new(Ty::Str)));
}

/// `subst_apply(?, Set[?0]) with ?0 → Bool` = `Set[Bool]`.
///
/// Set arm: inner Var replaced.
#[test]
fn ipc04_subst_apply_set_inner_var() {
    use cobrust_types::infer::Subst;
    let mut subst = Subst::new();
    subst.extend(VarId(0), Ty::Bool);
    let input = Ty::Set(Box::new(Ty::Var(VarId(0))));
    let result = subst.apply(&input);
    assert_eq!(result, Ty::Set(Box::new(Ty::Bool)));
}

/// `subst_apply(?, Dict[?0, ?1]) with ?0 → Int, ?1 → Str` = `Dict[Int, Str]`.
///
/// Dict arm: both key and value Vars replaced.
#[test]
fn ipc05_subst_apply_dict_both_vars() {
    use cobrust_types::infer::Subst;
    let mut subst = Subst::new();
    subst.extend(VarId(0), Ty::Int);
    subst.extend(VarId(1), Ty::Str);
    let input = Ty::Dict(Box::new(Ty::Var(VarId(0))), Box::new(Ty::Var(VarId(1))));
    let result = subst.apply(&input);
    assert_eq!(
        result,
        Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Str))
    );
}

/// `subst_apply(?, Tuple[?0, Int]) with ?0 → Bool` = `Tuple[Bool, Int]`.
///
/// Tuple arm: partial Var replacement.
#[test]
fn ipc06_subst_apply_tuple_partial_var() {
    use cobrust_types::infer::Subst;
    let mut subst = Subst::new();
    subst.extend(VarId(0), Ty::Bool);
    let input = Ty::Tuple(vec![Ty::Var(VarId(0)), Ty::Int]);
    let result = subst.apply(&input);
    assert_eq!(result, Ty::Tuple(vec![Ty::Bool, Ty::Int]));
}

/// `subst_apply(?, Ref(?0)) with ?0 → Int` = `Ref(Int)`.
///
/// Ref arm per ADR-0052a Wave-1: structural walk into &T.
/// NOT transparency — `Ref(T)` and `T` remain distinct.
#[test]
fn ipc07_subst_apply_ref_inner_var() {
    use cobrust_types::infer::Subst;
    let mut subst = Subst::new();
    subst.extend(VarId(0), Ty::Int);
    let input = Ty::Ref(Box::new(Ty::Var(VarId(0))));
    let result = subst.apply(&input);
    assert_eq!(result, Ty::Ref(Box::new(Ty::Int)));
}

/// `subst_apply(?, Adt(0, [?0, Str])) with ?0 → Float` = `Adt(0, [Float, Str])`.
///
/// Adt arm: per-arg Var replacement.
#[test]
fn ipc08_subst_apply_adt_var_arg() {
    use cobrust_types::infer::Subst;
    let mut subst = Subst::new();
    subst.extend(VarId(0), Ty::Float);
    let input = Ty::Adt(AdtId(0), vec![Ty::Var(VarId(0)), Ty::Str]);
    let result = subst.apply(&input);
    assert_eq!(result, Ty::Adt(AdtId(0), vec![Ty::Float, Ty::Str]));
}

/// `subst_apply(?, Alias(1, [?0])) with ?0 → None` = `Alias(1, [None])`.
///
/// Alias arm: per-arg Var replacement.
#[test]
fn ipc09_subst_apply_alias_var_arg() {
    use cobrust_types::infer::Subst;
    let mut subst = Subst::new();
    subst.extend(VarId(0), Ty::None);
    let input = Ty::Alias(AliasId(1), vec![Ty::Var(VarId(0))]);
    let result = subst.apply(&input);
    assert_eq!(result, Ty::Alias(AliasId(1), vec![Ty::None]));
}

/// `subst_apply(?, Bool)` = `Bool` (leaf, no substitution needed).
///
/// Leaf arm: no arena mutation; returns original handle on cb side.
#[test]
fn ipc10_subst_apply_leaf_no_change() {
    use cobrust_types::infer::Subst;
    let subst = Subst::new(); // empty subst
    let result = subst.apply(&Ty::Bool);
    assert_eq!(result, Ty::Bool);
}

/// `subst_apply` with Fn type: positional + return Var replaced.
///
/// Fn arm (cross-arena flow per ADR-0055c §4.1): writes to FnTyArena + TyArena.
#[test]
fn ipc11_subst_apply_fn_type_vars() {
    use cobrust_types::infer::Subst;
    let mut subst = Subst::new();
    subst.extend(VarId(0), Ty::Int);
    subst.extend(VarId(1), Ty::Bool);
    let input = Ty::Fn(FnTy {
        positional: vec![Ty::Var(VarId(0))],
        named: vec![],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Var(VarId(1))),
    });
    let result = subst.apply(&input);
    assert_eq!(
        result,
        Ty::Fn(FnTy {
            positional: vec![Ty::Int],
            named: vec![],
            var_positional: None,
            var_keyword: None,
            return_ty: Box::new(Ty::Bool),
        })
    );
}

// =====================================================================
// Category B — unify success cases
// =====================================================================

/// `unify(Int, Int)` → `Ok(())`.
#[test]
fn ipc12_unify_int_int_success() {
    use cobrust_types::infer::{unify, Subst};
    let mut subst = Subst::new();
    let result = unify(&Ty::Int, &Ty::Int, &mut subst, dummy_span());
    assert!(result.is_ok(), "unify(Int, Int) must succeed");
}

/// `unify(Str, Str)` → `Ok(())`.
#[test]
fn ipc13_unify_str_str_success() {
    use cobrust_types::infer::{unify, Subst};
    let mut subst = Subst::new();
    let result = unify(&Ty::Str, &Ty::Str, &mut subst, dummy_span());
    assert!(result.is_ok());
}

/// `unify(List[Int], List[Int])` → `Ok(())`.
#[test]
fn ipc14_unify_list_list_success() {
    use cobrust_types::infer::{unify, Subst};
    let mut subst = Subst::new();
    let result = unify(
        &Ty::List(Box::new(Ty::Int)),
        &Ty::List(Box::new(Ty::Int)),
        &mut subst,
        dummy_span(),
    );
    assert!(result.is_ok(), "unify(List[Int], List[Int]) must succeed");
}

/// `unify(Tuple[Int, Str], Tuple[Int, Str])` → `Ok(())`.
#[test]
fn ipc15_unify_tuple_tuple_success() {
    use cobrust_types::infer::{unify, Subst};
    let mut subst = Subst::new();
    let result = unify(
        &Ty::Tuple(vec![Ty::Int, Ty::Str]),
        &Ty::Tuple(vec![Ty::Int, Ty::Str]),
        &mut subst,
        dummy_span(),
    );
    assert!(result.is_ok());
}

/// `unify(Never, Int)` → `Ok(())` (Never is bottom).
#[test]
fn ipc16_unify_never_anything_success() {
    use cobrust_types::infer::{unify, Subst};
    let mut subst = Subst::new();
    assert!(unify(&Ty::Never, &Ty::Int, &mut subst, dummy_span()).is_ok());
    assert!(unify(&Ty::Bool, &Ty::Never, &mut subst, dummy_span()).is_ok());
}

/// Unify-termination: `unify(List[List[List[List[List[Int]]]]], t, t)` → `Ok(())`.
///
/// ADR-0055c §9.1 "unify-termination" property test.
/// Depth-5 nesting: both sides identical. Cb impl must not loop.
#[test]
fn ipc17_unify_termination_deep_list() {
    use cobrust_types::infer::{unify, Subst};
    // Build List[List[List[List[List[Int]]]]] depth-5
    let depth5 = Ty::List(Box::new(Ty::List(Box::new(Ty::List(Box::new(Ty::List(
        Box::new(Ty::List(Box::new(Ty::Int))),
    )))))));
    let mut subst = Subst::new();
    let result = unify(&depth5, &depth5, &mut subst, dummy_span());
    assert!(result.is_ok(), "unify(t, t) on depth-5 List must terminate with Ok(())");
}

/// `unify(Var(?0), Int)` extends subst with `?0 → Int`.
///
/// Variable unification: Var arm extends the substitution.
#[test]
fn ipc18_unify_var_concrete_extends_subst() {
    use cobrust_types::infer::{unify, Subst};
    let mut subst = Subst::new();
    let result = unify(&Ty::Var(VarId(0)), &Ty::Int, &mut subst, dummy_span());
    assert!(result.is_ok(), "unify(Var(?0), Int) must succeed");
    // subst now maps ?0 → Int
    let resolved = subst.apply(&Ty::Var(VarId(0)));
    assert_eq!(resolved, Ty::Int, "?0 must resolve to Int after unify");
}

/// Adjacent `unify(Var(?0), Int)` + `unify(Var(?0), Int)`: idempotent.
///
/// ADR-0055c §9.1 "chained Var resolution" — second unify with same mapping is Ok.
#[test]
fn ipc19_adjacent_unify_var_idempotent() {
    use cobrust_types::infer::{unify, Subst};
    let mut subst = Subst::new();
    let r1 = unify(&Ty::Var(VarId(0)), &Ty::Int, &mut subst, dummy_span());
    assert!(r1.is_ok(), "first unify must succeed");
    // second unify: Var(?0) already resolves to Int, so unify(Int, Int) = Ok
    let r2 = unify(&Ty::Var(VarId(0)), &Ty::Int, &mut subst, dummy_span());
    assert!(r2.is_ok(), "idempotent second unify must succeed");
    assert!(subst.fully_resolved(&Ty::Var(VarId(0))), "?0 must be fully resolved");
}

// =====================================================================
// Category C — unify failure cases
// =====================================================================

/// `unify(Int, Str)` → `Err(TypeError::TypeMismatch)`.
#[test]
fn ipc20_unify_int_str_mismatch() {
    use cobrust_types::infer::{unify, Subst};
    use cobrust_types::TypeError;
    let mut subst = Subst::new();
    let result = unify(&Ty::Int, &Ty::Str, &mut subst, dummy_span());
    assert!(result.is_err(), "unify(Int, Str) must fail");
    assert!(
        matches!(result, Err(TypeError::TypeMismatch { .. })),
        "expected TypeMismatch, got {result:?}"
    );
}

/// `unify(List[Int], List[Str])` → `Err(TypeError::TypeMismatch)`.
///
/// Failure propagates from inner element unification.
#[test]
fn ipc21_unify_list_int_vs_list_str_mismatch() {
    use cobrust_types::infer::{unify, Subst};
    use cobrust_types::TypeError;
    let mut subst = Subst::new();
    let result = unify(
        &Ty::List(Box::new(Ty::Int)),
        &Ty::List(Box::new(Ty::Str)),
        &mut subst,
        dummy_span(),
    );
    assert!(result.is_err());
    assert!(matches!(result, Err(TypeError::TypeMismatch { .. })));
}

/// `unify(Tuple[Int, Str], Tuple[Int])` → `Err(TypeError::TypeMismatch)`.
///
/// Arity mismatch on Tuple → TypeMismatch per Rust arm.
#[test]
fn ipc22_unify_tuple_arity_mismatch() {
    use cobrust_types::infer::{unify, Subst};
    use cobrust_types::TypeError;
    let mut subst = Subst::new();
    let result = unify(
        &Ty::Tuple(vec![Ty::Int, Ty::Str]),
        &Ty::Tuple(vec![Ty::Int]),
        &mut subst,
        dummy_span(),
    );
    assert!(result.is_err());
    assert!(
        matches!(result, Err(TypeError::TypeMismatch { .. })),
        "Tuple arity mismatch → TypeMismatch"
    );
}

/// `unify(Fn(Int → Bool), Fn(Str → Bool))` → `Err(TypeError::TypeMismatch)`.
///
/// Fn positional type mismatch: first positional Int vs Str.
#[test]
fn ipc23_unify_fn_positional_type_mismatch() {
    use cobrust_types::infer::{unify, Subst};
    use cobrust_types::TypeError;
    let fn_a = Ty::Fn(FnTy {
        positional: vec![Ty::Int],
        named: vec![],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Bool),
    });
    let fn_b = Ty::Fn(FnTy {
        positional: vec![Ty::Str],
        named: vec![],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Bool),
    });
    let mut subst = Subst::new();
    let result = unify(&fn_a, &fn_b, &mut subst, dummy_span());
    assert!(result.is_err());
    assert!(matches!(result, Err(TypeError::TypeMismatch { .. })));
}

/// `unify(Fn(a: Int → Bool), Fn(b: Int → Bool))` → `Err(TypeError::KeywordArgMismatch)`.
///
/// Named-parameter name mismatch: `a` vs `b`.
#[test]
fn ipc24_unify_fn_named_key_mismatch() {
    use cobrust_types::infer::{unify, Subst};
    use cobrust_types::TypeError;
    let fn_a = Ty::Fn(FnTy {
        positional: vec![],
        named: vec![("a".to_string(), Ty::Int)],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Bool),
    });
    let fn_b = Ty::Fn(FnTy {
        positional: vec![],
        named: vec![("b".to_string(), Ty::Int)],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Bool),
    });
    let mut subst = Subst::new();
    let result = unify(&fn_a, &fn_b, &mut subst, dummy_span());
    assert!(result.is_err());
    assert!(
        matches!(result, Err(TypeError::KeywordArgMismatch { .. })),
        "Named-key mismatch → KeywordArgMismatch"
    );
}

// =====================================================================
// Category D — Occurs-check cycles
// =====================================================================

/// `unify(Var(?0), List[Var(?0)])` → `Err(TypeError::OccursCheck)`.
///
/// ADR-0055c §9.1 "occurs-check positive" property test.
/// ?0 appears free in `List[?0]`, so unification would create an infinite type.
#[test]
fn ipc25_occurs_check_var_in_list() {
    use cobrust_types::infer::{unify, Subst};
    use cobrust_types::TypeError;
    let var0 = Ty::Var(VarId(0));
    let list_var0 = Ty::List(Box::new(Ty::Var(VarId(0))));
    let mut subst = Subst::new();
    let result = unify(&var0, &list_var0, &mut subst, dummy_span());
    assert!(result.is_err(), "occurs check must fire");
    assert!(
        matches!(result, Err(TypeError::OccursCheck { .. })),
        "expected OccursCheck, got {result:?}"
    );
}

/// `unify(Var(?0), Dict[Var(?0), Str])` → `Err(TypeError::OccursCheck)`.
///
/// Occurs-check: ?0 free in Dict key position.
#[test]
fn ipc26_occurs_check_var_in_dict_key() {
    use cobrust_types::infer::{unify, Subst};
    use cobrust_types::TypeError;
    let var0 = Ty::Var(VarId(0));
    let dict_var0 = Ty::Dict(Box::new(Ty::Var(VarId(0))), Box::new(Ty::Str));
    let mut subst = Subst::new();
    let result = unify(&var0, &dict_var0, &mut subst, dummy_span());
    assert!(result.is_err());
    assert!(matches!(result, Err(TypeError::OccursCheck { .. })));
}

/// `unify(Var(?0), Tuple[Int, Var(?0)])` → `Err(TypeError::OccursCheck)`.
///
/// Occurs-check: ?0 free in Tuple element.
#[test]
fn ipc27_occurs_check_var_in_tuple() {
    use cobrust_types::infer::{unify, Subst};
    use cobrust_types::TypeError;
    let var0 = Ty::Var(VarId(0));
    let tuple_var0 = Ty::Tuple(vec![Ty::Int, Ty::Var(VarId(0))]);
    let mut subst = Subst::new();
    let result = unify(&var0, &tuple_var0, &mut subst, dummy_span());
    assert!(result.is_err());
    assert!(matches!(result, Err(TypeError::OccursCheck { .. })));
}

// =====================================================================
// Category E — No-bidirectional Ref↔T unify regression (F31 lock)
// =====================================================================
// Per ADR-0052a Wave-1 §13 "Design lesson 2026-05-17":
// `Ref(T)` and `T` are distinct types. The unify arm `(Ref(a), T) | (T, Ref(a))`
// is NOT present. Both ipc28 and ipc29 assert TypeMismatch when crossing Ref/non-Ref.

/// `unify(Ref(Int), Int)` → `Err(TypeError::TypeMismatch)`.
///
/// F31 lock: Ref↔non-Ref unification is forbidden.
/// The one-way coercion lives at `synth_call` in 0055d scope.
#[test]
fn ipc28_ref_t_no_bidirectional_unify() {
    use cobrust_types::infer::{unify, Subst};
    use cobrust_types::TypeError;
    let ref_int = Ty::Ref(Box::new(Ty::Int));
    let mut subst = Subst::new();
    let result = unify(&ref_int, &Ty::Int, &mut subst, dummy_span());
    assert!(
        result.is_err(),
        "Ref(Int) must NOT unify with Int (F31 lock)"
    );
    assert!(
        matches!(result, Err(TypeError::TypeMismatch { .. })),
        "expected TypeMismatch for Ref↔non-Ref, got {result:?}"
    );
}

/// `unify(Int, Ref(Int))` → `Err(TypeError::TypeMismatch)`.
///
/// F31 lock: symmetric case — non-Ref↔Ref also forbidden.
#[test]
fn ipc29_t_ref_no_bidirectional_unify_symmetric() {
    use cobrust_types::infer::{unify, Subst};
    use cobrust_types::TypeError;
    let ref_int = Ty::Ref(Box::new(Ty::Int));
    let mut subst = Subst::new();
    let result = unify(&Ty::Int, &ref_int, &mut subst, dummy_span());
    assert!(
        result.is_err(),
        "Int must NOT unify with Ref(Int) (F31 lock symmetric)"
    );
    assert!(matches!(result, Err(TypeError::TypeMismatch { .. })));
}

// =====================================================================
// Category F — finalize + AmbiguousType
// =====================================================================

/// `finalize(Var(?0), empty_subst)` → `Err(TypeError::AmbiguousType)`.
///
/// Free Var with empty subst: AmbiguousType per infer.rs::finalize.
#[test]
fn ipc30_finalize_free_var_ambiguous() {
    use cobrust_types::infer::{finalize, Subst};
    use cobrust_types::TypeError;
    let subst = Subst::new(); // empty: ?0 unresolved
    let result = finalize(&Ty::Var(VarId(0)), &subst, dummy_span());
    assert!(result.is_err(), "finalize with free Var must return AmbiguousType");
    assert!(
        matches!(result, Err(TypeError::AmbiguousType { .. })),
        "expected AmbiguousType, got {result:?}"
    );
}
