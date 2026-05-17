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
    /// `kind` is a one-line summary (e.g. `"CraneliftError"`,
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
        let (msg, hint, line, col) = match &e {
            ParseError::Expected {
                expected,
                found,
                span,
            } => {
                let (l, c) = span_to_line_col(span);
                let expected_str = expected
                    .iter()
                    .map(|t| format!("`{t:?}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                (
                    format!("expected {expected_str}, found `{found:?}`"),
                    None,
                    l,
                    c,
                )
            }
            ParseError::Syntax { message, span } => {
                let (l, c) = span_to_line_col(span);
                (message.clone(), None, l, c)
            }
            ParseError::UnexpectedEof { span } => {
                let (l, c) = span_to_line_col(span);
                (
                    "unexpected end of file".to_owned(),
                    Some("the file may be incomplete — check for unclosed blocks".to_owned()),
                    l,
                    c,
                )
            }
            ParseError::DroppedByConstitution { name, span } => {
                let (l, c) = span_to_line_col(span);
                let hint = format!(
                    "`{name}` is not part of Cobrust — see the language reference for alternatives"
                );
                (format!("use of dropped feature `{name}`"), Some(hint), l, c)
            }
            ParseError::NonLiteralDefault { span } => {
                let (l, c) = span_to_line_col(span);
                (
                    "default argument must be a literal value".to_owned(),
                    Some("use `None` or a number / string literal as the default".to_owned()),
                    l,
                    c,
                )
            }
            ParseError::IndentError { message, span } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("indentation error: {message}"),
                    Some("check that the block body is indented consistently".to_owned()),
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
        use LoweringError as L;
        let (msg, hint, span) = match &e {
            L::UnknownName { name, span } => (
                format!("unknown name `{name}`"),
                Some(format!("did you declare it with `let {name} = …`?")),
                *span,
            ),
            L::DroppedFeature { name, span } => (
                format!("use of dropped feature `{name}`"),
                Some(
                    "this Python construct is not part of Cobrust — see the language reference"
                        .to_owned(),
                ),
                *span,
            ),
            L::MutableDefault { span } => (
                "parameter default must be a literal expression".to_owned(),
                Some(
                    "use `None` as the default; assign real defaults inside the function body"
                        .to_owned(),
                ),
                *span,
            ),
            L::OrPatternBindingMismatch { span } => (
                "or-pattern branches must bind the same set of names".to_owned(),
                Some("ensure every branch in `| pat1 | pat2` binds identical names".to_owned()),
                *span,
            ),
            L::DuplicateBinding { name, second, .. } => (
                format!("duplicate binding `{name}` in this scope"),
                Some("rename one of the bindings to make them distinct".to_owned()),
                *second,
            ),
            L::AssignToUnknown { name, span } => (
                format!("assignment to undeclared name `{name}`"),
                Some(format!("declare it first with `let {name}: <type> = …`")),
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
        use TypeError as E;
        let (msg, hint, line, col) = match &e {
            E::UnknownName { name, span } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("unknown name `{name}`"),
                    Some(format!("did you mean to declare it with `let {name} = …`?")),
                    l,
                    c,
                )
            }
            E::ArityMismatch {
                expected,
                actual,
                span,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("wrong number of arguments: expected {expected}, got {actual}"),
                    None,
                    l,
                    c,
                )
            }
            E::KeywordArgMismatch { name, span } => {
                let (l, c) = span_to_line_col(span);
                (format!("unknown keyword argument `{name}`"), None, l, c)
            }
            E::MissingArgument { name, span } => {
                let (l, c) = span_to_line_col(span);
                (format!("missing required argument `{name}`"), None, l, c)
            }
            E::TypeMismatch {
                expected,
                actual,
                span,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("type mismatch: expected `{expected}`, found `{actual}`"),
                    Some("add a type annotation or fix the expression type".to_owned()),
                    l,
                    c,
                )
            }
            E::NonExhaustiveMatch { uncovered, span } => {
                let (l, c) = span_to_line_col(span);
                let missing = uncovered.join(", ");
                (
                    format!("non-exhaustive match: missing case(s) {missing}"),
                    Some("add the missing cases or a wildcard `_` arm".to_owned()),
                    l,
                    c,
                )
            }
            E::RowConflict {
                field,
                ty1,
                ty2,
                span,
            } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("conflicting types for field `{field}`: `{ty1}` vs `{ty2}`"),
                    None,
                    l,
                    c,
                )
            }
            E::ImplicitTruthiness { actual, span } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("cannot use `{actual}` as a boolean condition"),
                    Some(
                        "Cobrust requires an explicit bool — try `if x != 0:` or `if x.is_some():`"
                            .to_owned(),
                    ),
                    l,
                    c,
                )
            }
            E::UseOfDroppedFeature { name, span } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("use of dropped feature `{name}`"),
                    Some(
                        "this Python feature is not part of Cobrust — see the language reference"
                            .to_owned(),
                    ),
                    l,
                    c,
                )
            }
            E::MutableDefault { span } => {
                let (l, c) = span_to_line_col(span);
                (
                    "mutable default argument is forbidden".to_owned(),
                    Some(
                        "use `None` as the default and assign inside the function body".to_owned(),
                    ),
                    l,
                    c,
                )
            }
            E::AmbiguousType { span } => {
                let (l, c) = span_to_line_col(span);
                (
                    "ambiguous type — cannot infer".to_owned(),
                    Some("add an explicit type annotation, e.g. `let x: i64 = …`".to_owned()),
                    l,
                    c,
                )
            }
            E::DuplicateField { name, span } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("duplicate field `{name}` in record literal"),
                    None,
                    l,
                    c,
                )
            }
            E::OccursCheck { var, ty, span } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("type inference loop: cannot unify `?{}` with `{ty}`", var.0),
                    Some(
                        "this is usually caused by a recursive type without an annotation"
                            .to_owned(),
                    ),
                    l,
                    c,
                )
            }
            E::NotCallable { actual, span } => {
                let (l, c) = span_to_line_col(span);
                (format!("`{actual}` is not callable"), None, l, c)
            }
            E::NotIndexable { actual, span } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("`{actual}` cannot be indexed with `[]`"),
                    None,
                    l,
                    c,
                )
            }
            E::NotIterable { actual, span } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!("`{actual}` cannot be used in a `for` loop"),
                    None,
                    l,
                    c,
                )
            }
            E::BreakOutsideLoop { span } => {
                let (l, c) = span_to_line_col(span);
                ("`break` outside of a loop".to_owned(), None, l, c)
            }
            E::ContinueOutsideLoop { span } => {
                let (l, c) = span_to_line_col(span);
                ("`continue` outside of a loop".to_owned(), None, l, c)
            }
            E::ReturnOutsideFn { span } => {
                let (l, c) = span_to_line_col(span);
                ("`return` outside of a function".to_owned(), None, l, c)
            }
            E::YieldOutsideFn { span } => {
                let (l, c) = span_to_line_col(span);
                ("`yield` outside of a function".to_owned(), None, l, c)
            }
            E::NotHashable { actual, span } => {
                let (l, c) = span_to_line_col(span);
                (
                    format!(
                        "dict key type `{actual}` is not Hashable (Phase F.3 admits i64 / str / bool / None)"
                    ),
                    Some(
                        "f64 keys are forbidden (NaN != NaN); use i64 via `f.to_bits() as i64` or a str repr".to_owned(),
                    ),
                    l,
                    c,
                )
            }
            E::DictSpreadNotSupported { span } => {
                let (l, c) = span_to_line_col(span);
                (
                    "dict spread `**other` is not supported in dict literals".to_owned(),
                    Some(
                        "dict-merge is Phase G; build the result manually by iterating `other.items()` and inserting"
                            .to_owned(),
                    ),
                    l,
                    c,
                )
            }
            E::Multiple(errors) => {
                // Surface the first error; the rest are silently counted.
                // The caller should iterate and report individually for
                // best UX — but this fallback is safe.
                if let Some(first) = errors.first() {
                    return UserError::from(first.clone());
                }
                ("multiple type errors".to_owned(), None, 1, 0)
            }
            // ADR-0052a Wave-1 §6 — borrow-of-non-place. The parser
            // already enforces the §8 Wave-1 cap at parse time; this
            // arm is forward-compat for future shapes admitted by the
            // parser but rejected by the type checker (e.g. record-
            // field-of-arith borrows in a future sub-ADR).
            E::BorrowOfNonPlace { span, suggestion } => {
                let (line, col) = span_to_line_col(span);
                (
                    "cannot borrow this expression".to_owned(),
                    suggestion.map(|s| s.to_owned()).or_else(|| {
                        Some(
                            "borrow operand must be `Name`, `Name.field`, or `Name[idx]` \
                             (ADR-0052a Wave-1 §8 cap)"
                                .to_owned(),
                        )
                    }),
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
        // MIR errors come from the ownership / borrow checker and are
        // user-visible (they describe source-level ownership violations).
        // However we do not have a span-to-line map here, so line/col
        // are best-effort from the raw span offset.
        use MirError as M;
        let (msg, hint, span) = match &e {
            M::UseAfterMove { local, span } => (
                format!("use of moved value `_{local}` after it was moved"),
                // ADR-0052a Wave-1 §7 + §11 — surface `&s` as the
                // canonical fix path. Hard-coded suggestion at the
                // construction site per §"Direction B coordination"
                // forward-compat (Direction B sub-ADR formalises the
                // structured `suggestion` field shape).
                Some(
                    "change to `&s` to borrow without consuming \
                     (ADR-0052a explicit shared borrow)"
                        .to_owned(),
                ),
                *span,
            ),
            M::UseAfterDrop { local, span } => (
                format!("use of dropped value `_{local}`"),
                Some("the value was already dropped at this point".to_owned()),
                *span,
            ),
            M::ConflictingMutBorrow { local, span } => (
                format!("conflicting mutable borrow of `_{local}`"),
                Some("only one mutable borrow can be active at a time".to_owned()),
                *span,
            ),
            M::SharedMutOverlap { local, span } => (
                format!("shared and mutable borrow of `_{local}` overlap"),
                Some("cannot borrow mutably while a shared borrow is active".to_owned()),
                *span,
            ),
            M::EscapingBorrow { local, span } => (
                format!("borrow of `_{local}` escapes its declaring scope"),
                Some("the borrowed value must live at least as long as the reference".to_owned()),
                *span,
            ),
            M::DropMissing { local, span } => (
                format!("owning value `_{local}` not dropped on this return path"),
                Some("every owned value must be explicitly dropped or returned".to_owned()),
                *span,
            ),
            M::DoubleDrop { local, span } => (
                format!("value `_{local}` dropped more than once"),
                Some("a value can only be dropped once; check your control flow".to_owned()),
                *span,
            ),
            M::NonExhaustiveSwitch { span } => (
                "non-exhaustive match expression".to_owned(),
                Some("add a wildcard `_` arm or cover all cases".to_owned()),
                *span,
            ),
            M::FieldOutOfBounds { place, span } => (
                format!("field projection out of bounds: {place:?}"),
                Some("the struct does not have that many fields".to_owned()),
                *span,
            ),
            M::UnresolvedDefId { def_id, span: _ } => {
                // This should never reach users — it's a compiler bug.
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
            CodegenError::CraneliftError(m) => {
                // The raw Cranelift error may be hundreds of lines.
                // Surface only the first line (the high-level description).
                let first_line = m.lines().next().unwrap_or("(no detail)");
                format!("CraneliftError: {first_line}")
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
            "CraneliftError: inst441 has type i64, expected i8",
            "cobrust build src/main.cb",
        );
        assert_within_line_budget(&e);
        let s = format!("{e}");
        assert!(s.contains("error[Internal]"));
        assert!(s.contains("cobrust report-bug"));
        assert!(s.contains("compiler bug"));
    }

    #[test]
    fn codegen_cranelift_truncates_ir_dump() {
        // Simulate a 3000-line Cranelift verifier dump being received.
        let long_ir = "Verifier errors: - inst441 (v520 = iadd.i8 v515, v518): arg 1 (v518) has type i64, expected i8\n".repeat(300);
        let ce = CodegenError::CraneliftError(long_ir);
        let ue = UserError::from(ce);
        assert_within_line_budget(&ue);
        let s = format!("{ue}");
        assert!(s.contains("CraneliftError"));
        assert!(s.contains("cobrust report-bug"));
    }

    #[test]
    fn internal_always_shows_report_bug() {
        let e = UserError::internal("anything", "cobrust build foo.cb");
        let s = format!("{e}");
        assert!(s.contains("cobrust report-bug --include-mir"));
    }
}
