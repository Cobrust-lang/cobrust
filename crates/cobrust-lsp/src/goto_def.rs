//! `textDocument/definition` handler — ADR-0057e §3.1.
//!
//! Wave-3 ships go-to-definition with a same-document word-scan
//! fallback. The cursor → identifier → first textual occurrence path
//! resolves the def-site for any binding visible in the shared
//! `TypeCheckCtx`. Cross-file def-site indexing via HIR `DefId` span
//! map is deferred to wave-4 (ADR-0057 §5.4 source-map cost pay-down).
//!
//! # Algorithm
//!
//! 1. `Position` → byte offset (via `LineMap::position_to_byte`).
//! 2. `word_at_offset` (re-used from [`crate::hover`]) finds the
//!    identifier word boundary.
//! 3. Guard: the slice must be present in `TypeCheckCtx::lookup` (a
//!    known binding). Keywords and unbound symbols return `None`.
//! 4. Scan `source` for the first word-boundary occurrence of the
//!    identifier — this is the def-site (the first textual mention is
//!    necessarily the `let` / `def` binder for any well-typed program
//!    in wave-3 scope).
//! 5. Convert that range back to LSP `Range` and wrap in
//!    `GotoDefinitionResponse::Scalar(Location { uri, range })`.
//!
//! # Honest scope
//!
//! Wave-3 limit: same-document only. A symbol defined in file-A and
//! referenced in file-B resolves only if both files share the same
//! `TypeCheckCtx` row (which the cross-file `FileId` allocator from
//! ADR-0057b §3.4 enables) — but the def-site `Location` URI returned
//! is always the cursor's own URI. Cross-file `DefId`-span-map lookup
//! is deferred to wave-4.

use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};

use cobrust_types::TypeCheckCtx;

use crate::completion::KEYWORDS;
use crate::hover::word_at_offset;
use crate::span_convert::LineMap;

/// Find the first word-boundary occurrence of `name` in `source`. This
/// is the def-site under the wave-3 same-document word-scan
/// approximation.
fn first_word_occurrence(source: &str, name: &str) -> Option<(usize, usize)> {
    if name.is_empty() {
        return None;
    }
    let first_byte = name.as_bytes()[0];
    let name_len = name.len();
    let src_bytes = source.as_bytes();
    let src_len = src_bytes.len();
    let mut i = 0usize;
    while i < src_len {
        if src_bytes[i] == first_byte
            && let Some((ws, we)) = word_at_offset(source, i)
            && ws == i
            && we == i + name_len
            && source.get(ws..we) == Some(name)
        {
            return Some((ws, we));
        }
        i += 1;
    }
    None
}

/// Resolve the def-site location for the identifier under the cursor.
///
/// Returns `Some(GotoDefinitionResponse::Scalar(Location { uri, range }))`
/// where `range` covers the first textual word-boundary occurrence of
/// the identifier in `source` (the def-site under wave-3 scope), or
/// `None` if:
///
/// - the cursor is not on an identifier character (punctuation, EOF),
/// - the identifier is a Cobrust keyword,
/// - the identifier is not present in `ctx.lookup(name)` (unbound).
///
/// Wave-3 honest scope: same-document only. Cross-file def-site
/// indexing via HIR `DefId` span map is deferred to wave-4.
#[must_use]
pub fn resolve_definition(
    source: &str,
    line_map: &LineMap,
    position: Position,
    ctx: &TypeCheckCtx,
    uri: Url,
) -> Option<GotoDefinitionResponse> {
    // 1. LSP position → byte offset.
    let byte_offset = line_map.position_to_byte(position)? as usize;

    // 2. Word at cursor.
    let (word_start, word_end) = word_at_offset(source, byte_offset)?;
    let name = source.get(word_start..word_end)?;

    // 3. Guard: not a keyword.
    if KEYWORDS.contains(&name) {
        return None;
    }

    // 4. Guard: known binding.
    ctx.lookup(name)?;

    // 5. Find the first textual occurrence (def-site under wave-3 scope).
    let (def_start, def_end) = first_word_occurrence(source, name)?;

    // 6. Range conversion.
    let start_pos = line_map.byte_to_position(u32::try_from(def_start).expect("source < 4 GiB"));
    let end_pos = line_map.byte_to_position(u32::try_from(def_end).expect("source < 4 GiB"));

    Some(GotoDefinitionResponse::Scalar(Location {
        uri,
        range: Range {
            start: start_pos,
            end: end_pos,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_word_occurrence_basic() {
        let source = "let x = 42\nx + 1\n";
        // First occurrence of 'x' is at byte 4 ('let x ' → 'x' at 4).
        let result = first_word_occurrence(source, "x");
        assert_eq!(result, Some((4, 5)));
    }

    #[test]
    fn first_word_occurrence_skips_substring() {
        // 'xx' is not a match when scanning for 'x' (word_at_offset gives
        // a wider boundary, so ws != we when len doesn't match).
        let source = "let xx = 1\nx + 0\n";
        let result = first_word_occurrence(source, "x");
        // 'x' appears standalone on line 1 (byte 11) — should find that.
        assert_eq!(result, Some((11, 12)));
    }

    #[test]
    fn first_word_occurrence_missing() {
        let source = "let x = 1\n";
        let result = first_word_occurrence(source, "y");
        assert_eq!(result, None);
    }

    #[test]
    fn first_word_occurrence_empty_name() {
        let source = "let x = 1\n";
        assert_eq!(first_word_occurrence(source, ""), None);
    }
}
