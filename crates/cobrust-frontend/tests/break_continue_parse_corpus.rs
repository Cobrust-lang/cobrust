//! ADR-0050a M-F.3.0 parse corpus for `break` / `continue`.
//!
//! Covers:
//! - Parser accepts bare `break` / `continue` keywords in well-formed
//!   positions (loop body, nested-if-in-loop, nested-loop, etc.).
//! - Round-trip preserves AST shape (parse → unparse → re-parse) for
//!   the count of `break` / `continue` statements.
//! - Parser rejects malformed shapes (`break <label>`, `continue 0`,
//!   `break;`, `break(` etc.).
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
#![allow(clippy::single_match)]
#![allow(clippy::single_match_else)]

use cobrust_frontend::ast::{BreakKind, Module, Stmt, StmtKind};
use cobrust_frontend::span::FileId;
use cobrust_frontend::{parse_str, unparse};

fn visit_stmt(s: &Stmt, breaks: &mut usize, continues: &mut usize) {
    match &s.kind {
        StmtKind::BreakContinue(BreakKind::Break) => *breaks += 1,
        StmtKind::BreakContinue(BreakKind::Continue) => *continues += 1,
        StmtKind::If {
            then_block,
            elifs,
            else_block,
            ..
        } => {
            for st in &then_block.stmts {
                visit_stmt(st, breaks, continues);
            }
            for (_, blk) in elifs {
                for st in &blk.stmts {
                    visit_stmt(st, breaks, continues);
                }
            }
            if let Some(b) = else_block {
                for st in &b.stmts {
                    visit_stmt(st, breaks, continues);
                }
            }
        }
        StmtKind::While {
            body, else_block, ..
        } => {
            for st in &body.stmts {
                visit_stmt(st, breaks, continues);
            }
            if let Some(b) = else_block {
                for st in &b.stmts {
                    visit_stmt(st, breaks, continues);
                }
            }
        }
        StmtKind::For {
            body, else_block, ..
        } => {
            for st in &body.stmts {
                visit_stmt(st, breaks, continues);
            }
            if let Some(b) = else_block {
                for st in &b.stmts {
                    visit_stmt(st, breaks, continues);
                }
            }
        }
        StmtKind::Match { arms, .. } => {
            for a in arms {
                for st in &a.body.stmts {
                    visit_stmt(st, breaks, continues);
                }
            }
        }
        StmtKind::With { body, .. } => {
            for st in &body.stmts {
                visit_stmt(st, breaks, continues);
            }
        }
        StmtKind::Try {
            body,
            handlers,
            else_block,
            finally_block,
        } => {
            for st in &body.stmts {
                visit_stmt(st, breaks, continues);
            }
            for h in handlers {
                for st in &h.body.stmts {
                    visit_stmt(st, breaks, continues);
                }
            }
            if let Some(b) = else_block {
                for st in &b.stmts {
                    visit_stmt(st, breaks, continues);
                }
            }
            if let Some(b) = finally_block {
                for st in &b.stmts {
                    visit_stmt(st, breaks, continues);
                }
            }
        }
        StmtKind::Fn(fd) => {
            for st in &fd.body.stmts {
                visit_stmt(st, breaks, continues);
            }
        }
        StmtKind::Class(cd) => {
            for st in &cd.body.stmts {
                visit_stmt(st, breaks, continues);
            }
        }
        StmtKind::Decorated { inner, .. } => {
            visit_stmt(inner, breaks, continues);
        }
        _ => {}
    }
}

fn count_in(m: &Module) -> (usize, usize) {
    let mut breaks = 0;
    let mut continues = 0;
    for s in &m.items {
        visit_stmt(s, &mut breaks, &mut continues);
    }
    (breaks, continues)
}

fn count_break_continue(src: &str) -> (usize, usize) {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    count_in(&module)
}

fn assert_parses(name: &str, src: &str) {
    parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("{name}: must parse but failed: {e:?}\n--- source ---\n{src}"));
}

fn assert_rejects(name: &str, src: &str) {
    match parse_str(src, FileId::SYNTHETIC) {
        Ok(_) => panic!("{name}: must reject but parsed\n--- source ---\n{src}"),
        Err(_) => {}
    }
}

fn round_trip(name: &str, src: &str) {
    let m1 = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("{name}: parse #1 failed: {e:?}\n{src}"));
    let up = unparse(&m1);
    let m2 = parse_str(&up, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("{name}: re-parse of unparsed source failed: {e:?}\n{up}"));
    let (b1, c1) = count_in(&m1);
    let (b2, c2) = count_in(&m2);
    assert_eq!(
        (b1, c1),
        (b2, c2),
        "{name}: break/continue counts diverged across round-trip:\n  before: break={b1} continue={c1}\n  after:  break={b2} continue={c2}\n  src:\n{src}\n  unparsed:\n{up}"
    );
}

// =====================================================================
// Section A — happy-path parse + AST shape (≥10 cases)
// =====================================================================

#[test]
fn p01_bare_break_inside_while() {
    let src = "fn main() -> i64:\n    while True:\n        break\n    return 0\n";
    assert_parses("p01", src);
    assert_eq!(count_break_continue(src), (1, 0));
}

#[test]
fn p02_bare_continue_inside_while() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 5:\n        i = i + 1\n        continue\n    return 0\n";
    assert_parses("p02", src);
    assert_eq!(count_break_continue(src), (0, 1));
}

#[test]
fn p03_break_inside_if_inside_while() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        if i == 3:\n            break\n        i = i + 1\n    return 0\n";
    assert_parses("p03", src);
    assert_eq!(count_break_continue(src), (1, 0));
}

#[test]
fn p04_continue_inside_if_else_inside_while() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n        if i == 5:\n            continue\n        else:\n            print(i)\n    return 0\n";
    assert_parses("p04", src);
    assert_eq!(count_break_continue(src), (0, 1));
}

#[test]
fn p05_break_and_continue_in_same_loop() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 100:\n        i = i + 1\n        if i == 7:\n            continue\n        if i == 12:\n            break\n    return 0\n";
    assert_parses("p05", src);
    assert_eq!(count_break_continue(src), (1, 1));
}

#[test]
fn p06_break_inside_inner_of_nested_while() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        let j: i64 = 0\n        while j < 3:\n            if j == 1:\n                break\n            j = j + 1\n        i = i + 1\n    return 0\n";
    assert_parses("p06", src);
    assert_eq!(count_break_continue(src), (1, 0));
}

#[test]
fn p07_continue_inside_inner_of_nested_while() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        let j: i64 = 0\n        while j < 3:\n            j = j + 1\n            if j == 2:\n                continue\n        i = i + 1\n    return 0\n";
    assert_parses("p07", src);
    assert_eq!(count_break_continue(src), (0, 1));
}

#[test]
fn p08_break_at_top_of_loop_body() {
    let src = "fn main() -> i64:\n    while True:\n        break\n    return 0\n";
    assert_parses("p08", src);
    assert_eq!(count_break_continue(src), (1, 0));
}

#[test]
fn p09_continue_at_top_of_loop_body() {
    // Note: at runtime this is an infinite loop because `continue` skips
    // any body that would mutate the condition. But the parser accepts
    // it — the type checker doesn't reach a halting analysis here.
    let src =
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 5:\n        continue\n    return 0\n";
    assert_parses("p09", src);
    assert_eq!(count_break_continue(src), (0, 1));
}

#[test]
fn p10_break_with_post_loop_statements() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        if i == 5:\n            break\n        i = i + 1\n    print(i)\n    return 0\n";
    assert_parses("p10", src);
    assert_eq!(count_break_continue(src), (1, 0));
}

#[test]
fn p11_break_inside_elif_chain() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 20:\n        i = i + 1\n        if i == 3:\n            print(i)\n        elif i == 7:\n            break\n        elif i == 11:\n            print(i)\n        else:\n            pass\n    return 0\n";
    assert_parses("p11", src);
    assert_eq!(count_break_continue(src), (1, 0));
}

#[test]
fn p12_continue_inside_elif_chain() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 20:\n        i = i + 1\n        if i == 3:\n            continue\n        elif i == 7:\n            print(i)\n        else:\n            pass\n    return 0\n";
    assert_parses("p12", src);
    assert_eq!(count_break_continue(src), (0, 1));
}

#[test]
fn p13_break_after_assignment() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n        if i == 4:\n            i = 99\n            break\n    return 0\n";
    assert_parses("p13", src);
    assert_eq!(count_break_continue(src), (1, 0));
}

#[test]
fn p14_continue_after_assignment() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n        if i == 4:\n            i = i + 2\n            continue\n    return 0\n";
    assert_parses("p14", src);
    assert_eq!(count_break_continue(src), (0, 1));
}

#[test]
fn p15_break_in_three_level_nested_loop() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        let j: i64 = 0\n        while j < 3:\n            let k: i64 = 0\n            while k < 3:\n                if k == 1:\n                    break\n                k = k + 1\n            j = j + 1\n        i = i + 1\n    return 0\n";
    assert_parses("p15", src);
    assert_eq!(count_break_continue(src), (1, 0));
}

#[test]
fn p16_multiple_breaks_in_one_loop() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 100:\n        i = i + 1\n        if i == 5:\n            break\n        if i == 7:\n            break\n        if i == 9:\n            break\n    return 0\n";
    assert_parses("p16", src);
    assert_eq!(count_break_continue(src), (3, 0));
}

#[test]
fn p17_multiple_continues_in_one_loop() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 100:\n        i = i + 1\n        if i == 3:\n            continue\n        if i == 5:\n            continue\n        if i == 7:\n            continue\n    return 0\n";
    assert_parses("p17", src);
    assert_eq!(count_break_continue(src), (0, 3));
}

#[test]
fn p18_break_with_while_else_clause() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 5:\n        if i == 3:\n            break\n        i = i + 1\n    else:\n        print(99)\n    return 0\n";
    assert_parses("p18", src);
    assert_eq!(count_break_continue(src), (1, 0));
}

#[test]
fn p19_continue_with_while_else_clause() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        i = i + 1\n        if i == 2:\n            continue\n    else:\n        print(99)\n    return 0\n";
    assert_parses("p19", src);
    assert_eq!(count_break_continue(src), (0, 1));
}

#[test]
fn p20_break_then_unreachable_stmt() {
    // After `break`, anything else in the body is unreachable but the
    // parser doesn't enforce that — it's a type-checker / MIR concern.
    // The parser must accept the shape.
    let src = "fn main() -> i64:\n    while True:\n        break\n        print(0)\n    return 0\n";
    assert_parses("p20", src);
    assert_eq!(count_break_continue(src), (1, 0));
}

// =====================================================================
// Section B — round-trip shape preservation (≥5 cases)
// =====================================================================

#[test]
fn r01_round_trip_simple_break() {
    round_trip(
        "r01",
        "fn main() -> i64:\n    while True:\n        break\n    return 0\n",
    );
}

#[test]
fn r02_round_trip_simple_continue() {
    round_trip(
        "r02",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        i = i + 1\n        continue\n    return 0\n",
    );
}

#[test]
fn r03_round_trip_nested_loops() {
    round_trip(
        "r03",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        let j: i64 = 0\n        while j < 3:\n            if j == 1:\n                break\n            j = j + 1\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn r04_round_trip_mixed_break_continue() {
    round_trip(
        "r04",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 100:\n        i = i + 1\n        if i == 7:\n            continue\n        if i == 13:\n            break\n    return 0\n",
    );
}

#[test]
fn r05_round_trip_break_in_elif() {
    round_trip(
        "r05",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n        if i == 3:\n            print(i)\n        elif i == 5:\n            break\n        else:\n            pass\n    return 0\n",
    );
}

// =====================================================================
// Section C — parser rejects malformed shapes (≥10 cases)
// =====================================================================

#[test]
fn x01_break_with_label() {
    // Cobrust drops Python `break <label>` per constitution §2.2.
    let src = "fn main() -> i64:\n    while True:\n        break outer\n    return 0\n";
    assert_rejects("x01", src);
}

#[test]
fn x02_continue_with_label() {
    let src = "fn main() -> i64:\n    while True:\n        continue outer\n    return 0\n";
    assert_rejects("x02", src);
}

#[test]
fn x03_break_with_int_payload() {
    let src = "fn main() -> i64:\n    while True:\n        break 0\n    return 0\n";
    assert_rejects("x03", src);
}

#[test]
fn x04_continue_with_int_payload() {
    let src = "fn main() -> i64:\n    while True:\n        continue 0\n    return 0\n";
    assert_rejects("x04", src);
}

#[test]
fn x05_break_with_paren_expr() {
    let src = "fn main() -> i64:\n    while True:\n        break()\n    return 0\n";
    assert_rejects("x05", src);
}

#[test]
fn x06_break_as_call_target() {
    // break() reads `break` as a callable identifier — not valid.
    let src = "fn main() -> i64:\n    let x: i64 = break()\n    return 0\n";
    assert_rejects("x06", src);
}

#[test]
fn x07_continue_as_call_target() {
    let src = "fn main() -> i64:\n    let x: i64 = continue()\n    return 0\n";
    assert_rejects("x07", src);
}

#[test]
fn x08_break_as_identifier() {
    // `break` is reserved — cannot be used as an identifier.
    let src = "fn main() -> i64:\n    let break: i64 = 0\n    return 0\n";
    assert_rejects("x08", src);
}

#[test]
fn x09_continue_as_identifier() {
    let src = "fn main() -> i64:\n    let continue: i64 = 0\n    return 0\n";
    assert_rejects("x09", src);
}

#[test]
fn x10_break_in_expression_position() {
    let src = "fn main() -> i64:\n    while True:\n        let x: i64 = 1 + break\n    return 0\n";
    assert_rejects("x10", src);
}

#[test]
fn x11_continue_in_expression_position() {
    let src =
        "fn main() -> i64:\n    while True:\n        let x: i64 = 1 + continue\n    return 0\n";
    assert_rejects("x11", src);
}

#[test]
fn x12_break_with_string_payload() {
    let src = "fn main() -> i64:\n    while True:\n        break \"label\"\n    return 0\n";
    assert_rejects("x12", src);
}

#[test]
fn x13_continue_with_string_payload() {
    let src = "fn main() -> i64:\n    while True:\n        continue \"label\"\n    return 0\n";
    assert_rejects("x13", src);
}
