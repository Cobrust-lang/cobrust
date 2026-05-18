//! ADR-0055c Wave-3 Tier-2 — Display byte-parity for infer-produced `TyEntry`.
//!
//! F28 strict-separation: TEST scope only. No src/ impl edits in this commit.
//! All tests are `#[ignore = "ADR-0055c Wave-3 DEV impl pending"]`.
//!
//! These tests verify that `display_ty` on a cb `TyArena` root produced by
//! `ty_cb_arena_from_rust` is byte-identical to `format!("{}", rust_ty)` for
//! arena-form types that appear as outputs of `subst_apply`, `unify`, or
//! `finalize` in the `infer.rs` cb port.
//!
//! The `canonicalize_arena_root` path is also tested against the Rust
//! `Canonicalize for Ty` path to verify the 5-namespace canonical keys match.
//!
//! ## Test categories (7 tests)
//!
//! 1. Applied substitution output display    (idp01–idp03)
//! 2. Composite infer-form types display     (idp04–idp05)
//! 3. Arena-root canonical key parity        (idp06–idp07)
//!
//! ## Anchors (F34)
//!
//! - `infer_display_parity.rs::idp01_display_int_from_var_apply`
//! - `infer_display_parity.rs::idp06_canonical_key_parity_list_int`

#![allow(clippy::unwrap_used)]
#![allow(clippy::todo)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(unused_imports)]

use cobrust_types::{FnTy, Ty};
use cobrust_types_cb::{canonicalize_arena_root, display_ty, ty_cb_arena_from_rust, FnTyArena, RecordArena};
use cobrust_types_parity::{Canonicalize, TyArena as ParityArena};

// =====================================================================
// Helper: assert Display byte-parity for a Rust Ty
// =====================================================================

fn assert_display_parity(rust_ty: &Ty) {
    let expected = format!("{rust_ty}");
    let (root_id, cb_arena) = ty_cb_arena_from_rust(rust_ty);
    let fn_arena = FnTyArena::new();
    let rec_arena = RecordArena::new();
    let actual = display_ty(&cb_arena, &fn_arena, &rec_arena, root_id);
    assert_eq!(
        actual, expected,
        "Display byte-parity failed for {rust_ty}: expected {expected:?}, got {actual:?}"
    );
}

// =====================================================================
// Category 1 — Applied substitution output display
// =====================================================================

/// Display parity for `Int` — the most common `subst_apply` output.
///
/// `subst_apply({?0 → Int}, Var(?0))` produces `Int`; display must be `"i64"`.
#[test]
fn idp01_display_int_from_var_apply() {
    // The infer.rs apply of ?0→Int yields Ty::Int; cb side must produce "i64"
    assert_display_parity(&Ty::Int);
}

/// Display parity for `List[Str]` — `subst_apply({?0 → Str}, List[?0])` output form.
#[test]
fn idp02_display_list_str_from_apply() {
    assert_display_parity(&Ty::List(Box::new(Ty::Str)));
}

/// Display parity for `Dict[Int, Bool]` — `subst_apply` on Dict with two Vars.
#[test]
fn idp03_display_dict_int_bool_from_apply() {
    assert_display_parity(&Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Bool)));
}

// =====================================================================
// Category 2 — Composite infer-form types display
// =====================================================================

/// Display parity for a Fn type produced by `subst_apply` on a Fn with Vars.
///
/// `(Int) -> Bool` — positional + return after substitution.
#[test]
fn idp04_display_fn_int_to_bool() {
    let fn_ty = Ty::Fn(FnTy {
        positional: vec![Ty::Int],
        named: vec![],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Bool),
    });
    assert_display_parity(&fn_ty);
}

/// Display parity for a deep-nested type from `finalize` output.
///
/// `List[List[Dict[Int, Str]]]` — finalize on a fully-resolved nested type.
#[test]
fn idp05_display_deep_nested_finalize_output() {
    let ty = Ty::List(Box::new(Ty::List(Box::new(Ty::Dict(
        Box::new(Ty::Int),
        Box::new(Ty::Str),
    )))));
    assert_display_parity(&ty);
}

// =====================================================================
// Category 3 — Arena-root canonical key parity
// =====================================================================

/// Canonical key parity for `List[Int]` through arena-root path.
///
/// Verifies `canonicalize_arena_root` produces the same `CanonicalKey`
/// as `Ty::canonicalize` for a common unify output form.
#[test]
fn idp06_canonical_key_parity_list_int() {
    let rust_ty = Ty::List(Box::new(Ty::Int));
    let (cb_id, cb_arena) = ty_cb_arena_from_rust(&rust_ty);
    let mut rust_parity = ParityArena::new();
    let mut cb_parity = ParityArena::new();
    let rust_key = rust_ty.canonicalize(&mut rust_parity);
    let cb_key = canonicalize_arena_root(&cb_arena, &mut cb_parity, cb_id);
    assert_eq!(
        rust_key, cb_key,
        "canonical key parity failed for List[Int]"
    );
}

/// Canonical key parity for `Tuple[Bool, Float, Str]` — unify output form.
///
/// Three-element Tuple: verifies Tuple arm of `canonicalize_arena_root`.
#[test]
fn idp07_canonical_key_parity_tuple_three_elements() {
    let rust_ty = Ty::Tuple(vec![Ty::Bool, Ty::Float, Ty::Str]);
    let (cb_id, cb_arena) = ty_cb_arena_from_rust(&rust_ty);
    let mut rust_parity = ParityArena::new();
    let mut cb_parity = ParityArena::new();
    let rust_key = rust_ty.canonicalize(&mut rust_parity);
    let cb_key = canonicalize_arena_root(&cb_arena, &mut cb_parity, cb_id);
    assert_eq!(
        rust_key, cb_key,
        "canonical key parity failed for Tuple[Bool, Float, Str]"
    );
}
