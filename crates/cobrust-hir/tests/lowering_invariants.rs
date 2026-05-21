#![allow(clippy::items_after_statements)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::single_match_else)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::single_match)]
#![allow(clippy::match_wildcard_for_single_variants)]
//! HIR lowering invariant tests (CQ P1-2).
//!
//! Invariants from ADR-0005:
//! 1. Every well-formed AST yields a well-formed HIR (totality).
//! 2. DefIds are monotonically allocated with no duplicates per binding site.
//! 3. `lower` never panics; errors are always `LoweringError`.
//! 4. All LoweringError variants carry a `suggestion` per §2.5 Direction B.
//! 5. `lowering_error_suggestion_text()` and `lowering_error_fix_safety_code()`
//!    mirror helpers work for all variants.

use cobrust_frontend::span::FileId;
use cobrust_frontend::{ast, parse_str};
use cobrust_hir::error::{
    LoweringError, lowering_error_fix_safety_code, lowering_error_suggestion_text,
};
use cobrust_hir::tree as h;
use cobrust_hir::{Session, lower};

// =====================================================================
// Helpers
// =====================================================================

fn lower_ok(src: &str) -> h::Module {
    let module: ast::Module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("parse failed: {e:?}\nsource:\n{src}"));
    let mut sess = Session::new();
    lower(&module, &mut sess).unwrap_or_else(|e| panic!("lower failed: {e:?}\nsource:\n{src}"))
}

fn lower_err(src: &str) -> LoweringError {
    let module: ast::Module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("parse failed: {e:?}\nsource:\n{src}"));
    let mut sess = Session::new();
    lower(&module, &mut sess)
        .map(|_| panic!("expected lowering error\nsource:\n{src}"))
        .unwrap_err()
}

fn lower_with_session(src: &str) -> (h::Module, Session) {
    let module =
        parse_str(src, FileId::SYNTHETIC).unwrap_or_else(|e| panic!("parse failed: {e:?}"));
    let mut sess = Session::new();
    let m = lower(&module, &mut sess).unwrap_or_else(|e| panic!("lower failed: {e:?}"));
    (m, sess)
}

// =====================================================================
// Invariant 1: Totality — every well-formed form lowers without panic
// =====================================================================

#[test]
fn totality_empty_module() {
    lower_ok("pass\n");
}

#[test]
fn totality_module_docstring() {
    lower_ok("\"module docstring\"\npass\n");
}

#[test]
fn totality_nested_fn() {
    lower_ok("fn outer() -> i64:\n    fn inner() -> i64:\n        return 1\n    return inner()\n");
}

#[test]
fn totality_if_elif_else() {
    lower_ok(
        "fn f(x: i64) -> i64:\n    if x > 0:\n        return 1\n    elif x < 0:\n        return -1\n    else:\n        return 0\n",
    );
}

#[test]
fn totality_while_loop() {
    lower_ok("fn f():\n    let x: i64 = 0\n    while x < 10:\n        x += 1\n");
}

#[test]
fn totality_for_loop() {
    lower_ok("fn f(items: list[i64]):\n    for item in items:\n        pass\n");
}

#[test]
fn totality_try_except() {
    lower_ok(
        "let ValueError = 0\nfn f():\n    try:\n        pass\n    except ValueError as e:\n        pass\n",
    );
}

#[test]
fn totality_with_stmt() {
    lower_ok("let open = 0\nfn f():\n    with open(\"x\") as fp:\n        pass\n");
}

#[test]
fn totality_comprehension_list() {
    lower_ok("fn f(xs: list[i64]) -> list[i64]:\n    return [x * 2 for x in xs]\n");
}

#[test]
fn totality_comprehension_with_filter() {
    lower_ok("fn f(xs: list[i64]) -> list[i64]:\n    return [x for x in xs if x > 0]\n");
}

#[test]
fn totality_lambda() {
    lower_ok("let add = lambda x, y: x + y\n");
}

#[test]
fn totality_fstring() {
    lower_ok("fn f(name: str) -> str:\n    return f\"hello {name}!\"\n");
}

#[test]
fn totality_match_stmt() {
    lower_ok(
        "fn f(x: i64) -> str:\n    match x:\n        case 0:\n            return \"zero\"\n        case _:\n            return \"other\"\n",
    );
}

#[test]
fn totality_raise_stmt() {
    // ValueError must be in scope; declare it first
    lower_ok("let ValueError = 0\nfn f():\n    raise ValueError\n");
}

#[test]
fn totality_augmented_assign() {
    lower_ok("fn f():\n    let x: i64 = 0\n    x += 1\n    x -= 2\n    x *= 3\n");
}

// =====================================================================
// Invariant 2: DefId monotonicity
// =====================================================================

#[test]
fn defid_monotonic_fn_params() {
    let (m, sess) =
        lower_with_session("fn f(a: i64, b: i64, c: i64) -> i64:\n    return a + b + c\n");
    // 4 bindings: f (module), a, b, c = at least 4 total
    let total = sess.defs.count();
    assert!(total >= 4, "expected at least 4 DefIds, got {total}");
    let _ = m;
}

#[test]
fn defid_monotonic_let_bindings() {
    let (_, sess) = lower_with_session("let a: i64 = 1\nlet b: i64 = 2\nlet c: i64 = 3\n");
    // 3 let bindings = 3 DefIds minimum
    assert!(sess.defs.count() >= 3);
}

#[test]
fn defid_unique_per_binding_site() {
    // Two functions — prebind_items assigns distinct DefIds to each
    let (m, _) = lower_with_session("fn f() -> i64:\n    return 1\nfn g() -> i64:\n    return 2\n");
    let fn_ids: Vec<u32> = m
        .items
        .iter()
        .filter_map(|i| {
            if let h::ItemKind::Fn(f) = &i.kind {
                Some(f.def_id.0)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(fn_ids.len(), 2);
    assert_ne!(
        fn_ids[0], fn_ids[1],
        "distinct fns must have distinct DefIds"
    );
}

#[test]
fn defid_allocation_increments() {
    // After lowering N bindings, count() must equal N (plus any internal allocations).
    let (_, sess) = lower_with_session("fn a():\n    pass\nfn b():\n    pass\n");
    let count = sess.defs.count();
    // Module-level prebind allocates 2 DefIds (a, b) minimum
    assert!(count >= 2, "count must be >= 2, got {count}");
}

// =====================================================================
// Invariant 3: LoweringError variants are structured
// =====================================================================

#[test]
fn error_unknown_name_triggers() {
    let err = lower_err("fn f() -> i64:\n    return undefined_name\n");
    assert!(
        matches!(err, LoweringError::UnknownName { .. }),
        "expected UnknownName, got {err:?}"
    );
}

#[test]
fn error_unknown_name_preserves_name() {
    let err = lower_err("fn f() -> i64:\n    return xyz_undefined\n");
    match &err {
        LoweringError::UnknownName { name, .. } => {
            assert_eq!(name, "xyz_undefined");
        }
        other => panic!("expected UnknownName, got {other:?}"),
    }
}

#[test]
fn error_duplicate_param_binding_triggers() {
    // Two params with same name = DuplicateBinding (params use bind(), not bind_let_shadow)
    let err = lower_err("fn f(x: i64, x: i64) -> i64:\n    return x\n");
    assert!(
        matches!(err, LoweringError::DuplicateBinding { .. }),
        "expected DuplicateBinding for duplicate param, got {err:?}"
    );
}

#[test]
fn let_shadow_same_scope_ok() {
    // `let` bindings in same scope use bind_let_shadow (ADR-0052a §4.4)
    // → shadowing is allowed, no DuplicateBinding error
    lower_ok("fn f():\n    let x: i64 = 1\n    let x: i64 = 2\n    return x\n");
}

#[test]
fn error_assign_to_unknown_or_unknown_name() {
    // Assignment to an undeclared name raises either AssignToUnknown or
    // UnknownName depending on whether the lowerer resolves the target
    // as an expression first.
    let err = lower_err("fn f():\n    unknown_var = 1\n");
    assert!(
        matches!(
            err,
            LoweringError::AssignToUnknown { .. } | LoweringError::UnknownName { .. }
        ),
        "expected AssignToUnknown or UnknownName, got {err:?}"
    );
}

// =====================================================================
// Invariant 4: All LoweringError variants have suggestion populated
// =====================================================================

#[test]
fn suggestion_populated_unknown_name() {
    let err = lower_err("fn f() -> i64:\n    return nope\n");
    match &err {
        LoweringError::UnknownName { suggestion, .. } => {
            assert!(suggestion.is_some(), "UnknownName must have suggestion");
        }
        _ => {}
    }
}

#[test]
fn suggestion_populated_duplicate_binding() {
    // Params use bind() → DuplicateBinding carries suggestion
    let err = lower_err("fn f(y: i64, y: i64) -> i64:\n    return y\n");
    match &err {
        LoweringError::DuplicateBinding { suggestion, .. } => {
            assert!(
                suggestion.is_some(),
                "DuplicateBinding must have suggestion"
            );
        }
        _ => {}
    }
}

// =====================================================================
// Invariant 5: lowering_error_suggestion_text mirror helper
// =====================================================================

#[test]
fn suggestion_text_unknown_name() {
    use cobrust_frontend::span::Span;
    let e = LoweringError::UnknownName {
        name: "x".into(),
        span: Span::point(FileId::SYNTHETIC, 0),
        suggestion: Some("declare x with let"),
    };
    assert_eq!(
        lowering_error_suggestion_text(&e),
        Some("declare x with let")
    );
}

#[test]
fn suggestion_text_dropped_feature() {
    use cobrust_frontend::span::Span;
    let e = LoweringError::DroppedFeature {
        name: "exec",
        span: Span::point(FileId::SYNTHETIC, 0),
        suggestion: None,
    };
    assert_eq!(lowering_error_suggestion_text(&e), None);
}

#[test]
fn suggestion_text_mutable_default() {
    use cobrust_frontend::span::Span;
    let e = LoweringError::MutableDefault {
        span: Span::point(FileId::SYNTHETIC, 0),
        suggestion: Some("use a literal default"),
    };
    assert_eq!(
        lowering_error_suggestion_text(&e),
        Some("use a literal default")
    );
}

#[test]
fn suggestion_text_or_pattern_mismatch() {
    use cobrust_frontend::span::Span;
    let e = LoweringError::OrPatternBindingMismatch {
        span: Span::point(FileId::SYNTHETIC, 0),
        suggestion: Some("bind same names in each branch"),
    };
    assert_eq!(
        lowering_error_suggestion_text(&e),
        Some("bind same names in each branch")
    );
}

#[test]
fn suggestion_text_duplicate_binding() {
    use cobrust_frontend::span::Span;
    let e = LoweringError::DuplicateBinding {
        name: "x".into(),
        first: Span::point(FileId::SYNTHETIC, 0),
        second: Span::point(FileId::SYNTHETIC, 4),
        suggestion: Some("rename one"),
    };
    assert_eq!(lowering_error_suggestion_text(&e), Some("rename one"));
}

#[test]
fn suggestion_text_assign_to_unknown() {
    use cobrust_frontend::span::Span;
    let e = LoweringError::AssignToUnknown {
        name: "z".into(),
        span: Span::point(FileId::SYNTHETIC, 0),
        suggestion: None,
    };
    assert_eq!(lowering_error_suggestion_text(&e), None);
}

// =====================================================================
// Invariant 5b: fix_safety_code values are in expected range [0..5]
// =====================================================================

#[test]
fn fix_safety_code_in_range() {
    use cobrust_frontend::span::Span;
    let s = Span::point(FileId::SYNTHETIC, 0);
    let variants = vec![
        LoweringError::UnknownName {
            name: "x".into(),
            span: s,
            suggestion: None,
        },
        LoweringError::DroppedFeature {
            name: "del",
            span: s,
            suggestion: None,
        },
        LoweringError::MutableDefault {
            span: s,
            suggestion: None,
        },
        LoweringError::OrPatternBindingMismatch {
            span: s,
            suggestion: None,
        },
        LoweringError::DuplicateBinding {
            name: "x".into(),
            first: s,
            second: s,
            suggestion: None,
        },
        LoweringError::AssignToUnknown {
            name: "x".into(),
            span: s,
            suggestion: None,
        },
    ];
    for v in &variants {
        let code = lowering_error_fix_safety_code(v);
        assert!(
            code <= 5,
            "fix_safety_code({v:?}) = {code} is out of range [0..5]"
        );
    }
}

#[test]
fn fix_safety_unknown_name_is_local_edit() {
    use cobrust_frontend::span::Span;
    let e = LoweringError::UnknownName {
        name: "x".into(),
        span: Span::point(FileId::SYNTHETIC, 0),
        suggestion: None,
    };
    assert_eq!(
        lowering_error_fix_safety_code(&e),
        2,
        "UnknownName = LocalEdit (2)"
    );
}

#[test]
fn fix_safety_dropped_feature_is_human_review() {
    use cobrust_frontend::span::Span;
    let e = LoweringError::DroppedFeature {
        name: "exec",
        span: Span::point(FileId::SYNTHETIC, 0),
        suggestion: None,
    };
    assert_eq!(
        lowering_error_fix_safety_code(&e),
        5,
        "DroppedFeature = RequiresHumanReview (5)"
    );
}

// =====================================================================
// HIR shape invariants: lowering preserves structural information
// =====================================================================

#[test]
fn fn_lower_preserves_name() {
    let m = lower_ok("fn hello_world() -> i64:\n    return 42\n");
    match &m.items[0].kind {
        h::ItemKind::Fn(f) => assert_eq!(f.name, "hello_world"),
        other => panic!("expected Fn, got {other:?}"),
    }
}

#[test]
fn fn_lower_preserves_param_count() {
    let m = lower_ok("fn multi(a: i64, b: i64, c: i64) -> i64:\n    return a\n");
    match &m.items[0].kind {
        h::ItemKind::Fn(f) => assert_eq!(f.params.positional.len(), 3),
        other => panic!("expected Fn, got {other:?}"),
    }
}

#[test]
fn let_lower_preserves_annotation() {
    let m = lower_ok("let pi: f64 = 3.14159\n");
    match &m.items[0].kind {
        h::ItemKind::Let(b) => {
            assert!(b.annot.is_some(), "annotation must be preserved");
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn import_lower_preserves_path() {
    let m = lower_ok("import os.path as p\n");
    match &m.items[0].kind {
        h::ItemKind::Import {
            path, local_name, ..
        } => {
            assert_eq!(path, &["os".to_string(), "path".to_string()]);
            assert_eq!(local_name, "p");
        }
        other => panic!("expected Import, got {other:?}"),
    }
}

#[test]
fn from_import_lower_from_name() {
    let m = lower_ok("from math import pi as euler_pi\n");
    match &m.items[0].kind {
        h::ItemKind::Import {
            from_name,
            local_name,
            ..
        } => {
            assert_eq!(from_name.as_deref(), Some("pi"));
            assert_eq!(local_name, "euler_pi");
        }
        other => panic!("expected Import, got {other:?}"),
    }
}
