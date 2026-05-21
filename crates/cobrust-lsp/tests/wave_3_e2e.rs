//! ADR-0057e §5 integration + snapshot tests for Phase J wave-3.
//!
//! 15 tests total:
//! - §5 goto-def integration (3 tests)
//! - §5 codeAction integration (3 tests)
//! - §5 cross-file rename integration (3 tests)
//! - §5 snapshot (6 tests)

use std::collections::HashMap;

use cobrust_frontend::span::FileId;
use cobrust_lsp::code_action::build_code_actions;
use cobrust_lsp::diagnostic::DIAG_DATA_FIX_SAFETY_KEY;
use cobrust_lsp::goto_def::resolve_definition;
use cobrust_lsp::rename::rename_symbol_cross_file;
use cobrust_lsp::span_convert::LineMap;
use cobrust_types::{TypeCheckCtx, check_incremental};
use serde_json::json;
use tower_lsp::lsp_types::{
    CodeActionOrCommand, Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity,
    GotoDefinitionResponse, Location, NumberOrString, Position, Range, Url,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn checked_ctx(source: &str) -> TypeCheckCtx {
    let ast = cobrust_frontend::parse_str(source, FileId::SYNTHETIC)
        .expect("parse failed in test helper");
    let mut hir_sess = cobrust_hir::lower::Session::new();
    let hir = cobrust_hir::lower::lower(&ast, &mut hir_sess).expect("lower failed in test helper");
    let mut ctx = TypeCheckCtx::new();
    let _ = check_incremental(&mut ctx, &hir, 1);
    ctx
}

fn uri(path: &str) -> Url {
    Url::parse(&format!("file:///{path}")).expect("test URI invalid")
}

fn range_at(line: u32, char_start: u32, char_end: u32) -> Range {
    Range {
        start: Position {
            line,
            character: char_start,
        },
        end: Position {
            line,
            character: char_end,
        },
    }
}

/// Build a synthetic Diagnostic with a FixSafety tier code stamped into
/// `data` and a related-information suggestion. Used by codeAction
/// tests to exercise tier gating without re-running the full pipeline.
fn synthetic_diagnostic(
    message: &str,
    range: Range,
    tier_code: u8,
    suggestion: &str,
) -> Diagnostic {
    let placeholder_uri = Url::parse("cobrust://synthetic").expect("static URL parses");
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String("synthetic".into())),
        code_description: None,
        source: Some("cobrust".to_string()),
        message: message.to_string(),
        related_information: Some(vec![DiagnosticRelatedInformation {
            location: Location {
                uri: placeholder_uri,
                range,
            },
            message: suggestion.to_string(),
        }]),
        tags: None,
        data: Some(json!({ DIAG_DATA_FIX_SAFETY_KEY: tier_code })),
    }
}

// ---------------------------------------------------------------------------
// §5 goto-def integration (3 tests)
// ---------------------------------------------------------------------------

/// Test 1: cursor on use-site of local var returns Location of def-site.
#[test]
fn goto_def_local_var_returns_def_site() {
    let source = "let x = 42\nx + 1\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let u = uri("test.cb");

    // Cursor on 'x' at line 1 (the use), character 0.
    let pos = Position {
        line: 1,
        character: 0,
    };
    let result = resolve_definition(source, &line_map, pos, &ctx, u.clone());
    assert!(result.is_some(), "expected def for known binding 'x'");
    match result.expect("checked Some above") {
        GotoDefinitionResponse::Scalar(loc) => {
            assert_eq!(loc.uri, u);
            // Def-site is at line 0, char 4 ('let x' → 'x' at 4).
            assert_eq!(loc.range.start.line, 0);
            assert_eq!(loc.range.start.character, 4);
            assert_eq!(loc.range.end.character, 5);
        }
        other => panic!("expected Scalar Location response, got {other:?}"),
    }
}

/// Test 2: cursor on call-site of function returns Location of fn-def-site.
#[test]
fn goto_def_function_returns_def_site() {
    // Function def + a call site referencing it.
    let source =
        "fn double(x: i64) -> i64:\n    return (x + x)\nfn main() -> i64:\n    return double(3)\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let u = uri("test.cb");

    // Cursor on 'double' at the call site on line 3 ("    return double(3)").
    // "    return double" — 'd' of 'double' at character 11.
    let pos = Position {
        line: 3,
        character: 11,
    };
    let result = resolve_definition(source, &line_map, pos, &ctx, u.clone());
    assert!(result.is_some(), "expected def for known fn 'double'");
    match result.expect("checked Some above") {
        GotoDefinitionResponse::Scalar(loc) => {
            assert_eq!(loc.uri, u);
            // Def-site at line 0: "fn double(...)" → 'd' of 'double' at char 3.
            assert_eq!(loc.range.start.line, 0);
            assert_eq!(loc.range.start.character, 3);
        }
        other => panic!("expected Scalar Location, got {other:?}"),
    }
}

/// Test 3: cursor on unresolved symbol returns None.
#[test]
fn goto_def_unresolved_symbol_returns_none() {
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let u = uri("test.cb");

    // Cursor in whitespace (line 0, char 3 between 'let' and 'x').
    let pos_space = Position {
        line: 0,
        character: 3,
    };
    assert!(resolve_definition(source, &line_map, pos_space, &ctx, u.clone()).is_none());

    // Cursor on a keyword ('let' at char 0).
    let pos_kw = Position {
        line: 0,
        character: 0,
    };
    assert!(resolve_definition(source, &line_map, pos_kw, &ctx, u).is_none());
}

// ---------------------------------------------------------------------------
// §5 codeAction integration (3 tests)
// ---------------------------------------------------------------------------

/// Test 4: BehaviorPreserving suggestion → CodeAction with WorkspaceEdit.
#[test]
fn code_action_behavior_preserving_emits_workspace_edit() {
    let u = uri("test.cb");
    let r = range_at(0, 3, 4);
    // Tier code 1 = BehaviorPreserving.
    let diag = synthetic_diagnostic(
        "implicit truthiness on type `Int`",
        r,
        1,
        "change to `if x != 0:`",
    );

    let actions = build_code_actions(std::slice::from_ref(&diag), &u);
    assert_eq!(actions.len(), 1, "expected 1 CodeAction for tier 1");
    let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
        panic!("expected CodeAction variant");
    };
    assert_eq!(action.title, "change to `if x != 0:`");
    assert!(action.edit.is_some(), "BehaviorPreserving must carry edit");
    let edit = action.edit.as_ref().expect("checked Some above");
    let changes = edit.changes.as_ref().expect("WorkspaceEdit.changes Some");
    assert!(changes.contains_key(&u));
    let text_edits = &changes[&u];
    assert_eq!(text_edits.len(), 1);
    assert_eq!(text_edits[0].new_text, "change to `if x != 0:`");
}

/// Test 5: ApiChanging suggestion → CodeAction REFACTOR with no edit.
#[test]
fn code_action_api_changing_emits_suggest_only() {
    let u = uri("test.cb");
    let r = range_at(0, 0, 5);
    // Tier code 3 = ApiChanging.
    let diag = synthetic_diagnostic("api change required", r, 3, "rename method to `split2`");

    let actions = build_code_actions(&[diag], &u);
    assert_eq!(actions.len(), 1, "expected 1 CodeAction for tier 3");
    let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
        panic!("expected CodeAction variant");
    };
    assert!(
        action.edit.is_none(),
        "ApiChanging must NOT carry an edit payload"
    );
    assert_eq!(action.title, "rename method to `split2`");
}

/// Test 6: RequiresHumanReview → no CodeAction emitted.
#[test]
fn code_action_requires_human_review_emits_none() {
    let u = uri("test.cb");
    let r = range_at(0, 0, 1);
    // Tier code 5 = RequiresHumanReview → no CodeAction.
    let diag = synthetic_diagnostic("structural issue", r, 5, "redesign required");

    let actions = build_code_actions(&[diag], &u);
    assert!(
        actions.is_empty(),
        "RequiresHumanReview tier must produce no CodeAction"
    );

    // Also tier 4 (TargetChanging) — same skip behavior.
    let diag_target = synthetic_diagnostic("target change", r, 4, "switch target architecture");
    assert!(build_code_actions(&[diag_target], &u).is_empty());
}

// ---------------------------------------------------------------------------
// §5 cross-file rename integration (3 tests)
// ---------------------------------------------------------------------------

/// Test 7: rename in file-A propagates to file-B (both open).
#[test]
fn rename_cross_file_propagates_to_open_doc() {
    // file-A: defines + uses 'shared'.
    let src_a = "let shared = 1\nshared + 1\n";
    let lm_a = LineMap::from_source(src_a);
    // file-B: also uses 'shared' (textually — wave-3 word-scan).
    let src_b = "shared + 2\n";
    let lm_b = LineMap::from_source(src_b);
    let ctx = checked_ctx(src_a);
    let uri_a = uri("file_a.cb");
    let uri_b = uri("file_b.cb");

    // Cursor on 'shared' in file-A at line 0 char 4.
    let pos = Position {
        line: 0,
        character: 4,
    };
    let other_docs = vec![(uri_b.clone(), src_b.to_string(), lm_b)];
    let result = rename_symbol_cross_file(
        src_a,
        &lm_a,
        pos,
        "renamed",
        &ctx,
        uri_a.clone(),
        &other_docs,
    );
    assert!(result.is_some(), "cross-file rename must succeed");
    let ws = result.expect("checked Some above");
    let changes = ws.changes.expect("WorkspaceEdit.changes Some");

    // file-A: 'shared' appears 2 times (def + use).
    let edits_a = changes
        .get(&uri_a)
        .expect("primary file must be in changes");
    assert_eq!(edits_a.len(), 2, "file-A: 2 occurrences of 'shared'");

    // file-B: 'shared' appears 1 time.
    let edits_b = changes.get(&uri_b).expect("cross-file URI must be present");
    assert_eq!(edits_b.len(), 1, "file-B: 1 occurrence of 'shared'");
    assert_eq!(edits_b[0].new_text, "renamed");
}

/// Test 8: symbol absent from file-C → file-C NOT in WorkspaceEdit.
#[test]
fn rename_cross_file_omits_doc_without_occurrence() {
    let src_a = "let alpha = 5\n";
    let lm_a = LineMap::from_source(src_a);
    let src_c = "let beta = 6\nbeta + 1\n"; // no 'alpha'
    let lm_c = LineMap::from_source(src_c);
    let ctx = checked_ctx(src_a);
    let uri_a = uri("file_a.cb");
    let uri_c = uri("file_c.cb");

    let pos = Position {
        line: 0,
        character: 4,
    };
    let other_docs = vec![(uri_c.clone(), src_c.to_string(), lm_c)];
    let result =
        rename_symbol_cross_file(src_a, &lm_a, pos, "gamma", &ctx, uri_a.clone(), &other_docs);
    assert!(result.is_some());
    let ws = result.expect("rename Some");
    let changes = ws.changes.expect("changes Some");
    assert!(
        changes.contains_key(&uri_a),
        "file-A must be in changes (it has the def)"
    );
    assert!(
        !changes.contains_key(&uri_c),
        "file-C must NOT be in changes (no occurrence of 'alpha')"
    );
}

/// Test 9: WorkspaceEdit aggregates edits across multiple URIs correctly.
#[test]
fn rename_cross_file_aggregates_multi_uri() {
    let src_a = "let widget = 1\nwidget + 1\n"; // 2 occurrences
    let lm_a = LineMap::from_source(src_a);
    let src_b = "widget * 2\n"; // 1 occurrence
    let lm_b = LineMap::from_source(src_b);
    let src_c = "widget - widget\n"; // 2 occurrences
    let lm_c = LineMap::from_source(src_c);
    let ctx = checked_ctx(src_a);

    let uri_a = uri("a.cb");
    let uri_b = uri("b.cb");
    let uri_c = uri("c.cb");

    let pos = Position {
        line: 0,
        character: 4,
    };
    let other_docs = vec![
        (uri_b.clone(), src_b.to_string(), lm_b),
        (uri_c.clone(), src_c.to_string(), lm_c),
    ];
    let result = rename_symbol_cross_file(
        src_a,
        &lm_a,
        pos,
        "gadget",
        &ctx,
        uri_a.clone(),
        &other_docs,
    );
    let ws = result.expect("rename Some");
    let changes: HashMap<_, _> = ws.changes.expect("changes Some");

    assert_eq!(changes.len(), 3, "all 3 URIs must contribute edits");
    assert_eq!(changes[&uri_a].len(), 2);
    assert_eq!(changes[&uri_b].len(), 1);
    assert_eq!(changes[&uri_c].len(), 2);
    for edits in changes.values() {
        for e in edits {
            assert_eq!(e.new_text, "gadget");
        }
    }
}

// ---------------------------------------------------------------------------
// §5 snapshots (6 tests)
// ---------------------------------------------------------------------------

/// Snapshot 10: goto_def known symbol Location shape.
#[test]
fn snapshot_goto_def_known_symbol() {
    let source = "let x = 42\nx + 1\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let pos = Position {
        line: 1,
        character: 0,
    };
    let result = resolve_definition(source, &line_map, pos, &ctx, uri("test.cb"));
    insta::assert_json_snapshot!(result);
}

/// Snapshot 11: goto_def unresolved symbol → None.
#[test]
fn snapshot_goto_def_unresolved_none() {
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    // Cursor in whitespace.
    let pos = Position {
        line: 0,
        character: 3,
    };
    let result = resolve_definition(source, &line_map, pos, &ctx, uri("test.cb"));
    insta::assert_json_snapshot!(result);
}

/// Snapshot 12: CodeAction BehaviorPreserving shape.
#[test]
fn snapshot_code_action_behavior_preserving() {
    let u = uri("test.cb");
    let r = range_at(0, 3, 4);
    let diag = synthetic_diagnostic("truthiness", r, 1, "change to `if x != 0:`");
    let actions = build_code_actions(&[diag], &u);
    insta::assert_json_snapshot!(actions);
}

/// Snapshot 13: CodeAction ApiChanging shape (no edit).
#[test]
fn snapshot_code_action_api_changing() {
    let u = uri("test.cb");
    let r = range_at(0, 0, 5);
    let diag = synthetic_diagnostic("api change", r, 3, "rename method");
    let actions = build_code_actions(&[diag], &u);
    insta::assert_json_snapshot!(actions);
}

/// Snapshot 14: cross-file rename 2-URI WorkspaceEdit shape.
#[test]
fn snapshot_rename_cross_file_two_uri() {
    let src_a = "let shared = 1\nshared + 1\n";
    let lm_a = LineMap::from_source(src_a);
    let src_b = "shared + 2\n";
    let lm_b = LineMap::from_source(src_b);
    let ctx = checked_ctx(src_a);
    let uri_a = uri("a.cb");
    let uri_b = uri("b.cb");
    let pos = Position {
        line: 0,
        character: 4,
    };
    let other_docs = vec![(uri_b, src_b.to_string(), lm_b)];
    let result = rename_symbol_cross_file(src_a, &lm_a, pos, "renamed", &ctx, uri_a, &other_docs);
    // Snapshot the entire changes map (BTreeMap-sorted JSON for stability).
    let changes: std::collections::BTreeMap<String, _> = result
        .expect("rename Some")
        .changes
        .expect("changes Some")
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    insta::assert_json_snapshot!(changes);
}

/// Snapshot 15: cross-file rename 3-URI partial-match shape.
#[test]
fn snapshot_rename_cross_file_three_uri_partial() {
    let src_a = "let foo = 1\nfoo + 1\n";
    let lm_a = LineMap::from_source(src_a);
    let src_b = "foo * 2\n";
    let lm_b = LineMap::from_source(src_b);
    let src_c = "let bar = 3\n"; // no 'foo'
    let lm_c = LineMap::from_source(src_c);
    let ctx = checked_ctx(src_a);
    let uri_a = uri("a.cb");
    let uri_b = uri("b.cb");
    let uri_c = uri("c.cb");
    let pos = Position {
        line: 0,
        character: 4,
    };
    let other_docs = vec![
        (uri_b, src_b.to_string(), lm_b),
        (uri_c, src_c.to_string(), lm_c),
    ];
    let result = rename_symbol_cross_file(src_a, &lm_a, pos, "qux", &ctx, uri_a, &other_docs);
    let changes: std::collections::BTreeMap<String, _> = result
        .expect("rename Some")
        .changes
        .expect("changes Some")
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    insta::assert_json_snapshot!(changes);
}
