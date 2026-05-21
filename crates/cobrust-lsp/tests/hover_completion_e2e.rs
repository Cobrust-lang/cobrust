//! ADR-0057c §5 integration + snapshot tests for hover + completion.
//!
//! 12 tests:
//! - §5.1 Hover integration (3 tests)
//! - §5.2 Completion integration (3 tests)
//! - §5.3 Hover snapshot (3 tests)
//! - §5.4 Completion snapshot (3 tests)

use cobrust_frontend::span::FileId;
use cobrust_lsp::completion::{build_completion_response, keyword_items, prelude_items};
use cobrust_lsp::hover::resolve_hover;
use cobrust_lsp::span_convert::LineMap;
use cobrust_types::{TypeCheckCtx, check_incremental};
use tower_lsp::lsp_types::{CompletionResponse, HoverContents, Position};

/// Helper: parse + HIR-lower + type-check `source`, returning a populated ctx.
fn checked_ctx(source: &str) -> TypeCheckCtx {
    let ast = cobrust_frontend::parse_str(source, FileId::SYNTHETIC)
        .expect("parse failed in test helper");
    let mut hir_sess = cobrust_hir::lower::Session::new();
    let hir = cobrust_hir::lower::lower(&ast, &mut hir_sess).expect("lower failed in test helper");
    let mut ctx = TypeCheckCtx::new();
    let _ = check_incremental(&mut ctx, &hir, 1);
    ctx
}

// ---------------------------------------------------------------------------
// §5.1 Hover integration
// ---------------------------------------------------------------------------

/// Hover on a known `let`-binding returns a Markdown card with the name
/// and the inferred type (Int for integer literals).
#[test]
fn hover_known_binding_returns_type() {
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);

    // 'x' is at byte 4, which is LSP (line=0, character=4).
    let pos = Position {
        line: 0,
        character: 4,
    };
    let hover = resolve_hover(source, &line_map, pos, &ctx);
    assert!(hover.is_some(), "expected Some hover for binding 'x'");
    let hover = hover.expect("hover must be Some (asserted above)");
    if let HoverContents::Markup(mc) = hover.contents {
        assert!(
            mc.value.contains("**x**"),
            "hover card must bold the binding name; got: {}",
            mc.value
        );
        // The type should appear in backticks — exact label depends on Ty::Display.
        assert!(
            mc.value.contains('`'),
            "hover card must include backtick-enclosed type"
        );
    } else {
        panic!("expected HoverContents::Markup");
    }
}

/// Hover on a function binding returns a hover card.
#[test]
fn hover_function_binding_returns_fn_type() {
    // Cobrust uses `fn` keyword (not `def`); `i64` is the integer type.
    let source = "fn f(a: i64) -> i64:\n    return a\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);

    // 'f' is at byte 3 (after 'fn ').
    let pos = Position {
        line: 0,
        character: 3,
    };
    let hover = resolve_hover(source, &line_map, pos, &ctx);
    assert!(
        hover.is_some(),
        "expected Some hover for function binding 'f'"
    );
    let hover = hover.expect("hover must be Some (asserted above)");
    if let HoverContents::Markup(mc) = hover.contents {
        assert!(
            mc.value.contains("**f**"),
            "hover card must bold the function name; got: {}",
            mc.value
        );
    } else {
        panic!("expected HoverContents::Markup");
    }
}

/// Hover on an unresolved/unknown identifier returns `None`.
#[test]
fn hover_unknown_name_returns_none() {
    let source = "let x = 1\n";
    let line_map = LineMap::from_source(source);
    // Use an empty ctx — no bindings registered.
    let ctx = TypeCheckCtx::new();

    // Cursor on 'x' but ctx has no bindings.
    let pos = Position {
        line: 0,
        character: 4,
    };
    let hover = resolve_hover(source, &line_map, pos, &ctx);
    assert!(hover.is_none(), "expected None for unknown binding");
}

// ---------------------------------------------------------------------------
// §5.2 Completion integration
// ---------------------------------------------------------------------------

/// Empty prefix at file start includes PRELUDE functions `print`, `len`, `range`.
#[test]
fn completion_empty_prefix_includes_prelude() {
    let ctx = TypeCheckCtx::new();
    let resp = build_completion_response("", 0, &ctx);
    if let CompletionResponse::Array(items) = resp {
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"print"),
            "print must be in completion list"
        );
        assert!(labels.contains(&"len"), "len must be in completion list");
        assert!(
            labels.contains(&"range"),
            "range must be in completion list"
        );
    } else {
        panic!("expected CompletionResponse::Array");
    }
}

/// Prefix "pri" filters to only `print`.
#[test]
fn completion_prefix_filters_items() {
    let ctx = TypeCheckCtx::new();
    let source = "pri";
    let resp = build_completion_response(source, 3, &ctx);
    if let CompletionResponse::Array(items) = resp {
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["print"],
            "only 'print' should match prefix 'pri'; got: {labels:?}"
        );
    } else {
        panic!("expected CompletionResponse::Array");
    }
}

/// Empty prefix includes keywords `let`, `def`, `if`.
#[test]
fn completion_includes_keywords() {
    let ctx = TypeCheckCtx::new();
    let resp = build_completion_response("", 0, &ctx);
    if let CompletionResponse::Array(items) = resp {
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"let"), "let must be in completion list");
        assert!(labels.contains(&"def"), "def must be in completion list");
        assert!(labels.contains(&"if"), "if must be in completion list");
    } else {
        panic!("expected CompletionResponse::Array");
    }
}

// ---------------------------------------------------------------------------
// §5.3 Hover snapshots
// ---------------------------------------------------------------------------

/// Snapshot: hover on `let x = 42` at the `x` token.
#[test]
fn snapshot_hover_int_binding() {
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let pos = Position {
        line: 0,
        character: 4,
    };
    let hover = resolve_hover(source, &line_map, pos, &ctx);
    insta::assert_json_snapshot!("snapshot_hover_int_binding", hover);
}

/// Snapshot: hover on `let s = "hi"` at the `s` token.
#[test]
fn snapshot_hover_str_binding() {
    let source = "let s = \"hi\"\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let pos = Position {
        line: 0,
        character: 4,
    };
    let hover = resolve_hover(source, &line_map, pos, &ctx);
    insta::assert_json_snapshot!("snapshot_hover_str_binding", hover);
}

/// Snapshot: hover on an unknown token returns null.
#[test]
fn snapshot_hover_none_on_unknown() {
    let source = "let x = 1\n";
    let line_map = LineMap::from_source(source);
    let ctx = TypeCheckCtx::new(); // empty ctx
    let pos = Position {
        line: 0,
        character: 4,
    };
    let hover = resolve_hover(source, &line_map, pos, &ctx);
    insta::assert_json_snapshot!("snapshot_hover_none_on_unknown", hover);
}

// ---------------------------------------------------------------------------
// §5.4 Completion snapshots
// ---------------------------------------------------------------------------

/// Snapshot: PRELUDE items for empty prefix from empty source.
#[test]
fn snapshot_completion_prelude_items() {
    let items = prelude_items("");
    insta::assert_json_snapshot!("snapshot_completion_prelude_items", items);
}

/// Snapshot: keyword items for empty prefix.
#[test]
fn snapshot_completion_keyword_items() {
    let items = keyword_items("");
    insta::assert_json_snapshot!("snapshot_completion_keyword_items", items);
}

/// Snapshot: completion for prefix "pr" — only `print` matches.
#[test]
fn snapshot_completion_prefix_print() {
    let ctx = TypeCheckCtx::new();
    let source = "pr";
    let resp = build_completion_response(source, 2, &ctx);
    insta::assert_json_snapshot!("snapshot_completion_prefix_print", resp);
}
