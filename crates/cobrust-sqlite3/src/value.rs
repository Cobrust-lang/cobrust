// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: CPython sqlite3 (PEP 249 DB-API 2.0)
// oracle: cpython 3.11 (module: sqlite3)
// see PROVENANCE.toml for the full manifest.

//! The five SQLite storage classes as a closed Cobrust value enum.
//!
//! SQLite has exactly five storage classes: NULL, INTEGER, REAL, TEXT,
//! BLOB (<https://www.sqlite.org/datatype3.html>). Python's `sqlite3`
//! maps these to `None / int / float / str / bytes`. [`Value`] is the
//! Cobrust mirror — a closed enum (constitution §2.2) so a `match` over
//! a fetched cell is exhaustive at compile time (LLM-first §2.5).

use rusqlite::ToSql;
use rusqlite::types::{ToSqlOutput, Value as RusqliteValue, ValueRef};

/// One SQLite cell value, one of the five storage classes.
///
/// Used both as a bind parameter (qmark `?` placeholders, PEP 249
/// `paramstyle = "qmark"`) and as a fetched-row cell.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// SQLite NULL — Python `None`.
    Null,
    /// SQLite INTEGER — Python `int` (64-bit signed).
    Integer(i64),
    /// SQLite REAL — Python `float` (IEEE-754 double).
    Real(f64),
    /// SQLite TEXT — Python `str` (utf-8).
    Text(String),
    /// SQLite BLOB — Python `bytes`.
    Blob(Vec<u8>),
}

impl Value {
    /// Project a borrowed rusqlite cell into a [`Value`]. Total over the
    /// five storage classes; rusqlite has no sixth variant, so this is
    /// infallible.
    pub(crate) fn from_value_ref(raw: ValueRef<'_>) -> Self {
        match raw {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(i) => Value::Integer(i),
            ValueRef::Real(r) => Value::Real(r),
            // TEXT cells are stored as bytes in SQLite; lift to a
            // lossless utf-8 String (SQLite TEXT is utf-8 by contract).
            // Non-utf8 bytes fall back to a lossy decode rather than
            // erroring — matches `sqlite3`'s default `text_factory=str`
            // which uses utf-8 with replacement on malformed input.
            ValueRef::Text(bytes) => Value::Text(String::from_utf8_lossy(bytes).into_owned()),
            ValueRef::Blob(bytes) => Value::Blob(bytes.to_vec()),
        }
    }
}

// `ToSql` lets a `Value` be passed straight into a rusqlite bind. The
// qmark binder (`Cursor::execute`) relies on this.
impl ToSql for Value {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        let out = match self {
            Value::Null => ToSqlOutput::Owned(RusqliteValue::Null),
            Value::Integer(i) => ToSqlOutput::Owned(RusqliteValue::Integer(*i)),
            Value::Real(r) => ToSqlOutput::Owned(RusqliteValue::Real(*r)),
            Value::Text(s) => ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes())),
            Value::Blob(b) => ToSqlOutput::Borrowed(ValueRef::Blob(b)),
        };
        Ok(out)
    }
}

/// One fetched row — an ordered, owned sequence of cells.
///
/// Mirrors the default `sqlite3.Row`-as-tuple shape: positional access
/// by column index. (Named access / `Row` mapping is an M-next surface
/// per the module spec.)
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    cells: Vec<Value>,
}

impl Row {
    pub(crate) fn new(cells: Vec<Value>) -> Self {
        Self { cells }
    }

    /// The cell at column `index`, or `None` if out of range. Mirrors
    /// `row[index]` (Python raises `IndexError`; we return `Option`
    /// per constitution §2.2).
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Value> {
        self.cells.get(index)
    }

    /// Number of columns in this row.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// True when the row has zero columns.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Borrow the full cell sequence.
    #[must_use]
    pub fn cells(&self) -> &[Value] {
        &self.cells
    }

    /// Consume the row into its owned cell sequence.
    #[must_use]
    pub fn into_cells(self) -> Vec<Value> {
        self.cells
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn value_ref_projects_all_five_storage_classes() {
        assert_eq!(Value::from_value_ref(ValueRef::Null), Value::Null);
        assert_eq!(
            Value::from_value_ref(ValueRef::Integer(7)),
            Value::Integer(7)
        );
        assert_eq!(Value::from_value_ref(ValueRef::Real(1.5)), Value::Real(1.5));
        assert_eq!(
            Value::from_value_ref(ValueRef::Text(b"hi")),
            Value::Text("hi".to_owned())
        );
        assert_eq!(
            Value::from_value_ref(ValueRef::Blob(&[1, 2, 3])),
            Value::Blob(vec![1, 2, 3])
        );
    }

    #[test]
    fn non_utf8_text_decodes_lossily() {
        // 0xff is not valid utf-8; lossy decode yields the replacement char.
        let v = Value::from_value_ref(ValueRef::Text(&[0xff]));
        match v {
            Value::Text(s) => assert!(s.contains('\u{FFFD}')),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn row_accessors() {
        let row = Row::new(vec![Value::Integer(1), Value::Text("x".to_owned())]);
        assert_eq!(row.len(), 2);
        assert!(!row.is_empty());
        assert_eq!(row.get(0), Some(&Value::Integer(1)));
        assert_eq!(row.get(1), Some(&Value::Text("x".to_owned())));
        assert_eq!(row.get(2), None);
        assert_eq!(row.cells().len(), 2);
        assert_eq!(row.into_cells().len(), 2);
    }

    #[test]
    fn empty_row_is_empty() {
        let row = Row::new(Vec::new());
        assert!(row.is_empty());
        assert_eq!(row.len(), 0);
    }
}
