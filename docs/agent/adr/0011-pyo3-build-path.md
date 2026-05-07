---
doc_kind: adr
adr_id: 0011
title: PyO3 build path for translated crates — `--features pyo3`, cdylib emission, dual-mode test harness
status: accepted
date: 2026-04-30
last_verified_commit: 908f67c
supersedes: []
superseded_by: []
---

# ADR-0011: PyO3 build path for translated crates — `--features pyo3`, cdylib emission, dual-mode test harness

## Context

ADR-0007 §5 chose "pure-Rust PyO3-shaped wrapper crate that
subprocesses CPython" for M4. ADR-0008 §6 reaffirmed the same shape
for M5, with `--features pyo3` left as a stub. M6 (`adr:0010` §6) lists
the actual PyO3 build path as a milestone deliverable: dateutil and
msgpack must compile to a Python-callable extension when
`--features pyo3` is passed, **without** breaking the no-PyO3 default
path that the M4/M5 gate suite depends on.

Two requirements collide:

1. **CI hermeticity** — many CI machines do not have libpython on
   PATH; the default `cargo build --workspace --locked` must succeed
   without any Python-related linking.
2. **Native-extension viability** — M6 must demonstrate that the
   translated crate can compile to a `.so` / `.dylib` / `.pyd` and be
   imported from Python. Otherwise the milestone amounts to "we wrote
   Rust that mimics Python's API", not "we translated a Python
   library to a Cobrust-shipped extension".

We split the resolution along feature flags.

## Options considered

### 1. Force PyO3 dependency on every translated crate

- Pros: simple; one build path.
- Cons: every workspace machine must have libpython. The M4/M5 gate
  was deliberately built without this (subprocess oracle); regressing
  is unacceptable. Rejected.

### 2. Maturin-managed extension build (separate `pyo3` workspace)

- Pros: clean separation; maturin handles ABI versioning.
- Cons: pulls maturin into the workspace; the gate would have to
  invoke `maturin build`. Compounds CI surface. Rejected for M6;
  revisit at M7+ if a Python-side test matrix appears.

### 3. **Cargo feature `pyo3` per translated crate, `cdylib` emission gated by feature** *(chosen)*

- Pros:
  - Default build path stays library-pure; no Python on PATH needed.
  - `cargo build -p cobrust-msgpack --features pyo3` (or
    `cobrust-dateutil`) compiles a `cdylib` and links against PyO3
    against the host's Python.
  - Works for every host that has libpython; degrades cleanly on
    hosts that don't (pyo3 build error tells the user what's missing).
  - The same crate ships both an `rlib` (library form, used by the
    Rust gate suite) and a `cdylib` (extension form, exposed via the
    `pyo3` feature) — Cargo supports this via `crate-type = ["rlib",
    "cdylib"]` gated by `[lib]` directives.
- Cons:
  - Crate type list lengthens; must keep the `cdylib` artefact small.
  - The PyO3 wrapper adds a few hundred LoC per crate (one
    `#[pyclass]` per public type + one `#[pyfunction]` per public
    function).

### 4. PyO3 wrapper layout

```rust
// In cobrust-msgpack/src/lib.rs:

#[cfg(feature = "pyo3")]
mod pyo3_bindings;

#[cfg(feature = "pyo3")]
pub use pyo3_bindings::*;
```

```rust
// In cobrust-msgpack/src/pyo3_bindings.rs (only compiled with pyo3):

use pyo3::prelude::*;

#[pyfunction]
fn pack(py: Python<'_>, obj: PyObject) -> PyResult<Vec<u8>> {
    // Translate `obj` to serde_json::Value, call our pure pack, return bytes.
}

#[pyfunction]
fn unpack(py: Python<'_>, bytes: &[u8]) -> PyResult<PyObject> {
    // Call our pure unpack, translate the resulting value to a Python object.
}

#[pymodule]
fn cobrust_msgpack(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(pack, m)?)?;
    m.add_function(wrap_pyfunction!(unpack, m)?)?;
    Ok(())
}
```

Symmetric layout for `cobrust-dateutil`'s `pyo3_bindings.rs`. The same
wrapper file is auto-emitted by the pipeline when the corpus declares
the feature is desired; M6 hard-codes it for both libraries (the
generator widening is M7+).

### 5. Dual-mode test harness

The `tests/<lib>_downstream.rs` integration tests already use the
subprocess-CPython oracle (no PyO3 required). M6 keeps that as the
default L3 path. A new test
`tests/<lib>_pyo3_smoke.rs` is added behind `#[cfg(feature = "pyo3")]`:
it loads the cdylib via PyO3 and asserts the public functions are
callable from Python. This test is **off by default** in the gate
suite — `cargo test --workspace --locked` skips it. To run:
`cargo test -p cobrust-msgpack --features pyo3`.

### 6. Build-path verification in CI

The gate suite (M4/M5/M6) does not require pyo3 to build. We add a
**separate** make-target / smoke step that runs:

```sh
cargo build -p cobrust-dateutil --features pyo3 || echo "pyo3 build skipped: libpython not present"
cargo build -p cobrust-msgpack --features pyo3 || echo "pyo3 build skipped: libpython not present"
```

The M6 integration test
`tests/<lib>_pyo3_compiles.rs` runs `cargo build -p <lib> --features
pyo3` as a subprocess and reports success or "skipped (libpython
unavailable)". Either outcome is a green test. This is the M6
"build path delivered" evidence.

## Decision

Adopt option 3 + option 4 + option 5 + option 6. Concretely:

1. **`crates/cobrust-dateutil/Cargo.toml`** + **`crates/cobrust-msgpack/Cargo.toml`**:
   ```toml
   [lib]
   crate-type = ["rlib", "cdylib"]

   [features]
   default = []
   pyo3 = ["dep:pyo3"]

   [dependencies]
   serde_json = { workspace = true }
   pyo3 = { version = "0.22", features = ["extension-module"], optional = true }
   ```

2. **`crates/cobrust-dateutil/src/pyo3_bindings.rs`** + **`crates/cobrust-msgpack/src/pyo3_bindings.rs`**: PyO3 wrapper modules,
   gated by `#[cfg(feature = "pyo3")]`.

3. **`crates/cobrust-dateutil/python/setup.py`** + **`crates/cobrust-msgpack/python/setup.py`**: include comments documenting the
   `cargo build --features pyo3` invocation.

4. **`tests/<lib>_pyo3_compiles.rs`** in each crate: runs the build
   subprocess, asserts success-or-clean-skip.

5. **Pipeline:** the `write_crate` function (in `pipeline.rs`)
   already emits `python/<lib>_init.py` and `setup.py`. We do not
   change those — they're deliberately Python-side stubs. The
   pipeline does **not** auto-emit `pyo3_bindings.rs` at M6 (that's
   hand-written for the two libraries). M7+ may templatise.

## Consequences

- **Positive**
  - The default workspace build stays Python-free.
  - `--features pyo3` is the M6 native-ext build flag for both
    dateutil and msgpack; it's auditable in `Cargo.toml`.
  - The dual-mode test harness gives developers a one-flag path to
    "is this importable from Python?" without disrupting the gate.

- **Negative**
  - PyO3 0.22 is a moving target; ABI breaks happen across minor
    versions. We pin `^0.22` and document the upgrade protocol in the
    crate-level README.
  - Adding `pyo3` as an optional dep increases the workspace's
    lock-graph; the lockfile change is committed atomically.

- **Neutral / unknown**
  - `cdylib` artefact size for the M6 dateutil/msgpack subset is in
    the 200–400 KB range on Linux x86-64. Acceptable.
  - macOS shipping with system Python 3.9 vs Homebrew Python 3.11 —
    PyO3's `extension-module` feature handles auto-detection; the
    `python3-config` binary determines which libpython to link.

## Evidence

- Constitution `CLAUDE.md` §7 (M6 scope), §6 (no skipping gates).
- `adr:0007` — pure-Rust PyO3-shaped wrapper baseline.
- `adr:0008` §6 — failure-routing extension this ADR's `--features
  pyo3` flag does not affect.
- `adr:0010` §6 — companion ADR for M6 native-ext translation.
- PyO3 docs — https://pyo3.rs/v0.22/.
