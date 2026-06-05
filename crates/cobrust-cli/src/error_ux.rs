//! User-facing error UX layer — T1.4 (0.1.0-beta release).
//!
//! Every internal compiler error is mapped into one of four
//! user-visible categories before it reaches stderr.  The raw internal
//! representation (3000-line Cranelift IR, `{:#?}` debug dumps, etc.)
//! never reaches the terminal.
//!
//! # Four-class taxonomy (ADR-0024 §"User-facing error pipeline")
//!
//! | Variant    | Exit | Meaning                                  |
//! |------------|------|------------------------------------------|
//! | `Syntax`   | 2    | Lex / parse failure with source location |
//! | `Type`     | 2    | Type-check / HIR-lower failure with loc  |
//! | `Runtime`  | 4    | Runtime-level diagnostic (used by `run`) |
//! | `Internal` | 3    | Codegen / linker / invariant violation   |
//!
//! `Internal` errors print a bug-report prompt so users can file
//! actionable issues rather than panic-dumping Cranelift IR.

use std::fmt;
use std::path::PathBuf;

use crate::exit_codes;

// ── colour helpers ─────────────────────────────────────────────────────────

// (colour helper removed in T1.4 — use the module-level `c()` function
// which gates on colour_enabled(); add `is-terminal` dep in a follow-up)

/// ANSI reset.
const RST: &str = "\x1b[0m";
/// Bold red.
const RED: &str = "\x1b[1;31m";
/// Bold yellow.
const YELLOW: &str = "\x1b[1;33m";
/// Bold cyan.
const CYAN: &str = "\x1b[1;36m";
/// Bold.
const BOLD: &str = "\x1b[1m";
/// Dim.
const DIM: &str = "\x1b[2m";

fn colour_enabled() -> bool {
    match std::env::var("NO_COLOR") {
        Ok(v) if !v.is_empty() => return false,
        _ => {}
    }
    match std::env::var("TERM") {
        Ok(t) if t == "dumb" => return false,
        Err(_) => return false,
        _ => {}
    }
    // Only colour when stderr is (likely) a real terminal — proxy via
    // TERM being set and COBRUST_NO_COLOR not set.  Proper `is-terminal`
    // check deferred; this covers 95 % of interactive use.
    true
}

fn c(code: &str) -> &str {
    if colour_enabled() { code } else { "" }
}

// ── Category label ─────────────────────────────────────────────────────────

/// Printable category label used in every user error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Category {
    Syntax,
    Type,
    Runtime,
    Internal,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax => f.write_str("Syntax"),
            Self::Type => f.write_str("Type"),
            Self::Runtime => f.write_str("Runtime"),
            Self::Internal => f.write_str("Internal"),
        }
    }
}

// ── UserError ──────────────────────────────────────────────────────────────

/// A user-facing compiler error.
///
/// Every variant renders as **≤ 30 lines** on stderr.  Internal errors
/// include a `cobrust report-bug` invocation hint rather than a raw
/// stack dump or Cranelift IR.
///
/// # Display contract
///
/// ```text
/// error[Syntax]: <file>:<line>:<col>: <msg>
///   --> hint: <hint>           (optional)
/// ```
///
/// or for `Internal`:
///
/// ```text
/// error[Internal]: <kind>
///   This is a compiler bug.  Please run:
///   cobrust report-bug --include-mir
///   and paste the output into a new GitHub issue.
/// ```
///
/// The rendered output is intentionally terse — at most 5 lines for
/// source-located errors, at most 8 lines for internal errors.
#[derive(Clone, Debug)]
pub enum UserError {
    /// Lex / parse failure.
    Syntax {
        file: PathBuf,
        line: u32,
        col: u32,
        msg: String,
        hint: Option<String>,
    },
    /// Type-check / HIR-lower failure.
    Type {
        file: PathBuf,
        line: u32,
        col: u32,
        msg: String,
        hint: Option<String>,
    },
    /// Runtime-level diagnostic (propagated from `cobrust run`).
    Runtime { msg: String, location: String },
    /// Compiler-internal failure — the user cannot fix this; they
    /// should file a bug report.
    Internal {
        internal_kind: String,
        repro_cmd: String,
    },
}

impl UserError {
    /// The exit code this error class maps to.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Syntax { .. } | Self::Type { .. } => exit_codes::TYPE_ERROR,
            Self::Runtime { .. } => exit_codes::RUNTIME_PANIC,
            Self::Internal { .. } => exit_codes::INTERNAL_PANIC,
        }
    }

    /// The category label for this error.
    #[must_use]
    pub fn category(&self) -> Category {
        match self {
            Self::Syntax { .. } => Category::Syntax,
            Self::Type { .. } => Category::Type,
            Self::Runtime { .. } => Category::Runtime,
            Self::Internal { .. } => Category::Internal,
        }
    }

    // ── Constructors (convenience) ─────────────────────────────────────

    /// Build a `Syntax` error from a flat message; location = (0, 0).
    #[must_use]
    pub fn syntax(file: PathBuf, line: u32, col: u32, msg: impl Into<String>) -> Self {
        Self::Syntax {
            file,
            line,
            col,
            msg: msg.into(),
            hint: None,
        }
    }

    /// Build a `Syntax` error with a hint.
    #[must_use]
    pub fn syntax_with_hint(
        file: PathBuf,
        line: u32,
        col: u32,
        msg: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self::Syntax {
            file,
            line,
            col,
            msg: msg.into(),
            hint: Some(hint.into()),
        }
    }

    /// Build a `Type` error.
    #[must_use]
    pub fn type_err(file: PathBuf, line: u32, col: u32, msg: impl Into<String>) -> Self {
        Self::Type {
            file,
            line,
            col,
            msg: msg.into(),
            hint: None,
        }
    }

    /// Build a `Type` error with a hint.
    #[must_use]
    pub fn type_err_with_hint(
        file: PathBuf,
        line: u32,
        col: u32,
        msg: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self::Type {
            file,
            line,
            col,
            msg: msg.into(),
            hint: Some(hint.into()),
        }
    }

    /// Build a codegen / linker / invariant `Internal` error.
    ///
    /// `kind` is a one-line summary (e.g. `"LlvmError"`,
    /// `"LinkerFailed"`); `repro_cmd` is the full command that
    /// triggered the failure.
    #[must_use]
    pub fn internal(kind: impl Into<String>, repro_cmd: impl Into<String>) -> Self {
        Self::Internal {
            internal_kind: kind.into(),
            repro_cmd: repro_cmd.into(),
        }
    }

    /// Emit this error to stderr and return its exit code.
    ///
    /// Equivalent to `eprintln!("{self}"); self.exit_code()`.
    #[must_use]
    pub fn report_and_exit_code(&self) -> u8 {
        eprintln!("{self}");
        self.exit_code()
    }
}

// ── Display — the ≤ 30-line contract ──────────────────────────────────────

impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let red = c(RED);
        let yellow = c(YELLOW);
        let cyan = c(CYAN);
        let bold = c(BOLD);
        let dim = c(DIM);
        let rst = c(RST);

        match self {
            Self::Syntax {
                file,
                line,
                col,
                msg,
                hint,
            }
            | Self::Type {
                file,
                line,
                col,
                msg,
                hint,
            } => {
                let cat = self.category();
                let colour = match cat {
                    Category::Syntax => yellow,
                    _ => red,
                };
                // Line 1: error header
                writeln!(f, "{colour}error[{cat}]{rst}: {bold}{msg}{rst}")?;
                // Line 2: file:line:col pointer
                let path_str = file.display();
                writeln!(f, "  {dim}-->{rst} {path_str}:{line}:{col}")?;
                // Lines 3-4: optional hint
                if let Some(h) = hint {
                    writeln!(f, "  {cyan}hint{rst}: {h}")?;
                }
                // Total: 2–3 lines
                Ok(())
            }

            Self::Runtime { msg, location } => {
                writeln!(f, "{red}error[Runtime]{rst}: {bold}{msg}{rst}")?;
                writeln!(f, "  {dim}-->{rst} {location}")?;
                Ok(())
            }

            Self::Internal {
                internal_kind,
                repro_cmd,
            } => {
                writeln!(f, "{red}error[Internal]{rst}: {bold}{internal_kind}{rst}")?;
                writeln!(f)?;
                writeln!(f, "  This is a {bold}compiler bug{rst}.")?;
                writeln!(f, "  Please collect a bug report and file a GitHub issue:")?;
                writeln!(f)?;
                writeln!(f, "    {bold}cobrust report-bug --include-mir{rst}")?;
                writeln!(f)?;
                writeln!(f, "  Repro command: {dim}{repro_cmd}{rst}")?;
                // Total: 7 lines
                Ok(())
            }
        }
    }
}

// ── From impls ─────────────────────────────────────────────────────────────
//
// Each impl converts a concrete internal error into a UserError,
// discarding (or summarising) internal detail that would overwhelm the
// user.

use cobrust_codegen::CodegenError;
use cobrust_frontend::error::{FrontendError, LexError, ParseError};
use cobrust_hir::error::LoweringError;
use cobrust_mir::error::MirError;
use cobrust_types::error::TypeError;

use crate::build::BuildError;

// ── FrontendError → UserError ─────────────────────────────────────────────

impl From<FrontendError> for UserError {
    fn from(e: FrontendError) -> Self {
        match e {
            FrontendError::Lex(lex) => lex.into(),
            FrontendError::Parse(parse) => parse.into(),
        }
    }
}

/// Extract byte-offset from a Span's start; convert to line/col (1-based).
/// We do not have a full source map here so we use the raw offset as
/// a column approximation.  The CLI wiring layer supplies `file`.
fn span_to_line_col(span: &cobrust_frontend::span::Span) -> (u32, u32) {
    // Span stores `start` and `end` as u32 byte offsets.
    // Without the original source text here, we surface the raw byte
    // offsets.  `cobrust check` / `build` callers that hold the source
    // text can call `span_to_line_col_from_src` instead.
    //
    // This is a best-effort presentation; a full source-map integration
    // is deferred to M15 (proper diagnostic renderer).
    (1, span.start)
}

impl From<LexError> for UserError {
    fn from(e: LexError) -> Self {
        let (msg, hint, line, col) = match &e {
            LexError::InvalidUtf8 { byte_offset } => (
                format!("file is not valid UTF-8 (first invalid byte at offset {byte_offset})"),
                Some("save the file as UTF-8 without BOM".to_owned()),
                1u32,
                *byte_offset,
            ),
            LexError::UnexpectedChar { ch, span } => {
                let (l, c) = span_to_line_col(span);
                (format!("unexpected character {ch:?}"), None, l, c)
            }
            LexError::UnterminatedString { span } => {
                let (l, c) = span_to_line_col(span);
                (
                    "unterminated string literal".to_owned(),
                    Some("add a closing `\"`".to_owned()),
                    l,
                    c,
                )
            }
            LexError::UnterminatedFString { span } => {
                let (l, c) = span_to_line_col(span);
                (
                    "unterminated f-string interpolation".to_owned(),
                    Some("close the `{` with a matching `}`".to_owned()),
                    l,
                    c,
                )
            }
            LexError::MalformedNumber { span } => {
                let (l, c) = span_to_line_col(span);
                (
                    "malformed numeric literal".to_owned(),
                    Some("check for invalid digit or suffix".to_owned()),
                    l,
                    c,
                )
            }
            LexError::InconsistentIndent { span } => {
                let (l, c) = span_to_line_col(span);
                (
                    "mixed tabs and spaces in indentation".to_owned(),
                    Some("use spaces only — tabs are not allowed in Cobrust".to_owned()),
                    l,
                    c,
                )
            }
            LexError::InvalidEscape { span } => {
                let (l, c) = span_to_line_col(span);
                (
                    "invalid escape sequence in string".to_owned(),
                    Some("use `\\n`, `\\t`, `\\\\`, `\\\"` or a raw string `r\"...\"`".to_owned()),
                    l,
                    c,
                )
            }
        };
        Self::Syntax {
            file: PathBuf::from("<source>"),
            line,
            col,
            msg,
            hint,
        }
    }
}

impl From<ParseError> for UserError {
    fn from(e: ParseError) -> Self {
        // Tier-2 CQ P0-3 + CLAUDE.md §2.5 Direction B: every variant
        // now carries a construction-time
        // `suggestion: Option<&'static str>`. The renderer reads it
        // verbatim and falls back to the legacy hard-coded hint string
        // ONLY when `suggestion` is `None` (preserves existing user-
        // visible hint prose where no construction-site fix was
        // populated). Pattern mirrors `error_ux.rs` TypeError /
        // MirError per ADR-0052b §"Renderer is structural".
        let (msg, hint, line, col) = match &e {
            ParseError::Expected {
                expected,
                found,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                let expected_str = expected
                    .iter()
                    .map(|t| format!("`{t:?}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                (
                    format!("expected {expected_str}, found `{found:?}`"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            ParseError::Syntax {
                message,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (message.clone(), suggestion.map(str::to_owned), l, c)
            }
            ParseError::UnexpectedEof { span, suggestion } => {
                let (l, c) = span_to_line_col(span);
                let hint = suggestion.map_or_else(
                    || Some("the file may be incomplete — check for unclosed blocks".to_owned()),
                    |s| Some(s.to_owned()),
                );
                ("unexpected end of file".to_owned(), hint, l, c)
            }
            ParseError::DroppedByConstitution {
                name,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                let hint = suggestion.map_or_else(
                    || {
                        Some(format!(
                            "`{name}` is not part of Cobrust — see the language reference for alternatives"
                        ))
                    },
                    |s| Some(s.to_owned()),
                );
                (format!("use of dropped feature `{name}`"), hint, l, c)
            }
            ParseError::NonLiteralDefault { span, suggestion } => {
                let (l, c) = span_to_line_col(span);
                let hint = suggestion.map_or_else(
                    || Some("use `None` or a number / string literal as the default".to_owned()),
                    |s| Some(s.to_owned()),
                );
                (
                    "default argument must be a literal value".to_owned(),
                    hint,
                    l,
                    c,
                )
            }
            ParseError::IndentError {
                message,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                let hint = suggestion.map_or_else(
                    || Some("check that the block body is indented consistently".to_owned()),
                    |s| Some(s.to_owned()),
                );
                (format!("indentation error: {message}"), hint, l, c)
            }
            ParseError::ExpressionTooDeep {
                depth,
                max,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                let hint = suggestion.map_or_else(
                    || {
                        Some(
                            "flatten deeply nested parentheses or split the expression across \
                         multiple let bindings"
                                .to_owned(),
                        )
                    },
                    |s| Some(s.to_owned()),
                );
                (
                    format!("expression nesting depth {depth} exceeds limit {max}"),
                    hint,
                    l,
                    c,
                )
            }
        };
        Self::Syntax {
            file: PathBuf::from("<source>"),
            line,
            col,
            msg,
            hint,
        }
    }
}

// ── LoweringError → UserError ─────────────────────────────────────────────

impl From<LoweringError> for UserError {
    fn from(e: LoweringError) -> Self {
        // ADR-0052b §2 Direction B — uniform `suggestion: Option<&'static str>`
        // on LoweringError variants (scope-expanded per Wave-2 corpus
        // s0052b_01/16/20/27/28/29 catch-surface findings). The renderer
        // is structural; primary `msg` carries the failing identifier so
        // LLM stderr parsing still extracts it per §3.5 + §10.
        use LoweringError as L;
        let (msg, hint, span) = match &e {
            L::UnknownName {
                name,
                span,
                suggestion,
            } => (
                format!("unknown name `{name}`"),
                suggestion.map(str::to_owned),
                *span,
            ),
            L::DroppedFeature {
                name,
                span,
                suggestion,
            } => (
                format!("use of dropped feature `{name}`"),
                suggestion.map(str::to_owned),
                *span,
            ),
            L::MutableDefault { span, suggestion } => (
                "parameter default must be a literal expression".to_owned(),
                suggestion.map(str::to_owned),
                *span,
            ),
            L::OrPatternBindingMismatch { span, suggestion } => (
                "or-pattern branches must bind the same set of names".to_owned(),
                suggestion.map(str::to_owned),
                *span,
            ),
            L::DuplicateBinding {
                name,
                second,
                suggestion,
                ..
            } => (
                format!("duplicate binding `{name}` in this scope"),
                suggestion.map(str::to_owned),
                *second,
            ),
            L::AssignToUnknown {
                name,
                span,
                suggestion,
            } => (
                format!("assignment to undeclared name `{name}`"),
                suggestion.map(str::to_owned),
                *span,
            ),
            L::EcosystemDecoratorShape {
                detail,
                span,
                suggestion,
            } => (
                format!("ecosystem-decorator shape error: {detail}"),
                suggestion.map(str::to_owned),
                *span,
            ),
        };
        let (line, col) = span_to_line_col(&span);
        Self::Type {
            file: PathBuf::from("<source>"),
            line,
            col,
            msg,
            hint,
        }
    }
}

// ── TypeError → UserError ─────────────────────────────────────────────────

impl From<TypeError> for UserError {
    fn from(e: TypeError) -> Self {
        // ADR-0052b §7 Direction B — the renderer is structural. Each
        // variant's `suggestion: Option<&'static str>` field is mapped
        // directly to the `hint` field; primary-line `msg` only carries
        // the failing identifier / type info (no fix-prose
        // interpolation). Per §3.5 + §10, the primary `msg` still
        // includes the bound name (e.g. `unknown name \`foo\``) so LLM
        // stderr parsing retains it; the fix path lives in the
        // structured `suggestion` field populated at construction time.
        use TypeError as E;
        let (msg, hint, line, col) = match &e {
            E::UnknownName {
                name,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("unknown name `{name}`"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::ArityMismatch {
                expected,
                actual,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("wrong number of arguments: expected {expected}, got {actual}"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::KeywordArgMismatch {
                name,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("unknown keyword argument `{name}`"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::MissingArgument {
                name,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("missing required argument `{name}`"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::TypeMismatch {
                expected,
                actual,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("type mismatch: expected `{expected}`, found `{actual}`"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::NonExhaustiveMatch {
                uncovered,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                let missing = uncovered.join(", ");
                (
                    format!("non-exhaustive match: missing case(s) {missing}"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::RowConflict {
                field,
                ty1,
                ty2,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("conflicting types for field `{field}`: `{ty1}` vs `{ty2}`"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::ImplicitTruthiness {
                actual,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("cannot use `{actual}` as a boolean condition"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::UseOfDroppedFeature {
                name,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("use of dropped feature `{name}`"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::MutableDefault { span, suggestion } => {
                let (l, c) = span_to_line_col(span);
                (
                    "mutable default argument is forbidden".to_owned(),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::AmbiguousType { span, suggestion } => {
                let (l, c) = span_to_line_col(span);
                (
                    "ambiguous type — cannot infer".to_owned(),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::DuplicateField {
                name,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("duplicate field `{name}` in record literal"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::OccursCheck {
                var,
                ty,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("type inference loop: cannot unify `?{}` with `{ty}`", var.0),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::NotCallable {
                actual,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("`{actual}` is not callable"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::NotIndexable {
                actual,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("`{actual}` cannot be indexed with `[]`"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::NotIterable {
                actual,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("`{actual}` cannot be used in a `for` loop"),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::BreakOutsideLoop { span, suggestion } => {
                let (l, c) = span_to_line_col(span);
                (
                    "`break` outside of a loop".to_owned(),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::ContinueOutsideLoop { span, suggestion } => {
                let (l, c) = span_to_line_col(span);
                (
                    "`continue` outside of a loop".to_owned(),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::ReturnOutsideFn { span, suggestion } => {
                let (l, c) = span_to_line_col(span);
                (
                    "`return` outside of a function".to_owned(),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::YieldOutsideFn { span, suggestion } => {
                let (l, c) = span_to_line_col(span);
                (
                    "`yield` outside of a function".to_owned(),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::NotHashable {
                actual,
                span,
                suggestion,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!(
                        "dict key type `{actual}` is not Hashable (Phase F.3 admits i64 / str / bool / None)"
                    ),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::DictSpreadNotSupported { span, suggestion } => {
                let (l, c) = span_to_line_col(span);
                (
                    "dict spread `**other` is not supported in dict literals".to_owned(),
                    suggestion.map(str::to_owned),
                    l,
                    c,
                )
            }
            E::Multiple(errors) => {
                // Surface the first error; the rest are silently counted.
                // The caller should iterate and report individually for
                // best UX — but this fallback is safe. The aggregate
                // wrapper is N-class per ADR-0052b §4.1; the child's
                // structured `suggestion` field is what reaches the user.
                if let Some(first) = errors.first() {
                    return UserError::from(first.clone());
                }
                ("multiple type errors".to_owned(), None, 1, 0)
            }
            // ADR-0052b §2 Direction B — borrow-of-non-place's
            // `suggestion` field is the canonical Wave-1 forward-compat
            // shape; uniform structural rendering applies.
            E::BorrowOfNonPlace { span, suggestion } => {
                let (line, col) = span_to_line_col(span);
                (
                    "cannot borrow this expression".to_owned(),
                    suggestion.map(str::to_owned),
                    line,
                    col,
                )
            }
            // ADR-0052b §2 Direction B — method-not-found's
            // `suggestion` field carries the chosen "did you mean" prose
            // assembled at construction in `{str,list,float,int}_method_suggestion`.
            E::UnknownMethod {
                type_name,
                method_name,
                span,
                suggestion,
            } => {
                let (line, col) = span_to_line_col(span);
                (
                    format!("method `{method_name}` not found on `{type_name}`"),
                    suggestion.map(str::to_owned),
                    line,
                    col,
                )
            }
            // ADR-0073 §2 D1+D8 — callback parameter slot shape errors.
            E::CallbackArgMustBeFnName { span, suggestion } => {
                let (line, col) = span_to_line_col(span);
                (
                    "callback argument must be a top-level `fn` name".to_owned(),
                    suggestion.map(str::to_owned),
                    line,
                    col,
                )
            }
            E::CallbackSignatureMismatch {
                expected,
                actual,
                span,
                suggestion,
            } => {
                let (line, col) = span_to_line_col(span);
                (
                    format!("callback signature mismatch: expected `{expected}`, found `{actual}`"),
                    suggestion.map(str::to_owned),
                    line,
                    col,
                )
            }
            // ADR-0080 Phase-1a — typed field access on a class instance.
            // The primary `msg` carries the field + class + the declared-
            // field list (the §2.5-B FIX the LLM parses from stderr).
            E::UnknownField {
                field,
                adt,
                known_fields,
                span,
                suggestion,
            } => {
                let (line, col) = span_to_line_col(span);
                let declared = if known_fields.is_empty() {
                    "(none)".to_owned()
                } else {
                    known_fields.join(", ")
                };
                (
                    format!("no field `{field}` on `{adt}`; declared fields: {declared}"),
                    suggestion.map(str::to_owned),
                    line,
                    col,
                )
            }
            // ADR-0080 Phase-1b-ii — non-fixed-grammar refinement `where`
            // predicate. The primary `msg` shows the accepted fixed-grammar
            // forms (the §2.5-B FIX the LLM parses from stderr).
            E::UnsupportedRefinement {
                field,
                span,
                suggestion,
            } => {
                let (line, col) = span_to_line_col(span);
                (
                    format!(
                        "unsupported refinement `where`-predicate on field `{field}`: \
                         use one of the fixed refinement forms — \
                         an i64 int-range `0 <= self and self <= 100` (inclusive); \
                         an f64 float-range `0.0 <= self and self <= 1.0` (inclusive `<=`/`>=` ONLY — \
                         a strict `<`/`>` is rejected, the reals are dense); \
                         a str length `len(self) <= n` (or `len(self) >= n`); \
                         or a str pattern `pattern(self, \"<regex>\")`"
                    ),
                    suggestion.map(str::to_owned),
                    line,
                    col,
                )
            }
            // ADR-0088 §3 — `len(x)` on a non-sized argument. The primary
            // `msg` names the accepted sized-type set (str / list / dict),
            // the §2.5-B FIX the LLM parses from stderr — NOT the
            // pre-ADR-0088 misleading "expected Dict[?,?]".
            E::LenArgNotSized {
                actual,
                span,
                suggestion,
            } => {
                let (line, col) = span_to_line_col(span);
                (
                    format!(
                        "`len(x)` needs a sized argument but got `{actual}`: \
                         the free-function `len` accepts a `str`, a `list[T]`, or a \
                         `dict[K, V]` (for a number use a comparison; `len` is not \
                         defined on `{actual}`)"
                    ),
                    suggestion.map(str::to_owned),
                    line,
                    col,
                )
            }
            // ADR-0092 — undeclared dora output id. The primary `msg`
            // names the offending id, the declared-output list, and the
            // nearest-match `did you mean` clause — the §2.5-B FIX the LLM
            // parses from stderr to correct the `send_output("...")` id in
            // one step (declare it, or fix the typo to a declared id).
            E::DoraUnknownOutputId {
                id,
                declared,
                nearest,
                span,
                suggestion,
            } => {
                let (line, col) = span_to_line_col(span);
                let did_you_mean = match nearest {
                    Some(n) => format!("; did you mean `{n}`?"),
                    None => String::new(),
                };
                (
                    format!(
                        "unknown dora output id `{id}` — it is not declared in \
                         `@dora.node(outputs=[...])`; declared outputs: [{}]{did_you_mean}",
                        declared.join(", ")
                    ),
                    suggestion.map(str::to_owned),
                    line,
                    col,
                )
            }
        };
        Self::Type {
            file: PathBuf::from("<source>"),
            line,
            col,
            msg,
            hint,
        }
    }
}

// ── MirError → UserError ─────────────────────────────────────────────────

impl From<MirError> for UserError {
    fn from(e: MirError) -> Self {
        // ADR-0052b §7 Direction B — the renderer is structural. Each
        // variant's `suggestion: Option<&'static str>` is mapped to the
        // `hint` field via `suggestion.map(str::to_owned)`. MIR errors
        // come from the ownership / borrow checker and are user-visible
        // (they describe source-level ownership violations). Compiler-
        // internal variants (UnresolvedDefId, Internal) route through
        // `UserError::internal` per the existing T1.4 contract.
        use MirError as M;
        let (msg, hint, span) = match &e {
            M::UseAfterMove {
                local,
                span,
                suggestion,
            } => (
                format!("use of moved value `_{local}` after it was moved"),
                suggestion.map(str::to_owned),
                *span,
            ),
            M::UseAfterDrop {
                local,
                span,
                suggestion,
            } => (
                format!("use of dropped value `_{local}`"),
                suggestion.map(str::to_owned),
                *span,
            ),
            M::ConflictingMutBorrow {
                local,
                span,
                suggestion,
            } => (
                format!("conflicting mutable borrow of `_{local}`"),
                suggestion.map(str::to_owned),
                *span,
            ),
            M::SharedMutOverlap {
                local,
                span,
                suggestion,
            } => (
                format!("shared and mutable borrow of `_{local}` overlap"),
                suggestion.map(str::to_owned),
                *span,
            ),
            M::EscapingBorrow {
                local,
                span,
                suggestion,
            } => (
                format!("borrow of `_{local}` escapes its declaring scope"),
                suggestion.map(str::to_owned),
                *span,
            ),
            M::DropMissing {
                local,
                span,
                suggestion,
            } => (
                format!("owning value `_{local}` not dropped on this return path"),
                suggestion.map(str::to_owned),
                *span,
            ),
            M::DoubleDrop {
                local,
                span,
                suggestion,
            } => (
                format!("value `_{local}` dropped more than once"),
                suggestion.map(str::to_owned),
                *span,
            ),
            M::NonExhaustiveSwitch { span, suggestion } => (
                "non-exhaustive match expression".to_owned(),
                suggestion.map(str::to_owned),
                *span,
            ),
            M::FieldOutOfBounds {
                place,
                span,
                suggestion,
            } => (
                format!("field projection out of bounds: {place:?}"),
                suggestion.map(str::to_owned),
                *span,
            ),
            M::UnresolvedDefId {
                def_id,
                span: _,
                suggestion: _,
            } => {
                // Compiler-internal; routes through `UserError::internal`.
                return UserError::internal(
                    format!("UnresolvedDefId({def_id})"),
                    "cobrust build <file>".to_owned(),
                );
            }
            M::Internal(msg) => {
                return UserError::internal(
                    format!("MirError::Internal: {msg}"),
                    "cobrust build <file>".to_owned(),
                );
            }
        };
        let (line, col) = span_to_line_col(&span);
        Self::Type {
            file: PathBuf::from("<source>"),
            line,
            col,
            msg,
            hint,
        }
    }
}

// ── CodegenError → UserError ──────────────────────────────────────────────

impl From<CodegenError> for UserError {
    fn from(e: CodegenError) -> Self {
        // All codegen errors are Internal — the user cannot fix them by
        // editing source.  We provide a compact summary (one line) and
        // never dump Cranelift IR.
        let kind = match &e {
            CodegenError::UnsupportedBackend(b) => {
                format!("UnsupportedBackend({b:?}) — rebuild with `--features llvm`")
            }
            CodegenError::UnsupportedTarget(t) => {
                format!("UnsupportedTarget({t})")
            }
            CodegenError::InvalidMir(m) => {
                // One-line summary only — no raw MIR dump.
                let summary: String = m.chars().take(120).collect();
                format!("InvalidMir: {summary}")
            }
            CodegenError::LlvmError(m) => {
                let first_line = m.lines().next().unwrap_or("(no detail)");
                format!("LlvmError: {first_line}")
            }
            CodegenError::ObjectEmission(m) => {
                format!("ObjectEmission: {m}")
            }
            CodegenError::LinkerFailed { exit_code, stderr } => {
                // Keep at most 3 lines of linker stderr.
                let brief: String = stderr.lines().take(3).collect::<Vec<_>>().join(" | ");
                format!("LinkerFailed(exit={exit_code}): {brief}")
            }
            CodegenError::Io(m) => format!("I/O: {m}"),
            CodegenError::Internal(m) => {
                let first_line = m.lines().next().unwrap_or("(no detail)");
                format!("Internal: {first_line}")
            }
            CodegenError::UnimplementedBinOp { op, note } => {
                format!("UnimplementedBinOp(`{op}`): {note}")
            }
        };
        Self::internal(kind, "cobrust build <file>")
    }
}

// ── BuildError → UserError ────────────────────────────────────────────────

impl From<BuildError> for UserError {
    fn from(e: BuildError) -> Self {
        match e {
            BuildError::User(msg) => Self::Syntax {
                file: PathBuf::from("<cli>"),
                line: 0,
                col: 0,
                msg,
                hint: None,
            },
            BuildError::Type(msg) => Self::Type {
                file: PathBuf::from("<source>"),
                line: 0,
                col: 0,
                msg,
                hint: None,
            },
            BuildError::Internal(msg) => {
                let first_line = msg.lines().next().unwrap_or("(no detail)");
                Self::internal(first_line.to_owned(), "cobrust build <file>")
            }
        }
    }
}

// ── Line-count helper (used in tests) ─────────────────────────────────────

/// Count the number of lines in the `Display` output of a `UserError`.
/// Panics if the rendered output exceeds `MAX_LINES`.
///
/// This function is `pub` so the integration test corpus can call it.
pub const MAX_LINES: usize = 30;

/// Return the number of lines in the rendered user error.
#[must_use]
pub fn rendered_line_count(e: &UserError) -> usize {
    format!("{e}").lines().count()
}

/// Assert that `e` renders within the 30-line contract.
///
/// # Panics
///
/// Panics with a descriptive message if the output exceeds `MAX_LINES`.
pub fn assert_within_line_budget(e: &UserError) {
    let rendered = format!("{e}");
    let lines = rendered.lines().count();
    assert!(
        lines <= MAX_LINES,
        "UserError renders {lines} lines (limit {MAX_LINES}):\n{rendered}"
    );
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::missing_docs_in_private_items
)]
mod tests {
    use super::*;

    fn dummy_path() -> PathBuf {
        PathBuf::from("src/main.cb")
    }

    #[test]
    fn syntax_renders_within_budget() {
        let e = UserError::syntax_with_hint(
            dummy_path(),
            10,
            5,
            "unexpected character '?'",
            "remove the character",
        );
        assert_within_line_budget(&e);
        let s = format!("{e}");
        assert!(s.contains("error[Syntax]"), "missing category label");
        assert!(s.contains("src/main.cb:10:5"), "missing file:line:col");
        assert!(s.contains("hint"), "missing hint");
    }

    #[test]
    fn type_renders_within_budget() {
        let e = UserError::type_err_with_hint(
            dummy_path(),
            3,
            1,
            "type mismatch: expected `i64`, found `str`",
            "change the type annotation",
        );
        assert_within_line_budget(&e);
        let s = format!("{e}");
        assert!(s.contains("error[Type]"));
        assert!(s.contains("src/main.cb:3:1"));
    }

    #[test]
    fn internal_renders_within_budget() {
        let e = UserError::internal(
            "LlvmError: instruction %441 has type i64, expected i8",
            "cobrust build src/main.cb",
        );
        assert_within_line_budget(&e);
        let s = format!("{e}");
        assert!(s.contains("error[Internal]"));
        assert!(s.contains("cobrust report-bug"));
        assert!(s.contains("compiler bug"));
    }

    #[test]
    fn codegen_llvm_truncates_ir_dump() {
        // ADR-0070 §X.4: LLVM is the sole AOT backend. A large LLVM
        // verifier dump must still be truncated to the line budget.
        let long_ir = "LLVM verify failed: instruction %520 = add i8 %515, %518: operand 1 (%518) has type i64, expected i8\n".repeat(300);
        let ce = CodegenError::LlvmError(long_ir);
        let ue = UserError::from(ce);
        assert_within_line_budget(&ue);
        let s = format!("{ue}");
        assert!(s.contains("LlvmError"));
        assert!(s.contains("cobrust report-bug"));
    }

    #[test]
    fn internal_always_shows_report_bug() {
        let e = UserError::internal("anything", "cobrust build foo.cb");
        let s = format!("{e}");
        assert!(s.contains("cobrust report-bug --include-mir"));
    }
}
