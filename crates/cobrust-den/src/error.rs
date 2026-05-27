// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: CPython sqlite3 (PEP 249 DB-API 2.0)
// oracle: cpython 3.11 (module: sqlite3)
// see PROVENANCE.toml for the full manifest.

//! Error taxonomy for `cobrust-den`.
//!
//! Constitution §2.2: exceptions are NOT the default error path —
//! `Result<T, E>` is. Python's `sqlite3` raises an exception hierarchy
//! (`sqlite3.OperationalError`, `sqlite3.IntegrityError`,
//! `sqlite3.ProgrammingError`, …). We collapse that hierarchy into a
//! single closed enum because a closed enum is exhaustively
//! pattern-matchable (constitution §2.2 forbids open enums) and the
//! LLM-first §2.5 "compile-time-catch-errors" rule prefers a `match`
//! the type-checker can prove total.

/// A SQLite / connection error. Returned via [`Result::Err`]; the
/// surface never panics on a SQL or connection failure (task contract
/// + constitution §5.1).
#[derive(Clone, Debug)]
pub struct SqliteError {
    /// The error class (closed taxonomy).
    pub kind: SqliteErrorKind,
    /// Human-readable detail, lifted from the rusqlite/libsqlite3
    /// message. Mirrors the `args[0]` string Python's exceptions carry.
    pub message: String,
}

/// Closed error taxonomy. Mirrors the union of the PEP 249 exception
/// classes that Python's `sqlite3` raises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqliteErrorKind {
    /// Could not open / access the database file
    /// (`sqlite3.OperationalError` at connect time).
    CannotOpen,
    /// SQL did not parse or referenced a missing table/column
    /// (`sqlite3.OperationalError` / `sqlite3.ProgrammingError`).
    Sql,
    /// A constraint was violated — UNIQUE, NOT NULL, FK, CHECK
    /// (`sqlite3.IntegrityError`).
    Constraint,
    /// The supplied parameter count / type did not match the
    /// placeholders (`sqlite3.ProgrammingError`).
    Parameter,
    /// A returned column held a type that could not be projected into
    /// the requested [`crate::Value`] (`sqlite3` type-affinity edge).
    TypeMismatch,
    /// Any other libsqlite3 error not covered above. Mirrors the
    /// catch-all `sqlite3.DatabaseError`.
    Other,
}

impl std::fmt::Display for SqliteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            SqliteErrorKind::CannotOpen => "cannot open",
            SqliteErrorKind::Sql => "sql",
            SqliteErrorKind::Constraint => "constraint",
            SqliteErrorKind::Parameter => "parameter",
            SqliteErrorKind::TypeMismatch => "type mismatch",
            SqliteErrorKind::Other => "other",
        };
        write!(f, "sqlite3 {kind} error: {}", self.message)
    }
}

impl std::error::Error for SqliteError {}

impl SqliteError {
    pub(crate) fn cannot_open(message: impl Into<String>) -> Self {
        Self {
            kind: SqliteErrorKind::CannotOpen,
            message: message.into(),
        }
    }

    pub(crate) fn sql(message: impl Into<String>) -> Self {
        Self {
            kind: SqliteErrorKind::Sql,
            message: message.into(),
        }
    }

    pub(crate) fn parameter(message: impl Into<String>) -> Self {
        Self {
            kind: SqliteErrorKind::Parameter,
            message: message.into(),
        }
    }

    pub(crate) fn type_mismatch(message: impl Into<String>) -> Self {
        Self {
            kind: SqliteErrorKind::TypeMismatch,
            message: message.into(),
        }
    }

    /// Lift a `rusqlite::Error` into our taxonomy. The classification
    /// mirrors which PEP 249 exception CPython's `sqlite3` would raise
    /// for the same underlying libsqlite3 condition.
    pub(crate) fn from_rusqlite(err: &rusqlite::Error) -> Self {
        match err {
            rusqlite::Error::SqliteFailure(e, msg) => {
                let detail = msg.clone().unwrap_or_else(|| e.to_string());
                match e.code {
                    rusqlite::ErrorCode::ConstraintViolation => Self {
                        kind: SqliteErrorKind::Constraint,
                        message: detail,
                    },
                    rusqlite::ErrorCode::CannotOpen => Self::cannot_open(detail),
                    _ => Self::sql(detail),
                }
            }
            // A SQL parse error surfaces as `SqlInputError` (rusqlite
            // 0.31+); CPython's sqlite3 raises OperationalError for the
            // same condition -> our `Sql` kind.
            rusqlite::Error::SqlInputError { .. } => Self::sql(err.to_string()),
            rusqlite::Error::InvalidParameterCount(..)
            | rusqlite::Error::InvalidParameterName(..)
            | rusqlite::Error::InvalidColumnType(..) => Self::parameter(err.to_string()),
            rusqlite::Error::FromSqlConversionFailure(..)
            | rusqlite::Error::IntegralValueOutOfRange(..) => Self::type_mismatch(err.to_string()),
            other => Self {
                kind: SqliteErrorKind::Other,
                message: other.to_string(),
            },
        }
    }
}

#[cfg(test)]
impl SqliteError {
    fn constraint_for_test(message: impl Into<String>) -> Self {
        Self {
            kind: SqliteErrorKind::Constraint,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_carries_kind_and_message() {
        let e = SqliteError::constraint_for_test("UNIQUE failed");
        let s = format!("{e}");
        assert!(s.contains("constraint"), "display: {s}");
        assert!(s.contains("UNIQUE failed"), "display: {s}");
    }

    #[test]
    fn kinds_are_distinct() {
        assert_ne!(SqliteErrorKind::Sql, SqliteErrorKind::Constraint);
        assert_ne!(SqliteErrorKind::Parameter, SqliteErrorKind::TypeMismatch);
        assert_ne!(SqliteErrorKind::CannotOpen, SqliteErrorKind::Other);
    }
}
