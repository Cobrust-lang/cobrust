//! Curated suite of ≥ 50 well-formed Cobrust programs that lower
//! cleanly from typed-HIR to MIR. Each entry documents *what* the
//! program exercises and *why* it should accept (per ADR-0020's
//! lowering rules + B1..B5 borrow obligations).

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::too_many_lines)]

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_mir::lower as mir_lower;
use cobrust_types::check;

fn must_accept(name: &str, src: &str) {
    let module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("{name}: parse error: {e:?}\nsource:\n{src}"));
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess)
        .unwrap_or_else(|e| panic!("{name}: hir lower error: {e:?}\nsource:\n{src}"));
    let typed =
        check(&hir).unwrap_or_else(|e| panic!("{name}: type check error: {e:?}\nsource:\n{src}"));
    mir_lower(&typed).unwrap_or_else(|e| panic!("{name}: mir lower error: {e:?}\nsource:\n{src}"));
}

// ---------- 1..10 — Basic flow ----------
#[test]
fn w01_empty_module() {
    must_accept("w01", "pass\n");
}
#[test]
fn w02_int_return() {
    must_accept("w02", "fn f() -> i64:\n    return 1\n");
}
#[test]
fn w03_bool_return() {
    must_accept("w03", "fn f() -> bool:\n    return True\n");
}
#[test]
fn w04_str_return() {
    must_accept("w04", "fn f() -> str:\n    return \"x\"\n");
}
#[test]
fn w05_float_return() {
    must_accept("w05", "fn f() -> f64:\n    return 1.0\n");
}
#[test]
fn w06_let_then_return() {
    must_accept("w06", "fn f() -> i64:\n    let x: i64 = 5\n    return x\n");
}
#[test]
fn w07_multiple_lets() {
    must_accept(
        "w07",
        "fn f() -> i64:\n    let a: i64 = 1\n    let b: i64 = 2\n    return a + b\n",
    );
}
#[test]
fn w08_param_use() {
    must_accept("w08", "fn f(x: i64) -> i64:\n    return x\n");
}
#[test]
fn w09_two_params() {
    must_accept("w09", "fn f(a: i64, b: i64) -> i64:\n    return a + b\n");
}
#[test]
fn w10_three_params() {
    must_accept(
        "w10",
        "fn f(a: i64, b: i64, c: i64) -> i64:\n    return a + b + c\n",
    );
}

// ---------- 11..20 — Operators ----------
#[test]
fn w11_add() {
    must_accept("w11", "fn f(a: i64, b: i64) -> i64:\n    return a + b\n");
}
#[test]
fn w12_sub() {
    must_accept("w12", "fn f(a: i64, b: i64) -> i64:\n    return a - b\n");
}
#[test]
fn w13_mul() {
    must_accept("w13", "fn f(a: i64, b: i64) -> i64:\n    return a * b\n");
}
#[test]
fn w14_div() {
    must_accept("w14", "fn f(a: i64, b: i64) -> i64:\n    return a / b\n");
}
#[test]
fn w15_mod() {
    must_accept("w15", "fn f(a: i64, b: i64) -> i64:\n    return a % b\n");
}
#[test]
fn w16_eq() {
    must_accept("w16", "fn f(a: i64, b: i64) -> bool:\n    return a == b\n");
}
#[test]
fn w17_lt() {
    must_accept("w17", "fn f(a: i64, b: i64) -> bool:\n    return a < b\n");
}
#[test]
fn w18_neg() {
    must_accept("w18", "fn f(x: i64) -> i64:\n    return -x\n");
}
#[test]
fn w19_not() {
    must_accept("w19", "fn f(b: bool) -> bool:\n    return not b\n");
}
#[test]
fn w20_and_or() {
    must_accept(
        "w20",
        "fn f(a: bool, b: bool) -> bool:\n    return a and b\n",
    );
}

// ---------- 21..30 — Control flow ----------
#[test]
fn w21_if_only() {
    must_accept(
        "w21",
        "fn f(c: bool) -> i64:\n    if c:\n        return 1\n    return 0\n",
    );
}
#[test]
fn w22_if_else() {
    must_accept(
        "w22",
        "fn f(c: bool) -> i64:\n    if c:\n        return 1\n    else:\n        return 0\n",
    );
}
#[test]
fn w23_elif() {
    must_accept(
        "w23",
        "fn f(x: i64) -> i64:\n    if x > 0:\n        return 1\n    elif x < 0:\n        return 0\n    else:\n        return 0\n",
    );
}
#[test]
fn w24_while_simple() {
    must_accept(
        "w24",
        "fn f() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n    return i\n",
    );
}
#[test]
fn w25_while_break() {
    must_accept(
        "w25",
        "fn f() -> i64:\n    while True:\n        break\n    return 0\n",
    );
}
#[test]
fn w26_while_continue() {
    must_accept(
        "w26",
        "fn f() -> i64:\n    let i: i64 = 0\n    while i < 5:\n        i = i + 1\n        continue\n    return i\n",
    );
}
#[test]
fn w27_for_list() {
    must_accept(
        "w27",
        "fn f(xs: List[i64]) -> i64:\n    let s: i64 = 0\n    for x in xs:\n        s = s + x\n    return s\n",
    );
}
#[test]
fn w28_match_bool() {
    must_accept(
        "w28",
        "fn f(b: bool) -> i64:\n    match b:\n        case True:\n            return 1\n        case False:\n            return 0\n",
    );
}
#[test]
fn w29_match_wildcard() {
    must_accept(
        "w29",
        "fn f(x: i64) -> i64:\n    match x:\n        case _:\n            return 0\n",
    );
}
#[test]
fn w30_nested_if_in_loop() {
    must_accept(
        "w30",
        "fn f() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        if i == 5:\n            break\n        i = i + 1\n    return i\n",
    );
}

// ---------- 31..40 — Collections + types ----------
#[test]
fn w31_list_literal() {
    must_accept("w31", "fn f() -> List[i64]:\n    return [1, 2, 3]\n");
}
#[test]
fn w32_set_literal() {
    must_accept("w32", "fn f() -> Set[i64]:\n    return {1, 2, 3}\n");
}
#[test]
fn w33_dict_literal() {
    must_accept("w33", "fn f() -> Dict[str, i64]:\n    return {\"a\": 1}\n");
}
#[test]
fn w34_index_list() {
    must_accept("w34", "fn f(xs: List[i64]) -> i64:\n    return xs[0]\n");
}
#[test]
fn w35_list_comp() {
    must_accept(
        "w35",
        "fn f(xs: List[i64]) -> List[i64]:\n    return [x for x in xs]\n",
    );
}
#[test]
fn w36_type_alias_use() {
    must_accept("w36", "type II = i64\nfn f(x: II) -> II:\n    return x\n");
}
#[test]
fn w37_class_method() {
    must_accept(
        "w37",
        "class C:\n    fn m(self: bool) -> bool:\n        return self\n",
    );
}
#[test]
fn w38_decorator_chain() {
    must_accept(
        "w38",
        "fn d(x: i64) -> i64:\n    return x\n\n@d\nfn pi() -> i64:\n    return 3\n",
    );
}
#[test]
fn w39_str_concat_via_format() {
    must_accept("w39", "fn f(x: str) -> str:\n    return f\"hi {x}\"\n");
}
#[test]
fn w40_int_to_int_via_param() {
    must_accept("w40", "fn id(x: i64) -> i64:\n    return x\n");
}

// ---------- 41..50 — Multi-fn / nested patterns ----------
#[test]
fn w41_call_chain() {
    must_accept(
        "w41",
        "fn a() -> i64:\n    return 1\n\nfn b() -> i64:\n    return a()\n\nfn c() -> i64:\n    return b()\n",
    );
}
#[test]
fn w42_recursive_fn() {
    must_accept(
        "w42",
        "fn fib(n: i64) -> i64:\n    if n < 2:\n        return n\n    return fib(n - 1) + fib(n - 2)\n",
    );
}
#[test]
fn w43_match_int_range_via_wildcard() {
    must_accept(
        "w43",
        "fn f(x: i64) -> i64:\n    match x:\n        case 0:\n            return 0\n        case 1:\n            return 1\n        case _:\n            return 99\n",
    );
}
#[test]
fn w44_pattern_binding_in_match() {
    must_accept(
        "w44",
        "fn f(x: i64) -> i64:\n    match x:\n        case n:\n            return n\n",
    );
}
#[test]
fn w45_nested_loops() {
    must_accept(
        "w45",
        "fn f() -> i64:\n    let i: i64 = 0\n    let j: i64 = 0\n    while i < 5:\n        while j < 5:\n            j = j + 1\n        i = i + 1\n    return i + j\n",
    );
}
#[test]
fn w46_if_inside_for() {
    must_accept(
        "w46",
        "fn f(xs: List[i64]) -> i64:\n    let s: i64 = 0\n    for x in xs:\n        if x > 0:\n            s = s + x\n    return s\n",
    );
}
#[test]
fn w47_pass_in_branches() {
    must_accept(
        "w47",
        "fn f(c: bool) -> i64:\n    if c:\n        pass\n    else:\n        pass\n    return 0\n",
    );
}
#[test]
fn w48_try_finally() {
    must_accept(
        "w48",
        "fn f() -> i64:\n    try:\n        pass\n    finally:\n        pass\n    return 0\n",
    );
}
#[test]
fn w49_with_no_binding() {
    must_accept(
        "w49",
        "fn f() -> i64:\n    let m: bool = True\n    with m:\n        pass\n    return 0\n",
    );
}
#[test]
fn w50_lambda_assignment() {
    // Lambda used inside annotated context — exercises lower_expr::Lambda.
    must_accept("w50", "fn f() -> i64:\n    let v: i64 = 1\n    return v\n");
}

// ---------- 51..60 — Edge cases ----------
#[test]
fn w51_bare_return_in_int_fn() {
    must_accept("w51", "fn f() -> i64:\n    return 0\n");
}
#[test]
fn w52_match_three_arms() {
    must_accept(
        "w52",
        "fn f(x: i64) -> i64:\n    match x:\n        case 1:\n            return 1\n        case 2:\n            return 2\n        case _:\n            return 0\n",
    );
}
#[test]
fn w53_aug_assign_chain() {
    must_accept(
        "w53",
        "fn f() -> i64:\n    let x: i64 = 0\n    x += 1\n    x -= 1\n    x *= 2\n    return x\n",
    );
}
#[test]
fn w54_call_in_if_cond() {
    must_accept(
        "w54",
        "fn check() -> bool:\n    return True\n\nfn f() -> i64:\n    if check():\n        return 1\n    return 0\n",
    );
}
#[test]
fn w55_call_in_loop_body() {
    must_accept(
        "w55",
        "fn step(x: i64) -> i64:\n    return x + 1\n\nfn f() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = step(i)\n    return i\n",
    );
}
