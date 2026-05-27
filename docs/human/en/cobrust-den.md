# cobrust-den — SQLite for Cobrust (PEP 249 DB-API 2.0)

`cobrust-den` is the Cobrust translation of Python's `sqlite3`
standard-library module. It gives you the familiar DB-API 2.0 surface —
`connect(...).cursor().execute(...).fetchall()` — backed by the mature
Rust `rusqlite` crate. SQLite itself is bundled (compiled from source),
so there is **no system library to install**.

It is the v0.7.0 "MUST-ship" database connector (Stream Z.7.c).

## Example first

A complete round-trip — create a table, insert with bound parameters,
read it back:

```rust
use den::{connect, Value, MEMORY};

// 1. Open an in-memory database (or pass a file path).
let conn = connect(MEMORY)?;          // MEMORY == ":memory:"
let mut cur = conn.cursor();

// 2. Create a table.
cur.execute(
    "CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, score REAL)",
    &[],
)?;

// 3. Insert with qmark (`?`) parameters — PEP 249 "qmark" paramstyle.
cur.execute(
    "INSERT INTO people (name, score) VALUES (?, ?)",
    &[Value::Text("ada".to_owned()), Value::Real(9.5)],
)?;
println!("inserted rowid = {:?}", cur.lastrowid()); // Some(1)

// 4. Query and read the rows back.
cur.execute("SELECT id, name, score FROM people", &[])?;
for row in cur.by_ref() {
    println!("{:?}", row.cells());
}
```

The shape matches what you would write in Python:

```python
import sqlite3
conn = sqlite3.connect(":memory:")
cur = conn.cursor()
cur.execute("CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, score REAL)")
cur.execute("INSERT INTO people (name, score) VALUES (?, ?)", ("ada", 9.5))
cur.execute("SELECT id, name, score FROM people")
for row in cur:
    print(row)
```

## What you get

- **`connect(path)`** — open `":memory:"` (via the `MEMORY` constant) or
  a file path. Returns `Result<Connection, SqliteError>`.
- **`Connection`** — `.cursor()`, `.execute(sql)`,
  `.execute_params(sql, params)`, `.commit()`, `.rollback()`,
  `.close()`.
- **`Cursor`** — `.execute(sql, params)`, `.fetchone()`,
  `.fetchmany(n)`, `.fetchall()`, `.rowcount()`, `.lastrowid()`, and you
  can iterate it directly (`for row in cursor`).
- **`Value`** — the five SQLite storage classes:
  `Null / Integer / Real / Text / Blob` (Python
  `None / int / float / str / bytes`).
- **`Row`** — positional cell access: `row.get(i)`, `row.cells()`.

## The five SQLite types

SQLite stores every value as one of exactly five storage classes. They
map one-to-one to `Value` and to Python:

| SQLite      | `Value`             | Python  |
|-------------|---------------------|---------|
| `NULL`      | `Value::Null`       | `None`  |
| `INTEGER`   | `Value::Integer(i64)` | `int`   |
| `REAL`      | `Value::Real(f64)`  | `float` |
| `TEXT`      | `Value::Text(String)` | `str`   |
| `BLOB`      | `Value::Blob(Vec<u8>)` | `bytes` |

## Errors are values, not exceptions

Python's `sqlite3` raises exceptions (`OperationalError`,
`IntegrityError`, `ProgrammingError`, …). Cobrust returns a
`Result<T, SqliteError>` instead — you handle failure with `?` or a
`match`, and the compiler makes sure you do not forget. The error kinds:

| `SqliteErrorKind` | When | Python equivalent |
|---|---|---|
| `CannotOpen` | database file cannot be opened | `OperationalError` |
| `Sql` | malformed SQL / missing table or column | `OperationalError` |
| `Constraint` | UNIQUE / NOT NULL / FK / CHECK violation | `IntegrityError` |
| `Parameter` | wrong number of `?` parameters | `ProgrammingError` |
| `TypeMismatch` | a cell could not be projected | (rare) |
| `Other` | anything else from libsqlite3 | `DatabaseError` |

A bad query never panics — it returns `Err`:

```rust
let err = cur.execute("SELCT oops", &[]).unwrap_err();
assert_eq!(err.kind, den::SqliteErrorKind::Sql);
```

## Why this design?

- **Match Python's priors.** The constitution's LLM-first principle
  (§2.5) says Cobrust is the language an AI agent writes correctly on
  the first try. `connect(":memory:").cursor().execute(...).fetchall()`
  is exactly the canonical pattern in the training data, so we keep it.
- **`Result`, never exceptions.** The constitution (§2.2) makes
  `Result<T, E>` the default error path. A closed `SqliteErrorKind`
  enum means a `match` over the failure modes is exhaustive — the type
  checker catches the case you forgot.
- **Sync, not async.** SQLite is an embedded engine with no network
  round-trips; `rusqlite` is sync and Python's `sqlite3` is sync. There
  is no two-colour async problem to import (§2.2), so the surface stays
  sync.
- **Bundled SQLite.** Compiling libsqlite3 from source (`rusqlite`'s
  `bundled` feature) makes builds deterministic and portable — no
  system package to chase.

## Compatibility tier: `semantic`

`cobrust-den` is tagged `@py_compat(semantic)`. It preserves PEP 249
behaviour and the type mapping, but it is not byte-for-byte identical to
CPython. The known differences:

- A `SELECT`'s rows are read fully into memory at `execute` time (Python
  fetches lazily). `fetchone` / `fetchmany` / `fetchall` / iteration
  return the same rows in the same order.
- Errors are `Result::Err`, not raised exceptions.
- Rows are positional only (no `sqlite3.Row` name mapping yet).
- Non-UTF-8 `TEXT` is decoded with the Unicode replacement character
  (matching the default `text_factory`).

## Not yet supported

- Named / numeric parameter styles (`:name`, `?1`).
- `executemany` / `executescript`.
- `sqlite3.Row` named access and `row_factory`.
- Using SQLite directly from Cobrust `.cb` source (`import den`) —
  that wiring is a separate, deferred step.
