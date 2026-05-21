//! ADR-0057d §5 integration + snapshot tests for rename.
//!
//! 9 tests:
//! - §5 prepare_rename integration (3 tests)
//! - §5 rename integration (3 tests)
//! - §5 rename snapshot (3 tests)

use cobrust_frontend::span::FileId;
use cobrust_lsp::rename::{prepare_rename, rename_symbol};
use cobrust_lsp::span_convert::LineMap;
use cobrust_types::{TypeCheckCtx, check_incremental};
use tower_lsp::lsp_types::{Position, PrepareRenameResponse, Url};

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

fn test_uri() -> Url {
    Url::parse("file:///test.cb").expect("test URI invalid")
}

// ---------------------------------------------------------------------------
// §5 prepare_rename integration (3 tests)
// ---------------------------------------------------------------------------

/// Test 1: Local variable at cursor is rename-able — returns Range.
#[test]
fn prepare_rename_local_var_returns_range() {
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);

    // 'x' is at byte 4, LSP (line=0, character=4).
    let pos = Position {
        line: 0,
        character: 4,
    };
    let result = prepare_rename(source, &line_map, pos, &ctx);
    assert!(result.is_some(), "local var must be rename-able");
    if let Some(PrepareRenameResponse::Range(r)) = result {
        // Range should cover exactly 'x': character 4..5 on line 0.
        assert_eq!(r.start.line, 0);
        assert_eq!(r.start.character, 4);
        assert_eq!(r.end.line, 0);
        assert_eq!(r.end.character, 5);
    } else {
        panic!("expected PrepareRenameResponse::Range");
    }
}

/// Test 2: Keyword at cursor (`let`) → returns None (not rename-able).
#[test]
fn prepare_rename_keyword_returns_none() {
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);

    // 'let' starts at byte 0 = LSP (line=0, character=0).
    let pos = Position {
        line: 0,
        character: 0,
    };
    let result = prepare_rename(source, &line_map, pos, &ctx);
    assert!(
        result.is_none(),
        "keyword 'let' must not be rename-able, got: {result:?}"
    );
}

/// Test 3: Undefined identifier / whitespace at cursor → returns None.
#[test]
fn prepare_rename_undefined_or_space_returns_none() {
    // 'y' is referenced but not bound in ctx — unknown binding.
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);

    // Cursor on space between 'let' and 'x' (byte 3).
    let pos_space = Position {
        line: 0,
        character: 3,
    };
    let result_space = prepare_rename(source, &line_map, pos_space, &ctx);
    assert!(result_space.is_none(), "space must not be rename-able");

    // Cursor on '4' (a digit, not an ident) — byte 8.
    let pos_digit = Position {
        line: 0,
        character: 8,
    };
    let result_digit = prepare_rename(source, &line_map, pos_digit, &ctx);
    assert!(result_digit.is_none(), "digit must not be rename-able");
}

// ---------------------------------------------------------------------------
// §5 rename integration (3 tests)
// ---------------------------------------------------------------------------

/// Test 4: `let x = 42; x + 1` rename x → y produces 2 TextEdits.
#[test]
fn rename_two_occurrences() {
    // Two-line source with def and use of `x`.
    let source = "let x = 42\nx + 1\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);

    // Cursor on 'x' at line 0, character 4.
    let pos = Position {
        line: 0,
        character: 4,
    };
    let result = rename_symbol(source, &line_map, pos, "y", &ctx, test_uri());
    assert!(result.is_some(), "rename must succeed for known binding");
    let ws = result.unwrap();
    let changes = ws.changes.expect("WorkspaceEdit.changes must be Some");
    let edits = changes
        .get(&test_uri())
        .expect("changes must have entry for test URI");
    assert_eq!(edits.len(), 2, "expected 2 TextEdits (def + use)");
    for edit in edits {
        assert_eq!(edit.new_text, "y", "all edits must replace with 'y'");
    }
}

/// Test 5: Rename def-only symbol (single occurrence) → 1 TextEdit.
#[test]
fn rename_single_occurrence() {
    let source = "let alpha = 10\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);

    // Cursor on 'alpha' at line 0, character 4.
    let pos = Position {
        line: 0,
        character: 4,
    };
    let result = rename_symbol(source, &line_map, pos, "beta", &ctx, test_uri());
    assert!(result.is_some(), "rename must succeed");
    let ws = result.unwrap();
    let changes = ws.changes.expect("changes must be Some");
    let edits = changes
        .get(&test_uri())
        .expect("changes must have entry for test URI");
    assert_eq!(edits.len(), 1, "expected 1 TextEdit (def only)");
    assert_eq!(edits[0].new_text, "beta");
}

/// Test 6: Multi-occurrence symbol across multiple lines.
#[test]
fn rename_multi_occurrence_multiline() {
    // `val` appears 3 times: def, two uses.
    let source = "let val = 5\nval + val\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);

    // Cursor on 'val' at line 0, character 4.
    let pos = Position {
        line: 0,
        character: 4,
    };
    let result = rename_symbol(source, &line_map, pos, "num", &ctx, test_uri());
    assert!(result.is_some(), "rename must succeed");
    let ws = result.unwrap();
    let changes = ws.changes.expect("changes must be Some");
    let edits = changes
        .get(&test_uri())
        .expect("changes must have entry for test URI");
    assert_eq!(edits.len(), 3, "expected 3 TextEdits (1 def + 2 uses)");
    for edit in edits {
        assert_eq!(edit.new_text, "num");
    }
}

// ---------------------------------------------------------------------------
// §5 rename snapshots (3 tests)
// ---------------------------------------------------------------------------

/// Snapshot 7: prepare_rename on known symbol — Range shape.
#[test]
fn snapshot_prepare_rename_known_symbol() {
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let pos = Position {
        line: 0,
        character: 4,
    };
    let result = prepare_rename(source, &line_map, pos, &ctx);
    insta::assert_json_snapshot!(result);
}

/// Snapshot 8: rename result WorkspaceEdit edits serialised.
#[test]
fn snapshot_rename_workspace_edit() {
    let source = "let x = 42\nx + 1\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let pos = Position {
        line: 0,
        character: 4,
    };
    let result = rename_symbol(source, &line_map, pos, "y", &ctx, test_uri());
    // Snapshot the WorkspaceEdit (includes changes map + TextEdit ranges).
    let changes = result.unwrap().changes.unwrap();
    let edits = changes.get(&test_uri()).unwrap();
    insta::assert_json_snapshot!(edits);
}

/// Snapshot 9: prepare_rename on keyword → None.
#[test]
fn snapshot_prepare_rename_keyword_none() {
    let source = "let x = 42\n";
    let line_map = LineMap::from_source(source);
    let ctx = checked_ctx(source);
    let pos = Position {
        line: 0,
        character: 0,
    }; // 'let' starts at char 0
    let result = prepare_rename(source, &line_map, pos, &ctx);
    insta::assert_json_snapshot!(result);
}
