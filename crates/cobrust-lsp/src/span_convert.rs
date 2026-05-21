//! `Span` → LSP `Range` conversion via `LineMap`.
//!
//! Per ADR-0057a §6: Cobrust `Span { file: FileId, start: u32, end: u32 }`
//! is a byte-offset half-open range; LSP `Range { start: Position { line,
//! character }, end: ... }` is 0-indexed with UTF-16 code-unit columns
//! (LSP spec §"Position Encoding Kinds" defaults to `utf-16`).
//!
//! Wave-1 builds the [`LineMap`] once per `did_open` and reuses it on
//! every `did_change` for the same URI. Phase J+ may lift this helper
//! into `cobrust-frontend` so `cobrust-cli` M15 source-map rendering
//! (`crates/cobrust-cli/src/error_ux.rs:343-352` `span_to_line_col`
//! stub) can reuse the same logic.

use cobrust_frontend::span::Span;
use tower_lsp::lsp_types::{Position, Range};

/// Maps byte offsets in a source string to (line, UTF-16 column) pairs.
///
/// Built once per document version. `line_starts[i]` is the byte
/// offset of the first character on line `i` (0-indexed). Lookup is
/// `O(log n)` via binary search.
#[derive(Clone, Debug)]
pub struct LineMap {
    /// Byte offset of the start of each line (0-indexed). The first
    /// entry is always `0`. The last entry is the byte length of the
    /// source (so a span ending at EOF maps cleanly).
    line_starts: Vec<u32>,
    /// Cached copy of the source text; needed to compute UTF-16
    /// column offsets at lookup time.
    source: String,
}

impl LineMap {
    /// Build a `LineMap` from a source string.
    ///
    /// Recognises `\n` as a line break. `\r\n` collapses to `\n`-only
    /// tracking — the LSP range still points at the byte after the
    /// `\r`, which all major editors render correctly because the
    /// `\r` is treated as part of the prior line.
    #[must_use]
    pub fn from_source(source: &str) -> Self {
        let mut line_starts: Vec<u32> = vec![0];
        let bytes = source.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                // The next byte is the first column of the next line.
                let next = u32::try_from(i + 1).unwrap_or(u32::MAX);
                line_starts.push(next);
            }
        }
        // Sentinel for EOF lookups.
        let total = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        if line_starts.last().copied() != Some(total) {
            line_starts.push(total);
        }
        Self {
            line_starts,
            source: source.to_string(),
        }
    }

    /// Convert an LSP [`Position`] back to a byte offset in the source.
    ///
    /// Inverse of [`Self::byte_to_position`] modulo positions that point
    /// past EOF (returned as `Some(source.len() as u32)`) and positions
    /// past the end of their line (clamped to the line's last byte).
    /// Returns `None` only if `position.line` exceeds the number of
    /// lines in the source.
    ///
    /// Per LSP spec §"Position Encoding Kinds", `character` is a UTF-16
    /// code-unit count from the start of the line. The traversal here
    /// walks chars one at a time, accumulating both byte-length and
    /// UTF-16 length, stopping when we hit `position.character`.
    #[must_use]
    pub fn position_to_byte(&self, position: Position) -> Option<u32> {
        let line = position.line as usize;
        if line >= self.line_starts.len() {
            return None;
        }
        let line_start = self.line_starts[line] as usize;
        let source_len = self.source.len();
        // Determine line end: next line's start, or EOF.
        let line_end = self
            .line_starts
            .get(line + 1)
            .copied()
            .map(|x| x as usize)
            .unwrap_or(source_len);
        // Slice the line content (excluding the terminating newline
        // for clean per-char walks; the trailing \n's byte is line_end-1).
        let line_bytes_end = if line_end > 0
            && line_end <= source_len
            && self.source.as_bytes().get(line_end - 1) == Some(&b'\n')
        {
            line_end - 1
        } else {
            line_end
        };
        let line_bytes_end = line_bytes_end.min(source_len);
        let line_start = line_start.min(line_bytes_end);
        let line_str = &self.source[line_start..line_bytes_end];
        let target = position.character as usize;
        let mut utf16_seen = 0usize;
        let mut byte_off = line_start;
        for ch in line_str.chars() {
            if utf16_seen >= target {
                break;
            }
            byte_off += ch.len_utf8();
            utf16_seen += ch.len_utf16();
        }
        // If `target > utf16_seen` we clamp to the line's end.
        Some(u32::try_from(byte_off).unwrap_or(u32::MAX))
    }

    /// Convert a byte offset into the source into an LSP [`Position`].
    ///
    /// Per LSP spec §"Position Encoding Kinds", `character` is a
    /// UTF-16 code-unit count from the start of the line, not a byte
    /// count and not a Unicode codepoint count. ASCII source files
    /// behave identically under all three encodings; this matters
    /// only for sources with codepoints outside the BMP (e.g. emoji).
    #[must_use]
    pub fn byte_to_position(&self, byte_offset: u32) -> Position {
        // Binary-search for the largest `line_starts[i] <= byte_offset`.
        let line_idx = match self.line_starts.binary_search(&byte_offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts.get(line_idx).copied().unwrap_or(0);
        let line_byte_off = byte_offset.saturating_sub(line_start) as usize;
        let line_start_us = line_start as usize;
        // Slice the line up to `byte_offset` and count UTF-16 units.
        let bytes = self.source.as_bytes();
        let safe_end = bytes.len().min(line_start_us + line_byte_off);
        let safe_end = clamp_to_char_boundary(&self.source, safe_end);
        let prefix = &self.source[line_start_us..safe_end];
        let utf16_units: u32 = u32::try_from(prefix.encode_utf16().count()).unwrap_or(u32::MAX);
        Position {
            line: u32::try_from(line_idx).unwrap_or(u32::MAX),
            character: utf16_units,
        }
    }
}

/// Round `byte_offset` down to the nearest `char` boundary in `source`.
fn clamp_to_char_boundary(source: &str, byte_offset: usize) -> usize {
    let mut off = byte_offset.min(source.len());
    while off > 0 && !source.is_char_boundary(off) {
        off -= 1;
    }
    off
}

/// Convert a Cobrust [`Span`] to an LSP [`Range`] using `line_map`.
///
/// Per ADR-0057a §6, byte-offset half-open range → 0-indexed
/// (line, UTF-16-column) half-open range.
#[must_use]
pub fn span_to_lsp_range(span: &Span, line_map: &LineMap) -> Range {
    Range {
        start: line_map.byte_to_position(span.start),
        end: line_map.byte_to_position(span.end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_map_basic_ascii() {
        let lm = LineMap::from_source("abc\ndef\nghi");
        // 'a' at offset 0 -> (0, 0)
        assert_eq!(
            lm.byte_to_position(0),
            Position {
                line: 0,
                character: 0
            }
        );
        // 'b' at offset 1 -> (0, 1)
        assert_eq!(
            lm.byte_to_position(1),
            Position {
                line: 0,
                character: 1
            }
        );
        // 'd' at offset 4 -> (1, 0)
        assert_eq!(
            lm.byte_to_position(4),
            Position {
                line: 1,
                character: 0
            }
        );
        // 'g' at offset 8 -> (2, 0)
        assert_eq!(
            lm.byte_to_position(8),
            Position {
                line: 2,
                character: 0
            }
        );
    }

    #[test]
    fn line_map_handles_empty_source() {
        let lm = LineMap::from_source("");
        assert_eq!(
            lm.byte_to_position(0),
            Position {
                line: 0,
                character: 0
            }
        );
    }

    #[test]
    fn line_map_handles_trailing_newline() {
        let lm = LineMap::from_source("a\n");
        // Offset 2 is right after the '\n' — on a virtual line 1, col 0.
        assert_eq!(
            lm.byte_to_position(2),
            Position {
                line: 1,
                character: 0
            }
        );
    }

    #[test]
    fn line_map_utf16_emoji() {
        // 🦀 is U+1F980, encoded as 4 bytes in UTF-8 and 2 code units
        // in UTF-16 (surrogate pair).
        let lm = LineMap::from_source("a🦀b");
        // After 'a': byte 1, line 0, char 1.
        assert_eq!(
            lm.byte_to_position(1),
            Position {
                line: 0,
                character: 1
            }
        );
        // After '🦀': byte 5, line 0, char 3 (1 + 2 surrogates).
        assert_eq!(
            lm.byte_to_position(5),
            Position {
                line: 0,
                character: 3
            }
        );
    }

    #[test]
    fn span_to_range_basic() {
        let lm = LineMap::from_source("let x = 1\nlet y = 2");
        let span = Span::new(cobrust_frontend::span::FileId::SYNTHETIC, 10, 13);
        let range = span_to_lsp_range(&span, &lm);
        assert_eq!(
            range.start,
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            range.end,
            Position {
                line: 1,
                character: 3
            }
        );
    }

    #[test]
    fn position_to_byte_basic_ascii() {
        let lm = LineMap::from_source("abc\ndef\nghi");
        assert_eq!(
            lm.position_to_byte(Position {
                line: 0,
                character: 0
            }),
            Some(0)
        );
        assert_eq!(
            lm.position_to_byte(Position {
                line: 0,
                character: 2
            }),
            Some(2)
        );
        assert_eq!(
            lm.position_to_byte(Position {
                line: 1,
                character: 0
            }),
            Some(4)
        );
        assert_eq!(
            lm.position_to_byte(Position {
                line: 2,
                character: 1
            }),
            Some(9)
        );
    }

    #[test]
    fn position_to_byte_roundtrip_ascii() {
        let src = "let x = 1\nlet y = 2\n";
        let lm = LineMap::from_source(src);
        for byte in [0u32, 4, 9, 10, 14, 19] {
            let pos = lm.byte_to_position(byte);
            assert_eq!(lm.position_to_byte(pos), Some(byte), "byte={byte}");
        }
    }

    #[test]
    fn position_to_byte_handles_utf16_emoji() {
        // 🦀 = 4 bytes UTF-8 / 2 UTF-16 code units.
        let lm = LineMap::from_source("a🦀b");
        // Char position 3 = after 🦀 = byte 5.
        assert_eq!(
            lm.position_to_byte(Position {
                line: 0,
                character: 3
            }),
            Some(5)
        );
    }

    #[test]
    fn position_to_byte_out_of_bounds_line_returns_none() {
        let lm = LineMap::from_source("abc");
        assert_eq!(
            lm.position_to_byte(Position {
                line: 99,
                character: 0
            }),
            None
        );
    }

    #[test]
    fn position_to_byte_clamps_past_line_end() {
        let lm = LineMap::from_source("abc\ndef");
        // Position past end of line 0 → clamps to line 0's last byte.
        let off = lm
            .position_to_byte(Position {
                line: 0,
                character: 100,
            })
            .expect("line 0 exists");
        assert!(off <= 3, "off={off} should clamp ≤ 3");
    }
}
