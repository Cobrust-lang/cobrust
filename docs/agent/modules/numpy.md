---
doc_kind: module
module_id: mod:numpy
crate: cobrust-numpy
last_verified_commit: def4b42
dependencies: [mod:translator]
---

# Module: numpy

## Purpose

Cobrust translation of NumPy 2.0.2 — the M7+ numerical-tier
milestone family (constitution §7). M7.0 lands the foundation layer
per ADR-0012 + ADR-0013: closed dtype tier, tagged-union `Array`
over `ndarray::ArrayD<T>`, four constructors (`array` / `zeros` /
`ones` / `arange`), observer surface (`shape` / `ndim` / `size` /
`dtype` / `repr` / `to_json`).

Per ADR-0012 §"Backend strategy: translate the surface, bind the
core", cobrust-numpy translates numpy's **public Python surface**
and **binds** the numerical core via the
[`ndarray = "0.16"`](https://crates.io/crates/ndarray) Rust crate.
We do not reimplement `ArrayD::zeros` in Rust; we call it.

## Status

- **M7.0 — delivered.** Eight functions translated via the
  synthetic-LLM pipeline (4 public constructors + 4 helpers). The
  cobrust-numpy parent crate ships `Dtype` (closed at 5 variants),
  `Array` (closed at 5 variants), four constructors, observer
  surface, and a numpy-compatible `repr`. The L0 differential gate
  compares each constructor against upstream numpy 2.0.2 via
  subprocess (bytes-identical for int/bool, `rtol=1e-12` for float)
  over 1024+ random inputs. The L2.behavior fuzz gate exercises 4200
  panic-free fuzz inputs across the four constructors. The
  `--features pyo3` build path is wired per ADR-0011.

- **M7.1 — pending.** Universal functions + broadcasting; lands the
  next sub-milestone per ADR-0012.

## Public surface (M7.0)

```rust
// Closed dtype tier per ADR-0013 §3.
pub enum Dtype {
    Int32,
    Int64,
    Float32,
    Float64,
    Bool,
}

impl Dtype {
    pub fn from_python_string(s: &str) -> Result<Self, NumpyError>;
    pub fn to_python_string(self) -> &'static str;
    pub fn to_rust_variant_name(self) -> &'static str;
    pub fn item_size(self) -> usize;
}

// Tagged-union Array per ADR-0013 §4. Closed at 5 variants for M7.0.
pub enum Array {
    Int32(ndarray::ArrayD<i32>),
    Int64(ndarray::ArrayD<i64>),
    Float32(ndarray::ArrayD<f32>),
    Float64(ndarray::ArrayD<f64>),
    Bool(ndarray::ArrayD<bool>),
}

impl Array {
    pub fn dtype(&self) -> Dtype;
    pub fn shape(&self) -> Vec<usize>;
    pub fn ndim(&self) -> usize;
    pub fn size(&self) -> usize;
    pub fn repr(&self) -> String;          // numpy-compatible array_repr
    pub fn to_json(&self) -> serde_json::Value;
    pub fn shape_size(shape: &[usize]) -> usize;
}

// Constructors (per ADR-0013 §"Public surface").
pub fn array(values: &[f64], shape: &[usize], dtype: Dtype) -> Result<Array, NumpyError>;
pub fn zeros(shape: &[usize], dtype: Dtype) -> Result<Array, NumpyError>;
pub fn ones(shape: &[usize], dtype: Dtype) -> Result<Array, NumpyError>;
pub fn arange(start: f64, stop: f64, step: f64, dtype: Dtype) -> Result<Array, NumpyError>;
pub fn arange_count(start: f64, stop: f64, step: f64) -> usize;
pub fn array_repr(arr: &Array) -> String;

// Closed error taxonomy.
pub struct NumpyError {
    pub kind: NumpyErrorKind,
    pub message: String,
}
pub enum NumpyErrorKind {
    UnsupportedDtype,
    ShapeMismatch,
    NegativeDimension,
    ZeroStep,
    BoolArangeUnsupported,
    CastFailed,
}
```

## Dtype tier (M7.0 — per ADR-0013 §3)

| Python string(s) | Rust type | `Dtype` variant | Notes |
|---|---|---|---|
| `"int32"` / `"i4"` | `i32` | `Dtype::Int32` | exact 32-bit signed |
| `"int64"` / `"i8"` | `i64` | `Dtype::Int64` | M7.0 default integer dtype on 64-bit hosts |
| `"float32"` / `"f4"` | `f32` | `Dtype::Float32` | exact single-precision |
| `"float64"` / `"f8"` | `f64` | `Dtype::Float64` | M7.0 default float dtype |
| `"bool"` / `"?"` | `bool` | `Dtype::Bool` | 1-byte numpy form |

Out-of-scope at M7.0 (M7.1+ may widen via ADR-0014+): `int8`,
`int16`, `uint*`, `float16`, `complex*`, `datetime64`,
`timedelta64`, `object`, `str`, `void`.

## Differential gate (M7.0)

The gate at `crates/cobrust-numpy/tests/numpy_differential.rs`
drives the upstream numpy 2.0.2 oracle via subprocess
(`corpus/numpy/M7.0/harness/h_array.py`):

- **Bytes-identical** for `Int32`, `Int64`, `Bool` dtypes.
- **`rtol = 1e-12`** for `Float32`, `Float64` dtypes.

Exercises 1024+ random constructor calls (covers `zeros`, `ones`,
`arange`) plus a curated 32-case basic suite per constructor.
When upstream numpy is unavailable on the host (e.g., CI without
Python+numpy), the gate skips with a clear message — same pattern
as M6 msgpack's `tests/msgpack_pyo3_compiles.rs`.

## Fuzz gate (M7.0)

`crates/cobrust-numpy/tests/numpy_fuzz.rs` drives 4200 random
inputs across the four constructors with deterministic seeds
(`[42, 1337, 0xDEADBEEF]` per the `verification.seeds` manifest
field). Asserts:

1. **Panic-freedom**: every input either returns
   `Ok(Array)` or `Err(NumpyError)` cleanly.
2. **Round-trip**: every successful output's `to_json()` payload
   round-trips through `serde_json` without loss.
3. **Observer stability**: `shape() / ndim() / size() / dtype() /
   repr()` never panic on any successful Array.

Total fuzz budget: 4200 calls (3 seeds × 350 per seed × 4
constructors) — exceeds constitution §4.2 floor of 1000 per
public function.

## Well-typed / ill-typed gate (M7.0)

Per ADR-0013 §"M7.0 scope window": ≥ 50 well-typed + ≥ 50 ill-typed
programs. Actual: 55 well-typed (`tests/well_typed.rs`) + 56
ill-typed (`tests/ill_typed.rs`).

The "type" check is the runtime contract; most shape/dtype/value
mismatches surface as `Result::Err(NumpyError { kind })` at the
M7.0 surface. M7.1+ may lift some into compile-time errors as the
static core consumes cobrust-numpy.

## Pipeline integration (M7.0)

`crates/cobrust-numpy/tests/numpy_pipeline.rs` drives
`cobrust_translator::translate_with_verifiers` against the M7.0
corpus and asserts:

- All 8 functions emit (4 public + 4 helpers).
- Every function carries a non-empty body + provenance fields
  (`source_sha16`, `router_decision_id`, `provider`, `model`).
- The assembled `parser.rs` contains every function as a `pub fn`.
- The manifest validates with `gates.l1_files_emitted = 8`.

Per ADR-0013 §"Synthetic provider — task field stays `translate`":
M7.0 reuses the M4/M5/M6 task value; no new task is introduced.

## Invariants

- **Closed dtype set.** Adding `Int8` / `Float16` / `Complex` etc.
  is an ADR-bumpable decision, not a silent variant addition.
- **Owned storage at M7.0.** `Array` always owns its `ArrayD<T>`
  buffer. Views (`ArrayView` / `ArrayViewMut`) are deferred to M7.2
  indexing per ADR-0012.
- **Backend-bound, not reimplemented.** `zeros` / `ones` / `arange`
  delegate to `ndarray::ArrayD`'s constructors. Per ADR-0012
  §"Backend strategy".
- **Differential bytes-identical for int/bool.** Any deviation from
  upstream numpy 2.0.2 on the M7.0 dtype tier is a behavior-gate
  failure.

## Done means (M7.0 — DONE)

- [x] `Array` enum with 5 dtype variants compiles + lints clean.
- [x] `Dtype::from_python_string` accepts the closed set
      (10 strings) and rejects everything else with
      `NumpyErrorKind::UnsupportedDtype`.
- [x] Four constructors emit Array via `ndarray::ArrayD`.
- [x] ≥ 50 well-typed programs accepted (actual: 55).
- [x] ≥ 50 ill-typed programs rejected (actual: 56).
- [x] ≥ 1000 fuzz inputs panic-free (actual: 4200).
- [x] Differential vs upstream numpy 2.0.2 on basic constructors —
      bytes-identical for int/bool, `rtol=1e-12` for float, ≥ 1024
      fuzz inputs verified.
- [x] PyO3-shaped wrapper compiles under `--features pyo3`.
- [x] Pipeline integration test drives the M7.0 corpus end-to-end.
- [x] PROVENANCE.toml validates with `gates.l1_files_emitted = 8`.
- [x] ADR-0013 lands; doc tree updated; doc-coverage extended.

## Done means (M7.1 — PENDING)

- [ ] Universal functions: `+ - * / **`, `np.add/subtract/...`.
- [ ] Element-wise math (`sin`, `cos`, `exp`, `log`, `sqrt`).
- [ ] Broadcasting rules implemented per numpy.
- [ ] Bit-identical for int dtypes; `rtol=1e-7` for float; 1000+
      input differential corpus per ufunc.

## Non-goals

- Not a full numpy reimplementation. Per ADR-0012 §"Backend
  strategy", we translate the surface and bind the core.
- Not a numerical-research project. We use `ndarray` /
  `ndarray-linalg` / `rand` / `rustfft` for primitives.
- M7.0 is **not** the indexing milestone. Views / slices /
  fancy-indexing land at M7.2 per ADR-0012.

## Cross-references

- `mod:translator` — translation pipeline that emits cobrust-numpy.
- `mod:msgpack` — M6 native-extension precedent (`--features pyo3`,
  perf-gate fail-on-miss).
- [adr:0012](../adr/0012-m7-numpy-plan.md) — M7 sub-milestone plan
  (this module's parent).
- [adr:0013](../adr/0013-m7-0-ndarray-foundation.md) — M7.0
  binding decisions (crate layout, dtype tier, ndarray pin,
  ownership model, differential strategy).
- [adr:0007](../adr/0007-translator-pipeline.md) — pipeline.
- [adr:0010](../adr/0010-native-ext-translation.md) — native-ext
  methodology M7.0 inherits.
- [adr:0011](../adr/0011-pyo3-build-path.md) — PyO3 build path.
- Constitution `CLAUDE.md` §2.4 (`@py_compat(numerical, rtol=…)`),
  §4.2 (L0..L3 gates), §7 (M7+ "the big one").
- Upstream `ndarray` — https://crates.io/crates/ndarray (MIT OR
  Apache-2.0; license-compatible per `adr:0001`).
- Upstream NumPy — https://github.com/numpy/numpy (BSD-3-Clause;
  license-compatible per `adr:0001`).
