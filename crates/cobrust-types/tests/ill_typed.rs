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
//! Curated ill-typed program suite — ≥ 50 programs the type checker
//! must reject with the right error category.
//!
//! Each test names the expected `TypeError` discriminant. The suite
//! is deliberately structured by error category — adding a new
//! variant to `TypeError` should come with at least one test here.

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower};
use cobrust_types::{TypeError, check};

#[derive(Clone, Copy, Debug)]
enum Cat {
    TypeMismatch,
    ImplicitTruthiness,
    NotCallable,
    NotIndexable,
    NotIterable,
    ArityMismatch,
    KeywordArgMismatch,
    NonExhaustiveMatch,
    BreakOutsideLoop,
    ContinueOutsideLoop,
    ReturnOutsideFn,
    YieldOutsideFn,
    AmbiguousType,
    MutableDefault,
    UnknownName,
    /// ADR-0052d-prereq §"New error variant" — `Cat::UnknownMethod`
    /// pairs with `TypeError::UnknownMethod`. TEST author documented
    /// the placeholder swap (`Cat::UnknownName` → `Cat::UnknownMethod`)
    /// in i0052dpre_01..06 inline comments; DEV graduates per F28.
    UnknownMethod,
    /// ADR-0052g Wave-2 round 2 — `Cat::BorrowOfNonPlace` pairs with
    /// `TypeError::BorrowOfNonPlace`. Used by i0052g_* tests that
    /// assert `&recv.method()` (non-Copy return) + `&free_fn(...)`
    /// rejections per ADR-0052g §4.2-§4.3.
    BorrowOfNonPlace,
    /// ADR-0080 Phase-1a — `Cat::UnknownField` pairs with
    /// `TypeError::UnknownField`. Used by i153/i154: accessing an
    /// undeclared field on a field-tracked class instance is a
    /// compile-time error (with a §2.5-B FIX listing the declared
    /// fields), NOT a silent `fresh_var()`.
    UnknownField,
    /// ADR-0080 Phase-1b-ii — `Cat::UnsupportedRefinement` pairs with
    /// `TypeError::UnsupportedRefinement`. Used by i157: a class field
    /// `where`-clause outside the fixed int-range grammar (Q6) is a
    /// compile-time error with a §2.5-B FIX naming the accepted forms.
    UnsupportedRefinement,
    /// ADR-0080 Phase-1b-ii — `Cat::CallbackSignatureMismatch` pairs with
    /// `TypeError::CallbackSignatureMismatch`. Used by i158/i159: a
    /// `route_validated` handler with the wrong arity (1-arg) or a non-class
    /// 2nd param is a callback-shape mismatch with a §2.5-B FIX.
    CallbackSignatureMismatch,
    /// ADR-0088 §3 — `Cat::LenArgNotSized` pairs with
    /// `TypeError::LenArgNotSized`. Used by i170/i171: the Python-canonical
    /// `len(x)` free-function on a NON-sized arg (`len(5)` / `len(3.0)`) is a
    /// compile-time error whose §2.5-B FIX names the accepted sized types
    /// (str / list / dict) — NOT the misleading "expected Dict".
    LenArgNotSized,
}

fn matches_cat(err: &TypeError, cat: Cat) -> bool {
    match (cat, err) {
        (Cat::TypeMismatch, TypeError::TypeMismatch { .. }) => true,
        (Cat::ImplicitTruthiness, TypeError::ImplicitTruthiness { .. }) => true,
        (Cat::NotCallable, TypeError::NotCallable { .. }) => true,
        (Cat::NotIndexable, TypeError::NotIndexable { .. }) => true,
        (Cat::NotIterable, TypeError::NotIterable { .. }) => true,
        (Cat::ArityMismatch, TypeError::ArityMismatch { .. }) => true,
        (Cat::KeywordArgMismatch, TypeError::KeywordArgMismatch { .. }) => true,
        (Cat::NonExhaustiveMatch, TypeError::NonExhaustiveMatch { .. }) => true,
        (Cat::BreakOutsideLoop, TypeError::BreakOutsideLoop { .. }) => true,
        (Cat::ContinueOutsideLoop, TypeError::ContinueOutsideLoop { .. }) => true,
        (Cat::ReturnOutsideFn, TypeError::ReturnOutsideFn { .. }) => true,
        (Cat::YieldOutsideFn, TypeError::YieldOutsideFn { .. }) => true,
        (Cat::AmbiguousType, TypeError::AmbiguousType { .. }) => true,
        (Cat::MutableDefault, TypeError::MutableDefault { .. }) => true,
        (Cat::UnknownName, TypeError::UnknownName { .. }) => true,
        (Cat::UnknownMethod, TypeError::UnknownMethod { .. }) => true,
        (Cat::BorrowOfNonPlace, TypeError::BorrowOfNonPlace { .. }) => true,
        (Cat::UnknownField, TypeError::UnknownField { .. }) => true,
        (Cat::UnsupportedRefinement, TypeError::UnsupportedRefinement { .. }) => true,
        (Cat::CallbackSignatureMismatch, TypeError::CallbackSignatureMismatch { .. }) => true,
        (Cat::LenArgNotSized, TypeError::LenArgNotSized { .. }) => true,
        _ => false,
    }
}

fn must_reject(name: &str, src: &str, cat: Cat) {
    let module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("{name}: parse failed (test snippet must parse): {e:?}\n{src}"));
    let mut sess = Session::new();
    match lower(&module, &mut sess) {
        Err(_e) => return, // lowering caught it (defense in depth) — accept as rejection
        Ok(hir) => match check(&hir) {
            Ok(_) => panic!("{name}: must reject but passed type check\nsource:\n{src}"),
            Err(e) => assert!(
                matches_cat(&e, cat),
                "{}: rejected with wrong category\n  expected: {:?}\n  got:      {:?}\n  source:\n{}",
                name,
                cat,
                e,
                src
            ),
        },
    }
}

/// Like [`must_reject`] but ALSO asserts the rejection's RENDERED message
/// (`TypeError`'s `Display`, i.e. `error.rs`'s `#[error(...)]`) contains every
/// `needle`. For §2.5 FIX-text guarantees the error CATEGORY alone is
/// insufficient — the message must STEER the author to a valid form. Pins the
/// canonical Display content so it cannot silently regress (the 2026-05-30
/// audit proved a category-only check stays green when the Display is gutted —
/// an F36 fixture-name-vs-behavior gap). Mutation-verified for #161.
fn must_reject_with_msg(name: &str, src: &str, cat: Cat, needles: &[&str]) {
    let module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("{name}: parse failed (snippet must parse): {e:?}\n{src}"));
    let mut sess = Session::new();
    match lower(&module, &mut sess) {
        Err(e) => panic!(
            "{name}: lowering caught it, but a message-text assertion needs the check-stage \
             TypeError: {e:?}\n{src}"
        ),
        Ok(hir) => match check(&hir) {
            Ok(_) => panic!("{name}: must reject but passed type check\nsource:\n{src}"),
            Err(e) => {
                assert!(
                    matches_cat(&e, cat),
                    "{name}: wrong category\n  expected: {cat:?}\n  got: {e:?}\n{src}"
                );
                let msg = e.to_string();
                for needle in needles {
                    assert!(
                        msg.contains(needle),
                        "{name}: §2.5 FIX-text must contain {needle:?}; got:\n{msg}"
                    );
                }
            }
        },
    }
}

// ============================================================
// Implicit truthiness
// ============================================================

#[test]
fn i01_if_int_cond() {
    must_reject(
        "if-int-cond",
        "fn f(x: i64) -> i64:\n    if x:\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i02_while_int_cond() {
    must_reject(
        "while-int-cond",
        "fn f(x: i64) -> i64:\n    while x:\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i03_not_int() {
    must_reject(
        "not-int",
        "fn f(x: i64) -> bool:\n    return (not x)\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i04_and_int() {
    must_reject(
        "and-int",
        "fn f(a: i64, b: bool) -> bool:\n    return (a and b)\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i05_or_str() {
    must_reject(
        "or-str",
        "fn f(a: str, b: bool) -> bool:\n    return (a or b)\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i06_if_list_cond() {
    must_reject(
        "if-list",
        "fn f(xs: List[i64]) -> i64:\n    if xs:\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

// ============================================================
// Type mismatch (no silent coercion)
// ============================================================

#[test]
fn i07_int_plus_str() {
    must_reject(
        "int-plus-str",
        "fn f(x: i64) -> i64:\n    return (x + \"1\")\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i08_str_plus_int() {
    must_reject(
        "str-plus-int",
        "fn f(s: str) -> str:\n    return (s + 1)\n",
        Cat::TypeMismatch,
    );
}

// F85 §2.5-A / §5.1 — `str <op> str` for a NON-`+` arithmetic op
// (`-` / `*` / `/` / `%`) must be REJECTED at type-check (a clean
// `TypeMismatch`), NOT slip through into codegen where it PANICKED the
// compiler. `Str + Str` (concat) and `Str * Int` (repeat) remain valid
// (covered by well_typed + str_mul_e2e); these four are the unsupported
// shapes. CPython 3: all four are `TypeError`s. The §2.5-B fix-printing
// HINT (the `suggestion`) is asserted at the CLI layer (str_mul_e2e_07),
// since `TypeError`'s `Display` omits the suggestion (rendered by the
// CLI's error_ux). Here we pin the type-check REJECT + its category.

#[test]
fn i08a_str_minus_str() {
    must_reject(
        "str-minus-str",
        "fn f(s: str, t: str) -> str:\n    return (s - t)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i08b_str_times_str() {
    must_reject(
        "str-times-str",
        "fn f(s: str, t: str) -> str:\n    return (s * t)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i08c_str_div_str() {
    must_reject(
        "str-div-str",
        "fn f(s: str, t: str) -> str:\n    return (s / t)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i08d_str_mod_str() {
    must_reject(
        "str-mod-str",
        "fn f(s: str, t: str) -> str:\n    return (s % t)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i09_bool_plus_int() {
    must_reject(
        "bool-plus-int",
        "fn f(p: bool) -> i64:\n    return (p + 1)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i10_mixed_int_float_arith() {
    must_reject(
        "mixed-int-float",
        "fn f(a: i64, b: f64) -> f64:\n    return (a + b)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i11_assign_wrong_type() {
    must_reject(
        "let-annot-wrong",
        "fn f() -> i64:\n    let x: i64 = \"hi\"\n    return x\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i12_return_wrong_type() {
    must_reject(
        "return-wrong",
        "fn f() -> i64:\n    return \"hi\"\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i13_list_mixed_elements() {
    must_reject(
        "list-mixed",
        "fn f() -> List[i64]:\n    return [1, \"x\"]\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i14_dict_mixed_value() {
    must_reject(
        "dict-mixed-value",
        "fn f() -> Dict[str, i64]:\n    return {\"k\": 1, \"v\": \"x\"}\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i15_int_eq_str() {
    must_reject(
        "int-eq-str",
        "fn f(a: i64) -> bool:\n    return (a == \"x\")\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i16_assign_to_let_wrong_type() {
    must_reject(
        "assign-wrong",
        "fn f() -> i64:\n    let x: i64 = 0\n    x = \"hi\"\n    return x\n",
        Cat::TypeMismatch,
    );
}

// ============================================================
// Calls — arity / keyword / not-callable
// ============================================================

#[test]
fn i17_arity_too_many() {
    must_reject(
        "arity-too-many",
        "fn g(x: i64) -> i64:\n    return x\nfn f() -> i64:\n    return g(1, 2)\n",
        Cat::ArityMismatch,
    );
}

#[test]
fn i18_arity_too_few() {
    must_reject(
        "arity-too-few",
        "fn g(x: i64, y: i64) -> i64:\n    return (x + y)\nfn f() -> i64:\n    return g(1)\n",
        Cat::ArityMismatch,
    );
}

#[test]
fn i19_keyword_unknown() {
    must_reject(
        "kw-unknown",
        "fn g(*, x: i64) -> i64:\n    return x\nfn f() -> i64:\n    return g(unknown=1)\n",
        Cat::KeywordArgMismatch,
    );
}

#[test]
fn i20_not_callable_int() {
    must_reject(
        "not-callable",
        "fn f(x: i64) -> i64:\n    return x(0)\n",
        Cat::NotCallable,
    );
}

#[test]
fn i21_call_string_literal() {
    must_reject(
        "call-string",
        "fn f() -> i64:\n    return \"x\"(0)\n",
        Cat::NotCallable,
    );
}

// ============================================================
// Indexing / iteration
// ============================================================

#[test]
fn i22_index_int() {
    must_reject(
        "index-int",
        "fn f(x: i64) -> i64:\n    return x[0]\n",
        Cat::NotIndexable,
    );
}

#[test]
fn i23_index_bool() {
    must_reject(
        "index-bool",
        "fn f(p: bool) -> i64:\n    return p[0]\n",
        Cat::NotIndexable,
    );
}

#[test]
fn i24_iter_int() {
    must_reject(
        "iter-int",
        "fn f(x: i64) -> i64:\n    for v in x:\n        return v\n    return 0\n",
        Cat::NotIterable,
    );
}

#[test]
fn i25_iter_bool() {
    must_reject(
        "iter-bool",
        "fn f(p: bool) -> i64:\n    for v in p:\n        return 1\n    return 0\n",
        Cat::NotIterable,
    );
}

#[test]
fn i26_dict_index_wrong_key() {
    must_reject(
        "dict-wrong-key",
        "fn f(d: Dict[str, i64]) -> i64:\n    return d[1]\n",
        Cat::TypeMismatch,
    );
}

// ============================================================
// Match exhaustiveness
// ============================================================

#[test]
fn i27_match_bool_only_true() {
    must_reject(
        "match-bool-only-true",
        "fn f(p: bool) -> i64:\n    match p:\n        case True:\n            return 1\n",
        Cat::NonExhaustiveMatch,
    );
}

#[test]
fn i28_match_bool_only_false() {
    must_reject(
        "match-bool-only-false",
        "fn f(p: bool) -> i64:\n    match p:\n        case False:\n            return 0\n",
        Cat::NonExhaustiveMatch,
    );
}

// ============================================================
// Flow misuse
// ============================================================

#[test]
fn i29_break_outside_loop() {
    must_reject(
        "break-outside",
        "fn f() -> i64:\n    break\n    return 0\n",
        Cat::BreakOutsideLoop,
    );
}

#[test]
fn i30_continue_outside_loop() {
    must_reject(
        "continue-outside",
        "fn f() -> i64:\n    continue\n    return 0\n",
        Cat::ContinueOutsideLoop,
    );
}

#[test]
fn i31_class_method_return_wrong_type() {
    must_reject(
        "class-method-wrong",
        "class C:\n    fn m() -> i64:\n        return \"x\"\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i32_yield_in_module_pre_check() {
    // module-level yield isn't a `Stmt::Yield`; it's an expr-stmt.
    // Lowering accepts; type checker rejects with YieldOutsideFn.
    must_reject("yield-module", "yield 1\n", Cat::YieldOutsideFn);
}

// ============================================================
// Mutable default arguments
// ============================================================

// Note: ADR-0003 already rejects non-literal defaults at parse time.
// At type-check time, even literal-sized lists become TypeMismatch
// because the parser refuses to take them as defaults. So we
// explicitly do not test "mutable default" via list literal — the
// parser already gates it. Instead exercise the rule via a default
// whose lowered HIR-literal type is mutable: the AST literal grammar
// only admits scalar literals, so this category is automatically
// satisfied by construction. We retain a placeholder smoke test.
#[test]
fn i33_mutable_default_smoke() {
    // An empty body fn with a literal default — accepted (no
    // mutable container at literal level). The actual mutable-default
    // pathway runs at the HIR step but cannot be reached from valid
    // surface syntax (parser blocks it). Defense-in-depth verified
    // by the unit test in `well_typed::w36_fstring` etc. surviving.
    let src = "fn f(x: i64 = 0) -> i64:\n    return x\n";
    let module = parse_str(src, FileId::SYNTHETIC).unwrap();
    let mut sess = Session::new();
    let hir = lower(&module, &mut sess).unwrap();
    check(&hir).unwrap_or_else(|e| panic!("scalar default must accept: {e:?}"));
}

// ============================================================
// Inference / ambiguity
// ============================================================

#[test]
fn i34_lambda_no_annotation_call_used() {
    // Without an annotation and the lambda's parameter is never
    // constrained by use, inference cannot pick a type.
    must_reject(
        "ambiguous",
        "fn f() -> i64:\n    let g = lambda x: x\n    return 0\n",
        Cat::AmbiguousType,
    );
}

#[test]
fn i35_empty_list_no_use() {
    must_reject(
        "empty-list",
        "fn f() -> i64:\n    let xs = []\n    return 0\n",
        Cat::AmbiguousType,
    );
}

// ============================================================
// Misc structural mismatches
// ============================================================

#[test]
fn i36_tuple_arity_let() {
    must_reject(
        "tuple-arity-let",
        "fn f() -> i64:\n    let (a, b) = (1, 2, 3)\n    return a\n",
        Cat::ArityMismatch,
    );
}

#[test]
fn i37_let_pattern_type() {
    must_reject(
        "let-pattern",
        "fn f() -> i64:\n    let (a, b) = 0\n    return a\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i38_dict_value_key_swap() {
    must_reject(
        "dict-key-swap",
        "fn f() -> Dict[i64, str]:\n    return {\"a\": \"b\"}\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i39_list_int_to_set_str() {
    must_reject(
        "list-set-mismatch",
        "fn f() -> List[str]:\n    return [1, 2, 3]\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i40_set_int_to_dict() {
    must_reject(
        "set-to-dict",
        "fn f() -> Dict[str, i64]:\n    return {1, 2}\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i41_neg_bool() {
    must_reject(
        "neg-bool",
        "fn f(p: bool) -> bool:\n    return (-p)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i42_bitnot_str() {
    must_reject(
        "bitnot-str",
        "fn f(s: str) -> i64:\n    return (~s)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i43_shift_str() {
    must_reject(
        "shift-str",
        "fn f(s: str) -> str:\n    return (s << 2)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i44_str_div_int() {
    must_reject(
        "str-div-int",
        "fn f(s: str) -> str:\n    return (s / 1)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i45_bool_lt_int() {
    must_reject(
        "bool-lt-int",
        "fn f(p: bool, x: i64) -> bool:\n    return (p < x)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i46_int_in_int() {
    must_reject(
        "int-in-int",
        "fn f(a: i64, b: i64) -> bool:\n    return (a in b)\n",
        Cat::NotIterable,
    );
}

// ============================================================
// Closure / scoping defenses
// ============================================================

#[test]
fn i47_use_undefined_via_assign() {
    // Lowering catches this (UnknownName) — `must_reject` accepts
    // either lowering or type-check rejection.
    must_reject(
        "use-undefined",
        "fn f() -> i64:\n    return undefined\n",
        Cat::UnknownName,
    );
}

#[test]
fn i48_call_let_with_wrong_type() {
    must_reject(
        "let-call-wrong",
        "fn g(x: i64) -> i64:\n    return x\nfn f() -> i64:\n    return g(\"x\")\n",
        Cat::TypeMismatch,
    );
}

// ============================================================
// More truthiness / coercion
// ============================================================

#[test]
fn i49_if_str() {
    must_reject(
        "if-str",
        "fn f(s: str) -> i64:\n    if s:\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i50_if_dict() {
    must_reject(
        "if-dict",
        "fn f(d: Dict[str, i64]) -> i64:\n    if d:\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i51_if_tuple() {
    must_reject(
        "if-tuple",
        "fn f() -> i64:\n    if (1, 2):\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i52_match_int_no_wildcard() {
    must_reject(
        "match-int-no-wildcard",
        "fn f(x: i64) -> i64:\n    match x:\n        case 0:\n            return 0\n",
        Cat::NonExhaustiveMatch,
    );
}

#[test]
fn i53_match_str_no_wildcard() {
    must_reject(
        "match-str-no-wildcard",
        "fn f(s: str) -> i64:\n    match s:\n        case \"a\":\n            return 0\n",
        Cat::NonExhaustiveMatch,
    );
}

#[test]
fn i54_seq_pattern_arity() {
    must_reject(
        "seq-arity",
        "fn f() -> i64:\n    let (a, b, c) = (1, 2)\n    return a\n",
        Cat::ArityMismatch,
    );
}

// ============================================================
// M-F.3.1 for-loop ill-typed corpus (ADR-0050b)
//
// Iter-source classifier rejects non-iterable expressions:
//   - int, bool, float, str (str-iter is Phase G alongside iter
//     protocol per ADR-0050b §"Iter source type checking")
//   - calls returning non-list/dict/set types
//
// Loop-var typing: rebinding inside body to wrong type is a
// regular `TypeMismatch`; not specific to for-loops.
// ============================================================

#[test]
fn i55_for_iter_str_now_accepted_f88() {
    // F88 / ADR-0101 (§2.5 LLM-first) — `for c in <str>:` codepoint
    // iteration is NOW ACCEPTED (was Phase-G-deferred per ADR-0050b
    // §"Iter source type checking", a clean reject — never a silent
    // miscompile). `iter_element(Ty::Str) -> Ty::Str` binds each `c`
    // to a fresh 1-codepoint owned `str` (CPython semantics).
    let src = "fn f() -> i64:\n    for c in \"hello\":\n        return 0\n    return 0\n";
    let module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("i55: parse failed: {e:?}\n{src}"));
    let mut sess = Session::new();
    let hir = lower(&module, &mut sess).expect("i55: lowering must succeed");
    check(&hir).expect("i55: `for c in <str>:` must now type-check (F88)");
}

#[test]
fn i56_for_iter_float() {
    // f64 lands in Wave 2; here it's an unknown name + the iter
    // source isn't a list. Cover the iter side specifically by
    // calling a fn that returns i64 then iterating it.
    must_reject(
        "for-iter-i64-call",
        "fn g() -> i64:\n    return 42\nfn f() -> i64:\n    for v in g():\n        return v\n    return 0\n",
        Cat::NotIterable,
    );
}

#[test]
fn i57_for_iter_unknown_name() {
    must_reject(
        "for-iter-unknown",
        "fn f() -> i64:\n    for v in undefined_iter:\n        return 0\n    return 0\n",
        Cat::UnknownName,
    );
}

#[test]
fn i58_for_range_called_with_one_arg_now_accepted() {
    // ADR-0089 §4 — the 1-arg `range(stop)` form is now VALID
    // (`range(5) == range(0, 5)`), reversing the pre-ADR-0089 arity
    // rejection this test originally asserted. The `try_synth_range_builtin`
    // special-case injects `start = 0`. (Kept in the ill_typed corpus as a
    // behaviour-change marker; it now type-checks cleanly.)
    let src = "fn range(a: i64, b: i64) -> List[i64]:\n    let xs: List[i64] = []\n    return xs\nfn f() -> i64:\n    for i in range(5):\n        return i\n    return 0\n";
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = lower(&module, &mut sess).expect("lower");
    check(&hir).expect("ADR-0089: range(5) must now type-check (== range(0, 5))");
}

#[test]
fn i59_for_range_called_with_three_args() {
    // 3-arg range_step is deferred to Phase G per ADR-0050b.
    must_reject(
        "for-range-arity-3",
        "fn range(a: i64, b: i64) -> List[i64]:\n    let xs: List[i64] = []\n    return xs\nfn f() -> i64:\n    for i in range(0, 10, 2):\n        return i\n    return 0\n",
        Cat::ArityMismatch,
    );
}

#[test]
fn i60_for_range_with_str_args() {
    // range expects i64 args.
    must_reject(
        "for-range-str-args",
        "fn range(a: i64, b: i64) -> List[i64]:\n    let xs: List[i64] = []\n    return xs\nfn f() -> i64:\n    for i in range(\"a\", \"b\"):\n        return i\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i61_for_var_rebind_wrong_type() {
    // Reassigning loop-var inside body to a string is a type-mismatch
    // because var is bound to i64 (range element type).
    must_reject(
        "for-range-rebind-wrong",
        "fn range(a: i64, b: i64) -> List[i64]:\n    let xs: List[i64] = []\n    return xs\nfn f() -> i64:\n    for i in range(0, 5):\n        i = \"oops\"\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i62_for_iter_tuple_heterogeneous() {
    // Heterogeneous tuple isn't iterable (per existing iter_element).
    must_reject(
        "for-iter-tuple-hetero",
        "fn f() -> i64:\n    let t = (1, \"two\")\n    for v in t:\n        return 0\n    return 0\n",
        Cat::NotIterable,
    );
}

// ============================================================
// M-F.3.3 — f64 ill-typed corpus (i63..i92)
// Targets: implicit coercion rejections, illegal cast types, wrong
// argument types to math functions, and IEEE 754 misuse patterns.
//
// Constitution §2.2 (non-negotiable):
//   "Silent coercion (`"1" + 1`, `0 == False`, truthiness of arbitrary
//    types) → type error"
//   No implicit i64 ↔ f64; explicit `as` cast required.
// ============================================================

// ---- Implicit coercion — rejected ----

#[test]
fn i63_implicit_i64_to_f64_assign() {
    // `let x: f64 = 1` — implicit i64 literal → f64 must be rejected.
    // Constitution §2.2: no silent coercion.
    must_reject(
        "implicit-i64-to-f64",
        "fn f() -> f64:\n    let x: f64 = 1\n    return x\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i64_implicit_f64_to_i64_assign() {
    // `let x: i64 = 1.0` — implicit f64 literal → i64 must be rejected.
    must_reject(
        "implicit-f64-to-i64",
        "fn f() -> i64:\n    let x: i64 = 1.0\n    return x\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i65_implicit_i64_to_f64_return() {
    // Returning i64 from f64-typed function is a type mismatch.
    must_reject(
        "implicit-return-i64-as-f64",
        "fn f(n: i64) -> f64:\n    return n\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i66_implicit_f64_to_i64_return() {
    // Returning f64 from i64-typed function is a type mismatch.
    must_reject(
        "implicit-return-f64-as-i64",
        "fn f(v: f64) -> i64:\n    return v\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i67_mixed_int_float_add_is_rejected() {
    // `i64 + f64` is a type mismatch; already exercised by i10 but
    // this variant tests the assignment context.
    must_reject(
        "add-int-float-assign",
        "fn f(n: i64, x: f64) -> f64:\n    let r: f64 = (n + x)\n    return r\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i68_mixed_float_int_mul_is_rejected() {
    // `f64 * i64` ordering variant.
    must_reject(
        "mul-float-int",
        "fn f(x: f64, n: i64) -> f64:\n    return (x * n)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i69_implicit_int_to_float_fn_arg() {
    // Passing an i64 where f64 is expected (no implicit coerce in call).
    must_reject(
        "arg-int-to-float",
        "fn g(x: f64) -> f64:\n    return x\nfn f(n: i64) -> f64:\n    return g(n)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i70_implicit_float_to_int_fn_arg() {
    // Passing f64 where i64 is expected.
    must_reject(
        "arg-float-to-int",
        "fn g(x: i64) -> i64:\n    return x\nfn f(v: f64) -> i64:\n    return g(v)\n",
        Cat::TypeMismatch,
    );
}

// ---- `as` cast invalid types (M-F.3.3 gap item a — ill-typed side) ----
// NOTE: After the DEV agent adds `x as T` expression syntax, the
// type-checker must reject these cases. Until the DEV lands, these
// will fail at the PARSER level (the `must_reject` helper panics on
// parse failure). That is the correct "failing" state for a TDD corpus —
// both the parse gap and the future type-check gap are surfaced.
//
// The DEV agent must:
//   1. Add parser support for `x as T`.
//   2. Add type-check rule: `as` only valid for i64↔f64 and bool↔i64;
//      casting str → f64 is a TypeError::TypeMismatch (no such cast).

#[test]
fn i71_cast_str_to_f64_rejected() {
    // `"hello" as f64` — str is not castable to float; must be TypeError.
    must_reject(
        "cast-str-to-f64",
        "fn f() -> f64:\n    return (\"hello\" as f64)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i72_cast_bool_to_f64_rejected() {
    // `True as f64` — bool → f64 cast not supported (only bool → i64).
    must_reject(
        "cast-bool-to-f64",
        "fn f() -> f64:\n    return (True as f64)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i73_cast_str_to_i64_rejected() {
    // `"42" as i64` — no str→i64 cast; use `parse_int` for parsing.
    must_reject(
        "cast-str-to-i64",
        "fn f() -> i64:\n    return (\"42\" as i64)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i74_cast_f64_to_str_rejected() {
    // `3.14 as str` — no numeric → str cast; use f-string formatting.
    must_reject(
        "cast-f64-to-str",
        "fn f() -> str:\n    return (3.14 as str)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i75_cast_i64_to_str_rejected() {
    // `42 as str` — no i64 → str cast.
    must_reject(
        "cast-i64-to-str",
        "fn f() -> str:\n    return (42 as str)\n",
        Cat::TypeMismatch,
    );
}

// ---- Math function argument type mismatches ----
// NOTE: These stub the math functions inline so the type checker
// exercises its own constraint propagation, not the PRELUDE.
// Once the PRELUDE ships, the inline stubs can be removed and the
// tests will still exercise the same type-check path via built-ins.

#[test]
fn i76_sqrt_with_int_arg_rejected() {
    // `sqrt(n: i64)` where `sqrt` expects f64 — type mismatch.
    must_reject(
        "sqrt-int-arg",
        "fn sqrt(x: f64) -> f64:\n    return x\nfn f(n: i64) -> f64:\n    return sqrt(n)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i77_pow_second_arg_int_rejected() {
    // `pow(x: f64, n: i64)` — second arg must be f64.
    must_reject(
        "pow-second-arg-int",
        "fn pow(base: f64, exp: f64) -> f64:\n    return base\nfn f(b: f64, n: i64) -> f64:\n    return pow(b, n)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i78_floor_with_str_arg_rejected() {
    // `floor("hello")` — str is not a valid argument to floor.
    must_reject(
        "floor-str-arg",
        "fn floor(x: f64) -> f64:\n    return x\nfn f() -> f64:\n    return floor(\"hello\")\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i79_abs_with_bool_arg_rejected() {
    // `abs(True)` — bool is not valid for abs(f64).
    must_reject(
        "abs-bool-arg",
        "fn abs(x: f64) -> f64:\n    return x\nfn f() -> f64:\n    return abs(True)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i80_min_heterogeneous_args_rejected() {
    // `min(1.0, 2)` — heterogeneous arg types; second arg is i64 not f64.
    must_reject(
        "min-hetero-args",
        "fn min(a: f64, b: f64) -> f64:\n    return a\nfn f() -> f64:\n    return min(1.0, 2)\n",
        Cat::TypeMismatch,
    );
}

// ---- f64 truthiness / implicit bool (constitution §2.2) ----

#[test]
fn i81_float_in_if_condition_rejected() {
    // `if x:` where x: f64 — ImplicitTruthiness; §2.2 "if x requires x: bool".
    must_reject(
        "float-if-cond",
        "fn f(x: f64) -> i64:\n    if x:\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i82_float_in_while_condition_rejected() {
    // `while x:` where x: f64 — same ImplicitTruthiness rule.
    must_reject(
        "float-while-cond",
        "fn f(x: f64) -> i64:\n    while x:\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

// ---- f64 comparison result used in arithmetic (type chain) ----

#[test]
fn i83_cmp_result_used_as_float_rejected() {
    // `(a < b) + 1.0` — bool + f64 is a type mismatch.
    must_reject(
        "cmp-result-plus-float",
        "fn f(a: f64, b: f64) -> f64:\n    return ((a < b) + 1.0)\n",
        Cat::TypeMismatch,
    );
}

// ---- f64 in bit-ops (must reject — bit ops are int-only) ----

#[test]
fn i84_float_bitand_rejected() {
    // `x & y` where x, y: f64 — bitwise ops are i64-only.
    must_reject(
        "float-bitand",
        "fn f(x: f64, y: f64) -> i64:\n    return (x & y)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i85_float_bitor_rejected() {
    // `x | y` where x, y: f64.
    must_reject(
        "float-bitor",
        "fn f(x: f64, y: f64) -> i64:\n    return (x | y)\n",
        Cat::TypeMismatch,
    );
}

// ---- Annotated return type mismatch with f64 expression ----

#[test]
fn i86_f64_expr_returned_as_i64() {
    // Addition of two f64 literals returned as i64.
    must_reject(
        "f64-add-returned-as-i64",
        "fn f() -> i64:\n    return (1.0 + 2.0)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i87_i64_expr_returned_as_f64() {
    // Addition of two i64 literals returned as f64 (no implicit coerce).
    must_reject(
        "i64-add-returned-as-f64",
        "fn f() -> f64:\n    return (1 + 2)\n",
        Cat::TypeMismatch,
    );
}

// ---- f64 as list element type mismatch ----

#[test]
fn i88_list_i64_pushed_with_f64() {
    // Assigning f64 into a List[i64] slot — type mismatch.
    must_reject(
        "list-i64-assign-f64",
        "fn f() -> i64:\n    let xs: List[i64] = [1, 2, 3]\n    let x: i64 = 1.5\n    return x\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i89_list_f64_get_annotated_as_i64() {
    // Annotating a List[f64] element retrieval as i64.
    must_reject(
        "list-f64-as-i64",
        "fn f() -> i64:\n    let xs: List[f64] = [1.0, 2.0]\n    let x: i64 = xs[0]\n    return x\n",
        Cat::TypeMismatch,
    );
}

// ---- f64 mod operator type-check ----

#[test]
fn i90_float_mod_with_int_rejected() {
    // `x % n` where x: f64, n: i64 — operand types must match.
    must_reject(
        "float-mod-int",
        "fn f(x: f64, n: i64) -> f64:\n    return (x % n)\n",
        Cat::TypeMismatch,
    );
}

// ---- Tuple/record containing f64 — wrong field type ----

#[test]
fn i91_f64_fn_result_annotated_as_i64() {
    // A function returning f64 whose result is annotated as i64 — type mismatch.
    // (Replaces the tuple-float variant that requires tuple-float-literal parse
    // support which is deferred. This exercises the same "f64 used in i64 binding"
    // path without needing float literals in tuple context.)
    must_reject(
        "f64-fn-result-as-i64",
        "fn get_float(x: f64) -> f64:\n    return x\nfn f(v: f64) -> i64:\n    let x: i64 = get_float(v)\n    return x\n",
        Cat::TypeMismatch,
    );
}

// ---- inf / nan as identifier (reserved) ----
// NOTE: After DEV adds `inf`/`nan` as f64 prelude constants, using them
// as variable names should remain valid (they are names, not keywords).
// But assigning a non-f64 value to a variable named `inf` that is
// declared as f64 is still a type mismatch.

#[test]
fn i92_assign_int_to_f64_named_inf_binding() {
    // Declaring `let x: f64 = 1` (int literal, not inf) — type mismatch.
    // This is another variant of i63 testing the f64 annotation path.
    must_reject(
        "int-to-f64-binding",
        "fn f() -> f64:\n    let result: f64 = 42\n    return result\n",
        Cat::TypeMismatch,
    );
}

// ============================================================
// M-F.3.2 — list[str] ownership ill-typed corpus (i93..i104)
// Closes TD-1 per ADR-0050c Option A. The type checker must REJECT:
//   - element-type heterogeneity in list[str] literals
//   - silent Str→i64 / Str→bool coercion when reading list[str][i]
//   - mutable default argument with list[str] type (constitution §2.2,
//     ADR-0050c §"list[str] knock-on (audit Finding 1.3 carry-forward)")
//   - assigning list[i64] to list[str] binding (and vice versa)
//   - implicit truthy/falsy on list[str] (must use list_is_empty)
//   - iterating non-iterable in str-targeted for-loops
//
// Each test cites the ADR-0050c §"Consequences" or constitution §2.2
// clause it locks.
// ============================================================

// ---- Tier B.1: literal element-type mismatch ----

#[test]
fn i93_list_str_literal_with_int_elem_rejected() {
    // `let xs: list[str] = [1, 2, 3]` — annotation says str but
    // literal elements are i64. Head + unify rejects.
    must_reject(
        "list-str-literal-int-elem",
        "fn f() -> i64:\n    let xs: list[str] = [1, 2, 3]\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i94_list_str_mixed_literal_rejected() {
    // `["a", 1]` — head-element ("a": str), tail-element (1: i64);
    // unify rejects.
    must_reject(
        "list-str-mixed-literal",
        "fn f() -> i64:\n    let xs: list[str] = [\"a\", 1]\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i95_list_str_literal_with_bool_elem_rejected() {
    // `let xs: list[str] = [True, False]` — annotation/literal mismatch.
    must_reject(
        "list-str-literal-bool-elem",
        "fn f() -> i64:\n    let xs: list[str] = [True, False]\n    return 0\n",
        Cat::TypeMismatch,
    );
}

// ---- Tier B.2: Str→i64 / Str→bool implicit coercion rejected ----

#[test]
fn i96_list_str_index_assigned_to_i64_rejected() {
    // `let y: i64 = xs[0]` where xs: list[str] — Str→i64 silent
    // coercion rejected per constitution §2.2.
    must_reject(
        "list-str-index-as-i64",
        "fn f() -> i64:\n    let xs: list[str] = [\"a\", \"b\"]\n    let y: i64 = xs[0]\n    return y\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i97_list_str_index_in_bool_condition_rejected() {
    // `if xs[0]:` — xs[0]: str, not bool. Constitution §2.2 forbids
    // implicit truthy/falsy on str.
    must_reject(
        "list-str-index-in-if",
        "fn f() -> i64:\n    let xs: list[str] = [\"a\"]\n    if xs[0]:\n        return 0\n    return 1\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i98_list_str_used_directly_in_if_condition_rejected() {
    // `if xs:` — xs: list[str], not bool. Constitution §2.2 forbids
    // implicit truthy/falsy on collections; users must call
    // `list_is_empty(xs)` (which returns bool).
    must_reject(
        "list-str-bare-if-cond",
        "fn f() -> i64:\n    let xs: list[str] = [\"a\"]\n    if xs:\n        return 0\n    return 1\n",
        Cat::ImplicitTruthiness,
    );
}

// ---- Tier B.3: list[i64] / list[str] mutual incompatibility ----

#[test]
fn i99_list_i64_assigned_to_list_str_binding_rejected() {
    // `let xs: list[str] = [1, 2]` — synth `[1, 2]` to list[i64],
    // unify with list[str] annotation rejects.
    must_reject(
        "list-i64-to-list-str-binding",
        "fn f() -> i64:\n    let xs: list[str] = [1, 2]\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i100_list_str_passed_to_list_i64_param_rejected() {
    // `fn count_i(xs: list[i64]) -> i64; count_i(list[str])` — arg
    // type mismatch.
    must_reject(
        "list-str-to-list-i64-arg",
        "fn count_i(xs: list[i64]) -> i64:\n    return list_len(xs)\nfn main() -> i64:\n    let ys: list[str] = [\"a\"]\n    return count_i(ys)\n",
        Cat::TypeMismatch,
    );
}

// ---- Tier B.4: mutable default argument with list[str]
// (audit Finding 1.3 carry-forward; ADR-0050c §"list[str] knock-on") ----

#[test]
fn i101_mutable_default_arg_list_str_rejected() {
    // `fn f(xs: list[str] = []) -> i64:` — constitution §2.2 forbids
    // mutable default arguments. ADR-0050c §"list[str] knock-on" binds
    // this as `MutableDefaultArgument` at fn declaration site (forward-
    // looking for when the default-arg surface widens to non-Lit
    // expressions, ADR-0036 candidate / Phase F.4+).
    //
    // At HEAD the parser rejects this earlier as `NonLiteralDefault`
    // (since `[]` is an `Expr::List`, not a `Lit`). Either rejection
    // is acceptable for the constitution §2.2 invariant; this test
    // accepts both paths via a custom helper that allows parse-layer
    // rejection (in addition to lower-layer + type-check-layer per
    // the standard `must_reject`).
    //
    // When DEV widens default-arg syntax (Phase F.4+), this test must
    // graduate to `Cat::MutableDefault` (type-check rejection).
    must_reject_with_parse_ok(
        "mutable-default-list-str",
        "fn f(xs: list[str] = []) -> i64:\n    return list_len(xs)\nfn main() -> i64:\n    return f([\"a\"])\n",
        Cat::MutableDefault,
    );
}

/// Like [`must_reject`] but ALSO accepts a parse-layer rejection.
///
/// ADR-0050c §"list[str] knock-on" forward-looking case: the mutable
/// default arg `list[str] = []` is rejected at parse-layer today
/// (`NonLiteralDefault`); when the default-arg surface widens it must
/// be rejected at type-check (`MutableDefault`). This helper accepts
/// either — locks the constitution §2.2 invariant without depending
/// on which layer enforces it.
fn must_reject_with_parse_ok(name: &str, src: &str, cat: Cat) {
    match parse_str(src, FileId::SYNTHETIC) {
        Err(_e) => {
            // Parse-layer rejection counts — constitution §2.2 honored.
        }
        Ok(module) => {
            let mut sess = Session::new();
            match lower(&module, &mut sess) {
                Err(_e) => return, // lowering caught it
                Ok(hir) => match check(&hir) {
                    Ok(_) => panic!(
                        "{name}: must reject (parse/lower/type-check) but passed everything\nsource:\n{src}"
                    ),
                    Err(e) => assert!(
                        matches_cat(&e, cat),
                        "{name}: rejected with wrong category\n  expected: {cat:?}\n  got:      {e:?}\n  source:\n{src}"
                    ),
                },
            }
        }
    }
}

// ---- Tier B.5: for-loop iteration over non-iter / str-typed iter ----

#[test]
fn i102_for_over_str_loop_now_accepted_f88() {
    // F88 / ADR-0101 (§2.5 LLM-first) — `for c in "hello":` is NOW
    // ACCEPTED: the iter-source check binds each `c` to a fresh
    // 1-codepoint `str` (was Phase-G-deferred per ADR-0050b §"Iter
    // source type checking", a clean reject — never a silent miscompile).
    // Body binds `c` (a `str`) to a `str` annotation to PROVE the
    // loop-var type is `str` — no prelude `print` (this harness lowers
    // bare modules with no PRELUDE, so a `print` call would fail at
    // lowering and mask the iter-source acceptance under test).
    let src = "fn f() -> i64:\n    for c in \"hello\":\n        let ch: str = c\n        let _ = ch\n    return 0\n";
    let module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("i102: parse failed: {e:?}\n{src}"));
    let mut sess = Session::new();
    let hir = lower(&module, &mut sess).expect("i102: lowering must succeed");
    check(&hir).expect("i102: `for c in <str>:` must now type-check (F88)");
}

#[test]
fn i102b_str_comprehension_still_rejected_f88() {
    // F88 / ADR-0101 wired ONLY the MIR `for`-loop STR arm. A `str` iter
    // source in a COMPREHENSION is STILL rejected at check (its MIR path
    // is `__cobrust_iter_init`, which has no str support — accepting it
    // would degrade to a codegen-time LLVM-verify error, a §2.5 regression
    // from the clean compile-time reject). Keep it `NotIterable`.
    must_reject(
        "str-comprehension",
        "fn f() -> i64:\n    let xs: list[str] = [c for c in \"hi\"]\n    let _ = xs\n    return 0\n",
        Cat::NotIterable,
    );
}

#[test]
fn i102c_str_in_operator_still_rejected_f88() {
    // F88 / ADR-0101 did NOT add `str` membership (`x in <str>`). The `in`
    // operator's iter-element check stays a clean `NotIterable` reject
    // (its MIR membership path is unimplemented for str — accepting it
    // would degrade to a codegen-time error).
    must_reject(
        "str-in-operator",
        "fn f() -> bool:\n    let s: str = \"hi\"\n    return (\"h\" in s)\n",
        Cat::NotIterable,
    );
}

// ---- Tier B.6: list_is_empty arity / type errors ----

#[test]
fn i103_list_is_empty_with_str_arg_rejected() {
    // `list_is_empty(s)` where s: str — list_is_empty only accepts
    // list types. Type mismatch.
    must_reject(
        "list-is-empty-str-arg",
        "fn f() -> bool:\n    let s: str = \"hi\"\n    return list_is_empty(s)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i104_list_is_empty_no_args_rejected() {
    // `list_is_empty()` — arity mismatch (expects 1 arg).
    must_reject(
        "list-is-empty-no-args",
        "fn f() -> bool:\n    return list_is_empty()\n",
        Cat::ArityMismatch,
    );
}

// ============================================================
// Tier B — Dict ill-typed corpus
// (ADR-0050d sub-sprint a parser/AST/HIR/types surface lock).
//
// Each rejection targets a constitution-§2.2 invariant or an
// ADR-0050d Decision constraint that the type checker (post
// sub-sprint b amendments) MUST surface as a `TypeError::*`
// variant. Some rejections already work pre-impl (TypeMismatch
// is shipped); the NotHashable + DictSpreadNotSupported variants
// are explicitly net-new sub-sprint b additions and the tests
// here SHOULD fail pre-impl, then turn green when DEV ships the
// type-checker amendments.
//
// Test name pattern: `iNNN_dict_<rejection-scenario>`.
//
// Pre-impl status legend (also in the dispatch report):
//   PASS = test passes against current scaffolding (TypeMismatch
//          / ImplicitTruthiness / MutableDefault / etc. already
//          wired); DEV must NOT regress.
//   FAIL = test correctly fails pre-impl; surfaces the gap DEV
//          closes via sub-sprint b new TypeError variant or
//          new check.rs amendment.
// ============================================================

// Cat extension for sub-sprint b net-new TypeError variants.
//
// These categories MUST appear in the `Cat` enum at the top of
// this file once DEV's sub-sprint b adds the corresponding
// TypeError variants. Pre-impl, the tests using these categories
// stay marked with `#[ignore]` so the gate passes against the
// current scaffolding while still documenting the expected
// rejection category.
//
// When DEV lands `TypeError::NotHashable { actual: Ty, span: Span }`
// and `TypeError::DictSpreadNotSupported { span: Span }` (per
// ADR-0050d §"Type-checker amendments" 1 + 2), the test author
// adds the matching `Cat::NotHashable` / `Cat::DictSpreadNotSupported`
// variants to the enum + `matches_cat` switch, removes the
// `#[ignore]` attrs, and the suite turns green.

// ---- Tier B.1: mixed key types — TypeMismatch (PRE-IMPL: PASS) ----

#[test]
fn i105_dict_mixed_key_str_then_i64_rejected() {
    // `{"a": 1, 2: 3}` — first entry seeds K=str (check.rs:651-657),
    // second entry's key `2: i64` unifies vs str → TypeMismatch.
    must_reject(
        "dict-mixed-keys-str-then-i64",
        "fn f() -> Dict[str, i64]:\n    return {\"a\": 1, 2: 3}\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i106_dict_mixed_key_i64_then_str_rejected() {
    // Reverse order: i64 seeded first, str key second.
    must_reject(
        "dict-mixed-keys-i64-then-str",
        "fn f() -> Dict[i64, i64]:\n    return {1: 1, \"a\": 2}\n",
        Cat::TypeMismatch,
    );
}

// ---- Tier B.2: mixed value types — TypeMismatch (PRE-IMPL: PASS) ----

#[test]
fn i107_dict_mixed_value_str_then_i64_rejected() {
    // First entry seeds V=str; second entry's value i64 unifies vs str.
    // Mirrors existing i14_dict_mixed_value pattern.
    must_reject(
        "dict-mixed-values-str-then-i64",
        "fn f() -> Dict[str, str]:\n    return {\"a\": \"x\", \"b\": 2}\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i108_dict_homogeneous_str_keys_mixed_values_rejected() {
    // All str keys; values: i64, str, i64 — TypeMismatch on second entry.
    must_reject(
        "dict-str-keys-mixed-values",
        "fn f() -> Dict[str, i64]:\n    return {\"a\": 1, \"b\": \"x\", \"c\": 3}\n",
        Cat::TypeMismatch,
    );
}

// ---- Tier B.3: index with wrong key type — TypeMismatch (PRE-IMPL: PASS) ----

#[test]
fn i109_dict_index_i64_into_str_keyed_rejected() {
    // `d[1]` where `d: Dict[str, i64]` — i64 key unifies vs str → TM.
    // Already lockable; mirrors existing i26_dict_index_wrong_key.
    must_reject(
        "dict-index-i64-into-str-keyed",
        "fn f(d: Dict[str, i64]) -> i64:\n    return d[1]\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i110_dict_index_str_into_i64_keyed_rejected() {
    // `d["a"]` where `d: Dict[i64, i64]` — str vs i64 → TypeMismatch.
    must_reject(
        "dict-index-str-into-i64-keyed",
        "fn f(d: Dict[i64, i64]) -> i64:\n    return d[\"a\"]\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i111_dict_index_bool_into_str_keyed_rejected() {
    // `d[True]` where `d: Dict[str, i64]` — bool vs str → TM.
    must_reject(
        "dict-index-bool-into-str-keyed",
        "fn f(d: Dict[str, i64]) -> i64:\n    return d[True]\n",
        Cat::TypeMismatch,
    );
}

// ---- Tier B.4: `d[k] = v` write with wrong V type — TypeMismatch
//                (PRE-IMPL: may FAIL — sub-sprint c wires LHS-index
//                 assignment unification at check.rs)              ----

#[test]
fn i112_dict_index_assign_wrong_value_type_rejected() {
    // `d["a"] = "x"` where `d: Dict[str, i64]` — V=i64 vs "x":str → TM.
    must_reject(
        "dict-assign-wrong-value-type",
        "fn f() -> i64:\n    let d: Dict[str, i64] = {\"a\": 1}\n    d[\"a\"] = \"x\"\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i113_dict_index_assign_wrong_key_type_rejected() {
    // `d[1] = 2` where `d: Dict[str, i64]` — K=str vs 1:i64 → TM.
    must_reject(
        "dict-assign-wrong-key-type",
        "fn f() -> i64:\n    let d: Dict[str, i64] = {\"a\": 1}\n    d[1] = 2\n    return 0\n",
        Cat::TypeMismatch,
    );
}

// ---- Tier B.5: implicit truthiness `if d:` — ImplicitTruthiness
//                (PRE-IMPL: PASS — already wired)                  ----

#[test]
fn i114_dict_in_if_predicate_rejected_truthiness() {
    // `if d:` where d: Dict[str, i64] — constitution §2.2 forbids;
    // user must call `dict_is_empty_si(d)` or `len(d) > 0`. Already
    // wired at i50 (negative duplicate); this entry locks the lookalike
    // shape inside a fn body for sub-sprint a's surface coverage.
    must_reject(
        "dict-if-truthiness-rejected",
        "fn f(d: Dict[str, i64]) -> i64:\n    if d:\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i115_dict_in_while_predicate_rejected_truthiness() {
    // `while d:` — same rejection class.
    must_reject(
        "dict-while-truthiness-rejected",
        "fn f(d: Dict[str, i64]) -> i64:\n    while d:\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

// ---- Tier B.6: mutable default arg with dict — MutableDefault
//                (PRE-IMPL: PASS — already wired for `= {}`)       ----

#[test]
fn i116_dict_mutable_default_empty_rejected() {
    // `fn f(d: Dict[str, i64] = {}) -> i64:` — constitution §2.2.
    // At HEAD the parser rejects this earlier as `NonLiteralDefault`
    // (since `{}` is `Expr::Dict`, not a `Lit`). Either rejection
    // path is acceptable; mirrors i101's list-str mutable-default lock.
    must_reject_with_parse_ok(
        "dict-mutable-default-empty",
        "fn f(d: Dict[str, i64] = {}) -> i64:\n    return 0\nfn main() -> i64:\n    return f({\"a\": 1})\n",
        Cat::MutableDefault,
    );
}

#[test]
fn i117_dict_mutable_default_nonempty_rejected() {
    // `fn f(d: Dict[str, i64] = {\"a\": 1}) -> i64:` — same constitution
    // §2.2 invariant on a non-empty literal.
    must_reject_with_parse_ok(
        "dict-mutable-default-nonempty",
        "fn f(d: Dict[str, i64] = {\"a\": 1}) -> i64:\n    return 0\nfn main() -> i64:\n    return f({})\n",
        Cat::MutableDefault,
    );
}

// ---- Tier B.7: f64-keyed dict — NotHashable
//                (PRE-IMPL: FAIL — sub-sprint b net-new variant)   ----

// NotHashable is a Cat addition that DEV's sub-sprint b lands per
// ADR-0050d §"Type-checker amendments" item 1. Pre-impl, the test
// is `#[ignore]` so the suite stays green; once DEV adds
// `TypeError::NotHashable { actual: Ty::Float, span }` + the
// `Cat::NotHashable` enum variant + `matches_cat` row, removing the
// `#[ignore]` re-engages the test and it must turn green.

#[test]
#[ignore = "sub-sprint b lands TypeError::NotHashable; turn green when DEV adds Cat::NotHashable"]
fn i118_dict_f64_key_literal_rejected_not_hashable() {
    // `Dict[f64, i64] = {1.0: 1}` — NaN != NaN breaks Hash invariants;
    // constitution §2.2 "no silent coercion" rejects via NotHashable.
    // Pre-impl the type checker accepts (no NotHashable variant); DEV
    // sub-sprint b lands the rejection.
    //
    // When unmarked, this test category is `Cat::NotHashable` (to add
    // in the enum + matches_cat). Until then, the helper expects a
    // category that doesn't exist; the `#[ignore]` keeps the suite
    // green; the surface gap is documented for DEV.
    must_reject(
        "dict-f64-key-rejected-not-hashable",
        "fn f() -> Dict[f64, i64]:\n    return {1.0: 1}\n",
        Cat::TypeMismatch, // placeholder; replace with Cat::NotHashable post-DEV
    );
}

#[test]
#[ignore = "sub-sprint b lands TypeError::NotHashable; turn green when DEV adds Cat::NotHashable"]
fn i119_dict_f64_key_annot_only_rejected_not_hashable() {
    // `Dict[f64, i64] = {}` — annotation alone (no entries) should also
    // surface NotHashable at the annotation-validation site
    // (`lower_type` → Ty::Dict per ADR-0050d §"Type-checker amendments" 1).
    must_reject(
        "dict-f64-annot-only-rejected-not-hashable",
        "fn f() -> Dict[f64, i64]:\n    let d: Dict[f64, i64] = {}\n    return d\n",
        Cat::TypeMismatch, // placeholder; replace with Cat::NotHashable post-DEV
    );
}

#[test]
#[ignore = "sub-sprint b lands TypeError::NotHashable; turn green when DEV adds Cat::NotHashable"]
fn i120_dict_list_key_rejected_not_hashable() {
    // `Dict[List[i64], i64]` — lists are unhashable (Python tradition
    // and is_hashable(List) = false per ADR-0050d §"Type-checker
    // amendments" 2). Pre-impl the type checker accepts; DEV adds the
    // rejection.
    must_reject(
        "dict-list-key-rejected-not-hashable",
        "fn f() -> Dict[List[i64], i64]:\n    let xs: List[i64] = [1, 2]\n    let d: Dict[List[i64], i64] = {xs: 1}\n    return d\n",
        Cat::TypeMismatch, // placeholder; replace with Cat::NotHashable post-DEV
    );
}

// ---- Tier B.8: dict-spread in non-comprehension literal —
//      DictSpreadNotSupported (PRE-IMPL: FAIL — sub-sprint b
//      net-new variant per ADR-0050d §"Parser amendments" 1)      ----

#[test]
#[ignore = "sub-sprint b lands TypeError::DictSpreadNotSupported; turn green when DEV adds Cat::DictSpreadNotSupported"]
fn i121_dict_spread_in_literal_rejected() {
    // `{**other}` in a non-comprehension dict literal — Phase F.3 rejects
    // (dict-merge is Phase G per ADR-0050d Decision 1 footnote). Parser
    // already emits `DictEntry::Spread`; type-checker amendment surfaces
    // the rejection.
    must_reject(
        "dict-spread-in-literal-rejected",
        "fn f() -> Dict[str, i64]:\n    let other: Dict[str, i64] = {\"a\": 1}\n    return {**other}\n",
        Cat::TypeMismatch, // placeholder; replace with Cat::DictSpreadNotSupported post-DEV
    );
}

#[test]
#[ignore = "sub-sprint b lands TypeError::DictSpreadNotSupported; turn green when DEV adds Cat::DictSpreadNotSupported"]
fn i122_dict_spread_mixed_with_entries_rejected() {
    // `{"x": 1, **other}` — same rejection; mixed-mode literal.
    must_reject(
        "dict-spread-mixed-rejected",
        "fn f() -> Dict[str, i64]:\n    let other: Dict[str, i64] = {\"a\": 1}\n    return {\"x\": 1, **other}\n",
        Cat::TypeMismatch, // placeholder; replace with Cat::DictSpreadNotSupported post-DEV
    );
}

// ---- Tier B.9: indexing into a non-dict / non-list — NotIndexable
//                (PRE-IMPL: PASS — already wired)                  ----

#[test]
fn i123_dict_index_into_i64_rejected_not_indexable() {
    // `n["a"]` where n: i64 — i64 is not indexable.
    must_reject(
        "dict-index-into-i64",
        "fn f(n: i64) -> i64:\n    return n[\"a\"]\n",
        Cat::NotIndexable,
    );
}

#[test]
fn i124_dict_index_into_bool_rejected_not_indexable() {
    // `b["a"]` where b: bool — bool is not indexable.
    must_reject(
        "dict-index-into-bool",
        "fn f(b: bool) -> i64:\n    return b[\"a\"]\n",
        Cat::NotIndexable,
    );
}

// ---- Tier B.10: empty literal in ambiguous-K context — AmbiguousType
//                 (PRE-IMPL: may PASS or FAIL depending on whether
//                  the empty-dict synth narrows K with later uses) ----

#[test]
#[ignore = "sub-sprint b ratifies whether empty-dict in non-annotated context is Ambiguous or fresh-K; DEV decides"]
fn i125_dict_empty_no_annot_ambiguous_or_inferred() {
    // `let d = {}` with no subsequent use that pins K/V — type checker
    // should either pin via later use (current behavior?) or raise
    // AmbiguousType. This test captures the decision-point; sub-sprint b
    // ratifies which behavior is correct.
    must_reject(
        "dict-empty-no-annot-no-use",
        "fn f() -> i64:\n    let d = {}\n    return 0\n",
        Cat::AmbiguousType,
    );
}

// ============================================================
// Tier B — M-F.3.5 string stdlib ill-typed corpus (ADR-0050e).
//
// Locks the type-checker rejection surface for the eleven new PRELUDE
// fns from ADR-0050e §"Decision 3":
//   1.  split / 2. join / 3. replace / 4. trim / 5. find
//   6.  contains / 7. starts_with / 8. ends_with
//   9.  lower / 10. upper / 11. clone
//
// Per the precedent at well_typed.rs:STR_STDLIB_STUBS, these tests
// prepend the eleven PRELUDE signatures inline so the rejection is
// from arg-type / arity / return-type mismatch rather than UnknownName
// (which would be a less specific signal). Sub-sprint 1 DEV graduates
// the stubs into the canonical PRELUDE; after that the stub prefix
// is redundant but harmless.
//
// Coverage table (matches mission §"Tier B" requirements):
//   - wrong arg type for each fn (i126..i130)
//   - wrong return-bind type (i131..i133)
//   - clone on non-Str (i134) — clone is Str-only in M-F.3.5
//   - implicit-truthiness of find return (i135) — Cat::ImplicitTruthiness
//   - wrong arity for each variadic-position fn (i136..i140)
//
// ============================================================

// Shared stub block (mirror of STR_STDLIB_STUBS in well_typed.rs).
const STR_STDLIB_STUBS: &str = concat!(
    "fn print(s: str) -> i64:\n    return 0\n",
    "fn split(s: str, sep: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n",
    "fn join(parts: list[str], sep: str) -> str:\n    return \"\"\n",
    "fn replace(s: str, old: str, new: str) -> str:\n    return \"\"\n",
    "fn trim(s: str) -> str:\n    return \"\"\n",
    "fn find(s: str, needle: str) -> i64:\n    return -1\n",
    "fn contains(s: str, needle: str) -> bool:\n    return False\n",
    "fn starts_with(s: str, prefix: str) -> bool:\n    return False\n",
    "fn ends_with(s: str, suffix: str) -> bool:\n    return False\n",
    "fn lower(s: str) -> str:\n    return \"\"\n",
    "fn upper(s: str) -> str:\n    return \"\"\n",
    "fn clone(s: str) -> str:\n    return s\n",
);

fn must_reject_with_str_stdlib_stubs(name: &str, body: &str, cat: Cat) {
    let src = format!("{STR_STDLIB_STUBS}{body}");
    must_reject(name, &src, cat);
}

// ---- Tier B.1: wrong arg type for each surface fn ----

#[test]
fn i126_split_wrong_first_arg_int_rejected() {
    // `split(42, ",")` — first arg must be str, not i64.
    must_reject_with_str_stdlib_stubs(
        "split-int-first-arg",
        "fn f() -> i64:\n    let xs: list[str] = split(42, \",\")\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i127_split_wrong_second_arg_int_rejected() {
    // `split("a,b", 0)` — second arg must be str.
    must_reject_with_str_stdlib_stubs(
        "split-int-second-arg",
        "fn f() -> i64:\n    let xs: list[str] = split(\"a,b\", 0)\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i128_contains_wrong_needle_int_rejected() {
    // `contains(s, 42)` — needle must be str.
    must_reject_with_str_stdlib_stubs(
        "contains-int-needle",
        "fn f(s: str) -> bool:\n    return contains(s, 42)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i129_replace_wrong_third_arg_bool_rejected() {
    // `replace(s, "a", True)` — third arg must be str.
    must_reject_with_str_stdlib_stubs(
        "replace-bool-third-arg",
        "fn f(s: str) -> str:\n    return replace(s, \"a\", True)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i130_find_wrong_needle_list_rejected() {
    // `find(s, [1, 2])` — needle must be str; list[i64] rejected.
    must_reject_with_str_stdlib_stubs(
        "find-list-needle",
        "fn f(s: str) -> i64:\n    let xs: list[i64] = [1, 2]\n    return find(s, xs)\n",
        Cat::TypeMismatch,
    );
}

// ---- Tier B.2: wrong return-bind type ----

#[test]
fn i131_trim_return_bound_to_i64_rejected() {
    // `let v: i64 = trim("x")` — trim returns str, not i64.
    must_reject_with_str_stdlib_stubs(
        "trim-into-i64-let",
        "fn f() -> i64:\n    let v: i64 = trim(\"x\")\n    return v\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i132_find_return_bound_to_str_rejected() {
    // `let v: str = find(s, n)` — find returns i64, not str.
    must_reject_with_str_stdlib_stubs(
        "find-into-str-let",
        "fn f(s: str, n: str) -> str:\n    let v: str = find(s, n)\n    return v\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i133_contains_return_bound_to_str_rejected() {
    // `let v: str = contains(s, n)` — contains returns bool, not str.
    must_reject_with_str_stdlib_stubs(
        "contains-into-str-let",
        "fn f(s: str, n: str) -> str:\n    let v: str = contains(s, n)\n    return v\n",
        Cat::TypeMismatch,
    );
}

// ---- Tier B.3: clone on non-Str — clone is Str-only in M-F.3.5 ----

#[test]
fn i134_clone_on_i64_rejected_str_only() {
    // `clone(42)` — clone is `fn clone(s: str) -> str`; calling on i64
    // is an arg-type error. Generic clone is Phase G (Q10 in ADR-0050e
    // §"Open questions").
    must_reject_with_str_stdlib_stubs(
        "clone-on-i64",
        "fn f() -> i64:\n    let v: str = clone(42)\n    return 0\n",
        Cat::TypeMismatch,
    );
}

// ---- Tier B.4: implicit-truthiness of find's i64 return ----

#[test]
fn i135_find_in_if_predicate_implicit_truthy_rejected() {
    // The footgun ADR-0050e Decision 5 / Q2 calls out explicitly:
    // `if find(s, x):` is implicit-truthiness on an i64 return.
    // Constitution §2.2 forbids; type-check rejects with
    // ImplicitTruthiness. Users MUST write `if find(s, x) != -1:`.
    //
    // This test locks the §2.2 footgun-blocking gate documented at
    // ADR-0050e §"Decision 5 — `find` returns i64 with -1 sentinel".
    must_reject_with_str_stdlib_stubs(
        "find-in-if-implicit-truthy",
        "fn f(s: str, n: str) -> i64:\n    if find(s, n):\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

// ---- Tier B.5: wrong arity ----

#[test]
fn i136_split_one_arg_arity_rejected() {
    // `split("x")` — split requires 2 args; calling with 1 is arity.
    must_reject_with_str_stdlib_stubs(
        "split-arity-one",
        "fn f() -> i64:\n    let xs: list[str] = split(\"x\")\n    return 0\n",
        Cat::ArityMismatch,
    );
}

#[test]
fn i137_clone_zero_args_arity_rejected() {
    // `clone()` — clone requires 1 arg; zero is arity.
    must_reject_with_str_stdlib_stubs(
        "clone-arity-zero",
        "fn f() -> i64:\n    let v: str = clone()\n    return 0\n",
        Cat::ArityMismatch,
    );
}

#[test]
fn i138_replace_two_args_arity_rejected() {
    // `replace(s, old)` — replace requires 3 args; 2 is arity.
    must_reject_with_str_stdlib_stubs(
        "replace-arity-two",
        "fn f() -> str:\n    return replace(\"a\", \"b\")\n",
        Cat::ArityMismatch,
    );
}

#[test]
fn i139_trim_two_args_arity_rejected() {
    // `trim(s, x)` — trim accepts 1 arg; 2 is arity. The Phase G
    // `trim_chars(s, chars)` extension is a different surface
    // (per ADR-0050e §Q5).
    must_reject_with_str_stdlib_stubs(
        "trim-arity-two",
        "fn f() -> str:\n    return trim(\"  x  \", \" \")\n",
        Cat::ArityMismatch,
    );
}

#[test]
fn i140_starts_with_one_arg_arity_rejected() {
    // `starts_with(s)` — starts_with requires 2 args.
    must_reject_with_str_stdlib_stubs(
        "starts-with-arity-one",
        "fn f() -> bool:\n    return starts_with(\"abc\")\n",
        Cat::ArityMismatch,
    );
}

// ============================================================
// M-F.3.6 — File IO completion (ADR-0050f)
// i141..i150 — Tier B ill-typed corpus for 7 surface fns.
//
// Pre-impl status: the 7 fns do not exist in the PRELUDE yet.
// These tests inject FILE_IO_STUBS inline (same pattern as
// STR_STDLIB_STUBS above) so rejection is from arg-type /
// arity / return-type mismatch rather than UnknownName.
//
// Coverage table (ADR-0050f mission §"Tier B"):
//   i141: wrong arg type — write_file(42, "x") → TypeMismatch
//   i142: implicit truthy on i64 — if write_file(p, c): → ImplicitTruthiness
//   i143: wrong return bind — let s: str = write_file(p, c) → TypeMismatch
//   i144: wrong arg type — read_file(42) → TypeMismatch
//   i145: wrong arg type — append_file(42, "x") → TypeMismatch
//   i146: implicit truthy — if append_file(p, c): → ImplicitTruthiness
//   i147: wrong return bind — let b: bool = stdout_write(s) → TypeMismatch
//   i148: implicit truthy — if stdout_write(s): → ImplicitTruthiness
//   i149: wrong arg type — stdout_write(42) → TypeMismatch
//   i150: arity — write_file("/path") → ArityMismatch (1 arg, needs 2)
//
// NOTE: read_file with a non-existent path is a RUNTIME error,
// not a TYPE error. Tests for runtime errors live in the E2E
// corpus (file_io_e2e.rs). No ill-typed test covers that case.
// ============================================================

// Shared file-IO stub block (mirrors FILE_IO_STUBS in well_typed.rs).
const FILE_IO_STUBS: &str = concat!(
    "fn print(s: str) -> i64:\n    return 0\n",
    "fn str_len(s: str) -> i64:\n    return 0\n",
    "fn clone(s: str) -> str:\n    return s\n",
    "fn read_file(path: str) -> str:\n    return \"\"\n",
    "fn read_file_lines(path: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n",
    "fn write_file(path: str, contents: str) -> i64:\n    return 0\n",
    "fn append_file(path: str, contents: str) -> i64:\n    return 0\n",
    "fn stdin_read_all() -> str:\n    return \"\"\n",
    "fn stdout_write(s: str) -> i64:\n    return 0\n",
    "fn stderr_write(s: str) -> i64:\n    return 0\n",
);

fn must_reject_with_file_io_stubs(name: &str, body: &str, cat: Cat) {
    let src = format!("{FILE_IO_STUBS}{body}");
    must_reject(name, &src, cat);
}

// ---- Tier B.1: wrong arg type for write_file / read_file ----

#[test]
fn i141_write_file_first_arg_int_rejected() {
    // `write_file(42, "x")` — first arg must be str (path), not i64.
    // ADR-0050f §"Decision": `write_file(path: str, contents: str) -> i64`.
    must_reject_with_file_io_stubs(
        "write-file-int-path",
        "fn f() -> i64:\n    return write_file(42, \"x\")\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i142_write_file_implicit_truthy_on_i64_rejected() {
    // `if write_file(p, c):` — implicit truthiness on i64 return.
    // ADR-0050f Q1 + constitution §2.2 "if x requires x: bool".
    must_reject_with_file_io_stubs(
        "write-file-implicit-truthy",
        "fn f() -> i64:\n    if write_file(\"/tmp/x\", \"hello\"):\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i143_write_file_return_bound_to_str_rejected() {
    // `let s: str = write_file(p, c)` — return is i64, not str.
    // Type annotation mismatch.
    must_reject_with_file_io_stubs(
        "write-file-return-as-str",
        "fn f() -> i64:\n    let s: str = write_file(\"/tmp/x\", \"hello\")\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i144_read_file_int_path_rejected() {
    // `read_file(42)` — path must be str; i64 rejected.
    // ADR-0050f §"Decision": `read_file(path: str) -> str`.
    must_reject_with_file_io_stubs(
        "read-file-int-path",
        "fn f() -> str:\n    return read_file(42)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i145_append_file_first_arg_int_rejected() {
    // `append_file(42, "x")` — first arg must be str.
    // ADR-0050f §"Decision": `append_file(path: str, contents: str) -> i64`.
    must_reject_with_file_io_stubs(
        "append-file-int-path",
        "fn f() -> i64:\n    return append_file(42, \"x\")\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i146_append_file_implicit_truthy_on_i64_rejected() {
    // `if append_file(p, c):` — implicit truthiness on i64 return.
    must_reject_with_file_io_stubs(
        "append-file-implicit-truthy",
        "fn f() -> i64:\n    if append_file(\"/tmp/x\", \"more\"):\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i147_stdout_write_return_bound_to_bool_rejected() {
    // `let b: bool = stdout_write(s)` — return is i64, not bool.
    // Locks that i64-sentinel return is not silently coerced.
    must_reject_with_file_io_stubs(
        "stdout-write-return-as-bool",
        "fn f() -> i64:\n    let b: bool = stdout_write(\"msg\")\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i148_stdout_write_implicit_truthy_on_i64_rejected() {
    // `if stdout_write(s):` — implicit truthiness. Same rule as
    // print family; stdout_write i64 return cannot be used as bool.
    must_reject_with_file_io_stubs(
        "stdout-write-implicit-truthy",
        "fn f() -> i64:\n    if stdout_write(\"msg\"):\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

#[test]
fn i149_stdout_write_int_arg_rejected() {
    // `stdout_write(42)` — arg must be str; i64 rejected.
    // ADR-0050f §"Decision": `stdout_write(s: str) -> i64`.
    must_reject_with_file_io_stubs(
        "stdout-write-int-arg",
        "fn f() -> i64:\n    return stdout_write(42)\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i150_write_file_one_arg_arity_rejected() {
    // `write_file("/path")` — write_file requires 2 args; 1 is arity.
    must_reject_with_file_io_stubs(
        "write-file-arity-one",
        "fn f() -> i64:\n    return write_file(\"/tmp/x\")\n",
        Cat::ArityMismatch,
    );
}

// ============================================================
// ADR-0052a Wave 1 — Direction A explicit `&s` borrow type-error corpus
//
// 6 ill-typed programs the type checker MUST reject under the `&s`
// surface (CLAUDE.md §2.5 Direction A binding).
//
// Pre-DEV-impl status: every i0052a_* test below is `#[ignore]`'d
// pending Wave-1 DEV merge. DEV removes the `#[ignore]` markers and
// the suite turns green.
//
// Coverage map (mirrors ADR-0052a §10.1 ill-typed category):
// - `&undefined_ident`                          → i0052a_01 (UnknownName)
// - `&s` where s declared but not bound         → i0052a_02 (UnknownName)
// - `&` operand-arity mismatch surfaces TM      → i0052a_03 (TypeMismatch)
// - borrow used in arith without int coercion   → i0052a_04 (TypeMismatch)
// - borrow assigned to wrong typed annotation   → i0052a_05 (TypeMismatch)
// - borrow as if-cond (implicit truthiness)     → i0052a_06 (ImplicitTruthiness)
//
// NOTE: TypeError::BorrowOfNonPlace per ADR-0052a §6 is a Wave-1 net-new
// variant; tests would require Cat::BorrowOfNonPlace enum addition. We
// stage that via the `Cat::TypeMismatch` placeholder pattern established
// in i118+ for NotHashable / DictSpreadNotSupported.
// ============================================================

#[test]
fn i0052a_01_borrow_of_undefined_ident_rejected() {
    // `&missing` — borrow of an undefined name surfaces as
    // TypeError::UnknownName at type-check time.
    must_reject(
        "borrow-of-undefined-ident",
        "fn main() -> i64:\n    let n = str_len(&missing)\n    return n\n",
        Cat::UnknownName,
    );
}

#[test]
fn i0052a_02_borrow_of_out_of_scope_ident_rejected() {
    // `&s` where `s` was defined in an outer block that exited;
    // surfaces as UnknownName at the inner use site.
    must_reject(
        "borrow-of-out-of-scope",
        "fn main() -> i64:\n    let cond: bool = True\n    if cond:\n        let s: str = \"hi\"\n        let _ = str_len(&s)\n    let m = str_len(&s)\n    return m\n",
        Cat::UnknownName,
    );
}

#[test]
fn i0052a_03_borrow_assigned_to_int_annot_rejected() {
    // `let n: i64 = &s` — borrow of a Str cannot satisfy an i64 type
    // annotation. Surfaces as TypeMismatch at the assignment site.
    //
    // Note: under Wave-1 transparency `&Str` and `Str` are
    // interchangeable for read-only PRELUDE positions, but the type
    // annotation slot is NOT read-only — it constrains the local's
    // type. `&Str` ≠ `i64` regardless of transparency.
    must_reject(
        "borrow-assigned-int-annot",
        "fn main() -> i64:\n    let s: str = \"hi\"\n    let n: i64 = &s\n    return n\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i0052a_04_borrow_int_plus_borrow_str_rejected() {
    // `(&n) + (&s)` — adding a borrow of Int and a borrow of Str
    // must surface TypeMismatch the same way `n + s` does. Wave-1
    // transparency rule says PRELUDE-read positions accept both;
    // arithmetic is not a PRELUDE position.
    must_reject(
        "borrow-int-plus-borrow-str",
        "fn main() -> i64:\n    let n: i64 = 1\n    let s: str = \"hi\"\n    let total = (&n) + (&s)\n    return total\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i0052a_05_borrow_str_passed_where_int_expected_rejected() {
    // `&s` (borrow of str) passed where the function expects `n: i64`.
    // Transparency rule does NOT bridge str → i64; surfaces as
    // TypeMismatch.
    must_reject(
        "borrow-str-where-int-expected",
        "fn takes_int(n: i64) -> i64:\n    return n + 1\nfn main() -> i64:\n    let s: str = \"hi\"\n    let r = takes_int(&s)\n    return r\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i0052a_06_borrow_in_if_cond_implicit_truthiness_rejected() {
    // `if &s:` — borrow of Str used as if-condition surfaces
    // ImplicitTruthiness, same as `if s:`. Constitution §2.2
    // "Implicit truthy/falsy" rule applies through the transparency
    // rule.
    must_reject(
        "borrow-as-if-cond",
        "fn main() -> i64:\n    let s: str = \"hi\"\n    if &s:\n        return 1\n    return 0\n",
        Cat::ImplicitTruthiness,
    );
}

// ============================================================
// ADR-0052d-prereq Wave 2 — method-dispatch ill-typed corpus
//
// 12 rejection programs covering the new `TypeError::UnknownMethod`
// variant (per ADR-0052d-prereq §"New error variant") + arity /
// arg-type rejection paths for the four new per-type method tables.
//
// Pre-DEV-impl status: every i0052dpre_* test below is `#[ignore]`'d
// pending Wave-2 DEV merge per F28 strict-separation PAIR pattern
// (`findings/adsd-pair-pattern-impl-gap.md`). DEV's responsibility
// is to land (a) `TypeError::UnknownMethod { type_name, method_name,
// span, suggestion }`, (b) the four `try_synth_*_method` fns +
// chain dispatcher, (c) per-method arity / arg-type validation
// inside each table arm, then unmark the tests and add the
// `Cat::UnknownMethod` enum variant + `matches_cat` row so the
// typo / wrong-base-type cases lock onto the post-impl variant.
//
// Placeholder cat strategy: typo + wrong-base-type cases use
// `Cat::UnknownName` (semantically closest stable variant per
// ADR-0052d-prereq §"New error variant"); DEV swaps to
// `Cat::UnknownMethod` post-impl. Wrong-arity uses `Cat::ArityMismatch`
// (already wired in the existing dict-method table). Wrong-arg-type
// uses `Cat::TypeMismatch`.
//
// Coverage map (per dispatch contract §"Ill-typed corpus"):
//   - Typos:            i0052dpre_01..03 (Str / List / Float)
//   - Wrong-base-type:  i0052dpre_04..06 (Int→Str, Str→Float, List→Str)
//   - Wrong-arity:      i0052dpre_07..09 (Str.split / Str.split2 / List.push)
//   - Wrong-arg-type:   i0052dpre_10..12 (Str.split / List.push / Float.is_nan)
// ============================================================

// Shared stub block: PRELUDE-fn signatures for the new method-table
// rewrite targets. Mirrors `METHOD_DISPATCH_STUBS` in the well_typed
// sibling. Each i0052dpre_* test prepends these so the type-checker
// has signatures to consult for the rewritten PRELUDE-fn call.
const METHOD_DISPATCH_STUBS_IT: &str = concat!(
    "fn str_len(s: str) -> i64:\n    return 0\n",
    "fn split(s: str, sep: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n",
    "fn replace(s: str, old: str, new: str) -> str:\n    return \"\"\n",
    "fn trim(s: str) -> str:\n    return \"\"\n",
    "fn find(s: str, needle: str) -> i64:\n    return -1\n",
    "fn contains(s: str, needle: str) -> bool:\n    return False\n",
    "fn starts_with(s: str, prefix: str) -> bool:\n    return False\n",
    "fn ends_with(s: str, suffix: str) -> bool:\n    return False\n",
    "fn lower(s: str) -> str:\n    return \"\"\n",
    "fn upper(s: str) -> str:\n    return \"\"\n",
    "fn list_push(xs: list[i64], v: i64) -> i64:\n    return 0\n",
    "fn list_get(xs: list[i64], i: i64) -> i64:\n    return 0\n",
    "fn list_set(xs: list[i64], i: i64, v: i64) -> i64:\n    return 0\n",
    "fn list_is_empty(xs: list[i64]) -> bool:\n    return True\n",
    "fn len(xs: list[i64]) -> i64:\n    return 0\n",
    "fn floor(f: f64) -> f64:\n    return f\n",
    "fn ceil(f: f64) -> f64:\n    return f\n",
    "fn is_nan(f: f64) -> bool:\n    return False\n",
    "fn is_finite(f: f64) -> bool:\n    return True\n",
    "fn abs_f(f: f64) -> f64:\n    return f\n",
    "fn abs(n: i64) -> i64:\n    return n\n",
    "fn pow(n: i64, k: i64) -> i64:\n    return 0\n",
    "fn min(a: i64, b: i64) -> i64:\n    return a\n",
    "fn max(a: i64, b: i64) -> i64:\n    return a\n",
    "fn bit_count(n: i64) -> i64:\n    return 0\n",
);

fn must_reject_with_method_dispatch_stubs(name: &str, body: &str, cat: Cat) {
    let src = format!("{METHOD_DISPATCH_STUBS_IT}{body}");
    must_reject(name, &src, cat);
}

// ---- Tier A: typo on otherwise-valid receiver type ----

#[test]
fn i0052dpre_01_str_typo_splittt_rejected() {
    // `s.splittt(",")` — typo for `s.split(",")` on a Str receiver.
    // Post-impl: `TypeError::UnknownMethod { type_name: "str",
    // method_name: "splittt", suggestion: Some("did you mean
    // 'split'?") }` per ADR-0052d-prereq §"New error variant".
    // Placeholder: `Cat::UnknownName` (closest stable variant
    // semantically); DEV swaps to `Cat::UnknownMethod` post-impl.
    must_reject_with_method_dispatch_stubs(
        "str-typo-splittt",
        "fn f() -> i64:\n    let s: str = \"a,b\"\n    let xs: list[str] = s.splittt(\",\")\n    return 0\n",
        Cat::UnknownMethod, // ADR-0052d-prereq DEV graduation per inline-comment contract
    );
}

#[test]
fn i0052dpre_02_list_typo_lenggg_rejected() {
    // `xs.lenggg()` — typo for `xs.len()` on a List receiver.
    // Post-impl: UnknownMethod with type_name="list".
    must_reject_with_method_dispatch_stubs(
        "list-typo-lenggg",
        "fn f() -> i64:\n    let xs: list[i64] = [1, 2]\n    let n: i64 = xs.lenggg()\n    return n\n",
        Cat::UnknownMethod, // ADR-0052d-prereq DEV graduation per inline-comment contract
    );
}

#[test]
fn i0052dpre_03_float_typo_flrr_rejected() {
    // `f.flrr()` — typo for `f.floor()` on a Float receiver.
    // Post-impl: UnknownMethod with type_name="f64".
    must_reject_with_method_dispatch_stubs(
        "float-typo-flrr",
        "fn g() -> f64:\n    let x: f64 = 3.14\n    let y: f64 = x.flrr()\n    return y\n",
        Cat::UnknownMethod, // ADR-0052d-prereq DEV graduation per inline-comment contract
    );
}

// ---- Tier B: method-form on the WRONG receiver type ----

#[test]
fn i0052dpre_04_int_split_method_only_on_str_rejected() {
    // `n.split(",")` where `n: i64` — `split` lives on the Str table
    // ONLY; calling it on Int must surface UnknownMethod with
    // type_name="i64".
    must_reject_with_method_dispatch_stubs(
        "int-split-rejected-method-only-on-str",
        "fn f() -> i64:\n    let n: i64 = 42\n    let xs: list[str] = n.split(\",\")\n    return 0\n",
        Cat::UnknownMethod, // ADR-0052d-prereq DEV graduation per inline-comment contract
    );
}

#[test]
fn i0052dpre_05_str_floor_method_only_on_float_rejected() {
    // `s.floor()` where `s: str` — `floor` lives on the Float table
    // ONLY. UnknownMethod with type_name="str".
    must_reject_with_method_dispatch_stubs(
        "str-floor-rejected-method-only-on-float",
        "fn g() -> f64:\n    let s: str = \"hi\"\n    let y: f64 = s.floor()\n    return y\n",
        Cat::UnknownMethod, // ADR-0052d-prereq DEV graduation per inline-comment contract
    );
}

#[test]
fn i0052dpre_06_list_upper_method_only_on_str_rejected() {
    // `xs.upper()` where `xs: list[i64]` — `upper` lives on the Str
    // table ONLY. UnknownMethod with type_name="list".
    must_reject_with_method_dispatch_stubs(
        "list-upper-rejected-method-only-on-str",
        "fn f() -> str:\n    let xs: list[i64] = [1, 2]\n    let t: str = xs.upper()\n    return t\n",
        Cat::UnknownMethod, // ADR-0052d-prereq DEV graduation per inline-comment contract
    );
}

// ---- Tier C: wrong arity ----

#[test]
fn i0052dpre_07_str_split_missing_delim_rejected() {
    // `s.split()` — `split` requires 1 arg per ADR-0052d-prereq §4
    // row 2; calling with 0 args must surface ArityMismatch (already
    // wired in the existing dict-method table; the new Str table
    // mirrors the pattern).
    must_reject_with_method_dispatch_stubs(
        "str-split-missing-delim",
        "fn f() -> list[str]:\n    let s: str = \"a,b\"\n    let xs: list[str] = s.split()\n    return xs\n",
        Cat::ArityMismatch,
    );
}

#[test]
fn i0052dpre_08_str_split_extra_arg_rejected() {
    // `s.split(",", "x")` — 2 args where 1 is expected.
    must_reject_with_method_dispatch_stubs(
        "str-split-extra-arg",
        "fn f() -> list[str]:\n    let s: str = \"a,b\"\n    let xs: list[str] = s.split(\",\", \"x\")\n    return xs\n",
        Cat::ArityMismatch,
    );
}

#[test]
fn i0052dpre_09_list_push_missing_value_rejected() {
    // `xs.push()` — `push` requires 1 arg per ADR-0052d-prereq §4
    // row 12; calling with 0 args must surface ArityMismatch.
    must_reject_with_method_dispatch_stubs(
        "list-push-missing-value",
        "fn f() -> i64:\n    let xs: list[i64] = [1, 2]\n    let _ = xs.push()\n    return 0\n",
        Cat::ArityMismatch,
    );
}

// ---- Tier D: wrong arg type ----

#[test]
fn i0052dpre_10_str_split_int_delim_rejected() {
    // `s.split(42)` — `split` requires `sep: str` per ADR-0052d-prereq
    // §4 row 2; passing `i64` must surface TypeMismatch (the table
    // arm calls `unify(&Ty::Str, &arg_ty, ...)` exactly like the
    // existing dict-method table for `get(k)`).
    must_reject_with_method_dispatch_stubs(
        "str-split-int-delim",
        "fn f() -> list[str]:\n    let s: str = \"a,b\"\n    let xs: list[str] = s.split(42)\n    return xs\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i0052dpre_11_list_push_str_into_list_i64_rejected() {
    // `xs.push("x")` where `xs: list[i64]` — the element type must
    // unify with i64. TypeMismatch.
    must_reject_with_method_dispatch_stubs(
        "list-push-str-into-int-list",
        "fn f() -> i64:\n    let xs: list[i64] = [1, 2]\n    let _ = xs.push(\"x\")\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i0052dpre_12_float_is_nan_extra_arg_rejected() {
    // `f.is_nan(42)` — `is_nan` per ADR-0052d-prereq §4 row 18 is
    // 0-arity; passing any extra arg must surface ArityMismatch.
    // (Also a wrong-arity case; the ADR test plan calls this the
    // boundary case where wrong-arity dominates wrong-arg-type.)
    must_reject_with_method_dispatch_stubs(
        "float-is-nan-extra-arg",
        "fn g() -> bool:\n    let x: f64 = 1.0\n    let b: bool = x.is_nan(42)\n    return b\n",
        Cat::ArityMismatch,
    );
}

// ============================================================
// ADR-0052d-prereq × ADR-0052b cross-ADR coordination test
//
// Per ADR-0052d-prereq §"New error variant":
//
// ```rust
// TypeError::UnknownMethod {
//     type_name: String,
//     method_name: String,
//     span: Span,
//     suggestion: Option<&'static str>,
// }
// ```
//
// The `suggestion: Option<&'static str>` field is a Wave-2 stub for
// ADR-0052b Direction B's structured-suggestion record (the eventual
// promoted shape that lives on every `TypeError::*` variant per
// CLAUDE.md §2.5 line 78 "print the FIX, not just the diagnosis").
//
// This test locks the cross-ADR contract: for typo cases like
// `s.splittt(",")`, the `suggestion` field MUST be `Some(_)` (a
// concrete hint), NOT `None`. The hint contents are not asserted
// (DEV can choose "did you mean 'split'?" or a method-list); only
// the `Some`-ness is locked, because the structured-suggestion shape
// is promoted by 0052b after Wave-2 merge.
//
// Pre-impl strategy: since `TypeError::UnknownMethod` is a net-new
// variant that DEV adds in Wave-2, this test uses a string-match
// proxy on `format!("{:?}", err)` to forward-compat the assertion
// without depending on the variant existing. DEV updates the test
// to do a real `if let TypeError::UnknownMethod { suggestion, .. } =
// err` pattern-match post-impl.
//
// Test name: i0052dpre_cross_01.
// ============================================================

#[test]
fn i0052dpre_cross_01_unknown_method_suggestion_field_populated_for_typo() {
    // `s.splittt(",")` — typo. Post-impl: type-check produces
    // `TypeError::UnknownMethod { type_name: "str", method_name:
    // "splittt", suggestion: Some(_), .. }` per ADR-0052d-prereq
    // §"New error variant" coordinated with ADR-0052b Direction B.
    //
    // Locked property: `suggestion.is_some()`. (Hint contents are
    // not asserted; the structured-suggestion shape is 0052b's
    // post-Wave-2 refactor scope.)
    let src = format!(
        "{METHOD_DISPATCH_STUBS_IT}fn f() -> i64:\n    let s: str = \"a,b\"\n    let xs: list[str] = s.splittt(\",\")\n    return 0\n"
    );
    let module = parse_str(&src, FileId::SYNTHETIC).expect("parse must succeed");
    let mut sess = Session::new();
    let hir = lower(&module, &mut sess).expect("hir lower must succeed");
    let err = check(&hir).expect_err(
        "type check must reject `s.splittt(...)` (typo); pre-impl this assertion fails on the Ok branch",
    );
    // Forward-compat proxy: stringify the error and check for
    // `UnknownMethod` + a non-empty `suggestion: Some(...)` shape.
    // DEV replaces this with:
    //
    //   if let TypeError::UnknownMethod { suggestion, .. } = err {
    //       assert!(suggestion.is_some(), "suggestion must be populated");
    //   } else {
    //       panic!("expected UnknownMethod, got {err:?}");
    //   }
    let dbg = format!("{err:?}");
    assert!(
        dbg.contains("UnknownMethod"),
        "i0052dpre_cross_01: expected `TypeError::UnknownMethod` for `s.splittt(...)` typo, got: {dbg}"
    );
    assert!(
        dbg.contains("suggestion: Some"),
        "i0052dpre_cross_01: `suggestion` field must be `Some(_)` for typo (ADR-0052b Direction B coordination), got: {dbg}"
    );
}

// ============================================================
// ADR-0052g Wave 2 round 2 — `&recv.method()` rejection corpus
//
// 3 ill-typed programs the type checker MUST reject under the
// narrowed `Borrow` synth arm per ADR-0052g §4.2-§4.3:
//
//   - `&s.split(",")` (Str.split returns `list[str]` — non-Copy)
//   - `&xs.get(0)` (List[Str] element returns Str — non-Copy)
//   - `&literal_call()` (free-fn call — defense-in-depth)
//
// All emit `TypeError::BorrowOfNonPlace` with a populated `suggestion`
// field carrying the let-bind-then-borrow rewrite pattern (§2.5
// Direction B "print the FIX, not just the diagnosis").
//
// Pre-DEV-impl status: every i0052g_* test below is `#[ignore]`'d
// pending Wave-2 round 2 DEV merge at `check.rs:888-891`.
// ============================================================

const METHOD_STUBS_FOR_NON_COPY_BORROW: &str = concat!(
    "fn str_len(s: str) -> i64:\n    return 0\n",
    "fn split(s: str, sep: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n",
    "fn list_get_str(xs: list[str], i: i64) -> str:\n    return \"\"\n",
    "fn trim(s: str) -> str:\n    return \"\"\n",
);

#[test]
fn i0052g_01_borrow_str_split_non_copy_rejected() {
    // ADR-0052g §4.2 — `&s.split(",")` returns `list[str]` (non-Copy);
    // must emit `BorrowOfNonPlace` with FIX-text pointing at let-bind.
    let src = format!(
        "{METHOD_STUBS_FOR_NON_COPY_BORROW}fn read_xs(xs: list[str]) -> i64:\n    return 0\nfn f() -> i64:\n    let s: str = \"a,b\"\n    let r: i64 = read_xs(&s.split(\",\"))\n    return r\n",
    );
    must_reject("borrow-str-split-non-copy", &src, Cat::BorrowOfNonPlace);
}

#[test]
fn i0052g_02_borrow_method_returning_str_non_copy_rejected() {
    // ADR-0052g §4.2 — `&s.trim()` returns Str (non-Copy); must emit
    // `BorrowOfNonPlace` with FIX-text. Trim is a Str-table method.
    let src = format!(
        "{METHOD_STUBS_FOR_NON_COPY_BORROW}fn read_str(s: str) -> i64:\n    return 0\nfn f() -> i64:\n    let s: str = \"  hi  \"\n    let r: i64 = read_str(&s.trim())\n    return r\n",
    );
    must_reject("borrow-str-trim-non-copy", &src, Cat::BorrowOfNonPlace);
}

#[test]
fn i0052g_03_borrow_list_get_non_copy_rejected() {
    // ADR-0052g §4.2 — `&xs.get(0)` where `xs: list[str]` returns Str
    // (non-Copy); must emit `BorrowOfNonPlace` per the narrowed arm.
    //
    // Defense-in-depth note: free-fn-call borrow `&free_fn(x)` was the
    // original §4.3 test target but the parser §8 cap (post-0052f)
    // catches it at parse time — `must_reject` would panic on the
    // parse failure since the helper requires the snippet to parse.
    // Substituting a parser-admissible non-Copy method-form witness
    // keeps the test exercising the type-check rejection path.
    let src = format!(
        "{METHOD_STUBS_FOR_NON_COPY_BORROW}fn read_str(s: str) -> i64:\n    return 0\nfn f() -> i64:\n    let xs: list[str] = [\"a\", \"b\"]\n    let r: i64 = read_str(&xs.get(0))\n    return r\n",
    );
    must_reject("borrow-list-get-non-copy", &src, Cat::BorrowOfNonPlace);
}

// ============================================================
// ADR-0080 Phase-1a — class field tracking (ill-typed side)
// (i151..i154)
//
// ADR-0080 §1.1 ground-truth: the `Attr` arm (check.rs:1291) returns
// `self.fresh_var()` for any user-class instance base — verbatim
// comment "the static core does not yet track ADT fields"
// (check.rs:1260/1283). A fresh type variable UNIFIES WITH ANYTHING,
// so every field access below is WRONGLY ACCEPTED at HEAD (641e5f8).
// THAT mis-acceptance is the RED these tests exist to flip.
//
// Phase-1a: `check_class` (check.rs:757-762) records each class-body
// field (`let <name>: <ty>`) into a per-Adt field table, and the
// `Attr` arm returns the DECLARED field `Ty` (i64 / str) instead of a
// fresh var; an UNKNOWN field becomes a `TypeError` (not fresh_var).
//
// Class-field idiom + inferred-instance binding rationale: see the
// matching well_typed.rs w196..w199 header (an explicit `let s: Score`
// annotation is rejected at HEAD for the UNRELATED Alias↔Adt seam, so
// the instance is bound inferred to isolate field tracking).
//
// Two discriminating families:
//   (a) i151/i152 — a declared `i64` field used where a `str` is
//       required. Expected `TypeError::TypeMismatch` AFTER 1a (the
//       category EXISTS today, so these are LIVE `#[test]`s that FAIL
//       at HEAD = the visible RED, and turn green when DEV makes the
//       Attr arm yield the declared `i64`).
//   (b) i153/i154 — access of an UNDECLARED field (`s.nonexistent`).
//       Expected a TypeError WITH a §2.5-B FIX suggestion AFTER 1a.
//       No `Cat::UnknownField` / `TypeError::UnknownField` exists yet,
//       so per the i118+ NotHashable / DictSpreadNotSupported
//       precedent these are `#[ignore]`'d with a placeholder
//       `Cat::TypeMismatch`; DEV adds `TypeError::UnknownField`
//       (+ suggestion) + `Cat::UnknownField` + the `matches_cat` row,
//       then removes the `#[ignore]` and the test must turn green.

// ---- Family (a): i64 field used as str — TypeMismatch
//      (LIVE; PRE-IMPL: FAIL — fresh_var wrongly accepts)          ----

#[test]
fn i151_class_i64_field_bound_as_str_rejected() {
    // `let bad: str = s.rank` where `rank` is the declared `i64` field —
    // constitution §2.2 "no silent coercion": `i64` field flowing into
    // a `str` binding must be `TypeMismatch`. At HEAD `s.rank` is a
    // fresh var that unifies with `str`, so this is WRONGLY ACCEPTED
    // (the RED). Post-1a `s.rank` is `i64` and the unify with `str`
    // fails → TypeMismatch.
    must_reject(
        "class-i64-field-bound-as-str",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn f() -> str:\n    let s = Score()\n    let bad: str = s.rank\n    return bad\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i152_class_i64_field_in_str_concat_rejected() {
    // `s.name + s.rank` — str `+` i64 is a TypeMismatch (mirrors i08
    // str+int). At HEAD `s.rank` is a fresh var that unifies as the str
    // operand, so the concat is WRONGLY ACCEPTED (the RED). Post-1a
    // `s.rank` is `i64`, so the `str + i64` concat fails → TypeMismatch.
    must_reject(
        "class-i64-field-in-str-concat",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn f() -> str:\n    let s = Score()\n    return (s.name + s.rank)\n",
        Cat::TypeMismatch,
    );
}

// ---- Family (b): undeclared field access — UnknownField
//      (PRE-IMPL: FAIL — net-new variant; #[ignore] per i118+ idiom) ----
//
// When DEV lands `TypeError::UnknownField { field: String, adt: Ty,
// span, suggestion: Option<&'static str> }` (the §2.5-B FIX channel
// MUST be populated — e.g. "no field `nonexistent` on `Score`;
// declared fields: name, rank") + the `Cat::UnknownField` enum variant
// + the `matches_cat` row, the test author replaces the placeholder
// `Cat::TypeMismatch` with `Cat::UnknownField` and removes the
// `#[ignore]`; the test must then turn green.

#[test]
fn i153_class_undeclared_field_access_rejected() {
    // `s.nonexistent` where `Score` declares only `name` + `rank` —
    // post-1a accessing an undeclared field is a TypeError WITH a FIX
    // suggestion (§2.5-B). At HEAD the Attr arm returned fresh_var, which
    // unified with the `: i64` binding, so this was WRONGLY ACCEPTED;
    // ADR-0080 Phase-1a flips it to `UnknownField` (the declared-field
    // list is carried in the variant's `known_fields` + printed in the
    // Display message).
    must_reject(
        "class-undeclared-field-access",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn f() -> i64:\n    let s = Score()\n    let x: i64 = s.nonexistent\n    return x\n",
        Cat::UnknownField,
    );
}

#[test]
fn i154_class_undeclared_field_in_expr_rejected() {
    // `return s.missing` — undeclared field accessed bare (no binding
    // annotation to pin it). Post-1a still a TypeError (UnknownField);
    // the field-existence check is independent of how the result is
    // consumed. At HEAD fresh_var unified with the `-> i64` return type,
    // so WRONGLY ACCEPTED (the RED ADR-0080 Phase-1a flips).
    must_reject(
        "class-undeclared-field-in-expr",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn f() -> i64:\n    let s = Score()\n    return s.missing\n",
        Cat::UnknownField,
    );
}

// ============================================================
// ADR-0080 Phase-1b-i — class NAME in a type-annotation position
// resolves to the class's `Adt` (ill-typed guard side) (i155..i156)
//
// Companion to well_typed.rs w200..w202. Phase-1b-i makes a class-name
// annotation resolve to the class's `Ty::Adt` (the same id the ctor's
// `return_ty` carries) instead of the opaque `Ty::Alias` HEAD produces
// in `lower_named_type` (check.rs:2950-2956). These two tests guard the
// REJECTIONS that the fix MUST PRESERVE: resolving the annotation to an
// `Adt` must not start ACCEPTING a wrong-typed RHS.
//
// Both use `Cat::TypeMismatch` — these are nominal/primitive mismatches,
// no new error variant is needed (contrast i153/i154's `UnknownField`).
//
// HEAD STATUS (verified at e66dcfb via the parse→lower→check path):
// both ALREADY REJECT today, but for the pre-fix Alias reason
// (`expected: Alias(AliasId(2383749825), []), actual: <Int|Adt(1)>`).
// They are LIVE `#[test]`s (not `#[ignore]`) because the category is
// stable across the fix — only the `expected` side of the payload
// changes (`Alias` → `Adt`). The DISCRIMINATING value is i156: it is
// the nominal-distinctness guard — after the fix `Score` resolves to
// `Adt(Score)` and `Other()` is `Adt(Other)`, two DISTINCT `AdtId`s
// that must still NOT cross-unify (a regressing fix that resolved every
// class name to one shared `Adt`, or back to a by-name `Alias` that
// collides, would WRONGLY accept i156 — this test catches that).

#[test]
fn i155_noninstance_bound_to_class_type_rejected() {
    // (a) `let s: Score = 5` — a non-instance (`i64` literal) assigned to
    // a class-typed binding. constitution §2.2 "no silent coercion": an
    // `i64` may not satisfy a `Score` binding. At HEAD this rejects as
    // `Alias(Score)` vs `Int`; post-1b-i it rejects as `Adt(Score)` vs
    // `Int`. Either way TypeMismatch — the binding annotation, however
    // it lowers, must reject a primitive RHS.
    must_reject(
        "noninstance-bound-to-class-type",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn f() -> i64:\n    let s: Score = 5\n    return 0\n",
        Cat::TypeMismatch,
    );
}

#[test]
fn i156_cross_class_binding_rejected() {
    // (b) `let a: Score = Other()` — two DISTINCT classes. A `Score`-typed
    // binding must NOT accept an `Other` instance: distinct nominal
    // classes do not cross-unify. At HEAD this rejects as `Alias(Score)`
    // vs `Adt(Other)`; post-1b-i it MUST still reject as `Adt(Score)` vs
    // `Adt(Other)` — the nominal-distinctness the fix is required to
    // preserve (resolving the annotation to an `Adt` must keep each
    // class a distinct `AdtId`, not collapse to one shared/by-name type).
    must_reject(
        "cross-class-binding",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nclass Other:\n    let tag: i64 = 0\nfn f() -> i64:\n    let a: Score = Other()\n    return 0\n",
        Cat::TypeMismatch,
    );
}

// =====================================================================
// ADR-0080 Phase-1b-ii — validated-body refinement + route_validated
// callback-shape negatives (the §6 Phase-1 done-means ≥3 negatives,
// mirrored into the types harness so the FULL suite covers the surface).
// =====================================================================

#[test]
fn i157_non_fixed_where_predicate_rejected() {
    // A class field `where`-clause outside the fixed int-range grammar (an
    // arbitrary user-fn call) → `UnsupportedRefinement` with a §2.5-B FIX
    // (ADR-0080 Q6). The bare typed-field form (`rank: i64 where …`) parses
    // (Phase-1b-ii) so this exercises the type-check rejection, not a
    // parse error.
    must_reject(
        "non-fixed-where-predicate",
        "fn weird(x: i64) -> bool:\n    return True\nclass CreateScore:\n    name: str\n    rank: i64 where weird(self)\nfn main() -> i64:\n    return 0\n",
        Cat::UnsupportedRefinement,
    );
}

#[test]
fn i158_where_on_non_int_field_rejected() {
    // The int-range refinement applies only to an `i64` field; a `where`
    // bound on a `str` field is not the fixed grammar (Phase-1b-ii int
    // range only — `len(self)` on str is a Phase-2 surface). Rejected with
    // a FIX.
    must_reject(
        "where-on-str-field",
        "class CreateScore:\n    name: str where 0 <= self and self <= 10\n    rank: i64\nfn main() -> i64:\n    return 0\n",
        Cat::UnsupportedRefinement,
    );
}

#[test]
fn i159_route_validated_one_arg_handler_rejected() {
    // `app.route_validated("POST","/s", h)` where `h` is a 1-arg handler
    // (missing the `body` 2nd param) → CallbackSignatureMismatch (the
    // 2-arg validated-handler shape, ADR-0080 Q5). The arity check fires.
    must_reject(
        "route-validated-one-arg-handler",
        "import pit\nclass CreateScore:\n    name: str\n    rank: i64 where 0 <= self and self <= 100\nfn create_score(req: pit.Request) -> pit.Response:\n    return pit.text_response(201, \"ok\")\nfn main() -> i64:\n    let app = pit.App()\n    let _ = app.route_validated(\"POST\", \"/s\", create_score)\n    return 0\n",
        Cat::CallbackSignatureMismatch,
    );
}

#[test]
fn i160_route_validated_non_class_body_param_rejected() {
    // A 2nd param that is NOT a field-tracked body class (a bare `i64`) →
    // CallbackSignatureMismatch (the validated-body sentinel slot rejects a
    // non-class param, ADR-0080 §6 done-means third negative).
    must_reject(
        "route-validated-non-class-body",
        "import pit\nfn create_score(req: pit.Request, body: i64) -> pit.Response:\n    return pit.text_response(201, \"ok\")\nfn main() -> i64:\n    let app = pit.App()\n    let _ = app.route_validated(\"POST\", \"/s\", create_score)\n    return 0\n",
        Cat::CallbackSignatureMismatch,
    );
}

// =====================================================================
// ADR-0080 Phase-2 — STRING refinement NEGATIVES. The fixed str grammar
// (`len(self)` length / `pattern(self, "<re>")`) applies ONLY to a `str`
// field; a length/pattern form on a non-`str` field, or a malformed
// regex, is a `TypeError::UnsupportedRefinement` with a FIX (§2.5-B).
// =====================================================================

#[test]
fn i161_len_bound_on_int_field_rejected() {
    // `len(self)` is the str-LENGTH subject; on an `i64` field it is not the
    // int-range grammar (an `i64` field wants `lo <= self <= hi`, not
    // `len(self)`). Rejected with a FIX.
    must_reject(
        "len-bound-on-int-field",
        "class Body:\n    n: i64 where len(self) <= 10\nfn main() -> i64:\n    return 0\n",
        Cat::UnsupportedRefinement,
    );
}

#[test]
fn i162_pattern_on_int_field_rejected() {
    // `pattern(self, …)` is str-only; on an `i64` field it is rejected with
    // a FIX (the int-range grammar does not admit a pattern call).
    must_reject(
        "pattern-on-int-field",
        "class Body:\n    n: i64 where pattern(self, \".+\")\nfn main() -> i64:\n    return 0\n",
        Cat::UnsupportedRefinement,
    );
}

#[test]
fn i163_malformed_regex_in_pattern_rejected() {
    // A malformed regex in `pattern(self, "[")` (an unclosed character
    // class) fails to compile; ADR-0080 Phase-2 §2.5-B makes this a
    // BUILD-time `TypeError`, not a per-request runtime panic.
    must_reject(
        "malformed-regex-pattern",
        "class Body:\n    s: str where pattern(self, \"[\")\nfn main() -> i64:\n    return 0\n",
        Cat::UnsupportedRefinement,
    );
}

#[test]
fn i164_str_field_arbitrary_int_bound_rejected() {
    // A bare `0 <= self` int-range bound on a `str` field is NOT a str
    // refinement (the str grammar wants `len(self)` or `pattern`). Still
    // rejected with a FIX (mirrors i158 with the Phase-2 str path active).
    must_reject(
        "str-field-arbitrary-int-bound",
        "class Body:\n    s: str where 0 <= self and self <= 10\nfn main() -> i64:\n    return 0\n",
        Cat::UnsupportedRefinement,
    );
}

// =====================================================================
// ADR-0080 Phase-3a — f64 value-range refinement NEGATIVES. The fixed
// float-range grammar (`lo <= self <= hi`, inclusive `<=`/`>=` only)
// applies ONLY to an `f64` field. A wrong-shape predicate, a strict
// `<`/`>` bound (no clean inclusive rewrite for floats, D2), or an
// arbitrary fn call is a `TypeError::UnsupportedRefinement` with a FIX
// (§2.5-B). These MIRROR i161..i164 on the f64 base type.
// =====================================================================

#[test]
fn i165_len_bound_on_float_field_rejected() {
    // `len(self)` is the str-LENGTH subject; on an `f64` field it is not the
    // float-range grammar (an `f64` field wants `lo <= self <= hi`). Rejected
    // with a FIX. (Mirror of i161 on the f64 base type.)
    must_reject(
        "len-bound-on-float-field",
        "class Body:\n    x: f64 where len(self) <= 10\nfn main() -> i64:\n    return 0\n",
        Cat::UnsupportedRefinement,
    );
}

#[test]
fn i166_pattern_on_float_field_rejected() {
    // `pattern(self, …)` is str-only; on an `f64` field it is rejected with a
    // FIX (the float-range grammar does not admit a pattern call). (Mirror of
    // i162 on the f64 base type.)
    must_reject(
        "pattern-on-float-field",
        "class Body:\n    x: f64 where pattern(self, \".+\")\nfn main() -> i64:\n    return 0\n",
        Cat::UnsupportedRefinement,
    );
}

#[test]
fn i167_arbitrary_call_on_float_field_rejected() {
    // An arbitrary user-fn call (`weird(self)`) on an `f64` field is outside
    // the fixed float-range grammar → `UnsupportedRefinement` + FIX. (Mirror
    // of i157 on the f64 base type.)
    must_reject(
        "arbitrary-call-on-float-field",
        "fn weird(x: f64) -> bool:\n    return True\nclass Body:\n    x: f64 where weird(self)\nfn main() -> i64:\n    return 0\n",
        Cat::UnsupportedRefinement,
    );
}

#[test]
fn i168_strict_lt_bound_on_float_field_rejected() {
    // A STRICT `<` bound on an `f64` field is rejected (ADR-0080 Phase-3a D2):
    // unlike the integer grammar (which rewrites `S < N` to `<= N-1`), a float
    // strict bound has no clean inclusive ±1 rewrite (the reals are dense), so
    // the fixed grammar admits ONLY inclusive `<=`/`>=` and the §2.5-B FIX
    // steers the author to the inclusive spelling.
    // #161: assert the rendered FIX NAMES the f64 inclusive form (not just the
    // category) — `f64 float-range` + `dense` are unique to the f64 clause, so
    // a regression of the Display to i64-only turns this RED (mutation-verified).
    must_reject_with_msg(
        "strict-lt-bound-on-float-field",
        "class Body:\n    x: f64 where 0.0 <= self and self < 1.0\nfn main() -> i64:\n    return 0\n",
        Cat::UnsupportedRefinement,
        &["f64 float-range", "dense"],
    );
}

#[test]
fn i169_class_field_let_explicit_mismatched_value_rejected() {
    // #156 nested-object prerequisite-tightening (audit 2026-05-31): clearing
    // the no-initializer class-field wall narrowed `check_class`'s field-`let`
    // re-check skip to ONLY the synthetic-`None` default (a NON-scalar field,
    // whose `default_init_for_type` is `None`). A SCALAR field-`let` carrying an
    // explicit MISMATCHED value is STILL type-checked — `let z: str = 42`
    // (value `42`, not the synthetic `None`) is rejected with a TypeMismatch,
    // NOT silently masked. Pins the §2.5 compile-time-catch the earlier broad
    // skip (any Binding `let`) had lost; a regression to the broad skip turns
    // this RED.
    must_reject(
        "class-field-let-explicit-str-eq-int",
        "class C:\n    let z: str = 42\n\nfn main() -> i64:\n    return 0\n",
        Cat::TypeMismatch,
    );
}

// ============================================================
// ADR-0088 — `len(x)` on a NON-sized argument
//
// The Python-canonical free-function `len` accepts only SIZED types
// (str / list / dict). A number / bool argument is a compile-time error
// (§2.5-A). The §2.5-B FIX-text NAMES the accepted sized-type set and
// must NOT carry the pre-ADR-0088 misleading "expected Dict". The inline
// `fn len(d: dict[i64,i64]) -> i64` stub mirrors the PRELUDE shape so the
// `try_synth_len_builtin` special-case fires.
// ============================================================

/// PRELUDE `len` stub prefix for the rejection corpus.
const LEN_STUB_REJ: &str = "fn len(d: dict[i64, i64]) -> i64:\n    return 0\n";

#[test]
fn i170_len_of_int_rejected_with_sized_fix() {
    // `len(5)` — an i64 is not sized. The message names the accepted
    // sized types and does NOT say "expected Dict" (§2.5-B).
    must_reject_with_msg(
        "len-of-int",
        &format!("{LEN_STUB_REJ}fn main() -> i64:\n    return len(5)\n"),
        Cat::LenArgNotSized,
        &["len", "sized", "str", "list", "dict"],
    );
}

#[test]
fn i171_len_of_float_rejected() {
    // `len(3.0)` — an f64 is not sized.
    must_reject_with_msg(
        "len-of-float",
        &format!("{LEN_STUB_REJ}fn f(x: f64) -> i64:\n    return len(x)\n"),
        Cat::LenArgNotSized,
        &["len", "sized"],
    );
}

#[test]
fn i172_len_of_int_error_does_not_mention_dict_expectation() {
    // Explicit §2.5-B negative: the misleading pre-ADR-0088 "expected
    // Dict[?,?]" diagnostic must NOT appear (it steered the LLM toward a
    // dict). The corpus asserts the rendered Display has no "expected Dict".
    let src = format!("{LEN_STUB_REJ}fn main() -> i64:\n    return len(5)\n");
    let module = parse_str(&src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = lower(&module, &mut sess).expect("lower");
    let err = check(&hir).expect_err("len(5) must be rejected");
    let msg = err.to_string();
    assert!(
        !msg.contains("expected Dict") && !msg.contains("expected `Dict"),
        "§2.5-B: len(5) message must NOT say 'expected Dict'; got:\n{msg}"
    );
}

// ============================================================
// ADR-0089 §3/§4 rejection corpus — type-PRESERVING `abs(x)` rejects a
// NON-numeric arg (falls through to the canonical `TypeMismatch`, NO new
// variant), and 1-arg `range(stop)` does NOT loosen the 3-arg arity. The
// inline `abs` / `range` stubs mirror the PRELUDE shape so the special-
// cases fire.
// ============================================================

/// PRELUDE `abs` stub prefix for the rejection corpus (f64 signature).
const ABS_STUB_REJ: &str = "fn abs(x: f64) -> f64:\n    return 0.0\n";
/// PRELUDE `range` stub prefix for the rejection corpus.
const RANGE_STUB_REJ: &str =
    "fn range(start: i64, stop: i64) -> list[i64]:\n    let xs: list[i64] = []\n    return xs\n";

#[test]
fn i173_abs_of_str_rejected_type_mismatch() {
    // `abs("x")` — a non-numeric arg falls through to the `f64`-unify,
    // raising the canonical `TypeMismatch { expected: f64, found str }`
    // (NO new error variant — ADR-0089 reuses TypeMismatch).
    must_reject(
        "abs-of-str",
        &format!("{ABS_STUB_REJ}fn main() -> f64:\n    return abs(\"x\")\n"),
        Cat::TypeMismatch,
    );
}

#[test]
fn i174_range_three_args_rejected_arity() {
    // The 1-arg `range(stop)` special-case does NOT loosen the arity: a
    // 3-arg `range(a, b, c)` still hits the canonical `ArityMismatch`
    // (the PRELUDE `range` declares two params).
    must_reject(
        "range-three-args",
        &format!(
            "{RANGE_STUB_REJ}fn f() -> i64:\n    for i in range(0, 5, 1):\n        return i\n    return 0\n"
        ),
        Cat::ArityMismatch,
    );
}

// ============================================================
// ADR-0090 rejection corpus — the list-reducer builtins `min`/`max`/`sum`
// reject a NON-list arg (the canonical `NotIterable` variant, NO new
// error type) and do NOT silently accept the DEFERRED multi-scalar-arg
// form (which hits the canonical `ArityMismatch`). The inline stubs
// mirror the PRELUDE shape so the special-case fires.
// ============================================================

/// PRELUDE list-reducer stub prefix for the rejection corpus.
const REDUCE_STUB_REJ: &str = concat!(
    "fn min(xs: list[i64]) -> i64:\n    return 0\n",
    "fn max(xs: list[i64]) -> i64:\n    return 0\n",
    "fn sum(xs: list[i64]) -> i64:\n    return 0\n",
);

#[test]
fn i175_min_of_int_scalar_rejected_not_iterable() {
    // `min(5)` — a non-list (non-iterable) arg. Reuses the canonical
    // `NotIterable` variant (NO new error type — ADR-0090 §"reuse").
    must_reject(
        "min-of-int-scalar",
        &format!("{REDUCE_STUB_REJ}fn f() -> i64:\n    return min(5)\n"),
        Cat::NotIterable,
    );
}

#[test]
fn i176_sum_of_str_rejected_not_iterable() {
    // `sum("abc")` — a str is not a list-reducer input here (the bare
    // `sum` reduces a `list[T]`; CPython's str-iteration is out of scope).
    must_reject(
        "sum-of-str",
        &format!("{REDUCE_STUB_REJ}fn f() -> i64:\n    return sum(\"abc\")\n"),
        Cat::NotIterable,
    );
}

#[test]
fn i177_min_multi_nonnumeric_args_rejected_type_mismatch() {
    // ADR-0107 / F94 FLIP: the multi-scalar-arg form `min(1, 2, 3)` is NOW
    // ACCEPTED (the variadic scalar form — see well_typed.rs
    // `w237_min_max_variadic_int_args_returns_int`). What STILL rejects is a
    // NON-NUMERIC variadic call (`max("a", "b")`): the variadic arm
    // validates each arg is `Int`/`Float`, unifying a `str` arg against the
    // numeric target → canonical `TypeMismatch` (NO new variant). This
    // preserves a negative-corpus entry for the variadic path. (The
    // single-non-list `min(5)` reject is `i175`.)
    must_reject(
        "max-multi-nonnumeric-args",
        &format!("{REDUCE_STUB_REJ}fn f() -> i64:\n    return max(\"a\", \"b\")\n"),
        Cat::TypeMismatch,
    );
}

#[test]
fn i178_sum_of_str_list_rejected_type_mismatch() {
    // `sum(["a", "b"])` — a `list[str]` whose element type is neither
    // int nor float. The special-case unifies the elem against `i64`,
    // raising the canonical `TypeMismatch` (NO new variant).
    must_reject(
        "sum-of-str-list",
        &format!(
            "{REDUCE_STUB_REJ}fn f() -> i64:\n    let xs: list[str] = [\"a\", \"b\"]\n    return sum(xs)\n"
        ),
        Cat::TypeMismatch,
    );
}
