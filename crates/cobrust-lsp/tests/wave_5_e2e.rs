//! ADR-0057g §5 integration + snapshot tests for Phase J wave-5.
//!
//! 18 tests total:
//! - §5 semantic-tokens delta integration (4 tests)
//! - §5 inlayHint/resolve integration (3 tests)
//! - §5 cross-file call hierarchy integration (5 tests)
//! - §5 snapshot (6 tests)

use cobrust_frontend::span::FileId;
use cobrust_lsp::call_hierarchy::{
    build_incoming_calls_cross_file, build_outgoing_calls_cross_file,
};
use cobrust_lsp::inlay::{INLAY_DATA_KIND_PARAM, INLAY_DATA_KIND_TYPE};
use cobrust_lsp::semantic_tokens::build_semantic_tokens_delta;
use cobrust_lsp::span_convert::LineMap;
use cobrust_lsp::{build_inlay_hints, prepare_call_hierarchy, resolve_inlay_hint};
use cobrust_types::{TypeCheckCtx, check_incremental};
use serde_json::json;
use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintTooltip, Position, Range,
    SemanticTokensFullDeltaResult, Url,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn checked_ctx(source: &str) -> TypeCheckCtx {
    let ast = cobrust_frontend::parse_str(source, FileId::SYNTHETIC).expect("parse failed");
    let mut hir_sess = cobrust_hir::lower::Session::new();
    let hir = cobrust_hir::lower::lower(&ast, &mut hir_sess).expect("lower failed");
    let mut ctx = TypeCheckCtx::new();
    let _ = check_incremental(&mut ctx, &hir, 1);
    ctx
}

/// Build a TypeCheckCtx from the union of every supplied source.
///
/// Cross-file LSP scenarios reference fn names across docs (`primary`
/// calls `helper` defined in `other`). The real `Backend.session_ctx`
/// accumulates bindings across `did_open` of every open file via
/// `check_incremental(file_id=N)` per-doc. This helper simulates that
/// by concatenating sources into one HIR — every fn def reaches the
/// ctx so `prepare_call_hierarchy`'s `ctx.lookup(name)` guard
/// succeeds for any fn defined in any open doc.
fn cross_file_checked_ctx(sources: &[&str]) -> TypeCheckCtx {
    let combined: String = sources.join("\n");
    checked_ctx(&combined)
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
// §5 semantic-tokens delta integration (4 tests)
// ---------------------------------------------------------------------------

/// Test 1: first request with no previousResultId → full Tokens variant.
#[test]
fn delta_initial_request_returns_full_tokens() {
    let source = "let x: i64 = 42\n";
    let line_map = LineMap::from_source(source);
    let result =
        build_semantic_tokens_delta(source, &line_map, None, None, None, "st-1".to_string());
    match result {
        SemanticTokensFullDeltaResult::Tokens(tokens) => {
            assert_eq!(tokens.result_id.as_deref(), Some("st-1"));
            assert!(!tokens.data.is_empty(), "expected non-empty tokens");
        }
        other => panic!("expected Tokens variant; got {other:?}"),
    }
}

/// Test 2: matching previousResultId after single-token append → TokensDelta
/// with at least one edit.
#[test]
fn delta_matching_result_id_produces_delta_edits() {
    // Compute prev_tokens for the original source.
    let prev_source = "let x: i64 = 42\n";
    let prev_line_map = LineMap::from_source(prev_source);
    let prev_tokens = cobrust_lsp::build_semantic_tokens(prev_source, &prev_line_map);

    let new_source = "let x: i64 = 42\nlet y: i64 = 43\n";
    let new_line_map = LineMap::from_source(new_source);
    let result = build_semantic_tokens_delta(
        new_source,
        &new_line_map,
        Some("st-1"),
        Some("st-1"),
        Some(&prev_tokens.data),
        "st-2".to_string(),
    );
    match result {
        SemanticTokensFullDeltaResult::TokensDelta(delta) => {
            assert_eq!(delta.result_id.as_deref(), Some("st-2"));
            assert!(!delta.edits.is_empty(), "expected at least one edit");
        }
        other => panic!("expected TokensDelta variant; got {other:?}"),
    }
}

/// Test 3: matching previousResultId after multi-token rewrite → TokensDelta
/// with the diff bracketed by the longest common prefix + suffix.
#[test]
fn delta_multi_token_rewrite_produces_bracketed_diff() {
    let prev_source = "let a: i64 = 1\n";
    let prev_line_map = LineMap::from_source(prev_source);
    let prev_tokens = cobrust_lsp::build_semantic_tokens(prev_source, &prev_line_map);

    let new_source = "let alpha: str = \"x\"\n";
    let new_line_map = LineMap::from_source(new_source);
    let result = build_semantic_tokens_delta(
        new_source,
        &new_line_map,
        Some("st-prev"),
        Some("st-prev"),
        Some(&prev_tokens.data),
        "st-new".to_string(),
    );
    match result {
        SemanticTokensFullDeltaResult::TokensDelta(delta) => {
            assert_eq!(delta.result_id.as_deref(), Some("st-new"));
            // Empty edits is impossible since the token streams differ.
            assert!(!delta.edits.is_empty());
        }
        other => panic!("expected TokensDelta variant; got {other:?}"),
    }
}

/// Test 4: previousResultId out-of-sync (unknown id) → fall back to Tokens.
#[test]
fn delta_out_of_sync_result_id_falls_back_to_full() {
    let prev_source = "let x: i64 = 1\n";
    let prev_line_map = LineMap::from_source(prev_source);
    let prev_tokens = cobrust_lsp::build_semantic_tokens(prev_source, &prev_line_map);

    let new_source = "let x: i64 = 1\nlet y: i64 = 2\n";
    let new_line_map = LineMap::from_source(new_source);
    // Client claims previousResultId "stale" but our cache holds "fresh"
    // — id mismatch forces the full fallback.
    let result = build_semantic_tokens_delta(
        new_source,
        &new_line_map,
        Some("stale"),
        Some("fresh"),
        Some(&prev_tokens.data),
        "st-fallback".to_string(),
    );
    match result {
        SemanticTokensFullDeltaResult::Tokens(tokens) => {
            assert_eq!(tokens.result_id.as_deref(), Some("st-fallback"));
        }
        other => panic!("expected Tokens variant on id mismatch; got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// §5 inlayHint/resolve integration (3 tests)
// ---------------------------------------------------------------------------

/// Test 5: type-kind hint with name in ctx → resolve sets a Markdown tooltip.
#[test]
fn resolve_type_kind_sets_markdown_tooltip() {
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
    assert_eq!(hints.len(), 1);
    let resolved = resolve_inlay_hint(hints[0].clone(), &ctx);
    let tooltip = resolved.tooltip.expect("tooltip should be set");
    match tooltip {
        InlayHintTooltip::MarkupContent(markup) => {
            assert!(markup.value.contains('x'), "tooltip should mention name");
            assert!(markup.value.contains("i64"), "tooltip should include type");
        }
        InlayHintTooltip::String(_) => panic!("expected MarkupContent"),
    }
}

/// Test 6: param-kind hint with callee in ctx → resolve renders the callee
/// signature + parameter slot.
#[test]
fn resolve_param_kind_sets_callee_signature_tooltip() {
    let source = "fn add(left: i64, right: i64) -> i64:\n    return left + right\nlet a: i64 = 1\nlet b: i64 = 2\nlet r: i64 = add(a, b)\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
    let param_hint = hints
        .iter()
        .find(|h| h.kind == Some(InlayHintKind::PARAMETER))
        .expect("at least one param-name hint")
        .clone();
    let resolved = resolve_inlay_hint(param_hint, &ctx);
    let tooltip = resolved.tooltip.expect("tooltip should be set");
    match tooltip {
        InlayHintTooltip::MarkupContent(markup) => {
            assert!(
                markup.value.contains("add"),
                "tooltip should mention callee `add`"
            );
            assert!(
                markup.value.contains("Parameter"),
                "tooltip should describe the parameter slot"
            );
        }
        InlayHintTooltip::String(_) => panic!("expected MarkupContent"),
    }
}

/// Test 7: hint with absent data field → resolve returns unchanged
/// (tooltip stays None per ADR-0057g §4 honest-scope best-effort).
#[test]
fn resolve_absent_data_returns_unchanged() {
    let bare_hint = InlayHint {
        position: Position {
            line: 0,
            character: 0,
        },
        label: InlayHintLabel::String(": stub".to_string()),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: Some(false),
        padding_right: Some(false),
        data: None,
    };
    let ctx = TypeCheckCtx::new();
    let resolved = resolve_inlay_hint(bare_hint, &ctx);
    assert!(resolved.tooltip.is_none(), "no data → no tooltip");
}

// ---------------------------------------------------------------------------
// §5 cross-file call hierarchy integration (5 tests)
// ---------------------------------------------------------------------------

/// Test 8: incoming-from-other-file. Primary doc defines `add`; another
/// open doc calls `add` from its own fn body → cross-file IncomingCall
/// surfaces in the result vec.
#[test]
fn cross_file_incoming_calls_aggregates_other_doc_caller() {
    let primary = "fn add(x: i64, y: i64) -> i64:\n    return x + y\n";
    let primary_line_map = LineMap::from_source(primary);
    let primary_uri = uri("a.cb");
    let other_source = "fn other_caller() -> i64:\n    return add(1, 2)\n";
    let other_line_map = LineMap::from_source(other_source);
    let other_uri = uri("b.cb");
    // Union ctx so `add` resolves and `other_caller` lowers cleanly.
    let ctx = cross_file_checked_ctx(&[primary, other_source]);
    let pos = Position {
        line: 0,
        character: 3,
    };
    let items = prepare_call_hierarchy(primary, &primary_line_map, pos, &ctx, primary_uri.clone())
        .expect("item must be Some");
    let target = &items[0];
    let other_docs = vec![(other_uri.clone(), other_source.to_string(), other_line_map)];
    let calls = build_incoming_calls_cross_file(primary, &primary_line_map, target, &other_docs);
    // At least one IncomingCall comes from `other_caller` in `b.cb`.
    let cross = calls
        .iter()
        .find(|c| c.from.name == "other_caller")
        .expect("expected other_caller IncomingCall");
    assert_eq!(cross.from.uri, other_uri);
    assert_eq!(cross.from_ranges.len(), 1);
}

/// Test 9: outgoing-to-other-file. Primary doc's fn calls a callee
/// defined in another open doc → the OutgoingCall's `to.uri` resolves
/// to the cross-doc def location.
#[test]
fn cross_file_outgoing_calls_resolves_callee_in_other_doc() {
    let primary = "fn caller() -> i64:\n    return helper()\n";
    let primary_line_map = LineMap::from_source(primary);
    let primary_uri = uri("primary.cb");
    let other_source = "fn helper() -> i64:\n    return 7\n";
    let other_line_map = LineMap::from_source(other_source);
    let other_uri = uri("other.cb");
    // Cross-doc ctx unions both fn defs so prepare's lookup guard
    // succeeds for the primary cursor word.
    let ctx = cross_file_checked_ctx(&[primary, other_source]);
    let pos = Position {
        line: 0,
        character: 3,
    };
    let items = prepare_call_hierarchy(primary, &primary_line_map, pos, &ctx, primary_uri.clone())
        .expect("item");
    let target = &items[0];
    let other_docs = vec![(other_uri.clone(), other_source.to_string(), other_line_map)];
    let calls = build_outgoing_calls_cross_file(primary, &primary_line_map, target, &other_docs);
    let helper_call = calls
        .iter()
        .find(|c| c.to.name == "helper")
        .expect("expected helper OutgoingCall");
    assert_eq!(
        helper_call.to.uri, other_uri,
        "callee `to.uri` should resolve to the other-doc def"
    );
}

/// Test 10: both-directions. A in primary calls B in other; B in other
/// calls A in primary. Incoming AND outgoing both surface cross-file.
#[test]
fn cross_file_both_directions_surface() {
    let primary = "fn a() -> i64:\n    return b()\n";
    let primary_line_map = LineMap::from_source(primary);
    let primary_uri = uri("a.cb");
    let other_source = "fn b() -> i64:\n    return a()\n";
    let other_line_map = LineMap::from_source(other_source);
    let other_uri = uri("b.cb");
    let ctx = cross_file_checked_ctx(&[primary, other_source]);
    let pos = Position {
        line: 0,
        character: 3,
    };
    let items = prepare_call_hierarchy(primary, &primary_line_map, pos, &ctx, primary_uri.clone())
        .expect("item");
    let target = &items[0];
    let other_docs = vec![(other_uri.clone(), other_source.to_string(), other_line_map)];

    let incoming = build_incoming_calls_cross_file(primary, &primary_line_map, target, &other_docs);
    let outgoing = build_outgoing_calls_cross_file(primary, &primary_line_map, target, &other_docs);

    assert!(
        incoming
            .iter()
            .any(|c| c.from.name == "b" && c.from.uri == other_uri),
        "expected cross-file incoming from b in other_uri; got {incoming:?}"
    );
    assert!(
        outgoing
            .iter()
            .any(|c| c.to.name == "b" && c.to.uri == other_uri),
        "expected cross-file outgoing to b in other_uri; got {outgoing:?}"
    );
}

/// Test 11: no-cross-file-match. Callee not defined as a fn in any
/// open doc → no cross-file resolution; the placeholder stays.
#[test]
fn cross_file_no_match_keeps_placeholder() {
    // Bind `ghost` as a closure-typed let so HIR lowering succeeds but
    // there is NO `fn ghost` def anywhere. The cross-file outgoing
    // walker can't resolve `ghost` to a fn-def location in any open
    // doc → placeholder retained.
    let primary = "let ghost: (i64) -> i64 = id\nfn solo() -> i64:\n    return ghost(1)\nfn id(n: i64) -> i64:\n    return n\n";
    let primary_line_map = LineMap::from_source(primary);
    let primary_uri = uri("solo.cb");
    let other_source = "fn unrelated() -> i64:\n    return 0\n";
    let other_line_map = LineMap::from_source(other_source);
    let other_uri = uri("unrelated.cb");
    let ctx = cross_file_checked_ctx(&[primary, other_source]);
    // Cursor on `solo` def-name at line 1, char 3.
    let pos = Position {
        line: 1,
        character: 3,
    };
    let items = prepare_call_hierarchy(primary, &primary_line_map, pos, &ctx, primary_uri.clone())
        .expect("item");
    let target = &items[0];
    let other_docs = vec![(other_uri, other_source.to_string(), other_line_map)];
    let calls = build_outgoing_calls_cross_file(primary, &primary_line_map, target, &other_docs);
    let ghost_call = calls
        .iter()
        .find(|c| c.to.name == "ghost")
        .expect("expected ghost OutgoingCall");
    // Placeholder retained — to.range stays at the default zero
    // position because no fn def for `ghost` exists in any open doc.
    assert_eq!(
        ghost_call.to.range,
        Range {
            start: Position {
                line: 0,
                character: 0
            },
            end: Position {
                line: 0,
                character: 0
            },
        },
        "ghost has no fn def in any doc → placeholder retained"
    );
}

/// Test 12: cycle-detection. A calls B, B calls A — both incoming and
/// outgoing walks terminate; A appears exactly once as a caller of B.
#[test]
fn cross_file_cycle_no_infinite_walk() {
    // Same shape as test 10 but we assert termination + dedup.
    let primary = "fn a() -> i64:\n    return b()\n";
    let primary_line_map = LineMap::from_source(primary);
    let primary_uri = uri("a.cb");
    let other_source = "fn b() -> i64:\n    return a()\n";
    let other_line_map = LineMap::from_source(other_source);
    let other_uri = uri("b.cb");
    let ctx = cross_file_checked_ctx(&[primary, other_source]);
    let pos = Position {
        line: 0,
        character: 3,
    };
    let items = prepare_call_hierarchy(primary, &primary_line_map, pos, &ctx, primary_uri.clone())
        .expect("item");
    let target = &items[0];
    let other_docs = vec![(other_uri.clone(), other_source.to_string(), other_line_map)];

    let incoming = build_incoming_calls_cross_file(primary, &primary_line_map, target, &other_docs);
    // Filter to the `b` caller in other_uri; expect exactly one entry
    // (no duplicate from cycle re-entry).
    let b_callers: Vec<_> = incoming
        .iter()
        .filter(|c| c.from.name == "b" && c.from.uri == other_uri)
        .collect();
    assert_eq!(
        b_callers.len(),
        1,
        "expected exactly one b-caller entry; got {}",
        b_callers.len()
    );
}

// ---------------------------------------------------------------------------
// §5 snapshot tests (6 tests)
// ---------------------------------------------------------------------------

/// Test 13: snapshot — first-response Tokens variant shape.
#[test]
fn snapshot_delta_first_response_tokens() {
    let source = "let x: i64 = 42\n";
    let line_map = LineMap::from_source(source);
    let result =
        build_semantic_tokens_delta(source, &line_map, None, None, None, "st-snap-1".to_string());
    insta::assert_json_snapshot!(result);
}

/// Test 14: snapshot — delta-response TokensDelta variant shape with at
/// least one edit.
#[test]
fn snapshot_delta_response_with_edits() {
    let prev_source = "let x: i64 = 1\n";
    let prev_line_map = LineMap::from_source(prev_source);
    let prev_tokens = cobrust_lsp::build_semantic_tokens(prev_source, &prev_line_map);
    let new_source = "let x: i64 = 2\n";
    let new_line_map = LineMap::from_source(new_source);
    let result = build_semantic_tokens_delta(
        new_source,
        &new_line_map,
        Some("st-snap-2"),
        Some("st-snap-2"),
        Some(&prev_tokens.data),
        "st-snap-2b".to_string(),
    );
    insta::assert_json_snapshot!(result);
}

/// Test 15: snapshot — resolved type hint with tooltip shape.
#[test]
fn snapshot_resolve_type_hint_with_tooltip() {
    let source = "let y = 100\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
    let resolved: Vec<InlayHint> = hints
        .into_iter()
        .map(|h| resolve_inlay_hint(h, &ctx))
        .collect();
    insta::assert_json_snapshot!(resolved);
}

/// Test 16: snapshot — resolved param hint with tooltip shape.
#[test]
fn snapshot_resolve_param_hint_with_tooltip() {
    let source = "fn pair(first: i64, second: i64) -> i64:\n    return first + second\nlet a: i64 = 10\nlet b: i64 = 20\nlet r: i64 = pair(a, b)\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
    let resolved: Vec<InlayHint> = hints
        .into_iter()
        .filter(|h| h.kind == Some(InlayHintKind::PARAMETER))
        .map(|h| resolve_inlay_hint(h, &ctx))
        .collect();
    insta::assert_json_snapshot!(resolved);
}

/// Test 17: snapshot — IncomingCall vec shape across 2 files.
#[test]
fn snapshot_cross_file_incoming_calls_two_uris() {
    let primary = "fn target() -> i64:\n    return 1\n";
    let primary_line_map = LineMap::from_source(primary);
    let primary_uri = uri("primary.cb");
    let other_source = "fn ext_caller() -> i64:\n    return target()\n";
    let other_line_map = LineMap::from_source(other_source);
    let other_uri = uri("other.cb");
    let ctx = cross_file_checked_ctx(&[primary, other_source]);
    let pos = Position {
        line: 0,
        character: 3,
    };
    let items = prepare_call_hierarchy(primary, &primary_line_map, pos, &ctx, primary_uri.clone())
        .expect("item");
    let target = &items[0];
    let other_docs = vec![(other_uri.clone(), other_source.to_string(), other_line_map)];
    let mut calls =
        build_incoming_calls_cross_file(primary, &primary_line_map, target, &other_docs);
    calls.sort_by(|a, b| a.from.name.cmp(&b.from.name));
    insta::assert_json_snapshot!(calls);
}

/// Test 18: snapshot — OutgoingCall vec shape with mixed same-doc + cross-doc.
#[test]
fn snapshot_cross_file_outgoing_calls_mixed() {
    let primary = "fn helper_local() -> i64:\n    return 1\nfn root() -> i64:\n    return helper_local() + helper_remote()\n";
    let primary_line_map = LineMap::from_source(primary);
    let primary_uri = uri("primary.cb");
    let other_source = "fn helper_remote() -> i64:\n    return 99\n";
    let other_line_map = LineMap::from_source(other_source);
    let other_uri = uri("other.cb");
    let ctx = cross_file_checked_ctx(&[primary, other_source]);
    // Cursor on `root` (line 2 char 3).
    let pos = Position {
        line: 2,
        character: 3,
    };
    let items = prepare_call_hierarchy(primary, &primary_line_map, pos, &ctx, primary_uri.clone())
        .expect("item");
    let target = &items[0];
    let other_docs = vec![(other_uri, other_source.to_string(), other_line_map)];
    let mut calls =
        build_outgoing_calls_cross_file(primary, &primary_line_map, target, &other_docs);
    calls.sort_by(|a, b| a.to.name.cmp(&b.to.name));
    insta::assert_json_snapshot!(calls);
}

// ---------------------------------------------------------------------------
// Const-consumers (silence unused-import linter; surface the data-kind
// constants in tests so a future regression that breaks them flags here).
// ---------------------------------------------------------------------------

#[test]
fn data_kind_constants_match_expected_values() {
    assert_eq!(INLAY_DATA_KIND_TYPE, "type");
    assert_eq!(INLAY_DATA_KIND_PARAM, "param");
    // Touch json! to keep the import live even if a refactor drops it.
    let _ = json!({"kind": INLAY_DATA_KIND_TYPE});
}
