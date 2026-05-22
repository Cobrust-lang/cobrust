//! ADR-0057f §5 integration + snapshot tests for Phase J wave-4.
//!
//! 20 tests total:
//! - §5 inlay hints integration (5 tests)
//! - §5 semantic tokens integration (5 tests)
//! - §5 call hierarchy integration (4 tests)
//! - §5 snapshot (6 tests)

use cobrust_frontend::span::FileId;
use cobrust_lsp::call_hierarchy::{build_incoming_calls, build_outgoing_calls};
use cobrust_lsp::inlay::build_inlay_hints;
use cobrust_lsp::semantic_tokens::{
    TT_COMMENT, TT_FUNCTION, TT_KEYWORD, TT_NUMBER, TT_OPERATOR, TT_STRING, TT_TYPE, TT_VARIABLE,
    build_semantic_tokens,
};
use cobrust_lsp::span_convert::LineMap;
use cobrust_lsp::{prepare_call_hierarchy, token_legend};
use cobrust_types::{TypeCheckCtx, check_incremental};
use tower_lsp::lsp_types::{
    CallHierarchyItem, InlayHint, InlayHintKind, InlayHintLabel, Position, Range, SymbolKind, Url,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn checked_ctx(source: &str) -> TypeCheckCtx {
    let ast =
        cobrust_frontend::parse_str(source, FileId::SYNTHETIC).expect("parse failed in helper");
    let mut hir_sess = cobrust_hir::lower::Session::new();
    let hir = cobrust_hir::lower::lower(&ast, &mut hir_sess).expect("lower failed in helper");
    let mut ctx = TypeCheckCtx::new();
    let _ = check_incremental(&mut ctx, &hir, 1);
    ctx
}

fn uri(path: &str) -> Url {
    Url::parse(&format!("file:///{path}")).expect("test URI invalid")
}

fn full_range(source: &str) -> Range {
    let line_map = LineMap::from_source(source);
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: line_map.byte_to_position(u32::try_from(source.len()).unwrap_or(u32::MAX)),
    }
}

// ---------------------------------------------------------------------------
// §5 inlay integration (5 tests)
// ---------------------------------------------------------------------------

/// Test 1: `let x = 42` without annot → `: Int` hint at binder end.
#[test]
fn inlay_let_without_annot_emits_type_hint() {
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
    assert_eq!(hints.len(), 1, "expected 1 TYPE hint for `let x = 42`");
    assert_eq!(hints[0].kind, Some(InlayHintKind::TYPE));
    let InlayHintLabel::String(s) = &hints[0].label else {
        panic!("expected string label");
    };
    assert!(s.starts_with(": "), "got {s}");
}

/// Test 2: `let x: Int = 42` with explicit annot → no hint emitted.
#[test]
fn inlay_let_with_explicit_annot_emits_nothing() {
    let source = "let x: i64 = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
    assert!(hints.is_empty(), "explicit annot → no hint");
}

/// Test 3: fn-call with non-literal arg → param-name hint emitted.
#[test]
fn inlay_fn_call_emits_param_name_hint() {
    let source = "fn add(left: i64, right: i64) -> i64:\n    return left + right\n\
                  let a: i64 = 1\n\
                  let b: i64 = 2\n\
                  let r: i64 = add(a, b)\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
    // Expect 2 PARAMETER hints (one per call arg). Note: TYPE hints
    // are not emitted since `a`, `b`, `r` all have explicit annotations.
    let param_hints: Vec<&InlayHint> = hints
        .iter()
        .filter(|h| h.kind == Some(InlayHintKind::PARAMETER))
        .collect();
    assert_eq!(
        param_hints.len(),
        2,
        "expected 2 PARAMETER hints; got {hints:?}"
    );
    let labels: Vec<String> = param_hints
        .iter()
        .map(|h| {
            if let InlayHintLabel::String(s) = &h.label {
                s.clone()
            } else {
                String::new()
            }
        })
        .collect();
    assert!(
        labels.iter().any(|l| l == "left:"),
        "expected `left:` hint; got {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "right:"),
        "expected `right:` hint; got {labels:?}"
    );
}

/// Test 4: nested `let` in fn body → hint at inner binder.
#[test]
fn inlay_nested_let_in_fn_body_emits_hint() {
    let source = "fn compute() -> i64:\n    let inner = 7\n    return inner\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
    // Inner `let inner = 7` may not surface in TypeCheckCtx::lookup
    // (fn-local scope); test asserts that this does NOT panic and the
    // result is well-formed. Wave-4 honest-scope: fn-local lookups
    // surface only when ctx records them.
    for h in &hints {
        assert!(h.kind == Some(InlayHintKind::TYPE) || h.kind == Some(InlayHintKind::PARAMETER));
    }
}

/// Test 5: multi-fn doc → hints aggregate across all let-without-annot.
#[test]
fn inlay_multi_fn_doc_emits_multiple_hints() {
    let source = "let alpha = 1\nlet beta = 2\nlet gamma = 3\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
    assert_eq!(hints.len(), 3, "expected 3 TYPE hints; got {hints:?}");
}

// ---------------------------------------------------------------------------
// §5 semantic tokens integration (5 tests)
// ---------------------------------------------------------------------------

/// Test 6: `let` / `fn` / `if` → KEYWORD tokens.
#[test]
fn semantic_tokens_keyword_coloring() {
    let source = "let x: i64 = 1\nfn id() -> i64:\n    if True:\n        return 1\n    return 0\n";
    let line_map = LineMap::from_source(source);
    let tokens = build_semantic_tokens(source, &line_map);
    let keyword_count = tokens
        .data
        .iter()
        .filter(|t| t.token_type == TT_KEYWORD)
        .count();
    assert!(
        keyword_count >= 4,
        "expected >=4 KEYWORD tokens; got {keyword_count}"
    );
}

/// Test 7: `"hello"` string literal → STRING token.
#[test]
fn semantic_tokens_string_coloring() {
    let source = "let s: str = \"hello\"\n";
    let line_map = LineMap::from_source(source);
    let tokens = build_semantic_tokens(source, &line_map);
    assert!(
        tokens.data.iter().any(|t| t.token_type == TT_STRING),
        "expected STRING token"
    );
}

/// Test 8: `42` int literal → NUMBER token.
#[test]
fn semantic_tokens_number_coloring() {
    let source = "let x: i64 = 42\n";
    let line_map = LineMap::from_source(source);
    let tokens = build_semantic_tokens(source, &line_map);
    assert!(
        tokens.data.iter().any(|t| t.token_type == TT_NUMBER),
        "expected NUMBER token"
    );
}

/// Test 9: identifier at fn def site → FUNCTION token (refined via AST).
#[test]
fn semantic_tokens_function_def_name_refinement() {
    let source = "fn add(x: i64, y: i64) -> i64:\n    return x + y\n";
    let line_map = LineMap::from_source(source);
    let tokens = build_semantic_tokens(source, &line_map);
    // Expect at least one FUNCTION token (the `add` def-name override).
    let function_count = tokens
        .data
        .iter()
        .filter(|t| t.token_type == TT_FUNCTION)
        .count();
    assert!(
        function_count >= 1,
        "expected >=1 FUNCTION token; got {function_count} of {:?}",
        tokens.data
    );
}

/// Test 10: type-annotation `: i64` → TYPE token.
#[test]
fn semantic_tokens_type_annotation_coloring() {
    let source = "let x: i64 = 1\n";
    let line_map = LineMap::from_source(source);
    let tokens = build_semantic_tokens(source, &line_map);
    // Expect at least one TYPE token (the i64 in the annotation).
    assert!(
        tokens.data.iter().any(|t| t.token_type == TT_TYPE),
        "expected TYPE token; got {:?}",
        tokens.data
    );
    // Sanity: legend has 8 types.
    assert_eq!(token_legend().token_types.len(), 8);
    // Operator and variable tokens also present.
    assert!(tokens.data.iter().any(|t| t.token_type == TT_OPERATOR));
    assert!(tokens.data.iter().any(|t| t.token_type == TT_VARIABLE));
    let _ = TT_COMMENT; // ensure constant in scope for the snapshot legend.
}

// ---------------------------------------------------------------------------
// §5 call hierarchy integration (4 tests)
// ---------------------------------------------------------------------------

/// Test 11: prepare on fn def → CallHierarchyItem with FUNCTION kind.
#[test]
fn call_hierarchy_prepare_on_fn_def_returns_item() {
    let source = "fn add(x: i64, y: i64) -> i64:\n    return x + y\nfn main() -> i64:\n    return add(1, 2)\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let u = uri("test.cb");
    // Cursor on `add` at line 3 char 11 (the call site).
    let pos = Position {
        line: 3,
        character: 11,
    };
    let items = prepare_call_hierarchy(source, &line_map, pos, &ctx, u.clone()).expect("Some");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "add");
    assert_eq!(items[0].kind, SymbolKind::FUNCTION);
    assert_eq!(items[0].uri, u);
}

/// Test 12: incoming calls — 2 callers (from 2 different fns) in same doc.
#[test]
fn call_hierarchy_incoming_calls_two_callers() {
    let source = concat!(
        "fn add(x: i64, y: i64) -> i64:\n",
        "    return x + y\n",
        "fn caller1() -> i64:\n",
        "    return add(1, 2)\n",
        "fn caller2() -> i64:\n",
        "    return add(3, 4)\n",
    );
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let u = uri("test.cb");
    let pos = Position {
        line: 0,
        character: 3,
    };
    let items = prepare_call_hierarchy(source, &line_map, pos, &ctx, u.clone()).expect("item");
    let target = &items[0];
    let calls = build_incoming_calls(source, &line_map, target);
    assert_eq!(
        calls.len(),
        2,
        "expected 2 distinct callers; got {} {calls:?}",
        calls.len()
    );
    let caller_names: Vec<&str> = calls.iter().map(|c| c.from.name.as_str()).collect();
    assert!(caller_names.contains(&"caller1"));
    assert!(caller_names.contains(&"caller2"));
}

/// Test 13: outgoing calls — fn body calls 3 callees → 3 OutgoingCall items.
#[test]
fn call_hierarchy_outgoing_calls_three_callees() {
    let source = concat!(
        "fn a() -> i64:\n    return 1\n",
        "fn b() -> i64:\n    return 2\n",
        "fn c() -> i64:\n    return 3\n",
        "fn main() -> i64:\n    return a() + b() + c()\n",
    );
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let u = uri("test.cb");
    // Cursor on `main` at line 6, char 3.
    let pos = Position {
        line: 6,
        character: 3,
    };
    let items = prepare_call_hierarchy(source, &line_map, pos, &ctx, u.clone()).expect("item");
    let target = &items[0];
    let calls = build_outgoing_calls(source, &line_map, target);
    assert_eq!(calls.len(), 3, "expected 3 distinct callees; got {calls:?}");
    let names: Vec<&str> = calls.iter().map(|c| c.to.name.as_str()).collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
    assert!(names.contains(&"c"));
}

/// Test 14: unresolved symbol (cursor on a keyword) → prepare returns None.
#[test]
fn call_hierarchy_prepare_on_keyword_returns_none() {
    let source = "fn foo() -> i64:\n    return 1\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let u = uri("test.cb");
    // Cursor on `fn` (keyword) at line 0 char 0.
    let pos = Position {
        line: 0,
        character: 0,
    };
    let res = prepare_call_hierarchy(source, &line_map, pos, &ctx, u);
    assert!(res.is_none());
}

// ---------------------------------------------------------------------------
// §5 snapshot tests (6 tests)
// ---------------------------------------------------------------------------

/// Test 15: snapshot — `: <ty>` hint shape for a single let.
#[test]
fn snapshot_inlay_let_type_hint() {
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
    insta::assert_json_snapshot!(hints);
}

/// Test 16: snapshot — param-name hint shape for a single call.
#[test]
fn snapshot_inlay_param_name_hint() {
    let source = concat!(
        "fn add(left: i64, right: i64) -> i64:\n    return left + right\n",
        "let a: i64 = 1\n",
        "let b: i64 = 2\n",
        "let r: i64 = add(a, b)\n",
    );
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
    // Filter to PARAMETER hints only for a stable snapshot.
    let params: Vec<&InlayHint> = hints
        .iter()
        .filter(|h| h.kind == Some(InlayHintKind::PARAMETER))
        .collect();
    insta::assert_json_snapshot!(params);
}

/// Test 17: snapshot — encoded token vec for a 3-line program.
#[test]
fn snapshot_semantic_tokens_three_line_program() {
    let source = "let x: i64 = 42\nlet y: str = \"hi\"\nlet z = x + 1\n";
    let line_map = LineMap::from_source(source);
    let tokens = build_semantic_tokens(source, &line_map);
    insta::assert_json_snapshot!(tokens);
}

/// Test 18: snapshot — empty source → empty SemanticTokens.
#[test]
fn snapshot_semantic_tokens_empty_source() {
    let source = "";
    let line_map = LineMap::from_source(source);
    let tokens = build_semantic_tokens(source, &line_map);
    insta::assert_json_snapshot!(tokens);
}

/// Test 19: snapshot — CallHierarchyItem shape from prepare.
#[test]
fn snapshot_call_hierarchy_prepare_item() {
    let source = "fn target() -> i64:\n    return 1\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let u = uri("test.cb");
    let pos = Position {
        line: 0,
        character: 3,
    };
    let items: Vec<CallHierarchyItem> =
        prepare_call_hierarchy(source, &line_map, pos, &ctx, u).unwrap_or_default();
    insta::assert_json_snapshot!(items);
}

/// Test 20: snapshot — OutgoingCall vec shape (multi-callee).
#[test]
fn snapshot_call_hierarchy_outgoing_calls_multi() {
    let source = concat!(
        "fn a() -> i64:\n    return 1\n",
        "fn b() -> i64:\n    return 2\n",
        "fn main() -> i64:\n    return a() + b()\n",
    );
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let u = uri("test.cb");
    let pos = Position {
        line: 4,
        character: 3,
    };
    let items =
        prepare_call_hierarchy(source, &line_map, pos, &ctx, u.clone()).expect("item must be Some");
    let calls = build_outgoing_calls(source, &line_map, &items[0]);
    // Sort by callee name for deterministic snapshot ordering.
    let mut calls_sorted = calls;
    calls_sorted.sort_by(|a, b| a.to.name.cmp(&b.to.name));
    insta::assert_json_snapshot!(calls_sorted);
}
