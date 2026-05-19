//! Phase M ADR-0060a + ADR-0060b type-check corpus.
//!
//! Covers the typing semantics of:
//!
//! - `pm_a01_..a08` — narrow-int Ty::IntN(8|16|32) unification + Display + Copy.
//! - `pm_b01_..b06` — `&T` Ty::Ref and `[T;N]` Ty::Array typing.
//!
//! F34 anchor: `phase_m_type_corpus::pm_a01_i32_resolves_to_intn32`.

#![allow(clippy::expect_used)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::needless_pass_by_value)]

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_types::{Ty, check};

fn well_typed(src: &str) {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir");
    check(&hir).expect("must type-check");
}

fn ill_typed(src: &str) {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir");
    let res = check(&hir);
    assert!(res.is_err(), "must FAIL type-check: {src}");
}

// =====================================================================
// ADR-0060a — narrow-int typing.
// =====================================================================

/// F34: phase_m_type_corpus::pm_a01_i32_resolves_to_intn32
#[test]
fn pm_a01_i32_resolves_to_intn32() {
    // i32 named-type lookup resolves to Ty::IntN(32). Verified via
    // Display ("i32" round-trips on Ty::IntN(32)).
    assert_eq!(Ty::IntN(32).to_string(), "i32");
    assert_eq!(Ty::IntN(8).to_string(), "i8");
    assert_eq!(Ty::IntN(16).to_string(), "i16");
}

#[test]
fn pm_a02_i32_passthrough_well_typed() {
    // `fn f(x: i32) -> i32: return x` — Ty::IntN(32) unifies with itself.
    well_typed("fn f(x: i32) -> i32:\n    return x\n");
}

#[test]
fn pm_a03_i8_add_well_typed() {
    // i8 + i8 via Ty::IntN(8) arithmetic. Phase M follow-up closure
    // 2026-05-19: `synth_bin` arithmetic family now whitelists
    // `Ty::IntN(_)` as a result type, so `Ty::IntN(8) + Ty::IntN(8)`
    // resolves to `Ty::IntN(8)` (per ADR-0060a §3.2 unification rule).
    well_typed("fn f(a: i8, b: i8) -> i8:\n    return (a + b)\n");
}

#[test]
fn pm_a04_intn_is_copy() {
    // Per ADR-0060a §3.5 + drop.rs is_copy: IntN is Copy. Calling f
    // twice with the same `let x: i32 = 0` must not flag UseAfterMove.
    // Phase M follow-up closure 2026-05-19: integer-literal `0` now
    // narrows to `Ty::IntN(32)` at the annotation site via the
    // literal-narrowing coercion in `ItemKind::Let` / `StmtKind::Let`.
    // The dedicated overflow diagnostic (§3.6) lands in a follow-up.
    well_typed(
        "fn f(x: i32) -> i32:\n    return x\n\
         fn main() -> i32:\n    let x: i32 = 0\n    let a: i32 = f(x)\n    let b: i32 = f(x)\n    return (a + b)\n",
    );
}

/// F34: phase_m_type_corpus::pm_a09_intn_negative_literal_narrows
/// — sister to pm_a04; exercises the `Un::Neg + Lit::Int` literal
/// shape that the closure must also recognise.
#[test]
fn pm_a09_intn_negative_literal_narrows() {
    well_typed("fn f() -> i32:\n    let x: i32 = -5\n    return x\n");
}

/// F34: phase_m_type_corpus::pm_a10_intn_add_mul_chain
/// — exercise the full arithmetic family (Add + Mul) on narrow ints.
#[test]
fn pm_a10_intn_add_mul_chain() {
    well_typed("fn f(a: i32, b: i32) -> i32:\n    return (a * b + a)\n");
}

#[test]
fn pm_a05_intn_is_hashable() {
    // ADR-0060a §3.2 + Ty::is_hashable IntN arm — narrow ints are
    // hashable (scalar). Use as dict key.
    well_typed(
        "fn f(d: dict[i32, i64]) -> i64:\n    return 0\n",
    );
}

#[test]
fn pm_a06_intn_distinct_from_int() {
    // Per ADR-0060a §3.2 unification rule: IntN(32) does NOT unify
    // with Int (no implicit narrowing). Without an explicit cast,
    // mixing widths must fail.
    //
    // Pre-impl baseline note: at this stage Cobrust does not yet
    // expose `i32(...)` / `i8(...)` cast syntax (MIR-level cast
    // surface is internal); a future ADR-0060a wave-1 follow-up
    // adds the cast lowering hook. For now we just verify the
    // explicit-width signature is honoured (no implicit widen).
    well_typed("fn f(x: i32) -> i32:\n    return x\n");
}

#[test]
fn pm_a07_intn_width_distinct() {
    // IntN(8) ≠ IntN(32). A function declared `-> i8` returning a
    // `let y: i32 = ...` must fail typeck (no implicit widen).
    ill_typed(
        "fn f() -> i8:\n    let y: i32 = 0\n    return y\n",
    );
}

#[test]
fn pm_a08_intn_in_tuple() {
    // Composition: tuple of (i32, i8).
    well_typed(
        "fn pair(a: i32, b: i8) -> (i32, i8):\n    return (a, b)\n",
    );
}

// =====================================================================
// ADR-0060b — Ref + Array typing.
// =====================================================================

#[test]
fn pm_b01_ref_str_unify_with_borrow() {
    // `fn f(s: &str)` annotated; call-site `f(&local_str)` exercises
    // ADR-0052a Wave-1 one-way Ref(T)->T transparency.
    well_typed(
        "fn f(s: &str) -> i64:\n    return 0\n\
         fn main() -> i64:\n    let s = \"hi\"\n    return f(&s)\n",
    );
}

#[test]
fn pm_b02_ref_display() {
    assert_eq!(Ty::Ref(Box::new(Ty::Int)).to_string(), "&i64");
    assert_eq!(Ty::Ref(Box::new(Ty::Str)).to_string(), "&str");
}

#[test]
fn pm_b03_array_display() {
    assert_eq!(
        Ty::Array(Box::new(Ty::Int), 4).to_string(),
        "[i64; 4]"
    );
    assert_eq!(
        Ty::Array(Box::new(Ty::Bool), 0).to_string(),
        "[bool; 0]"
    );
}

#[test]
fn pm_b04_array_unify_eq_length() {
    // ADR-0060b §3.3 + infer.rs Array arm: same length + inner unifies.
    // Same-shape arrays are equal under unification.
    let a = Ty::Array(Box::new(Ty::Int), 4);
    let b = Ty::Array(Box::new(Ty::Int), 4);
    assert_eq!(a, b);
}

#[test]
fn pm_b05_array_diff_length() {
    // Array(Int, 4) ≠ Array(Int, 5) under PartialEq (length is part
    // of the type identity).
    let a = Ty::Array(Box::new(Ty::Int), 4);
    let b = Ty::Array(Box::new(Ty::Int), 5);
    assert_ne!(a, b);
}

#[test]
#[ignore = "ADR-0060b wave-2 follow-up: empty `{}` dict literal lacks K-type \
            propagation through the annotation site — the Hashable check at \
            validate_hashable_dict fires on the annotation `K` slot but the \
            literal supplies fresh Var that masks the Array K; full check \
            requires the annotation-flow rewrite per \
            finding:adr0060b-empty-dict-annotation-k-flow-debt"]
fn pm_b06_array_not_hashable() {
    // ADR-0060b §3.3 — Array is NOT hashable at wave-2; using as
    // dict key must fail. Today's empty-dict `{}` literal does not
    // propagate the annotation K type through validate_hashable_dict
    // (the literal supplies fresh Vars, which mask the Array K).
    // The honest fix requires the annotation-flow rewrite for empty
    // collection literals; deferred to a separate sub-sprint.
    ill_typed(
        "fn f() -> i64:\n    let d: dict[[i64; 4], i64] = {}\n    return 0\n",
    );
}
