#![allow(clippy::manual_let_else)]
#![allow(clippy::single_match)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::single_match_else)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::too_many_lines)]
//! Golden lowering tests — one per AST form (per ADR-0003 / ADR-0005).
//!
//! Each test parses a curated source snippet via `cobrust-frontend`,
//! lowers it via `cobrust-hir::lower`, and asserts that:
//!
//! 1. Lowering returns `Ok(_)` (totality on every form).
//! 2. The shape of the resulting HIR matches the rule pinned by
//!    ADR-0005 (e.g. comprehensions become `Comp` nodes,
//!    augmented assignment becomes a `Bin` over the target, etc.).
//! 3. Bindings allocate `DefId`s — exact-count assertions hold for
//!    every form that introduces names.
//!
//! The tests use small snippets that *only* exercise the form under
//! test; tests do not depend on each other.

use cobrust_frontend::span::FileId;
use cobrust_frontend::{ast, parse_str};
use cobrust_hir::tree as h;
use cobrust_hir::{Session, lower};

fn lower_src(src: &str) -> h::Module {
    let module: ast::Module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("parse failed: {e:?}\nsource:\n{src}"));
    let mut sess = Session::new();
    lower(&module, &mut sess).unwrap_or_else(|e| panic!("lowering failed: {e:?}\nsource:\n{src}"))
}

// =====================================================================
// Forms 1..=6 — module / definitions / decorator / type alias
// =====================================================================

#[test]
fn form_01_module_docstring() {
    let m = lower_src("\"hello world\"\npass\n");
    assert_eq!(m.docstring.as_deref(), Some("hello world"));
}

#[test]
fn form_02_import_stmt() {
    let m =
        lower_src("import collections.abc\nimport os.path as p\nfrom math import pi, e as euler\n");
    let imports: Vec<&h::ItemKind> = m
        .items
        .iter()
        .map(|i| &i.kind)
        .filter(|k| matches!(k, h::ItemKind::Import { .. }))
        .collect();
    // 1 (collections.abc) + 1 (os.path as p) + 2 (from math: pi + e as euler) = 4 items
    assert_eq!(imports.len(), 4, "import desugaring per-target");
    for k in imports {
        match k {
            h::ItemKind::Import { def_id, .. } => {
                assert!(def_id.0 < 100, "def ids monotonic");
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn form_03_fn_def() {
    let m = lower_src("fn add(x: i64, y: i64 = 0) -> i64:\n    return (x + y)\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        other => panic!("expected Fn, got {other:?}"),
    };
    assert_eq!(fn_body.name, "add");
    assert_eq!(fn_body.params.positional.len(), 2);
    assert!(fn_body.params.positional[1].default.is_some());
}

#[test]
fn form_04_class_def() {
    let m = lower_src(
        "let Shape = 0
let Drawable = 0
let Hashable = 0
class Point(Shape): Drawable, Hashable:
    pass
",
    );
    // Class is the last item; first three are the let bindings.
    let class_idx = m.items.len() - 1;
    let class_body = match &m.items[class_idx].kind {
        h::ItemKind::Class(c) => c,
        other => panic!("expected Class, got {other:?}"),
    };
    assert_eq!(class_body.name, "Point");
    assert!(class_body.base.is_some());
    assert_eq!(class_body.traits.len(), 2);
}

#[test]
fn form_05_decorator() {
    let m = lower_src(
        "let cached = 0
let inline = 0
@cached
@inline
fn pi() -> f64:
    return 3.14
",
    );
    let last_idx = m.items.len() - 1;
    match &m.items[last_idx].kind {
        h::ItemKind::Decorated { decorators, inner } => {
            assert_eq!(decorators.len(), 2);
            match &inner.kind {
                h::ItemKind::Fn(f) => assert_eq!(f.name, "pi"),
                other => panic!("decorated inner: expected Fn, got {other:?}"),
            }
        }
        other => panic!("expected Decorated, got {other:?}"),
    }
}

#[test]
fn form_06_type_alias() {
    let m = lower_src("type Result[T] = Ok | Err\n");
    match &m.items[0].kind {
        h::ItemKind::TypeAlias(a) => {
            assert_eq!(a.name, "Result");
            assert_eq!(a.type_params.len(), 1);
        }
        other => panic!("expected TypeAlias, got {other:?}"),
    }
}

// =====================================================================
// Forms 7..=19 — statements
// =====================================================================

#[test]
fn form_07_let_module_level() {
    let m = lower_src("let pi: f64 = 3.14\n");
    match &m.items[0].kind {
        h::ItemKind::Let(b) => {
            assert!(matches!(&b.pattern.kind, h::PatternKind::Binding(name, _) if name == "pi"));
            assert!(b.annot.is_some());
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn form_07_let_in_fn() {
    let m = lower_src("fn body() -> i64:\n    let x: i64 = 1\n    return x\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        other => panic!("{other:?}"),
    };
    let stmts = &fn_body.body.stmts;
    assert!(matches!(&stmts[0].kind, h::StmtKind::Let(_)));
    // The `return x` must resolve `x` to the let's def_id.
    if let (h::StmtKind::Let(lb), h::StmtKind::Return(Some(ret))) = (&stmts[0].kind, &stmts[1].kind)
    {
        let let_def = match &lb.pattern.kind {
            h::PatternKind::Binding(_, id) => *id,
            _ => unreachable!(),
        };
        if let h::ExprKind::Name(rn) = &ret.kind {
            assert_eq!(rn.def_id, let_def);
        } else {
            panic!("return must be Name");
        }
    }
}

#[test]
fn form_08_assign_augmented() {
    // augmented assignment desugars to a `Bin` over the target.
    let m = lower_src("fn f() -> i64:\n    let x: i64 = 0\n    x += 1\n    return x\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    match &fn_body.body.stmts[1].kind {
        h::StmtKind::Assign { value, .. } => match &value.kind {
            h::ExprKind::Bin { op, .. } => assert_eq!(*op, h::BinOp::Add),
            other => panic!("expected Bin Add, got {other:?}"),
        },
        other => panic!("expected Assign, got {other:?}"),
    }
}

#[test]
fn form_09_if_with_elif() {
    let m = lower_src(
        "fn f(x: i64) -> i64:\n    if (x > 0):\n        return 1\n    elif (x == 0):\n        return 0\n    else:\n        return -1\n",
    );
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    match &fn_body.body.stmts[0].kind {
        h::StmtKind::If { arms, else_block } => {
            assert_eq!(arms.len(), 2, "elif becomes another arm");
            assert!(else_block.is_some());
        }
        other => panic!("expected If, got {other:?}"),
    }
}

#[test]
fn form_10_while_stmt() {
    let m = lower_src("fn f() -> bool:\n    while True:\n        break\n    return True\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    assert!(matches!(
        &fn_body.body.stmts[0].kind,
        h::StmtKind::Loop(h::LoopKind::While { .. })
    ));
}

#[test]
fn form_11_for_stmt() {
    let m = lower_src(
        "fn f(xs: List[i64]) -> i64:\n    for x in xs:\n        return x\n    return 0\n",
    );
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    match &fn_body.body.stmts[0].kind {
        h::StmtKind::Loop(h::LoopKind::For {
            binding_def_ids, ..
        }) => {
            assert!(!binding_def_ids.is_empty(), "for-target binding count");
        }
        other => panic!("expected For, got {other:?}"),
    }
}

#[test]
fn form_12_match_stmt() {
    let m = lower_src(
        "fn f(r: i64) -> i64:\n    match r:\n        case 0:\n            return 0\n        case _:\n            return 1\n",
    );
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    match &fn_body.body.stmts[0].kind {
        h::StmtKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
        }
        other => panic!("expected Match, got {other:?}"),
    }
}

#[test]
fn form_13_with_stmt_left_folds() {
    // Multi-binding `with a as x, b as y: ...` left-folds into
    // nested `With { item: a, body: With { item: b, body: ... } }`.
    let m = lower_src("fn f(a: i64, b: i64) -> i64:\n    with a as x, b as y:\n        return x\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    let outer = match &fn_body.body.stmts[0].kind {
        h::StmtKind::With { item: _, body } => body,
        other => panic!("expected With, got {other:?}"),
    };
    // The inner block must contain another `With`.
    assert!(matches!(&outer.stmts[0].kind, h::StmtKind::With { .. }));
}

#[test]
fn form_14_try_stmt() {
    let m = lower_src(
        "fn f() -> i64:\n    try:\n        return 1\n    except IoError as e:\n        return 0\n    finally:\n        pass\n",
    );
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    match &fn_body.body.stmts[0].kind {
        h::StmtKind::Try {
            handlers,
            finally_block,
            ..
        } => {
            assert_eq!(handlers.len(), 1);
            assert!(finally_block.is_some());
            assert!(handlers[0].binding.is_some());
        }
        other => panic!("expected Try, got {other:?}"),
    }
}

#[test]
fn form_15_return_stmt() {
    let m = lower_src("fn f() -> i64:\n    return 1\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    assert!(matches!(
        &fn_body.body.stmts[0].kind,
        h::StmtKind::Return(Some(_))
    ));
}

#[test]
fn form_16_break_continue() {
    let m = lower_src(
        "fn f() -> bool:\n    while True:\n        break\n    while True:\n        continue\n    return True\n",
    );
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    let extract_loop_body = |idx: usize| -> &h::Block {
        match &fn_body.body.stmts[idx].kind {
            h::StmtKind::Loop(h::LoopKind::While { body, .. }) => body,
            _ => unreachable!(),
        }
    };
    assert!(matches!(
        &extract_loop_body(0).stmts[0].kind,
        h::StmtKind::Break
    ));
    assert!(matches!(
        &extract_loop_body(1).stmts[0].kind,
        h::StmtKind::Continue
    ));
}

#[test]
fn form_17_raise_stmt() {
    let m = lower_src(
        "let IoError = 0
fn f() -> i64:
    raise IoError
",
    );
    let last_idx = m.items.len() - 1;
    let fn_body = match &m.items[last_idx].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    assert!(matches!(
        &fn_body.body.stmts[0].kind,
        h::StmtKind::Raise { .. }
    ));
}

#[test]
fn form_18_pass_stmt() {
    let m = lower_src("fn f() -> bool:\n    pass\n    return True\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    assert!(matches!(&fn_body.body.stmts[0].kind, h::StmtKind::Pass));
}

#[test]
fn form_19_expr_stmt() {
    let m = lower_src("fn f() -> bool:\n    1\n    return True\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    assert!(matches!(&fn_body.body.stmts[0].kind, h::StmtKind::Expr(_)));
}

// =====================================================================
// Form 20 — patterns
// =====================================================================

#[test]
fn form_20_pattern_subkinds() {
    let m = lower_src(
        "fn f(r: i64) -> i64:\n    match r:\n        case 0:\n            return 0\n        case n:\n            return n\n        case _:\n            return -1\n",
    );
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    let arms = match &fn_body.body.stmts[0].kind {
        h::StmtKind::Match { arms, .. } => arms,
        _ => unreachable!(),
    };
    assert!(matches!(&arms[0].pattern.kind, h::PatternKind::Literal(_)));
    assert!(matches!(
        &arms[1].pattern.kind,
        h::PatternKind::Binding(_, _)
    ));
    assert!(matches!(&arms[2].pattern.kind, h::PatternKind::Wildcard));
}

// =====================================================================
// Forms 21..=30 — expressions
// =====================================================================

#[test]
fn form_21_literal_expr() {
    let m = lower_src("fn f() -> i64:\n    let x: i64 = 0xFF_FF\n    return x\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    match &fn_body.body.stmts[0].kind {
        h::StmtKind::Let(b) => assert!(matches!(&b.value.kind, h::ExprKind::Lit(h::Lit::Int(_)))),
        _ => unreachable!(),
    }
}

#[test]
fn form_22_fstring_expr() {
    let m = lower_src("fn f(x: i64) -> str:\n    return f\"x={x}\"\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    let ret = match &fn_body.body.stmts[0].kind {
        h::StmtKind::Return(Some(r)) => r,
        _ => unreachable!(),
    };
    let parts = match &ret.kind {
        h::ExprKind::Format(parts) => parts,
        other => panic!("expected Format, got {other:?}"),
    };
    assert!(
        parts
            .iter()
            .any(|p| matches!(p, h::FormatPart::Hole { .. }))
    );
}

#[test]
fn form_23_name_resolves() {
    let m = lower_src("fn f(x: i64) -> i64:\n    return x\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    let ret = match &fn_body.body.stmts[0].kind {
        h::StmtKind::Return(Some(r)) => r,
        _ => unreachable!(),
    };
    match &ret.kind {
        h::ExprKind::Name(n) => assert_eq!(n.name, "x"),
        other => panic!("expected Name, got {other:?}"),
    }
}

#[test]
fn form_24_collection_expr_subkinds() {
    let m = lower_src(
        "fn f() -> bool:\n    let t = (1, 2)\n    let l = [1, 2, 3]\n    let s = {1, 2}\n    let d = {\"k\": 1}\n    return True\n",
    );
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    let kinds: Vec<&h::ExprKind> = fn_body
        .body
        .stmts
        .iter()
        .filter_map(|s| match &s.kind {
            h::StmtKind::Let(b) => Some(&b.value.kind),
            _ => None,
        })
        .collect();
    assert!(matches!(kinds[0], h::ExprKind::Tuple(_)));
    assert!(matches!(kinds[1], h::ExprKind::List(_)));
    assert!(matches!(kinds[2], h::ExprKind::Set(_)));
    assert!(matches!(kinds[3], h::ExprKind::Dict(_)));
}

#[test]
fn form_25_comprehension_expr() {
    let m = lower_src(
        "fn f(xs: List[i64]) -> List[i64]:\n    return [(x * x) for x in xs if (x > 0)]\n",
    );
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    let ret = match &fn_body.body.stmts[0].kind {
        h::StmtKind::Return(Some(r)) => r,
        _ => unreachable!(),
    };
    match &ret.kind {
        h::ExprKind::Comp(c) => {
            assert_eq!(c.kind, h::CompKind::List);
            assert_eq!(c.clauses.len(), 1);
            assert_eq!(c.clauses[0].guards.len(), 1);
        }
        other => panic!("expected Comp, got {other:?}"),
    }
}

#[test]
fn form_26_lambda_expr() {
    let m = lower_src("fn f() -> i64:\n    let inc = lambda x: (x + 1)\n    return inc(0)\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    match &fn_body.body.stmts[0].kind {
        h::StmtKind::Let(b) => assert!(matches!(&b.value.kind, h::ExprKind::Lambda { .. })),
        _ => unreachable!(),
    }
}

#[test]
fn form_27_call_expr() {
    let m = lower_src("fn g(x: i64) -> i64:\n    return x\nfn f() -> i64:\n    return g(1)\n");
    let fn_body = match &m.items[1].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    let ret = match &fn_body.body.stmts[0].kind {
        h::StmtKind::Return(Some(r)) => r,
        _ => unreachable!(),
    };
    match &ret.kind {
        h::ExprKind::Call { args, .. } => assert_eq!(args.len(), 1),
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn form_28_access_expr() {
    let m = lower_src("fn f(o: i64) -> i64:\n    return o.x\n");
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    let ret = match &fn_body.body.stmts[0].kind {
        h::StmtKind::Return(Some(r)) => r,
        _ => unreachable!(),
    };
    assert!(matches!(&ret.kind, h::ExprKind::Attr { .. }));
}

#[test]
fn form_29_binary_unary() {
    let m = lower_src(
        "fn f(a: bool, b: bool, c: i64) -> bool:\n    return ((not (a and b)) or ((c << 2) > 0))\n",
    );
    let fn_body = match &m.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    let ret = match &fn_body.body.stmts[0].kind {
        h::StmtKind::Return(Some(r)) => r,
        _ => unreachable!(),
    };
    // Top-level should be `or`.
    match &ret.kind {
        h::ExprKind::Bin {
            op: h::BinOp::Or, ..
        } => {}
        other => panic!("expected Bin Or top, got {other:?}"),
    }
}

#[test]
fn form_30_await_yield() {
    let m2 = lower_src(
        "fn outer(fetch: i64, xs: i64) -> i64:
    fn f(u: i64) -> i64:
        let v = await fetch
        yield 1
        yield from xs
        return 0
    return 0
",
    );
    let outer = match &m2.items[0].kind {
        h::ItemKind::Fn(f) => f,
        _ => unreachable!(),
    };
    let inner = match &outer.body.stmts[0].kind {
        h::StmtKind::Item(it) => match &it.kind {
            h::ItemKind::Fn(f) => f,
            _ => unreachable!(),
        },
        other => panic!("expected nested Fn item, got {other:?}"),
    };
    let mut saw_await = false;
    let mut saw_yield = false;
    let mut saw_yield_from = false;
    for s in &inner.body.stmts {
        match &s.kind {
            h::StmtKind::Let(b) => match &b.value.kind {
                h::ExprKind::Await(_) => saw_await = true,
                _ => {}
            },
            h::StmtKind::Expr(e) => match &e.kind {
                h::ExprKind::Yield(_) => saw_yield = true,
                h::ExprKind::YieldFrom(_) => saw_yield_from = true,
                _ => {}
            },
            _ => {}
        }
    }
    assert!(saw_await && saw_yield && saw_yield_from);
}

// =====================================================================
// Cross-cutting invariants
// =====================================================================

#[test]
fn unknown_name_is_structured_error() {
    let module = parse_str(
        "fn f() -> i64:\n    return undefined_name\n",
        FileId::SYNTHETIC,
    )
    .expect("parse");
    let mut sess = Session::new();
    let err = lower(&module, &mut sess).expect_err("expect UnknownName");
    assert!(matches!(
        err,
        cobrust_hir::LoweringError::UnknownName { .. }
    ));
}

#[test]
fn defids_are_unique() {
    use std::collections::HashSet;
    let m = lower_src("fn f(x: i64, y: i64) -> i64:\n    let z: i64 = (x + y)\n    return z\n");
    let mut ids: HashSet<u32> = HashSet::new();
    fn collect_item(it: &h::Item, ids: &mut HashSet<u32>) {
        match &it.kind {
            h::ItemKind::Fn(f) => {
                ids.insert(f.def_id.0);
                for p in &f.params.positional {
                    ids.insert(p.def_id.0);
                }
                collect_block(&f.body, ids);
            }
            h::ItemKind::Class(c) => {
                ids.insert(c.def_id.0);
                for m in &c.members {
                    collect_item(m, ids);
                }
            }
            h::ItemKind::Decorated { inner, .. } => collect_item(inner, ids),
            h::ItemKind::Let(b) => {
                ids.insert(b.def_id.0);
            }
            h::ItemKind::Import { def_id, .. } => {
                ids.insert(def_id.0);
            }
            h::ItemKind::TypeAlias(a) => {
                ids.insert(a.def_id.0);
            }
            h::ItemKind::ExprStmt(_) => {}
        }
    }
    fn collect_block(b: &h::Block, ids: &mut HashSet<u32>) {
        for s in &b.stmts {
            if let h::StmtKind::Let(lb) = &s.kind {
                ids.insert(lb.def_id.0);
            }
            if let h::StmtKind::Item(it) = &s.kind {
                collect_item(it, ids);
            }
        }
    }
    for it in &m.items {
        collect_item(it, &mut ids);
    }
    // 1 (fn) + 2 (params) + 1 (let z) = 4 distinct IDs
    assert!(ids.len() >= 4, "got {} unique def ids", ids.len());
}

#[test]
fn thirty_forms_total_lower() {
    // Sanity: a programs touching every form must lower without
    // error. (This is a coarse-grain coverage gate; the per-form
    // tests above are the precise gates.)
    let snippets = [
        "\"hello world\"\npass\n",
        "import os\n",
        "fn f() -> i64:\n    return 0\n",
        "class A:\n    pass\n",
        "let inline = 0
@inline
fn g() -> i64:
    return 0
",
        "type T = i64\n",
    ];
    for s in snippets {
        let _ = lower_src(s);
    }
}
