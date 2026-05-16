//! Cobrust lexer.
//!
//! - **Input**: arbitrary `&str` (caller has already validated UTF-8;
//!   the convenience entrypoint accepts `&[u8]` and surfaces invalid
//!   UTF-8 as a [`LexError::InvalidUtf8`]).
//! - **Output**: `Vec<Token>` with explicit layout tokens
//!   ([`TokenKind::Newline`], [`TokenKind::Indent`], [`TokenKind::Dedent`])
//!   and a final [`TokenKind::Eof`].
//! - **Spans**: every token carries a [`Span`] referencing the input
//!   bytes (UTF-8 codepoint boundaries).
//! - **Soft keywords**: `match`, `case`, `type` are full keywords in
//!   M1 (the cost of M1 simplicity; if user feedback shows that
//!   ergonomics suffer, ADR follow-up). `_` is the [`TokenKind::Underscore`]
//!   token, never a regular identifier.
//!
//! The lexer is **panic-free on any UTF-8 input** (constitution gate).
//! Malformed input produces [`LexError`].

#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]

use std::str::FromStr;

use unicode_normalization::UnicodeNormalization;
use unicode_xid::UnicodeXID;

use crate::error::LexError;
use crate::span::{FileId, Span};
use crate::token::{FStringPiece, StrPrefix, Token, TokenKind};

/// Lex `source` into a stream of tokens.
///
/// # Errors
///
/// Returns [`LexError`] for malformed input. The lexer makes a best
/// effort to fail at the earliest offending byte.
pub fn lex(source: &str, file_id: FileId) -> Result<Vec<Token>, LexError> {
    Lexer::new(source, file_id).run()
}

/// Variant that accepts arbitrary bytes; useful for fuzz harnesses.
///
/// # Errors
///
/// Returns [`LexError::InvalidUtf8`] when `bytes` is not valid UTF-8.
pub fn lex_bytes(bytes: &[u8], file_id: FileId) -> Result<Vec<Token>, LexError> {
    let s = std::str::from_utf8(bytes).map_err(|e| LexError::InvalidUtf8 {
        byte_offset: e.valid_up_to() as u32,
    })?;
    lex(s, file_id)
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    file: FileId,
    /// Indentation stack (in spaces). Always begins with 0.
    indents: Vec<u32>,
    /// Set when we are between tokens *on a logical line* (so leading
    /// whitespace emits indent/dedent tokens). Reset at every newline.
    at_line_start: bool,
    /// Bracket nesting depth: physical newlines inside `()`, `[]` or
    /// `{}` are line-joins, not statement terminators.
    bracket_depth: i32,
    /// Whether the previous logical token was a layout token. Used
    /// to suppress consecutive `Newline` tokens.
    last_emitted_newline: bool,
    out: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str, file: FileId) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            file,
            indents: vec![0],
            at_line_start: true,
            bracket_depth: 0,
            last_emitted_newline: true,
            out: Vec::with_capacity(src.len() / 4 + 8),
        }
    }

    fn run(mut self) -> Result<Vec<Token>, LexError> {
        while self.pos < self.bytes.len() {
            if self.at_line_start && self.bracket_depth == 0 {
                self.handle_line_start()?;
                if self.pos >= self.bytes.len() {
                    break;
                }
            }
            let b = self.bytes[self.pos];
            match b {
                // Whitespace inside a line.
                b' ' | b'\t' => {
                    self.pos += 1;
                }
                // Comment to end of line.
                b'#' => {
                    while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                // Line continuation.
                b'\\' if self.peek_byte(1) == Some(b'\n') => {
                    self.pos += 2;
                }
                b'\\' if self.peek_byte(1) == Some(b'\r') => {
                    self.pos += if self.peek_byte(2) == Some(b'\n') {
                        3
                    } else {
                        2
                    };
                }
                b'\n' | b'\r' => {
                    self.consume_newline();
                    if self.bracket_depth == 0 {
                        self.emit_newline_if_needed();
                        self.at_line_start = true;
                    }
                }
                _ => {
                    self.lex_one_token()?;
                }
            }
        }
        // Flush remaining indent levels.
        let final_pos = self.pos as u32;
        self.emit_newline_if_needed();
        while self.indents.len() > 1 {
            self.indents.pop();
            self.out.push(Token::new(
                TokenKind::Dedent,
                Span::point(self.file, final_pos),
            ));
        }
        self.out.push(Token::new(
            TokenKind::Eof,
            Span::point(self.file, final_pos),
        ));
        Ok(self.out)
    }

    // ---- line-leading whitespace handling ------------------------------

    fn handle_line_start(&mut self) -> Result<(), LexError> {
        let line_start = self.pos as u32;
        let mut spaces: u32 = 0;
        let mut saw_tab = false;
        let mut saw_space = false;
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' => {
                    spaces += 1;
                    saw_space = true;
                    self.pos += 1;
                }
                b'\t' => {
                    // Treat tab as 8-column round-up. We *forbid* mixing
                    // tabs and spaces in leading indentation, so the
                    // exact tab-stop rule rarely matters; we still
                    // need a deterministic mapping.
                    spaces = (spaces / 8 + 1) * 8;
                    saw_tab = true;
                    self.pos += 1;
                }
                _ => break,
            }
        }
        if saw_tab && saw_space {
            return Err(LexError::InconsistentIndent {
                span: Span::new(self.file, line_start, self.pos as u32),
            });
        }
        // Blank line / comment line — no indent change.
        if self.pos >= self.bytes.len()
            || self.bytes[self.pos] == b'\n'
            || self.bytes[self.pos] == b'\r'
            || self.bytes[self.pos] == b'#'
        {
            return Ok(());
        }
        let current = *self.indents.last().expect("indent stack non-empty");
        if spaces > current {
            self.indents.push(spaces);
            self.out.push(Token::new(
                TokenKind::Indent,
                Span::new(self.file, line_start, self.pos as u32),
            ));
        } else {
            while *self.indents.last().expect("indent stack non-empty") > spaces {
                self.indents.pop();
                self.out.push(Token::new(
                    TokenKind::Dedent,
                    Span::point(self.file, line_start),
                ));
            }
            if *self.indents.last().expect("indent stack non-empty") != spaces {
                return Err(LexError::InconsistentIndent {
                    span: Span::new(self.file, line_start, self.pos as u32),
                });
            }
        }
        self.at_line_start = false;
        Ok(())
    }

    fn consume_newline(&mut self) {
        if self.bytes[self.pos] == b'\r' {
            self.pos += 1;
            if self.peek_byte(0) == Some(b'\n') {
                self.pos += 1;
            }
        } else {
            self.pos += 1;
        }
    }

    fn emit_newline_if_needed(&mut self) {
        if !self.last_emitted_newline {
            let p = self.pos as u32;
            self.out
                .push(Token::new(TokenKind::Newline, Span::point(self.file, p)));
            self.last_emitted_newline = true;
        }
    }

    // ---- per-token dispatch -------------------------------------------

    fn lex_one_token(&mut self) -> Result<(), LexError> {
        let start = self.pos;
        let b = self.bytes[self.pos];
        match b {
            b'(' => self.single(TokenKind::LParen, start),
            b')' => self.single(TokenKind::RParen, start),
            b'[' => self.single(TokenKind::LBracket, start),
            b']' => self.single(TokenKind::RBracket, start),
            b'{' => self.single(TokenKind::LBrace, start),
            b'}' => self.single(TokenKind::RBrace, start),
            b',' => self.simple(TokenKind::Comma, start, 1),
            b';' => self.simple(TokenKind::Semicolon, start, 1),
            b'@' => self.simple(TokenKind::At, start, 1),
            b'~' => self.simple(TokenKind::Tilde, start, 1),
            b'.' => {
                if self.starts_with(b"...") {
                    self.simple(TokenKind::DotDotDot, start, 3)
                } else if self.peek_byte(1).is_some_and(|c| c.is_ascii_digit()) {
                    self.lex_number(start)
                } else {
                    self.simple(TokenKind::Dot, start, 1)
                }
            }
            b':' => {
                if self.starts_with(b":=") {
                    self.simple(TokenKind::Walrus, start, 2)
                } else {
                    self.simple(TokenKind::Colon, start, 1)
                }
            }
            b'+' => self.maybe_eq(start, TokenKind::Plus, TokenKind::PlusEq),
            b'-' => {
                if self.starts_with(b"->") {
                    self.simple(TokenKind::Arrow, start, 2)
                } else if self.starts_with(b"-=") {
                    self.simple(TokenKind::MinusEq, start, 2)
                } else {
                    self.simple(TokenKind::Minus, start, 1)
                }
            }
            b'*' => {
                if self.starts_with(b"**=") {
                    self.simple(TokenKind::StarStarEq, start, 3)
                } else if self.starts_with(b"**") {
                    self.simple(TokenKind::StarStar, start, 2)
                } else if self.starts_with(b"*=") {
                    self.simple(TokenKind::StarEq, start, 2)
                } else {
                    self.simple(TokenKind::Star, start, 1)
                }
            }
            b'/' => {
                if self.starts_with(b"//=") {
                    self.simple(TokenKind::SlashSlashEq, start, 3)
                } else if self.starts_with(b"//") {
                    self.simple(TokenKind::SlashSlash, start, 2)
                } else if self.starts_with(b"/=") {
                    self.simple(TokenKind::SlashEq, start, 2)
                } else {
                    self.simple(TokenKind::Slash, start, 1)
                }
            }
            b'%' => self.maybe_eq(start, TokenKind::Percent, TokenKind::PercentEq),
            b'&' => self.maybe_eq(start, TokenKind::Amp, TokenKind::AmpEq),
            b'|' => self.maybe_eq(start, TokenKind::Pipe, TokenKind::PipeEq),
            b'^' => self.maybe_eq(start, TokenKind::Caret, TokenKind::CaretEq),
            b'<' => {
                if self.starts_with(b"<<=") {
                    self.simple(TokenKind::ShlEq, start, 3)
                } else if self.starts_with(b"<<") {
                    self.simple(TokenKind::Shl, start, 2)
                } else if self.starts_with(b"<=") {
                    self.simple(TokenKind::LtEq, start, 2)
                } else {
                    self.simple(TokenKind::Lt, start, 1)
                }
            }
            b'>' => {
                if self.starts_with(b">>=") {
                    self.simple(TokenKind::ShrEq, start, 3)
                } else if self.starts_with(b">>") {
                    self.simple(TokenKind::Shr, start, 2)
                } else if self.starts_with(b">=") {
                    self.simple(TokenKind::GtEq, start, 2)
                } else {
                    self.simple(TokenKind::Gt, start, 1)
                }
            }
            b'=' => {
                if self.starts_with(b"==") {
                    self.simple(TokenKind::EqEq, start, 2)
                } else {
                    self.simple(TokenKind::Eq, start, 1)
                }
            }
            b'!' => {
                if self.starts_with(b"!=") {
                    self.simple(TokenKind::NotEq, start, 2)
                } else {
                    Err(LexError::UnexpectedChar {
                        ch: '!',
                        span: Span::new(self.file, start as u32, (start + 1) as u32),
                    })
                }
            }
            b'\'' | b'"' => self.lex_string(start, StrPrefix::default()),
            b'0'..=b'9' => self.lex_number(start),
            _ => self.lex_word_or_string_with_prefix(start),
        }
    }

    fn simple(&mut self, kind: TokenKind, start: usize, len: usize) -> Result<(), LexError> {
        self.pos = start + len;
        self.out.push(Token::new(
            kind,
            Span::new(self.file, start as u32, self.pos as u32),
        ));
        self.last_emitted_newline = false;
        Ok(())
    }

    fn single(&mut self, kind: TokenKind, start: usize) -> Result<(), LexError> {
        // Track bracket depth here; this is only reachable for the
        // six bracket characters.
        match kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                self.bracket_depth += 1;
            }
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                self.bracket_depth = (self.bracket_depth - 1).max(0);
            }
            _ => {}
        }
        self.simple(kind, start, 1)
    }

    fn maybe_eq(
        &mut self,
        start: usize,
        plain: TokenKind,
        with_eq: TokenKind,
    ) -> Result<(), LexError> {
        if self.peek_byte(1) == Some(b'=') {
            self.simple(with_eq, start, 2)
        } else {
            self.simple(plain, start, 1)
        }
    }

    // ---- words: identifiers, keywords, prefixed strings ----------------

    fn lex_word_or_string_with_prefix(&mut self, start: usize) -> Result<(), LexError> {
        // Identifier-start: ASCII letter, `_`, or any XID_Start.
        let first_ch = self.current_char()?;
        if first_ch == '_' {
            // Distinguish lone `_` from identifiers like `_x` or `__init__`.
            let next = self.bytes.get(self.pos + 1).copied();
            let is_continuation = match next {
                Some(c) if c.is_ascii_alphanumeric() || c == b'_' => true,
                Some(_) => {
                    // Multi-byte continuation? Treat any non-ASCII as
                    // potentially identifier-continuation; checked below.
                    true
                }
                None => false,
            };
            if !is_continuation {
                self.pos += 1;
                self.out.push(Token::new(
                    TokenKind::Underscore,
                    Span::new(self.file, start as u32, self.pos as u32),
                ));
                self.last_emitted_newline = false;
                return Ok(());
            }
        }
        if !is_xid_start(first_ch) && first_ch != '_' {
            return Err(LexError::UnexpectedChar {
                ch: first_ch,
                span: Span::new(
                    self.file,
                    start as u32,
                    (start + first_ch.len_utf8()) as u32,
                ),
            });
        }
        // Consume the word.
        let word_start = self.pos;
        self.advance_char(first_ch);
        while self.pos < self.bytes.len() {
            let ch = self.current_char()?;
            if ch == '_' || is_xid_continue(ch) {
                self.advance_char(ch);
            } else {
                break;
            }
        }
        let word = &self.src[word_start..self.pos];

        // String literal with prefix: `r"..."`, `b"..."`, `rb"..."`, `f"..."`.
        if let Some(b) = self.bytes.get(self.pos).copied() {
            if (b == b'"' || b == b'\'') && is_string_prefix(word) {
                let prefix = parse_str_prefix(word);
                if word.eq_ignore_ascii_case("f")
                    || word.eq_ignore_ascii_case("rf")
                    || word.eq_ignore_ascii_case("fr")
                {
                    return self.lex_fstring(start);
                }
                return self.lex_string(start, prefix);
            }
        }

        // `inf` and `nan` are Float literals, not identifiers (M-F.3.3 gap d).
        if word == "inf" {
            self.out.push(Token::new(
                TokenKind::Float("inf".to_owned()),
                Span::new(self.file, start as u32, self.pos as u32),
            ));
            self.last_emitted_newline = false;
            return Ok(());
        }
        if word == "nan" {
            self.out.push(Token::new(
                TokenKind::Float("nan".to_owned()),
                Span::new(self.file, start as u32, self.pos as u32),
            ));
            self.last_emitted_newline = false;
            return Ok(());
        }

        // Otherwise: keyword or normalized identifier.
        let kind = if let Some(kw) = match_keyword(word) {
            kw
        } else {
            // NFKC-normalize the identifier text.
            let nfkc: String = word.nfkc().collect();
            TokenKind::Ident(nfkc)
        };
        self.out.push(Token::new(
            kind,
            Span::new(self.file, start as u32, self.pos as u32),
        ));
        self.last_emitted_newline = false;
        Ok(())
    }

    // ---- numbers -------------------------------------------------------

    fn lex_number(&mut self, start: usize) -> Result<(), LexError> {
        let mut is_float = false;
        let mut is_imag = false;
        // Hex / oct / bin prefix.
        if self.bytes[self.pos] == b'0' && self.pos + 1 < self.bytes.len() {
            let next = self.bytes[self.pos + 1];
            let prefix_len = match next {
                b'x' | b'X' | b'o' | b'O' | b'b' | b'B' => 2,
                _ => 0,
            };
            if prefix_len == 2 {
                self.pos += 2;
                let valid: fn(u8) -> bool = match next.to_ascii_lowercase() {
                    b'x' => |c| c.is_ascii_hexdigit() || c == b'_',
                    b'o' => |c| (b'0'..=b'7').contains(&c) || c == b'_',
                    b'b' => |c| c == b'0' || c == b'1' || c == b'_',
                    _ => unreachable!(),
                };
                let digits_start = self.pos;
                while self.pos < self.bytes.len() && valid(self.bytes[self.pos]) {
                    self.pos += 1;
                }
                if self.pos == digits_start {
                    return Err(LexError::MalformedNumber {
                        span: Span::new(self.file, start as u32, self.pos as u32),
                    });
                }
                let text = self.src[start..self.pos].to_owned();
                self.out.push(Token::new(
                    TokenKind::Int(text),
                    Span::new(self.file, start as u32, self.pos as u32),
                ));
                self.last_emitted_newline = false;
                return Ok(());
            }
        }
        // Decimal int / float.
        let started_with_dot = self.bytes[self.pos] == b'.';
        if started_with_dot {
            is_float = true;
            self.pos += 1;
            // Must have at least one digit.
            if !self.bytes.get(self.pos).is_some_and(|c| c.is_ascii_digit()) {
                return Err(LexError::MalformedNumber {
                    span: Span::new(self.file, start as u32, self.pos as u32),
                });
            }
        }
        while self.pos < self.bytes.len()
            && (self.bytes[self.pos].is_ascii_digit() || self.bytes[self.pos] == b'_')
        {
            self.pos += 1;
        }
        if !started_with_dot && self.peek_byte(0) == Some(b'.') {
            // Don't consume `.` if followed by another `.` (range) — but
            // we don't have ranges, so treat `.` after digits as float.
            // Disambiguate `1.method()`: a dot followed by an identifier
            // start is *not* a float decimal point.
            let after_dot = self.peek_byte(1);
            let dot_is_method =
                matches!(after_dot, Some(c) if c == b'_' || c.is_ascii_alphabetic());
            if !dot_is_method {
                is_float = true;
                self.pos += 1;
                while self.pos < self.bytes.len()
                    && (self.bytes[self.pos].is_ascii_digit() || self.bytes[self.pos] == b'_')
                {
                    self.pos += 1;
                }
            }
            let _ = started_with_dot;
        }
        // Exponent.
        if matches!(self.peek_byte(0), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek_byte(0), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exp_start = self.pos;
            while self.pos < self.bytes.len()
                && (self.bytes[self.pos].is_ascii_digit() || self.bytes[self.pos] == b'_')
            {
                self.pos += 1;
            }
            if self.pos == exp_start {
                return Err(LexError::MalformedNumber {
                    span: Span::new(self.file, start as u32, self.pos as u32),
                });
            }
        }
        // Imaginary suffix.
        if matches!(self.peek_byte(0), Some(b'j' | b'J')) {
            is_imag = true;
            self.pos += 1;
        }
        let text = self.src[start..self.pos].to_owned();
        let kind = if is_imag {
            TokenKind::Imag(text)
        } else if is_float {
            TokenKind::Float(text)
        } else {
            TokenKind::Int(text)
        };
        self.out.push(Token::new(
            kind,
            Span::new(self.file, start as u32, self.pos as u32),
        ));
        self.last_emitted_newline = false;
        let _ = String::from_str("");
        Ok(())
    }

    // ---- strings -------------------------------------------------------

    fn lex_string(&mut self, start: usize, prefix: StrPrefix) -> Result<(), LexError> {
        let quote = self.bytes[self.pos];
        // Triple quotes?
        let triple = self.bytes.get(self.pos + 1..self.pos + 3) == Some(&[quote, quote]);
        let quote_len = if triple { 3 } else { 1 };
        self.pos += quote_len;
        let body_start = self.pos;
        let mut value = String::new();
        let mut bytes_value: Vec<u8> = Vec::new();
        loop {
            if self.pos >= self.bytes.len() {
                return Err(LexError::UnterminatedString {
                    span: Span::new(self.file, start as u32, self.pos as u32),
                });
            }
            let b = self.bytes[self.pos];
            if !triple && (b == b'\n' || b == b'\r') {
                return Err(LexError::UnterminatedString {
                    span: Span::new(self.file, start as u32, self.pos as u32),
                });
            }
            // Closing quote(s).
            if b == quote {
                if triple && self.bytes.get(self.pos + 1..self.pos + 3) == Some(&[quote, quote]) {
                    self.pos += 3;
                    break;
                } else if !triple {
                    self.pos += 1;
                    break;
                } else {
                    if prefix.bytes {
                        bytes_value.push(b);
                    } else {
                        value.push(b as char);
                    }
                    self.pos += 1;
                    continue;
                }
            }
            // Escape.
            if b == b'\\' && !prefix.raw {
                let esc_start = self.pos;
                self.pos += 1;
                if self.pos >= self.bytes.len() {
                    return Err(LexError::UnterminatedString {
                        span: Span::new(self.file, start as u32, self.pos as u32),
                    });
                }
                let next = self.bytes[self.pos];
                match next {
                    b'\n' => {
                        self.pos += 1; // line continuation
                    }
                    b'\\' => {
                        self.pos += 1;
                        if prefix.bytes {
                            bytes_value.push(b'\\');
                        } else {
                            value.push('\\');
                        }
                    }
                    b'\'' => {
                        self.pos += 1;
                        if prefix.bytes {
                            bytes_value.push(b'\'');
                        } else {
                            value.push('\'');
                        }
                    }
                    b'"' => {
                        self.pos += 1;
                        if prefix.bytes {
                            bytes_value.push(b'"');
                        } else {
                            value.push('"');
                        }
                    }
                    b'n' => {
                        self.pos += 1;
                        if prefix.bytes {
                            bytes_value.push(b'\n');
                        } else {
                            value.push('\n');
                        }
                    }
                    b't' => {
                        self.pos += 1;
                        if prefix.bytes {
                            bytes_value.push(b'\t');
                        } else {
                            value.push('\t');
                        }
                    }
                    b'r' => {
                        self.pos += 1;
                        if prefix.bytes {
                            bytes_value.push(b'\r');
                        } else {
                            value.push('\r');
                        }
                    }
                    b'0' => {
                        self.pos += 1;
                        if prefix.bytes {
                            bytes_value.push(b'\0');
                        } else {
                            value.push('\0');
                        }
                    }
                    b'x' => {
                        self.pos += 1;
                        let v = self.read_hex(2).ok_or_else(|| LexError::InvalidEscape {
                            span: Span::new(self.file, esc_start as u32, self.pos as u32),
                        })?;
                        if prefix.bytes {
                            bytes_value.push(v as u8);
                        } else {
                            // hex byte; collapse to char for non-bytes literal.
                            value.push(v as u8 as char);
                        }
                    }
                    b'u' => {
                        self.pos += 1;
                        let v = self.read_hex(4).ok_or_else(|| LexError::InvalidEscape {
                            span: Span::new(self.file, esc_start as u32, self.pos as u32),
                        })?;
                        if prefix.bytes {
                            return Err(LexError::InvalidEscape {
                                span: Span::new(self.file, esc_start as u32, self.pos as u32),
                            });
                        }
                        let ch = char::from_u32(v).ok_or_else(|| LexError::InvalidEscape {
                            span: Span::new(self.file, esc_start as u32, self.pos as u32),
                        })?;
                        value.push(ch);
                    }
                    _ => {
                        return Err(LexError::InvalidEscape {
                            span: Span::new(self.file, esc_start as u32, self.pos as u32),
                        });
                    }
                }
                continue;
            }
            // Plain character.
            let ch = self.current_char()?;
            self.advance_char(ch);
            if prefix.bytes {
                let mut buf = [0u8; 4];
                let s = ch.encode_utf8(&mut buf);
                bytes_value.extend_from_slice(s.as_bytes());
            } else {
                value.push(ch);
            }
        }
        let _ = body_start;
        let kind = if prefix.bytes {
            TokenKind::Bytes {
                value: bytes_value,
                prefix,
            }
        } else {
            TokenKind::Str { value, prefix }
        };
        self.out.push(Token::new(
            kind,
            Span::new(self.file, start as u32, self.pos as u32),
        ));
        self.last_emitted_newline = false;
        Ok(())
    }

    fn lex_fstring(&mut self, start: usize) -> Result<(), LexError> {
        // We've already consumed the prefix word (e.g. `f`, `fr`).
        let quote = self.bytes[self.pos];
        let triple = self.bytes.get(self.pos + 1..self.pos + 3) == Some(&[quote, quote]);
        let quote_len = if triple { 3 } else { 1 };
        self.pos += quote_len;
        let mut pieces: Vec<FStringPiece> = Vec::new();
        let mut current_lit = String::new();
        loop {
            if self.pos >= self.bytes.len() {
                return Err(LexError::UnterminatedString {
                    span: Span::new(self.file, start as u32, self.pos as u32),
                });
            }
            let b = self.bytes[self.pos];
            if !triple && (b == b'\n' || b == b'\r') {
                return Err(LexError::UnterminatedString {
                    span: Span::new(self.file, start as u32, self.pos as u32),
                });
            }
            // Closing quote.
            if b == quote {
                if triple && self.bytes.get(self.pos + 1..self.pos + 3) == Some(&[quote, quote]) {
                    self.pos += 3;
                    break;
                } else if !triple {
                    self.pos += 1;
                    break;
                } else {
                    current_lit.push(b as char);
                    self.pos += 1;
                    continue;
                }
            }
            // Escaped braces: `{{` and `}}`.
            if b == b'{' && self.peek_byte(1) == Some(b'{') {
                current_lit.push('{');
                self.pos += 2;
                continue;
            }
            if b == b'}' && self.peek_byte(1) == Some(b'}') {
                current_lit.push('}');
                self.pos += 2;
                continue;
            }
            if b == b'{' {
                if !current_lit.is_empty() {
                    pieces.push(FStringPiece::Lit(std::mem::take(&mut current_lit)));
                }
                self.pos += 1;
                let interp_start = self.pos;
                let mut depth: i32 = 1;
                let mut in_str: Option<u8> = None;
                let mut debug_equals = false;
                let mut format_spec: Option<String> = None;
                while self.pos < self.bytes.len() && depth > 0 {
                    let c = self.bytes[self.pos];
                    if let Some(q) = in_str {
                        if c == b'\\' {
                            self.pos = (self.pos + 2).min(self.bytes.len());
                            continue;
                        }
                        if c == q {
                            in_str = None;
                        }
                        self.pos += 1;
                        continue;
                    }
                    match c {
                        b'\'' | b'"' => {
                            in_str = Some(c);
                            self.pos += 1;
                        }
                        b'{' | b'(' | b'[' => {
                            depth += 1;
                            self.pos += 1;
                        }
                        b')' | b']' => {
                            depth -= 1;
                            self.pos += 1;
                        }
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                let source = self.src[interp_start..self.pos].trim().to_owned();
                                self.pos += 1;
                                pieces.push(FStringPiece::Expr {
                                    source,
                                    debug_equals,
                                    format_spec,
                                });
                                break;
                            }
                            self.pos += 1;
                        }
                        b':' if depth == 1 => {
                            // Format spec begins. Capture until matching `}`.
                            let expr_text = self.src[interp_start..self.pos].trim().to_owned();
                            self.pos += 1;
                            let spec_start = self.pos;
                            let mut spec_depth = 0i32;
                            while self.pos < self.bytes.len() {
                                let cc = self.bytes[self.pos];
                                if cc == b'{' {
                                    spec_depth += 1;
                                    self.pos += 1;
                                } else if cc == b'}' {
                                    if spec_depth == 0 {
                                        break;
                                    }
                                    spec_depth -= 1;
                                    self.pos += 1;
                                } else {
                                    self.pos += 1;
                                }
                            }
                            if self.pos >= self.bytes.len() {
                                return Err(LexError::UnterminatedFString {
                                    span: Span::new(self.file, start as u32, self.pos as u32),
                                });
                            }
                            format_spec = Some(self.src[spec_start..self.pos].to_owned());
                            // Skip the `}`.
                            self.pos += 1;
                            pieces.push(FStringPiece::Expr {
                                source: expr_text,
                                debug_equals,
                                format_spec: format_spec.clone(),
                            });
                            depth = 0;
                            break;
                        }
                        b'=' if depth == 1 && self.peek_byte(1) != Some(b'=') && !debug_equals => {
                            debug_equals = true;
                            self.pos += 1;
                        }
                        _ => {
                            // Advance one full UTF-8 scalar.
                            let ch = self.current_char()?;
                            self.advance_char(ch);
                        }
                    }
                }
                if depth != 0 {
                    return Err(LexError::UnterminatedFString {
                        span: Span::new(self.file, start as u32, self.pos as u32),
                    });
                }
                continue;
            }
            // Plain char (or line continuation in triple-quoted body).
            let ch = self.current_char()?;
            self.advance_char(ch);
            current_lit.push(ch);
        }
        if !current_lit.is_empty() {
            pieces.push(FStringPiece::Lit(current_lit));
        }
        self.out.push(Token::new(
            TokenKind::FString { pieces },
            Span::new(self.file, start as u32, self.pos as u32),
        ));
        self.last_emitted_newline = false;
        Ok(())
    }

    // ---- helpers -------------------------------------------------------

    fn current_char(&self) -> Result<char, LexError> {
        let rest = &self.src[self.pos..];
        rest.chars().next().ok_or(LexError::InvalidUtf8 {
            byte_offset: self.pos as u32,
        })
    }

    fn advance_char(&mut self, ch: char) {
        self.pos += ch.len_utf8();
    }

    fn peek_byte(&self, off: usize) -> Option<u8> {
        self.bytes.get(self.pos + off).copied()
    }

    fn starts_with(&self, needle: &[u8]) -> bool {
        self.bytes[self.pos..].starts_with(needle)
    }

    fn read_hex(&mut self, n: usize) -> Option<u32> {
        if self.pos + n > self.bytes.len() {
            return None;
        }
        // Verify all `n` bytes are ASCII hex digits — guarantees the
        // slice is a valid UTF-8 substring **and** parses as hex.
        let bytes = &self.bytes[self.pos..self.pos + n];
        if !bytes.iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        // SAFETY: ASCII hex digits are valid UTF-8.
        let s = std::str::from_utf8(bytes).ok()?;
        let v = u32::from_str_radix(s, 16).ok()?;
        self.pos += n;
        Some(v)
    }
}

fn match_keyword(s: &str) -> Option<TokenKind> {
    Some(match s {
        "and" => TokenKind::KwAnd,
        "as" => TokenKind::KwAs,
        "await" => TokenKind::KwAwait,
        "break" => TokenKind::KwBreak,
        "case" => TokenKind::KwCase,
        "class" => TokenKind::KwClass,
        "continue" => TokenKind::KwContinue,
        "elif" => TokenKind::KwElif,
        "else" => TokenKind::KwElse,
        "except" => TokenKind::KwExcept,
        "False" => TokenKind::KwFalse,
        "finally" => TokenKind::KwFinally,
        "fn" => TokenKind::KwFn,
        "for" => TokenKind::KwFor,
        "from" => TokenKind::KwFrom,
        "if" => TokenKind::KwIf,
        "import" => TokenKind::KwImport,
        "in" => TokenKind::KwIn,
        "lambda" => TokenKind::KwLambda,
        "let" => TokenKind::KwLet,
        "match" => TokenKind::KwMatch,
        "None" => TokenKind::KwNone,
        "not" => TokenKind::KwNot,
        "or" => TokenKind::KwOr,
        "pass" => TokenKind::KwPass,
        "raise" => TokenKind::KwRaise,
        "return" => TokenKind::KwReturn,
        "True" => TokenKind::KwTrue,
        "try" => TokenKind::KwTry,
        "type" => TokenKind::KwType,
        "while" => TokenKind::KwWhile,
        "with" => TokenKind::KwWith,
        "yield" => TokenKind::KwYield,
        _ => return None,
    })
}

fn is_string_prefix(word: &str) -> bool {
    matches!(
        word,
        "r" | "R"
            | "b"
            | "B"
            | "f"
            | "F"
            | "rb"
            | "Rb"
            | "rB"
            | "RB"
            | "br"
            | "Br"
            | "bR"
            | "BR"
            | "rf"
            | "Rf"
            | "rF"
            | "RF"
            | "fr"
            | "Fr"
            | "fR"
            | "FR"
    )
}

fn parse_str_prefix(word: &str) -> StrPrefix {
    let mut p = StrPrefix::default();
    for c in word.chars() {
        match c {
            'r' | 'R' => p.raw = true,
            'b' | 'B' => p.bytes = true,
            _ => {}
        }
    }
    p
}

#[inline]
fn is_xid_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || UnicodeXID::is_xid_start(ch)
}

#[inline]
fn is_xid_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || UnicodeXID::is_xid_continue(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src, FileId::SYNTHETIC)
            .expect("lex ok")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn empty_input_is_just_eof() {
        assert_eq!(kinds(""), vec![TokenKind::Eof]);
    }

    #[test]
    fn keywords_are_recognized() {
        let k = kinds("if elif else while for");
        assert_eq!(k[0], TokenKind::KwIf);
        assert_eq!(k[1], TokenKind::KwElif);
        assert_eq!(k[2], TokenKind::KwElse);
        assert_eq!(k[3], TokenKind::KwWhile);
        assert_eq!(k[4], TokenKind::KwFor);
    }

    #[test]
    fn integer_literals() {
        match &kinds("0xFF_FF 0b101 42_000")[0] {
            TokenKind::Int(s) => assert_eq!(s, "0xFF_FF"),
            other => panic!("not int: {other:?}"),
        }
    }

    #[test]
    fn float_with_exponent() {
        match &kinds("1.5e-3")[0] {
            TokenKind::Float(s) => assert_eq!(s, "1.5e-3"),
            other => panic!("not float: {other:?}"),
        }
    }

    #[test]
    fn imaginary_literal() {
        match &kinds("3j")[0] {
            TokenKind::Imag(s) => assert_eq!(s, "3j"),
            other => panic!("not imag: {other:?}"),
        }
    }

    #[test]
    fn indentation_emits_indent_dedent() {
        let src = "if True:\n    x\n    y\nz\n";
        let k: Vec<_> = kinds(src);
        assert!(k.contains(&TokenKind::Indent));
        assert!(k.contains(&TokenKind::Dedent));
    }

    #[test]
    fn invalid_utf8_does_not_panic() {
        let bad = b"abc\xff\xfedef";
        let err = lex_bytes(bad, FileId::SYNTHETIC).expect_err("invalid utf8 must error");
        assert!(matches!(err, LexError::InvalidUtf8 { .. }));
    }

    #[test]
    fn arbitrary_utf8_does_not_panic() {
        // Random-ish UTF-8 input must never panic. A few inputs below
        // are intentionally syntactically invalid; the lexer must
        // either succeed or return an Err, never panic.
        let inputs: &[&str] = &[
            "🦀",
            "let x = 1",
            "\"\"\"\nmulti\nline\n\"\"\"",
            "f\"{x=:>10}\"",
            "0x",
            "'unterminated",
            "\\",
            "if\n\t  x",
        ];
        for s in inputs {
            let _ = lex(s, FileId::SYNTHETIC);
        }
    }

    #[test]
    fn fstring_is_segmented() {
        let k = kinds("f\"hello {name=:>10}!\"");
        match &k[0] {
            TokenKind::FString { pieces } => {
                assert!(pieces.len() >= 2);
            }
            other => panic!("not fstring: {other:?}"),
        }
    }

    #[test]
    fn underscore_alone_is_token() {
        match &kinds("_")[0] {
            TokenKind::Underscore => {}
            other => panic!("not underscore: {other:?}"),
        }
    }
}
