//! `textDocument/hover` handler — ADR-0057c §3.1.
//!
//! Resolves the identifier at the cursor position from the shared
//! `TypeCheckCtx` and renders the inferred type as a Markdown hover
//! card. Wave-2.2 uses a word-boundary heuristic to find the token
//! at the cursor; full DefId-span-indexed hover is deferred to wave-3.
//!
//! Hover card format:
//! ```text
//! **name**: `TypeDisplay`
//!
//! Inferred type.
//! ```

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};

use cobrust_types::TypeCheckCtx;

use crate::span_convert::LineMap;

/// Scan `source` backwards from `byte_offset` to find the start of
/// the identifier token that contains or immediately precedes the
/// cursor. Returns the `(start_byte, end_byte)` half-open range of
/// the word, or `None` if `byte_offset` is not within or adjacent to
/// an identifier character.
///
/// An "identifier character" for Cobrust is `[A-Za-z_][A-Za-z0-9_]*`
/// (ASCII only for the heuristic; non-ASCII identifiers are wave-3
/// scope).
#[must_use]
pub fn word_at_offset(source: &str, byte_offset: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return None;
    }
    // Clamp the offset to [0, len].
    let offset = byte_offset.min(len);

    // Helper: is `b` an identifier continuation character?
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    // The cursor may be at the start of the token or inside it.
    // We do NOT back up past a non-ident character — space/punctuation
    // at the cursor means "no identifier here."
    let in_range: usize = if offset < len && is_ident(bytes[offset]) {
        offset
    } else {
        return None;
    };

    // Walk backwards to the start of the word.
    let mut start = in_range;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }

    // Walk forwards to the end of the word.
    let mut end = in_range + 1;
    while end < len && is_ident(bytes[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }
    Some((start, end))
}

/// Build the Markdown hover card for a named binding and its type.
///
/// Format: `**name**: \`TypeDisplay\`\n\nInferred type.\n`
#[must_use]
pub fn render_hover_markdown(name: &str, ty_display: &str) -> String {
    format!("**{name}**: `{ty_display}`\n\nInferred type.")
}

/// Resolve the hover response for `position` in `source`.
///
/// Returns `Some(Hover)` if the cursor is on a known binding in
/// `ctx`, or `None` if the name is not found (unknown name, keyword,
/// punctuation).
///
/// The `line_map` is used to convert the LSP `Position` to a byte
/// offset and to convert the word's byte range back to an LSP `Range`
/// for the hover's optional highlight span.
#[must_use]
pub fn resolve_hover(
    source: &str,
    line_map: &LineMap,
    position: Position,
    ctx: &TypeCheckCtx,
) -> Option<Hover> {
    // 1. LSP position → byte offset.
    let byte_offset = line_map.position_to_byte(position)? as usize;

    // 2. Find the word at the cursor.
    let (word_start, word_end) = word_at_offset(source, byte_offset)?;

    // 3. Guard: valid UTF-8 slice.
    let name = source.get(word_start..word_end)?;

    // 4. Look up in the type context.
    let ty = ctx.lookup(name)?;

    // 5. Render Markdown hover card.
    let ty_display = ty.to_string();
    let markdown = render_hover_markdown(name, &ty_display);

    // 6. Compute the LSP Range for the word so editors highlight the
    //    token under the cursor while the hover card is open.
    let start_pos = line_map.byte_to_position(u32::try_from(word_start).expect("source < 4 GiB"));
    let end_pos = line_map.byte_to_position(u32::try_from(word_end).expect("source < 4 GiB"));
    let range = Range {
        start: start_pos,
        end: end_pos,
    };

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: Some(range),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_at_offset_basic() {
        let src = "let x = 42";
        // Cursor on 'x' (offset 4).
        assert_eq!(word_at_offset(src, 4), Some((4, 5)));
    }

    #[test]
    fn word_at_offset_multi_char() {
        let src = "let foo = 1";
        // 'f' is at byte 4, 'o' at 5, 'o' at 6.
        assert_eq!(word_at_offset(src, 4), Some((4, 7)));
        // Cursor on the last 'o' (byte 6) should still find the word.
        assert_eq!(word_at_offset(src, 6), Some((4, 7)));
        // Cursor one past end of word (byte 7 = space) → no hover.
        assert_eq!(word_at_offset(src, 7), None);
    }

    #[test]
    fn word_at_offset_on_space_returns_none() {
        let src = "let x = 42";
        // Offset 3 is the space after 'let'.
        assert_eq!(word_at_offset(src, 3), None);
    }

    #[test]
    fn word_at_offset_empty_source() {
        assert_eq!(word_at_offset("", 0), None);
    }

    #[test]
    fn render_hover_markdown_format() {
        let md = render_hover_markdown("x", "Int");
        assert!(md.contains("**x**"), "expected bold name");
        assert!(md.contains("`Int`"), "expected backtick type");
        assert!(md.contains("Inferred type."), "expected body text");
    }

    #[test]
    fn resolve_hover_known_binding() {
        use cobrust_frontend::span::FileId;
        use cobrust_types::{TypeCheckCtx, check_incremental};

        let source = "let x = 42\n";
        let line_map = LineMap::from_source(source);
        let mut ctx = TypeCheckCtx::new();
        let ast = cobrust_frontend::parse_str(source, FileId::SYNTHETIC)
            .expect("test: source must parse");
        let mut hir_sess = cobrust_hir::lower::Session::new();
        let hir = cobrust_hir::lower::lower(&ast, &mut hir_sess)
            .expect("test: HIR lower must succeed");
        let _ = check_incremental(&mut ctx, &hir, 1);

        // Cursor on 'x' at position (0, 4).
        let pos = Position {
            line: 0,
            character: 4,
        };
        let hover = resolve_hover(source, &line_map, pos, &ctx);
        assert!(hover.is_some(), "expected Some hover for known binding");
        if let Some(h) = hover {
            if let HoverContents::Markup(mc) = h.contents {
                assert!(
                    mc.value.contains("**x**"),
                    "hover card must contain bold name"
                );
            } else {
                panic!("expected Markup hover contents");
            }
        }
    }

    #[test]
    fn resolve_hover_unknown_name_returns_none() {
        let source = "let x = 1\n";
        let line_map = LineMap::from_source(source);
        let ctx = TypeCheckCtx::new();
        // Cursor at position of 'x' but ctx is empty.
        let pos = Position {
            line: 0,
            character: 4,
        };
        let hover = resolve_hover(source, &line_map, pos, &ctx);
        assert!(hover.is_none());
    }
}
