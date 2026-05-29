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
    must_accept("f64-unary-neg", "fn f(x: f64) -> f64:\n    return (-x)\n");
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

// ============================================================
// Tier A — Dict literal + indexing well-typed corpus
// (ADR-0050d sub-sprint a parser/AST/HIR/types surface lock).
//
// Verifies that the existing scaffolding (already 60% on main per
// ADR-0050d §A2) cleanly accepts the documented surface:
//   - `Dict[K, V]` annotation
//   - `{}` empty + `{k:v, ...}` literal
//   - `d[k]` read
//   - `key in d` / `key not in d`
//   - `len(d)` and `dict.is_empty()` (Decision 5 + 5-addendum)
//   - `for k in d:` / `.keys()` / `.values()` / `.items()` (Decision 6)
//   - Nested dict: `Dict[K, Dict[K2, V]]`
//   - Type params: `K ∈ {i64, str}` × `V ∈ {i64, str, list, dict}`
//   - Insertion-order semantics implied at type level
//   - Rebind via `d[k] = v` (Decision 3)
//
// Tests w116..w145 cover the type-checker surface only. End-to-end
// program execution (with M12.x stub or future indexmap backing) is
// the responsibility of Tier C / Tier D codegen-side corpora — see
// `dict_e2e.rs`.
//
// Stubs: most tests stand alone with PRELUDE; tests that exercise
// `dict.is_empty()` or `.keys()` / `.values()` / `.items()` /
// `.get()` / `.copy()` use a `DICT_METHOD_STUBS` helper that
// declares stubs at the SAME shape sub-sprint b will accept as
// intrinsic-recognised methods. The stubs let TEST corpus land
// before DEV impl ships; DEV will graduate the stubs to real
// intrinsic-method recognition without invalidating any test.
//
// Test name pattern: `wNNN_dict_<scenario>`.
// ============================================================

// Shared stub block: minimal dict-method intrinsics expressed as
// free-fn stubs. ADR-0050d Decision 5-addendum + sub-sprint e wire
// these as intrinsic-rewrite methods; pre-impl, the corpus uses the
// free-fn shape so the type checker has signatures to consult.
//
// `dict_is_empty(d)` mirrors `list_is_empty(lst)` per
// `__cobrust_list_len == 0` precedent. `dict_keys`/`dict_values`/
// `dict_items` return list shapes for sub-sprint e iteration desugar.
const DICT_METHOD_STUBS: &str = concat!(
    "fn dict_is_empty_si(d: Dict[str, i64]) -> bool:\n    return True\n",
    "fn dict_is_empty_ii(d: Dict[i64, i64]) -> bool:\n    return True\n",
    "fn dict_len_si(d: Dict[str, i64]) -> i64:\n    return 0\n",
    "fn dict_len_ii(d: Dict[i64, i64]) -> i64:\n    return 0\n",
    "fn dict_keys_si(d: Dict[str, i64]) -> List[str]:\n    let r: List[str] = []\n    return r\n",
    "fn dict_values_si(d: Dict[str, i64]) -> List[i64]:\n    let r: List[i64] = []\n    return r\n",
    "fn dict_get_si(d: Dict[str, i64], k: str) -> i64:\n    return 0\n",
);

fn must_accept_with_dict_stubs(name: &str, body: &str) {
    let src = format!("{DICT_METHOD_STUBS}{body}");
    must_accept(name, &src);
}

// ---- Tier A.1: empty dict literal + parametric inference ----

#[test]
fn w116_dict_empty_literal_annot_str_i64() {
    // Empty `{}` synthesises `Ty::Dict(?K, ?V)`; the `Dict[str, i64]`
    // annotation pins K=str, V=i64. ADR-0050d Decision 1A — `{}` is
    // dict, not set, per parser.rs:1473-1481 already-existing shape.
    must_accept(
        "dict-empty-annot-str-i64",
        "fn f() -> Dict[str, i64]:\n    let d: Dict[str, i64] = {}\n    return d\n",
    );
}

#[test]
fn w117_dict_empty_literal_annot_i64_i64() {
    // Empty literal with `Dict[i64, i64]` annotation — covers the
    // M12.x stub-typed shape (k_size=8, v_size=8).
    must_accept(
        "dict-empty-annot-i64-i64",
        "fn f() -> Dict[i64, i64]:\n    let d: Dict[i64, i64] = {}\n    return d\n",
    );
}

#[test]
fn w118_dict_empty_literal_annot_str_str() {
    // `Dict[str, str]` — both K and V are heap-pointer Strs. Sub-sprint
    // d's str_str shim shape; type checker accepts pre-impl.
    must_accept(
        "dict-empty-annot-str-str",
        "fn f() -> Dict[str, str]:\n    let d: Dict[str, str] = {}\n    return d\n",
    );
}

#[test]
fn w119_dict_empty_literal_annot_i64_str() {
    // `Dict[i64, str]` — i64 key, str value. Sub-sprint d's i64_str
    // shape. Type checker accepts.
    must_accept(
        "dict-empty-annot-i64-str",
        "fn f() -> Dict[i64, str]:\n    let d: Dict[i64, str] = {}\n    return d\n",
    );
}

// ---- Tier A.2: single-entry literal ----

#[test]
fn w120_dict_single_entry_str_i64() {
    // `{"a": 1}` — synth K=str, V=i64 from the entry.
    must_accept(
        "dict-single-str-i64",
        "fn f() -> Dict[str, i64]:\n    let d: Dict[str, i64] = {\"a\": 1}\n    return d\n",
    );
}

#[test]
fn w121_dict_single_entry_i64_str() {
    // `{1: "a"}` — synth K=i64, V=str.
    must_accept(
        "dict-single-i64-str",
        "fn f() -> Dict[i64, str]:\n    let d: Dict[i64, str] = {1: \"a\"}\n    return d\n",
    );
}

#[test]
fn w122_dict_single_entry_str_str() {
    // `{"k": "v"}` — both str.
    must_accept(
        "dict-single-str-str",
        "fn f() -> Dict[str, str]:\n    let d: Dict[str, str] = {\"k\": \"v\"}\n    return d\n",
    );
}

// ---- Tier A.3: multi-entry literal with homogeneous K, V ----

#[test]
fn w123_dict_multi_entry_str_i64_three() {
    // Three-entry literal `{"a":1, "b":2, "c":3}`; check.rs:651 unifies
    // K = str and V = i64 entry-wise.
    must_accept(
        "dict-multi-str-i64",
        "fn f() -> Dict[str, i64]:\n    let d: Dict[str, i64] = {\"a\": 1, \"b\": 2, \"c\": 3}\n    return d\n",
    );
}

#[test]
fn w124_dict_multi_entry_i64_i64_five() {
    // Five-entry i64,i64 literal — the M12.x stub-typed shape.
    must_accept(
        "dict-multi-i64-i64",
        "fn f() -> Dict[i64, i64]:\n    let d: Dict[i64, i64] = {1: 10, 2: 20, 3: 30, 4: 40, 5: 50}\n    return d\n",
    );
}

// ---- Tier A.4: indexing `d[k]` read ----

#[test]
fn w125_dict_index_str_key_yields_i64_value() {
    // `d["a"]` where `d: Dict[str, i64]` types as i64. Already wired
    // at check.rs:737 (Ty::Dict + IndexKind::Expr → unify K with index,
    // return V). ADR-0050d Decision 2A.
    must_accept(
        "dict-index-str-yields-i64",
        "fn f(d: Dict[str, i64]) -> i64:\n    return d[\"a\"]\n",
    );
}

#[test]
fn w126_dict_index_i64_key_yields_str_value() {
    // `d[1]` where `d: Dict[i64, str]` types as str.
    must_accept(
        "dict-index-i64-yields-str",
        "fn f(d: Dict[i64, str]) -> str:\n    return d[1]\n",
    );
}

#[test]
fn w127_dict_index_from_literal_then_read() {
    // Inline build + read: `{"a":1}["a"]` types as i64.
    must_accept(
        "dict-literal-then-index",
        "fn f() -> i64:\n    let d: Dict[str, i64] = {\"a\": 1, \"b\": 2}\n    return d[\"a\"]\n",
    );
}

#[test]
fn w128_dict_index_into_expr_arith() {
    // `d["a"] + d["b"]` — both reads are V-typed, arith on i64.
    must_accept(
        "dict-index-arith",
        "fn f() -> i64:\n    let d: Dict[str, i64] = {\"a\": 10, \"b\": 20}\n    return (d[\"a\"] + d[\"b\"])\n",
    );
}

// ---- Tier A.5: `key in d` returns bool ----

#[test]
fn w129_dict_membership_in_returns_bool() {
    // `"a" in d` — Decision 4A: bool. Wired at check.rs:881 via BinOp::In
    // + iter_element(Dict(K,_)) -> K.
    must_accept(
        "dict-in-returns-bool",
        "fn f(d: Dict[str, i64]) -> bool:\n    return (\"a\" in d)\n",
    );
}

#[test]
fn w130_dict_membership_negated_in_returns_bool() {
    // `not (k in d)` — explicit unary-not over the membership bool.
    // The parser exposes `not in` as BinOp::NotIn at PREC_CMP
    // (`parser.rs:943-946`) but the Pratt loop only consults that
    // table when KwNot sits in binary-op position; after a primary
    // string literal at PREC_CMP the lookahead matcher choked on
    // `KwNot` in the test fixture verified during sub-sprint a (see
    // §"Coverage gaps surfaced" in the dispatch report). Until DEV
    // fixes the `not in` parse, the lock test uses `not (k in d)`
    // which is equivalent at the type level: `bool -> bool`.
    must_accept(
        "dict-not-in-via-unary-not",
        "fn f(d: Dict[str, i64]) -> bool:\n    return not (\"z\" in d)\n",
    );
}

#[test]
fn w131_dict_membership_if_then_index() {
    // Common Python idiom: `if k in d: d[k] else: 0`.
    must_accept(
        "dict-if-in-then-index",
        "fn f(d: Dict[str, i64]) -> i64:\n    if (\"a\" in d):\n        return d[\"a\"]\n    return 0\n",
    );
}

// ---- Tier A.6: `len(d)` and `dict_is_empty(d)` (Decision 5 + addendum) ----

#[test]
fn w132_dict_len_returns_i64_via_stub() {
    // `dict_len_si(d) -> i64` — Decision 5A: `len(d) -> i64`. Pre-impl
    // uses the stub free-fn that sub-sprint b/e graduates to intrinsic
    // dispatch. Mirrors `list_len(lst) -> i64` precedent.
    must_accept_with_dict_stubs(
        "dict-len-yields-i64-via-stub",
        "fn f(d: Dict[str, i64]) -> i64:\n    return dict_len_si(d)\n",
    );
}

#[test]
fn w133_dict_is_empty_returns_bool_via_stub() {
    // `dict_is_empty_si(d) -> bool` — Decision 5-addendum: replaces
    // `if d:` (forbidden by §2.2) with an explicit predicate. Mirrors
    // `list_is_empty`.
    must_accept_with_dict_stubs(
        "dict-is-empty-yields-bool-via-stub",
        "fn f(d: Dict[str, i64]) -> bool:\n    return dict_is_empty_si(d)\n",
    );
}

// ---- Tier A.7: iteration (`for k in d:`) — keys-mode by Decision 6 ----

#[test]
fn w134_for_over_dict_keys_loop_var_is_key_type() {
    // `for k in d:` where `d: Dict[str, i64]` binds `k: str`. The
    // iter_element(Dict(K,_)) = K rule at check.rs:451 already gives
    // this. The actual MIR desugar to `__cobrust_dict_iter_init` is
    // sub-sprint e's job; the type-checker surface is locked here.
    must_accept(
        "for-over-dict-keys-yields-str",
        "fn f(d: Dict[str, i64]) -> i64:\n    for k in d:\n        return 0\n    return 0\n",
    );
}

#[test]
fn w135_for_over_dict_i64_keys_loop_var_is_i64() {
    // `for k in d:` where d: Dict[i64, i64] binds k: i64.
    must_accept(
        "for-over-dict-i64-keys-yields-i64",
        "fn f(d: Dict[i64, i64]) -> i64:\n    for k in d:\n        return k\n    return 0\n",
    );
}

// ---- Tier A.8: rebind `d[k] = v` (Decision 3) ----

#[test]
fn w136_dict_index_assign_rebind_or_insert() {
    // `d["a"] = 99` — rebind/insert. The HIR layer treats this as a
    // Stmt with LHS = IndexExpr + RHS = i64; the type checker unifies
    // V (i64) with the RHS type.
    must_accept(
        "dict-index-assign-rebind",
        "fn f() -> i64:\n    let d: Dict[str, i64] = {\"a\": 1}\n    d[\"a\"] = 99\n    return d[\"a\"]\n",
    );
}

#[test]
fn w137_dict_index_assign_new_key_insert() {
    // `d["b"] = 2` — insert a fresh key. Same type-check shape as
    // w136 (rebind/insert unification).
    must_accept(
        "dict-index-assign-new-key",
        "fn f() -> i64:\n    let d: Dict[str, i64] = {\"a\": 1}\n    d[\"b\"] = 2\n    return d[\"b\"]\n",
    );
}

// ---- Tier A.9: nested dicts ----

#[test]
fn w138_dict_nested_str_dict_str_i64() {
    // `Dict[str, Dict[str, i64]]` — outer K=str, inner K=str, inner V=i64.
    // Recursive Aggregate at MIR.
    must_accept(
        "dict-nested-str-dict-str-i64",
        "fn f() -> Dict[str, Dict[str, i64]]:\n    let inner: Dict[str, i64] = {\"a\": 1}\n    let outer: Dict[str, Dict[str, i64]] = {\"x\": inner}\n    return outer\n",
    );
}

#[test]
fn w139_dict_value_is_list_str() {
    // `Dict[str, List[str]]` — V is a list[str]. Sub-sprint d's V=list
    // shape (Phase G extension per ADR-0050d Decision 7 footnote).
    // Type-checker accepts; codegen needs Phase G.
    must_accept(
        "dict-value-list-str",
        "fn f() -> Dict[str, List[str]]:\n    let xs: List[str] = [\"a\", \"b\"]\n    let d: Dict[str, List[str]] = {\"k\": xs}\n    return d\n",
    );
}

#[test]
fn w140_dict_value_is_list_i64() {
    // `Dict[i64, List[i64]]` — K=i64, V=list[i64]. Common pattern
    // (adjacency list, histogram bucket).
    must_accept(
        "dict-value-list-i64",
        "fn f() -> Dict[i64, List[i64]]:\n    let xs: List[i64] = [1, 2]\n    let d: Dict[i64, List[i64]] = {7: xs}\n    return d\n",
    );
}

// ---- Tier A.10: dict as fn-param + fn-return ----

#[test]
fn w141_dict_as_fn_param_then_index_read() {
    // `fn f(d: Dict[str, i64]) -> i64` — by-value param; type-check
    // accepts. Ownership semantics (move vs borrow per the LC-100
    // honest-debt regression) are codegen / drop-pass concerns, not
    // type-check concerns.
    must_accept(
        "dict-fn-param-then-index",
        "fn f(d: Dict[str, i64]) -> i64:\n    return d[\"k\"]\n",
    );
}

#[test]
fn w142_dict_as_fn_return_then_consume() {
    // `fn build() -> Dict[str, i64]` + `let d = build(); d["k"]`.
    // NOTE: this test deliberately AVOIDS the LC-100 regression
    // pattern (no `let n = dict_len(d); let v = dict_get(d, k)` sequence
    // that would trigger Str-style UseAfterMove). We index once.
    must_accept(
        "dict-fn-return-then-consume",
        "fn build() -> Dict[str, i64]:\n    let d: Dict[str, i64] = {\"k\": 42}\n    return d\nfn main() -> i64:\n    let d: Dict[str, i64] = build()\n    return d[\"k\"]\n",
    );
}

// ---- Tier A.11: dict in if/while predicates via explicit boolean ----

#[test]
fn w143_dict_is_empty_in_if_predicate() {
    // `if dict_is_empty_si(d):` — constitution §2.2 forbids implicit
    // truthiness, so we use the explicit predicate stub. Locks Decision
    // 5-addendum at the use-site.
    must_accept_with_dict_stubs(
        "dict-is-empty-in-if",
        "fn f(d: Dict[str, i64]) -> i64:\n    if dict_is_empty_si(d):\n        return 0\n    return 1\n",
    );
}

#[test]
fn w144_dict_membership_in_while_condition() {
    // `while k in d:` — k in d is Bool, valid while condition.
    // Common pattern for dict-based work-queue exhaustion.
    must_accept(
        "dict-while-in-condition",
        "fn f(d: Dict[str, i64]) -> i64:\n    while (\"sentinel\" in d):\n        return 0\n    return 1\n",
    );
}

// ---- Tier A.12: dict comprehension (Decision 9, ADR-0050d sub-sprint c lock-in) ----

#[test]
fn w145_dict_comp_squares_i64() {
    // `{x: x*x for x in xs}` — type-check synthesises Dict[i64, i64].
    // Parser already produces ComprehensionKind::Dict +
    // ComprehensionElem::KeyValue per parser.rs:1491-1503; check.rs
    // already synthesises Dict[K, V] per check.rs:1006-1009. Sub-sprint
    // a corpus locks the surface; sub-sprint c wires MIR lowering.
    must_accept(
        "dict-comp-squares",
        "fn f(xs: List[i64]) -> Dict[i64, i64]:\n    return {x: (x * x) for x in xs}\n",
    );
}

// ============================================================
// Tier A — M-F.3.5 string stdlib well-typed corpus (ADR-0050e).
//
// Locks the type-checker surface for the eleven new PRELUDE fns:
//   1.  `split(s: str, sep: str) -> list[str]`     (Move both args)
//   2.  `join(parts: list[str], sep: str) -> str`  (Move both args)
//   3.  `replace(s: str, old: str, new: str) -> str`
//   4.  `trim(s: str) -> str`
//   5.  `find(s: str, needle: str) -> i64`         (-1 sentinel)
//   6.  `contains(s: str, needle: str) -> bool`
//   7.  `starts_with(s: str, prefix: str) -> bool`
//   8.  `ends_with(s: str, suffix: str) -> bool`
//   9.  `lower(s: str) -> str`
//   10. `upper(s: str) -> str`
//   11. `clone(s: str) -> str`                     (LC-100 mitigation)
//
// Pre-impl status: the PRELUDE in `crates/cobrust-cli/src/build.rs`
// does NOT yet declare these eleven fns; the `Kind` enum +
// `kind_for_name` in `crates/cobrust-cli/src/build/intrinsics.rs` has
// no M-F.3.5 entries. Type-check pipeline drops the PRELUDE step
// (see `must_accept` at top of file), so these w146.. tests use a
// STR_STDLIB_STUBS const that ships the eleven PRELUDE signatures
// inline. DEV sub-sprint 1 lands the PRELUDE entries; sub-sprint 2
// wires intrinsic-rewrite; sub-sprint 3 ships the C-ABI shims.
//
// w146..w175 cover the type-check layer surface. Runtime semantics
// (split round-trips, find sentinel correctness, etc.) live in the
// Tier C E2E corpus at `crates/cobrust-cli/tests/string_stdlib_e2e.rs`.
// ============================================================

// Shared stub block: the eleven PRELUDE signatures M-F.3.5 must
// accept. Mirrors the LIST_STR_STUBS + DICT_METHOD_STUBS pattern so
// tests can land BEFORE DEV graduates the stubs into the canonical
// PRELUDE. The `print` declaration is included so tests that pipe
// returns into print() are well-formed.
const STR_STDLIB_STUBS: &str = concat!(
    "fn print(s: str) -> i64:\n    return 0\n",
    "fn str_len(s: str) -> i64:\n    return 0\n",
    "fn str_at(s: str, i: i64) -> str:\n    return \"\"\n",
    "fn input(prompt: str) -> str:\n    return \"\"\n",
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

fn must_accept_with_str_stdlib_stubs(name: &str, body: &str) {
    let src = format!("{STR_STDLIB_STUBS}{body}");
    must_accept(name, &src);
}

// ---- Tier A.1: split — basic signatures ----

#[test]
fn w146_split_basic_signature() {
    // `split(s, sep)` consumes both Str args and returns list[str].
    // ADR-0050e Decision 3 row 1.
    must_accept_with_str_stdlib_stubs(
        "split-basic",
        "fn f() -> list[str]:\n    let xs: list[str] = split(\"a,b,c\", \",\")\n    return xs\n",
    );
}

#[test]
fn w147_split_return_into_let_list_str() {
    // The return type is list[str]; binding via `let xs: list[str] = split(...)`
    // type-checks cleanly even though both args are Move-consumed.
    must_accept_with_str_stdlib_stubs(
        "split-return-into-let",
        "fn f() -> i64:\n    let xs: list[str] = split(\"x,y\", \",\")\n    return 0\n",
    );
}

#[test]
fn w148_split_empty_separator_returns_list_str() {
    // Empty-sep edge case (ADR-0050e Decision 8): the surface
    // signature accepts `""` literally; runtime returns [s].
    must_accept_with_str_stdlib_stubs(
        "split-empty-sep",
        "fn f() -> list[str]:\n    let xs: list[str] = split(\"abc\", \"\")\n    return xs\n",
    );
}

// ---- Tier A.2: join — list[str] arg + str return ----

#[test]
fn w149_join_basic_signature() {
    // `join(parts, sep)` returns str. parts is list[str] (List walk-back
    // per ADR-0050c Phase 2a means parts survives operand-level; still
    // dropped at scope exit).
    must_accept_with_str_stdlib_stubs(
        "join-basic",
        "fn f() -> str:\n    let xs: list[str] = [\"a\", \"b\"]\n    return join(xs, \",\")\n",
    );
}

#[test]
fn w150_join_empty_list_returns_empty_str() {
    // Edge: join of empty list. Type-check accepts; runtime returns "".
    must_accept_with_str_stdlib_stubs(
        "join-empty-list",
        "fn f() -> str:\n    let xs: list[str] = []\n    return join(xs, \",\")\n",
    );
}

// ---- Tier A.3: replace — three-arg Str → Str ----

#[test]
fn w151_replace_basic_signature() {
    // `replace(s, old, new)` returns str.
    must_accept_with_str_stdlib_stubs(
        "replace-basic",
        "fn f() -> str:\n    return replace(\"foo bar\", \"bar\", \"baz\")\n",
    );
}

#[test]
fn w152_replace_into_let_str() {
    // Bind into `let result: str = replace(...)`.
    must_accept_with_str_stdlib_stubs(
        "replace-into-let",
        "fn f() -> i64:\n    let result: str = replace(\"aaa\", \"a\", \"b\")\n    return 0\n",
    );
}

// ---- Tier A.4: trim — Str → Str ----

#[test]
fn w153_trim_basic_signature() {
    // `trim(s)` returns str (whitespace stripped both sides).
    must_accept_with_str_stdlib_stubs(
        "trim-basic",
        "fn f() -> str:\n    return trim(\"  hello  \")\n",
    );
}

#[test]
fn w154_trim_empty_input_well_typed() {
    // `trim("")` — type-check accepts; runtime returns "".
    must_accept_with_str_stdlib_stubs("trim-empty", "fn f() -> str:\n    return trim(\"\")\n");
}

// ---- Tier A.5: find — Str x Str → i64 (with -1 sentinel) ----

#[test]
fn w155_find_basic_signature() {
    // `find(s, needle)` returns i64 (per ADR-0050e Decision 5 / Q2).
    // The -1 sentinel is a runtime concern; type-check sees i64 only.
    must_accept_with_str_stdlib_stubs(
        "find-basic",
        "fn f() -> i64:\n    return find(\"hello\", \"world\")\n",
    );
}

#[test]
fn w156_find_into_let_i64() {
    // Bind result to `let pos: i64`. Locks the doc-required idiom
    // `if pos != -1:` is well-typed (i64 == -1 is bool).
    must_accept_with_str_stdlib_stubs(
        "find-into-let",
        "fn f() -> i64:\n    let pos: i64 = find(\"hi\", \"i\")\n    if pos != -1:\n        return 1\n    return 0\n",
    );
}

#[test]
fn w157_find_sentinel_compare_in_predicate() {
    // The doc-required `if find(s, x) != -1:` idiom from Decision 5.
    // Lacks an intermediate let; tests the inline compare-against-sentinel.
    must_accept_with_str_stdlib_stubs(
        "find-inline-sentinel",
        "fn f() -> i64:\n    if find(\"abc\", \"b\") != -1:\n        return 1\n    return 0\n",
    );
}

// ---- Tier A.6: contains / starts_with / ends_with — Str x Str → bool ----

#[test]
fn w158_contains_basic_signature() {
    // `contains(s, needle)` returns bool.
    must_accept_with_str_stdlib_stubs(
        "contains-basic",
        "fn f() -> bool:\n    return contains(\"hello\", \"ell\")\n",
    );
}

#[test]
fn w159_contains_in_if_predicate() {
    // `if contains(s, x):` works because contains returns bool. No
    // implicit-truthy violation here — bool IS the type.
    must_accept_with_str_stdlib_stubs(
        "contains-in-if",
        "fn f() -> i64:\n    if contains(\"hi\", \"i\"):\n        return 1\n    return 0\n",
    );
}

#[test]
fn w160_starts_with_basic_signature() {
    // `starts_with(s, prefix) -> bool`.
    must_accept_with_str_stdlib_stubs(
        "starts-with-basic",
        "fn f() -> bool:\n    return starts_with(\"foobar\", \"foo\")\n",
    );
}

#[test]
fn w161_ends_with_basic_signature() {
    // `ends_with(s, suffix) -> bool`.
    must_accept_with_str_stdlib_stubs(
        "ends-with-basic",
        "fn f() -> bool:\n    return ends_with(\"foobar\", \"bar\")\n",
    );
}

// ---- Tier A.7: lower / upper — Str → Str ----

#[test]
fn w162_lower_basic_signature() {
    // `lower(s) -> str` — ASCII fast path matches Rust str::to_lowercase.
    must_accept_with_str_stdlib_stubs(
        "lower-basic",
        "fn f() -> str:\n    return lower(\"HELLO\")\n",
    );
}

#[test]
fn w163_upper_basic_signature() {
    // `upper(s) -> str`.
    must_accept_with_str_stdlib_stubs(
        "upper-basic",
        "fn f() -> str:\n    return upper(\"hello\")\n",
    );
}

#[test]
fn w164_lower_mixed_case_input() {
    // Mixed-case input — type-check sees no special case.
    must_accept_with_str_stdlib_stubs(
        "lower-mixed",
        "fn f() -> str:\n    return lower(\"HeLLo\")\n",
    );
}

// ---- Tier A.8: clone — the LC-100 unblocker ----

#[test]
fn w165_clone_basic_signature() {
    // `clone(s) -> str` deep-copies the StringBuffer (per ADR-0050e
    // Decision 2). The C-ABI shim already ships at fmt.rs:306; M-F.3.5
    // adds the PRELUDE + intrinsic-rewrite plumbing.
    must_accept_with_str_stdlib_stubs(
        "clone-basic",
        "fn f() -> str:\n    return clone(\"hello\")\n",
    );
}

#[test]
fn w166_clone_into_let_str_then_reuse_original() {
    // THE LC-100 unblocker pattern. Under ADR-0050c, Str is non-Copy,
    // so `let n = str_len(s); let c = str_at(s, 0)` fails UseAfterMove.
    // With `let s2 = clone(s); let n = str_len(s); let c = str_at(s2, 0)`,
    // the type checker accepts both reads because s and s2 are
    // independent owned bindings. This is the load-bearing test for
    // the LC-100 honest-debt mitigation.
    must_accept_with_str_stdlib_stubs(
        "clone-lc100-unblock",
        "fn f() -> i64:\n    let s: str = input(\"\")\n    let s2: str = clone(s)\n    let n: i64 = str_len(s)\n    let c: str = str_at(s2, 0)\n    return n\n",
    );
}

#[test]
fn w167_clone_double_clone_independent_bindings() {
    // `let a = clone(s); let b = clone(s); ...` — multi-clone safety
    // pattern. Each clone moves the previous arg, but the inputs are
    // (a) `clone(s)` — moves s once and (b) `clone(s)` — needs s alive.
    // The trick: this pattern still moves s on the FIRST clone; the
    // second clone needs s alive. We test the well-typed shape WITH a
    // re-clone idiom: `let a = clone(s); let b = clone(a);` (chained
    // clones, each from the previous).
    must_accept_with_str_stdlib_stubs(
        "clone-chained-pair",
        "fn f() -> i64:\n    let s: str = \"x\"\n    let a: str = clone(s)\n    let b: str = clone(a)\n    return 0\n",
    );
}

// ---- Tier A.9: idiomatic compositions ----

#[test]
fn w168_trim_then_split_chained() {
    // `split(trim(s), ",")` — split's first arg is trim's return (a
    // fresh str). Both args consumed; OK because each is rvalue.
    must_accept_with_str_stdlib_stubs(
        "trim-then-split",
        "fn f() -> list[str]:\n    return split(trim(\"  a,b,c  \"), \",\")\n",
    );
}

#[test]
fn w169_lower_into_contains_chained() {
    // `contains(lower(s), needle)` — case-insensitive substring search
    // workaround per ADR-0050e Decision 7 (Phase G adds the explicit
    // variant; today users hand-compose).
    must_accept_with_str_stdlib_stubs(
        "lower-into-contains",
        "fn f() -> bool:\n    return contains(lower(\"FooBar\"), \"foo\")\n",
    );
}

#[test]
fn w170_join_split_roundtrip_typed() {
    // `join(split(s, ","), ",")` — types: split → list[str], join takes
    // list[str] + str. Type-check accepts; runtime is a near-identity
    // (non-pathological s).
    must_accept_with_str_stdlib_stubs(
        "join-split-roundtrip",
        "fn f() -> str:\n    return join(split(\"a,b,c\", \",\"), \",\")\n",
    );
}

// ---- Tier A.10: as fn argument / return type ----

#[test]
fn w171_fn_takes_str_returns_via_lower() {
    // `fn lowercase_of(s: str) -> str: return lower(s)` — user-defined
    // wrapper over the M-F.3.5 surface.
    must_accept_with_str_stdlib_stubs(
        "fn-wraps-lower",
        "fn lowercase_of(s: str) -> str:\n    return lower(s)\nfn f() -> str:\n    return lowercase_of(\"FOO\")\n",
    );
}

#[test]
fn w172_fn_returns_split_result_then_iter() {
    // `fn parts_of(s: str) -> list[str]: return split(s, ",")` chained
    // with for-iter over list[str]. Locks the user-traction surface.
    must_accept_with_str_stdlib_stubs(
        "fn-returns-split-then-iter",
        "fn parts_of(s: str) -> list[str]:\n    return split(s, \",\")\nfn f() -> i64:\n    let xs: list[str] = parts_of(\"a,b\")\n    for x in xs:\n        let _ = print(x)\n    return 0\n",
    );
}

// ---- Tier A.11: f-string composition with M-F.3.5 returns ----

#[test]
fn w173_fstring_contains_split_index_result() {
    // F30 bug-witness shape — f-string with split result indexed.
    // `f"first={split(s, ",")[0]}"` locks (a) f-string Str hole accepts
    // a str; (b) list[str] indexing yields str; (c) split's return
    // composes with f-string lowering.
    must_accept_with_str_stdlib_stubs(
        "fstring-with-split-index",
        "fn f() -> str:\n    let xs: list[str] = split(\"a,b\", \",\")\n    return f\"first={xs[0]}\"\n",
    );
}

#[test]
fn w174_fstring_contains_trim_result() {
    // f-string with trim() return slotted into a hole — lightweight
    // composition test.
    must_accept_with_str_stdlib_stubs(
        "fstring-with-trim",
        "fn f() -> str:\n    return f\"trimmed=[{trim(\"  x  \")}]\"\n",
    );
}

// ---- Tier A.12: find result in boolean predicate ----

#[test]
fn w175_find_compared_to_sentinel_doc_idiom() {
    // The documented `let pos = find(...); if pos != -1: ...` shape.
    // Locks Decision 5's doc-required idiom is well-typed end-to-end.
    must_accept_with_str_stdlib_stubs(
        "find-doc-idiom",
        "fn f() -> i64:\n    let pos: i64 = find(\"hello world\", \"world\")\n    if pos != -1:\n        return pos\n    return -1\n",
    );
}

// ============================================================
// M-F.3.6 — File IO completion (ADR-0050f)
// w176..w195 — Tier A well-typed corpus for 7 surface fns.
//
// Pre-impl status: none of the 7 fns exist in the PRELUDE; the
// `Kind` enum + `kind_for_name` in intrinsics.rs have no M-F.3.6
// entries. C-ABI shims (__cobrust_read_file, __cobrust_write_file,
// etc.) do not yet exist. Every test below SHOULD FAIL pre-impl.
// The type-check layer does not run the PRELUDE (see `must_accept`
// at top of file), so we inject FILE_IO_STUBS inline — the same
// pattern as STR_STDLIB_STUBS above.
//
// Signature table (binding — ADR-0050f §"Decision"):
//   fn read_file(path: str) -> str
//   fn read_file_lines(path: str) -> list[str]
//   fn write_file(path: str, contents: str) -> i64
//   fn append_file(path: str, contents: str) -> i64
//   fn stdin_read_all() -> str
//   fn stdout_write(s: str) -> i64
//   fn stderr_write(s: str) -> i64
//
// Q1 resolution: i64-sentinel (0=success, 1=I/O error) for
// write/append/stdout/stderr; bare str / list[str] for reads.
// Q2 resolution: read_file_lines strips \n and \r\n.
//
// w176..w182: basic signature acceptance per fn.
// w183..w188: i64-sentinel binding + comparison patterns.
// w189..w192: inline-clone-at-callsite (M-F.3.5 carry-forward).
// w193..w195: f-string composition with file-IO returns.
// ============================================================

// Shared stub block: the 7 M-F.3.6 PRELUDE signatures + helpers
// needed by the tests (print, str_len, clone, list helpers).
const FILE_IO_STUBS: &str = concat!(
    "fn print(s: str) -> i64:\n    return 0\n",
    "fn print_int(n: i64) -> i64:\n    return 0\n",
    "fn str_len(s: str) -> i64:\n    return 0\n",
    "fn clone(s: str) -> str:\n    return s\n",
    "fn list_len(xs: list[str]) -> i64:\n    return 0\n",
    "fn read_file(path: str) -> str:\n    return \"\"\n",
    "fn read_file_lines(path: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n",
    "fn write_file(path: str, contents: str) -> i64:\n    return 0\n",
    "fn append_file(path: str, contents: str) -> i64:\n    return 0\n",
    "fn stdin_read_all() -> str:\n    return \"\"\n",
    "fn stdout_write(s: str) -> i64:\n    return 0\n",
    "fn stderr_write(s: str) -> i64:\n    return 0\n",
);

fn must_accept_with_file_io_stubs(name: &str, body: &str) {
    let src = format!("{FILE_IO_STUBS}{body}");
    must_accept(name, &src);
}

// ---- Tier A.1: read_file basic signatures ----

#[test]
fn w176_read_file_basic_signature_accepted() {
    // ADR-0050f §"Decision" row 1: `read_file(path: str) -> str`.
    // path is consumed (Move); return is owned str.
    must_accept_with_file_io_stubs(
        "read-file-basic",
        "fn f() -> str:\n    let contents: str = read_file(\"/tmp/x.txt\")\n    return contents\n",
    );
}

#[test]
fn w177_read_file_return_bound_to_str_var() {
    // Binding read_file result to a `let` of type `str` is well-typed.
    // Locks that the type checker accepts `str` return annotation.
    must_accept_with_file_io_stubs(
        "read-file-let-bind",
        "fn f() -> i64:\n    let s: str = read_file(\"/tmp/x.txt\")\n    let _ = print(s)\n    return 0\n",
    );
}

// ---- Tier A.2: read_file_lines signatures ----

#[test]
fn w178_read_file_lines_returns_list_str_accepted() {
    // ADR-0050f §"Decision" row 2: `read_file_lines(path: str) -> list[str]`.
    // Locks the return type annotation is accepted.
    must_accept_with_file_io_stubs(
        "read-file-lines-basic",
        "fn f() -> list[str]:\n    let xs: list[str] = read_file_lines(\"/tmp/x.txt\")\n    return xs\n",
    );
}

#[test]
fn w179_read_file_lines_iterated_with_for_loop() {
    // read_file_lines result iterated via ADR-0050b for-loop over list[str].
    // Each element `s` has type `str`; print(s) is well-typed.
    must_accept_with_file_io_stubs(
        "read-file-lines-for-iter",
        "fn f() -> i64:\n    let xs: list[str] = read_file_lines(\"/tmp/x.txt\")\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
    );
}

// ---- Tier A.3: write_file signatures ----

#[test]
fn w180_write_file_returns_i64_accepted() {
    // ADR-0050f §"Decision" row 3: `write_file(path: str, contents: str) -> i64`.
    // 0 = success sentinel; both path and contents are consumed.
    must_accept_with_file_io_stubs(
        "write-file-basic",
        "fn f() -> i64:\n    let rc: i64 = write_file(\"/tmp/x.txt\", \"hello\")\n    return rc\n",
    );
}

#[test]
fn w181_write_file_sentinel_compared_to_zero() {
    // i64-sentinel pattern: `if write_file(p, c) != 0: ...`
    // ADR-0050f §"i64-sentinel error reporting (Q1 resolution)".
    must_accept_with_file_io_stubs(
        "write-file-sentinel-check",
        "fn f() -> i64:\n    if write_file(\"/tmp/x.txt\", \"hello\") != 0:\n        return 1\n    return 0\n",
    );
}

// ---- Tier A.4: append_file signatures ----

#[test]
fn w182_append_file_returns_i64_accepted() {
    // ADR-0050f §"Decision" row 4: `append_file(path: str, contents: str) -> i64`.
    // Same sentinel pattern as write_file.
    must_accept_with_file_io_stubs(
        "append-file-basic",
        "fn f() -> i64:\n    let rc: i64 = append_file(\"/tmp/x.txt\", \"line\")\n    return rc\n",
    );
}

#[test]
fn w183_append_file_sentinel_checked() {
    // Sentinel comparison: `if append_file(p, c) != 0:` well-typed.
    must_accept_with_file_io_stubs(
        "append-file-sentinel-check",
        "fn f() -> i64:\n    if append_file(\"/tmp/x.txt\", \"more\") != 0:\n        return 1\n    return 0\n",
    );
}

// ---- Tier A.5: stdin_read_all ----

#[test]
fn w184_stdin_read_all_returns_str_accepted() {
    // ADR-0050f §"Decision" row 5: `stdin_read_all() -> str`.
    // Zero args; return is owned str.
    must_accept_with_file_io_stubs(
        "stdin-read-all-basic",
        "fn f() -> str:\n    let s: str = stdin_read_all()\n    return s\n",
    );
}

#[test]
fn w185_stdin_read_all_used_with_str_len() {
    // stdin_read_all() returns str; str_len(s) consumes it (Move).
    // Single-use pattern: well-typed.
    must_accept_with_file_io_stubs(
        "stdin-read-all-str-len",
        "fn f() -> i64:\n    let s: str = stdin_read_all()\n    return str_len(s)\n",
    );
}

// ---- Tier A.6: stdout_write / stderr_write ----

#[test]
fn w186_stdout_write_returns_i64_accepted() {
    // ADR-0050f §"Decision" row 6: `stdout_write(s: str) -> i64`.
    // Differs from print family: explicit i64 return + no trailing newline.
    must_accept_with_file_io_stubs(
        "stdout-write-basic",
        "fn f() -> i64:\n    let rc: i64 = stdout_write(\"hello\")\n    return rc\n",
    );
}

#[test]
fn w187_stderr_write_returns_i64_accepted() {
    // ADR-0050f §"Decision" row 7: `stderr_write(s: str) -> i64`.
    must_accept_with_file_io_stubs(
        "stderr-write-basic",
        "fn f() -> i64:\n    let rc: i64 = stderr_write(\"error msg\")\n    return rc\n",
    );
}

#[test]
fn w188_stdout_write_sentinel_compared_to_zero() {
    // Sentinel: `if stdout_write(s) != 0:` is well-typed.
    // ADR-0050f §"Cross-surface dispatch table".
    must_accept_with_file_io_stubs(
        "stdout-write-sentinel-check",
        "fn f() -> i64:\n    if stdout_write(\"msg\") != 0:\n        return 1\n    return 0\n",
    );
}

// ---- Tier A.7: inline-clone-at-callsite (M-F.3.5 carry-forward) ----

#[test]
fn w189_inline_clone_before_write_file_then_read_file() {
    // ADR-0050f §"Step 2.8 idiom" (from ADR-0050e): multi-use str
    // requires clone. Pattern: `let n = write_file(clone(path),
    // clone(contents)); read_file(path)`.
    // path and contents each consumed once after cloning.
    must_accept_with_file_io_stubs(
        "inline-clone-write-then-read",
        "fn f() -> str:\n    let path: str = \"/tmp/x.txt\"\n    let n: i64 = write_file(clone(path), \"hello\")\n    return read_file(path)\n",
    );
}

#[test]
fn w190_clone_path_for_write_then_read_file_lines() {
    // Clone path so write_file + read_file_lines both have an owned str.
    must_accept_with_file_io_stubs(
        "clone-for-write-then-lines",
        "fn f() -> list[str]:\n    let path: str = \"/tmp/x.txt\"\n    let n: i64 = write_file(clone(path), \"a\")\n    return read_file_lines(path)\n",
    );
}

#[test]
fn w191_clone_contents_for_write_and_append() {
    // Clone contents so write_file and append_file each consume a copy.
    must_accept_with_file_io_stubs(
        "clone-contents-write-append",
        "fn f() -> i64:\n    let c: str = \"line\"\n    let rc1: i64 = write_file(\"/tmp/a.txt\", clone(c))\n    let rc2: i64 = append_file(\"/tmp/a.txt\", c)\n    return rc1\n",
    );
}

#[test]
fn w192_clone_str_for_stdout_write_and_stderr_write() {
    // Clone s so stdout_write and stderr_write each consume a copy.
    must_accept_with_file_io_stubs(
        "clone-for-stdout-stderr",
        "fn f() -> i64:\n    let s: str = \"msg\"\n    let rc1: i64 = stdout_write(clone(s))\n    let rc2: i64 = stderr_write(s)\n    return rc1\n",
    );
}

// ---- Tier A.8: f-string composition with file-IO returns ----

#[test]
fn w193_fstring_with_read_file_return_str_hole() {
    // f-string Str hole: `f"contents={read_file(p)}"`.
    // Locks the f-string Str-hole dispatch fix (Wave 2 commit 9c8b1d2
    // per ADR-0050f §"F30 §Consequences — f-string Str hole dispatch").
    must_accept_with_file_io_stubs(
        "fstring-read-file-hole",
        "fn f() -> str:\n    let p: str = \"/tmp/x.txt\"\n    return f\"contents={read_file(p)}\"\n",
    );
}

#[test]
fn w194_fstring_with_stdin_read_all_hole() {
    // f-string with stdin_read_all() slotted into a Str hole.
    // stdin_read_all() returns owned str; f-string consumes it.
    must_accept_with_file_io_stubs(
        "fstring-stdin-read-all-hole",
        "fn f() -> str:\n    return f\"stdin=[{stdin_read_all()}]\"\n",
    );
}

#[test]
fn w195_list_len_of_read_file_lines_result() {
    // read_file_lines returns list[str]; list_len(xs) consumes the list
    // and returns i64. Locks that list[str] return from file-IO fn can
    // be passed to list_len.
    must_accept_with_file_io_stubs(
        "list-len-of-read-file-lines",
        "fn f() -> i64:\n    let xs: list[str] = read_file_lines(\"/tmp/x.txt\")\n    return list_len(xs)\n",
    );
}

// ============================================================
// ADR-0052a Wave 1 — Direction A explicit `&s` borrow corpus
//
// 30 well-typed programs that the type checker MUST accept under the
// `&s` explicit-borrow surface (CLAUDE.md §2.5 Direction A binding).
//
// Pre-DEV-impl status: every w0052a_* test below is `#[ignore]`'d
// pending Wave-1 DEV merge (parser+HIR+types+MIR scaffolding per
// ADR-0052a §6–§9). DEV removes the `#[ignore]` markers and the suite
// turns green.
//
// DEV v3 post-impl wiring (TEST author pattern error correction):
// the w0052a_* tests originally called `must_accept` directly with
// bodies that reference PRELUDE names (`input`, `str_len`, `str_at`,
// `list_len`, etc.). `must_accept` does NOT prepend PRELUDE stubs;
// the test framework rejects with `UnknownName` before reaching the
// borrow surface. DEV adds `must_accept_with_borrow_stubs` below
// (mirrors `must_accept_with_str_stdlib_stubs` / LIST_STR_STUBS
// idiom) and the 30 w0052a tests use it.
const BORROW_STUBS: &str = concat!(
    "fn print(s: str) -> i64:\n    return 0\n",
    "fn print_int(n: i64) -> i64:\n    return 0\n",
    "fn print_no_nl(s: str) -> i64:\n    return 0\n",
    "fn input(prompt: str) -> str:\n    return \"\"\n",
    "fn str_len(s: str) -> i64:\n    return 0\n",
    "fn str_at(s: str, i: i64) -> str:\n    return \"\"\n",
    "fn str_ord(s: str) -> i64:\n    return 0\n",
    "fn str_eq(a: str, b: str) -> i64:\n    return 0\n",
    "fn str_eq_lit(s: str, lit: str) -> i64:\n    return 0\n",
    "fn list_len(lst: list[i64]) -> i64:\n    return 0\n",
    "fn list_new(capacity: i64) -> list[i64]:\n    let xs: list[i64] = []\n    return xs\n",
    "fn list_get(lst: list[i64], i: i64) -> i64:\n    return 0\n",
    "fn list_set(lst: list[i64], i: i64, v: i64) -> i64:\n    return 0\n",
);

fn must_accept_with_borrow_stubs(name: &str, body: &str) {
    let src = format!("{BORROW_STUBS}{body}");
    must_accept(name, &src);
}
//
// Coverage map (mirrors ADR-0052a §4 + §10.1 TEST corpus categories):
// - 4.1 LC-02 reverse_string pattern              → w0052a_01..02
// - 4.2 LC-13 roman_to_integer pattern            → w0052a_03..04
// - 4.3 LC-20 valid_parentheses pattern           → w0052a_05..06
// - 4.4 let-rebind shortcut (`let s = &s`)        → w0052a_07..09
// - 4.5 function-arg pass-by-borrow               → w0052a_10..11
// - 4.6 comprehension predicate `&s`              → w0052a_12..13
// - 4.7 conditional borrow                        → w0052a_14..15
// - 4.8 borrow chained through let                → w0052a_16..17
// - additional `&ident.field`                     → w0052a_18..19
// - additional `&ident[idx]`                      → w0052a_20..21
// - let-rebind chains (`let r = &s; let r2 = r`)  → w0052a_22..23
// - mixed borrowed + owned reads in same scope    → w0052a_24..25
// - sequenced &s reads then full move at end      → w0052a_26..27
// - nested borrow inside tuple / aggregate ctor   → w0052a_28
// - parenthesised &s on identifier                → w0052a_29
// - &s used as argument to user fn taking str     → w0052a_30
// ============================================================

#[test]
fn w0052a_01_lc02_reverse_string_pattern() {
    // ADR-0052a §4.1 LC-02 canonical trigger. `let n = str_len(&s)`
    // followed by `let c = str_at(&s, i)` must type-check clean under
    // Wave-1 transparency rule (PRELUDE Str helpers accept both `s: Str`
    // and `&s: &Str` for read-only positions).
    must_accept_with_borrow_stubs(
        "lc02-reverse-string-borrow",
        "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let i: i64 = n - 1\n    while i >= 0:\n        let c = str_at(&s, i)\n        i = i - 1\n    return 0\n",
    );
}

#[test]
fn w0052a_02_lc02_reverse_string_three_reads() {
    // LC-02 variant with three borrow reads on the same Str.
    must_accept_with_borrow_stubs(
        "lc02-reverse-string-three-reads",
        "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let m = str_len(&s)\n    let p = str_len(&s)\n    return (n + m) + p\n",
    );
}

#[test]
fn w0052a_03_lc13_roman_to_integer_pattern() {
    // ADR-0052a §4.2 LC-13 pattern. `&s` in str_len + str_at sequence.
    must_accept_with_borrow_stubs(
        "lc13-roman-to-integer-borrow",
        "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let i: i64 = 0\n    while i < n:\n        let c = str_at(&s, i)\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn w0052a_04_lc13_with_ord_extract() {
    // LC-13 variant: `&s` read in str_at followed by str_ord on the
    // sub-string borrow (Wave-1 transparency: `&c` and `c` are
    // interchangeable for str_ord).
    must_accept_with_borrow_stubs(
        "lc13-with-ord",
        "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let c = str_at(&s, 0)\n    let o = str_ord(&c)\n    return o + n\n",
    );
}

#[test]
fn w0052a_05_lc20_valid_parentheses_pattern() {
    // ADR-0052a §4.3 LC-20 pattern. While-bound iteration with `&s`.
    must_accept_with_borrow_stubs(
        "lc20-valid-parens-borrow",
        "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let i: i64 = 0\n    while i < n:\n        let c = str_at(&s, i)\n        i = i + 1\n    return 0\n",
    );
}

#[test]
#[ignore = "finding:cluster-a-letrebind-fieldborrow — test source uses `if str_eq_lit(&c, \"(\"):` which surfaces `ImplicitTruthiness` (constitution §2.2 ban on `if <int>:`), NOT a Wave-1 nested-borrow gap. The borrow path itself works; the TEST author's source needs `!= 0` to be well-typed. Deferred to a TEST corpus fixup sprint (out of scope for this DEV impl)."]
fn w0052a_06_lc20_nested_str_eq_borrow() {
    // LC-20 variant: `&c` used in str_eq comparison; multiple borrowed
    // reads off the same str within an if-else chain.
    must_accept_with_borrow_stubs(
        "lc20-nested-streq",
        "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let c = str_at(&s, 0)\n    if str_eq_lit(&c, \"(\"):\n        return 1\n    return 0\n",
    );
}

#[test]
fn w0052a_07_let_rebind_shortcut_basic() {
    // ADR-0052a §4.4 let-rebind shortcut. `let s = &s` creates a borrow
    // that shadows the outer binding for the new scope.
    must_accept_with_borrow_stubs(
        "let-rebind-basic",
        "fn main() -> i64:\n    let s = input(\"\")\n    let s = &s\n    let n = str_len(s)\n    return n\n",
    );
}

#[test]
fn w0052a_08_let_rebind_then_multi_read() {
    // Let-rebind followed by multiple reads via the rebound (borrowed)
    // binding; works because the rebound `s` is `&Str`.
    must_accept_with_borrow_stubs(
        "let-rebind-multi-read",
        "fn main() -> i64:\n    let s = input(\"\")\n    let s = &s\n    let n = str_len(s)\n    let m = str_len(s)\n    return n + m\n",
    );
}

#[test]
fn w0052a_09_let_rebind_with_typed_function_arg() {
    // Let-rebind shortcut inside a function with typed parameter.
    must_accept_with_borrow_stubs(
        "let-rebind-typed-arg",
        "fn count(s: str) -> i64:\n    let s = &s\n    let n = str_len(s)\n    let m = str_len(s)\n    return n + m\n",
    );
}

#[test]
fn w0052a_10_function_arg_pass_by_borrow() {
    // ADR-0052a §4.5 fn-arg pass-by-borrow. PRELUDE printer takes `s: str`;
    // `print(&label)` works under transparency rule.
    must_accept_with_borrow_stubs(
        "fn-arg-borrow-print",
        "fn main() -> i64:\n    let label = input(\"\")\n    let _ = print(&label)\n    let _ = print(&label)\n    return 0\n",
    );
}

#[test]
fn w0052a_11_function_arg_borrow_user_fn() {
    // User-defined fn accepting `s: str`; caller passes `&label`.
    // Transparency rule makes `&str` compatible with `str` parameter.
    must_accept_with_borrow_stubs(
        "fn-arg-borrow-user-fn",
        "fn echo(s: str) -> i64:\n    return str_len(s)\nfn main() -> i64:\n    let label = input(\"\")\n    let a = echo(&label)\n    let b = echo(&label)\n    return a + b\n",
    );
}

#[test]
fn w0052a_12_comprehension_predicate_borrow() {
    // ADR-0052a §4.6 comprehension predicate with `&line`. The
    // comprehension variable `line` would be moved by str_len; `&line`
    // borrows for read-only use.
    must_accept_with_borrow_stubs(
        "comprehension-predicate-borrow",
        "fn main() -> i64:\n    let lines: list[str] = [\"a\", \"bb\"]\n    let xs: list[i64] = [str_len(&line) for line in lines]\n    return list_len(xs)\n",
    );
}

#[test]
fn w0052a_13_comprehension_with_borrow_in_predicate() {
    // Comprehension with `if` predicate using `&line` for str_len > 0.
    must_accept_with_borrow_stubs(
        "comprehension-if-borrow",
        "fn main() -> i64:\n    let lines: list[str] = [\"\", \"x\", \"ab\"]\n    let xs: list[i64] = [str_len(&line) for line in lines if str_len(&line) > 0]\n    return list_len(xs)\n",
    );
}

#[test]
fn w0052a_14_conditional_borrow_if_branch() {
    // ADR-0052a §4.7 conditional borrow. `if cond: f(&s)` lowers the
    // borrow only in the taken branch; the else branch leaves `s`
    // owned.
    must_accept_with_borrow_stubs(
        "conditional-borrow-if",
        "fn main() -> i64:\n    let cond: bool = True\n    let s = input(\"\")\n    let v: i64 = 0\n    if cond:\n        v = str_len(&s)\n    return v\n",
    );
}

#[test]
fn w0052a_15_conditional_borrow_else_branch() {
    // Conditional borrow used in the else-branch; analogous to §4.7
    // but routed through the else path.
    must_accept_with_borrow_stubs(
        "conditional-borrow-else",
        "fn main() -> i64:\n    let cond: bool = False\n    let s = input(\"\")\n    let v: i64 = 0\n    if cond:\n        v = 0\n    else:\n        v = str_len(&s)\n    return v\n",
    );
}

#[test]
fn w0052a_16_borrow_chained_through_let() {
    // ADR-0052a §4.8 borrow chained through let-statements. Multiple
    // `&s` reads through let bindings.
    must_accept_with_borrow_stubs(
        "borrow-chained-lets",
        "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let m = str_ord(&s)\n    let p = n + m\n    return p\n",
    );
}

#[test]
fn w0052a_17_borrow_in_arithmetic_expression() {
    // Borrowed read inside a more complex arithmetic expression. The
    // arithmetic result is i64; the borrow is transient.
    must_accept_with_borrow_stubs(
        "borrow-in-arith",
        "fn main() -> i64:\n    let s = input(\"\")\n    let total = str_len(&s) + str_len(&s)\n    return total\n",
    );
}

#[test]
fn w0052a_18_borrow_field_access() {
    // ADR-0052a §3 + §8 — `&ident.field` is one of the three Wave-1
    // production paths. Wave-1 transparency keeps this exposable to
    // PRELUDE-fn callers.
    //
    // NOTE: tuple-field syntax `.0` is the Cobrust convention for
    // accessing positional tuple fields (see ADR-0050a / round-trip
    // tests). The borrow form `&p.0` must type-check clean.
    must_accept_with_borrow_stubs(
        "borrow-field-access-tuple",
        "fn main() -> i64:\n    let p = (\"left\", \"right\")\n    let n = str_len(&p.0)\n    return n\n",
    );
}

#[test]
fn w0052a_19_borrow_field_access_then_arith() {
    // Borrow of tuple-field used in arithmetic.
    must_accept_with_borrow_stubs(
        "borrow-field-then-arith",
        "fn main() -> i64:\n    let p = (\"a\", \"bb\")\n    let total = str_len(&p.0) + str_len(&p.1)\n    return total\n",
    );
}

#[test]
fn w0052a_20_borrow_indexed_list_str() {
    // ADR-0052a §3 + §8 — `&ident[idx]` is one of the three Wave-1
    // production paths. Index into list[str], take borrow.
    must_accept_with_borrow_stubs(
        "borrow-indexed-list-str",
        "fn main() -> i64:\n    let xs: list[str] = [\"alpha\", \"beta\"]\n    let n = str_len(&xs[0])\n    return n\n",
    );
}

#[test]
fn w0052a_21_borrow_indexed_in_loop() {
    // Borrow of indexed list[str] element inside a length-bound loop.
    must_accept_with_borrow_stubs(
        "borrow-indexed-in-loop",
        "fn main() -> i64:\n    let xs: list[str] = [\"a\", \"bb\", \"ccc\"]\n    let n: i64 = list_len(xs)\n    let i: i64 = 0\n    let total: i64 = 0\n    while i < n:\n        total = total + str_len(&xs[i])\n        i = i + 1\n    return total\n",
    );
}

#[test]
fn w0052a_22_let_rebind_chain_basic() {
    // Two-step let-rebind chain: `let r = &s; let r2 = r;`. Both `r`
    // and `r2` are `&Str`; transparency rule allows PRELUDE call on
    // both.
    must_accept_with_borrow_stubs(
        "let-rebind-chain-2",
        "fn main() -> i64:\n    let s = input(\"\")\n    let r = &s\n    let r2 = r\n    let n = str_len(r2)\n    return n\n",
    );
}

#[test]
fn w0052a_23_let_rebind_chain_three_levels() {
    // Three-step let-rebind chain.
    must_accept_with_borrow_stubs(
        "let-rebind-chain-3",
        "fn main() -> i64:\n    let s = input(\"\")\n    let r = &s\n    let r2 = r\n    let r3 = r2\n    let n = str_len(r3)\n    return n\n",
    );
}

#[test]
fn w0052a_24_mixed_borrowed_and_owned_reads() {
    // First read is borrowed (`&s`); LAST read consumes the owned
    // local. Both reads must succeed; this exercises the Wave-1
    // semantics that borrows don't conflict with a subsequent move.
    must_accept_with_borrow_stubs(
        "mixed-borrow-then-move",
        "fn consume(s: str) -> i64:\n    return str_len(s)\nfn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let m = consume(s)\n    return n + m\n",
    );
}

#[test]
fn w0052a_25_two_borrows_then_owned_read() {
    // Two `&s` reads, then a single owned consume. Three reads total
    // on the same Str local; only the last consumes.
    must_accept_with_borrow_stubs(
        "two-borrows-then-consume",
        "fn consume(s: str) -> i64:\n    return str_len(s)\nfn main() -> i64:\n    let s = input(\"\")\n    let a = str_len(&s)\n    let b = str_len(&s)\n    let c = consume(s)\n    return (a + b) + c\n",
    );
}

#[test]
fn w0052a_26_sequenced_borrows_in_loop() {
    // Borrowed reads sequenced inside a loop body; the binding `s` is
    // never moved, so loop iterations work uniformly.
    must_accept_with_borrow_stubs(
        "sequenced-borrows-loop",
        "fn main() -> i64:\n    let s = input(\"\")\n    let i: i64 = 0\n    let total: i64 = 0\n    while i < 3:\n        total = total + str_len(&s)\n        i = i + 1\n    return total\n",
    );
}

#[test]
fn w0052a_27_borrows_then_final_owned_move() {
    // Borrowed reads in a loop; final owned move after loop exits.
    must_accept_with_borrow_stubs(
        "loop-borrows-then-final-move",
        "fn consume(s: str) -> i64:\n    return str_len(s)\nfn main() -> i64:\n    let s = input(\"\")\n    let i: i64 = 0\n    while i < 2:\n        let _ = str_len(&s)\n        i = i + 1\n    let final = consume(s)\n    return final\n",
    );
}

#[test]
fn w0052a_28_nested_borrow_in_tuple_constructor() {
    // Borrowed reads used to build a fresh tuple of i64s. The tuple
    // construction site reads `&s` twice; `s` is never consumed.
    must_accept_with_borrow_stubs(
        "nested-borrow-in-tuple",
        "fn main() -> i64:\n    let s = input(\"\")\n    let t = (str_len(&s), str_len(&s))\n    return t.0 + t.1\n",
    );
}

#[test]
fn w0052a_29_parenthesised_borrow_on_ident() {
    // ADR-0052a §8 Wave-1 cap: `&(complex_expr)` without parens is a
    // parse error, but `&(ident)` with redundant parens is accepted.
    // This codifies the parenthesisation rule positively.
    must_accept_with_borrow_stubs(
        "parenthesised-borrow-ident",
        "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&(s))\n    return n\n",
    );
}

#[test]
fn w0052a_30_borrow_passed_to_user_fn_with_typed_arg() {
    // §4.5 corollary — multiple `&label` calls into a user-defined fn
    // that takes `s: str`; transparency rule keeps both calls valid.
    must_accept_with_borrow_stubs(
        "user-fn-borrow-arg",
        "fn read_len(s: str) -> i64:\n    let n = str_len(&s)\n    return n\nfn main() -> i64:\n    let label = input(\"\")\n    let a = read_len(&label)\n    let b = read_len(&label)\n    return a + b\n",
    );
}

// ============================================================
// ADR-0052d-prereq Wave 2 — method-dispatch infrastructure corpus
//
// 25 well-typed programs covering the four new per-type method
// tables (Str / List / Float / Int) that ADR-0052d-prereq §4
// "Surface" enumerates. Each test asserts that the method-form
// `base.method(args)` parses + type-checks clean to the same return
// type as the equivalent PRELUDE-fn call (e.g. `s.split(",")` ≡
// `split(s, ",")`).
//
// Pre-DEV-impl status: every w0052dpre_* test below is `#[ignore]`'d
// pending Wave-2 DEV merge per F28 strict-separation PAIR pattern
// (`findings/adsd-pair-pattern-impl-gap.md`). DEV's responsibility
// is to (a) land four `try_synth_*_method` fns next to the existing
// `try_synth_dict_method`, (b) wire them into the `synth_call`
// chain, (c) add `TypeError::UnknownMethod { type_name, method_name,
// span, suggestion }` per ADR-0052d-prereq §"New error variant",
// then remove the `#[ignore]` markers.
//
// Stub strategy mirrors the existing `STR_STDLIB_STUBS` /
// `DICT_METHOD_STUBS` idiom: tests prepend a PRELUDE-fn stub block
// (`METHOD_DISPATCH_STUBS`) declaring the canonical signatures so
// the type checker has names to consult both for the method-form
// (when DEV rewrites it) AND for hand-authored PRELUDE-fn calls
// that the corpus uses as cross-checks.
//
// Coverage map (mirrors ADR-0052d-prereq §4 "Surface" table):
//   - Str (10):   w0052dpre_01..10
//   - List (5):   w0052dpre_11..15
//   - Float (5):  w0052dpre_16..20
//   - Int (5):    w0052dpre_21..25
// ============================================================

// Shared stub block: PRELUDE-fn signatures the four new method
// tables rewrite into. Mirrors `STR_STDLIB_STUBS` +
// `DICT_METHOD_STUBS` so the corpus lands BEFORE DEV graduates the
// stubs into the canonical PRELUDE.
//
// Note on `len` polymorphism: `xs.len()` per ADR-0052d-prereq §4
// rewrites to `len(xs)` (polymorphic; already wired at
// `check.rs:1710`). `s.len()` rewrites to `str_len(s)` (str-only
// signature) per ADR-0052d-prereq §4 row 1. Two distinct PRELUDE
// targets — the method-table dispatch carries the discriminating
// receiver type.
const METHOD_DISPATCH_STUBS: &str = concat!(
    // Str method targets (10).
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
    // List method targets (5). `list_push` / `list_get` / `list_set`
    // / `list_is_empty` / `len` (polymorphic). For w0052dpre_*
    // tests, we use `list[i64]` shape uniformly so the polymorphic
    // intrinsics resolve unambiguously.
    "fn list_push(xs: list[i64], v: i64) -> i64:\n    return 0\n",
    "fn list_get(xs: list[i64], i: i64) -> i64:\n    return 0\n",
    "fn list_set(xs: list[i64], i: i64, v: i64) -> i64:\n    return 0\n",
    "fn list_is_empty(xs: list[i64]) -> bool:\n    return True\n",
    "fn len(xs: list[i64]) -> i64:\n    return 0\n",
    // Float method targets (5). `abs_f` matches ADR-0052d-prereq §4
    // row "f.abs() → abs_f(f)" — separate from int `abs`.
    "fn floor(f: f64) -> f64:\n    return f\n",
    "fn ceil(f: f64) -> f64:\n    return f\n",
    "fn is_nan(f: f64) -> bool:\n    return False\n",
    "fn is_finite(f: f64) -> bool:\n    return True\n",
    "fn abs_f(f: f64) -> f64:\n    return f\n",
    // Int method targets (5).
    "fn abs(n: i64) -> i64:\n    return n\n",
    "fn pow(n: i64, k: i64) -> i64:\n    return 0\n",
    "fn min(a: i64, b: i64) -> i64:\n    return a\n",
    "fn max(a: i64, b: i64) -> i64:\n    return a\n",
    "fn bit_count(n: i64) -> i64:\n    return 0\n",
);

fn must_accept_with_method_dispatch_stubs(name: &str, body: &str) {
    let src = format!("{METHOD_DISPATCH_STUBS}{body}");
    must_accept(name, &src);
}

// ---- Tier A: Str method forms (w0052dpre_01..10) ----

#[test]
fn w0052dpre_01_str_len_method_form() {
    // ADR-0052d-prereq §4 row 1: `s.len()` → `str_len(s)`. Return i64.
    must_accept_with_method_dispatch_stubs(
        "str-len-method",
        "fn f() -> i64:\n    let s: str = \"hello\"\n    let n: i64 = s.len()\n    return n\n",
    );
}

#[test]
fn w0052dpre_02_str_split_method_form() {
    // ADR-0052d-prereq §4 row 2: `s.split(",")` → `split(s, ",")`. Return list[str].
    must_accept_with_method_dispatch_stubs(
        "str-split-method",
        "fn f() -> list[str]:\n    let s: str = \"a,b,c\"\n    let xs: list[str] = s.split(\",\")\n    return xs\n",
    );
}

#[test]
fn w0052dpre_03_str_replace_method_form() {
    // ADR-0052d-prereq §4 row 3: `s.replace(a, b)` → `replace(s, a, b)`. Return str.
    must_accept_with_method_dispatch_stubs(
        "str-replace-method",
        "fn f() -> str:\n    let s: str = \"foo bar\"\n    let t: str = s.replace(\"bar\", \"baz\")\n    return t\n",
    );
}

#[test]
fn w0052dpre_04_str_trim_method_form() {
    // ADR-0052d-prereq §4 row 4: `s.trim()` → `trim(s)`. Return str.
    must_accept_with_method_dispatch_stubs(
        "str-trim-method",
        "fn f() -> str:\n    let s: str = \"  hi  \"\n    let t: str = s.trim()\n    return t\n",
    );
}

#[test]
fn w0052dpre_05_str_find_method_form() {
    // ADR-0052d-prereq §4 row 5: `s.find("x")` → `find(s, "x")`. Return i64.
    must_accept_with_method_dispatch_stubs(
        "str-find-method",
        "fn f() -> i64:\n    let s: str = \"abc\"\n    let i: i64 = s.find(\"b\")\n    return i\n",
    );
}

#[test]
fn w0052dpre_06_str_contains_method_form() {
    // ADR-0052d-prereq §4 row 6: `s.contains("x")` → `contains(s, "x")`. Return bool.
    must_accept_with_method_dispatch_stubs(
        "str-contains-method",
        "fn f() -> bool:\n    let s: str = \"abc\"\n    let b: bool = s.contains(\"b\")\n    return b\n",
    );
}

#[test]
fn w0052dpre_07_str_starts_with_method_form() {
    // ADR-0052d-prereq §4 row 7: `s.starts_with("x")` → `starts_with(s, "x")`. Return bool.
    must_accept_with_method_dispatch_stubs(
        "str-starts-with-method",
        "fn f() -> bool:\n    let s: str = \"prefix-rest\"\n    let b: bool = s.starts_with(\"prefix\")\n    return b\n",
    );
}

#[test]
fn w0052dpre_08_str_ends_with_method_form() {
    // ADR-0052d-prereq §4 row 8: `s.ends_with("x")` → `ends_with(s, "x")`. Return bool.
    must_accept_with_method_dispatch_stubs(
        "str-ends-with-method",
        "fn f() -> bool:\n    let s: str = \"rest-suffix\"\n    let b: bool = s.ends_with(\"suffix\")\n    return b\n",
    );
}

#[test]
fn w0052dpre_09_str_lower_method_form() {
    // ADR-0052d-prereq §4 row 9: `s.lower()` → `lower(s)`. Return str.
    must_accept_with_method_dispatch_stubs(
        "str-lower-method",
        "fn f() -> str:\n    let s: str = \"HELLO\"\n    let t: str = s.lower()\n    return t\n",
    );
}

#[test]
fn w0052dpre_10_str_upper_method_form() {
    // ADR-0052d-prereq §4 row 10: `s.upper()` → `upper(s)`. Return str.
    must_accept_with_method_dispatch_stubs(
        "str-upper-method",
        "fn f() -> str:\n    let s: str = \"hello\"\n    let t: str = s.upper()\n    return t\n",
    );
}

// ---- Tier B: List method forms (w0052dpre_11..15) ----

#[test]
fn w0052dpre_11_list_len_method_form() {
    // ADR-0052d-prereq §4 row 11: `xs.len()` → `len(xs)` (polymorphic per
    // check.rs:1710). Return i64.
    must_accept_with_method_dispatch_stubs(
        "list-len-method",
        "fn f() -> i64:\n    let xs: list[i64] = [1, 2, 3]\n    let n: i64 = xs.len()\n    return n\n",
    );
}

#[test]
fn w0052dpre_12_list_push_method_form() {
    // ADR-0052d-prereq §4 row 12: `xs.push(v)` → `list_push(xs, v)`. Return () (i64 stub).
    must_accept_with_method_dispatch_stubs(
        "list-push-method",
        "fn f() -> i64:\n    let xs: list[i64] = [1, 2]\n    let _ = xs.push(3)\n    return 0\n",
    );
}

#[test]
fn w0052dpre_13_list_get_method_form() {
    // ADR-0052d-prereq §4 row 13: `xs.get(i)` → `list_get(xs, i)` (polymorphic
    // per check.rs:1696). Return T (i64 in this fixture).
    must_accept_with_method_dispatch_stubs(
        "list-get-method",
        "fn f() -> i64:\n    let xs: list[i64] = [10, 20, 30]\n    let v: i64 = xs.get(1)\n    return v\n",
    );
}

#[test]
fn w0052dpre_14_list_set_method_form() {
    // ADR-0052d-prereq §4 row 14: `xs.set(i, v)` → `list_set(xs, i, v)`
    // (polymorphic per check.rs:1697). Return () (i64 stub).
    must_accept_with_method_dispatch_stubs(
        "list-set-method",
        "fn f() -> i64:\n    let xs: list[i64] = [10, 20, 30]\n    let _ = xs.set(1, 99)\n    return 0\n",
    );
}

#[test]
fn w0052dpre_15_list_is_empty_method_form() {
    // ADR-0052d-prereq §4 row 15: `xs.is_empty()` → `list_is_empty(xs)`
    // (polymorphic per check.rs:1699). Return bool.
    must_accept_with_method_dispatch_stubs(
        "list-is-empty-method",
        "fn f() -> bool:\n    let xs: list[i64] = []\n    let b: bool = xs.is_empty()\n    return b\n",
    );
}

// ---- Tier C: Float method forms (w0052dpre_16..20) ----

#[test]
fn w0052dpre_16_float_floor_method_form() {
    // ADR-0052d-prereq §4 row 16: `f.floor()` → `floor(f)`. Return f64.
    must_accept_with_method_dispatch_stubs(
        "float-floor-method",
        "fn g() -> f64:\n    let x: f64 = 3.7\n    let y: f64 = x.floor()\n    return y\n",
    );
}

#[test]
fn w0052dpre_17_float_ceil_method_form() {
    // ADR-0052d-prereq §4 row 17: `f.ceil()` → `ceil(f)`. Return f64.
    must_accept_with_method_dispatch_stubs(
        "float-ceil-method",
        "fn g() -> f64:\n    let x: f64 = 3.2\n    let y: f64 = x.ceil()\n    return y\n",
    );
}

#[test]
fn w0052dpre_18_float_is_nan_method_form() {
    // ADR-0052d-prereq §4 row 18: `f.is_nan()` → `is_nan(f)`. Return bool.
    must_accept_with_method_dispatch_stubs(
        "float-is-nan-method",
        "fn g() -> bool:\n    let x: f64 = nan\n    let b: bool = x.is_nan()\n    return b\n",
    );
}

#[test]
fn w0052dpre_19_float_is_finite_method_form() {
    // ADR-0052d-prereq §4 row 19: `f.is_finite()` → `is_finite(f)`. Return bool.
    must_accept_with_method_dispatch_stubs(
        "float-is-finite-method",
        "fn g() -> bool:\n    let x: f64 = 1.0\n    let b: bool = x.is_finite()\n    return b\n",
    );
}

#[test]
fn w0052dpre_20_float_abs_method_form() {
    // ADR-0052d-prereq §4 row 20: `f.abs()` → `abs_f(f)` (NOT the int
    // `abs`; method-table dispatch chooses the right PRELUDE alias
    // by receiver type). Return f64.
    must_accept_with_method_dispatch_stubs(
        "float-abs-method",
        "fn g() -> f64:\n    let x: f64 = 0.0 - 3.5\n    let y: f64 = x.abs()\n    return y\n",
    );
}

// ---- Tier D: Int method forms (w0052dpre_21..25) ----

#[test]
fn w0052dpre_21_int_abs_method_form() {
    // ADR-0052d-prereq §4 row 21: `n.abs()` → `abs(n)`. Return i64.
    // Cross-disambiguation versus float `f.abs()` lives in the
    // method-table receiver-type guard (Ty::Int branch picks `abs`,
    // Ty::Float branch picks `abs_f`).
    must_accept_with_method_dispatch_stubs(
        "int-abs-method",
        "fn h() -> i64:\n    let n: i64 = 0 - 42\n    let m: i64 = n.abs()\n    return m\n",
    );
}

#[test]
fn w0052dpre_22_int_pow_method_form() {
    // ADR-0052d-prereq §4 row 22: `n.pow(k)` → `pow(n, k)`. Return i64.
    must_accept_with_method_dispatch_stubs(
        "int-pow-method",
        "fn h() -> i64:\n    let n: i64 = 2\n    let m: i64 = n.pow(10)\n    return m\n",
    );
}

#[test]
fn w0052dpre_23_int_min_method_form() {
    // ADR-0052d-prereq §4 row 23: `n.min(m)` → `min(n, m)`. Return i64.
    must_accept_with_method_dispatch_stubs(
        "int-min-method",
        "fn h() -> i64:\n    let n: i64 = 7\n    let m: i64 = n.min(3)\n    return m\n",
    );
}

#[test]
fn w0052dpre_24_int_max_method_form() {
    // ADR-0052d-prereq §4 row 24: `n.max(m)` → `max(n, m)`. Return i64.
    must_accept_with_method_dispatch_stubs(
        "int-max-method",
        "fn h() -> i64:\n    let n: i64 = 7\n    let m: i64 = n.max(3)\n    return m\n",
    );
}

#[test]
fn w0052dpre_25_int_bit_count_method_form() {
    // ADR-0052d-prereq §4 row 25: `n.bit_count()` → `bit_count(n)`. Return i64.
    must_accept_with_method_dispatch_stubs(
        "int-bit-count-method",
        "fn h() -> i64:\n    let n: i64 = 31\n    let m: i64 = n.bit_count()\n    return m\n",
    );
}

// ============================================================
// ADR-0052g Wave 2 round 2 — `&recv.method()` Copy-primitive borrow
//
// 5 well-typed programs covering the type-check arm narrowing per
// ADR-0052g §5 (admit `&Call(Attr(...))` when method returns a Copy
// primitive — `Int`, `Float`, `Bool`).
//
// Pre-DEV-impl status: every w0052g_* test below is `#[ignore]`'d
// pending Wave-2 round 2 DEV merge at `check.rs:888-891`. DEV
// removes the `#[ignore]` markers + lands the synth_expr branch
// narrowing per ADR-0052g §5 diff.
//
// Each test relies on the method-form rewrite chain (ADR-0052d-prereq)
// so we reuse `METHOD_DISPATCH_STUBS`. The outer `&` admission is the
// new behaviour under test.
// ============================================================

#[test]
fn w0052g_01_borrow_str_len_returns_ref_int() {
    // ADR-0052g §4.1 canonical witness — `&s.len()` admits as `Ref(Int)`.
    // Int is Copy → §5 `is_copy_primitive` returns true → admit.
    must_accept_with_method_dispatch_stubs(
        "borrow-str-len",
        "fn read_i64(n: i64) -> i64:\n    return n\nfn f() -> i64:\n    let s: str = \"hello\"\n    let r: i64 = read_i64(&s.len())\n    return r\n",
    );
}

#[test]
fn w0052g_02_borrow_list_len_returns_ref_int() {
    // ADR-0052g §4.1 List variant — `&xs.len()` admits as `Ref(Int)`.
    must_accept_with_method_dispatch_stubs(
        "borrow-list-len",
        "fn read_i64(n: i64) -> i64:\n    return n\nfn f() -> i64:\n    let xs: list[i64] = [1, 2, 3]\n    let r: i64 = read_i64(&xs.len())\n    return r\n",
    );
}

#[test]
fn w0052g_03_borrow_float_is_nan_returns_ref_bool() {
    // ADR-0052g §4.1 Bool variant — `&f.is_nan()` admits as `Ref(Bool)`.
    must_accept_with_method_dispatch_stubs(
        "borrow-float-is-nan",
        "fn read_bool(b: bool) -> bool:\n    return b\nfn f() -> bool:\n    let x: f64 = 1.0\n    let r: bool = read_bool(&x.is_nan())\n    return r\n",
    );
}

#[test]
fn w0052g_04_borrow_int_abs_returns_ref_int() {
    // ADR-0052g §4.1 Int.abs variant — `&n.abs()` admits as `Ref(Int)`.
    must_accept_with_method_dispatch_stubs(
        "borrow-int-abs",
        "fn read_i64(n: i64) -> i64:\n    return n\nfn f() -> i64:\n    let n: i64 = -5\n    let r: i64 = read_i64(&n.abs())\n    return r\n",
    );
}

#[test]
fn w0052g_05_borrow_str_len_at_fn_arg_one_way_coercion() {
    // ADR-0052g §2 motivation — the one-way call-site coercion at
    // `check.rs:1649-1661` drops the `Ref` wrapper when `Ref(Int)`
    // flows into an `i64` parameter slot. This is the originating
    // motivation from ADR-0052f §11.
    must_accept_with_method_dispatch_stubs(
        "borrow-str-len-fn-arg-coercion",
        "fn read_i64(n: i64) -> i64:\n    return n + 1\nfn f() -> i64:\n    let s: str = \"abc\"\n    return read_i64(&s.len())\n",
    );
}

// ============================================================
// ADR-0080 Phase-1a — class field tracking (well-typed side)
// (w196..w199)
//
// ADR-0080 §1.1 ground-truth: today `check_class` (check.rs:757-762)
// records NO field types into the Adt, and the `Attr` arm
// (check.rs:1291) returns `self.fresh_var()` for any user-class base
// — the verbatim comment "the static core does not yet track ADT
// fields" (check.rs:1260/1283). Phase-1a makes `check_class` record
// each class-body field declaration (`let <name>: <ty> = <init>`, the
// idiom from `tests/syntax-corpus/01_keywords.cb:59-60` + the parsed
// `ItemKind::Let` per ADR-0080 §1.1) into a per-Adt field table, and
// makes the `Attr` arm return the DECLARED field `Ty`.
//
// The class-field declaration idiom is the EXISTING corpus idiom:
//   `class Score:`
//   `    let name: str = ""`   ← str field
//   `    let rank: i64 = 0`    ← i64 field
// (mirrors `class Counter:\n    let count: i64 = 0`).
//
// INSTANCE-BINDING NOTE (load-bearing): these tests bind the instance
// WITHOUT a `: Score` type annotation (`let s = Score()`). An explicit
// `let s: Score = Score()` annotation is REJECTED at HEAD for a reason
// UNRELATED to field tracking — the class-name type annotation lowers
// to `Ty::Alias(AliasId)` while the zero-arg ctor returns
// `Ty::Adt(AdtId)` (prebind_item, check.rs:519-530), and the two do
// not unify (verified at 641e5f8: `TypeMismatch { expected: Alias,
// actual: Adt }`). The inferred binding lets `s` infer to the `Adt`
// the ctor returns, isolating the field-tracking behavior under test.
// (The Alias↔Adt unification gap is a separate seam, out of 1a scope.)
//
// These ACCEPT today (via fresh_var unifying with any annotation) and
// MUST KEEP ACCEPTING after 1a — now for the RIGHT reason: `s.rank`
// resolves to the declared `i64`, `s.name` to the declared `str`.

#[test]
fn w196_class_field_bound_at_declared_types() {
    // The §2.5 happy path: declared `i64` field → `i64` binding,
    // declared `str` field → `str` binding. Post-1a this passes because
    // the Attr arm returns the recorded declared field Ty (today it
    // passes only because fresh_var unifies with both annotations).
    must_accept(
        "class-field-bound-at-declared-types",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn f() -> i64:\n    let s = Score()\n    let r: i64 = s.rank\n    let n: str = s.name\n    return r\n",
    );
}

#[test]
fn w197_class_i64_field_in_i64_arith() {
    // Declared `i64` field used in i64 arithmetic — post-1a the field
    // type flows as `i64`, so `s.rank + 1` is well-typed `i64`.
    must_accept(
        "class-i64-field-in-i64-arith",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn f() -> i64:\n    let s = Score()\n    return (s.rank + 1)\n",
    );
}

#[test]
fn w198_class_str_field_in_str_concat() {
    // Declared `str` field used in str concatenation — post-1a `s.name`
    // is `str`, so `s.name + "!"` is well-typed `str`.
    must_accept(
        "class-str-field-in-str-concat",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn f() -> str:\n    let s = Score()\n    return (s.name + \"!\")\n",
    );
}

#[test]
fn w199_class_field_returned_at_declared_type() {
    // Declared `i64` field returned from an `-> i64` fn directly —
    // post-1a `s.rank` is `i64`, matching the return type.
    must_accept(
        "class-field-returned-at-declared-type",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn f() -> i64:\n    let s = Score()\n    return s.rank\n",
    );
}

// ============================================================
// ADR-0080 Phase-1b-i — class NAME in a type-annotation position
// resolves to the class's `Adt` (well-typed side) (w200..w202)
//
// RED MECHANISM (verified at HEAD e66dcfb, via the parse→lower→check
// harness path): a class-name annotation lowers through
// `lower_named_type` (check.rs:2904); a USER class name is unrecognised
// by every concrete arm, so it falls through to the opaque-alias arm
// (check.rs:2950-2956) and becomes a synthetic
// `Ty::Alias(AliasId(hash(name) | 0x8000_0000))`. The verbatim comment
// there: this "only unifies with another opaque alias of the same
// name." Meanwhile the zero-arg ctor `Score()` returns
// `Ty::Adt(AdtId(c.def_id.0))` (prebind_item, the `ItemKind::Class` arm,
// check.rs:552-562). `Alias` and `Adt` do NOT unify, so wherever the
// annotation is unified against a ctor result the check fails with
// `TypeMismatch { expected: Alias(AliasId(2383749825), []), actual:
// Adt(AdtId(0), []) }` (the exact payload observed at HEAD for the
// `Score` name).
//
// Phase-1b-i makes `lower_named_type` resolve a name that names a class
// in scope to that class's `Ty::Adt(AdtId(def_id), …)` — the SAME id
// the ctor's `return_ty` already carries — so the annotation and the
// instance unify. This is the seam the Phase-1a header (w196..w199 +
// i151..i154) called out as "a separate seam, out of 1a scope": those
// tests had to bind instances INFERRED (`let s = Score()`, no `: Score`)
// precisely because the explicit annotation was rejected here.
//
// SEAM NOTE (why the binding/call form, not a bare param): a bare param
// annotation (`fn uses(s: Score)`) is merely RECORDED as the param's
// type and is never contradicted on its own — at HEAD it silently
// lowers to `Alias` and the fn body type-checks (a class-typed param
// alone ACCEPTS today, masked further by the Phase-1a `fresh_var` Attr
// hole). The Alias↔Adt gap is only OBSERVABLE where the annotation is
// UNIFIED AGAINST a concrete `Adt` instance: a `let : Score = Score()`
// binding (w200), an `-> Score` return of `Score()` (w202), or a
// `uses(Score())` CALL whose `Score()` arg meets the `Score` param
// (w201). All three REJECT at HEAD = the RED these tests flip.
//
// These MUST FAIL today (RED — `must_accept` PANICS: "should accept but
// rejected: TypeMismatch { expected: Alias, actual: Adt }") and MUST
// pass after Phase-1b-i lands.

#[test]
fn w200_class_typed_binding_from_ctor() {
    // (a) The §2.5 happy path: an explicit `: Score` annotation on a
    // binding initialised from the `Score()` ctor. At HEAD the `Score`
    // annotation is `Ty::Alias` and the ctor is `Ty::Adt`, so the unify
    // fails (TypeMismatch{expected: Alias, actual: Adt}) — this is the
    // exact rejection the w196..w199 header documented as the reason
    // Phase-1a had to bind instances inferred. Post-1b-i the `Score`
    // annotation resolves to the ctor's `Adt`, so they unify.
    must_accept(
        "class-typed-binding-from-ctor",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn f() -> i64:\n    let s: Score = Score()\n    return 0\n",
    );
}

#[test]
fn w201_class_typed_param_reads_field() {
    // (b) A fn with a class-typed PARAM that READS a field, exercised at
    // a CALL site that forces the param annotation against a real
    // instance: `uses(Score())`. This COMBINES the two seams — 1b-i
    // (the `Score` param annotation must resolve to the class `Adt` so
    // the `Score()` arg unifies with it) AND 1a (the `s.rank` field
    // access must yield the declared `i64` to match `-> i64`). At HEAD
    // the `Score()` arg (`Adt`) meets the `Score` param (`Alias`) and
    // the call is REJECTED (TypeMismatch{expected: Alias, actual: Adt}
    // on the arg). Post-fix both are the same `Adt` and `s.rank` is
    // `i64`, so the whole program type-checks.
    must_accept(
        "class-typed-param-reads-field",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn uses(s: Score) -> i64:\n    return s.rank\nfn main() -> i64:\n    return uses(Score())\n",
    );
}

#[test]
fn w202_class_typed_return_from_ctor() {
    // (c) A class-typed RETURN: `fn make() -> Score: return Score()`.
    // At HEAD the declared return type `Score` is `Ty::Alias` while the
    // returned `Score()` is `Ty::Adt`, so the return-type unify fails
    // (TypeMismatch{expected: Alias, actual: Adt}). Post-1b-i the `Score`
    // return annotation resolves to the ctor's `Adt`, so the returned
    // instance matches the declared return type.
    must_accept(
        "class-typed-return-from-ctor",
        "class Score:\n    let name: str = \"\"\n    let rank: i64 = 0\nfn make() -> Score:\n    return Score()\n",
    );
}
