//! ADR-0057a §7 Day 3 snapshot tests — 5 canonical `TypeError`
//! variants → LSP `Diagnostic` JSON shape.
//!
//! Each snapshot captures the wire-shape per ADR-0057a §3 so any
//! drift in the conversion path (severity, code string, message,
//! related_information layout) surfaces in CI review. Snapshots
//! exclude the per-document `Url` (synthetic placeholder) and
//! `version` field so the diffs are stable across editors.

use cobrust_frontend::span::{FileId, Span};
use cobrust_lsp::diagnostic::type_error_to_diagnostics;
use cobrust_lsp::span_convert::LineMap;
use cobrust_types::TypeError;
use cobrust_types::ty::Ty;

fn span(start: u32, end: u32) -> Span {
    Span::new(FileId::SYNTHETIC, start, end)
}

fn line_map_for(source: &str) -> LineMap {
    LineMap::from_source(source)
}

#[test]
fn snapshot_type_mismatch() {
    let source = "let x: i64 = \"hello\"\n";
    let err = TypeError::TypeMismatch {
        expected: Ty::Int,
        actual: Ty::Str,
        span: span(13, 20),
        suggestion: Some("change to `: str` (or convert the value to `i64`)"),
    };
    let diags = type_error_to_diagnostics(&err, &line_map_for(source));
    insta::assert_json_snapshot!("type_mismatch", diags);
}

#[test]
fn snapshot_occurs_check() {
    let source = "let f = fn x: x(x)\n";
    let err = TypeError::OccursCheck {
        var: cobrust_types::ty::VarId(7),
        ty: Ty::Int,
        span: span(8, 18),
        suggestion: Some("add an explicit type annotation to break the recursive constraint"),
    };
    let diags = type_error_to_diagnostics(&err, &line_map_for(source));
    insta::assert_json_snapshot!("occurs_check", diags);
}

#[test]
fn snapshot_unbound_name() {
    let source = "y + 1\n";
    let err = TypeError::UnknownName {
        name: "y".to_string(),
        span: span(0, 1),
        suggestion: Some("did you forget to bind `y` with `let`?"),
    };
    let diags = type_error_to_diagnostics(&err, &line_map_for(source));
    insta::assert_json_snapshot!("unbound_name", diags);
}

#[test]
fn snapshot_implicit_truthiness() {
    let source = "if x:\n    pass\n";
    let err = TypeError::ImplicitTruthiness {
        actual: Ty::Int,
        span: span(3, 4),
        suggestion: Some("change to `if x != 0:` (use `.is_some()` for Option)"),
    };
    let diags = type_error_to_diagnostics(&err, &line_map_for(source));
    insta::assert_json_snapshot!("implicit_truthiness", diags);
}

#[test]
fn snapshot_arity_mismatch() {
    let source = "f(1, 2, 3)\n";
    let err = TypeError::ArityMismatch {
        expected: 2,
        actual: 3,
        span: span(0, 10),
        suggestion: Some("remove the extra argument or update `f`'s signature"),
    };
    let diags = type_error_to_diagnostics(&err, &line_map_for(source));
    insta::assert_json_snapshot!("arity_mismatch", diags);
}

// =====================================================================
// ADR-0057b wave-2.1 — snapshots of diagnostics produced by the
// stateful compile path AFTER applying a content-change event. These
// pin the JSON wire shape that LSP clients observe in the second
// `publish_diagnostics` emission triggered by `did_change`.
// =====================================================================

use cobrust_lsp::Backend;
use cobrust_lsp::LineMap as LspLineMap;
use cobrust_types::TypeCheckCtx;
use tower_lsp::lsp_types::{
    Position, Range as LspRange, TextDocumentContentChangeEvent,
};

fn full_replace(text: &str) -> TextDocumentContentChangeEvent {
    TextDocumentContentChangeEvent {
        range: None,
        range_length: None,
        text: text.to_string(),
    }
}

fn incremental(range: LspRange, text: &str) -> TextDocumentContentChangeEvent {
    TextDocumentContentChangeEvent {
        range: Some(range),
        range_length: None,
        text: text.to_string(),
    }
}

#[test]
fn snapshot_after_incremental_type_mismatch() {
    // Open with valid `let x: i64 = 1`, then incrementally edit the
    // literal `1` to `"oops"` (re-introduces a TypeMismatch).
    let initial = "let x: i64 = 1\n".to_string();
    let mut ctx = TypeCheckCtx::new();
    let file_id = 100u32;
    let _open = Backend::compile_diagnostics_with_session(
        &initial,
        &LspLineMap::from_source(&initial),
        &mut ctx,
        file_id,
    );

    // Replace `1` at line 0, chars 13..14 with `"oops"`.
    let edit = incremental(
        LspRange {
            start: Position {
                line: 0,
                character: 13,
            },
            end: Position {
                line: 0,
                character: 14,
            },
        },
        "\"oops\"",
    );
    let after = Backend::apply_content_changes(initial, &[edit]);
    let diags = Backend::compile_diagnostics_with_session(
        &after,
        &LspLineMap::from_source(&after),
        &mut ctx,
        file_id,
    );
    insta::assert_json_snapshot!("after_incremental_type_mismatch", diags);
}

#[test]
fn snapshot_after_full_replace_unbound_name() {
    let initial = "let x: i64 = 0\n".to_string();
    let mut ctx = TypeCheckCtx::new();
    let file_id = 101u32;
    let _open = Backend::compile_diagnostics_with_session(
        &initial,
        &LspLineMap::from_source(&initial),
        &mut ctx,
        file_id,
    );

    let replace = full_replace("let z: i64 = y + 1\n");
    let after = Backend::apply_content_changes(initial, &[replace]);
    let diags = Backend::compile_diagnostics_with_session(
        &after,
        &LspLineMap::from_source(&after),
        &mut ctx,
        file_id,
    );
    insta::assert_json_snapshot!("after_full_replace_unbound_name", diags);
}

#[test]
fn snapshot_after_incremental_implicit_truthiness() {
    let initial = "let x: i64 = 0\n".to_string();
    let mut ctx = TypeCheckCtx::new();
    let file_id = 102u32;
    let _open = Backend::compile_diagnostics_with_session(
        &initial,
        &LspLineMap::from_source(&initial),
        &mut ctx,
        file_id,
    );

    // Append an `if x:` block that triggers ImplicitTruthiness on
    // an Int value. We use full-replace to keep the snapshot stable
    // against any range-arithmetic edge in the test setup.
    let replace = full_replace("let x: i64 = 0\nif x:\n    pass\n");
    let after = Backend::apply_content_changes(initial, &[replace]);
    let diags = Backend::compile_diagnostics_with_session(
        &after,
        &LspLineMap::from_source(&after),
        &mut ctx,
        file_id,
    );
    insta::assert_json_snapshot!("after_incremental_implicit_truthiness", diags);
}

#[test]
fn snapshot_after_full_replace_arity_mismatch() {
    let initial = "let x: i64 = 0\n".to_string();
    let mut ctx = TypeCheckCtx::new();
    let file_id = 103u32;
    let _open = Backend::compile_diagnostics_with_session(
        &initial,
        &LspLineMap::from_source(&initial),
        &mut ctx,
        file_id,
    );

    let replace = full_replace(
        "fn add(a: i64, b: i64) -> i64:\n    return a + b\nlet r: i64 = add(1, 2, 3)\n",
    );
    let after = Backend::apply_content_changes(initial, &[replace]);
    let diags = Backend::compile_diagnostics_with_session(
        &after,
        &LspLineMap::from_source(&after),
        &mut ctx,
        file_id,
    );
    insta::assert_json_snapshot!("after_full_replace_arity_mismatch", diags);
}

#[test]
fn snapshot_after_incremental_clears_diagnostics() {
    // Open with TypeMismatch; the incremental fix should clear all
    // diagnostics — snapshot the empty vector to lock that behavior.
    let initial = "let x: i64 = \"oops\"\n".to_string();
    let mut ctx = TypeCheckCtx::new();
    let file_id = 104u32;
    let _open = Backend::compile_diagnostics_with_session(
        &initial,
        &LspLineMap::from_source(&initial),
        &mut ctx,
        file_id,
    );

    let edit = incremental(
        LspRange {
            start: Position {
                line: 0,
                character: 7,
            },
            end: Position {
                line: 0,
                character: 10,
            },
        },
        "str",
    );
    let after = Backend::apply_content_changes(initial, &[edit]);
    let diags = Backend::compile_diagnostics_with_session(
        &after,
        &LspLineMap::from_source(&after),
        &mut ctx,
        file_id,
    );
    insta::assert_json_snapshot!("after_incremental_clears_diagnostics", diags);
}
