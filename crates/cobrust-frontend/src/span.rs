//! Source spans.
//!
//! Every AST node and every token carries a [`Span`] of the form
//! `(file_id, byte_start, byte_end)`. The byte range is **half-open**
//! and addresses **bytes**, not chars — a single UTF-8 codepoint may
//! span multiple bytes.

use std::fmt;

/// Opaque source-file identifier, assigned by whoever owns the
/// `FileMap` (out of scope for M1).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FileId(pub u32);

impl FileId {
    /// A sentinel value useful for tests and synthetic input.
    pub const SYNTHETIC: Self = Self(0);
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "file#{}", self.0)
    }
}

/// Half-open byte range in a known source file.
///
/// `start <= end` is an invariant; constructors enforce it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    /// Build a span; if the order is wrong, the args are swapped so the
    /// invariant `start <= end` holds. This is intentional — we never
    /// want a panic in the lexer hot path because of an arithmetic
    /// underflow on a corrupted offset.
    #[must_use]
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        let (s, e) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        Self {
            file,
            start: s,
            end: e,
        }
    }

    /// A zero-width span anchored at `pos` — useful for synthetic
    /// nodes (e.g. the empty body of a `pass` statement that the
    /// unparser still wants to address).
    #[must_use]
    pub fn point(file: FileId, pos: u32) -> Self {
        Self::new(file, pos, pos)
    }

    /// Smallest span enclosing both inputs. Both must reference the
    /// same file; if they do not, the file of `self` wins (this only
    /// arises in pathological synthesized AST and is documented as
    /// best-effort, not load-bearing).
    #[must_use]
    pub fn merge(self, other: Span) -> Span {
        Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Length in bytes.
    #[must_use]
    pub fn len(self) -> u32 {
        self.end - self.start
    }

    /// Whether the span covers zero bytes.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}..{}", self.file, self.start, self.end)
    }
}

/// A value `T` carrying its source span.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub const fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_orders_endpoints() {
        let s = Span::new(FileId(7), 10, 5);
        assert_eq!(s.start, 5);
        assert_eq!(s.end, 10);
        assert_eq!(s.file, FileId(7));
    }

    #[test]
    fn span_merge_covers_both() {
        let a = Span::new(FileId(0), 0, 5);
        let b = Span::new(FileId(0), 3, 12);
        let m = a.merge(b);
        assert_eq!(m.start, 0);
        assert_eq!(m.end, 12);
    }

    #[test]
    fn span_point_is_zero_width() {
        let s = Span::point(FileId(0), 4);
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }
}
