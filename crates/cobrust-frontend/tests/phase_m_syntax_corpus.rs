//! Phase M ADR-0060a + ADR-0060b parser syntax corpus.
//!
//! Covers the three syntax-gap closures introduced by ADR-0060b:
//!
//! - `pm_b01_..b03` — `-> None` return type (gap #3).
//! - `pm_b04_..b09` — `&T` in type-annotation position (gap #5).
//! - `pm_b10_..b14` — `[T; N]` fixed-size array type (gap #4).
//!
//! Plus narrow-int parse smoke for ADR-0060a (the parser change is
//! purely a named-type lookup, so a single positive smoke suffices —
//! the deeper test lives in `cobrust-types/tests`).
//!
//! F34 anchor: `phase_m_syntax_corpus::pm_b01_none_return_type`.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::expect_used)]

use cobrust_frontend::{parse_str, span::FileId};

fn ok(src: &str) {
    parse_str(src, FileId::SYNTHETIC).expect("phase-m must parse");
}

fn rejects(src: &str) {
    let res = parse_str(src, FileId::SYNTHETIC);
    assert!(res.is_err(), "phase-m must reject: {src}");
}

// =====================================================================
// ADR-0060b §3.1 — `-> None` return type (gap #3).
// =====================================================================

/// F34: phase_m_syntax_corpus::pm_b01_none_return_type
#[test]
fn pm_b01_none_return_type() {
    // Explicit `-> None` resolves to Ty::None via lower_named_type.
    ok("fn f() -> None:\n    pass\n");
}

#[test]
fn pm_b02_none_param_type() {
    // `None` as parameter type (rare but parser-legal; useful in
    // dependent-type signatures that just consume Unit).
    ok("fn f(x: None) -> i64:\n    return 0\n");
}

#[test]
fn pm_b03_none_in_tuple() {
    // Composition: `(i64, None)` tuple type passes through.
    ok("fn f() -> (i64, None):\n    return (0, None)\n");
}

// =====================================================================
// ADR-0060b §3.2 — `&T` in type-annotation position (gap #5).
// =====================================================================

#[test]
fn pm_b04_ref_str_param() {
    // The dominant ADR-0051 Priority A use case: `&str` parameter
    // shape lets the type signature itself encode the borrow contract
    // rather than relying on call-site `&` ergonomics.
    ok("fn f(s: &str) -> i64:\n    return 0\n");
}

#[test]
fn pm_b05_ref_int_param() {
    // `&i64` — Ref(Int); transparent at codegen.
    ok("fn f(p: &i64) -> i64:\n    return 0\n");
}

#[test]
fn pm_b06_ref_generic_inner() {
    // `&list[i64]` — Ref wrapping a Generic application.
    ok("fn f(xs: &list[i64]) -> i64:\n    return 0\n");
}

#[test]
fn pm_b07_ref_in_return_type() {
    // `-> &i64` return type. Legal at parse; type-check rejects
    // returning a borrow longer than its scope (separate lifetime
    // story per ADR-0052a Wave-1).
    ok("fn f(x: i64) -> &i64:\n    return &x\n");
}

#[test]
fn pm_b08_ref_in_tuple() {
    // `(&i64, &str)` tuple of borrows.
    ok("fn f(a: &i64, b: &str) -> i64:\n    return 0\n");
}

#[test]
fn pm_b09_double_ref_parses() {
    // `&&i64` parses at wave-2 but fails at use-site type-check
    // (no nested-Ref call-site coercion). The parser MUST accept it
    // to let typeck produce the better error message downstream.
    ok("fn f(p: &&i64) -> i64:\n    return 0\n");
}

// =====================================================================
// ADR-0060b §3.3 — `[T; N]` fixed-size array type (gap #4).
// =====================================================================

#[test]
fn pm_b10_array_i64_4() {
    // Positive smoke: `[i64; 4]` parameter annotation.
    ok("fn first(a: [i64; 4]) -> i64:\n    return a[0]\n");
}

#[test]
fn pm_b11_array_str_0() {
    // Zero-length arrays are parser-legal at wave-2.
    ok("fn empty() -> i64:\n    return 0\n");
    ok("fn f(a: [str; 0]) -> i64:\n    return 0\n");
}

#[test]
fn pm_b12_array_nested_in_tuple() {
    // `(i64, [i64; 2])` — composition with tuple.
    ok("fn f(p: (i64, [i64; 2])) -> i64:\n    return 0\n");
}

#[test]
fn pm_b13_array_rejects_non_int_length() {
    // `[i64; n]` — non-literal length rejected at wave-2.
    rejects("fn f(a: [i64; n]) -> i64:\n    return 0\n");
}

#[test]
fn pm_b14_array_rejects_missing_semicolon() {
    // `[i64 4]` — missing semicolon rejected with concrete suggestion.
    rejects("fn f(a: [i64 4]) -> i64:\n    return 0\n");
}

// =====================================================================
// ADR-0060a — narrow-int parse smoke (deeper typeck tests in types crate).
// =====================================================================

#[test]
fn pm_a01_i32_param_parses() {
    // `i32` is a named type at parser level; resolved to Ty::IntN(32)
    // by cobrust-types::check::lower_named_type. Parser just needs the
    // identifier path to land.
    ok("fn f(x: i32) -> i32:\n    return x\n");
}

#[test]
fn pm_a02_i8_param_parses() {
    ok("fn f(x: i8) -> i8:\n    return x\n");
}

#[test]
fn pm_a03_i16_param_parses() {
    // Phase M includes i16 as a future-proof named type per
    // ADR-0060a §3.2 ({8, 16, 32} width set).
    ok("fn f(x: i16) -> i16:\n    return x\n");
}
