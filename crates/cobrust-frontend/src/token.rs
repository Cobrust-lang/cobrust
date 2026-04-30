//! Token types produced by the lexer.
//!
//! See `docs/agent/adr/0003-core-30-forms.md` "Lexer scope" for the
//! authoritative list of token classes.

use std::fmt;

use crate::span::Span;

/// A lexed token: kind + source span. The token itself does not own
/// the source string; substrings are recovered by spanning back into
/// the original input when needed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    #[must_use]
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Token kinds. String / number / identifier payloads are stored
/// inline so the parser need not re-slice the source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    // ---- Identifiers and literals --------------------------------------
    /// NFKC-normalized identifier text.
    Ident(String),
    /// Integer literal (no leading sign — sign is a unary op). Keeps
    /// the source spelling so we can preserve underscores and base
    /// prefix in unparse.
    Int(String),
    /// Floating-point literal (source spelling preserved).
    Float(String),
    /// Imaginary numeric literal (source spelling, including `j`).
    Imag(String),
    /// String literal: stores prefix flags and the *parsed* value
    /// (escapes resolved). The original source is recoverable via
    /// the span.
    Str {
        value: String,
        prefix: StrPrefix,
    },
    /// Raw byte string literal.
    Bytes {
        value: Vec<u8>,
        prefix: StrPrefix,
    },
    /// F-string literal kept as a single token. The parser drives
    /// re-lexing of interpolation pieces from the inner source via
    /// the `pieces` payload (already segmented by the lexer).
    FString {
        pieces: Vec<FStringPiece>,
    },

    // ---- Keywords (only those used by the 30 forms) --------------------
    KwAnd,
    KwAs,
    KwAwait,
    KwBreak,
    KwCase,
    KwClass,
    KwContinue,
    KwElif,
    KwElse,
    KwExcept,
    KwFalse,
    KwFinally,
    KwFn,
    KwFor,
    KwFrom,
    KwIf,
    KwImport,
    KwIn,
    KwLambda,
    KwLet,
    KwMatch,
    KwNone,
    KwNot,
    KwOr,
    KwPass,
    KwRaise,
    KwReturn,
    KwTrue,
    KwTry,
    KwType,
    KwWhile,
    KwWith,
    KwYield,

    // ---- Punctuation / operators ---------------------------------------
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Semicolon,
    Dot,
    DotDotDot,
    Arrow,  // ->
    At,     // @
    Eq,     // =
    Walrus, // :=

    Plus,
    Minus,
    Star,
    StarStar,
    Slash,
    SlashSlash,
    Percent,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,

    PlusEq,
    MinusEq,
    StarEq,
    StarStarEq,
    SlashEq,
    SlashSlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,

    /// Wildcard pattern. Soft keyword in pattern context, but the
    /// lexer always emits it as a distinct token because the
    /// identifier `_` is not legal as a binding name in Cobrust.
    Underscore,

    // ---- Layout --------------------------------------------------------
    Newline,
    Indent,
    Dedent,
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.classify())
    }
}

impl TokenKind {
    /// Short, stable name for diagnostics. Distinct from `Display`
    /// only in that we never include the *value* (e.g. "Ident" not
    /// "Ident(foo)") — diagnostics quote spans, not text.
    #[must_use]
    pub fn classify(&self) -> &'static str {
        match self {
            Self::Ident(_) => "identifier",
            Self::Int(_) => "integer literal",
            Self::Float(_) => "float literal",
            Self::Imag(_) => "imaginary literal",
            Self::Str { .. } => "string literal",
            Self::Bytes { .. } => "bytes literal",
            Self::FString { .. } => "f-string literal",
            Self::KwAnd => "`and`",
            Self::KwAs => "`as`",
            Self::KwAwait => "`await`",
            Self::KwBreak => "`break`",
            Self::KwCase => "`case`",
            Self::KwClass => "`class`",
            Self::KwContinue => "`continue`",
            Self::KwElif => "`elif`",
            Self::KwElse => "`else`",
            Self::KwExcept => "`except`",
            Self::KwFalse => "`False`",
            Self::KwFinally => "`finally`",
            Self::KwFn => "`fn`",
            Self::KwFor => "`for`",
            Self::KwFrom => "`from`",
            Self::KwIf => "`if`",
            Self::KwImport => "`import`",
            Self::KwIn => "`in`",
            Self::KwLambda => "`lambda`",
            Self::KwLet => "`let`",
            Self::KwMatch => "`match`",
            Self::KwNone => "`None`",
            Self::KwNot => "`not`",
            Self::KwOr => "`or`",
            Self::KwPass => "`pass`",
            Self::KwRaise => "`raise`",
            Self::KwReturn => "`return`",
            Self::KwTrue => "`True`",
            Self::KwTry => "`try`",
            Self::KwType => "`type`",
            Self::KwWhile => "`while`",
            Self::KwWith => "`with`",
            Self::KwYield => "`yield`",
            Self::LParen => "`(`",
            Self::RParen => "`)`",
            Self::LBracket => "`[`",
            Self::RBracket => "`]`",
            Self::LBrace => "`{`",
            Self::RBrace => "`}`",
            Self::Comma => "`,`",
            Self::Colon => "`:`",
            Self::Semicolon => "`;`",
            Self::Dot => "`.`",
            Self::DotDotDot => "`...`",
            Self::Arrow => "`->`",
            Self::At => "`@`",
            Self::Eq => "`=`",
            Self::Walrus => "`:=`",
            Self::Plus => "`+`",
            Self::Minus => "`-`",
            Self::Star => "`*`",
            Self::StarStar => "`**`",
            Self::Slash => "`/`",
            Self::SlashSlash => "`//`",
            Self::Percent => "`%`",
            Self::Amp => "`&`",
            Self::Pipe => "`|`",
            Self::Caret => "`^`",
            Self::Tilde => "`~`",
            Self::Shl => "`<<`",
            Self::Shr => "`>>`",
            Self::EqEq => "`==`",
            Self::NotEq => "`!=`",
            Self::Lt => "`<`",
            Self::LtEq => "`<=`",
            Self::Gt => "`>`",
            Self::GtEq => "`>=`",
            Self::PlusEq => "`+=`",
            Self::MinusEq => "`-=`",
            Self::StarEq => "`*=`",
            Self::StarStarEq => "`**=`",
            Self::SlashEq => "`/=`",
            Self::SlashSlashEq => "`//=`",
            Self::PercentEq => "`%=`",
            Self::AmpEq => "`&=`",
            Self::PipeEq => "`|=`",
            Self::CaretEq => "`^=`",
            Self::ShlEq => "`<<=`",
            Self::ShrEq => "`>>=`",
            Self::Underscore => "`_`",
            Self::Newline => "newline",
            Self::Indent => "indent",
            Self::Dedent => "dedent",
            Self::Eof => "end of input",
        }
    }
}

/// String / bytes literal prefix flags.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct StrPrefix {
    pub raw: bool,
    pub bytes: bool,
}

/// One segment of an f-string, in source order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FStringPiece {
    /// Literal text between `{...}` (escapes resolved). Empty allowed.
    Lit(String),
    /// An interpolation: source text of the embedded expression, the
    /// optional debug-equals form (`{x=}`), and an optional format
    /// spec (text between `:` and the closing `}`). The expression
    /// text is later re-lexed and re-parsed by the parser.
    Expr {
        source: String,
        debug_equals: bool,
        format_spec: Option<String>,
    },
}
