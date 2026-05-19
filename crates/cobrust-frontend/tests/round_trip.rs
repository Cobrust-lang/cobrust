#![allow(clippy::too_many_lines)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::similar_names)]
#![allow(clippy::single_match_else)]
//! Round-trip integration test for the M1 "core 30 forms".
//!
//! For each form listed in `docs/agent/adr/0003-core-30-forms.md`,
//! we parse a curated snippet, then assert
//! `parse(unparse(ast)) == ast` modulo span normalization.
//!
//! Spans are zero-ed in the equality check because the unparser
//! emits canonical layout, not byte-faithful source. The equality
//! contract is on AST *shape*.

use cobrust_frontend::ast;
use cobrust_frontend::ast::{Block, Expr, FStrPart, Module, Param, Pattern, Stmt, Type};
use cobrust_frontend::span::{FileId, Span};
use cobrust_frontend::{parse_str, unparse};

/// Run the round-trip property on `src` and report which form failed.
fn round_trip(name: &str, src: &str) {
    let module1 = match parse_str(src, FileId::SYNTHETIC) {
        Ok(m) => m,
        Err(e) => panic!("{name}: parse failed: {e:?}\n--- source ---\n{src}"),
    };
    let unparsed = unparse(&module1);
    let module2 = match parse_str(&unparsed, FileId::SYNTHETIC) {
        Ok(m) => m,
        Err(e) => panic!(
            "{name}: re-parse of unparsed source failed: {e:?}\n--- unparsed ---\n{unparsed}"
        ),
    };
    let m1 = normalize(&module1);
    let m2 = normalize(&module2);
    assert_eq!(
        m1, m2,
        "{name}: round-trip mismatch\n--- original source ---\n{src}\n--- unparsed ---\n{unparsed}\n--- ast1 ---\n{module1:#?}\n--- ast2 ---\n{module2:#?}"
    );
}

// =====================================================================
// Forms 1..=6 — module / definitions / decorator / type alias
// =====================================================================

#[test]
fn form_01_module() {
    round_trip("01-module", "\"hello world\"\npass\n");
}

#[test]
fn form_02_import_stmt() {
    round_trip(
        "02-import",
        "import collections.abc\nimport os.path as p\nfrom math import pi, e as euler\n",
    );
}

#[test]
fn form_03_fn_def() {
    round_trip(
        "03-fn",
        "fn add(x: i64, y: i64 = 0) -> i64:\n    return (x + y)\n",
    );
}

#[test]
fn form_04_class_def() {
    round_trip(
        "04-class",
        "class Point(Shape): Drawable, Hashable:\n    pass\n",
    );
}

#[test]
fn form_05_decorator() {
    round_trip(
        "05-decorator",
        "@cached\n@inline\nfn pi() -> f64:\n    return 3.14\n",
    );
}

#[test]
fn form_06_type_alias() {
    round_trip("06-type-alias", "type Result[T] = Ok | Err\n");
}

// =====================================================================
// Forms 7..=19 — statements
// =====================================================================

#[test]
fn form_07_let_stmt() {
    round_trip("07-let", "let pi: f64 = 3.14\nlet name = \"Cobra\"\n");
}

#[test]
fn form_08_assign_stmt() {
    round_trip("08-assign", "x = 1\nx += 2\ny -= 3\nz *= 4\nw //= 5\n");
}

#[test]
fn form_09_if_stmt() {
    round_trip(
        "09-if",
        "if (x > 0):\n    y = 1\nelif (x == 0):\n    y = 0\nelse:\n    y = -1\n",
    );
}

#[test]
fn form_10_while_stmt() {
    round_trip("10-while", "while (i < 10):\n    i += 1\nelse:\n    pass\n");
}

#[test]
fn form_11_for_stmt() {
    round_trip(
        "11-for",
        "for x in xs:\n    pass\nfor (k, v) in items:\n    pass\n",
    );
}

#[test]
fn form_12_match_stmt() {
    round_trip(
        "12-match",
        "match r:\n    case Ok(v):\n        x = v\n    case Err(_):\n        x = 0\n",
    );
}

#[test]
fn form_13_with_stmt() {
    round_trip("13-with", "with open(p) as f, lock(m):\n    pass\n");
}

#[test]
fn form_14_try_stmt() {
    round_trip(
        "14-try",
        "try:\n    parse()\nexcept IoError as e:\n    log(e)\nexcept ValueError:\n    pass\nelse:\n    ok()\nfinally:\n    cleanup()\n",
    );
}

#[test]
fn form_15_return_stmt() {
    round_trip("15-return", "fn f() -> i64:\n    return 42\n");
}

#[test]
fn form_16_break_continue() {
    round_trip(
        "16-break-continue",
        "while True:\n    if x:\n        break\n    else:\n        continue\n",
    );
}

#[test]
fn form_17_raise_stmt() {
    round_trip("17-raise", "raise IoError(\"bad path\") from cause\n");
}

#[test]
fn form_18_pass_stmt() {
    round_trip("18-pass", "pass\n");
}

#[test]
fn form_19_expr_stmt() {
    round_trip("19-expr-stmt", "compute(42)\n");
}

// =====================================================================
// Form 20 — pattern sub-grammar (covered via match arms exercising each subkind)
// =====================================================================

#[test]
fn form_20_pattern_subkinds() {
    // Wildcard, binding, literal, sequence, mapping, class, or, guard.
    round_trip(
        "20-patterns",
        "match v:\n    case 0:\n        pass\n    case [1, 2, *rest]:\n        pass\n    case {\"k\": x, **rest}:\n        pass\n    case Point(x=0, y):\n        pass\n    case 1 | 2 | 3:\n        pass\n    case (a, b) if (a == b):\n        pass\n    case _:\n        pass\n",
    );
}

// =====================================================================
// Forms 21..=30 — expressions
// =====================================================================

#[test]
fn form_21_literal_expr() {
    round_trip(
        "21-literal",
        "x = 0xFF_FF\ny = 1.5e-3\nz = 3j\nq = True\nr = None\ns = b\"\\x00\"\n",
    );
}

#[test]
fn form_22_fstring_expr() {
    round_trip(
        "22-fstring",
        "x = f\"hello {name}!\"\ny = f\"x={value:>10}\"\n",
    );
}

#[test]
fn form_23_name_expr() {
    round_trip("23-name", "count = 0\ntotal = (count + 1)\n");
}

#[test]
fn form_24_collection_expr() {
    round_trip(
        "24-collection",
        "a = (1, 2)\nb = [1, 2, 3]\nc = {1, 2}\nd = {\"k\": v, **rest}\n",
    );
}

#[test]
fn form_25_comprehension_expr() {
    round_trip(
        "25-comprehension",
        "a = [(x * x) for x in xs if (x > 0)]\nb = {x for x in xs}\nc = {k: v for (k, v) in items}\nd = (x for x in xs)\n",
    );
}

#[test]
fn form_26_lambda_expr() {
    round_trip("26-lambda", "f = lambda x: (x + 1)\ng = lambda: 0\n");
}

#[test]
fn form_27_call_expr() {
    round_trip("27-call", "y = f(1, 2, key=v, *xs, **kw)\n");
}

#[test]
fn form_28_access_expr() {
    round_trip(
        "28-access",
        "a = obj.field.nested\nb = arr[1:10:2]\nc = arr[i, j]\nd = arr[0]\n",
    );
}

#[test]
fn form_29_binary_unary_expr() {
    round_trip(
        "29-binary-unary",
        "x = (((not (a and b)) or (c | (2 << 1))) == ((((-d + e) * 2) % 3)))\n",
    );
}

#[test]
fn form_30_await_yield_expr() {
    round_trip(
        "30-await-yield",
        "fn fetch_all():\n    let v = (await fetch(u))\n    (yield v)\n    (yield from xs)\n",
    );
}

// =====================================================================
// Span normalization
// =====================================================================

const ZERO: Span = Span {
    file: FileId(0),
    start: 0,
    end: 0,
};

fn normalize(m: &Module) -> Module {
    let mut out = m.clone();
    out.span = ZERO;
    out.items = out.items.iter().map(norm_stmt).collect();
    out
}

fn norm_stmt(s: &Stmt) -> Stmt {
    let kind = match s.kind.clone() {
        ast::StmtKind::Decorated { decorators, inner } => ast::StmtKind::Decorated {
            decorators: decorators.iter().map(norm_expr).collect(),
            inner: Box::new(norm_stmt(&inner)),
        },
        ast::StmtKind::Fn(fd) => ast::StmtKind::Fn(ast::FnDef {
            name: fd.name,
            params: norm_params(&fd.params),
            return_type: fd.return_type.as_ref().map(norm_type),
            body: norm_block(&fd.body),
        }),
        ast::StmtKind::Class(cd) => ast::StmtKind::Class(ast::ClassDef {
            name: cd.name,
            base: cd.base.as_ref().map(norm_expr),
            traits: cd.traits.iter().map(norm_type).collect(),
            body: norm_block(&cd.body),
        }),
        ast::StmtKind::Let {
            target,
            annot,
            value,
        } => ast::StmtKind::Let {
            target: norm_pattern(&target),
            annot: annot.as_ref().map(norm_type),
            value: norm_expr(&value),
        },
        ast::StmtKind::Assign { target, op, value } => ast::StmtKind::Assign {
            target: Box::new(norm_expr(&target)),
            op,
            value: norm_expr(&value),
        },
        ast::StmtKind::If {
            cond,
            then_block,
            elifs,
            else_block,
        } => ast::StmtKind::If {
            cond: norm_expr(&cond),
            then_block: norm_block(&then_block),
            elifs: elifs
                .iter()
                .map(|(c, b)| (norm_expr(c), norm_block(b)))
                .collect(),
            else_block: else_block.as_ref().map(norm_block),
        },
        ast::StmtKind::While {
            cond,
            body,
            else_block,
        } => ast::StmtKind::While {
            cond: norm_expr(&cond),
            body: norm_block(&body),
            else_block: else_block.as_ref().map(norm_block),
        },
        ast::StmtKind::For {
            target,
            iter,
            body,
            else_block,
        } => ast::StmtKind::For {
            target: norm_pattern(&target),
            iter: norm_expr(&iter),
            body: norm_block(&body),
            else_block: else_block.as_ref().map(norm_block),
        },
        ast::StmtKind::Match { scrutinee, arms } => ast::StmtKind::Match {
            scrutinee: norm_expr(&scrutinee),
            arms: arms
                .iter()
                .map(|a| ast::MatchArm {
                    pattern: norm_pattern(&a.pattern),
                    guard: a.guard.as_ref().map(norm_expr),
                    body: norm_block(&a.body),
                })
                .collect(),
        },
        ast::StmtKind::With { items, body } => ast::StmtKind::With {
            items: items
                .iter()
                .map(|w| ast::WithItem {
                    context: norm_expr(&w.context),
                    target: w.target.as_ref().map(norm_pattern),
                })
                .collect(),
            body: norm_block(&body),
        },
        ast::StmtKind::Try {
            body,
            handlers,
            else_block,
            finally_block,
        } => ast::StmtKind::Try {
            body: norm_block(&body),
            handlers: handlers
                .iter()
                .map(|h| ast::ExceptHandler {
                    exc_type: norm_type(&h.exc_type),
                    binding: h.binding.clone(),
                    body: norm_block(&h.body),
                })
                .collect(),
            else_block: else_block.as_ref().map(norm_block),
            finally_block: finally_block.as_ref().map(norm_block),
        },
        ast::StmtKind::Return(v) => ast::StmtKind::Return(v.as_ref().map(norm_expr)),
        ast::StmtKind::Raise { exc, cause } => ast::StmtKind::Raise {
            exc: exc.as_ref().map(norm_expr),
            cause: cause.as_ref().map(norm_expr),
        },
        ast::StmtKind::Expr(e) => ast::StmtKind::Expr(norm_expr(&e)),
        ast::StmtKind::TypeAlias(ta) => ast::StmtKind::TypeAlias(ast::TypeAlias {
            name: ta.name,
            type_params: ta.type_params,
            value: norm_type(&ta.value),
        }),
        // Variants without nested spans:
        other => other,
    };
    Stmt { kind, span: ZERO }
}

fn norm_block(b: &Block) -> Block {
    Block {
        stmts: b.stmts.iter().map(norm_stmt).collect(),
        span: ZERO,
    }
}

fn norm_expr(e: &Expr) -> Expr {
    let kind = match e.kind.clone() {
        ast::ExprKind::Literal(l) => ast::ExprKind::Literal(l),
        ast::ExprKind::FString(parts) => ast::ExprKind::FString(
            parts
                .into_iter()
                .map(|p| match p {
                    FStrPart::Lit(s) => FStrPart::Lit(s),
                    FStrPart::Expr {
                        expr,
                        debug_equals,
                        format_spec,
                    } => FStrPart::Expr {
                        expr: Box::new(norm_expr(&expr)),
                        debug_equals,
                        format_spec,
                    },
                })
                .collect(),
        ),
        ast::ExprKind::Name(n) => ast::ExprKind::Name(n),
        ast::ExprKind::Collection(c) => ast::ExprKind::Collection(norm_collection(c)),
        ast::ExprKind::Comprehension(c) => {
            ast::ExprKind::Comprehension(Box::new(norm_comprehension(*c)))
        }
        ast::ExprKind::Lambda { params, body } => ast::ExprKind::Lambda {
            params: norm_params(&params),
            body: Box::new(norm_expr(&body)),
        },
        ast::ExprKind::Call { callee, args } => ast::ExprKind::Call {
            callee: Box::new(norm_expr(&callee)),
            args: args
                .into_iter()
                .map(|a| match a {
                    ast::CallArg::Positional(e) => ast::CallArg::Positional(norm_expr(&e)),
                    ast::CallArg::Keyword(n, e) => ast::CallArg::Keyword(n, norm_expr(&e)),
                    ast::CallArg::StarArgs(e) => ast::CallArg::StarArgs(norm_expr(&e)),
                    ast::CallArg::StarStarKwargs(e) => ast::CallArg::StarStarKwargs(norm_expr(&e)),
                })
                .collect(),
        },
        ast::ExprKind::Access(ast::AccessKind::Attribute { base, name }) => {
            ast::ExprKind::Access(ast::AccessKind::Attribute {
                base: Box::new(norm_expr(&base)),
                name,
            })
        }
        ast::ExprKind::Access(ast::AccessKind::Index { base, index }) => {
            ast::ExprKind::Access(ast::AccessKind::Index {
                base: Box::new(norm_expr(&base)),
                index: Box::new(norm_index(*index)),
            })
        }
        ast::ExprKind::Binary { op, lhs, rhs } => ast::ExprKind::Binary {
            op,
            lhs: Box::new(norm_expr(&lhs)),
            rhs: Box::new(norm_expr(&rhs)),
        },
        ast::ExprKind::Unary { op, operand } => ast::ExprKind::Unary {
            op,
            operand: Box::new(norm_expr(&operand)),
        },
        // ADR-0052a Wave-1 — `&expr` borrow round-trip.
        ast::ExprKind::Borrow(inner) => ast::ExprKind::Borrow(Box::new(norm_expr(&inner))),
        ast::ExprKind::Await(e) => ast::ExprKind::Await(Box::new(norm_expr(&e))),
        ast::ExprKind::Yield(None) => ast::ExprKind::Yield(None),
        ast::ExprKind::Yield(Some(e)) => ast::ExprKind::Yield(Some(Box::new(norm_expr(&e)))),
        ast::ExprKind::YieldFrom(e) => ast::ExprKind::YieldFrom(Box::new(norm_expr(&e))),
        ast::ExprKind::Cast { expr, target } => ast::ExprKind::Cast {
            expr: Box::new(norm_expr(&expr)),
            target,
        },
    };
    Expr { kind, span: ZERO }
}

fn norm_collection(c: ast::CollectionLit) -> ast::CollectionLit {
    match c {
        ast::CollectionLit::Tuple(v) => {
            ast::CollectionLit::Tuple(v.iter().map(norm_expr).collect())
        }
        ast::CollectionLit::List(v) => ast::CollectionLit::List(v.iter().map(norm_expr).collect()),
        ast::CollectionLit::Set(v) => ast::CollectionLit::Set(v.iter().map(norm_expr).collect()),
        ast::CollectionLit::Dict(v) => ast::CollectionLit::Dict(
            v.into_iter()
                .map(|d| match d {
                    ast::DictEntry::Pair(k, v) => {
                        ast::DictEntry::Pair(norm_expr(&k), norm_expr(&v))
                    }
                    ast::DictEntry::Spread(e) => ast::DictEntry::Spread(norm_expr(&e)),
                })
                .collect(),
        ),
    }
}

fn norm_comprehension(c: ast::Comprehension) -> ast::Comprehension {
    ast::Comprehension {
        kind: c.kind,
        element: match c.element {
            ast::ComprehensionElem::Single(e) => ast::ComprehensionElem::Single(norm_expr(&e)),
            ast::ComprehensionElem::KeyValue(k, v) => {
                ast::ComprehensionElem::KeyValue(norm_expr(&k), norm_expr(&v))
            }
        },
        clauses: c
            .clauses
            .into_iter()
            .map(|cl| ast::ComprehensionClause {
                target: norm_pattern(&cl.target),
                iter: norm_expr(&cl.iter),
                guards: cl.guards.iter().map(norm_expr).collect(),
            })
            .collect(),
    }
}

fn norm_index(idx: ast::IndexKind) -> ast::IndexKind {
    match idx {
        ast::IndexKind::Expr(e) => ast::IndexKind::Expr(norm_expr(&e)),
        ast::IndexKind::Slice { start, stop, step } => ast::IndexKind::Slice {
            start: start.as_ref().map(norm_expr),
            stop: stop.as_ref().map(norm_expr),
            step: step.as_ref().map(norm_expr),
        },
        ast::IndexKind::Tuple(items) => {
            ast::IndexKind::Tuple(items.into_iter().map(norm_index).collect())
        }
    }
}

fn norm_type(t: &Type) -> Type {
    let kind = match t.kind.clone() {
        ast::TypeKind::Name(n) => ast::TypeKind::Name(n),
        ast::TypeKind::Generic { base, args } => ast::TypeKind::Generic {
            base,
            args: args.iter().map(norm_type).collect(),
        },
        ast::TypeKind::Union(parts) => ast::TypeKind::Union(parts.iter().map(norm_type).collect()),
        ast::TypeKind::Fn {
            params,
            return_type,
        } => ast::TypeKind::Fn {
            params: params.iter().map(norm_type).collect(),
            return_type: Box::new(norm_type(&return_type)),
        },
        ast::TypeKind::Tuple(parts) => ast::TypeKind::Tuple(parts.iter().map(norm_type).collect()),
        // ADR-0060b — recurse into Ref + Array inner annotations.
        ast::TypeKind::Ref(inner) => ast::TypeKind::Ref(Box::new(norm_type(&inner))),
        ast::TypeKind::Array { elem, len } => ast::TypeKind::Array {
            elem: Box::new(norm_type(&elem)),
            len,
        },
    };
    Type { kind, span: ZERO }
}

fn norm_pattern(p: &Pattern) -> Pattern {
    let kind = match p.kind.clone() {
        ast::PatternKind::Wildcard => ast::PatternKind::Wildcard,
        ast::PatternKind::Binding(s) => ast::PatternKind::Binding(s),
        ast::PatternKind::Literal(l) => ast::PatternKind::Literal(l),
        ast::PatternKind::Sequence { items, rest } => ast::PatternKind::Sequence {
            items: items.iter().map(norm_pattern).collect(),
            rest: rest.map(|r| Box::new(norm_pattern(&r))),
        },
        ast::PatternKind::Mapping { entries, rest } => ast::PatternKind::Mapping {
            entries: entries
                .iter()
                .map(|(k, v)| (norm_expr(k), norm_pattern(v)))
                .collect(),
            rest,
        },
        ast::PatternKind::Class {
            base,
            positional,
            keyword,
        } => ast::PatternKind::Class {
            base,
            positional: positional.iter().map(norm_pattern).collect(),
            keyword: keyword
                .iter()
                .map(|(n, p)| (n.clone(), norm_pattern(p)))
                .collect(),
        },
        ast::PatternKind::Or(alts) => ast::PatternKind::Or(alts.iter().map(norm_pattern).collect()),
    };
    Pattern { kind, span: ZERO }
}

fn norm_params(p: &ast::Params) -> ast::Params {
    ast::Params {
        positional: p.positional.iter().map(norm_param).collect(),
        var_positional: p.var_positional.as_ref().map(norm_param),
        keyword_only: p.keyword_only.iter().map(norm_param).collect(),
        var_keyword: p.var_keyword.as_ref().map(norm_param),
    }
}

fn norm_param(p: &Param) -> Param {
    Param {
        name: p.name.clone(),
        annot: p.annot.as_ref().map(norm_type),
        default: p.default.clone(),
        span: ZERO,
    }
}
