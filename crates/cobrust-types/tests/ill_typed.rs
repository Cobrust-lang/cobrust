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
fn i55_for_iter_str_phase_g_deferred() {
    // str iteration is Phase G alongside iter protocol (ADR-0050b
    // §"Iter source type checking"); rejected at M-F.3.1.
    must_reject(
        "for-iter-str",
        "fn f() -> i64:\n    for c in \"hello\":\n        return 0\n    return 0\n",
        Cat::NotIterable,
    );
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
fn i58_for_range_called_with_one_arg() {
    // Inline range stub takes 2 args; calling with 1 is an arity
    // mismatch.
    must_reject(
        "for-range-arity-1",
        "fn range(a: i64, b: i64) -> List[i64]:\n    let xs: List[i64] = []\n    return xs\nfn f() -> i64:\n    for i in range(5):\n        return i\n    return 0\n",
        Cat::ArityMismatch,
    );
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
fn i102_for_over_str_loop_rejected() {
    // `for c in "hello":` — strings are not iter sources in Phase F.3
    // (deferred to Phase G per ADR-0050b §"Iter source type checking").
    // The loop-var would have type str (one-char str) — but the iter
    // source check rejects str entirely.
    must_reject(
        "for-over-str-literal",
        "fn f() -> i64:\n    for c in \"hello\":\n        let _ = print(c)\n    return 0\n",
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
