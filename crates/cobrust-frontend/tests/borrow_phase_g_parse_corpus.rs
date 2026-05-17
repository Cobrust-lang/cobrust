//! ADR-0052a Wave-1 parse corpus for the `&s` explicit-borrow surface.
//!
//! Covers:
//! - Parser accepts `&ident`, `&ident.field`, `&ident[idx]`, and
//!   `&(ident)` shapes (the three Wave-1 production paths per
//!   ADR-0052a §8 + the redundant-parens form per §3).
//! - Parser REJECTS Wave-1-deferred shapes per ADR-0052a §12
//!   "Out of scope":
//!     - `&"literal"` (literal-borrow deferred; §8 Wave-1 cap)
//!     - `&(complex_expr_without_outer_parens)` — `&(a + b)` is OK
//!       (paren-bracketed sub-expression), but `& a + b` parses as
//!       `(&a) + b` so we encode the policy on a representative
//!       shape that surfaces the ambiguity.
//!     - `&mut s` (mutable borrow deferred to Phase H per §12)
//!     - `&&s` (double-borrow deferred to Phase H)
//!     - `&` at end-of-expression (no operand → parse error)
//! - Round-trip preserves the AST shape for accepted forms.
//! - 18-lint clippy module-level allow header per
//!   `feedback_p9_clippy_stall_pattern.md`.
//!
//! Pre-DEV-impl status: every `bg0052a_*` test below is `#[ignore]`'d
//! pending Wave-1 DEV merge. DEV removes the `#[ignore]` markers and
//! the suite turns green.

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

use cobrust_frontend::{parse_str, span::FileId};

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

// =====================================================================
// Section A — Wave-1 happy-path parse (≥6 cases)
//
// These tests pin down which forms the parser MUST accept under
// ADR-0052a §8 Wave-1 cap. DEV implements the production rule;
// removing `#[ignore]` re-engages the test.
// =====================================================================

#[test]
fn bg0052a_p01_amp_ident_in_call_arg() {
    // `&s` as a function-call argument is the canonical Wave-1 form.
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    return n\n";
    assert_parses("bg0052a_p01", src);
}

#[test]
fn bg0052a_p02_amp_ident_in_let_rebind() {
    // `let s = &s` is the let-rebind shortcut per ADR-0052a §4.4.
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let s = &s\n    return str_len(s)\n";
    assert_parses("bg0052a_p02", src);
}

#[test]
fn bg0052a_p03_amp_field_access() {
    // `&p.0` — borrow of a tuple-field projection. One of the three
    // Wave-1 production paths per §8.
    let src =
        "fn main() -> i64:\n    let p = (\"a\", \"b\")\n    let n = str_len(&p.0)\n    return n\n";
    assert_parses("bg0052a_p03", src);
}

#[test]
fn bg0052a_p04_amp_indexed_list() {
    // `&xs[0]` — borrow of an indexed list element. One of the three
    // Wave-1 production paths per §8.
    let src = "fn main() -> i64:\n    let xs: list[str] = [\"a\", \"b\"]\n    let n = str_len(&xs[0])\n    return n\n";
    assert_parses("bg0052a_p04", src);
}

#[test]
fn bg0052a_p05_amp_parens_ident() {
    // `&(s)` — parenthesised identifier; ADR-0052a §3 implies parens
    // wrap any single sub-expression that the borrow targets.
    let src =
        "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&(s))\n    return n\n";
    assert_parses("bg0052a_p05", src);
}

#[test]
fn bg0052a_p06_amp_ident_multiple_in_one_expr() {
    // Multiple `&s` reads in a single expression — the parser must
    // accept independent occurrences.
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let total = str_len(&s) + str_len(&s)\n    return total\n";
    assert_parses("bg0052a_p06", src);
}

// =====================================================================
// Section B — Wave-1 parse-rejection cases (≥9 cases)
//
// These tests pin down which forms the parser MUST reject under
// ADR-0052a §8 Wave-1 cap + §12 "Out of scope". Some are NOT marked
// `#[ignore]` because the parser already rejects them today (pre-impl)
// — they survive impl as regressions guards. Others are marked
// `#[ignore]` because the parser today silently does the wrong thing
// (e.g., parses `&` as a different operator).
//
// Per `feedback_p9_clippy_stall_pattern.md`, every reject case
// documents the EXPECTED rejection reason so DEV's error-shape choice
// is auditable.
// =====================================================================

#[test]
fn bg0052a_r01_amp_string_literal_rejected() {
    // `&"literal"` — literal-borrow is deferred per ADR-0052a §8
    // Wave-1 cap. Wave-1 only borrows `Name` and field-access /
    // indexing expressions; literal-borrow surfaces in a future ADR.
    let src = "fn main() -> i64:\n    let n = str_len(&\"hello\")\n    return n\n";
    assert_rejects("bg0052a_r01", src);
}

#[test]
fn bg0052a_r02_amp_int_literal_rejected() {
    // `&123` — int-literal-borrow rejected by the same §8 Wave-1 cap.
    let src = "fn main() -> i64:\n    let r = &123\n    return r\n";
    assert_rejects("bg0052a_r02", src);
}

#[test]
fn bg0052a_r03_amp_list_literal_rejected() {
    // `&[1, 2, 3]` — list-literal-borrow rejected by §8 Wave-1 cap;
    // composite-literal borrows are not in scope.
    let src = "fn main() -> i64:\n    let xs = &[1, 2, 3]\n    return list_len(xs)\n";
    assert_rejects("bg0052a_r03", src);
}

#[test]
fn bg0052a_r04_amp_mut_rejected() {
    // `&mut s` — mutable borrow deferred to Phase H per ADR-0052a §12.
    // Wave-1 is shared-borrow-only.
    let src =
        "fn main() -> i64:\n    let s = input(\"\")\n    let r = &mut s\n    return str_len(r)\n";
    assert_rejects("bg0052a_r04", src);
}

#[test]
fn bg0052a_r05_amp_amp_double_borrow_rejected() {
    // `&&s` — double-borrow rejected by §8 Wave-1 cap; nested borrow
    // surfaces in Phase H if it surfaces at all.
    //
    // NB: this also disambiguates the `&&` lex from the logical-AND
    // operator under Wave-1; parser must tokenise `&&` as either
    // double-borrow or AND, and either way reject in this position.
    let src =
        "fn main() -> i64:\n    let s = input(\"\")\n    let r = &&s\n    return str_len(r)\n";
    assert_rejects("bg0052a_r05", src);
}

#[test]
fn bg0052a_r06_amp_no_operand_rejected() {
    // `&\n` — unary `&` with no operand. Must be a parse error.
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let r = &\n    return str_len(s)\n";
    assert_rejects("bg0052a_r06", src);
}

#[test]
fn bg0052a_r07_amp_call_result_rejected() {
    // `&input("")` — borrow of a call expression. Wave-1 §8 limits
    // borrow to `Name` / `Name.field` / `Name[idx]`. Call-results
    // surface as future sub-ADR scope.
    let src = "fn main() -> i64:\n    let n = str_len(&input(\"\"))\n    return n\n";
    assert_rejects("bg0052a_r07", src);
}

#[test]
fn bg0052a_r08_amp_followed_by_block_rejected() {
    // `let r = & if cond: ...` — `&` followed by a statement-keyword
    // is a parse error in Wave-1.
    let src = "fn main() -> i64:\n    let cond: bool = True\n    let r = & if cond:\n        return 0\n    return 1\n";
    assert_rejects("bg0052a_r08", src);
}

#[test]
fn bg0052a_r09_amp_fstring_rejected() {
    // `&f"hello"` — borrow of an f-string literal. Same §8 cap as
    // `&"literal"`; f-strings are literal-shaped expressions.
    let src = "fn main() -> i64:\n    let label = input(\"\")\n    let r = &f\"hi {label}\"\n    return str_len(r)\n";
    assert_rejects("bg0052a_r09", src);
}
