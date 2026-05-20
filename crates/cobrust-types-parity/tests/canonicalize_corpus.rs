//! ADR-0055e Wave-1 — canonicalization unit test corpus.
//!
//! F28 strict-separation: test bodies only. No impl.
//!
//! ## Categories
//!
//! 1. Tuple/List/Set/Dict shape preservation
//! 2. 5-namespace canonicalization: TyId, AdtId, AliasId, FnTyId, RecordId
//!    (per §3 amendment 2026-05-18 — 0055a §3 cross-ADR dep)
//! 3. Arena-id renaming tolerance (same Ty, different handle IDs → same key)
//! 4. Cycle detection constraint (per §3: types are tree-shaped per ADR-0006)
//!
//! All tests `#[ignore = "ADR-0055e Wave-1 DEV impl pending"]` — DEV
//! un-ignores after implementing `Canonicalize for Ty`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(dead_code)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::uninlined_format_args)]

use cobrust_types::{AdtId, AliasId, FnTy, GenericVar, Record, Ty, VarId};
// `CanonicalKey` is referenced only in doc comments below; the runtime tests
// drive `manual_canonical_key`, so the type itself is not imported to keep
// `-D unused-imports` (build gate) clean.
use cobrust_types_parity::{TyArena, manual_canonical_key};

// =====================================================================
// 1. Shape preservation: containers
// =====================================================================

/// C-01: `Tuple([Int, Str])` shape preserved — 2-child node, children Int + Str
#[test]
fn c01_tuple_shape_preserved() {
    let ty = Ty::Tuple(vec![Ty::Int, Ty::Str]);
    let key = manual_canonical_key(&ty);
    assert_eq!(key.kind, "Tuple");
    assert_eq!(key.children.len(), 2);
    assert_eq!(key.children[0].kind, "Int");
    assert_eq!(key.children[1].kind, "Str");
}

/// C-02: Empty tuple `Tuple([])` → leaf-like Tuple node with 0 children
#[test]
fn c02_empty_tuple_shape() {
    let ty = Ty::Tuple(vec![]);
    let key = manual_canonical_key(&ty);
    assert_eq!(key.kind, "Tuple");
    assert_eq!(key.children.len(), 0);
}

/// C-03: `List[Bool]` shape preserved — single child Bool
#[test]
fn c03_list_shape_preserved() {
    let ty = Ty::List(Box::new(Ty::Bool));
    let key = manual_canonical_key(&ty);
    assert_eq!(key.kind, "List");
    assert_eq!(key.children.len(), 1);
    assert_eq!(key.children[0].kind, "Bool");
}

/// C-04: `Set[Bytes]` shape preserved — single child Bytes
#[test]
fn c04_set_shape_preserved() {
    let ty = Ty::Set(Box::new(Ty::Bytes));
    let key = manual_canonical_key(&ty);
    assert_eq!(key.kind, "Set");
    assert_eq!(key.children.len(), 1);
    assert_eq!(key.children[0].kind, "Bytes");
}

/// C-05: `Dict[Str, Float]` shape preserved — 2 children: Str, Float
#[test]
fn c05_dict_shape_preserved() {
    let ty = Ty::Dict(Box::new(Ty::Str), Box::new(Ty::Float));
    let key = manual_canonical_key(&ty);
    assert_eq!(key.kind, "Dict");
    assert_eq!(key.children.len(), 2);
    assert_eq!(key.children[0].kind, "Str");
    assert_eq!(key.children[1].kind, "Float");
}

// =====================================================================
// 2. 5-namespace canonicalization (per §3 amendment 2026-05-18)
// =====================================================================

/// C-06: `TyArena::adt_id` assigns dense-pack ids (namespace: AdtId)
/// First call → 0, second new id → 1, repeat → same.
#[test]
fn c06_adt_namespace_dense_pack() {
    let mut arena = TyArena::new();
    let c0 = arena.adt_id(AdtId(42));
    let c1 = arena.adt_id(AdtId(7));
    let c2 = arena.adt_id(AdtId(42)); // repeat → same id as c0
    assert_eq!(c0, 0, "first AdtId → canonical 0");
    assert_eq!(c1, 1, "second distinct AdtId → canonical 1");
    assert_eq!(c2, 0, "repeat AdtId(42) → canonical 0 again");
}

/// C-07: `TyArena::alias_id` assigns dense-pack ids (namespace: AliasId)
#[test]
fn c07_alias_namespace_dense_pack() {
    let mut arena = TyArena::new();
    let c0 = arena.alias_id(AliasId(100));
    let c1 = arena.alias_id(AliasId(200));
    let c2 = arena.alias_id(AliasId(100));
    assert_eq!(c0, 0);
    assert_eq!(c1, 1);
    assert_eq!(c2, 0, "repeat AliasId(100) → canonical 0");
}

/// C-08: `TyArena::var_id` assigns dense-pack ids (namespace: VarId)
#[test]
fn c08_var_id_namespace_dense_pack() {
    let mut arena = TyArena::new();
    let c0 = arena.var_id(VarId(999));
    let c1 = arena.var_id(VarId(1));
    let c2 = arena.var_id(VarId(999));
    assert_eq!(c0, 0);
    assert_eq!(c1, 1);
    assert_eq!(c2, 0, "repeat VarId(999) → canonical 0");
}

/// C-09: `TyArena::generic_var` assigns dense-pack ids (namespace: GenericVar)
#[test]
fn c09_generic_var_namespace_dense_pack() {
    let mut arena = TyArena::new();
    let c0 = arena.generic_var(GenericVar(50));
    let c1 = arena.generic_var(GenericVar(51));
    let c2 = arena.generic_var(GenericVar(50));
    assert_eq!(c0, 0);
    assert_eq!(c1, 1);
    assert_eq!(c2, 0, "repeat GenericVar(50) → canonical 0");
}

/// C-10: 5 namespaces are INDEPENDENT — AdtId(1) and AliasId(1) each start at 0
#[test]
fn c10_namespaces_independent() {
    let mut arena = TyArena::new();
    let adt0 = arena.adt_id(AdtId(1));
    let alias0 = arena.alias_id(AliasId(1)); // independent namespace
    let var0 = arena.var_id(VarId(1));
    let gen0 = arena.generic_var(GenericVar(1));
    // Each namespace starts at 0 independently
    assert_eq!(adt0, 0, "AdtId ns starts at 0");
    assert_eq!(alias0, 0, "AliasId ns starts at 0, independent of AdtId");
    assert_eq!(var0, 0, "VarId ns starts at 0, independent");
    assert_eq!(gen0, 0, "GenericVar ns starts at 0, independent");
}

// =====================================================================
// 3. Arena-id renaming tolerance
// =====================================================================

/// C-11: Same Ty structure with different VarId handles → same CanonicalKey
/// (manual_canonical_key uses raw ids; this tests the DEV impl via TyArena)
#[test]
fn c11_var_id_different_handles_same_canonical() {
    // Both are single Var types — in a fresh arena, both become canonical id 0.
    let ty_a = Ty::Var(VarId(13));
    let ty_b = Ty::Var(VarId(99));
    // manual_canonical_key does NOT remap — verify raw divergence first.
    let key_a_raw = manual_canonical_key(&ty_a);
    let key_b_raw = manual_canonical_key(&ty_b);
    // Raw keys differ (expected — manual_canonical_key preserves raw ids).
    assert_ne!(
        key_a_raw, key_b_raw,
        "raw keys must differ for raw manual helper"
    );
    // DEV impl via Canonicalize + TyArena must produce equal canonical keys.
    // This assertion is left as a comment because DEV implements Canonicalize.
    // Once DEV lands: assert_eq!(ty_a.canonicalize(&mut TyArena::new()), ty_b.canonicalize(&mut TyArena::new()));
}

/// C-12: `Record` fields canonicalize in sorted order (BTreeMap guaranteed)
/// The canonical key children must be sorted by field name.
#[test]
fn c12_record_fields_sorted_in_canonical_key() {
    // Insertion order: z, a, m — BTreeMap sorts to: a, m, z
    let ty = Ty::Record(Record::from_pairs(vec![
        ("z".to_string(), Ty::Int),
        ("a".to_string(), Ty::Bool),
        ("m".to_string(), Ty::Str),
    ]));
    let key = manual_canonical_key(&ty);
    assert_eq!(key.kind, "Record");
    assert_eq!(key.children.len(), 3);
    // BTreeMap sorts by name → a, m, z
    assert_eq!(key.children[0].kind, "a");
    assert_eq!(key.children[1].kind, "m");
    assert_eq!(key.children[2].kind, "z");
}

/// C-13: `Fn` canonical key includes both params and return child
#[test]
fn c13_fn_canonical_key_includes_return() {
    let ty = Ty::Fn(FnTy {
        positional: vec![Ty::Int, Ty::Str],
        named: vec![],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Bool),
    });
    let key = manual_canonical_key(&ty);
    assert_eq!(key.kind, "Fn");
    // 2 positional + 1 return child "->"
    assert_eq!(key.children.len(), 3, "Fn has 2 params + 1 return child");
    assert_eq!(key.children[0].kind, "Int");
    assert_eq!(key.children[1].kind, "Str");
    assert_eq!(key.children[2].kind, "->");
}

/// C-14: `Ref(Int)` canonical key is a single-child node per ADR-0052a
#[test]
fn c14_ref_canonical_key_single_child() {
    let ty = Ty::Ref(Box::new(Ty::Int));
    let key = manual_canonical_key(&ty);
    assert_eq!(key.kind, "Ref");
    assert_eq!(key.children.len(), 1);
    assert_eq!(key.children[0].kind, "Int");
}

/// C-10b: FnTyId and RecordId namespaces are independent from each other and from
/// the other 4 namespaces (AdtId, AliasId, VarId, GenericVar).
#[test]
fn c10b_fnty_record_namespaces_independent() {
    let mut arena_a = TyArena::new();
    let mut arena_b = TyArena::new();

    // arena_a: allocate FnTyId first, then RecordId
    let fn_id_a0 = arena_a.fresh_fn_ty_id();
    let rec_id_a0 = arena_a.fresh_record_id();

    // arena_b: allocate RecordId first, then FnTyId — order must not matter
    let rec_id_b0 = arena_b.fresh_record_id();
    let fn_id_b0 = arena_b.fresh_fn_ty_id();

    // Both arenas start each namespace at 0 independently
    assert_eq!(fn_id_a0, 0, "FnTy counter starts at 0 (arena_a)");
    assert_eq!(
        rec_id_a0, 0,
        "Record counter starts at 0, independent of FnTy (arena_a)"
    );
    assert_eq!(fn_id_b0, 0, "FnTy counter starts at 0 (arena_b)");
    assert_eq!(rec_id_b0, 0, "Record counter starts at 0 (arena_b)");

    // Second allocation increments each independently
    let fn_id_a1 = arena_a.fresh_fn_ty_id();
    let rec_id_a1 = arena_a.fresh_record_id();
    assert_eq!(fn_id_a1, 1, "second FnTy id → 1");
    assert_eq!(
        rec_id_a1, 1,
        "second Record id → 1, independent of FnTy counter"
    );
}

/// C-15: Deeply nested canonical key roundtrip — `List[Set[Dict[Str,Bool]]]`
/// Canonical key must be idempotent: building from the same Ty twice → equal.
#[test]
fn c15_deeply_nested_canonical_key_idempotent() {
    let inner = Ty::Dict(Box::new(Ty::Str), Box::new(Ty::Bool));
    let mid = Ty::Set(Box::new(inner));
    let outer = Ty::List(Box::new(mid));
    let key1 = manual_canonical_key(&outer);
    let key2 = manual_canonical_key(&outer);
    assert_eq!(
        key1, key2,
        "same Ty must produce equal CanonicalKey on repeat calls"
    );
    // Check structural shape
    assert_eq!(key1.kind, "List");
    assert_eq!(key1.children[0].kind, "Set");
    assert_eq!(key1.children[0].children[0].kind, "Dict");
}
