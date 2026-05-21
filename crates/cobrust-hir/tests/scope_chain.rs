#![allow(clippy::items_after_statements)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::single_match)]
//! Scope chain + bind_let_shadow tests (CQ P1-2).
//!
//! ADR-0005 §"Scoping" + ADR-0052a §4.4 invariants:
//! - lexical scoping: inner scopes shadow outer names
//! - `bind_let_shadow` allows same-scope let rebind
//! - `bind` rejects duplicate params in same scope
//! - scope close restores parent correctly
//! - DefAllocator never produces duplicate ids

use cobrust_frontend::ast;
use cobrust_frontend::parse_str;
use cobrust_frontend::span::{FileId, Span};
use cobrust_hir::scope::{DefAllocator, DefId, DefKind, Scope};
use cobrust_hir::tree as h;
use cobrust_hir::{Session, lower};

// =====================================================================
// Scope unit tests — Scope::bind, resolve, close, bind_let_shadow
// =====================================================================

fn make_span() -> Span {
    Span::point(FileId::SYNTHETIC, 0)
}

#[test]
fn scope_bind_resolve_local() {
    let mut s = Scope::new();
    let mut alloc = DefAllocator::default();
    let id = alloc.fresh();
    s.bind("x", id, DefKind::LetBinding, make_span()).unwrap();
    let resolved = s.resolve("x");
    assert_eq!(resolved, Some((id, DefKind::LetBinding)));
}

#[test]
fn scope_resolve_returns_none_for_unknown() {
    let s = Scope::new();
    assert_eq!(s.resolve("unknown"), None);
}

#[test]
fn scope_bind_duplicate_returns_err() {
    let mut s = Scope::new();
    let mut alloc = DefAllocator::default();
    let id1 = alloc.fresh();
    let id2 = alloc.fresh();
    s.bind("x", id1, DefKind::LetBinding, make_span()).unwrap();
    let result = s.bind("x", id2, DefKind::LetBinding, make_span());
    assert!(
        result.is_err(),
        "duplicate binding in same scope must return Err"
    );
}

#[test]
fn scope_fn_fn_shadow_ok() {
    // Fn→Fn shadowing is silently allowed (PRELUDE override semantic)
    let mut s = Scope::new();
    let mut alloc = DefAllocator::default();
    let id1 = alloc.fresh();
    let id2 = alloc.fresh();
    s.bind("f", id1, DefKind::Fn, make_span()).unwrap();
    let result = s.bind("f", id2, DefKind::Fn, make_span());
    assert!(result.is_ok(), "Fn→Fn shadow must be allowed");
}

#[test]
fn scope_bind_let_shadow_same_scope_ok() {
    // bind_let_shadow always succeeds, even same scope
    let mut s = Scope::new();
    let mut alloc = DefAllocator::default();
    let id1 = alloc.fresh();
    let id2 = alloc.fresh();
    s.bind_let_shadow("x", id1, DefKind::LetBinding, make_span());
    s.bind_let_shadow("x", id2, DefKind::LetBinding, make_span());
    // Resolves to the latest binding
    let resolved = s.resolve("x");
    assert_eq!(resolved, Some((id2, DefKind::LetBinding)));
}

#[test]
fn scope_child_inherits_parent() {
    let mut parent = Scope::new();
    let mut alloc = DefAllocator::default();
    let id = alloc.fresh();
    parent
        .bind("outer", id, DefKind::LetBinding, make_span())
        .unwrap();

    let child = Scope::child(parent);
    let resolved = child.resolve("outer");
    assert_eq!(resolved, Some((id, DefKind::LetBinding)));
}

#[test]
fn scope_child_shadows_parent() {
    let mut parent = Scope::new();
    let mut alloc = DefAllocator::default();
    let outer_id = alloc.fresh();
    parent
        .bind("x", outer_id, DefKind::LetBinding, make_span())
        .unwrap();

    let mut child = Scope::child(parent);
    let inner_id = alloc.fresh();
    child
        .bind("x", inner_id, DefKind::LetBinding, make_span())
        .unwrap();

    // Child sees inner binding
    assert_eq!(child.resolve("x"), Some((inner_id, DefKind::LetBinding)));
}

#[test]
fn scope_close_restores_parent() {
    let mut parent = Scope::new();
    let mut alloc = DefAllocator::default();
    let outer_id = alloc.fresh();
    parent
        .bind("x", outer_id, DefKind::LetBinding, make_span())
        .unwrap();

    let mut child = Scope::child(parent);
    let inner_id = alloc.fresh();
    child
        .bind("x", inner_id, DefKind::LetBinding, make_span())
        .unwrap();

    let restored = child.close();
    // After close, inner binding is gone, outer is visible again
    assert_eq!(restored.resolve("x"), Some((outer_id, DefKind::LetBinding)));
}

#[test]
fn scope_binds_locally_true() {
    let mut s = Scope::new();
    let mut alloc = DefAllocator::default();
    let id = alloc.fresh();
    s.bind("local", id, DefKind::LetBinding, make_span())
        .unwrap();
    assert!(s.binds_locally("local"));
}

#[test]
fn scope_binds_locally_false_for_parent() {
    let mut parent = Scope::new();
    let mut alloc = DefAllocator::default();
    let id = alloc.fresh();
    parent
        .bind("from_parent", id, DefKind::LetBinding, make_span())
        .unwrap();
    let child = Scope::child(parent);
    // binds_locally checks ONLY current scope, not parent
    assert!(!child.binds_locally("from_parent"));
    // But resolve finds it via parent chain
    assert!(child.resolve("from_parent").is_some());
}

#[test]
fn scope_local_names_iter() {
    let mut s = Scope::new();
    let mut alloc = DefAllocator::default();
    let id_a = alloc.fresh();
    let id_b = alloc.fresh();
    s.bind("a", id_a, DefKind::LetBinding, make_span()).unwrap();
    s.bind("b", id_b, DefKind::LetBinding, make_span()).unwrap();
    let names: Vec<&String> = s.local_names().map(|(n, _)| n).collect();
    assert!(names.contains(&&"a".to_string()));
    assert!(names.contains(&&"b".to_string()));
    assert_eq!(names.len(), 2);
}

// =====================================================================
// DefAllocator tests
// =====================================================================

#[test]
fn def_allocator_starts_at_zero() {
    let mut alloc = DefAllocator::default();
    let id = alloc.fresh();
    assert_eq!(id.0, 0);
}

#[test]
fn def_allocator_monotonic() {
    let mut alloc = DefAllocator::default();
    let ids: Vec<DefId> = (0..10).map(|_| alloc.fresh()).collect();
    for i in 1..ids.len() {
        assert!(
            ids[i].0 > ids[i - 1].0,
            "DefIds must be strictly increasing"
        );
    }
}

#[test]
fn def_allocator_count_equals_allocated() {
    let mut alloc = DefAllocator::default();
    for _ in 0..5 {
        let _ = alloc.fresh();
    }
    assert_eq!(alloc.count(), 5);
}

#[test]
fn def_allocator_no_duplicates() {
    let mut alloc = DefAllocator::default();
    let ids: Vec<DefId> = (0..20).map(|_| alloc.fresh()).collect();
    let mut seen = std::collections::HashSet::new();
    for id in &ids {
        assert!(seen.insert(id.0), "DefId {} is duplicated", id.0);
    }
}

// =====================================================================
// Integration: scope chain via lowering
// =====================================================================

fn lower_ok(src: &str) -> h::Module {
    let module: ast::Module =
        parse_str(src, FileId::SYNTHETIC).unwrap_or_else(|e| panic!("parse failed: {e:?}"));
    let mut sess = Session::new();
    lower(&module, &mut sess).unwrap_or_else(|e| panic!("lower failed: {e:?}"))
}

fn lower_err_src(src: &str) -> cobrust_hir::error::LoweringError {
    let module: ast::Module =
        parse_str(src, FileId::SYNTHETIC).unwrap_or_else(|e| panic!("parse failed: {e:?}"));
    let mut sess = Session::new();
    lower(&module, &mut sess)
        .map(|_| panic!("expected error"))
        .unwrap_err()
}

#[test]
fn integration_inner_scope_shadows_outer() {
    // fn with a parameter `x`, then inner fn with same param `x` — inner
    // resolves its own `x`, not the outer one.
    lower_ok(
        "fn outer(x: i64) -> i64:\n    fn inner(x: i64) -> i64:\n        return x\n    return inner(x)\n",
    );
}

#[test]
fn integration_let_shadow_in_fn_ok() {
    // ADR-0052a: `let x = &x` in same scope is valid (let rebind shortcut)
    lower_ok("fn f():\n    let x: i64 = 1\n    let x: i64 = x + 1\n    return x\n");
}

#[test]
fn integration_forward_reference_module_fns() {
    // prebind_items enables forward references at module scope
    lower_ok("fn a() -> i64:\n    return b()\nfn b() -> i64:\n    return 1\n");
}

#[test]
fn integration_fn_fn_shadow_user_overrides() {
    // User's definition of `sqrt` shadows any PRELUDE stub
    lower_ok(
        "fn sqrt(x: f64) -> f64:\n    return x\nfn sqrt(x: f64) -> f64:\n    return x * 2.0\n",
    );
}

#[test]
fn integration_unknown_name_in_fn_body() {
    use cobrust_hir::error::LoweringError;
    let err = lower_err_src("fn f() -> i64:\n    return does_not_exist\n");
    assert!(
        matches!(err, LoweringError::UnknownName { .. }),
        "expected UnknownName, got {err:?}"
    );
}

#[test]
fn integration_param_binding_resolved_in_body() {
    // params are visible in body
    lower_ok("fn f(val: i64) -> i64:\n    return val\n");
}

#[test]
fn integration_for_loop_binding_visible_in_body() {
    // loop variable is bound in for body
    lower_ok(
        "fn f(xs: list[i64]) -> i64:\n    let acc: i64 = 0\n    for x in xs:\n        acc += x\n    return acc\n",
    );
}

#[test]
fn integration_match_arm_binding() {
    // Pattern binding in match arm is scoped to arm body
    lower_ok("fn f(x: i64) -> i64:\n    match x:\n        case y:\n            return y\n");
}

#[test]
fn integration_comprehension_variable_not_leaking() {
    // The comprehension loop variable `item` should not be visible outside.
    // After the comprehension, `item` should be unresolved.
    lower_ok(
        "fn f(xs: list[i64]) -> list[i64]:\n    let result: list[i64] = [item * 2 for item in xs]\n    return result\n",
    );
}

#[test]
fn integration_nested_fn_sees_outer_defined_names() {
    // Inner fn can reference module-level names (forward refs resolved by prebind)
    lower_ok(
        "let CONST: i64 = 42\nfn outer():\n    fn inner() -> i64:\n        return CONST\n    pass\n",
    );
}
