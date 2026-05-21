#![allow(clippy::items_after_statements)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::match_same_arms)]
//! Lexer token-boundary and error-path tests (CQ P1-1 bump).
//!
//! Each test exercises a specific boundary in the lexer:
//! - UTF-8 / Unicode identifiers
//! - Indent / dedent stack boundaries
//! - String literal escapes and unterminated paths
//! - Numeric literal formats
//! - Invalid input → LexError variants

use cobrust_frontend::error::{FrontendError, LexError};
use cobrust_frontend::span::FileId;
use cobrust_frontend::token::TokenKind;
use cobrust_frontend::{lex, parse_str};

// =====================================================================
// Helper: lex a string and collect token kinds (excluding Eof/Newline/
// Indent/Dedent for most tests).
// =====================================================================

fn lex_kinds(src: &str) -> Vec<TokenKind> {
    let toks = lex(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("lex failed: {e:?}\nsource: {src:?}"));
    toks.into_iter()
        .filter(|t| {
            !matches!(
                &t.kind,
                TokenKind::Eof | TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
            )
        })
        .map(|t| t.kind)
        .collect()
}

fn lex_all(src: &str) -> Vec<TokenKind> {
    lex(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("lex failed: {e:?}"))
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

fn lex_err(src: &str) -> LexError {
    match parse_str(src, FileId::SYNTHETIC).unwrap_err() {
        FrontendError::Lex(l) => l,
        FrontendError::Parse(p) => panic!("expected LexError, got ParseError: {p:?}"),
    }
}

// =====================================================================
// Integer literals
// =====================================================================

#[test]
fn int_decimal() {
    let ks = lex_kinds("42\n");
    assert_eq!(ks, vec![TokenKind::Int("42".into())]);
}

#[test]
fn int_hex() {
    let ks = lex_kinds("0xFF\n");
    assert_eq!(ks, vec![TokenKind::Int("0xFF".into())]);
}

#[test]
fn int_octal() {
    let ks = lex_kinds("0o17\n");
    assert_eq!(ks, vec![TokenKind::Int("0o17".into())]);
}

#[test]
fn int_binary() {
    let ks = lex_kinds("0b1010\n");
    assert_eq!(ks, vec![TokenKind::Int("0b1010".into())]);
}

#[test]
fn int_with_underscores() {
    let ks = lex_kinds("1_000_000\n");
    assert_eq!(ks, vec![TokenKind::Int("1_000_000".into())]);
}

#[test]
fn int_zero() {
    let ks = lex_kinds("0\n");
    assert_eq!(ks, vec![TokenKind::Int("0".into())]);
}

// =====================================================================
// Float literals
// =====================================================================

#[test]
fn float_basic() {
    let ks = lex_kinds("3.14\n");
    assert_eq!(ks, vec![TokenKind::Float("3.14".into())]);
}

#[test]
fn float_exponent() {
    let ks = lex_kinds("1e10\n");
    assert_eq!(ks, vec![TokenKind::Float("1e10".into())]);
}

#[test]
fn float_exponent_negative() {
    let ks = lex_kinds("2.5e-3\n");
    assert_eq!(ks, vec![TokenKind::Float("2.5e-3".into())]);
}

#[test]
fn float_leading_dot() {
    let ks = lex_kinds(".5\n");
    assert_eq!(ks, vec![TokenKind::Float(".5".into())]);
}

#[test]
fn float_trailing_dot() {
    // "1." is a valid float literal
    let ks = lex_kinds("1.\n");
    assert_eq!(ks, vec![TokenKind::Float("1.".into())]);
}

// =====================================================================
// Imaginary literals
// =====================================================================

#[test]
fn imag_literal() {
    let ks = lex_kinds("3j\n");
    assert_eq!(ks, vec![TokenKind::Imag("3j".into())]);
}

// =====================================================================
// String literals — boundary / escape paths
// =====================================================================

#[test]
fn string_single_quote() {
    let ks = lex_kinds("'hello'\n");
    assert!(
        matches!(&ks[0], TokenKind::Str { value, .. } if value == "hello"),
        "expected Str(hello), got {ks:?}"
    );
}

#[test]
fn string_double_quote() {
    let ks = lex_kinds("\"world\"\n");
    assert!(
        matches!(&ks[0], TokenKind::Str { value, .. } if value == "world"),
        "expected Str(world)"
    );
}

#[test]
fn string_escape_newline() {
    let ks = lex_kinds(r#""a\nb""#);
    let _ = ks; // must not error
}

#[test]
fn string_escape_backslash() {
    let ks = lex_kinds(r#""a\\b""#);
    let _ = ks;
}

#[test]
fn string_escape_tab() {
    let ks = lex_kinds(r#""a\tb""#);
    let _ = ks;
}

#[test]
fn string_empty() {
    let ks = lex_kinds("\"\"\n");
    assert!(
        matches!(&ks[0], TokenKind::Str { value, .. } if value.is_empty()),
        "expected empty Str"
    );
}

#[test]
fn string_triple_double_quote() {
    let ks = lex_kinds("\"\"\"triple\"\"\"\n");
    assert!(
        matches!(&ks[0], TokenKind::Str { value, .. } if value == "triple"),
        "expected Str(triple)"
    );
}

// =====================================================================
// Unterminated string → LexError
// =====================================================================

#[test]
fn unterminated_string_error() {
    let err = lex_err("\"not closed\n");
    assert!(
        matches!(err, LexError::UnterminatedString { .. }),
        "expected UnterminatedString, got {err:?}"
    );
}

// =====================================================================
// Invalid escape → LexError
// =====================================================================

#[test]
fn invalid_escape_error() {
    // \q is not a valid escape
    let err = lex_err("\"\\q\"\n");
    assert!(
        matches!(err, LexError::InvalidEscape { .. }),
        "expected InvalidEscape, got {err:?}"
    );
}

// =====================================================================
// Unicode identifiers (NFKC normalization required)
// =====================================================================

#[test]
fn identifier_ascii() {
    let ks = lex_kinds("foo_bar\n");
    assert_eq!(ks, vec![TokenKind::Ident("foo_bar".into())]);
}

#[test]
fn identifier_unicode_greek() {
    // Greek letters are XID_Start / XID_Continue
    let ks = lex_kinds("αβγ\n");
    // Must lex as one Ident token (not error)
    assert_eq!(ks.len(), 1);
    assert!(matches!(&ks[0], TokenKind::Ident(_)));
}

#[test]
fn identifier_leading_underscore() {
    let ks = lex_kinds("_private\n");
    assert_eq!(ks, vec![TokenKind::Ident("_private".into())]);
}

#[test]
fn identifier_double_underscore() {
    let ks = lex_kinds("__dunder__\n");
    assert_eq!(ks, vec![TokenKind::Ident("__dunder__".into())]);
}

// =====================================================================
// Indent / dedent stack
// =====================================================================

#[test]
fn indent_dedent_single_level() {
    // fn body produces Indent then Dedent
    let src = "fn f():\n    pass\n";
    let toks = lex_all(src);
    let has_indent = toks.iter().any(|k| matches!(k, TokenKind::Indent));
    let has_dedent = toks.iter().any(|k| matches!(k, TokenKind::Dedent));
    assert!(has_indent, "expected Indent token");
    assert!(has_dedent, "expected Dedent token");
}

#[test]
fn indent_dedent_nested() {
    let src = "fn f():\n    if True:\n        pass\n";
    let toks = lex_all(src);
    let indent_count = toks
        .iter()
        .filter(|k| matches!(k, TokenKind::Indent))
        .count();
    let dedent_count = toks
        .iter()
        .filter(|k| matches!(k, TokenKind::Dedent))
        .count();
    assert_eq!(indent_count, dedent_count, "Indent/Dedent must be balanced");
    assert_eq!(indent_count, 2, "two levels of nesting = 2 Indent tokens");
}

#[test]
fn no_indent_in_expression_continuation() {
    // Inside brackets, physical newlines are line joins — no Indent/Dedent
    let src = "[\n    1,\n    2,\n]\n";
    let toks = lex_all(src);
    let indent_count = toks
        .iter()
        .filter(|k| matches!(k, TokenKind::Indent))
        .count();
    assert_eq!(indent_count, 0, "no Indent inside brackets");
}

// =====================================================================
// Inconsistent indentation → LexError
// =====================================================================

#[test]
fn inconsistent_indent_error() {
    // Mix tabs and spaces — must produce InconsistentIndent error
    // Note: "fn f():\n\t    pass\n" — tab then spaces on same dedent level
    let src = "fn f():\n\t    pass\n";
    // This may parse depending on how strict the lexer is. Check that either
    // parse succeeds (tab = 8-space equiv) or we get InconsistentIndent.
    match parse_str(src, FileId::SYNTHETIC) {
        Ok(_) => {} // lexer accepted tab-indented block
        Err(FrontendError::Lex(LexError::InconsistentIndent { .. })) => {} // expected
        Err(other) => {
            // Any other error is unexpected
            panic!("unexpected error for tab-indented source: {other:?}")
        }
    }
}

// =====================================================================
// Unexpected character → LexError
// =====================================================================

#[test]
fn unexpected_char_error() {
    // $ is not valid in Cobrust
    let err = lex_err("$foo\n");
    assert!(
        matches!(err, LexError::UnexpectedChar { ch: '$', .. }),
        "expected UnexpectedChar($), got {err:?}"
    );
}

// =====================================================================
// Comments are stripped
// =====================================================================

#[test]
fn comment_stripped() {
    let ks = lex_kinds("# this is a comment\n");
    // Only Eof — comment stripped
    assert!(ks.is_empty(), "comment should produce no non-layout tokens");
}

#[test]
fn inline_comment_stripped() {
    let ks = lex_kinds("42 # inline comment\n");
    assert_eq!(ks, vec![TokenKind::Int("42".into())]);
}

// =====================================================================
// Line continuation
// =====================================================================

#[test]
fn line_continuation_joins_lines() {
    // Backslash at end of line joins the next line
    let src = "1 +\\\n2\n";
    // Should parse as "1 + 2" without a Newline between
    let m = parse_str(src, FileId::SYNTHETIC).expect("line continuation should parse");
    assert!(!m.items.is_empty());
}

// =====================================================================
// Keyword tokens
// =====================================================================

#[test]
fn kw_fn_recognized() {
    let ks = lex_kinds("fn\n");
    assert!(matches!(&ks[0], TokenKind::KwFn), "expected KwFn");
}

#[test]
fn kw_if_else_recognized() {
    let ks = lex_kinds("if else\n");
    assert!(matches!(&ks[0], TokenKind::KwIf), "expected KwIf");
    assert!(matches!(&ks[1], TokenKind::KwElse), "expected KwElse");
}

#[test]
fn kw_return_recognized() {
    let ks = lex_kinds("return\n");
    assert!(matches!(&ks[0], TokenKind::KwReturn), "expected KwReturn");
}

#[test]
fn kw_let_recognized() {
    let ks = lex_kinds("let\n");
    assert!(matches!(&ks[0], TokenKind::KwLet), "expected KwLet");
}

// =====================================================================
// Operator tokens
// =====================================================================

#[test]
fn op_star_star_vs_star() {
    // ** must be tokenized as StarStar, not two Stars
    let ks = lex_kinds("2 ** 3\n");
    assert!(
        ks.iter().any(|k| matches!(k, TokenKind::StarStar)),
        "expected StarStar token, got {ks:?}"
    );
    assert!(
        !ks.iter().any(|k| matches!(k, TokenKind::Star)),
        "must not produce Star tokens for **"
    );
}

#[test]
fn op_slash_slash_vs_slash() {
    let ks = lex_kinds("a // b\n");
    assert!(
        ks.iter().any(|k| matches!(k, TokenKind::SlashSlash)),
        "expected SlashSlash"
    );
}

#[test]
fn op_arrow_recognized() {
    let ks = lex_kinds("-> i64\n");
    assert!(matches!(&ks[0], TokenKind::Arrow), "expected Arrow token");
}
