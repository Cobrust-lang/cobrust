---
doc_kind: module
module_id: mod:tomli
crate: cobrust-tomli
last_verified_commit: TBD
dependencies: [mod:translator]
---

# Module: tomli

## Purpose

The first library translated end-to-end by `mod:translator` (M4
deliverable). Pure-Rust TOML 1.0 parser subset, mirroring the
[`tomli`](https://github.com/hukkin/tomli) Python library's `loads()`
public surface. **Auto-generated** — every byte of `src/` is emitted
by the translator pipeline; the crate is committed to the repo for
gate stability.

## Status

- **M4 — delivered.** All gates green:
  - L0: `corpus/tomli/spec.toml` + harness committed.
  - L1: 12 functions translated via synthetic-LLM mode; provenance
    headers per function; `PROVENANCE.toml` validates.
  - L2.build: zero warnings on `cargo build --release`.
  - L2.behavior: 27 positive + 5 negative cases match CPython
    `tomllib`; 1024-input panic-free fuzz; 1050-input differential
    fuzz vs CPython.
  - L3 (PyO3-shaped wrapper): subprocess-based differential gate
    against CPython `tomllib`; pure-Rust API surface ready for M5
    PyO3 build flip (`--features pyo3`).
- **Out of scope for M4 (M5 widens)**: native PyO3 extension build,
  downstream-dependents validation, perf gate.

## Public surface (M4)

```rust
// crate root re-exports.
pub use cobrust_tomli::{loads, table_to_json, to_json, TomliError, Value};

pub fn loads(src: &str) -> Result<BTreeMap<String, Value>, TomliError>;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Str(String),
    Array(Vec<Value>),
    Table(BTreeMap<String, Value>),
}

#[derive(Clone, Debug)]
pub struct TomliError { pub message: String, pub pos: usize }

pub fn to_json(value: &Value) -> serde_json::Value;
pub fn table_to_json(t: &BTreeMap<String, Value>) -> serde_json::Value;
```

## Scope window (M4)

In scope (CPython `tomllib` is the oracle for inputs in this list):

- Decimal integers with optional `+`/`-` sign.
- `true` / `false`.
- `"..."` basic strings with escapes `\n \t \r \\ \"`.
- `'...'` literal strings (no escapes).
- Bare keys: `[a-zA-Z0-9_-]+`.
- `key = value` pairs, comments (`#...`).
- `[a.b.c]` table headers; arbitrarily nested.
- `[v1, v2, ...]` arrays with optional trailing comma.
- `{ k = v, ... }` inline tables (TOML 1.0 — no trailing comma).
- CRLF line endings.

Out of scope (deferred to M5 widening — inputs outside this set are
not required to match CPython):

- Multi-line strings (`"""..."""`).
- Hex / octal / binary integers; underscores in numerals; floats;
  infinity / NaN.
- Datetime / time / date types.
- Array-of-tables (`[[...]]`).
- Inline-table key paths (`a.b.c = 1`).

## Provenance

Every emitted file in `src/` carries a comment header:

```text
// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: tomli 2.0.1
// oracle: cpython 3.11 (module: tomllib)
// functions translated: 12
// see PROVENANCE.toml for the full manifest.
```

Per-function blocks in `parser.rs` carry a one-liner:

```text
// fn:loads provider=synthetic model=tomli-canned-v1 cache_hit=false decision_id=blake3:<hex>
```

The full manifest at `crates/cobrust-tomli/PROVENANCE.toml` records:

- Source library + version + 64-hex SHA-256.
- Oracle runtime + version + import path.
- Verification seeds (`[42, 1337, 0xDEADBEEF]`) + per-function fuzz
  budget (1024 default).
- Router strategy (`synthetic`) + models used.
- Toolchain string + `deterministic_id` (BLAKE3).
- L0..L3 gate evidence.

## Done means (M4 — DONE)

- [x] `cobrust translate corpus/tomli` produces `cobrust-tomli/`.
- [x] PyO3-shaped wrapper directory present (`python/`); subprocess
      differential gate against CPython `tomllib` passes.
- [x] `tomli` upstream test bank (27 positive + 5 negative) passes
      against `loads()` + the CPython oracle.
- [x] Manifest captures: source SHA, oracle versions, fuzz seeds,
      router decisions, deterministic build ID.

## Done means (M5)

- [ ] Replace synthetic provider with at least one real-LLM
      smoke-test invocation.
- [ ] Build + ship the native PyO3 extension under `--features pyo3`.
- [ ] Run downstream dependents' testsuites (top-5) against the
      wrapper.
- [ ] Land the L2.perf gate (≥ 0.8× CPython `tomllib` throughput on a
      pinned benchmark corpus).

## Non-goals

- **Not** a full TOML 1.0 implementation — see "Scope window" above.
- **Not** hand-written. Editing `src/parser.rs` or `src/lib.rs`
  directly is forbidden; regenerate via the pipeline.

## Cross-references

- `mod:translator` — pipeline that emits this crate.
- `adr:0007` — translator architecture + provenance schema.
- `corpus/tomli/README.md` — vendored upstream + scope window doc.
- Constitution `CLAUDE.md` §4.2 (translator pipeline), §7 (M4 done).
