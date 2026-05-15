//! ADR-0050a M-F.3.0 types corpus for `break` / `continue`.
//!
//! Covers:
//! - Type checker accepts break/continue inside loop scope, including
//!   nested loops, inside if/elif/else within a loop, inside match/with.
//! - Type checker rejects break/continue outside any loop:
//!   - module top-level
//!   - function body top-level
//!   - inside an `if` that is not in a loop
//!   - inside a `match` arm that is not in a loop
//!   - inside a nested function whose outer scope is inside a loop
//!     (loop scope MUST NOT cross function boundaries)
//! - 18-lint clippy module-level allow header per
//!   `feedback_p9_clippy_stall_pattern.md`.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::single_match_else)]
#![allow(clippy::single_match)]
#![allow(clippy::needless_return)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::uninlined_format_args)]

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower};
use cobrust_types::{TypeError, check};

#[derive(Clone, Copy, Debug)]
enum ExpectedErr {
    BreakOutsideLoop,
    ContinueOutsideLoop,
}

fn must_accept(name: &str, src: &str) {
    let module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("{name}: parse failed: {e:?}\n--- src ---\n{src}"));
    let mut sess = Session::new();
    let hir = lower(&module, &mut sess)
        .unwrap_or_else(|e| panic!("{name}: hir lower failed: {e:?}\n--- src ---\n{src}"));
    if let Err(e) = check(&hir) {
        panic!("{name}: type check failed: {e:?}\n--- src ---\n{src}");
    }
}

fn must_reject(name: &str, src: &str, expected: ExpectedErr) {
    let module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("{name}: parse failed: {e:?}\n--- src ---\n{src}"));
    let mut sess = Session::new();
    match lower(&module, &mut sess) {
        Err(_) => return,
        Ok(hir) => match check(&hir) {
            Ok(_) => panic!("{name}: must reject but type check passed\n--- src ---\n{src}"),
            Err(e) => {
                let ok = match (expected, &e) {
                    (ExpectedErr::BreakOutsideLoop, TypeError::BreakOutsideLoop { .. }) => true,
                    (ExpectedErr::ContinueOutsideLoop, TypeError::ContinueOutsideLoop { .. }) => {
                        true
                    }
                    _ => false,
                };
                assert!(
                    ok,
                    "{}: rejected with wrong category\n  expected: {:?}\n  got:      {:?}\n  src:\n{}",
                    name, expected, e, src
                );
            }
        },
    }
}

// =====================================================================
// Section A — well-typed acceptance (≥15 cases)
// =====================================================================

#[test]
fn a01_break_in_while_body() {
    must_accept(
        "a01",
        "fn main() -> i64:\n    while True:\n        break\n    return 0\n",
    );
}

#[test]
fn a02_continue_in_while_body() {
    must_accept(
        "a02",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 5:\n        i = i + 1\n        continue\n    return 0\n",
    );
}

#[test]
fn a03_break_in_if_in_while() {
    must_accept(
        "a03",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        if i == 3:\n            break\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn a04_continue_in_elif_in_while() {
    must_accept(
        "a04",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n        if i == 3:\n            pass\n        elif i == 5:\n            continue\n        else:\n            pass\n    return 0\n",
    );
}

#[test]
fn a05_break_in_else_in_while() {
    must_accept(
        "a05",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n        if i < 5:\n            pass\n        else:\n            break\n    return 0\n",
    );
}

#[test]
fn a06_break_in_inner_of_nested_while() {
    must_accept(
        "a06",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        let j: i64 = 0\n        while j < 3:\n            if j == 1:\n                break\n            j = j + 1\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn a07_continue_in_inner_of_nested_while() {
    must_accept(
        "a07",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        let j: i64 = 0\n        while j < 3:\n            j = j + 1\n            if j == 2:\n                continue\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn a08_break_in_outer_after_inner_loop() {
    // The outer `break` is inside the outer loop (outer body, after the
    // inner while). Loop scope = outer.
    must_accept(
        "a08",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        let j: i64 = 0\n        while j < 3:\n            j = j + 1\n        if i == 1:\n            break\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn a09_deep_nested_break_at_innermost() {
    // 5-level nesting; break binds to innermost.
    must_accept(
        "a09",
        "fn main() -> i64:\n    let a: i64 = 0\n    while a < 2:\n        let b: i64 = 0\n        while b < 2:\n            let c: i64 = 0\n            while c < 2:\n                let d: i64 = 0\n                while d < 2:\n                    let e: i64 = 0\n                    while e < 2:\n                        if e == 1:\n                            break\n                        e = e + 1\n                    d = d + 1\n                c = c + 1\n            b = b + 1\n        a = a + 1\n    return 0\n",
    );
}

#[test]
fn a10_break_and_continue_together() {
    must_accept(
        "a10",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 100:\n        i = i + 1\n        if i == 7:\n            continue\n        if i == 13:\n            break\n    return 0\n",
    );
}

#[test]
fn a11_break_inside_while_else_clause() {
    // The `else` block of `while...else` does NOT enter loop scope —
    // but at parse + types level, our check_loop reaches the else
    // ONLY after popping loop_depth. So break inside the else_block
    // should fail. Test this in section B (rejection).
    //
    // Here just verify that a break inside the main body of a while
    // that ALSO has an else clause is accepted.
    must_accept(
        "a11",
        "fn main() -> i64:\n    let i: i64 = 0\n    let z: i64 = 0\n    while i < 5:\n        if i == 3:\n            break\n        i = i + 1\n    else:\n        z = 99\n    return z\n",
    );
}

#[test]
fn a12_break_after_early_assign() {
    must_accept(
        "a12",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n        if i == 4:\n            i = 99\n            break\n    return 0\n",
    );
}

#[test]
fn a13_continue_after_early_assign() {
    must_accept(
        "a13",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n        if i == 4:\n            i = i + 2\n            continue\n    return 0\n",
    );
}

#[test]
fn a14_multiple_breaks_same_loop() {
    must_accept(
        "a14",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 100:\n        i = i + 1\n        if i == 5:\n            break\n        if i == 7:\n            break\n    return 0\n",
    );
}

#[test]
fn a15_multiple_continues_same_loop() {
    must_accept(
        "a15",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 100:\n        i = i + 1\n        if i == 3:\n            continue\n        if i == 5:\n            continue\n    return 0\n",
    );
}

#[test]
fn a16_continue_then_break() {
    must_accept(
        "a16",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 100:\n        i = i + 1\n        if i == 3:\n            continue\n        if i == 10:\n            break\n    return 0\n",
    );
}

#[test]
fn a17_break_inside_match_inside_while() {
    must_accept(
        "a17",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 5:\n        i = i + 1\n        match i:\n            case 3:\n                break\n            case _:\n                pass\n    return 0\n",
    );
}

#[test]
fn a18_continue_inside_match_inside_while() {
    must_accept(
        "a18",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 5:\n        i = i + 1\n        match i:\n            case 2:\n                continue\n            case _:\n                pass\n    return 0\n",
    );
}

// =====================================================================
// Section B — type checker rejection (≥10 cases)
// =====================================================================

#[test]
fn b01_break_at_module_top() {
    must_reject("b01", "break\n", ExpectedErr::BreakOutsideLoop);
}

#[test]
fn b02_continue_at_module_top() {
    must_reject("b02", "continue\n", ExpectedErr::ContinueOutsideLoop);
}

#[test]
fn b03_break_at_fn_body_top() {
    must_reject(
        "b03",
        "fn f() -> i64:\n    break\n    return 0\n",
        ExpectedErr::BreakOutsideLoop,
    );
}

#[test]
fn b04_continue_at_fn_body_top() {
    must_reject(
        "b04",
        "fn f() -> i64:\n    continue\n    return 0\n",
        ExpectedErr::ContinueOutsideLoop,
    );
}

#[test]
fn b05_break_inside_if_outside_loop() {
    must_reject(
        "b05",
        "fn f(x: i64) -> i64:\n    if x == 1:\n        break\n    return 0\n",
        ExpectedErr::BreakOutsideLoop,
    );
}

#[test]
fn b06_continue_inside_if_outside_loop() {
    must_reject(
        "b06",
        "fn f(x: i64) -> i64:\n    if x == 1:\n        continue\n    return 0\n",
        ExpectedErr::ContinueOutsideLoop,
    );
}

#[test]
fn b07_break_inside_elif_outside_loop() {
    must_reject(
        "b07",
        "fn f(x: i64) -> i64:\n    if x == 1:\n        return 1\n    elif x == 2:\n        break\n    return 0\n",
        ExpectedErr::BreakOutsideLoop,
    );
}

#[test]
fn b08_continue_inside_else_outside_loop() {
    must_reject(
        "b08",
        "fn f(x: i64) -> i64:\n    if x == 1:\n        return 1\n    else:\n        continue\n    return 0\n",
        ExpectedErr::ContinueOutsideLoop,
    );
}

#[test]
fn b09_break_inside_match_outside_loop() {
    must_reject(
        "b09",
        "fn f(x: i64) -> i64:\n    match x:\n        case 0:\n            break\n        case _:\n            return 1\n    return 0\n",
        ExpectedErr::BreakOutsideLoop,
    );
}

#[test]
fn b10_continue_inside_match_outside_loop() {
    must_reject(
        "b10",
        "fn f(x: i64) -> i64:\n    match x:\n        case 0:\n            continue\n        case _:\n            return 1\n    return 0\n",
        ExpectedErr::ContinueOutsideLoop,
    );
}

#[test]
fn b11_break_in_else_clause_of_while() {
    // While-else's `else` runs after the loop's natural cond-false
    // termination — it is NOT inside the loop's iteration scope.
    // ADR-0050a binding: break/continue inside the else clause must
    // reject.
    must_reject(
        "b11",
        "fn f() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        i = i + 1\n    else:\n        break\n    return 0\n",
        ExpectedErr::BreakOutsideLoop,
    );
}

#[test]
fn b12_continue_in_else_clause_of_while() {
    must_reject(
        "b12",
        "fn f() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        i = i + 1\n    else:\n        continue\n    return 0\n",
        ExpectedErr::ContinueOutsideLoop,
    );
}

#[test]
fn b13_break_in_nested_fn_inside_loop() {
    // The nested function definition creates a new loop scope —
    // `break` inside it should reject even though the OUTER body is
    // inside a loop.
    must_reject(
        "b13",
        "fn outer() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        fn inner() -> i64:\n            break\n            return 0\n        i = i + 1\n    return 0\n",
        ExpectedErr::BreakOutsideLoop,
    );
}

#[test]
fn b14_continue_in_nested_fn_inside_loop() {
    must_reject(
        "b14",
        "fn outer() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        fn inner() -> i64:\n            continue\n            return 0\n        i = i + 1\n    return 0\n",
        ExpectedErr::ContinueOutsideLoop,
    );
}

#[test]
fn b15_break_after_loop_ends() {
    // The break is in the outer fn body AFTER the loop has been exited.
    must_reject(
        "b15",
        "fn f() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        i = i + 1\n    break\n    return 0\n",
        ExpectedErr::BreakOutsideLoop,
    );
}

#[test]
fn b16_continue_after_loop_ends() {
    must_reject(
        "b16",
        "fn f() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        i = i + 1\n    continue\n    return 0\n",
        ExpectedErr::ContinueOutsideLoop,
    );
}
