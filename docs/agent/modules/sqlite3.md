---
doc_kind: module
module_id: mod:sqlite3
crate: cobrust-sqlite3
last_verified_commit: pending
dependencies: [mod:translator]
---

# Module: sqlite3

## Purpose

Cobrust translation of CPython's `sqlite3` stdlib module — the PEP 249
DB-API 2.0 surface — over the Rust `rusqlite` crate (bundled
libsqlite3). The v0.7.0 Stream Z.7.c deliverable: the one MUST-ship DB
connector (roadmap §5). Surface-translates
`sqlite3.connect(":memory:").cursor().execute(sql, params).fetchall()`
onto `rusqlite::Connection`, keeping the public API sync (Python's
`sqlite3` is itself sync; the embedded engine has no async story —
roadmap Z-Q4 + constitution §2.2).

LLM-first (constitution §2.5): the surface mirrors the canonical Python
priors so an LLM agent writes it correctly on the first try
(maximize-overlap-with-training-data), and errors are a closed,
exhaustively-matchable `Result` taxonomy (compile-time-catch-errors).

## Status

- **Z.7.c — delivered.** PEP 249 minimum surface translated via the
  synthetic-LLM pattern (hand-authored to the spec, real-LLM pipeline
  rerun pending — same posture as `mod:requests` B8 demote). Backend
  bound to `rusqlite = "0.32"` (`bundled` feature, no system
  libsqlite3). The L3 differential gate runs against CPython `sqlite3`
  expected values captured into `tests/sqlite3_downstream.rs` with
  per-assertion `oracle:` comments. The `.cb`-source intrinsic/extern
  wiring (so Cobrust source can `import sqlite3`) is a deferred
  follow-on — see Non-goals.

## Public surface (Z.7.c)

```rust
pub const MEMORY: &str = ":memory:";

pub fn connect(path: &str) -> Result<Connection, SqliteError>;

pub struct Connection { /* private: Rc<RefCell<rusqlite::Connection>> */ }

impl Connection {
    pub fn cursor(&self) -> Cursor;
    pub fn execute(&self, sql: &str) -> Result<Cursor, SqliteError>;
    pub fn execute_params(&self, sql: &str, params: &[Value]) -> Result<Cursor, SqliteError>;
    pub fn commit(&self) -> Result<(), SqliteError>;
    pub fn rollback(&self) -> Result<(), SqliteError>;
    pub fn close(&self) -> Result<(), SqliteError>;
}

pub struct Cursor { /* private: shared conn + materialized rows */ }

impl Cursor {
    pub fn execute(&mut self, sql: &str, params: &[Value]) -> Result<&mut Self, SqliteError>;
    pub fn fetchone(&mut self) -> Option<Row>;
    pub fn fetchmany(&mut self, size: usize) -> Vec<Row>;
    pub fn fetchall(&mut self) -> Vec<Row>;
    pub fn rowcount(&self) -> i64;
    pub fn lastrowid(&self) -> Option<i64>;
}
impl Iterator for Cursor { type Item = Row; /* yields rows */ }

#[derive(Clone, Debug, PartialEq)]
pub enum Value { Null, Integer(i64), Real(f64), Text(String), Blob(Vec<u8>) }

#[derive(Clone, Debug, PartialEq)]
pub struct Row { /* private: Vec<Value> */ }
impl Row {
    pub fn get(&self, index: usize) -> Option<&Value>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn cells(&self) -> &[Value];
    pub fn into_cells(self) -> Vec<Value>;
}

#[derive(Clone, Debug)]
pub struct SqliteError { pub kind: SqliteErrorKind, pub message: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqliteErrorKind {
    CannotOpen, Sql, Constraint, Parameter, TypeMismatch, Other,
}
```

## Scope window (Z.7.c)

In scope:

- `connect(path)` — in-memory (`MEMORY` / `":memory:"`) + file paths.
- Cursor lifecycle: `execute(sql, params)`, `fetchone` / `fetchmany(n)`
  / `fetchall`, `rowcount`, `lastrowid`, `Iterator` over `Row`.
- qmark (`?`) parameter binding, PEP 249 `paramstyle = "qmark"`.
- The five SQLite storage classes (NULL/INTEGER/REAL/TEXT/BLOB) mapped
  to `Value` (Python `None/int/float/str/bytes`).
- `commit` / `rollback` / `close`.

Out of scope (deferred):

- `sqlite3.Row` name-mapping factory + `row_factory` hook (positional
  only for now).
- Named / numeric paramstyles (`:name`, `?NNN`, `pyformat`).
- `executemany` / `executescript` / `iterdump`.
- Custom adapters/converters (`detect_types`, `register_adapter`).
- The `.cb`-source `import sqlite3` extern wiring (codegen layer).

## Invariants

- **No panic on SQL/connection failure.** Every fault routes to a
  `SqliteError` `Result::Err`; the surface never panics on bad SQL,
  bad params, or an unopenable path (constitution §5.1 + task contract).
- **Closed error taxonomy.** Six `SqliteErrorKind` variants; opaque
  `Box<dyn Error>` is forbidden (constitution §2.2).
- **Closed value enum.** Exactly the five SQLite storage classes; a
  `match` over a fetched cell is exhaustive at compile time.
- **Sync surface.** Public API never exposes `Future` / `async fn`
  (constitution §2.2; roadmap Z-Q4).

## @py_compat tier

`semantic`. The surface preserves PEP 249 DB-API 2.0 behaviour and
SQLite's five-storage-class type mapping, but is not `strict`
byte-for-byte parity. Documented divergences (also in
`PROVENANCE.toml [verification] divergences`):

- **Eager materialization.** A SELECT's full result set is read into an
  owned `Vec<Row>` at `execute` time; CPython fetches lazily.
  Observationally identical for `fetchone`/`fetchmany`/`fetchall`/
  iteration; differs only for very large result sets and mid-iteration
  concurrent writes (not a supported pattern).
- **Errors are `Result`, not exceptions** (constitution §2.2). Kind
  mapping mirrors which PEP 249 exception CPython would raise:
  `OperationalError`/`SqlInputError` → `Sql`; `IntegrityError` →
  `Constraint`; param-count `ProgrammingError` → `Parameter`; connect
  failure → `CannotOpen`.
- **Positional rows only** (no `sqlite3.Row` factory).
- **Lossy non-utf8 TEXT decode** (matches default `text_factory=str`).
- **Tolerant `commit`/`rollback`** treat "no transaction is active" as
  a benign no-op (matches autocommit).

## Gates (Z.7.c — none optional)

| Stage | Gate | Pass criteria | Status |
|---|---|---|---|
| L0 | spec produced | PEP 249 qmark surface + CPython oracle capture | ✅ |
| L1 | code emitted | every file has provenance header | ✅ |
| L2.build | `cargo build -p cobrust-sqlite3` | zero warnings | ✅ |
| L2.behavior | differential + fuzz | 13 CPython-oracle differential cases + ≥ 1000 seeded fuzz inputs/fn × 3 seeds | ✅ |
| L2.perf | binding-overhead | surface-translate / Rust-binding tier per ADR-0022 §6 (rusqlite is the perf reference) | ✅ |
| L3.pyo3 | PyO3-shaped wrapper | `--features pyo3` compiles per ADR-0011 | ✅ |
| L3.dependents | (deferred) | sqlalchemy/peewee widen after `.cb` extern wiring | deferred 3/3 |

## Done means (Z.7.c — DONE)

- [x] `connect` (`:memory:` + file path), `Connection`, `Cursor`,
      `Value`, `Row`, `SqliteError` translated.
- [x] qmark `?` binding + five storage classes round-trip.
- [x] CPython-`sqlite3` differential gate: CREATE/INSERT/SELECT,
      all 5 type classes, param binding, fetchone-vs-fetchall-vs-many,
      empty result, error path (bad SQL/constraint/param mismatch).
- [x] Seeded fuzz: ≥ 1000 insert/select round-trips/fn × 3 seeds,
      panic-free, lossless.
- [x] PROVENANCE.toml with oracle + `@py_compat = semantic` +
      divergences.
- [x] PyO3 wrapper + `--features pyo3` build path per ADR-0011.

## Done means (deferred — open)

- [ ] `.cb`-source `import sqlite3` intrinsic/extern wiring (codegen
      layer, CTO serial follow-on).
- [ ] `sqlite3.Row` name-mapping + `row_factory`.
- [ ] `executemany` / `executescript`.
- [ ] Downstream ORM dependents (sqlalchemy / peewee).

## Non-goals

- **Not** a complete `sqlite3` implementation — see "Scope window".
- **Not** async on its public surface (constitution §2.2; roadmap
  Z-Q4). The embedded engine is sync; rusqlite is sync; Python
  `sqlite3` is sync.
- **Not** the `.cb`-language surface wiring — Z.7.c stops at the Rust
  crate + PyO3 + tests + docs layer to avoid `crates/cobrust-codegen/`
  cross-sprint contention; the codegen extern wiring is a deferred
  serial follow-on.

## Cross-references

- `mod:requests` — sister ecosystem crate (the layout template).
- `mod:translator` — pipeline that emits ecosystem crates.
- [adr:0011](../adr/0011-pyo3-build-path.md) — PyO3 build path.
- [adr:0022](../adr/0022-translation-ecosystem-batch.md) — ecosystem
  surface-translate methodology.
- roadmap — `docs/agent/strategy/v0.7.0-network-backend-libraries-roadmap.md`
  §4.1 (sqlite3 row) + §5 (MUST-ship DB connector).
- CPython sqlite3 — https://docs.python.org/3/library/sqlite3.html.
- rusqlite crate — https://crates.io/crates/rusqlite.
- PEP 249 — https://peps.python.org/pep-0249/.
