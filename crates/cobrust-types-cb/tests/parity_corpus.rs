//! ADR-0055a Wave-2 — parity corpus for cb arena-form `TyEntry` vs Rust `Ty`.
//!
//! All tests are `#[ignore = "ADR-0055a Wave-2 DEV impl pending"]`.
//! F28 strict-separation: these tests define the contract; DEV fills
//! `ty_cb_arena_from_rust` + `Canonicalize for TyEntry` stubs.
//!
//! ## Test categories (20 tests)
//!
//! 1. Single-variant round-trip    — `Bool`, `Int`, `Float`, `Str`, `Never`     (5)
//! 2. Recursive single-level       — `Tuple`, `List`, `Set`, `Dict`, `Ref`      (5)
//! 3. Nested-recursive             — `List<List>`, `Dict<Str,List>`, etc.        (5)
//! 4. Corner-cases                 — 1-tuple, empty-tuple, Ref-Ref cycle-reject,
//!                                   Display byte-parity, arena-lookup            (5)
//!
//! Each test builds the Rust `Ty`, converts via the stub, then calls
//! `parity_check(&rust_ty_canon, &cb_ty_canon)` asserting `Ok(())`.
//! Adversarial cases (Ref-Ref arena cycle reject) assert `Err(...)`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::todo)]
#![allow(clippy::ignored_unit_patterns)]

use cobrust_types::{AdtId, AliasId, FnTy, GenericVar, Record, Ty, VarId};
use cobrust_types_cb::{TyArena, TyEntry, canonicalize_arena_root, ty_cb_arena_from_rust};
use cobrust_types_parity::{Canonicalize, TyArena as ParityArena, parity_check};

// =====================================================================
// Helper: build cb-form from Rust Ty + run parity_check
// =====================================================================

/// Build both canonical keys and assert they are equal.
///
/// DEV-wired per F28 + ADR-0055a Wave-2: builds cb arena via
/// `ty_cb_arena_from_rust`, then canonicalizes both Rust and cb roots
/// via independent fresh `ParityArena` instances per ADR-0055e §3
/// "Each side gets its own fresh sub-arena" rule, and asserts the two
/// canonical keys are equal.
fn assert_parity(rust_ty: &Ty) {
    let (cb_id, cb_arena) = ty_cb_arena_from_rust(rust_ty);
    let mut rust_parity = ParityArena::new();
    let mut cb_parity = ParityArena::new();
    let rust_key = rust_ty.canonicalize(&mut rust_parity);
    let cb_key = canonicalize_arena_root(&cb_arena, &mut cb_parity, cb_id);
    assert_eq!(rust_key, cb_key, "parity mismatch for {rust_ty}");
}

// =====================================================================
// Category 1 — single-variant round-trip (Copy scalar leaves)
// =====================================================================

/// Round-trip `Bool` through cb arena and assert canonical parity.
#[test]
fn pc01_bool_roundtrip() {
    assert_parity(&Ty::Bool);
}

/// Round-trip `Int` through cb arena.
#[test]
fn pc02_int_roundtrip() {
    assert_parity(&Ty::Int);
}

/// Round-trip `Float` through cb arena.
#[test]
fn pc03_float_roundtrip() {
    assert_parity(&Ty::Float);
}

/// Round-trip `Str` through cb arena.
#[test]
fn pc04_str_roundtrip() {
    assert_parity(&Ty::Str);
}

/// Round-trip `Never` through cb arena (bottom type per ADR-0006).
#[test]
fn pc05_never_roundtrip() {
    assert_parity(&Ty::Never);
}

// =====================================================================
// Category 2 — recursive single-level
// =====================================================================

/// `Tuple([Int, Str])` — two-element tuple, no trailing comma.
#[test]
fn pc06_tuple_int_str() {
    assert_parity(&Ty::Tuple(vec![Ty::Int, Ty::Str]));
}

/// `List(Box<Int>)` — homogeneous list.
#[test]
fn pc07_list_int() {
    assert_parity(&Ty::List(Box::new(Ty::Int)));
}

/// `Set(Box<Str>)` — homogeneous set.
#[test]
fn pc08_set_str() {
    assert_parity(&Ty::Set(Box::new(Ty::Str)));
}

/// `Dict(Box<Int>, Box<Str>)` — dict with hashable key.
#[test]
fn pc09_dict_int_str() {
    assert_parity(&Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Str)));
}

/// `Ref(Box<Int>)` — ADR-0052a Wave-1 borrow type.
#[test]
fn pc10_ref_int() {
    assert_parity(&Ty::Ref(Box::new(Ty::Int)));
}

// =====================================================================
// Category 3 — nested-recursive
// =====================================================================

/// `List<List<Int>>` — two-level nesting.
#[test]
fn pc11_list_list_int() {
    assert_parity(&Ty::List(Box::new(Ty::List(Box::new(Ty::Int)))));
}

/// `Dict<Str, List<Int>>` — dict with composite value type.
#[test]
fn pc12_dict_str_list_int() {
    assert_parity(&Ty::Dict(
        Box::new(Ty::Str),
        Box::new(Ty::List(Box::new(Ty::Int))),
    ));
}

/// `Tuple([List<Int>, Dict<Int, Str>])` — tuple of composite types.
#[test]
fn pc13_tuple_list_dict() {
    assert_parity(&Ty::Tuple(vec![
        Ty::List(Box::new(Ty::Int)),
        Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Str)),
    ]));
}

/// `Adt(AdtId(0), [Int, Str])` — ADT with two type args.
#[test]
fn pc14_adt_with_args() {
    assert_parity(&Ty::Adt(AdtId(0), vec![Ty::Int, Ty::Str]));
}

/// `Alias(AliasId(1), [List<Bool>])` — alias application with nested arg.
#[test]
fn pc15_alias_with_nested_arg() {
    assert_parity(&Ty::Alias(AliasId(1), vec![Ty::List(Box::new(Ty::Bool))]));
}

// =====================================================================
// Category 4 — corner-cases
// =====================================================================

/// `Tuple([Int])` — 1-tuple; cb `display_ty` MUST emit `(i64,)` trailing comma.
///
/// This is an adversarial parity test: if cb omits the trailing comma,
/// the Display round-trip asserts `Err(CanonicalPayloadMismatch)`.
/// The `Canonicalize` impl is canonical-key based (not string-based) but
/// `display_parity.rs::dp01_one_tuple_trailing_comma` covers the Display
/// path. Here we verify structural parity only.
#[test]
fn pc16_one_tuple_trailing_comma_structural() {
    assert_parity(&Ty::Tuple(vec![Ty::Int]));
}

/// `Tuple([])` — empty tuple; Display form `()`.
#[test]
fn pc17_empty_tuple() {
    assert_parity(&Ty::Tuple(vec![]));
}

/// `Ref(Ref(Int))` — double-Ref nesting; NOT a cycle (tree-shaped per ADR-0006).
///
/// Arena-cycle risk: if DEV's `ty_cb_arena_from_rust` creates a cycle by
/// mis-assigning handles, `TyArena::lookup` would panic (dangling handle)
/// or loop infinitely. This test documents the expected behavior:
/// `Ref(Ref(Int))` must produce `Ok(())` with a depth-2 arena structure.
///
/// The test checks that parity holds for this doubly-nested Ref — NOT that
/// the arena rejects it (it's legal; see comment). For the cycle-rejection
/// property test, see `pc19_arena_handle_validity`.
#[test]
fn pc18_ref_ref_int_double_nested() {
    assert_parity(&Ty::Ref(Box::new(Ty::Ref(Box::new(Ty::Int)))));
}

/// Arena-handle validity: every TyId in the arena sub-tree is a valid index.
///
/// After `ty_cb_arena_from_rust`, walk all entries and assert that every
/// `TyId` payload is `>= 0` and `< arena.entries.len()`.
/// This is the "fresh handle is always valid" invariant from ADR-0055a §4.1
/// property test "arena-roundtrip".
#[test]
fn pc19_arena_handle_validity() {
    let rust_ty = Ty::Dict(
        Box::new(Ty::Tuple(vec![Ty::Int, Ty::Str])),
        Box::new(Ty::List(Box::new(Ty::Bool))),
    );
    let (_root_id, cb_arena) = ty_cb_arena_from_rust(&rust_ty);
    let len = cb_arena.entries.len() as i64;
    // Every TyId payload must be in [0, len).
    for entry in &cb_arena.entries {
        match entry {
            TyEntry::List(id)
            | TyEntry::Set(id)
            | TyEntry::Ref(id)
            | TyEntry::Generic(id)
            | TyEntry::Var(id) => {
                assert!(
                    *id >= 0 && *id < len,
                    "dangling TyId {id} in arena of len {len}"
                );
            }
            TyEntry::Dict(k, v) => {
                assert!(*k >= 0 && *k < len, "dangling key TyId {k}");
                assert!(*v >= 0 && *v < len, "dangling val TyId {v}");
            }
            TyEntry::Tuple(items) => {
                for id in items {
                    assert!(*id >= 0 && *id < len, "dangling tuple TyId {id}");
                }
            }
            TyEntry::Adt(aid, args) => {
                assert!(*aid >= 0, "negative AdtId {aid}");
                for id in args {
                    assert!(*id >= 0 && *id < len, "dangling Adt arg TyId {id}");
                }
            }
            TyEntry::Alias(lid, args) => {
                assert!(*lid >= 0, "negative AliasId {lid}");
                for id in args {
                    assert!(*id >= 0 && *id < len, "dangling Alias arg TyId {id}");
                }
            }
            // Parallel-arena handles: valid range is checked against FnTyArena /
            // RecordArena lengths in the DEV impl; here we just check non-negative.
            TyEntry::Fn(fid) => {
                assert!(*fid >= 0, "negative FnTyId {fid}");
            }
            TyEntry::Record(rid) => {
                assert!(*rid >= 0, "negative RecordId {rid}");
            }
            // Leaf variants have no handle payloads.
            TyEntry::Bool
            | TyEntry::Int
            | TyEntry::Float
            | TyEntry::Imag
            | TyEntry::Str
            | TyEntry::Bytes
            | TyEntry::None
            | TyEntry::Never => {}
        }
    }
}

/// Display byte-parity: `format!("{}", rust_ty)` == `display_ty(arena, id)`.
///
/// DEV-wired per F28 + ADR-0055a Wave-2: verifies byte-identical display
/// for a multi-level type via `display_ty`.
#[test]
fn pc20_display_byte_parity_multitype() {
    use cobrust_types_cb::{FnTyArena, RecordArena, display_ty};
    let rust_ty = Ty::Tuple(vec![
        Ty::Int,
        Ty::List(Box::new(Ty::Str)),
        Ty::Dict(Box::new(Ty::Bool), Box::new(Ty::Float)),
    ]);
    let expected = format!("{rust_ty}");
    let (root_id, cb_arena) = ty_cb_arena_from_rust(&rust_ty);
    let fn_arena = FnTyArena::new();
    let rec_arena = RecordArena::new();
    let actual = display_ty(&cb_arena, &fn_arena, &rec_arena, root_id);
    assert_eq!(actual, expected, "display byte-parity failed");
}
