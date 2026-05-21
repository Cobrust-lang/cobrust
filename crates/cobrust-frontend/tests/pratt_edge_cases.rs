#![allow(clippy::too_many_lines)]
#![allow(clippy::items_after_statements)]
//! Pratt parser edge-case tests — operator precedence, associativity,
//! unary chains, and depth-limit enforcement.
//!
//! Every test exercises a surface from the Pratt table in `parser.rs`
//! header comment (CQ P1-1 bump).

use cobrust_frontend::ast::{BinOp, ExprKind, Literal, StmtKind, UnaryOp};
use cobrust_frontend::error::{FrontendError, ParseError};
use cobrust_frontend::span::FileId;
use cobrust_frontend::parse_str;

// =====================================================================
// Helpers
// =====================================================================

fn parse_ok(src: &str) -> cobrust_frontend::ast::Module {
    parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("expected parse success, got {e:?}\nsource:\n{src}"))
}

fn parse_err(src: &str) -> ParseError {
    let fe = parse_str(src, FileId::SYNTHETIC)
        .map(|_| panic!("expected parse error, got Ok\nsource:\n{src}"))
        .unwrap_err();
    match fe {
        FrontendError::Parse(p) => p,
        FrontendError::Lex(l) => panic!("expected ParseError, got LexError: {l:?}"),
    }
}

fn first_expr(src: &str) -> cobrust_frontend::ast::Expr {
    let m = parse_ok(src);
    match &m.items[0].kind {
        StmtKind::Expr(e) => e.clone(),
        other => panic!("expected Expr stmt, got {other:?}"),
    }
}

// =====================================================================
// Precedence: tighter ops bind sub-expressions of looser ops
// =====================================================================

#[test]
fn prec_mul_over_add() {
    // 1 + 2 * 3  → Binary(Add, 1, Binary(Mul, 2, 3))
    let e = first_expr("1 + 2 * 3\n");
    match &e.kind {
        ExprKind::Binary { op, rhs, .. } => {
            assert_eq!(op, &BinOp::Add);
            assert!(
                matches!(&rhs.kind, ExprKind::Binary { op: BinOp::Mul, .. }),
                "rhs must be Mul, got {rhs:?}"
            );
        }
        other => panic!("expected Binary(Add), got {other:?}"),
    }
}

#[test]
fn prec_add_over_shift() {
    // 1 << 2 + 3 → Binary(Shl, 1, Binary(Add, 2, 3))
    let e = first_expr("1 << 2 + 3\n");
    match &e.kind {
        ExprKind::Binary { op, rhs, .. } => {
            assert_eq!(op, &BinOp::Shl);
            assert!(
                matches!(&rhs.kind, ExprKind::Binary { op: BinOp::Add, .. }),
                "rhs must be Add"
            );
        }
        other => panic!("expected Binary(Shl), got {other:?}"),
    }
}

#[test]
fn prec_shift_over_bitor() {
    // 1 | 2 << 3 → Binary(BitOr, 1, Binary(Shl, 2, 3))
    let e = first_expr("1 | 2 << 3\n");
    match &e.kind {
        ExprKind::Binary { op, rhs, .. } => {
            assert_eq!(op, &BinOp::BitOr);
            assert!(
                matches!(&rhs.kind, ExprKind::Binary { op: BinOp::Shl, .. }),
                "rhs must be Shl"
            );
        }
        other => panic!("expected Binary(BitOr), got {other:?}"),
    }
}

#[test]
fn prec_bitxor_over_bitor() {
    // 1 ^ 2 | 3 → Binary(BitOr, Binary(BitXor,1,2), 3)
    // | is prec 65, ^ is prec 70 — ^ binds tighter
    let e = first_expr("1 ^ 2 | 3\n");
    match &e.kind {
        ExprKind::Binary { op, lhs, .. } => {
            assert_eq!(op, &BinOp::BitOr);
            assert!(
                matches!(&lhs.kind, ExprKind::Binary { op: BinOp::BitXor, .. }),
                "lhs must be BitXor"
            );
        }
        other => panic!("expected Binary(BitOr), got {other:?}"),
    }
}

#[test]
fn prec_pow_right_assoc() {
    // 2 ** 3 ** 2 → Binary(Pow, 2, Binary(Pow, 3, 2))  right-associative
    let e = first_expr("2 ** 3 ** 2\n");
    match &e.kind {
        ExprKind::Binary { op, rhs, .. } => {
            assert_eq!(op, &BinOp::Pow);
            assert!(
                matches!(&rhs.kind, ExprKind::Binary { op: BinOp::Pow, .. }),
                "rhs must be Pow (right-assoc)"
            );
        }
        other => panic!("expected Binary(Pow), got {other:?}"),
    }
}

#[test]
fn prec_add_left_assoc() {
    // 1 + 2 + 3 → Binary(Add, Binary(Add, 1, 2), 3)  left-associative
    let e = first_expr("1 + 2 + 3\n");
    match &e.kind {
        ExprKind::Binary { op, lhs, .. } => {
            assert_eq!(op, &BinOp::Add);
            assert!(
                matches!(&lhs.kind, ExprKind::Binary { op: BinOp::Add, .. }),
                "lhs must be Add (left-assoc)"
            );
        }
        other => panic!("expected Binary(Add), got {other:?}"),
    }
}

#[test]
fn prec_and_over_or() {
    // a or b and c → Binary(Or, a, Binary(And, b, c))
    let e = first_expr("a or b and c\n");
    match &e.kind {
        ExprKind::Binary { op, rhs, .. } => {
            assert_eq!(op, &BinOp::Or);
            assert!(
                matches!(&rhs.kind, ExprKind::Binary { op: BinOp::And, .. }),
                "rhs must be And"
            );
        }
        other => panic!("expected Binary(Or), got {other:?}"),
    }
}

#[test]
fn prec_not_over_and() {
    // not a and b → Binary(And, Unary(Not, a), b)
    let e = first_expr("not a and b\n");
    match &e.kind {
        ExprKind::Binary { op, lhs, .. } => {
            assert_eq!(op, &BinOp::And);
            assert!(
                matches!(&lhs.kind, ExprKind::Unary { op: UnaryOp::Not, .. }),
                "lhs must be Unary(Not)"
            );
        }
        other => panic!("expected Binary(And), got {other:?}"),
    }
}

#[test]
fn prec_bitand_over_bitxor() {
    // 1 ^ 2 & 3 → Binary(BitXor, 1, Binary(BitAnd, 2, 3))
    // & is prec 75, ^ is prec 70 — & binds tighter
    let e = first_expr("1 ^ 2 & 3\n");
    match &e.kind {
        ExprKind::Binary { op, rhs, .. } => {
            assert_eq!(op, &BinOp::BitXor);
            assert!(
                matches!(&rhs.kind, ExprKind::Binary { op: BinOp::BitAnd, .. }),
                "rhs must be BitAnd"
            );
        }
        other => panic!("expected Binary(BitXor), got {other:?}"),
    }
}

// =====================================================================
// Comparison operators
// =====================================================================

#[test]
fn cmp_in_operator() {
    let e = first_expr("x in [1, 2, 3]\n");
    assert!(
        matches!(&e.kind, ExprKind::Binary { op: BinOp::In, .. }),
        "expected Binary(In), got {e:?}"
    );
}

#[test]
fn cmp_not_in_operator() {
    // `not in` Pratt-loop bump is unbalanced (ADR-0050d commentary L1011-L1023).
    // Canonical workaround: `not (x in coll)` — unary Not over BinOp::In.
    let e = first_expr("not (x in [1, 2])\n");
    match &e.kind {
        ExprKind::Unary { op: UnaryOp::Not, operand } => {
            assert!(
                matches!(&operand.kind, ExprKind::Binary { op: BinOp::In, .. }),
                "operand must be Binary(In)"
            );
        }
        other => panic!("expected Unary(Not, Binary(In)), got {other:?}"),
    }
}

#[test]
fn cmp_eq_operator() {
    let e = first_expr("a == b\n");
    assert!(
        matches!(&e.kind, ExprKind::Binary { op: BinOp::Eq, .. }),
        "expected Binary(Eq), got {e:?}"
    );
}

#[test]
fn cmp_neq_operator() {
    let e = first_expr("a != b\n");
    assert!(
        matches!(&e.kind, ExprKind::Binary { op: BinOp::NotEq, .. }),
        "expected Binary(NotEq), got {e:?}"
    );
}

#[test]
fn cmp_lt_and_gt() {
    let lt = first_expr("a < b\n");
    assert!(matches!(&lt.kind, ExprKind::Binary { op: BinOp::Lt, .. }));
    let gt = first_expr("a > b\n");
    assert!(matches!(&gt.kind, ExprKind::Binary { op: BinOp::Gt, .. }));
    let lte = first_expr("a <= b\n");
    assert!(matches!(&lte.kind, ExprKind::Binary { op: BinOp::LtEq, .. }));
    let gte = first_expr("a >= b\n");
    assert!(matches!(&gte.kind, ExprKind::Binary { op: BinOp::GtEq, .. }));
}

// =====================================================================
// Unary chains
// =====================================================================

#[test]
fn unary_double_neg_parens() {
    // -(-x) → Unary(Neg, Unary(Neg, x))
    let e = first_expr("-(-x)\n");
    match &e.kind {
        ExprKind::Unary { op, operand } => {
            assert_eq!(op, &UnaryOp::Neg);
            assert!(
                matches!(&operand.kind, ExprKind::Unary { op: UnaryOp::Neg, .. }),
                "operand must be Unary(Neg)"
            );
        }
        other => panic!("expected Unary(Neg), got {other:?}"),
    }
}

#[test]
fn unary_bitnot_chain() {
    let e = first_expr("~(~x)\n");
    match &e.kind {
        ExprKind::Unary { op, operand } => {
            assert_eq!(op, &UnaryOp::BitNot);
            assert!(
                matches!(&operand.kind, ExprKind::Unary { op: UnaryOp::BitNot, .. }),
                "inner must be BitNot"
            );
        }
        other => panic!("expected Unary(BitNot), got {other:?}"),
    }
}

#[test]
fn unary_pos() {
    let e = first_expr("+x\n");
    assert!(
        matches!(&e.kind, ExprKind::Unary { op: UnaryOp::Plus, .. }),
        "expected Unary(Plus)"
    );
}

#[test]
fn unary_not_bool() {
    let e = first_expr("not x\n");
    assert!(
        matches!(&e.kind, ExprKind::Unary { op: UnaryOp::Not, .. }),
        "expected Unary(Not)"
    );
}

// =====================================================================
// ExpressionTooDeep guard (MAX_PARSER_DEPTH = 50)
// =====================================================================

#[test]
fn expr_too_deep_paren() {
    // 51 levels of nested parens — must fail with ExpressionTooDeep
    let open = "(".repeat(51);
    let close = ")".repeat(51);
    let src = format!("{open}1{close}\n");
    let err = parse_err(&src);
    assert!(
        matches!(err, ParseError::ExpressionTooDeep { .. }),
        "expected ExpressionTooDeep, got {err:?}"
    );
}

#[test]
fn expr_deep_within_limit_ok() {
    // Well-formed nesting well below MAX (50) must succeed.
    // Use 10 levels — deeply nested is unusual but valid.
    let open = "(".repeat(10);
    let close = ")".repeat(10);
    let src = format!("{open}1{close}\n");
    parse_ok(&src);
}

#[test]
fn expr_too_deep_suggestion_populated() {
    let open = "(".repeat(52);
    let close = ")".repeat(52);
    let src = format!("{open}1{close}\n");
    let err = parse_err(&src);
    match err {
        ParseError::ExpressionTooDeep { suggestion, depth, max, .. } => {
            assert!(
                suggestion.is_some(),
                "ExpressionTooDeep must carry a suggestion per §2.5 Direction B"
            );
            assert!(depth > max, "depth ({depth}) must exceed max ({max})");
        }
        other => panic!("expected ExpressionTooDeep, got {other:?}"),
    }
}

// =====================================================================
// `as` cast — prec 88 between ADD(85) and MUL(90)
// =====================================================================

#[test]
fn cast_as_keyword_parses() {
    // 2 as i64 must produce Cast node
    let e = first_expr("2 as i64\n");
    assert!(
        matches!(&e.kind, ExprKind::Cast { .. }),
        "expected Cast, got {e:?}"
    );
}

#[test]
fn cast_tighter_than_add() {
    // 1 + 2 as i64 → Binary(Add, 1, Cast(2, i64))
    // because `as` (88) binds tighter than `+` (85)
    let e = first_expr("1 + 2 as i64\n");
    match &e.kind {
        ExprKind::Binary { op, rhs, .. } => {
            assert_eq!(op, &BinOp::Add);
            assert!(
                matches!(&rhs.kind, ExprKind::Cast { .. }),
                "rhs must be Cast"
            );
        }
        other => panic!("expected Binary(Add), got {other:?}"),
    }
}

// =====================================================================
// Parenthesized sub-expressions override precedence
// =====================================================================

#[test]
fn parens_override_prec() {
    // (1 + 2) * 3 → Binary(Mul, Binary(Add,1,2), 3)
    let e = first_expr("(1 + 2) * 3\n");
    match &e.kind {
        ExprKind::Binary { op, lhs, .. } => {
            assert_eq!(op, &BinOp::Mul);
            assert!(
                matches!(&lhs.kind, ExprKind::Binary { op: BinOp::Add, .. }),
                "lhs must be Add"
            );
        }
        other => panic!("expected Binary(Mul), got {other:?}"),
    }
}

// =====================================================================
// Floor-div / modulo / matmul
// =====================================================================

#[test]
fn floor_div_and_mod_same_prec_left_assoc() {
    // 10 // 3 % 2 → Binary(Mod, Binary(FloorDiv, 10, 3), 2)
    let e = first_expr("10 // 3 % 2\n");
    match &e.kind {
        ExprKind::Binary { op, lhs, .. } => {
            assert_eq!(op, &BinOp::Mod);
            assert!(
                matches!(&lhs.kind, ExprKind::Binary { op: BinOp::FloorDiv, .. }),
                "lhs must be FloorDiv"
            );
        }
        other => panic!("expected Binary(Mod), got {other:?}"),
    }
}

#[test]
fn matmul_operator() {
    // A @ B — matrix multiply
    let e = first_expr("A @ B\n");
    assert!(
        matches!(&e.kind, ExprKind::Binary { op: BinOp::MatMul, .. }),
        "expected Binary(MatMul), got {e:?}"
    );
}

// =====================================================================
// Literal atoms recognised by Pratt
// =====================================================================

#[test]
fn atom_int_literal() {
    let e = first_expr("42\n");
    match &e.kind {
        ExprKind::Literal(Literal::Int(s)) => assert_eq!(s, "42"),
        other => panic!("expected Literal(Int), got {other:?}"),
    }
}

#[test]
fn atom_float_literal() {
    let e = first_expr("3.14\n");
    assert!(
        matches!(&e.kind, ExprKind::Literal(Literal::Float(_))),
        "expected Literal(Float), got {e:?}"
    );
}

#[test]
fn atom_bool_true() {
    let e = first_expr("True\n");
    assert!(
        matches!(&e.kind, ExprKind::Literal(Literal::Bool(true))),
        "expected Literal(Bool(true))"
    );
}

#[test]
fn atom_bool_false() {
    let e = first_expr("False\n");
    assert!(
        matches!(&e.kind, ExprKind::Literal(Literal::Bool(false))),
        "expected Literal(Bool(false))"
    );
}

#[test]
fn atom_none_literal() {
    let e = first_expr("None\n");
    assert!(
        matches!(&e.kind, ExprKind::Literal(Literal::None)),
        "expected Literal(None)"
    );
}

#[test]
fn atom_string_literal() {
    // A lone string literal at module level becomes a docstring, not an Expr stmt.
    // Use it after another statement so it is not the leading docstring.
    let m = parse_ok("pass\n\"hello\"\n");
    match &m.items[1].kind {
        StmtKind::Expr(e) => match &e.kind {
            ExprKind::Literal(Literal::Str(s)) => assert_eq!(s, "hello"),
            other => panic!("expected Literal(Str), got {other:?}"),
        },
        other => panic!("expected Expr stmt, got {other:?}"),
    }
}

// =====================================================================
// Borrow expression (ADR-0052a)
// =====================================================================

#[test]
fn borrow_expr_parses() {
    let e = first_expr("&x\n");
    assert!(
        matches!(&e.kind, ExprKind::Borrow(_)),
        "expected Borrow, got {e:?}"
    );
}

// =====================================================================
// Shift operators
// =====================================================================

#[test]
fn shr_operator() {
    let e = first_expr("a >> b\n");
    assert!(
        matches!(&e.kind, ExprKind::Binary { op: BinOp::Shr, .. }),
        "expected Binary(Shr)"
    );
}

#[test]
fn shl_left_assoc() {
    // a << b << c → Binary(Shl, Binary(Shl, a, b), c)
    let e = first_expr("a << b << c\n");
    match &e.kind {
        ExprKind::Binary { op, lhs, .. } => {
            assert_eq!(op, &BinOp::Shl);
            assert!(
                matches!(&lhs.kind, ExprKind::Binary { op: BinOp::Shl, .. }),
                "lhs must be Shl (left-assoc)"
            );
        }
        other => panic!("expected Binary(Shl), got {other:?}"),
    }
}
