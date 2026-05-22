//! `cobrust-frontend` — lexer, parser, and AST for Cobrust source.
//!
//! Delivered at M1 (the "core 30 forms"). See
//! [`docs/agent/modules/frontend.md`](https://github.com/Cobrust-lang/cobrust)
//! for the agent-facing spec and `docs/agent/adr/0003-core-30-forms.md`
//! for the syntactic surface this crate accepts.
//!
//! The crate exposes three top-level entrypoints:
//!
//! - [`lex`] — UTF-8 source → token stream
//! - [`parse`] — token stream → [`ast::Module`]
//! - [`parse_str`] — convenience composition of the two
//!
//! Plus an unparser for round-trip testing:
//!
//! - [`unparse`] — [`ast::Module`] → canonical source form
//!
//! Spans on every AST node are `(file_id, byte_start, byte_end)`. The
//! crate has zero panic paths reachable from any byte input — invalid
//! UTF-8 is reported as a [`LexError`], not a panic.

#![forbid(unsafe_code)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::similar_names)]
#![allow(clippy::redundant_else)]
#![allow(clippy::unused_self)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::single_match_else)]

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod prelude;
pub mod span;
pub mod token;
pub mod unparse;

pub use error::{FrontendError, LexError, ParseError};
pub use lexer::lex;
pub use parser::parse;
pub use prelude::{PRELUDE, PRELUDE_BYTE_LEN, PRELUDE_LINE_COUNT};
pub use span::{FileId, Span};
pub use token::{Token, TokenKind};
pub use unparse::unparse;

/// One-shot helper: lex then parse a source string.
///
/// # Errors
///
/// Returns [`FrontendError::Lex`] on lexer failures (invalid UTF-8,
/// malformed numeric literal, unterminated string, indentation error)
/// or [`FrontendError::Parse`] on syntactic failures.
pub fn parse_str(source: &str, file_id: FileId) -> Result<ast::Module, FrontendError> {
    let tokens = lex(source, file_id).map_err(FrontendError::Lex)?;
    parse(&tokens).map_err(FrontendError::Parse)
}
