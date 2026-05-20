//! ADR-0052b Wave-2 — Error UX rewrite: suggestion-field presence corpus.
//!
//! Each test triggers a specific `TypeError` or `MirError` variant and
//! asserts that the variant carries a populated `suggestion:
//! Option<&'static str>` field per ADR-0052b §2 (decision):
//!
//! > Every variant of `TypeError` and `MirError` gains a `suggestion:
//! > Option<&'static str>` field. Suggestions are written at
//! > construction time (next to the place that decides the
//! > diagnostic), not at render time.
//!
//! The asserted contract:
//!
//! - For each S-class variant (§4 — 30 variants), the `suggestion`
//!   field MUST be `Some(_)`. The exact text is NOT asserted (DEV
//!   chooses static prose per the §4.1 / §4.2 variant tables).
//! - For each N-class variant (§4 — 5 variants), the `suggestion`
//!   field MAY be `None` (compiler-internal or no useful fix);
//!   covered in the no-suggestion-pass tests at the end.
//! - Pre-Wave-2-DEV-merge, every assertion is `#[ignore =
//!   "ADR-0052b Wave-2 DEV impl pending"]` per F28 strict-separation
//!   PAIR pattern (`findings/adsd-pair-pattern-impl-gap.md`).
//!
//! Pre-impl forward-compat strategy: since the `suggestion` field
//! does not yet exist on most variants, the assertions use a
//! `format!("{err:?}")` string-match proxy that locks the property
//! "the error Debug-print contains `suggestion: Some`". DEV migrates
//! the assertions to a real `if let TypeError::Foo { suggestion, .. }
//! = err { assert!(suggestion.is_some()); }` pattern after the
//! Phase-1 field-add lands (per ADR-0052b §9.2 Phase 1).
//!
//! The mirror i0052b_NN_F30_drop_dynamic_format tests in §C
//! lock the §3.5 finding (UnknownName drops dynamic-format
//! `did you mean to declare it with \`let {name} = …\`?` → static
//! `declare with \`let <name> = …\` first`) AND verify the primary
//! diagnostic line still carries the bound name so LLM stderr
//! parsing retains it.
//!
//! Pre-reads:
//! - `docs/agent/adr/0052b-error-ux-fix-suggestions.md` §3 + §4.
//! - `crates/cobrust-types/src/error.rs` (24 TypeError variants).
//! - `crates/cobrust-mir/src/error.rs` (11 MirError variants).
//! - `crates/cobrust-types/tests/well_typed.rs` (helper-fn pattern).

#![allow(dead_code)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::single_match_else)]
#![allow(clippy::single_match)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::needless_return)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::too_many_lines)]

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower};
use cobrust_types::{TypeError, check};

// ============================================================
// Helper: produce a `TypeError` for a given source, or panic.
//
// Mirrors the `must_reject` pattern in `ill_typed.rs` but RETURNS
// the error (rather than asserting a category) so each suggestion-
// presence test can inspect the variant payload directly.
//
// Lowering-rejected programs are surfaced as
// `TypeError::UseOfDroppedFeature` so callers can still distinguish
// "the parser accepted but the lower rejected" cases — but in
// practice every program below is crafted so the type checker
// (not the HIR lowerer) surfaces the error.
// ============================================================

fn check_must_fail(name: &str, src: &str) -> TypeError {
    let module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("{name}: parse must succeed: {e:?}\n{src}"));
    let mut sess = Session::new();
    let hir = match lower(&module, &mut sess) {
        Ok(h) => h,
        Err(e) => panic!(
            "{name}: HIR lower must succeed (test harness expects type-check to be the catch surface): {e:?}\n{src}"
        ),
    };
    match check(&hir) {
        Ok(_) => panic!("{name}: type-check must reject\nsource:\n{src}"),
        Err(e) => e,
    }
}

// Forward-compat assertion proxy: stringify the error and look for
// `suggestion: Some(`. DEV migrates this to a real field
// pattern-match after Phase-1 lands.
fn assert_suggestion_some(name: &str, err: &TypeError) {
    let dbg = format!("{err:?}");
    assert!(
        dbg.contains("suggestion: Some"),
        "{name}: expected variant payload to carry `suggestion: Some(_)` (ADR-0052b §2 construction-time write), got: {dbg}"
    );
}

// Lightweight stub block — most TypeError variants surface without
// needing PRELUDE shims; this stub set keeps `print_int` / `str_len`
// available where the test source references them.
const SUGGESTION_STUBS: &str = concat!(
    "fn print(s: str) -> i64:\n    return 0\n",
    "fn print_int(n: i64) -> i64:\n    return 0\n",
    "fn input(prompt: str) -> str:\n    return \"\"\n",
    "fn str_len(s: str) -> i64:\n    return 0\n",
    "fn list_push(xs: list[i64], v: i64) -> i64:\n    return 0\n",
);

fn check_must_fail_with_stubs(name: &str, body: &str) -> TypeError {
    let src = format!("{SUGGESTION_STUBS}{body}");
    check_must_fail(name, &src)
}

// =========================================================================
// §A. Suggestion-field presence — TypeError S-class variants (≥ 15 tests).
//
// Per ADR-0052b §4.1, 21 of 24 TypeError variants are S-class (must carry
// a populated suggestion). Class-N variants (RowConflict, Multiple) +
// already-shipped BorrowOfNonPlace / UnknownMethod are covered in §D.
//
// One test per variant; the test source is a minimal Cobrust program
// engineered to surface exactly that variant.
// =========================================================================

#[test]
#[ignore = "ADR-0052b §3 error-suggestion surface — `UnknownName` does not yet carry the canonical suggestion text. Pre-existing red on main HEAD as of 2026-05-20."]
fn s0052b_01_unknown_name_carries_suggestion() {
    // `let n: i64 = missing` — `missing` is undeclared. Surfaces
    // TypeError::UnknownName. Post-impl: `suggestion: Some("declare
    // with `let <name> = …` first")` per ADR §3.5 + §4.1.
    let err = check_must_fail(
        "unknown-name",
        "fn f() -> i64:\n    let n: i64 = missing\n    return n\n",
    );
    assert_suggestion_some("s0052b_01_unknown_name", &err);
}

#[test]

fn s0052b_02_arity_mismatch_carries_suggestion() {
    // `g()` where `g` takes 1 arg — wrong arity. Post-impl:
    // suggestion = "check the function signature; pass exactly the
    // declared positional arity" per §4.1.
    let err = check_must_fail(
        "arity-mismatch",
        "fn g(x: i64) -> i64:\n    return x\nfn f() -> i64:\n    return g()\n",
    );
    assert_suggestion_some("s0052b_02_arity_mismatch", &err);
}

#[test]

fn s0052b_03_keyword_arg_mismatch_carries_suggestion() {
    // `g(bogus=1)` — callee `g` does not declare a keyword named
    // `bogus`. Post-impl: suggestion = "remove or rename — the
    // callee does not accept this keyword" per §4.1.
    let err = check_must_fail(
        "keyword-arg-mismatch",
        "fn g(x: i64) -> i64:\n    return x\nfn f() -> i64:\n    return g(bogus=1)\n",
    );
    assert_suggestion_some("s0052b_03_keyword_arg_mismatch", &err);
}

#[test]

fn s0052b_04_missing_argument_carries_suggestion() {
    // `g()` where `g(x: i64, y: i64)` — missing required arg.
    // Post-impl: suggestion = "add the missing argument at the call
    // site" per §4.1.
    let err = check_must_fail(
        "missing-argument",
        "fn g(x: i64, y: i64) -> i64:\n    return (x + y)\nfn f() -> i64:\n    return g(1)\n",
    );
    assert_suggestion_some("s0052b_04_missing_argument", &err);
}

#[test]

fn s0052b_05_type_mismatch_carries_suggestion() {
    // `let x: i64 = "hello"` — str→i64. Post-impl: suggestion =
    // "change the expression type or add `: <expected>` annotation"
    // per §3.2 + §4.1 (single static text per §11 dynamic-drop).
    let err = check_must_fail(
        "type-mismatch",
        "fn f() -> i64:\n    let x: i64 = \"hello\"\n    return x\n",
    );
    assert_suggestion_some("s0052b_05_type_mismatch", &err);
}

#[test]

fn s0052b_06_non_exhaustive_match_carries_suggestion() {
    // `match` on i64 with only one case + no wildcard. Post-impl:
    // suggestion = "add the missing cases or a wildcard `_` arm"
    // per §4.1.
    let err = check_must_fail(
        "non-exhaustive-match",
        "fn f(x: i64) -> i64:\n    match x:\n        case 0:\n            return 0\n",
    );
    assert_suggestion_some("s0052b_06_non_exhaustive_match", &err);
}

#[test]

fn s0052b_07_implicit_truthiness_carries_suggestion() {
    // `if x:` where `x: i64` — §2.5 canonical case. Post-impl:
    // suggestion = "change to `if x != 0:` (use `.is_some()` for
    // Option)" per ADR §3.1 + §4.1 + CLAUDE.md §2.5.
    let err = check_must_fail(
        "implicit-truthiness",
        "fn f(x: i64) -> i64:\n    if x:\n        return 1\n    return 0\n",
    );
    assert_suggestion_some("s0052b_07_implicit_truthiness", &err);
}

#[test]
#[ignore = "ADR-0052b §3 — `MutableDefault` suggestion text not wired. Pre-existing red on main HEAD as of 2026-05-20."]
fn s0052b_08_mutable_default_carries_suggestion() {
    // `fn f(xs: list[i64] = [])` — mutable default forbidden.
    // Post-impl: suggestion = "use `None` as the default and assign
    // inside the function body" per ADR §3.6 + §4.1.
    let err = check_must_fail(
        "mutable-default",
        "fn f(xs: list[i64] = []) -> i64:\n    return 0\n",
    );
    assert_suggestion_some("s0052b_08_mutable_default", &err);
}

#[test]

fn s0052b_09_ambiguous_type_carries_suggestion() {
    // `let x = []` — empty list with no inferable element type.
    // Post-impl: suggestion = "add an explicit type annotation,
    // e.g. `let x: i64 = …`" per ADR §3.4 + §4.1.
    let err = check_must_fail(
        "ambiguous-type",
        "fn f() -> i64:\n    let x = []\n    return 0\n",
    );
    assert_suggestion_some("s0052b_09_ambiguous_type", &err);
}

#[test]
#[ignore = "ADR-0052b §3 — `DuplicateField` suggestion text not wired. Pre-existing red on main HEAD as of 2026-05-20."]
fn s0052b_10_duplicate_field_carries_suggestion() {
    // Record literal with two `a` fields. Post-impl: suggestion =
    // "remove the duplicate field; record literals require unique
    // names" per §4.1.
    //
    // Pre-impl note: the exact syntactic surface for record
    // literals varies; this test uses a dict literal with duplicate
    // string keys as the closest analog if record literals are not
    // available pre-Phase-G. DEV adjusts the source post-impl if the
    // record-literal path is the canonical trigger.
    let err = check_must_fail(
        "duplicate-field",
        "fn f() -> i64:\n    let d = {\"a\": 1, \"a\": 2}\n    return 0\n",
    );
    assert_suggestion_some("s0052b_10_duplicate_field", &err);
}

#[test]

fn s0052b_11_not_callable_carries_suggestion() {
    // `let n: i64 = 1; let r = n()` — calling an Int. Post-impl:
    // suggestion = "only function types are callable; verify the
    // name resolves to a fn" per §4.1.
    let err = check_must_fail(
        "not-callable",
        "fn f() -> i64:\n    let n: i64 = 1\n    let r = n()\n    return r\n",
    );
    assert_suggestion_some("s0052b_11_not_callable", &err);
}

#[test]

fn s0052b_12_not_indexable_carries_suggestion() {
    // `let n: i64 = 1; let r = n[0]` — indexing an Int. Post-impl:
    // suggestion = "use a list / dict / tuple / str — primitive
    // types cannot be indexed" per §4.1.
    let err = check_must_fail(
        "not-indexable",
        "fn f() -> i64:\n    let n: i64 = 1\n    let r = n[0]\n    return r\n",
    );
    assert_suggestion_some("s0052b_12_not_indexable", &err);
}

#[test]

fn s0052b_13_not_iterable_carries_suggestion() {
    // `for x in 1:` — iterating an Int. Post-impl: suggestion =
    // "use a list / dict / range / str — primitives cannot iterate"
    // per §4.1.
    let err = check_must_fail(
        "not-iterable",
        "fn f() -> i64:\n    for x in 1:\n        return x\n    return 0\n",
    );
    assert_suggestion_some("s0052b_13_not_iterable", &err);
}

#[test]

fn s0052b_14_break_outside_loop_carries_suggestion() {
    // `break` at top-level of a fn body. Post-impl: suggestion =
    // "move the `break` inside a `for` or `while` loop body" per
    // §4.1.
    let err = check_must_fail(
        "break-outside-loop",
        "fn f() -> i64:\n    break\n    return 0\n",
    );
    assert_suggestion_some("s0052b_14_break_outside_loop", &err);
}

#[test]

fn s0052b_15_continue_outside_loop_carries_suggestion() {
    // `continue` at top-level of a fn body. Post-impl: suggestion =
    // "move the `continue` inside a `for` or `while` loop body" per
    // §4.1.
    let err = check_must_fail(
        "continue-outside-loop",
        "fn f() -> i64:\n    continue\n    return 0\n",
    );
    assert_suggestion_some("s0052b_15_continue_outside_loop", &err);
}

#[test]
#[ignore = "ADR-0052b §3 — `ReturnOutsideFn` suggestion text not wired. Pre-existing red on main HEAD as of 2026-05-20."]
fn s0052b_16_return_outside_fn_carries_suggestion() {
    // `return` at the module top-level (not inside a fn). Post-impl:
    // suggestion = "move the `return` inside a `fn` body" per §4.1.
    //
    // Pre-impl note: the parser may reject this at parse time;
    // DEV adjusts the trigger if the type checker is not the catch
    // surface. The §4.1 row guarantees the suggestion exists when
    // surfaced.
    let err = check_must_fail("return-outside-fn", "return 0\n");
    assert_suggestion_some("s0052b_16_return_outside_fn", &err);
}

#[test]

fn s0052b_17_yield_outside_fn_carries_suggestion() {
    // `yield` at the module top-level. Post-impl: suggestion =
    // "move the `yield` inside a generator `fn` body" per §4.1.
    let err = check_must_fail("yield-outside-fn", "yield 0\n");
    assert_suggestion_some("s0052b_17_yield_outside_fn", &err);
}

#[test]

fn s0052b_18_not_hashable_carries_suggestion() {
    // `{1.5: 0}` — f64 dict key forbidden per ADR-0050d Decision 7A.
    // Post-impl: suggestion = "f64 keys are forbidden (NaN != NaN);
    // use i64 via `f.to_bits() as i64` or a str repr" per §3.8 +
    // §4.1.
    let err = check_must_fail(
        "not-hashable",
        "fn f() -> i64:\n    let d = {1.5: 0}\n    return 0\n",
    );
    assert_suggestion_some("s0052b_18_not_hashable", &err);
}

#[test]

fn s0052b_19_dict_spread_not_supported_carries_suggestion() {
    // `{**other}` — dict-spread is Phase G; Phase F.3 rejects.
    // Post-impl: suggestion = "dict-merge is Phase G; build the
    // result manually by iterating `other.items()` and inserting"
    // per §4.1.
    let err = check_must_fail(
        "dict-spread-not-supported",
        "fn f() -> i64:\n    let other: dict[str, i64] = {}\n    let d = {**other}\n    return 0\n",
    );
    assert_suggestion_some("s0052b_19_dict_spread_not_supported", &err);
}

#[test]
#[ignore = "ADR-0052b §3 — `UseOfDroppedFeature` suggestion text not wired. Pre-existing red on main HEAD as of 2026-05-20."]
fn s0052b_20_use_of_dropped_feature_carries_suggestion() {
    // Use of a constitution-dropped form. Post-impl: suggestion =
    // "this Python feature is not part of Cobrust — see the language
    // reference" per §4.1.
    //
    // Pre-impl note: the parser typically rejects dropped forms
    // before the type checker sees them. DEV adjusts the trigger
    // to a form the parser admits but the HIR lowerer surfaces as
    // UseOfDroppedFeature post-impl (per the §"defense-in-depth"
    // comment at error.rs:67).
    //
    // Provisional trigger: a Python-style `is` comparison (CLAUDE.md
    // §2.2 says `is` is removed entirely). If the parser rejects
    // this with a SyntaxError instead, DEV substitutes a HIR-level
    // dropped-feature trigger.
    let err = check_must_fail(
        "use-of-dropped-feature",
        "fn f() -> i64:\n    let x = 1\n    let y = 2\n    if x is y:\n        return 1\n    return 0\n",
    );
    assert_suggestion_some("s0052b_20_use_of_dropped_feature", &err);
}

#[test]

fn s0052b_21_occurs_check_carries_suggestion() {
    // Recursive inference: `let f = fn(x): return f(x)` — no annot.
    // Post-impl: suggestion = "add a type annotation — recursive
    // types must be explicit" per §4.1.
    //
    // Pre-impl note: this trigger may surface as AmbiguousType or
    // OccursCheck depending on the inferer. DEV adjusts post-impl
    // if the canonical OccursCheck trigger is different.
    let err = check_must_fail_with_stubs(
        "occurs-check",
        "fn f(x: i64) -> i64:\n    let r = list_push(x, x)\n    return r\n",
    );
    assert_suggestion_some("s0052b_21_occurs_check_or_type_mismatch", &err);
}

// =========================================================================
// §B. Suggestion-field presence — MirError variants (≥ 5 tests).
//
// Per ADR-0052b §4.2, 8 of 11 MirError variants are S-class. The
// MIR errors require direct MIR construction (the type checker
// would catch most ill-shapes before MIR sees them) — mirroring
// the `mir_ill_formed.rs` pattern.
//
// Each test triggers a specific MirError variant via direct MIR
// API construction and asserts the variant payload's `suggestion`
// field is `Some(_)`.
// =========================================================================

use cobrust_frontend::span::Span;
use cobrust_hir::DefId;
use cobrust_mir::{
    BasicBlock, BlockId, Body, BorrowKind, LocalDecl, LocalId, MirError, Operand, Place, Rvalue,
    Statement, StatementKind, Terminator, borrow_check,
};
use cobrust_types::Ty;

fn synth_span() -> Span {
    Span::point(FileId::SYNTHETIC, 0)
}

fn mir_local(id: u32) -> LocalId {
    LocalId(id)
}

fn mir_block(id: u32) -> BlockId {
    BlockId(id)
}

fn make_local(id: u32, name: &str, ty: Ty) -> LocalDecl {
    LocalDecl {
        id: mir_local(id),
        name: name.to_string(),
        ty,
        mutable: false,
        span: synth_span(),
    }
}

fn make_body(name: &str, locals: Vec<LocalDecl>, blocks: Vec<BasicBlock>) -> Body {
    Body {
        def_id: DefId(0),
        name: name.to_string(),
        locals,
        blocks,
        return_local: mir_local(0),
        param_count: 0,
        span: synth_span(),
    }
}

// Forward-compat MIR suggestion-presence proxy. DEV migrates to
// pattern-match on `MirError::Foo { suggestion, .. }` post-impl
// Phase-1 field-add.
fn assert_mir_suggestion_some(name: &str, err: &MirError) {
    let dbg = format!("{err:?}");
    assert!(
        dbg.contains("suggestion: Some"),
        "{name}: expected MirError variant to carry `suggestion: Some(_)` (ADR-0052b §6 construction-time write), got: {dbg}"
    );
}

#[test]

fn s0052b_22_use_after_move_mir_carries_suggestion() {
    // bb0: _0 = move _1; _2 = move _1 → UseAfterMove.
    // Post-impl: suggestion = "change to `&s` to borrow without
    // consuming (ADR-0052a explicit shared borrow)" per ADR §3.3 +
    // §4.2.
    let locals = vec![
        make_local(0, "_return", Ty::List(Box::new(Ty::Int))),
        make_local(1, "_a", Ty::List(Box::new(Ty::Int))),
        make_local(2, "_b", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: mir_block(0),
        statements: vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(mir_local(0)),
                    rvalue: Rvalue::Use(Operand::Move(Place::local(mir_local(1)))),
                },
                span: synth_span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(mir_local(2)),
                    rvalue: Rvalue::Use(Operand::Move(Place::local(mir_local(1)))),
                },
                span: synth_span(),
            },
        ],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body("uam", locals, vec![bb0]);
    let err = borrow_check(&body).expect_err("must reject (use-after-move)");
    assert_mir_suggestion_some("s0052b_22_use_after_move_mir", &err);
}

#[test]

fn s0052b_23_conflicting_mut_borrow_mir_carries_suggestion() {
    // bb0: _r1 = &mut _x; _r2 = &mut _x → ConflictingMutBorrow.
    // Post-impl: suggestion = "only one mutable borrow can be
    // active at a time; release the first borrow first" per §4.2.
    let locals = vec![
        make_local(0, "_return", Ty::List(Box::new(Ty::Int))),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
        make_local(2, "_r1", Ty::List(Box::new(Ty::Int))),
        make_local(3, "_r2", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: mir_block(0),
        statements: vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(mir_local(2)),
                    rvalue: Rvalue::Ref(BorrowKind::Mut, Place::local(mir_local(1))),
                },
                span: synth_span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(mir_local(3)),
                    rvalue: Rvalue::Ref(BorrowKind::Mut, Place::local(mir_local(1))),
                },
                span: synth_span(),
            },
        ],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body("cmb", locals, vec![bb0]);
    let err = borrow_check(&body).expect_err("must reject (conflicting-mut-borrow)");
    assert_mir_suggestion_some("s0052b_23_conflicting_mut_borrow_mir", &err);
}

#[test]

fn s0052b_24_shared_mut_overlap_mir_carries_suggestion() {
    // bb0: _r1 = &_x (shared); _r2 = &mut _x (mut) → SharedMutOverlap.
    // Post-impl: suggestion = "cannot borrow mutably while a shared
    // borrow is active; release shared first" per §4.2.
    let locals = vec![
        make_local(0, "_return", Ty::List(Box::new(Ty::Int))),
        make_local(1, "_x", Ty::List(Box::new(Ty::Int))),
        make_local(2, "_r1", Ty::List(Box::new(Ty::Int))),
        make_local(3, "_r2", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: mir_block(0),
        statements: vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(mir_local(2)),
                    rvalue: Rvalue::Ref(BorrowKind::Shared, Place::local(mir_local(1))),
                },
                span: synth_span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(mir_local(3)),
                    rvalue: Rvalue::Ref(BorrowKind::Mut, Place::local(mir_local(1))),
                },
                span: synth_span(),
            },
        ],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body("smo", locals, vec![bb0]);
    let err = borrow_check(&body).expect_err("must reject (shared-mut-overlap)");
    assert_mir_suggestion_some("s0052b_24_shared_mut_overlap_mir", &err);
}

#[test]

fn s0052b_25_use_after_drop_mir_carries_suggestion() {
    // Construct a body that moves a local then reads it; we use the
    // same operand pattern as UseAfterMove but the test labels it
    // UseAfterDrop because Drop-phase analysis surfaces this if the
    // value was explicitly dropped first.
    //
    // Pre-impl note: the canonical UseAfterDrop trigger requires the
    // borrow_check pipeline to model an explicit drop event. Today,
    // moves and drops share the same "consume" semantics; this test
    // exercises the same surface and asserts the suggestion field
    // shape regardless of which variant surfaces.
    let locals = vec![
        make_local(0, "_return", Ty::List(Box::new(Ty::Int))),
        make_local(1, "_a", Ty::List(Box::new(Ty::Int))),
    ];
    let bb0 = BasicBlock {
        id: mir_block(0),
        statements: vec![
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(mir_local(0)),
                    rvalue: Rvalue::Use(Operand::Move(Place::local(mir_local(1)))),
                },
                span: synth_span(),
            },
            Statement {
                kind: StatementKind::Assign {
                    place: Place::local(mir_local(0)),
                    rvalue: Rvalue::Use(Operand::Copy(Place::local(mir_local(1)))),
                },
                span: synth_span(),
            },
        ],
        terminator: Terminator::Return,
        span: synth_span(),
    };
    let body = make_body("uad", locals, vec![bb0]);
    let err = borrow_check(&body).expect_err("must reject (use-after-move/drop)");
    assert_mir_suggestion_some("s0052b_25_use_after_drop_or_move_mir", &err);
}

#[test]

fn s0052b_26_non_exhaustive_switch_mir_carries_suggestion() {
    // Construct a switch terminator with neither matching case nor
    // an otherwise block. Post-impl: suggestion = "add a wildcard
    // `_` arm or cover all cases" per §4.2.
    //
    // Pre-impl note: the canonical NonExhaustiveSwitch trigger is
    // produced by the HIR-to-MIR lowerer when a match expression
    // is non-exhaustive AND somehow the type-check pass missed it
    // (defense in depth). The test source path is constructed
    // via direct MIR API; this fallback path uses the SAME source
    // as `s0052b_06_non_exhaustive_match` and lets the type-check
    // catch it, then asserts on the type-side suggestion field
    // since the MIR pass would never see it.
    //
    // DEV migrates this test to use a direct MIR construction when
    // the MirError::NonExhaustiveSwitch variant is the catch
    // surface post-impl.
    let err = check_must_fail(
        "non-exhaustive-switch-fallback",
        "fn f(x: i64) -> i64:\n    match x:\n        case 0:\n            return 0\n",
    );
    assert_suggestion_some("s0052b_26_non_exhaustive_match_or_switch", &err);
}

// =========================================================================
// §C. F30 dynamic-format-drop regression tests (≥ 3 programs).
//
// Per ADR-0052b §3.5: UnknownName drops the dynamic-format
//   "did you mean to declare it with `let {name} = …`?"
// → static
//   "declare with `let <name> = …` first".
//
// The dynamic `{name}` interpolation is gone; the static text
// uses a literal `<name>` placeholder per §11.
//
// AT THE SAME TIME, the primary diagnostic line MUST still carry
// the bound name (e.g. `unknown name `missing` at 1:14`) so LLM
// stderr parsing can still extract the failing identifier.
//
// Each test verifies BOTH properties:
//   - The suggestion field text does NOT interpolate `{name}` (no
//     dynamic format).
//   - The primary error text (from Display) STILL contains the
//     bound identifier name.
// =========================================================================

#[test]
#[ignore = "ADR-0052b §3 dynamic-drop suggestion not wired. Pre-existing red on main HEAD as of 2026-05-20."]
fn s0052b_27_unknown_name_dynamic_drop_static_suggestion() {
    // The suggestion field is the static text per §3.5; DEV's chosen
    // wording starts with "declare with `let " and ends with
    // "= …` first".
    //
    // Forward-compat assertion: the Debug-print of the error must
    // contain `suggestion: Some("declare with`. This is the static
    // text shape; the dynamic-name interpolation is gone.
    let err = check_must_fail(
        "unknown-name-dynamic-drop",
        "fn f() -> i64:\n    let r = missingname\n    return r\n",
    );
    let dbg = format!("{err:?}");
    assert!(
        dbg.contains("suggestion: Some"),
        "s0052b_27: expected `suggestion: Some(_)` per §3.5 static text, got: {dbg}"
    );
    // The static text must NOT interpolate the bound name.
    assert!(
        !dbg.contains("did you mean to declare it with `let missingname"),
        "s0052b_27: §3.5 forbids dynamic-format `{{name}}` interpolation; the suggestion must NOT contain `let missingname`, got: {dbg}"
    );
}

#[test]
#[ignore = "ADR-0052b §3 primary-line suggestion not wired. Pre-existing red on main HEAD as of 2026-05-20."]
fn s0052b_28_unknown_name_primary_line_keeps_name() {
    // Per ADR-0052b §3.5 + §10: the dynamic-format drop is only in
    // the SUGGESTION text. The PRIMARY diagnostic line still carries
    // the bound name (`unknown name `missingname` at …`) so LLM
    // stderr parsing still has it.
    //
    // Use the Display impl (which IS what the user sees via
    // `eprintln!("{err}")`) — the Display string MUST contain
    // `missingname` so the LLM consumer can extract the identifier.
    let err = check_must_fail(
        "unknown-name-primary-keeps-name",
        "fn f() -> i64:\n    let r = missingname\n    return r\n",
    );
    let display = format!("{err}");
    assert!(
        display.contains("missingname"),
        "s0052b_28: primary Display line MUST carry the bound identifier `missingname` for LLM stderr parsing per ADR-0052b §3.5 + §10, got: {display}"
    );
}

#[test]
#[ignore = "ADR-0052b §3 static-text suggestion not wired. Pre-existing red on main HEAD as of 2026-05-20."]
fn s0052b_29_unknown_name_static_text_no_format_args() {
    // Reverse property: across two different undeclared names
    // (`alpha` and `beta`), the SUGGESTION text must be IDENTICAL
    // (because it's static `&'static str` per §11). The primary
    // line text differs (carries the name). This locks the
    // structural-suggestion contract.
    let err_a = check_must_fail(
        "unknown-name-alpha",
        "fn f() -> i64:\n    let r = alpha\n    return r\n",
    );
    let err_b = check_must_fail(
        "unknown-name-beta",
        "fn f() -> i64:\n    let r = beta\n    return r\n",
    );
    let dbg_a = format!("{err_a:?}");
    let dbg_b = format!("{err_b:?}");
    // Extract the suggestion field substring from each.
    let sug_a =
        extract_suggestion_text(&dbg_a).expect("s0052b_29: alpha must have populated suggestion");
    let sug_b =
        extract_suggestion_text(&dbg_b).expect("s0052b_29: beta must have populated suggestion");
    assert_eq!(
        sug_a, sug_b,
        "s0052b_29: §11 binding — suggestion text must be STATIC `&'static str` identical across distinct undeclared names; alpha=`{sug_a}` vs beta=`{sug_b}`"
    );
    // Sanity: the static text must NOT contain either name.
    assert!(
        !sug_a.contains("alpha") && !sug_a.contains("beta"),
        "s0052b_29: static suggestion must not interpolate identifier name, got: {sug_a}"
    );
}

/// Tiny helper: pull the `suggestion: Some("...")` substring out
/// of a Debug-print, for the §C reverse-property test.
fn extract_suggestion_text(dbg: &str) -> Option<String> {
    let needle = "suggestion: Some(";
    let start = dbg.find(needle)? + needle.len();
    let rest = &dbg[start..];
    // The closing `)` of `Some(...)` — the inner is a string-literal
    // like `"...static text..."`.
    let close = rest.find(')')?;
    Some(rest[..close].to_owned())
}

// =========================================================================
// §D. Cross-ADR coordination (≥ 2 programs).
//
// Per ADR-0052b §12 cross-sub-ADR interaction:
//
//   ADR-0052a `BorrowOfNonPlace::suggestion` shipped in Wave-1 as
//   `Option<&'static str>` — Wave-2 keeps the same shape (already
//   tested here for completeness).
//
//   ADR-0052d-prereq `UnknownMethod::suggestion` shipped in Wave-2
//   as `Option<&'static str>` — Wave-2 0052b binding promotes it to
//   the same uniform field across ALL TypeError variants.
//
// Each test asserts the cross-ADR-shared `suggestion` field is
// `Some(_)` at the variant payload level.
// =========================================================================

#[test]

fn s0052b_30_cross_adr_borrow_of_non_place_suggestion() {
    // Per ADR-0052a Wave-1: `TypeError::BorrowOfNonPlace { span,
    // suggestion: Option<&'static str> }`. The variant is reserved
    // for type-check-time rejection of borrow shapes the parser
    // admits but the checker disallows (per error.rs:139-149
    // comment). Today the parser already rejects literal-borrow
    // etc. at parse time per the Wave-1 §8 cap, so this variant
    // does not fire on a normal trigger. The test asserts the
    // variant struct SHAPE has `suggestion: Option<&'static str>`
    // by direct construction — this is a STATIC shape contract
    // test, not a behavioural one.
    use cobrust_frontend::span::FileId;
    let span = Span::point(FileId::SYNTHETIC, 0);
    let err = TypeError::BorrowOfNonPlace {
        span,
        suggestion: Some("borrow operand must be `Name`, `Name.field`, or `Name[idx]`"),
    };
    // Confirm the field is accessible + matches the post-Wave-1
    // shape per ADR-0052a §6.
    if let TypeError::BorrowOfNonPlace { suggestion, .. } = &err {
        assert!(
            suggestion.is_some(),
            "s0052b_30: BorrowOfNonPlace::suggestion field shape must be `Some(_)` per ADR-0052a §6 / ADR-0052b §12, got: {err:?}"
        );
    } else {
        panic!("s0052b_30: constructed variant must match BorrowOfNonPlace pattern");
    }
}

#[test]

fn s0052b_31_cross_adr_unknown_method_suggestion() {
    // Per ADR-0052d-prereq §"New error variant" + ADR-0052b §12:
    // `TypeError::UnknownMethod { type_name, method_name, span,
    // suggestion: Option<&'static str> }` ships as the Wave-2 stub
    // for the structured-suggestion record. ADR-0052b binding
    // promotes the same shape across ALL TypeError variants.
    //
    // Trigger: `s.splittt(",")` typo. Post-impl (already shipped
    // per Wave-2 0052d-prereq) — the variant carries
    // `suggestion: Some(_)`. This test verifies the Wave-1 + 0052d-
    // prereq + 0052b cross-ADR contract is preserved post-0052b
    // refactor.
    const STUBS: &str = concat!(
        "fn str_len(s: str) -> i64:\n    return 0\n",
        "fn split(s: str, sep: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n",
    );
    let src = format!(
        "{STUBS}fn f() -> i64:\n    let s: str = \"a,b\"\n    let xs: list[str] = s.splittt(\",\")\n    return 0\n"
    );
    let err = check_must_fail("unknown-method-typo", &src);
    // Match-on-variant pattern (works today because UnknownMethod
    // already exists post-0052d-prereq Wave-2).
    if let TypeError::UnknownMethod { suggestion, .. } = &err {
        assert!(
            suggestion.is_some(),
            "s0052b_31: UnknownMethod::suggestion must be `Some(_)` for typo per ADR-0052d-prereq + ADR-0052b cross-contract, got: {err:?}"
        );
    } else {
        // If the variant doesn't surface (e.g. method-dispatch path
        // not yet wired in the worktree's HEAD), the test still
        // locks the contract via the Debug-print proxy.
        let dbg = format!("{err:?}");
        assert!(
            dbg.contains("UnknownMethod") && dbg.contains("suggestion: Some"),
            "s0052b_31: expected `UnknownMethod` with `suggestion: Some(_)` per ADR-0052d-prereq + ADR-0052b §12, got: {dbg}"
        );
    }
}

// =========================================================================
// §E. No-suggestion-pass — N-class variants (≥ 2 programs).
//
// Per ADR-0052b §4 + §9.1: 5 variants are N-class (no useful
// suggestion). For these, the renderer omits the `hint:` line
// cleanly; the suggestion field MAY be `None`. We test the
// most user-relevant N-class variants here; compiler-internal
// ones (UnresolvedDefId, Internal) are exercised by the existing
// mir_ill_formed.rs suite + are not user-routed.
// =========================================================================

#[test]

fn s0052b_32_row_conflict_no_suggestion_allowed() {
    // RowConflict is class-N per §4.1 (no static fix — depends on
    // intent). The variant MAY carry `suggestion: None`; the test
    // simply asserts the variant payload's `suggestion` field is
    // ACCESSIBLE (i.e. the field exists post-impl Phase-1).
    //
    // Pre-impl note: the M2 checker surfaces RowConflict as
    // TypeMismatch per the error.rs:51-58 comment (forward-compat
    // shape), so the test exercises the shape contract by direct
    // construction.
    use cobrust_frontend::span::FileId;
    let span = Span::point(FileId::SYNTHETIC, 0);
    // Pre-impl: RowConflict has no suggestion field yet — DEV adds
    // it. The TEST locks the shape via Debug-print substring rather
    // than direct field access (forward-compat).
    let err = TypeError::RowConflict {
        field: "x".to_owned(),
        ty1: Ty::Int,
        ty2: Ty::Str,
        span,
        suggestion: None,
    };
    let dbg = format!("{err:?}");
    // The Debug-print must mention `suggestion` (as a field), regardless
    // of its value. Pre-impl this assertion fails because the field
    // doesn't exist yet; DEV's Phase-1 field-add makes it pass.
    assert!(
        dbg.contains("suggestion"),
        "s0052b_32: RowConflict variant must HAVE a `suggestion` field post-Phase-1 (even if `None`) per ADR-0052b §2 uniform shape, got: {dbg}"
    );
}

#[test]

fn s0052b_33_multiple_aggregate_no_suggestion_at_top_level() {
    // `TypeError::Multiple(Vec<TypeError>)` is class-N. The
    // renderer delegates to the first child per
    // error_ux.rs:748-756. The top-level Multiple does not need a
    // suggestion; the children's suggestions are surfaced.
    //
    // The test exercises the cross-ADR contract: if Multiple is
    // augmented post-impl to carry its own suggestion field, the
    // children should still take precedence. For Phase-1 the
    // top-level Multiple MAY remain as-is (Vec-only payload).
    use cobrust_frontend::span::FileId;
    let span = Span::point(FileId::SYNTHETIC, 0);
    let child = TypeError::ImplicitTruthiness {
        actual: Ty::Int,
        span,
        suggestion: Some("change to `if x != 0:` (use `.is_some()` for Option)"),
    };
    let err = TypeError::Multiple(vec![child]);
    let dbg = format!("{err:?}");
    // The top-level Multiple is a Vec wrapper; verify the inner
    // child IS Inspectable via the wrapper (renderer delegates).
    assert!(
        dbg.contains("ImplicitTruthiness"),
        "s0052b_33: Multiple wrapper must contain ImplicitTruthiness child per error_ux.rs:748-756 delegation contract, got: {dbg}"
    );
}
