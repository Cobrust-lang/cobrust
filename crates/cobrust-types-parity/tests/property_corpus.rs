//! ADR-0055e Wave-1 — Phase 1 + Phase 2 property test corpus.
//!
//! F28 strict-separation: test bodies + contract types ONLY. No impl.
//!
//! ## Categories
//!
//! **Phase 1 sanity (≥ 5)**: Rust-vs-Rust `parity_check` is always
//! `Ok` — proves the contract type compiles and the harness stub
//! wires to the right signature.
//!
//! **Phase 2 calibration (≥ 10)**: adversarial inputs that MUST surface
//! a `ParityError` variant — each test injects a concrete divergence
//! and asserts the harness catches it once DEV impl lands.
//!
//! All tests `#[ignore = "ADR-0055e Wave-1 DEV impl pending"]` — DEV
//! un-ignores after implementing `Canonicalize for Ty` + `parity_check`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(dead_code)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::uninlined_format_args)]

use cobrust_types::{AdtId, AliasId, FnTy, GenericVar, Record, Ty, VarId};
use cobrust_types_parity::{
    CanonicalKey, Canonicalize, ParityError, TyArena, manual_canonical_key, parity_check,
};

// =====================================================================
// Phase 1 — sanity: Rust-vs-Rust parity_check always Ok
// (≥ 5 tests)
// =====================================================================

/// P1-01: `parity_check(Int, Int)` → `Ok(())`
#[test]
fn p1_01_int_vs_int_ok() {
    let ty = Ty::Int;
    let mut arena = TyArena::new();
    assert!(
        parity_check(&ty, &ty, &mut arena).is_ok(),
        "Int vs Int must be Ok"
    );
}

/// P1-02: `parity_check(Bool, Bool)` → `Ok(())`
#[test]
fn p1_02_bool_vs_bool_ok() {
    let ty = Ty::Bool;
    let mut arena = TyArena::new();
    assert!(
        parity_check(&ty, &ty, &mut arena).is_ok(),
        "Bool vs Bool must be Ok"
    );
}

/// P1-03: `parity_check(List[Str], List[Str])` → `Ok(())`
#[test]
fn p1_03_list_str_vs_list_str_ok() {
    let ty = Ty::List(Box::new(Ty::Str));
    let mut arena = TyArena::new();
    assert!(
        parity_check(&ty, &ty, &mut arena).is_ok(),
        "List[Str] vs List[Str] must be Ok"
    );
}

/// P1-04: `parity_check(Dict[Int, Bool], Dict[Int, Bool])` → `Ok(())`
#[test]
fn p1_04_dict_int_bool_vs_same_ok() {
    let ty = Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Bool));
    let mut arena = TyArena::new();
    assert!(
        parity_check(&ty, &ty, &mut arena).is_ok(),
        "Dict[Int,Bool] vs Dict[Int,Bool] must be Ok"
    );
}

/// P1-05: `parity_check(Tuple([Int, Str, Float]), same)` → `Ok(())`
#[test]
fn p1_05_tuple_int_str_float_vs_same_ok() {
    let ty = Ty::Tuple(vec![Ty::Int, Ty::Str, Ty::Float]);
    let mut arena = TyArena::new();
    assert!(
        parity_check(&ty, &ty, &mut arena).is_ok(),
        "Tuple([Int,Str,Float]) vs same must be Ok"
    );
}

/// P1-06: `parity_check(Ref(Int), Ref(Int))` → `Ok(())` — ADR-0052a borrow type
#[test]
fn p1_06_ref_int_vs_ref_int_ok() {
    let ty = Ty::Ref(Box::new(Ty::Int));
    let mut arena = TyArena::new();
    assert!(
        parity_check(&ty, &ty, &mut arena).is_ok(),
        "Ref(Int) vs Ref(Int) must be Ok"
    );
}

/// P1-07: nested List[List[Dict[Str,Int]]] vs same → `Ok(())`
/// Exercises deep nesting — canonical traversal must recurse.
#[test]
fn p1_07_deeply_nested_ok() {
    let inner = Ty::Dict(Box::new(Ty::Str), Box::new(Ty::Int));
    let mid = Ty::List(Box::new(inner));
    let outer = Ty::List(Box::new(mid));
    let mut arena = TyArena::new();
    assert!(
        parity_check(&outer, &outer, &mut arena).is_ok(),
        "List[List[Dict[Str,Int]]] vs same must be Ok"
    );
}

// =====================================================================
// Phase 2 — calibration: harness MUST catch injected divergence
// (≥ 10 tests)
// =====================================================================

/// P2-01: `parity_check(Int, Str)` → `Err(CanonicalPayloadMismatch)`
/// Most fundamental divergence — scalar variant mismatch.
#[test]
fn p2_01_int_vs_str_catches_mismatch() {
    let rust_ty = Ty::Int;
    let cb_ty = Ty::Str;
    let mut arena = TyArena::new();
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        matches!(result, Err(ParityError::CanonicalPayloadMismatch { .. })),
        "Int vs Str must surface CanonicalPayloadMismatch, got: {result:?}"
    );
}

/// P2-02: `parity_check(List[Int], List[Str])` → `Err(CanonicalPayloadMismatch)`
/// Child-level divergence inside a container.
#[test]
fn p2_02_list_int_vs_list_str_catches_mismatch() {
    let rust_ty = Ty::List(Box::new(Ty::Int));
    let cb_ty = Ty::List(Box::new(Ty::Str));
    let mut arena = TyArena::new();
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        matches!(result, Err(ParityError::CanonicalPayloadMismatch { .. })),
        "List[Int] vs List[Str] must surface CanonicalPayloadMismatch, got: {result:?}"
    );
}

/// P2-03: `parity_check(Tuple([Int, Str]), Tuple([Str, Int]))` — order matters
/// Canonicalization must preserve child ordering; reversed tuple != original.
#[test]
fn p2_03_tuple_order_mismatch_caught() {
    let rust_ty = Ty::Tuple(vec![Ty::Int, Ty::Str]);
    let cb_ty = Ty::Tuple(vec![Ty::Str, Ty::Int]);
    let mut arena = TyArena::new();
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        matches!(result, Err(ParityError::CanonicalPayloadMismatch { .. })),
        "Tuple([Int,Str]) vs Tuple([Str,Int]) must be caught"
    );
}

/// P2-04: `parity_check(Dict[Int, Str], Dict[Str, Int])` — key/val swap
/// Inverted Dict[K,V] vs Dict[V,K] is a real divergence.
#[test]
fn p2_04_dict_key_val_swap_caught() {
    let rust_ty = Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Str));
    let cb_ty = Ty::Dict(Box::new(Ty::Str), Box::new(Ty::Int));
    let mut arena = TyArena::new();
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        matches!(result, Err(ParityError::CanonicalPayloadMismatch { .. })),
        "Dict[Int,Str] vs Dict[Str,Int] must be caught"
    );
}

/// P2-05: `parity_check(Set[Int], List[Int])` — container-kind swap
/// `Set` vs `List` with same inner type must be caught.
#[test]
fn p2_05_set_vs_list_kind_mismatch() {
    let rust_ty = Ty::Set(Box::new(Ty::Int));
    let cb_ty = Ty::List(Box::new(Ty::Int));
    let mut arena = TyArena::new();
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        matches!(result, Err(ParityError::CanonicalPayloadMismatch { .. })),
        "Set[Int] vs List[Int] must be caught"
    );
}

/// P2-06: VarId with different raw ids but SAME structure
/// `Var(VarId(7))` vs `Var(VarId(3))` should canonicalize to the same
/// key when both are first-encountered in the same traversal order.
/// This is the core arena-id renaming tolerance test — only works once
/// DEV implements the dense-pack renaming.
#[test]
fn p2_06_var_id_renaming_tolerance() {
    // Both are stand-alone Var types; same structure, different raw ids.
    // Post the rename, both should be Var(0) canonically → parity_check Ok.
    let rust_ty = Ty::Var(VarId(7));
    let cb_ty = Ty::Var(VarId(3));
    let mut arena = TyArena::new();
    // Same structural shape; only raw ids differ → must be Ok after renaming.
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        result.is_ok(),
        "Var(7) vs Var(3) must be Ok after dense-pack renaming, got: {result:?}"
    );
}

/// P2-07: AdtId with different raw ids but same structural args
/// `Adt(AdtId(1), [Int])` vs `Adt(AdtId(5), [Int])` → same canonical key
/// because both are "first-encountered Adt with arg Int".
#[test]
fn p2_07_adt_id_renaming_tolerance() {
    let rust_ty = Ty::Adt(AdtId(1), vec![Ty::Int]);
    let cb_ty = Ty::Adt(AdtId(5), vec![Ty::Int]);
    let mut arena = TyArena::new();
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        result.is_ok(),
        "Adt#1[Int] vs Adt#5[Int] must be Ok after dense-pack renaming, got: {result:?}"
    );
}

/// P2-08: AdtId renaming does NOT collapse Adt types with different args
/// `Adt(AdtId(1), [Int])` vs `Adt(AdtId(1), [Str])` — same raw id, different args
/// → must still be caught (canonical key differs by child).
#[test]
fn p2_08_adt_same_id_different_args_caught() {
    let rust_ty = Ty::Adt(AdtId(1), vec![Ty::Int]);
    let cb_ty = Ty::Adt(AdtId(1), vec![Ty::Str]);
    let mut arena = TyArena::new();
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        matches!(result, Err(ParityError::CanonicalPayloadMismatch { .. })),
        "Adt#1[Int] vs Adt#1[Str] must be caught (different args)"
    );
}

/// P2-09: AliasId renaming tolerance
/// `Alias(AliasId(2), [Bool])` vs `Alias(AliasId(9), [Bool])` → Ok after renaming.
#[test]
fn p2_09_alias_id_renaming_tolerance() {
    let rust_ty = Ty::Alias(AliasId(2), vec![Ty::Bool]);
    let cb_ty = Ty::Alias(AliasId(9), vec![Ty::Bool]);
    let mut arena = TyArena::new();
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        result.is_ok(),
        "Alias#2[Bool] vs Alias#9[Bool] must be Ok after dense-pack renaming, got: {result:?}"
    );
}

/// P2-10: GenericVar renaming tolerance
/// `Generic(GenericVar(0))` vs `Generic(GenericVar(4))` → Ok after renaming.
#[test]
fn p2_10_generic_var_renaming_tolerance() {
    let rust_ty = Ty::Generic(GenericVar(0));
    let cb_ty = Ty::Generic(GenericVar(4));
    let mut arena = TyArena::new();
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        result.is_ok(),
        "Generic#0 vs Generic#4 must be Ok after dense-pack renaming, got: {result:?}"
    );
}

/// P2-11: Record field name divergence is caught
/// Two Records with different field names but same value types → caught.
#[test]
fn p2_11_record_field_name_divergence_caught() {
    let rust_ty = Ty::Record(Record::from_pairs(vec![("x".to_string(), Ty::Int)]));
    let cb_ty = Ty::Record(Record::from_pairs(vec![("y".to_string(), Ty::Int)]));
    let mut arena = TyArena::new();
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        matches!(result, Err(ParityError::CanonicalPayloadMismatch { .. })),
        "Record{{x: Int}} vs Record{{y: Int}} must be caught"
    );
}

/// P2-12: Fn return type divergence is caught
/// Two `FnTy` with same params but different return type → caught.
#[test]
fn p2_12_fn_return_type_divergence_caught() {
    let rust_ty = Ty::Fn(FnTy {
        positional: vec![Ty::Int],
        named: vec![],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Bool),
    });
    let cb_ty = Ty::Fn(FnTy {
        positional: vec![Ty::Int],
        named: vec![],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Str), // divergence: Bool vs Str
    });
    let mut arena = TyArena::new();
    let result = parity_check(&rust_ty, &cb_ty, &mut arena);
    assert!(
        matches!(result, Err(ParityError::CanonicalPayloadMismatch { .. })),
        "Fn(Int)->Bool vs Fn(Int)->Str must be caught"
    );
}

/// P2-13: `ParityError::AcceptReject` variant is constructible and carries correct fields
/// Validates that the contract type is well-formed for accept/reject cases.
#[test]
fn p2_13_accept_reject_error_well_formed() {
    let err = ParityError::AcceptReject {
        rust_accepted: true,
        cb_accepted: false,
    };
    let msg = err.to_string();
    assert!(msg.contains("rust=true"), "error message must contain rust=true: {msg}");
    assert!(msg.contains("cb=false"), "error message must contain cb=false: {msg}");
}

/// P2-14: `ParityError::VariantMismatch` variant is constructible
#[test]
fn p2_14_variant_mismatch_error_well_formed() {
    let err = ParityError::VariantMismatch {
        rust_variant: "ImplicitTruthiness".to_string(),
        cb_variant: "TypeMismatch".to_string(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("ImplicitTruthiness"),
        "error message must name rust variant: {msg}"
    );
    assert!(
        msg.contains("TypeMismatch"),
        "error message must name cb variant: {msg}"
    );
}

/// P2-15: `ParityError::SuggestionMismatch` variant is constructible
#[test]
fn p2_15_suggestion_mismatch_error_well_formed() {
    let err = ParityError::SuggestionMismatch {
        variant: "ImplicitTruthiness".to_string(),
        rust_suggestion: Some("change to 'if x != 0:'".to_string()),
        cb_suggestion: None,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("ImplicitTruthiness"),
        "error message must name variant: {msg}"
    );
}
