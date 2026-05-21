//! ADR-0057b §5 — 5 integration tests for the `textDocument/didChange`
//! wave-2.1 handler surface.
//!
//! The tower-lsp `LanguageServer` impl is exercised at the public
//! `Backend` static-method layer + the in-process helpers
//! (`apply_content_changes`, `compile_diagnostics_with_session`); the
//! spawned-task debounce path is covered by the unit tests in
//! `src/debounce.rs`.
//!
//! End-to-end stdio LSP transport tests are deferred to a future smoke
//! sub-ADR (separate process boundary).

use cobrust_lsp::{Backend, DebounceTokens, LineMap};
use cobrust_types::TypeCheckCtx;
use std::sync::Arc;
use std::time::Duration;
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, Position, Range, TextDocumentContentChangeEvent,
    VersionedTextDocumentIdentifier,
};
use url::Url;

fn url(s: &str) -> Url {
    Url::parse(s).expect("static URL parses")
}

fn full_replace_event(text: &str) -> TextDocumentContentChangeEvent {
    TextDocumentContentChangeEvent {
        range: None,
        range_length: None,
        text: text.to_string(),
    }
}

fn incremental_event(range: Range, text: &str) -> TextDocumentContentChangeEvent {
    TextDocumentContentChangeEvent {
        range: Some(range),
        range_length: None,
        text: text.to_string(),
    }
}

fn _params(
    uri: &Url,
    version: i32,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> DidChangeTextDocumentParams {
    DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: uri.clone(),
            version,
        },
        content_changes: changes,
    }
}

/// Test 1 — Incremental did_change refreshes diagnostics.
///
/// Open with a `TypeError::TypeMismatch` → send an incremental edit
/// that fixes the type annotation → verify the second pipeline run
/// produces 0 diagnostics. We exercise the in-process static method
/// because the stdio transport layer is out of scope per ADR-0057b §5.
#[test]
fn did_change_incremental_refreshes_diagnostics() {
    // Open: `let x: i64 = "hello"` — TypeMismatch (Int vs Str).
    let initial = "let x: i64 = \"hello\"\n".to_string();
    let mut ctx = TypeCheckCtx::new();
    let file_id = 1u32;
    let line_map = LineMap::from_source(&initial);
    let diags_open =
        Backend::compile_diagnostics_with_session(&initial, &line_map, &mut ctx, file_id);
    assert!(
        !diags_open.is_empty(),
        "open should surface TypeMismatch; got {diags_open:?}"
    );

    // Incremental edit: replace `i64` with `str` so the annotation
    // matches the literal. The substring `i64` occupies bytes 7..10.
    // In Position terms that's line 0, chars 7..10.
    let edit = incremental_event(
        Range {
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
    let after = Backend::apply_content_changes(initial.clone(), &[edit]);
    assert_eq!(after, "let x: str = \"hello\"\n");

    let line_map_after = LineMap::from_source(&after);
    let diags_after =
        Backend::compile_diagnostics_with_session(&after, &line_map_after, &mut ctx, file_id);
    assert!(
        diags_after.is_empty(),
        "after the fixing edit, diagnostics should be empty; got {diags_after:?}"
    );
}

/// Test 2 — Full-replace did_change publishes fresh diagnostics.
///
/// Open with valid source → send a full-replace event introducing a
/// type error → verify the second emission carries 1 diagnostic.
#[test]
fn did_change_full_replace_diagnostics() {
    let initial = "let x: i64 = 1\n".to_string();
    let mut ctx = TypeCheckCtx::new();
    let file_id = 2u32;
    let line_map = LineMap::from_source(&initial);
    let diags_open =
        Backend::compile_diagnostics_with_session(&initial, &line_map, &mut ctx, file_id);
    assert!(
        diags_open.is_empty(),
        "open should be clean; got {diags_open:?}"
    );

    // Full-replace introducing TypeMismatch.
    let replace = full_replace_event("let y: i64 = \"oops\"\n");
    let after = Backend::apply_content_changes(initial.clone(), &[replace]);
    assert_eq!(after, "let y: i64 = \"oops\"\n");

    let line_map_after = LineMap::from_source(&after);
    let diags_after =
        Backend::compile_diagnostics_with_session(&after, &line_map_after, &mut ctx, file_id);
    assert!(
        !diags_after.is_empty(),
        "full-replace introducing TypeMismatch should surface a diagnostic; got {diags_after:?}"
    );
}

/// Test 3 — Debounce coalesces rapid edits to one pipeline emission.
///
/// Fires 5 schedule() calls for the same URI in quick succession.
/// Only the LAST scheduled version remains in `is_latest`; the
/// preceding 4 spawned tasks (in the real Backend handler) self-cancel
/// when they wake and observe their version was overtaken.
#[tokio::test(flavor = "current_thread")]
async fn did_change_debounce_coalesces() {
    let tokens = Arc::new(DebounceTokens::new(Duration::from_millis(50)));
    let u = url("file:///a.cb");

    // Schedule 5 rapid versions; only v5 should remain latest.
    for v in 1..=5 {
        let _t = tokens.schedule(u.clone(), v);
    }

    assert!(tokens.is_latest(&u, 5));
    for v in 1..=4 {
        assert!(!tokens.is_latest(&u, v), "v{v} should have been overtaken");
    }
}

/// Test 4 — invalidate drops stale type-cache rows.
///
/// Open `let x: i64 = 1` → second pipeline call (representing a
/// did_change) edits to `let x: str = "hi"` → verify the shared
/// `TypeCheckCtx` reports `Str` for `x`, not the stale `Int` row.
#[test]
fn did_change_invalidate_session_drops_stale_types() {
    let initial = "let x: i64 = 1\n".to_string();
    let mut ctx = TypeCheckCtx::new();
    let file_id = 3u32;
    let line_map = LineMap::from_source(&initial);
    let _ = Backend::compile_diagnostics_with_session(&initial, &line_map, &mut ctx, file_id);
    // After open, x is Int.
    let x_ty = ctx
        .lookup("x")
        .expect("`x` should be bound after first pipeline run");
    assert_eq!(
        format!("{x_ty:?}"),
        format!("{:?}", cobrust_types::ty::Ty::Int),
        "open should bind x to Int"
    );

    // Apply a full-replace event re-typing x to Str.
    let replace = full_replace_event("let x: str = \"hi\"\n");
    let after = Backend::apply_content_changes(initial.clone(), &[replace]);
    let line_map_after = LineMap::from_source(&after);
    let _ = Backend::compile_diagnostics_with_session(&after, &line_map_after, &mut ctx, file_id);

    let x_ty_after = ctx
        .lookup("x")
        .expect("`x` should still be bound after second pipeline run");
    assert_eq!(
        format!("{x_ty_after:?}"),
        format!("{:?}", cobrust_types::ty::Ty::Str),
        "invalidate + re-check should rebind x to Str, not stale Int"
    );
}

/// Test 5 — Concurrent edits serialise without race.
///
/// Spawn 10 concurrent debounce-schedule calls; assert no panic and
/// that the final `is_latest` reflects exactly one of the versions
/// (whichever recorded last). The debounce-tokens map is internally
/// Mutex-guarded; this verifies the invariant under contention.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn did_change_concurrent_serialized_no_race() {
    let tokens = Arc::new(DebounceTokens::new(Duration::from_millis(50)));
    let u = url("file:///b.cb");

    let mut handles = Vec::new();
    for v in 1..=10 {
        let t = Arc::clone(&tokens);
        let u_clone = u.clone();
        handles.push(tokio::spawn(async move {
            let _tok = t.schedule(u_clone, v);
        }));
    }
    for h in handles {
        h.await.expect("debounce schedule task panicked");
    }

    // One of {1..=10} should be the recorded latest. The map should
    // never have been corrupted; `is_latest` returns true for some v.
    let any_latest = (1..=10).any(|v| tokens.is_latest(&u, v));
    assert!(
        any_latest,
        "after 10 concurrent schedule calls some version should be latest"
    );
}
