//! ADR-0055a Wave-2 — Display byte-parity corpus.
//!
//! All tests are `#[ignore = "ADR-0055a Wave-2 DEV impl pending"]`.
//! F28 strict-separation: these tests define the Display contract; DEV
//! fills `display_ty` stub and wires `ty_cb_arena_from_rust`.
//!
//! ## Coverage (10 tests)
//!
//! - `dp01` — 1-tuple trailing comma `(i64,)` vs `(i64)`
//! - `dp02` — FnTy named-separator `(a: i64, b: str) -> bool`
//! - `dp03` — `&{inner}` glyph (Ref)
//! - `dp04` — `List[{T}]` bracket glyph
//! - `dp05` — `Dict[{K}, {V}]` bracket glyph
//! - `dp06` — `(,)` empty-tuple glyph `()`
//! - `dp07` — type-annotation glyph `:` in Record display `{name: T}`
//! - `dp08` — nested `List<List<Int>>` Display
//! - `dp09` — `Adt#0[i64, str]` prefix glyph
//! - `dp10` — `Alias#1[...]` prefix + `T0` generic + `?0` var glyphs
//!
//! Each test asserts `display_ty(arena, fn_arena, rec_arena, root_id)`
//! == `format!("{}", rust_ty)` byte-for-byte.

#![allow(clippy::unwrap_used)]
#![allow(clippy::todo)]
#![allow(clippy::ignored_unit_patterns)]

use cobrust_types::{AdtId, AliasId, FnTy, GenericVar, Record, Ty, VarId};
use cobrust_types_cb::{display_ty, ty_cb_arena_from_rust, FnTyArena, RecordArena, TyArena};

// =====================================================================
// Helper
// =====================================================================

/// Assert `display_ty` output equals Rust `fmt::Display` for `rust_ty`.
///
/// DEV-wired per F28 + ADR-0055a Wave-2: the `ty_cb_arena_from_rust`
/// conversion populates `TyArena.fn_entries` + `TyArena.record_entries`,
/// so the standalone `FnTyArena` + `RecordArena` are passed empty.
/// `display_ty` prefers the in-arena parallel storage when present.
fn assert_display(rust_ty: &Ty) {
    let expected = format!("{rust_ty}");
    let (root_id, cb_arena) = ty_cb_arena_from_rust(rust_ty);
    let fn_arena = FnTyArena::new();
    let rec_arena = RecordArena::new();
    let actual = display_ty(&cb_arena, &fn_arena, &rec_arena, root_id);
    assert_eq!(actual, expected, "Display parity failed for {rust_ty}");
}

// =====================================================================
// dp01 — 1-tuple trailing comma
// =====================================================================

/// `(i64,)` — Rust emits trailing comma for single-element tuples.
/// If cb emits `(i64)` the test fails with a byte-mismatch.
#[test]
fn dp01_one_tuple_trailing_comma() {
    assert_display(&Ty::Tuple(vec![Ty::Int]));
}

// =====================================================================
// dp02 — FnTy named-separator
// =====================================================================

/// `(i64, a: str) -> bool` — first named param separator after positional.
///
/// Rust arm: prepends `", "` before named param if positional non-empty
/// OR if not the first named param. cb `display_ty` must mirror this.
#[test]
fn dp02_fn_ty_named_separator() {
    let fn_ty = Ty::Fn(FnTy {
        positional: vec![Ty::Int],
        named: vec![("a".to_string(), Ty::Str)],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Bool),
    });
    assert_display(&fn_ty);
}

// =====================================================================
// dp03 — `&{inner}` Ref glyph
// =====================================================================

/// `&i64` — Ref display glyph per ADR-0052a Wave-1 + ty.rs Ref arm.
#[test]
fn dp03_ref_glyph() {
    assert_display(&Ty::Ref(Box::new(Ty::Int)));
}

// =====================================================================
// dp04 — `List[{T}]` bracket glyph
// =====================================================================

/// `List[str]` — square-bracket annotation glyph.
#[test]
fn dp04_list_bracket_glyph() {
    assert_display(&Ty::List(Box::new(Ty::Str)));
}

// =====================================================================
// dp05 — `Dict[{K}, {V}]` bracket glyph
// =====================================================================

/// `Dict[i64, str]` — dict bracket + comma.
#[test]
fn dp05_dict_bracket_glyph() {
    assert_display(&Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Str)));
}

// =====================================================================
// dp06 — empty-tuple glyph
// =====================================================================

/// `()` — empty tuple; no trailing comma (only 1-tuple gets trailing comma).
#[test]
fn dp06_empty_tuple_glyph() {
    assert_display(&Ty::Tuple(vec![]));
}

// =====================================================================
// dp07 — Record type-annotation glyph `:`
// =====================================================================

/// `{name: i64, tag: bool}` — Record display with `: ` field separator.
///
/// BTreeMap ordering means fields are lexicographically sorted:
/// `name` < `tag` → `name` first.
#[test]
fn dp07_record_field_annotation_glyph() {
    let record_ty = Ty::Record(Record::from_pairs(vec![
        ("name".to_string(), Ty::Int),
        ("tag".to_string(), Ty::Bool),
    ]));
    assert_display(&record_ty);
}

// =====================================================================
// dp08 — nested Display
// =====================================================================

/// `List[List[i64]]` — two-level nesting, both bracket glyphs.
#[test]
fn dp08_nested_list_display() {
    assert_display(&Ty::List(Box::new(Ty::List(Box::new(Ty::Int)))));
}

// =====================================================================
// dp09 — `Adt#{id}` prefix glyph
// =====================================================================

/// `Adt#3[i64, str]` — AdtId prefix with two type args.
///
/// Rust arm: `write!(f, "Adt#{}", id.0)` then `[args]` if non-empty.
/// cb must emit identical prefix and bracket form.
#[test]
fn dp09_adt_prefix_glyph() {
    assert_display(&Ty::Adt(AdtId(3), vec![Ty::Int, Ty::Str]));
}

// =====================================================================
// dp10 — `Alias#`, `T{n}` Generic, `?{n}` Var glyphs
// =====================================================================

/// `Alias#2[T0, ?1]` — alias prefix + Generic `T{n}` + Var `?{n}` glyphs.
///
/// Three distinct special-case glyphs in a single type; any divergence in
/// the glyph encoding produces a byte-mismatch.
#[test]
fn dp10_alias_generic_var_glyphs() {
    assert_display(&Ty::Alias(
        AliasId(2),
        vec![Ty::Generic(GenericVar(0)), Ty::Var(VarId(1))],
    ));
}
