//! L3 differential gate for cobrust-den.
//!
//! Constitution §4.2 / §6: differential tests against CPython's
//! `sqlite3` as the oracle. The expected values in this file were
//! captured from CPython 3.11 `sqlite3` (paramstyle = "qmark"); they
//! are identical on every CPython 3.x for the DB-API 2.0 surface under
//! test. Each `// oracle:` comment records the exact CPython output the
//! assertion mirrors.
//!
//! Coverage (per the task brief):
//! - CREATE / INSERT / SELECT round-trip.
//! - All five SQLite storage classes (NULL/INTEGER/REAL/TEXT/BLOB).
//! - qmark `?` parameter binding.
//! - fetchone vs fetchall vs fetchmany.
//! - empty result set.
//! - error path: bad SQL -> Err (never panic).
//! - constraint violation + parameter-count mismatch classification.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::approx_constant)]

use den::{MEMORY, Row, SqliteErrorKind, Value, connect};

use den::SqliteError;

/// Helper: the single cell of a one-column row.
fn cell0(row: &Row) -> &Value {
    row.get(0).expect("row has at least one column")
}

/// Helper: unwrap the `Err` of a `Result` whose `Ok` type is not
/// `Debug` (`Cursor` / `Connection`). `Result::expect_err` requires the
/// `Ok` type to be `Debug`; this matches manually instead.
fn must_err<T>(result: Result<T, SqliteError>, ctx: &str) -> SqliteError {
    match result {
        Ok(_) => panic!("expected Err for: {ctx}"),
        Err(e) => e,
    }
}

#[test]
fn create_insert_select_round_trip() {
    let conn = connect(MEMORY).expect("open in-memory db");
    let mut cur = conn.cursor();

    // oracle: cur.rowcount == -1 after CREATE.
    cur.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, score REAL, data BLOB, note TEXT)",
        &[],
    )
    .expect("create");
    assert_eq!(cur.rowcount(), -1, "CREATE leaves rowcount at -1");

    // oracle: insert1 rowcount == 1, lastrowid == 1.
    cur.execute(
        "INSERT INTO t (name, score, data, note) VALUES (?, ?, ?, ?)",
        &[
            Value::Text("ada".to_owned()),
            Value::Real(9.5),
            Value::Blob(vec![0x00, 0x01, 0xff]),
            Value::Null,
        ],
    )
    .expect("insert1");
    assert_eq!(cur.rowcount(), 1);
    assert_eq!(cur.lastrowid(), Some(1));

    // oracle: insert2 rowcount == 1, lastrowid == 2.
    cur.execute(
        "INSERT INTO t (name, score, data, note) VALUES (?, ?, ?, ?)",
        &[
            Value::Text("alan".to_owned()),
            Value::Real(8.25),
            Value::Blob(b"xyz".to_vec()),
            Value::Text("hi".to_owned()),
        ],
    )
    .expect("insert2");
    assert_eq!(cur.lastrowid(), Some(2));

    // oracle: fetchall ==
    //   [(1, 'ada', 9.5, b'\x00\x01\xff', None),
    //    (2, 'alan', 8.25, b'xyz', 'hi')]
    cur.execute("SELECT id, name, score, data, note FROM t ORDER BY id", &[])
        .expect("select");
    let rows = cur.fetchall();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].cells(),
        &[
            Value::Integer(1),
            Value::Text("ada".to_owned()),
            Value::Real(9.5),
            Value::Blob(vec![0x00, 0x01, 0xff]),
            Value::Null,
        ]
    );
    assert_eq!(
        rows[1].cells(),
        &[
            Value::Integer(2),
            Value::Text("alan".to_owned()),
            Value::Real(8.25),
            Value::Blob(b"xyz".to_vec()),
            Value::Text("hi".to_owned()),
        ]
    );
}

#[test]
fn all_five_storage_classes_round_trip() {
    // oracle: types roundtrip ==
    //   [(None,), (42,), (3.14,), ('héllo',), (b'\x00\xff',)]
    let conn = connect(MEMORY).expect("open");
    let mut cur = conn.cursor();
    cur.execute("CREATE TABLE types (v)", &[]).expect("create");

    let inputs = [
        Value::Null,
        Value::Integer(42),
        Value::Real(3.14),
        Value::Text("héllo".to_owned()),
        Value::Blob(vec![0x00, 0xff]),
    ];
    for v in &inputs {
        cur.execute("INSERT INTO types (v) VALUES (?)", std::slice::from_ref(v))
            .expect("insert");
    }

    cur.execute("SELECT v FROM types", &[]).expect("select");
    let rows = cur.fetchall();
    let got: Vec<&Value> = rows.iter().map(cell0).collect();
    let want: Vec<&Value> = inputs.iter().collect();
    assert_eq!(got, want, "all five storage classes must round-trip");
}

#[test]
fn fetchone_walks_then_returns_none() {
    // oracle: fetchone1 == (1, 'ada'), fetchone2 == (2, 'alan'),
    //         fetchone3 == None.
    let conn = connect(MEMORY).expect("open");
    let mut cur = conn.cursor();
    cur.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)", &[])
        .expect("create");
    cur.execute(
        "INSERT INTO t (name) VALUES (?)",
        &[Value::Text("ada".to_owned())],
    )
    .expect("i1");
    cur.execute(
        "INSERT INTO t (name) VALUES (?)",
        &[Value::Text("alan".to_owned())],
    )
    .expect("i2");

    cur.execute("SELECT id, name FROM t ORDER BY id", &[])
        .expect("select");
    let one = cur.fetchone().expect("first row");
    assert_eq!(
        one.cells(),
        &[Value::Integer(1), Value::Text("ada".to_owned())]
    );
    let two = cur.fetchone().expect("second row");
    assert_eq!(
        two.cells(),
        &[Value::Integer(2), Value::Text("alan".to_owned())]
    );
    assert!(cur.fetchone().is_none(), "exhausted cursor yields None");
}

#[test]
fn fetchmany_chunks_then_drains() {
    // oracle: fetchmany(1) == [(1,)], fetchmany(5) == [(2,)].
    let conn = connect(MEMORY).expect("open");
    let mut cur = conn.cursor();
    cur.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)", &[])
        .expect("create");
    cur.execute(
        "INSERT INTO t (name) VALUES (?)",
        &[Value::Text("a".to_owned())],
    )
    .expect("i1");
    cur.execute(
        "INSERT INTO t (name) VALUES (?)",
        &[Value::Text("b".to_owned())],
    )
    .expect("i2");

    cur.execute("SELECT id FROM t ORDER BY id", &[])
        .expect("select");
    let first = cur.fetchmany(1);
    assert_eq!(first.len(), 1);
    assert_eq!(cell0(&first[0]), &Value::Integer(1));
    // Asking for 5 with only 1 remaining yields just the 1 remaining.
    let rest = cur.fetchmany(5);
    assert_eq!(rest.len(), 1);
    assert_eq!(cell0(&rest[0]), &Value::Integer(2));
    // Now exhausted.
    assert!(cur.fetchmany(5).is_empty());
}

#[test]
fn empty_result_set() {
    // oracle: empty fetchall == [], empty fetchone == None.
    let conn = connect(MEMORY).expect("open");
    let mut cur = conn.cursor();
    cur.execute("CREATE TABLE t (id INTEGER PRIMARY KEY)", &[])
        .expect("create");
    cur.execute("SELECT id FROM t WHERE id = 999", &[])
        .expect("select");
    assert!(cur.fetchall().is_empty(), "no matching rows -> empty");

    cur.execute("SELECT id FROM t WHERE id = 999", &[])
        .expect("select again");
    assert!(cur.fetchone().is_none(), "no matching rows -> None");
}

#[test]
fn cursor_iteration_yields_rows() {
    let conn = connect(MEMORY).expect("open");
    let mut cur = conn.cursor();
    cur.execute("CREATE TABLE t (n INTEGER)", &[])
        .expect("create");
    for n in 0..3 {
        cur.execute("INSERT INTO t (n) VALUES (?)", &[Value::Integer(n)])
            .expect("insert");
    }
    cur.execute("SELECT n FROM t ORDER BY n", &[])
        .expect("select");
    let collected: Vec<i64> = cur
        .by_ref()
        .filter_map(|row| match cell0(&row) {
            Value::Integer(i) => Some(*i),
            _ => None,
        })
        .collect();
    assert_eq!(collected, vec![0, 1, 2]);
}

// ── Error path (oracle: CPython raises; we return Err, never panic) ──────────

#[test]
fn bad_sql_returns_err_not_panic() {
    // oracle: sqlite3 raises OperationalError for "SELCT bad".
    let conn = connect(MEMORY).expect("open");
    let mut cur = conn.cursor();
    let err = must_err(cur.execute("SELCT bad", &[]), "malformed SQL");
    assert_eq!(err.kind, SqliteErrorKind::Sql);
}

#[test]
fn missing_table_returns_err() {
    let conn = connect(MEMORY).expect("open");
    let mut cur = conn.cursor();
    let err = must_err(
        cur.execute("SELECT * FROM no_such_table", &[]),
        "missing table",
    );
    assert_eq!(err.kind, SqliteErrorKind::Sql);
}

#[test]
fn constraint_violation_is_classified() {
    // oracle: sqlite3 raises IntegrityError on a UNIQUE collision.
    let conn = connect(MEMORY).expect("open");
    let mut cur = conn.cursor();
    cur.execute(
        "CREATE TABLE u (id INTEGER PRIMARY KEY, x TEXT UNIQUE)",
        &[],
    )
    .expect("create");
    cur.execute("INSERT INTO u (x) VALUES ('dup')", &[])
        .expect("first");
    let err = must_err(
        cur.execute("INSERT INTO u (x) VALUES ('dup')", &[]),
        "UNIQUE collision",
    );
    assert_eq!(err.kind, SqliteErrorKind::Constraint);
}

#[test]
fn parameter_count_mismatch_is_classified() {
    // oracle: sqlite3 raises ProgrammingError for a param-count mismatch.
    let conn = connect(MEMORY).expect("open");
    let mut cur = conn.cursor();
    cur.execute("CREATE TABLE u (id INTEGER PRIMARY KEY, x TEXT)", &[])
        .expect("create");
    let err = must_err(
        cur.execute(
            "INSERT INTO u (x) VALUES (?)",
            &[Value::Text("a".to_owned()), Value::Text("b".to_owned())],
        ),
        "too many params",
    );
    assert_eq!(err.kind, SqliteErrorKind::Parameter);
}

#[test]
fn connect_bad_path_returns_err() {
    // A path under a non-existent directory cannot be opened.
    let err = must_err(connect("/no/such/dir/xyz/db.sqlite"), "unopenable path");
    assert_eq!(err.kind, SqliteErrorKind::CannotOpen);
}

// ── Connection-level convenience surface ─────────────────────────────────────

#[test]
fn connection_execute_shorthand_and_commit() {
    let conn = connect(MEMORY).expect("open");
    conn.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)")
        .expect("create via Connection::execute");
    conn.execute_params(
        "INSERT INTO t (v) VALUES (?)",
        &[Value::Text("x".to_owned())],
    )
    .expect("insert via Connection::execute_params");
    // commit is a no-op under autocommit but must not error.
    conn.commit().expect("commit");

    let mut cur = conn
        .execute("SELECT v FROM t")
        .expect("select via Connection::execute");
    let rows = cur.fetchall();
    assert_eq!(rows.len(), 1);
    assert_eq!(cell0(&rows[0]), &Value::Text("x".to_owned()));

    conn.close().expect("close");
}

#[test]
fn cursors_share_one_connection() {
    // Two cursors over the same Connection see each other's writes —
    // mirrors Python where multiple cursors share the connection.
    let conn = connect(MEMORY).expect("open");
    let mut writer = conn.cursor();
    writer
        .execute("CREATE TABLE t (n INTEGER)", &[])
        .expect("create");
    writer
        .execute("INSERT INTO t (n) VALUES (?)", &[Value::Integer(7)])
        .expect("insert");

    let mut reader = conn.cursor();
    reader.execute("SELECT n FROM t", &[]).expect("select");
    let rows = reader.fetchall();
    assert_eq!(rows.len(), 1);
    assert_eq!(cell0(&rows[0]), &Value::Integer(7));
}
