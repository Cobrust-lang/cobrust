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
