//! `textDocument/prepareRename` and `textDocument/rename` handlers —
//! ADR-0057d §3.1 + §3.2.
//!
//! # Design
//!
//! Both handlers share a common word-at-cursor resolution step (reusing
//! [`crate::hover::word_at_offset`]) followed by guard checks:
//!
//! 1. **Non-identifier** — cursor on whitespace / punctuation → `None`.
//! 2. **Keyword** — name is in the Cobrust keyword list → `None`.
//! 3. **Unknown binding** — name absent from `TypeCheckCtx::bindings()`
//!    → `None`.
//!
//! If guards pass, `prepare_rename` returns a `PrepareRenameResponse::Range`
//! covering the word. `rename_symbol` additionally scans the entire source
//! for all word-boundary occurrences of the old name and builds a
//! `WorkspaceEdit` with one `TextEdit` per occurrence.
//!
//! ## Occurrence scan (§6.2)
//!
//! O(n) single-pass over the source bytes. For each byte position `i`
//! where `source[i]` matches the first byte of `old_name`:
//! - call `word_at_offset(source, i)` to get the token boundary,
//! - confirm the slice equals `old_name` exactly,
//! - record a `TextEdit` replacing that range.
//!
//! This avoids a full AST traversal for single-file scope (ADR-0057d §4
//! non-goal: no cross-file rename in wave-2.3).
//!
//! ## Scope
//!
//! Single-document only. `WorkspaceEdit.changes` always contains exactly
//! one URI key — the URI supplied by the caller. Cross-file workspace
//! rename is deferred to ADR-0057e (wave-3).

use std::collections::HashMap;

use tower_lsp::lsp_types::{Position, PrepareRenameResponse, Range, TextEdit, Url, WorkspaceEdit};

use cobrust_types::TypeCheckCtx;

use crate::completion::KEYWORDS;
use crate::hover::word_at_offset;
use crate::span_convert::LineMap;

// ─── guards ──────────────────────────────────────────────────────────────────

/// Return `true` if `name` is a Cobrust keyword (not rename-able).
fn is_keyword(name: &str) -> bool {
    KEYWORDS.contains(&name)
}

/// Core guard: resolve the symbol at `position`, check all rename-ability
/// conditions, return `Some((name, word_start, word_end))` or `None`.
///
/// Three failure paths:
/// 1. Position maps outside source / cursor not on an identifier byte.
/// 2. Identifier is a keyword.
/// 3. Identifier is not present in `ctx` (unbound / unknown).
fn resolve_rename_symbol<'src>(
    source: &'src str,
    line_map: &LineMap,
    position: Position,
    ctx: &TypeCheckCtx,
) -> Option<(&'src str, usize, usize)> {
    // LSP position → byte offset.
    let byte_offset = line_map.position_to_byte(position)? as usize;

    // Word boundary at offset.
    let (word_start, word_end) = word_at_offset(source, byte_offset)?;

    // Guard: valid UTF-8 slice (ASCII heuristic guarantees this).
    let name = source.get(word_start..word_end)?;

    // Guard: not a keyword.
    if is_keyword(name) {
        return None;
    }

    // Guard: name is a known binding in the type context.
    ctx.lookup(name)?;

    Some((name, word_start, word_end))
}

// ─── public API ──────────────────────────────────────────────────────────────

/// `textDocument/prepareRename` handler (ADR-0057d §3.1).
///
/// Returns `Some(PrepareRenameResponse::Range(range))` where `range` covers
/// the rename-able symbol under the cursor, or `None` if the cursor is not
/// on a rename-able symbol.
///
/// Rename-able conditions: cursor on an identifier that is (a) not a keyword
/// and (b) present in `ctx.bindings()`.
#[must_use]
pub fn prepare_rename(
    source: &str,
    line_map: &LineMap,
    position: Position,
    ctx: &TypeCheckCtx,
) -> Option<PrepareRenameResponse> {
    let (_name, word_start, word_end) = resolve_rename_symbol(source, line_map, position, ctx)?;

    let start_pos = line_map.byte_to_position(u32::try_from(word_start).expect("source < 4 GiB"));
    let end_pos = line_map.byte_to_position(u32::try_from(word_end).expect("source < 4 GiB"));
    Some(PrepareRenameResponse::Range(Range {
        start: start_pos,
        end: end_pos,
    }))
}

/// `textDocument/rename` handler (ADR-0057d §3.2).
///
/// Finds all word-boundary occurrences of the symbol at `position` in
/// `source` and returns a `WorkspaceEdit` replacing each with `new_name`.
///
/// Returns `None` if the cursor is not on a rename-able symbol (same guards
/// as [`prepare_rename`]).
///
/// The `WorkspaceEdit.changes` map contains exactly one entry keyed by
/// `uri` — the document URI supplied by the caller. Cross-file rename
/// is deferred to ADR-0057e (wave-3, non-goal for wave-2.3).
#[must_use]
pub fn rename_symbol(
    source: &str,
    line_map: &LineMap,
    position: Position,
    new_name: &str,
    ctx: &TypeCheckCtx,
    uri: Url,
) -> Option<WorkspaceEdit> {
    let (old_name, _def_start, _def_end) = resolve_rename_symbol(source, line_map, position, ctx)?;

    let edits = collect_occurrences(source, old_name, new_name, line_map);

    let mut changes = HashMap::new();
    changes.insert(uri, edits);
    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

/// Scan `source` for all word-boundary occurrences of `old_name` and build
/// a `TextEdit` for each, replacing with `new_name`.
///
/// Algorithm (ADR-0057d §6.2): O(n) single-pass.
/// For each index where `source[i]` matches the first byte of `old_name`,
/// invoke `word_at_offset` to confirm it's a standalone token, then confirm
/// the slice matches `old_name` exactly before recording the edit.
fn collect_occurrences(
    source: &str,
    old_name: &str,
    new_name: &str,
    line_map: &LineMap,
) -> Vec<TextEdit> {
    if old_name.is_empty() {
        return Vec::new();
    }
    let first_byte = old_name.as_bytes()[0];
    let name_len = old_name.len();
    let src_bytes = source.as_bytes();
    let src_len = src_bytes.len();
    let mut edits: Vec<TextEdit> = Vec::new();

    let mut i = 0usize;
    while i < src_len {
        if src_bytes[i] == first_byte
            && let Some((ws, we)) = word_at_offset(source, i)
        {
            // Only record if this occurrence starts at exactly `i`
            // (word_at_offset may return a wider boundary if called
            // in the middle of a token; we only want occurrences
            // whose start aligns with our scan position).
            if ws == i && we == i + name_len && source.get(ws..we) == Some(old_name) {
                let start_pos =
                    line_map.byte_to_position(u32::try_from(ws).expect("source < 4 GiB"));
                let end_pos =
                    line_map.byte_to_position(u32::try_from(we).expect("source < 4 GiB"));
                edits.push(TextEdit {
                    range: Range {
                        start: start_pos,
                        end: end_pos,
                    },
                    new_text: new_name.to_string(),
                });
                // Advance past the matched word.
                i = we;
                continue;
            }
        }
        i += 1;
    }

    edits
}

// ─── unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_keyword_let() {
        assert!(is_keyword("let"));
        assert!(is_keyword("def"));
        assert!(is_keyword("if"));
        assert!(!is_keyword("x"));
        assert!(!is_keyword("foo"));
    }

    #[test]
    fn collect_occurrences_two_refs() {
        let source = "let x = 42\nx + 1\n";
        let line_map = LineMap::from_source(source);
        let edits = collect_occurrences(source, "x", "y", &line_map);
        // Def at col 4 (line 0) + use at col 0 (line 1).
        assert_eq!(edits.len(), 2, "expected 2 occurrences of 'x'");
        assert_eq!(edits[0].new_text, "y");
        assert_eq!(edits[1].new_text, "y");
    }

    #[test]
    fn collect_occurrences_no_false_positives() {
        // "xx" should not match when scanning for "x".
        let source = "let xx = 1\n";
        let line_map = LineMap::from_source(source);
        let edits = collect_occurrences(source, "x", "y", &line_map);
        assert_eq!(edits.len(), 0, "'xx' must not match single 'x'");
    }

    #[test]
    fn collect_occurrences_single() {
        let source = "let alpha = 10\n";
        let line_map = LineMap::from_source(source);
        let edits = collect_occurrences(source, "alpha", "beta", &line_map);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "beta");
    }
}
