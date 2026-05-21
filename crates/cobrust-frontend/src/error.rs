//! Frontend error types.
//!
//! All errors are span-bearing: every variant carries enough source
//! information to point at the offending byte range. The frontend
//! treats *every* lex/parse failure as recoverable diagnostics rather
//! than panics — see crate-level docs.
//!
//! Tier-2 CQ P0-3 + CLAUDE.md §2.5 Direction B (LLM-first error UX):
//! every `ParseError` variant carries a uniform
//! `suggestion: Option<&'static str>` field populated at construction
//! time so the LSP / `--emit-json` / agent loop sees the fix path
//! without re-deriving it. The pattern mirrors ADR-0052b §2 (already
//! applied to `TypeError` / `MirError` / `LoweringError`). Wave-1 of
//! the frontend suggestion roll-out: `ParseError`. `LexError` shapes
//! stay structural for now — character-class diagnostics rarely have
//! a single canonical fix.

use thiserror::Error;

use crate::span::Span;
use crate::token::TokenKind;

/// Lexer error kinds.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum LexError {
    /// The input contains a byte sequence that is not valid UTF-8.
    /// `byte_offset` points at the first invalid byte.
    #[error("invalid UTF-8 at byte {byte_offset}")]
    InvalidUtf8 { byte_offset: u32 },
    /// A character that does not start any token.
    #[error("unexpected character {ch:?} at {span}")]
    UnexpectedChar { ch: char, span: Span },
    /// A string or bytes literal that ran past EOF.
    #[error("unterminated string literal at {span}")]
    UnterminatedString { span: Span },
    /// An f-string whose `{` was never closed.
    #[error("unterminated f-string interpolation at {span}")]
    UnterminatedFString { span: Span },
    /// A numeric literal that the lexer could not classify.
    #[error("malformed numeric literal at {span}")]
    MalformedNumber { span: Span },
    /// Mixed tabs and spaces in leading indentation.
    #[error("inconsistent indentation at {span}")]
    InconsistentIndent { span: Span },
    /// A `\` escape that is not recognized.
    #[error("invalid escape sequence at {span}")]
    InvalidEscape { span: Span },
}

/// Parser error kinds.
///
/// Each variant carries a uniform `suggestion: Option<&'static str>`
/// field populated at construction time per CLAUDE.md §2.5 Direction B
/// (LLM-first error UX). Variants where no actionable fix is obvious
/// at the call site pass `suggestion: None`.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum ParseError {
    /// We expected one set of token kinds, got something else.
    #[error("expected one of {expected:?} but found {found:?} at {span}")]
    Expected {
        expected: Vec<TokenKind>,
        found: TokenKind,
        span: Span,
        suggestion: Option<&'static str>,
    },
    /// Generic syntax error for less-tractable cases.
    #[error("{message} at {span}")]
    Syntax {
        message: String,
        span: Span,
        suggestion: Option<&'static str>,
    },
    /// Hit EOF while still parsing a construct.
    #[error("unexpected end of input at {span}")]
    UnexpectedEof {
        span: Span,
        suggestion: Option<&'static str>,
    },
    /// A statement-level form is not yet supported. Reserved for
    /// constructs that the constitution drops by name (`is`, `del`,
    /// `global`, `nonlocal`, etc.).
    #[error("the form `{name}` is not part of Cobrust (see CLAUDE.md §2.2) at {span}")]
    DroppedByConstitution {
        name: &'static str,
        span: Span,
        suggestion: Option<&'static str>,
    },
    /// A default argument value that is not a literal expression. M1
    /// rejects mutable / computed defaults at parse time
    /// (constitution §2.2). The type-checker (M2) does the rest.
    #[error("default argument must be a literal expression at {span}")]
    NonLiteralDefault {
        span: Span,
        suggestion: Option<&'static str>,
    },
    /// Indentation level is inconsistent with surrounding context.
    #[error("indentation error at {span}: {message}")]
    IndentError {
        message: String,
        span: Span,
        suggestion: Option<&'static str>,
    },
    /// Expression nesting exceeds the compile-time safety limit.
    ///
    /// Prevents stack-overflow DoS from adversarially deeply-nested
    /// parentheses / unary chains. Limit: `MAX_PARSER_DEPTH = 1024`.
    /// Suggestion: flatten deeply nested expressions.
    #[error(
        "expression nesting depth {depth} exceeds maximum ({max}) at {span}; \
         suggestion: flatten nested parentheses or sub-expressions"
    )]
    ExpressionTooDeep {
        depth: u32,
        max: u32,
        span: Span,
        suggestion: Option<&'static str>,
    },
}

/// Extract the construction-time `suggestion: Option<&'static str>`
/// payload from a `ParseError`. Mirrors `type_error_suggestion_text` /
/// `mir_error_suggestion_text` / `lowering_error_suggestion_text` per
/// CLAUDE.md §2.5 Direction B.
#[must_use]
pub fn parse_error_suggestion_text(err: &ParseError) -> Option<&'static str> {
    use ParseError::*;
    match err {
        Expected { suggestion, .. }
        | Syntax { suggestion, .. }
        | UnexpectedEof { suggestion, .. }
        | DroppedByConstitution { suggestion, .. }
        | NonLiteralDefault { suggestion, .. }
        | IndentError { suggestion, .. }
        | ExpressionTooDeep { suggestion, .. } => *suggestion,
    }
}

/// Top-level error returned by [`crate::parse_str`].
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum FrontendError {
    #[error(transparent)]
    Lex(LexError),
    #[error(transparent)]
    Parse(ParseError),
}
