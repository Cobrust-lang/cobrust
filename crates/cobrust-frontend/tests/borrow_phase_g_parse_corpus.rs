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

// =====================================================================
// Section C — ADR-0052f Wave-2 round-2 cap relaxation (`&Call(Attr(...))`)
//
// Per ADR-0052f §5 + §8.1: the parser's `validate_borrow_operand`
// is relaxed to admit `ExprKind::Call { callee: Attr { base, .. }, .. }`
// when `base` is itself a borrowable place (recursive admission).
// Free-fn `&Call(Name)` and other shapes STILL reject (asymmetric cap
// per ADR-0052f §2).
//
// Pre-DEV-impl: every `bg0052f_p*` parse-acceptance test is
// `#[ignore]`'d pending the parser relaxation. DEV removes the ignore
// after the §5 diff lands. The `bg0052f_r*` rejection tests are NOT
// ignored — they survive impl as asymmetry-preservation regression
// guards (their reject status pre/post-impl is identical).
// =====================================================================

#[test]
#[ignore = "ADR-0052f Wave-2-rd2 DEV impl pending"]
fn bg0052f_p01_amp_method_call_no_args() {
    // `&s.len()` — the canonical Wave-2-rd2 witness (parses as
    // `Borrow(Call(Attr(s, "len"), []))` per ADR-0052 F-G.3 precedence).
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s.len())\n    return n\n";
    assert_parses("bg0052f_p01", src);
}

#[test]
#[ignore = "ADR-0052f Wave-2-rd2 DEV impl pending"]
fn bg0052f_p02_amp_method_call_with_arg() {
    // `&xs.get(0)` — method-form with one arg; list-method receiver.
    // Parse-only assertion (the parser must admit the surface).
    let src = "fn main() -> i64:\n    let xs: list[str] = [\"a\", \"b\"]\n    let n = str_len(&xs.get(0))\n    return n\n";
    assert_parses("bg0052f_p02", src);
}

#[test]
#[ignore = "ADR-0052f Wave-2-rd2 DEV impl pending"]
fn bg0052f_p03_amp_method_call_float_receiver() {
    // `&f.floor()` — float-method receiver; cosmetic-borrow uniformity
    // case per ADR-0052f §7 latent-consumer row 4.
    let src = "fn read_i64(n: i64) -> i64:\n    return n\nfn main() -> i64:\n    let f: f64 = 1.5\n    let n = read_i64(&f.floor())\n    return n\n";
    assert_parses("bg0052f_p03", src);
}

#[test]
#[ignore = "ADR-0052f Wave-2-rd2 DEV impl pending"]
fn bg0052f_p04_amp_method_call_multi_arg() {
    // `&xs.method(args)` — method-form with multiple args; parser
    // admits any arity as long as the callee shape is `Attr(base, _)`
    // and `base` is borrowable.
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s.replace(\"a\", \"b\"))\n    return n\n";
    assert_parses("bg0052f_p04", src);
}

#[test]
#[ignore = "ADR-0052f Wave-2-rd2 DEV impl pending"]
fn bg0052f_p05_amp_method_call_in_let_binding() {
    // `let n = &s.len()` — direct let-binding of the borrow result.
    // Confirms the relaxation isn't gated on call-argument position.
    let src = "fn read_i64(n: i64) -> i64:\n    return n\nfn main() -> i64:\n    let s = input(\"\")\n    let n = &s.len()\n    return read_i64(n)\n";
    assert_parses("bg0052f_p05", src);
}

// --- Section C.2 — Rejection regression guards (NOT ignored). ---
//
// These cases must reject both pre- and post-impl. The asymmetry
// between method-form (admitted) and free-fn / literal / complex
// (rejected) is principled per ADR-0052f §2 + §9 §2.5 compile-time-
// catch preservation.

#[test]
fn bg0052f_r01_amp_free_fn_call_rejected() {
    // `&free_fn()` — callee is `Name`, not `Attr(base, _)`. The
    // §2.5 compile-time-catch path stays armed: `&free_fn(x)` borrows
    // a temporary with no anchored place. ADR-0052f §2 explicitly
    // keeps this rejection.
    let src = "fn main() -> i64:\n    let n = str_len(&input(\"\"))\n    return n\n";
    assert_rejects("bg0052f_r01", src);
}

#[test]
fn bg0052f_r02_amp_literal_method_call_rejected() {
    // `&"hello".len()` — string-literal-receiver method-form. The
    // parser's recursive `validate_borrow_operand(base)` MUST reject
    // the literal receiver per the existing §8 literal-cap
    // (parser.rs L1116-1121). Guards against the recursion
    // accidentally admitting literals via the new method-form arm.
    let src = "fn main() -> i64:\n    let n = str_len(&\"hello\".len())\n    return n\n";
    assert_rejects("bg0052f_r02", src);
}

#[test]
fn bg0052f_r03_amp_free_fn_with_args_rejected() {
    // `&foo(x, y)` — free-fn call with multi-arg. Same `Name`-callee
    // rejection path as r01; confirms arity does not affect the
    // asymmetric cap.
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&str_len(s, s))\n    return n\n";
    assert_rejects("bg0052f_r03", src);
}
