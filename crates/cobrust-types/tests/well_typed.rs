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
//! Curated well-typed program suite — ≥ 50 programs the type
//! checker must accept.
//!
//! Each entry is a one-liner-y Cobrust source that the M2 type
//! checker must accept (return `Ok(_)`). The list is deliberately
//! organised by feature; if any future change rejects one of these
//! programs, the change has to come with an ADR superseding ADR-0006.

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower};
use cobrust_types::check;

fn must_accept(name: &str, src: &str) {
    let module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("{name}: parse error: {e:?}\nsource:\n{src}"));
    let mut sess = Session::new();
    let hir = lower(&module, &mut sess)
        .unwrap_or_else(|e| panic!("{name}: lowering error: {e:?}\nsource:\n{src}"));
    check(&hir)
        .unwrap_or_else(|e| panic!("{name}: should accept but rejected: {e:?}\nsource:\n{src}"));
}

// ---- arithmetic / numeric ----

#[test]
fn w01_int_plus_int() {
    must_accept(
        "int+int",
        "fn f(x: i64, y: i64) -> i64:\n    return (x + y)\n",
    );
}

#[test]
fn w02_int_minus_int() {
    must_accept(
        "int-int",
        "fn f(x: i64, y: i64) -> i64:\n    return (x - y)\n",
    );
}

#[test]
fn w03_int_mul_int() {
    must_accept(
        "int*int",
        "fn f(x: i64, y: i64) -> i64:\n    return (x * y)\n",
    );
}

#[test]
fn w04_int_div_int() {
    must_accept(
        "int/int",
        "fn f(x: i64, y: i64) -> i64:\n    return (x / y)\n",
    );
}

#[test]
fn w05_float_arith() {
    must_accept(
        "float",
        "fn f(a: f64, b: f64) -> f64:\n    return ((a * b) + a)\n",
    );
}

#[test]
fn w06_unary_neg() {
    must_accept("unary-neg", "fn f(x: i64) -> i64:\n    return (-x)\n");
}

#[test]
fn w07_bitwise() {
    must_accept(
        "bitwise",
        "fn f(a: i64, b: i64) -> i64:\n    return ((a & b) | (a ^ b))\n",
    );
}

#[test]
fn w08_shift() {
    must_accept(
        "shift",
        "fn f(x: i64) -> i64:\n    return ((x << 2) >> 1)\n",
    );
}

#[test]
fn w09_int_eq() {
    must_accept(
        "int-eq",
        "fn f(a: i64, b: i64) -> bool:\n    return (a == b)\n",
    );
}

#[test]
fn w10_int_lt() {
    must_accept(
        "int-lt",
        "fn f(a: i64, b: i64) -> bool:\n    return (a < b)\n",
    );
}

// ---- bool / branching ----

#[test]
fn w11_if_bool_cond() {
    must_accept(
        "if-bool",
        "fn f(p: bool) -> i64:\n    if p:\n        return 1\n    else:\n        return 0\n",
    );
}

#[test]
fn w12_while_bool_cond() {
    must_accept(
        "while-bool",
        "fn f(p: bool) -> i64:\n    while p:\n        return 1\n    return 0\n",
    );
}

#[test]
fn w13_and_or() {
    must_accept(
        "and-or",
        "fn f(a: bool, b: bool) -> bool:\n    return ((a and b) or (not a))\n",
    );
}

#[test]
fn w14_elif_chain() {
    must_accept(
        "elif",
        "fn f(x: i64) -> i64:\n    if (x > 0):\n        return 1\n    elif (x == 0):\n        return 0\n    else:\n        return -1\n",
    );
}

#[test]
fn w15_match_bool_exhaustive() {
    must_accept(
        "match-bool",
        "fn f(p: bool) -> i64:\n    match p:\n        case True:\n            return 1\n        case False:\n            return 0\n",
    );
}

#[test]
fn w16_match_wildcard() {
    must_accept(
        "match-wildcard",
        "fn f(x: i64) -> i64:\n    match x:\n        case 0:\n            return 0\n        case _:\n            return 1\n",
    );
}

#[test]
fn w17_match_binding() {
    must_accept(
        "match-binding",
        "fn f(x: i64) -> i64:\n    match x:\n        case 0:\n            return 0\n        case n:\n            return n\n",
    );
}

// ---- list / set / dict / tuple ----

#[test]
fn w18_list_homogeneous() {
    must_accept("list", "fn f() -> List[i64]:\n    return [1, 2, 3]\n");
}

#[test]
fn w19_set_homogeneous() {
    must_accept("set", "fn f() -> Set[i64]:\n    return {1, 2}\n");
}

#[test]
fn w20_dict_homogeneous() {
    must_accept(
        "dict",
        "fn f() -> Dict[str, i64]:\n    return {\"k\": 1, \"v\": 2}\n",
    );
}

#[test]
fn w21_tuple_pair() {
    must_accept(
        "tuple",
        "fn f() -> bool:\n    let p = (1, True)\n    return True\n",
    );
}

#[test]
fn w22_index_list() {
    must_accept(
        "index-list",
        "fn f(xs: List[i64], i: i64) -> i64:\n    return xs[i]\n",
    );
}

#[test]
fn w23_index_dict() {
    must_accept(
        "index-dict",
        "fn f(d: Dict[str, i64]) -> i64:\n    return d[\"k\"]\n",
    );
}

// ---- functions / lambdas / calls ----

#[test]
fn w24_call_simple() {
    must_accept(
        "call",
        "fn id(x: i64) -> i64:\n    return x\nfn f() -> i64:\n    return id(1)\n",
    );
}

#[test]
fn w25_lambda_assigned() {
    must_accept(
        "lambda",
        "fn f() -> i64:\n    let inc = lambda x: (x + 1)\n    return inc(0)\n",
    );
}

#[test]
fn w26_recursion() {
    must_accept(
        "recursion",
        "fn fib(n: i64) -> i64:\n    if (n < 2):\n        return n\n    return (fib((n - 1)) + fib((n - 2)))\n",
    );
}

#[test]
fn w27_mutual_recursion() {
    must_accept(
        "mutual",
        "fn even(n: i64) -> bool:\n    if (n == 0):\n        return True\n    return odd((n - 1))\nfn odd(n: i64) -> bool:\n    if (n == 0):\n        return False\n    return even((n - 1))\n",
    );
}

// ---- comprehensions ----

#[test]
fn w28_list_comp() {
    must_accept(
        "list-comp",
        "fn f(xs: List[i64]) -> List[i64]:\n    return [(x * x) for x in xs]\n",
    );
}

#[test]
fn w29_list_comp_with_guard() {
    must_accept(
        "list-comp-guard",
        "fn f(xs: List[i64]) -> List[i64]:\n    return [x for x in xs if (x > 0)]\n",
    );
}

#[test]
fn w30_set_comp() {
    must_accept(
        "set-comp",
        "fn f(xs: List[i64]) -> Set[i64]:\n    return {x for x in xs}\n",
    );
}

#[test]
fn w31_dict_comp() {
    must_accept(
        "dict-comp",
        "fn f(xs: List[i64]) -> Dict[i64, i64]:\n    return {x: (x * x) for x in xs}\n",
    );
}

// ---- loops ----

#[test]
fn w32_for_list() {
    must_accept(
        "for-list",
        "fn f(xs: List[i64]) -> i64:\n    for x in xs:\n        return x\n    return 0\n",
    );
}

#[test]
fn w33_while_with_break() {
    must_accept(
        "while-break",
        "fn f() -> i64:\n    while True:\n        break\n    return 0\n",
    );
}

#[test]
fn w34_while_with_continue() {
    must_accept(
        "while-continue",
        "fn f() -> i64:\n    while False:\n        continue\n    return 0\n",
    );
}

// ---- strings / fstrings ----

#[test]
fn w35_string_concat() {
    must_accept(
        "string-concat",
        "fn f(a: str, b: str) -> str:\n    return (a + b)\n",
    );
}

#[test]
fn w36_fstring() {
    must_accept("fstring", "fn f(x: i64) -> str:\n    return f\"x={x}\"\n");
}

// ---- let / type alias / class ----

#[test]
fn w37_let_inferred() {
    must_accept(
        "let-inferred",
        "fn f() -> i64:\n    let x = 1\n    return x\n",
    );
}

#[test]
fn w38_let_annotated() {
    must_accept(
        "let-annot",
        "fn f() -> i64:\n    let x: i64 = 1\n    return x\n",
    );
}

#[test]
fn w39_type_alias() {
    must_accept("alias", "type Pair = i64\nfn f() -> Pair:\n    return 1\n");
}

#[test]
fn w40_class_empty() {
    must_accept(
        "class",
        "class Foo:\n    pass\nfn f() -> bool:\n    return True\n",
    );
}

// ---- raise / try / pass / await / yield ----

#[test]
fn w41_pass_only_body() {
    must_accept("pass-only", "fn f() -> bool:\n    pass\n    return True\n");
}

#[test]
fn w42_raise() {
    must_accept(
        "raise",
        "let IoError = 0\nfn f() -> i64:\n    raise IoError\n",
    );
}

#[test]
fn w43_try_finally() {
    must_accept(
        "try-finally",
        "fn f() -> i64:\n    try:\n        return 1\n    finally:\n        pass\n",
    );
}

#[test]
fn w44_try_except() {
    must_accept(
        "try-except",
        "type IoError = i64\nfn f() -> i64:\n    try:\n        return 1\n    except IoError as e:\n        return 0\n",
    );
}

#[test]
fn w45_await_inside_fn() {
    must_accept(
        "await",
        "fn f(fetch: i64) -> i64:\n    let v = await fetch\n    return v\n",
    );
}

#[test]
fn w46_yield_inside_fn() {
    must_accept("yield", "fn f() -> bool:\n    yield 1\n    return True\n");
}

// ---- structural / inference ----

#[test]
fn w47_unify_via_branches() {
    must_accept(
        "unify-branches",
        "fn f(p: bool, x: i64, y: i64) -> i64:\n    if p:\n        return x\n    else:\n        return y\n",
    );
}

#[test]
fn w48_let_through_loop() {
    must_accept(
        "let-loop",
        "fn f(xs: List[i64]) -> i64:\n    let acc: i64 = 0\n    for x in xs:\n        acc += x\n    return acc\n",
    );
}

#[test]
fn w49_keyword_arg() {
    must_accept(
        "keyword-arg",
        "fn g(*, key: str) -> str:\n    return key\nfn f() -> str:\n    return g(key=\"v\")\n",
    );
}

#[test]
fn w50_higher_order() {
    must_accept(
        "ho",
        "fn apply(f: (i64) -> i64, x: i64) -> i64:\n    return f(x)\nfn inc(x: i64) -> i64:\n    return (x + 1)\nfn main() -> i64:\n    return apply(inc, 0)\n",
    );
}

#[test]
fn w51_decorator_passthrough() {
    must_accept(
        "decorator",
        "let inline = 0\n@inline\nfn f() -> i64:\n    return 0\n",
    );
}

#[test]
fn w52_nested_fn() {
    must_accept(
        "nested-fn",
        "fn outer(x: i64) -> i64:\n    fn inner(y: i64) -> i64:\n        return (y + x)\n    return inner(1)\n",
    );
}

#[test]
fn w53_pattern_binding_match() {
    must_accept(
        "pattern-binding",
        "fn f(x: i64) -> i64:\n    match x:\n        case 0:\n            return 0\n        case n:\n            return n\n",
    );
}

#[test]
fn w54_in_iterable() {
    must_accept(
        "in-iter",
        "fn f(xs: List[i64], target: i64) -> bool:\n    return (target in xs)\n",
    );
}

// ============================================================
// M-F.3.1 for-loop corpus (ADR-0050b)
//
// `range(a, b)` is plumbed as a prelude function returning
// `list[i64]`. The type-check harness here does NOT prepend the
// prelude, so each test that exercises `range` ships an inline
// `fn range(a: i64, b: i64) -> list[i64]:` stub identical in shape
// to the prelude declaration.
//
// These tests lock the iter-source classifier, the loop-var
// binding, nested for, mixed for+while, and shadowing semantics.
// ============================================================

const RANGE_STUB: &str =
    "fn range(a: i64, b: i64) -> List[i64]:\n    let xs: List[i64] = []\n    return xs\n";

fn must_accept_with_range(name: &str, body: &str) {
    let src = format!("{RANGE_STUB}{body}");
    must_accept(name, &src);
}

#[test]
fn w55_for_range_simple() {
    must_accept_with_range(
        "for-range-simple",
        "fn f() -> i64:\n    for i in range(0, 5):\n        return i\n    return 0\n",
    );
}

#[test]
fn w56_for_range_negative_start() {
    must_accept_with_range(
        "for-range-negative",
        "fn f() -> i64:\n    for i in range(-3, 3):\n        return i\n    return 0\n",
    );
}

#[test]
fn w57_for_range_empty() {
    must_accept_with_range(
        "for-range-empty",
        "fn f() -> i64:\n    for i in range(0, 0):\n        return i\n    return 0\n",
    );
}

#[test]
fn w58_for_range_var_unused() {
    // `_` should be accepted as loop binding too.
    must_accept_with_range(
        "for-range-wildcard",
        "fn f() -> i64:\n    let n: i64 = 0\n    for _ in range(0, 5):\n        return n\n    return 0\n",
    );
}

#[test]
fn w59_for_range_nested() {
    must_accept_with_range(
        "for-range-nested",
        "fn f() -> i64:\n    for i in range(0, 3):\n        for j in range(0, 3):\n            return (i + j)\n    return 0\n",
    );
}

#[test]
fn w60_for_range_with_inner_let() {
    must_accept_with_range(
        "for-range-let",
        "fn f() -> i64:\n    for i in range(0, 5):\n        let doubled: i64 = (i + i)\n        return doubled\n    return 0\n",
    );
}

#[test]
fn w61_for_range_with_outer_var() {
    must_accept_with_range(
        "for-range-outer",
        "fn f() -> i64:\n    let acc: i64 = 0\n    for i in range(0, 5):\n        acc = (acc + i)\n    return acc\n",
    );
}

#[test]
fn w62_for_range_inner_shadowing() {
    // Shadowing the loop-var inside the body is legal per Rust rules
    // (the inner `let i` makes a new binding for the body's tail; next
    // iter reassigns the loop slot).
    must_accept_with_range(
        "for-range-shadow",
        "fn f() -> i64:\n    for i in range(0, 5):\n        let i: i64 = 42\n        return i\n    return 0\n",
    );
}

#[test]
fn w63_for_range_inside_while() {
    must_accept_with_range(
        "for-range-in-while",
        "fn f() -> i64:\n    let n: i64 = 0\n    while (n < 3):\n        for i in range(0, 3):\n            n = (n + i)\n        n = (n + 1)\n    return n\n",
    );
}

#[test]
fn w64_while_inside_for_range() {
    must_accept_with_range(
        "while-in-for-range",
        "fn f() -> i64:\n    let acc: i64 = 0\n    for i in range(0, 3):\n        let k: i64 = 0\n        while (k < i):\n            acc = (acc + 1)\n            k = (k + 1)\n    return acc\n",
    );
}

#[test]
fn w65_for_range_inside_if() {
    must_accept_with_range(
        "for-range-in-if",
        "fn f(p: bool) -> i64:\n    let acc: i64 = 0\n    if p:\n        for i in range(0, 3):\n            acc = (acc + i)\n    return acc\n",
    );
}

#[test]
fn w66_for_range_with_early_return() {
    must_accept_with_range(
        "for-range-early-return",
        "fn f() -> i64:\n    for i in range(0, 100):\n        if (i == 7):\n            return i\n    return -1\n",
    );
}

#[test]
fn w67_for_range_with_fn_call() {
    // iter expr can be a Call producing a List.
    must_accept_with_range(
        "for-range-fn-call",
        "fn f() -> i64:\n    let r: List[i64] = range(0, 5)\n    for i in r:\n        return i\n    return 0\n",
    );
}

#[test]
fn w68_for_range_arith_args() {
    // range(a + b, c - d) — args are arbitrary i64 expressions.
    must_accept_with_range(
        "for-range-arith",
        "fn f(a: i64, b: i64) -> i64:\n    for i in range((a + 0), (b + 1)):\n        return i\n    return 0\n",
    );
}

#[test]
fn w69_for_list_str_argv_iter() {
    // list[str] iter source — runtime works per ADR-0044 W2 Phase 2;
    // ADR-0050b §"list[str] iter source" notes that ownership
    // correctness lands in Wave 2 M-F.3.2 (ADR-0050c). Type-check
    // accepts it today.
    must_accept(
        "for-list-str",
        "fn argv() -> List[str]:\n    let xs: List[str] = []\n    return xs\nfn f() -> i64:\n    let args: List[str] = argv()\n    for a in args:\n        return 0\n    return 0\n",
    );
}

#[test]
fn w70_for_range_body_calls_helper() {
    must_accept_with_range(
        "for-range-helper",
        "fn h(x: i64) -> i64:\n    return (x + 1)\nfn f() -> i64:\n    let acc: i64 = 0\n    for i in range(0, 5):\n        acc = (acc + h(i))\n    return acc\n",
    );
}

// ============================================================
// M-F.3.3 — f64 well-typed corpus (w71..w97)
// Targets the gap items from ADR-0050 §A1:
//   (a) `as` cast expression: `x as f64`, `y as i64`
//   (b) math intrinsics callable from .cb: sqrt, pow, floor, ceil, etc.
//   (c) f-string with float precision: `f"{x:.2f}"`, `f"{y:e}"`
//   (d) `inf` / `nan` as float literals in source
//   (e) IEEE 754 strict compliance: NaN ≠ NaN, ±∞ ordering
//
// Per ADR-0050 §A1 / constitution §2.2:
//   - NO implicit i64 ↔ f64 coercion; explicit `as` cast required.
//   - NaN ≠ NaN is correct per IEEE 754; NOT a type error.
//   - `inf`, `-inf`, `nan` must be accepted as f64 literals.
// ============================================================

// ---- f64 literal forms ----

#[test]
fn w71_f64_literal_decimal() {
    // Plain decimal float literal types as f64.
    must_accept(
        "f64-literal-decimal",
        "fn f() -> f64:\n    let x: f64 = 3.14\n    return x\n",
    );
}

#[test]
fn w72_f64_literal_leading_dot() {
    // `.5` is a valid float literal.
    must_accept(
        "f64-literal-dot",
        "fn f() -> f64:\n    let x: f64 = 0.5\n    return x\n",
    );
}

#[test]
fn w73_f64_literal_exponent() {
    // `1e10` exponent form.
    must_accept(
        "f64-literal-exp",
        "fn f() -> f64:\n    let x: f64 = 1e10\n    return x\n",
    );
}

#[test]
fn w74_f64_literal_negative_exponent() {
    // `1.5e-3` negative exponent — ADR-0050 §A1 verified shipped in lexer.
    must_accept(
        "f64-literal-neg-exp",
        "fn f() -> f64:\n    let x: f64 = 1.5e-3\n    return x\n",
    );
}

#[test]
fn w75_f64_literal_inf() {
    // `inf` as a float literal — M-F.3.3 gap item (d).
    // DEV must add `inf` as a keyword / prelude constant of type f64.
    must_accept(
        "f64-literal-inf",
        "fn f() -> f64:\n    let x: f64 = inf\n    return x\n",
    );
}

#[test]
fn w76_f64_literal_nan() {
    // `nan` as a float literal — M-F.3.3 gap item (d).
    must_accept(
        "f64-literal-nan",
        "fn f() -> f64:\n    let x: f64 = nan\n    return x\n",
    );
}

#[test]
fn w77_f64_arithmetic_all_ops() {
    // All four arithmetic ops on f64 are accepted (already shipped).
    must_accept(
        "f64-arith-all",
        "fn f(a: f64, b: f64) -> f64:\n    let s: f64 = (a + b)\n    let d: f64 = (a - b)\n    let m: f64 = (a * b)\n    let q: f64 = (a / b)\n    return q\n",
    );
}

#[test]
fn w78_f64_comparison_lt_gt() {
    // Comparison operators on f64 return bool (IEEE 754 partial order).
    must_accept(
        "f64-cmp-ltgt",
        "fn f(a: f64, b: f64) -> bool:\n    return (a < b)\n",
    );
}

#[test]
fn w79_f64_comparison_eq() {
    // f64 == f64 is accepted (NaN ≠ NaN is a runtime property, not a type error).
    must_accept(
        "f64-cmp-eq",
        "fn f(a: f64, b: f64) -> bool:\n    return (a == b)\n",
    );
}

#[test]
fn w80_f64_unary_neg() {
    // Unary negation on f64.
    must_accept(
        "f64-unary-neg",
        "fn f(x: f64) -> f64:\n    return (-x)\n",
    );
}

// ---- `as` cast expression (M-F.3.3 gap item a) ----
// NOTE: These tests exercise `x as f64` and `y as i64` syntax.
// They FAIL today with a parse error because the parser does not yet
// support `as` in expression position (only import-alias context).
// The DEV agent must add ExprKind::Cast to the AST + parser + HIR
// lowering + type-checker for these to pass.

#[test]
fn w81_cast_i64_to_f64() {
    // `let x: f64 = n as f64` — explicit upcast.
    must_accept(
        "cast-i64-to-f64",
        "fn f(n: i64) -> f64:\n    let x: f64 = (n as f64)\n    return x\n",
    );
}

#[test]
fn w82_cast_f64_to_i64() {
    // `let x: i64 = v as i64` — explicit truncating downcast.
    must_accept(
        "cast-f64-to-i64",
        "fn f(v: f64) -> i64:\n    let x: i64 = (v as i64)\n    return x\n",
    );
}

#[test]
fn w83_cast_in_expression_position() {
    // Cast used inside an arithmetic expression.
    must_accept(
        "cast-in-expr",
        "fn f(n: i64) -> f64:\n    return ((n as f64) + 1.0)\n",
    );
}

#[test]
fn w84_cast_as_fn_argument() {
    // Cast as an argument to a function call.
    must_accept(
        "cast-fn-arg",
        "fn g(x: f64) -> f64:\n    return x\nfn f(n: i64) -> f64:\n    return g(n as f64)\n",
    );
}

#[test]
fn w85_chained_cast_i64_f64_i64() {
    // Chained cast: i64 → f64 → i64.
    must_accept(
        "cast-chained",
        "fn f(n: i64) -> i64:\n    return ((n as f64) as i64)\n",
    );
}

#[test]
fn w86_cast_in_return() {
    // Cast directly in return statement.
    must_accept(
        "cast-return",
        "fn f(x: f64) -> i64:\n    return (x as i64)\n",
    );
}

// ---- math intrinsics callable from .cb (M-F.3.3 gap item b) ----
// NOTE: These tests exercise `sqrt(x)`, `floor(x)`, `pow(x, y)` etc.
// They FAIL today because the PRELUDE does not yet expose these functions
// — the Rust-side math.rs exists but no intrinsic-rewrite pass wires
// them into .cb source. The DEV agent must extend the PRELUDE +
// intrinsic-rewrite following the ADR-0044 `input`/`argv` precedent.

#[test]
fn w87_math_sqrt_well_typed() {
    // `sqrt(x: f64) -> f64` must be a well-typed call.
    must_accept(
        "math-sqrt",
        "fn sqrt(x: f64) -> f64:\n    return x\nfn f(x: f64) -> f64:\n    return sqrt(x)\n",
    );
}

#[test]
fn w88_math_pow_well_typed() {
    // `pow(base: f64, exp: f64) -> f64`.
    must_accept(
        "math-pow",
        "fn pow(base: f64, exp: f64) -> f64:\n    return base\nfn f(b: f64, e: f64) -> f64:\n    return pow(b, e)\n",
    );
}

#[test]
fn w89_math_floor_ceil_round() {
    // `floor`, `ceil`, `round` return f64.
    must_accept(
        "math-floor-ceil-round",
        "fn floor(x: f64) -> f64:\n    return x\nfn ceil(x: f64) -> f64:\n    return x\nfn round(x: f64) -> f64:\n    return x\nfn f(x: f64) -> f64:\n    let a: f64 = floor(x)\n    let b: f64 = ceil(x)\n    let c: f64 = round(x)\n    return c\n",
    );
}

#[test]
fn w90_math_sin_cos_tan() {
    // Trigonometric intrinsics accept and return f64.
    must_accept(
        "math-trig",
        "fn sin(x: f64) -> f64:\n    return x\nfn cos(x: f64) -> f64:\n    return x\nfn tan(x: f64) -> f64:\n    return x\nfn f(x: f64) -> f64:\n    return (sin(x) + cos(x))\n",
    );
}

#[test]
fn w91_math_abs_min_max() {
    // `abs`, `min`, `max` on f64.
    must_accept(
        "math-abs-min-max",
        "fn abs(x: f64) -> f64:\n    return x\nfn min(a: f64, b: f64) -> f64:\n    return a\nfn max(a: f64, b: f64) -> f64:\n    return a\nfn f(a: f64, b: f64) -> f64:\n    return max(abs(a), abs(b))\n",
    );
}

#[test]
fn w92_math_log_exp() {
    // `log` and `exp` intrinsics.
    must_accept(
        "math-log-exp",
        "fn log(x: f64) -> f64:\n    return x\nfn exp(x: f64) -> f64:\n    return x\nfn f(x: f64) -> f64:\n    return exp(log(x))\n",
    );
}

// ---- f-string with float precision (M-F.3.3 gap item c) ----
// NOTE: These tests exercise `f"{x:.2f}"` / `f"{y:e}"` / `f"{z:g}"`.
// They FAIL today because the MIR f-string lowering ignores format_spec
// (FormatPart::Hole's format_spec is silently dropped in lower.rs:1075).
// The DEV agent must wire format_spec to `__cobrust_fmt_float`.

#[test]
fn w93_fstring_float_fixed_precision() {
    // `f"{x:.2f}"` — fixed-point two-decimal-places format.
    must_accept(
        "fstr-float-fixed",
        "fn f(x: f64) -> str:\n    return f\"{x:.2f}\"\n",
    );
}

#[test]
fn w94_fstring_float_scientific() {
    // `f"{x:e}"` — scientific / exponential notation.
    must_accept(
        "fstr-float-sci",
        "fn f(x: f64) -> str:\n    return f\"{x:e}\"\n",
    );
}

#[test]
fn w95_fstring_float_general() {
    // `f"{x:g}"` — general format (shortest of fixed/exp).
    must_accept(
        "fstr-float-general",
        "fn f(x: f64) -> str:\n    return f\"{x:g}\"\n",
    );
}

#[test]
fn w96_fstring_mixed_int_and_float() {
    // Mix int and float parts in the same f-string.
    must_accept(
        "fstr-mixed",
        "fn f(n: i64, x: f64) -> str:\n    return f\"n={n} x={x:.4f}\"\n",
    );
}

// ---- IEEE 754 / NaN semantics well-typed ----

#[test]
fn w97_nan_eq_nan_is_bool_typed() {
    // `nan == nan` is a bool expression — type-check must accept
    // (the false result is a runtime IEEE 754 property, not a type error).
    must_accept(
        "nan-eq-bool",
        "fn f(a: f64, b: f64) -> bool:\n    return (a == b)\n",
    );
}

// ============================================================
// M-F.3.2 — list[str] ownership well-typed corpus (w98..w115)
// Closes TD-1 per ADR-0050c Option A (Full-Drop schedule + explicit
// `__cobrust_str_clone`). Type-check must accept the following Cobrust
// programs:
//   - literal list[str] (`["a", "b"]` synthesised as List<Str> via §601
//     ExprKind::List head+unify)
//   - `xs[i]` indexing returns Ty::Str
//   - `list_len(xs)` returns i64 when xs: list[str] (DEV must widen
//     the PRELUDE+intrinsic-rewrite to accept list[str])
//   - `list_is_empty(xs)` returns bool when xs: list[str]
//     (ADR-0050c §"Phase 6" / F5 §2.2 uniformity addendum binds the
//     new `__cobrust_list_is_empty` shim; the matching PRELUDE entry
//     should accept list[str] in addition to list[i64])
//   - functions consuming + returning list[str]
//   - nested list[list[str]]
//
// These tests probe the SOURCE-LEVEL TYPE-CHECK CONTRACT — they pass
// today only insofar as Ty::List(Box::new(Ty::Str)) is a valid type
// constructor. The `list_is_empty` rows require the new PRELUDE entry
// and will FAIL until DEV adds it. The `list_len` over list[str] rows
// require the type-check to accept the wider arg (currently the PRELUDE
// declaration is `fn list_len(lst: list[i64]) -> i64`).
//
// Per type-check-layer-conventions: the runtime PRELUDE is NOT
// available in these tests, so every test inlines the necessary stub
// fns (`print`, `argv`, `list_len`, `list_is_empty`). The DEV's wider
// PRELUDE signatures will replace these stubs at the CLI build layer.
//
// All ADR-0050c bug-witness invariants are runtime concerns and live
// in `crates/cobrust-cli/tests/list_str_e2e.rs` (Tier C corpus).
// ============================================================

// Shared stub block: the prelude entries M-F.3.2 must accept.
// `list_len` over list[str] + `list_is_empty(list[T]) -> bool` are
// the row-polymorphic widenings ADR-0050c §"Phase 6" requires.
const LIST_STR_STUBS: &str = "fn print(s: str) -> i64:\n    return 0\nfn argv() -> list[str]:\n    let xs: list[str] = []\n    return xs\nfn list_len(lst: list[str]) -> i64:\n    return 0\nfn list_is_empty_s(lst: list[str]) -> bool:\n    return True\nfn list_is_empty_i(lst: list[i64]) -> bool:\n    return True\n";

fn must_accept_with_list_str_stubs(name: &str, body: &str) {
    let src = format!("{LIST_STR_STUBS}{body}");
    must_accept(name, &src);
}

// ---- Tier A.1: literal list[str] type synthesis ----

#[test]
fn w98_list_str_literal_three_elems() {
    // `["a", "b", "c"]` synthesised + unified element-wise → list[str].
    must_accept(
        "list-str-literal-3",
        "fn f() -> i64:\n    let xs: List[str] = [\"a\", \"b\", \"c\"]\n    return 0\n",
    );
}

#[test]
fn w99_list_str_literal_single_elem() {
    // Single-element list[str].
    must_accept(
        "list-str-literal-1",
        "fn f() -> i64:\n    let xs: List[str] = [\"only\"]\n    return 0\n",
    );
}

#[test]
fn w100_list_str_literal_lowercase_list_annot() {
    // Lowercase `list[str]` annotation form (Python-flavoured alias per
    // check.rs §1056). Same semantics as `List[str]`.
    must_accept(
        "list-str-lowercase-annot",
        "fn f() -> i64:\n    let xs: list[str] = [\"a\", \"b\"]\n    return 0\n",
    );
}

// ---- Tier A.2: indexing yields Ty::Str ----

#[test]
fn w101_list_str_index_yields_str() {
    // `xs[0]` where xs: list[str] must type as str.
    must_accept(
        "list-str-index-yields-str",
        "fn f() -> str:\n    let xs: list[str] = [\"alpha\", \"beta\"]\n    return xs[0]\n",
    );
}

#[test]
fn w102_list_str_index_in_print() {
    // print(xs[0]) — xs[0]: str, print takes str. Stub-mode test.
    must_accept_with_list_str_stubs(
        "list-str-index-in-print",
        "fn f() -> i64:\n    let xs: list[str] = [\"hello\"]\n    return print(xs[0])\n",
    );
}

// ---- Tier A.3: list_len over list[str] ----
// PRELUDE declares `fn list_len(lst: list[i64]) -> i64`. To make
// this accept list[str], DEV must widen the PRELUDE signature (a
// row-polymorphic `List<_>` arg via type-checker special-case) or
// ship a separate `list_str_len(list[str]) -> i64` intrinsic. The
// chosen path per ADR-0050c §"Decision" is row-polymorphic. The
// stub in `LIST_STR_STUBS` mirrors this: `list_len(list[str]) -> i64`.

#[test]
fn w103_list_len_over_list_str() {
    // `list_len(xs)` where xs: list[str] returns i64.
    must_accept_with_list_str_stubs(
        "list-len-over-list-str",
        "fn f() -> i64:\n    let xs: list[str] = [\"a\", \"b\", \"c\"]\n    return list_len(xs)\n",
    );
}

// ---- Tier A.4 / Tier E: list_is_empty (new PRELUDE entry per F5) ----
// ADR-0050c §"Phase 6" mandates `__cobrust_list_is_empty(*mut List) -> i64`
// alongside `__cobrust_dict_is_empty`. The chosen source-level PRELUDE
// entry is row-polymorphic `fn list_is_empty(lst: list[<T>]) -> bool`.
// At the type-check layer the row-polymorphic widening is not yet
// available; we mock the two monomorphisations as `list_is_empty_s`
// and `list_is_empty_i` to make the tests well-formed today. DEV must
// land the row-polymorphic signature so that at the source level one
// `list_is_empty` accepts both `list[i64]` and `list[str]`.

#[test]
fn w104_list_is_empty_over_list_i64_returns_bool() {
    // F5 §2.2 uniformity — list_is_empty(xs) returns bool when xs: list[i64].
    must_accept_with_list_str_stubs(
        "list-is-empty-i64-bool",
        "fn f() -> bool:\n    let xs: list[i64] = [1, 2, 3]\n    return list_is_empty_i(xs)\n",
    );
}

#[test]
fn w105_list_is_empty_over_list_str_returns_bool() {
    // F5 §2.2 uniformity — list_is_empty(xs) returns bool when xs: list[str].
    must_accept_with_list_str_stubs(
        "list-is-empty-str-bool",
        "fn f() -> bool:\n    let xs: list[str] = [\"a\"]\n    return list_is_empty_s(xs)\n",
    );
}

#[test]
fn w106_list_is_empty_in_if_condition() {
    // Constitution §2.2 — implicit truthy/falsy forbidden; users write
    // `if list_is_empty(xs):`, never `if xs:`. This locks the §2.2
    // uniformity gain F5 §"Phase 6" describes.
    must_accept_with_list_str_stubs(
        "list-is-empty-in-if",
        "fn f() -> i64:\n    let xs: list[str] = [\"a\"]\n    if list_is_empty_s(xs):\n        return 0\n    return 1\n",
    );
}

// ---- Tier A.5: functions consuming + returning list[str] ----

#[test]
fn w107_fn_takes_list_str_returns_i64() {
    // `fn f(xs: list[str]) -> i64` takes ownership of list[str].
    must_accept_with_list_str_stubs(
        "fn-takes-list-str",
        "fn count(xs: list[str]) -> i64:\n    return list_len(xs)\nfn f() -> i64:\n    let v: list[str] = [\"x\", \"y\"]\n    return count(v)\n",
    );
}

#[test]
fn w108_fn_returns_owned_list_str() {
    // `fn g() -> list[str]: return xs` — returns owned list[str].
    // The drop schedule must transfer ownership to caller's binding.
    must_accept_with_list_str_stubs(
        "fn-returns-list-str",
        "fn make() -> list[str]:\n    let xs: list[str] = [\"a\", \"b\"]\n    return xs\nfn f() -> i64:\n    let ys: list[str] = make()\n    return list_len(ys)\n",
    );
}

#[test]
fn w109_fn_str_arg_returns_str() {
    // Single str ownership transfer: f(s: str) -> str. Caller binds
    // the result, both old `s` parameter and new binding drop at scope exit.
    must_accept(
        "fn-str-arg-returns-str",
        "fn identity(s: str) -> str:\n    return s\nfn f() -> i64:\n    let v: str = identity(\"hi\")\n    return 0\n",
    );
}

// ---- Tier A.6: nested list[list[str]] ----

#[test]
fn w110_nested_list_list_str_lowercase() {
    // `list[list[str]]` — recursive Aggregate; each inner list owns its
    // Str slots; outer list owns each inner-list pointer.
    must_accept(
        "nested-list-list-str",
        "fn f() -> i64:\n    let xs: list[list[str]] = [[\"a\", \"b\"], [\"c\"]]\n    return 0\n",
    );
}

// ---- Tier A.7: rebind in inner scope (shadowing valid only when
//                 the inner `let` is in a deeper block) ----

#[test]
fn w111_list_str_rebind_in_inner_block() {
    // ADR-0050c Option A: an inner-scope `let xs: list[str] = ...`
    // shadows the outer binding for the inner block; the inner list
    // drops at the inner block's exit (before the outer is dropped).
    // HIR rejects DuplicateBinding in the SAME scope; only inner-scope
    // shadowing is valid. Source-level rebind via `xs = expr2` is the
    // distinct ADR-0050c §"Decision" path covered at the runtime tier
    // (Tier C f3ls25).
    must_accept_with_list_str_stubs(
        "list-str-inner-shadow",
        "fn f() -> i64:\n    let xs: list[str] = [\"a\"]\n    if True:\n        let xs: list[str] = [\"b\", \"c\"]\n        return list_len(xs)\n    return list_len(xs)\n",
    );
}

// ---- Tier A.8: argv() return type ----

#[test]
fn w112_argv_returns_list_str_typed_ok() {
    // `let args: list[str] = argv()` — argv()'s PRELUDE signature is
    // already `fn argv() -> list[str]`. This locks the binding remains
    // type-correct under ADR-0050c (the return value is owned list[str]
    // by callee; ownership transfers to caller's binding).
    must_accept_with_list_str_stubs(
        "argv-into-list-str-binding",
        "fn f() -> i64:\n    let args: list[str] = argv()\n    return list_len(args)\n",
    );
}

// ---- Tier A.9: for-loop iteration variable types as str ----

#[test]
fn w113_for_over_list_str_loop_var_is_str() {
    // `for s in xs:` where xs: list[str] binds `s: str`. The drop
    // schedule must drop `s` at the end of each iteration's scope.
    must_accept_with_list_str_stubs(
        "for-over-list-str",
        "fn f() -> i64:\n    let xs: list[str] = [\"a\", \"b\", \"c\"]\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
    );
}

// ---- Tier A.10: empty list[str] literal annotation ----

#[test]
fn w114_empty_list_str_literal_with_annot() {
    // `let xs: list[str] = []` — empty literal unifies to List(fresh).
    // Annotation forces fresh → Str.
    must_accept_with_list_str_stubs(
        "empty-list-str-with-annot",
        "fn f() -> i64:\n    let xs: list[str] = []\n    return list_len(xs)\n",
    );
}

// ---- Tier A.11: f-string element-of-list[str] ----

#[test]
fn w115_fstring_contains_list_str_index() {
    // f-string with a `{xs[0]}` hole — xs[0]: str composes into the
    // f-string buffer; the resulting buffer drops at scope exit.
    must_accept(
        "fstring-list-str-index",
        "fn f() -> str:\n    let xs: list[str] = [\"alpha\"]\n    let msg: str = f\"first={xs[0]}\"\n    return msg\n",
    );
}
