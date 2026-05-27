// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: flask 3.0 (web-server surface)
// oracle: cpython 3.11 (module: flask)
// see PROVENANCE.toml for the full manifest.

//! Error taxonomy for `cobrust-pit`.
//!
//! Constitution §2.2: exceptions are NOT the default error path —
//! `Result<T, E>` is. Flask raises Python exceptions (`OSError` on a
//! bind failure, `RuntimeError` for a re-registered route, …). We
//! collapse those into a single closed enum because a closed enum is
//! exhaustively pattern-matchable (constitution §2.2 forbids open
//! enums) and the LLM-first §2.5 "compile-time-catch-errors" rule
//! prefers a `match` the type-checker can prove total.

/// A server / routing error. Returned via [`Result::Err`]; the surface
/// never panics on a bind failure, a duplicate route, or a malformed
/// request body (constitution §5.1).
#[derive(Clone, Debug)]
pub struct PitError {
    /// The error class (closed taxonomy).
    pub kind: PitErrorKind,
    /// Human-readable detail.
    pub message: String,
}

/// Closed error taxonomy. Mirrors the union of the failure modes Flask
/// surfaces as Python exceptions on the server path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PitErrorKind {
    /// The listen socket could not be bound (address in use, bad host,
    /// permission). Mirrors Flask's `OSError` at `app.run` time.
    Bind,
    /// A route was registered twice for the same `(method, path)`.
    /// Mirrors Flask's `AssertionError: View function mapping is
    /// overwriting an existing endpoint`.
    DuplicateRoute,
    /// A route path was malformed (empty, or a `<param>` segment that
    /// did not close). Mirrors Werkzeug's rule-compile error.
    InvalidRoute,
    /// The tokio runtime backing the server could not be created or the
    /// server task failed. Internal — has no direct Flask analogue.
    Runtime,
}

impl std::fmt::Display for PitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            PitErrorKind::Bind => "bind",
            PitErrorKind::DuplicateRoute => "duplicate route",
            PitErrorKind::InvalidRoute => "invalid route",
            PitErrorKind::Runtime => "runtime",
        };
        write!(f, "pit {kind} error: {}", self.message)
    }
}

impl std::error::Error for PitError {}

impl PitError {
    pub(crate) fn bind(message: impl Into<String>) -> Self {
        Self {
            kind: PitErrorKind::Bind,
            message: message.into(),
        }
    }

    pub(crate) fn duplicate_route(message: impl Into<String>) -> Self {
        Self {
            kind: PitErrorKind::DuplicateRoute,
            message: message.into(),
        }
    }

    pub(crate) fn invalid_route(message: impl Into<String>) -> Self {
        Self {
            kind: PitErrorKind::InvalidRoute,
            message: message.into(),
        }
    }

    pub(crate) fn runtime(message: impl Into<String>) -> Self {
        Self {
            kind: PitErrorKind::Runtime,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_carries_kind_and_message() {
        let e = PitError::bind("address already in use");
        let s = format!("{e}");
        assert!(s.contains("bind"), "display: {s}");
        assert!(s.contains("address already in use"), "display: {s}");
    }

    #[test]
    fn kinds_are_distinct() {
        assert_ne!(PitErrorKind::Bind, PitErrorKind::DuplicateRoute);
        assert_ne!(PitErrorKind::InvalidRoute, PitErrorKind::Runtime);
        assert_ne!(PitErrorKind::Bind, PitErrorKind::Runtime);
    }
}
