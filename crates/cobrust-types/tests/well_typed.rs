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
