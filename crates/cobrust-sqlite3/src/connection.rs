// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: CPython sqlite3 (PEP 249 DB-API 2.0)
// oracle: cpython 3.11 (module: sqlite3)
// see PROVENANCE.toml for the full manifest.

//! `Connection` + `Cursor` — the PEP 249 DB-API 2.0 surface.
//!
//! Mirrors `sqlite3.connect(path).cursor().execute(sql, params)
//! .fetchall()`. The backend is `rusqlite` (bundled libsqlite3). The
//! public surface is sync (Python `sqlite3` is sync; the embedded
//! engine has no async story — roadmap §4.1 Z-Q4).
//!
//! ## Architecture note (honest divergence)
//!
//! rusqlite's `Statement` borrows its `Connection`, which does not map
//! cleanly onto PEP 249's free-standing `Cursor`. We therefore share
//! one `rusqlite::Connection` behind `Rc<RefCell<…>>` between the
//! `Connection` and every `Cursor` it spawns, and we **eagerly
//! materialize** a SELECT's result set into an owned `Vec<Row>` at
//! `execute` time. CPython's `sqlite3` fetches lazily; materializing is
//! a documented divergence (see `PROVENANCE.toml [verification]
//! divergences`) that is observationally identical for the supported
//! surface — `fetchone` / `fetchmany` / `fetchall` / iteration return
//! the same rows in the same order. The single-threaded `Rc` is
//! deliberate: `sqlite3.Connection` objects are likewise not safe to
//! share across threads by default (`check_same_thread=True`).

use std::cell::RefCell;
use std::rc::Rc;

use rusqlite::Connection as RusqliteConnection;

use crate::error::SqliteError;
use crate::value::{Row, Value};

/// Sentinel path for an in-memory database, matching
/// `sqlite3.connect(":memory:")`.
pub const MEMORY: &str = ":memory:";

type SharedConn = Rc<RefCell<RusqliteConnection>>;

/// A DB-API 2.0 connection. Mirrors `sqlite3.Connection`.
///
/// Construct via [`connect`]. Holds the single backing
/// `rusqlite::Connection`; every [`Cursor`] spawned from it shares the
/// same handle (so writes from one cursor are visible to another, as in
/// Python).
pub struct Connection {
    inner: SharedConn,
}

impl Connection {
    /// Spawn a new cursor over this connection. Mirrors
    /// `Connection.cursor()`.
    #[must_use]
    pub fn cursor(&self) -> Cursor {
        Cursor {
            conn: Rc::clone(&self.inner),
            rows: Vec::new(),
            position: 0,
            rowcount: -1,
            lastrowid: None,
        }
    }

    /// Convenience shorthand: spawn a cursor, run `sql`, return it.
    /// Mirrors `Connection.execute(sql)` (PEP 249 optional extension
    /// that CPython's `sqlite3` provides).
    ///
    /// # Errors
    /// Returns [`SqliteError`] when the SQL is invalid or execution
    /// fails.
    pub fn execute(&self, sql: &str) -> Result<Cursor, SqliteError> {
        let mut cur = self.cursor();
        cur.execute(sql, &[])?;
        Ok(cur)
    }

    /// Convenience shorthand with bound parameters. Mirrors
    /// `Connection.execute(sql, params)`.
    ///
    /// # Errors
    /// Returns [`SqliteError`] when the SQL is invalid, the parameter
    /// count mismatches, or execution fails.
    pub fn execute_params(&self, sql: &str, params: &[Value]) -> Result<Cursor, SqliteError> {
        let mut cur = self.cursor();
        cur.execute(sql, params)?;
        Ok(cur)
    }

    /// Commit the current transaction. Mirrors `Connection.commit()`.
    ///
    /// SQLite autocommits each statement by default unless an explicit
    /// `BEGIN` is open; this issues a `COMMIT` and is a no-op when no
    /// transaction is active (matching `sqlite3`'s tolerant commit).
    ///
    /// # Errors
    /// Returns [`SqliteError`] only on an unexpected backend failure.
    pub fn commit(&self) -> Result<(), SqliteError> {
        let conn = self.inner.borrow();
        match conn.execute_batch("COMMIT") {
            Ok(()) => Ok(()),
            // "cannot commit - no transaction is active" is benign and
            // matches `sqlite3.Connection.commit()` being a no-op when
            // autocommit already flushed the statement.
            Err(e) if e.to_string().contains("no transaction is active") => Ok(()),
            Err(e) => Err(SqliteError::from_rusqlite(&e)),
        }
    }

    /// Roll back the current transaction. Mirrors
    /// `Connection.rollback()`.
    ///
    /// # Errors
    /// Returns [`SqliteError`] only on an unexpected backend failure.
    pub fn rollback(&self) -> Result<(), SqliteError> {
        let conn = self.inner.borrow();
        match conn.execute_batch("ROLLBACK") {
            Ok(()) => Ok(()),
            Err(e) if e.to_string().contains("no transaction is active") => Ok(()),
            Err(e) => Err(SqliteError::from_rusqlite(&e)),
        }
    }

    /// Close the connection. Mirrors `Connection.close()`.
    ///
    /// Dropping the last `Connection`/`Cursor` referencing the shared
    /// handle releases libsqlite3 resources; this is an explicit
    /// best-effort flush (`COMMIT` any open transaction) for parity with
    /// callers that expect `close()` to durably flush. Idempotent.
    ///
    /// # Errors
    /// Returns [`SqliteError`] only on an unexpected backend failure.
    pub fn close(&self) -> Result<(), SqliteError> {
        // Best-effort flush; ignore the no-transaction benign case.
        self.commit()
    }
}

/// A DB-API 2.0 cursor. Mirrors `sqlite3.Cursor`.
///
/// Result rows are materialized into `rows` at [`Cursor::execute`]
/// time (see the module-level architecture note). `fetchone` /
/// `fetchmany` / `fetchall` and iteration advance `position` over that
/// buffer.
pub struct Cursor {
    conn: SharedConn,
    rows: Vec<Row>,
    position: usize,
    rowcount: i64,
    lastrowid: Option<i64>,
}

impl Cursor {
    /// Execute `sql` with qmark (`?`) bound `params` (PEP 249
    /// `paramstyle = "qmark"`). Returns `&mut self` so calls chain like
    /// `cur.execute(...)?` then `cur.fetchall()`.
    ///
    /// For a SELECT, the full result set is materialized; for a DML
    /// statement (`INSERT`/`UPDATE`/`DELETE`), `rowcount` and
    /// `lastrowid` are updated and `rows` is left empty.
    ///
    /// # Errors
    /// Returns [`SqliteError`]:
    /// - [`crate::SqliteErrorKind::Sql`] for malformed SQL or a missing
    ///   table/column,
    /// - [`crate::SqliteErrorKind::Parameter`] for a placeholder/param
    ///   count mismatch,
    /// - [`crate::SqliteErrorKind::Constraint`] for a constraint
    ///   violation.
    pub fn execute(&mut self, sql: &str, params: &[Value]) -> Result<&mut Self, SqliteError> {
        // All backend work happens inside this block so the
        // `conn`/`stmt` borrow of the shared connection ends before we
        // hand back `&mut self`. The block yields the new cursor state
        // as owned data `(rowcount, lastrowid, rows)`.
        let (rowcount, lastrowid, rows) = {
            let conn = self.conn.borrow();
            let mut stmt = conn
                .prepare(sql)
                .map_err(|e| SqliteError::from_rusqlite(&e))?;

            let expected = stmt.parameter_count();
            if expected != params.len() {
                return Err(SqliteError::parameter(format!(
                    "SQL has {expected} placeholder(s) but {} parameter(s) were supplied",
                    params.len()
                )));
            }

            let column_count = stmt.column_count();
            // rusqlite wants `&[&dyn ToSql]`; build it from our owned slice.
            let bound: Vec<&dyn rusqlite::ToSql> =
                params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

            if column_count == 0 {
                // DML / DDL — no result set. `execute` returns affected rows.
                let affected = stmt
                    .execute(rusqlite::params_from_iter(bound.iter()))
                    .map_err(|e| SqliteError::from_rusqlite(&e))?;
                if is_dml(sql) {
                    // DML — rowcount is the affected-row count, lastrowid
                    // is the last inserted rowid (matches sqlite3).
                    // i64 cast: affected counts never exceed i64 in practice.
                    let rowcount = i64::try_from(affected).unwrap_or(i64::MAX);
                    (rowcount, Some(conn.last_insert_rowid()), Vec::new())
                } else {
                    // DDL / other non-row statement — sqlite3 reports
                    // rowcount == -1 and lastrowid is not meaningful.
                    (-1, None, Vec::new())
                }
            } else {
                // SELECT — materialize the full result set.
                let mut query_rows = stmt
                    .query(rusqlite::params_from_iter(bound.iter()))
                    .map_err(|e| SqliteError::from_rusqlite(&e))?;
                let mut materialized: Vec<Row> = Vec::new();
                loop {
                    match query_rows.next() {
                        Ok(Some(raw_row)) => {
                            let mut cells: Vec<Value> = Vec::with_capacity(column_count);
                            for idx in 0..column_count {
                                let raw = raw_row
                                    .get_ref(idx)
                                    .map_err(|e| SqliteError::from_rusqlite(&e))?;
                                cells.push(Value::from_value_ref(raw));
                            }
                            materialized.push(Row::new(cells));
                        }
                        Ok(None) => break,
                        Err(e) => return Err(SqliteError::from_rusqlite(&e)),
                    }
                }
                // i64 cast: row counts never exceed i64 in practice.
                let rowcount = i64::try_from(materialized.len()).unwrap_or(i64::MAX);
                (rowcount, None, materialized)
            }
        };

        // Commit the new state only after the borrow has ended.
        self.position = 0;
        self.rowcount = rowcount;
        self.lastrowid = lastrowid;
        self.rows = rows;
        Ok(self)
    }

    /// Fetch the next row, or `None` when exhausted. Mirrors
    /// `Cursor.fetchone()` (which returns `None` at end-of-set).
    #[must_use]
    pub fn fetchone(&mut self) -> Option<Row> {
        if self.position < self.rows.len() {
            let row = self.rows[self.position].clone();
            self.position += 1;
            Some(row)
        } else {
            None
        }
    }

    /// Fetch up to `size` remaining rows. Mirrors
    /// `Cursor.fetchmany(size)`; returns fewer than `size` (possibly
    /// empty) near end-of-set.
    #[must_use]
    pub fn fetchmany(&mut self, size: usize) -> Vec<Row> {
        let end = (self.position + size).min(self.rows.len());
        let slice = self.rows[self.position..end].to_vec();
        self.position = end;
        slice
    }

    /// Fetch all remaining rows. Mirrors `Cursor.fetchall()`.
    #[must_use]
    pub fn fetchall(&mut self) -> Vec<Row> {
        let slice = self.rows[self.position..].to_vec();
        self.position = self.rows.len();
        slice
    }

    /// Rows affected by the last DML, or the row count of the last
    /// SELECT result set; `-1` before any execute or for statements
    /// where the count is not determined. Mirrors `Cursor.rowcount`.
    #[must_use]
    pub fn rowcount(&self) -> i64 {
        self.rowcount
    }

    /// Rowid of the last inserted row, or `None` if the last statement
    /// was not an INSERT. Mirrors `Cursor.lastrowid`.
    #[must_use]
    pub fn lastrowid(&self) -> Option<i64> {
        self.lastrowid
    }
}

// Iteration over a cursor yields rows, matching `for row in cursor:`.
impl Iterator for Cursor {
    type Item = Row;

    fn next(&mut self) -> Option<Self::Item> {
        self.fetchone()
    }
}

/// True when the statement is data-manipulation (INSERT / UPDATE /
/// DELETE / REPLACE) — the statement classes for which `sqlite3`
/// reports a non-`-1` `rowcount`. DDL (CREATE/DROP/…) and other
/// statements leave `rowcount` at `-1`, matching CPython.
fn is_dml(sql: &str) -> bool {
    let leading = sql.trim_start();
    let keyword: String = leading
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    matches!(keyword.as_str(), "INSERT" | "UPDATE" | "DELETE" | "REPLACE")
}

/// Open a connection to the SQLite database at `path`. Pass
/// [`MEMORY`] (`":memory:"`) for an in-memory database. Mirrors
/// `sqlite3.connect(path)`.
///
/// # Errors
/// Returns [`SqliteError`] with [`crate::SqliteErrorKind::CannotOpen`]
/// when the database cannot be opened (e.g. an unwritable directory).
pub fn connect(path: &str) -> Result<Connection, SqliteError> {
    let inner = if path == MEMORY {
        RusqliteConnection::open_in_memory()
    } else {
        RusqliteConnection::open(path)
    }
    .map_err(|e| SqliteError::from_rusqlite(&e))?;
    Ok(Connection {
        inner: Rc::new(RefCell::new(inner)),
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn dml_classification() {
        for sql in [
            "INSERT INTO t VALUES (1)",
            "  insert into t values (1)",
            "UPDATE t SET x = 1",
            "DELETE FROM t",
            "replace into t values (1)",
        ] {
            assert!(is_dml(sql), "should be DML: {sql}");
        }
        for sql in [
            "CREATE TABLE t (x)",
            "DROP TABLE t",
            "SELECT * FROM t",
            "BEGIN",
            "PRAGMA foreign_keys = ON",
        ] {
            assert!(!is_dml(sql), "should not be DML: {sql}");
        }
    }

    #[test]
    fn memory_sentinel_opens() {
        let conn = connect(MEMORY).expect("open in-memory");
        let _cur = conn.cursor();
    }
}
