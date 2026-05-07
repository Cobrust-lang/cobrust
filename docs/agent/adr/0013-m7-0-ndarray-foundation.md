---
doc_kind: adr
adr_id: 0013
title: M7.0 ndarray foundation — crate layout, dtype tier, ndarray backend pin, ownership model, differential strategy
status: accepted
date: 2026-04-30
last_verified_commit: def4b42
supersedes: []
superseded_by: []
---

# ADR-0013: M7.0 ndarray foundation — crate layout, dtype tier, ndarray backend pin, ownership model, differential strategy

## Context

ADR-0012 fixed the M7 sub-milestone breakdown and the strategic
backend principle ("translate the surface, bind the core"). It
intentionally left several M7.0-specific decisions open:

1. **Crate layout** — one parent `cobrust-numpy` crate growing across
   M7.0..M7.5 vs. one sub-crate per area (`cobrust-numpy-array`,
   `cobrust-numpy-ufunc`, …). ADR-0012 §"Per-sub-milestone
   deliverables" delegates to M7.0 to choose.
2. **`ndarray` crate version pin** — ADR-0012 mentions `ndarray` 0.16
   as the candidate; M7.0 must commit to a concrete pin and document
   why.
3. **Dtype tier** — which Python `dtype` strings map to which Rust
   types. The constitution §2.4 `@py_compat` tag says "explicit";
   M7.0 must enumerate the M7.0-scope dtypes.
4. **Ownership model** — `cobrust-numpy`'s `Array` newtype around
   `ndarray::ArrayD<T>` vs. a tagged-union enum carrying multiple
   element types. Choice has cascading consequences for M7.1
   (broadcasting), M7.2 (views), M7.3 (reductions over axes).
5. **Differential testing strategy** — how to compare cobrust-numpy
   against upstream numpy bit-for-bit (int) / `rtol`-bounded (float)
   without dragging the whole numpy import surface into the test
   harness.

These are all "decision affecting two or more files" per constitution
§6, so each lands here in M7.0's binding ADR before any code commits.

## Options considered

### 1. Crate layout — parent crate vs. sub-crate per area

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| **Parent `cobrust-numpy` crate** | Single user import; shared `Array` / `Dtype` types across M7.0..M7.5 (avoids duplicate enum lookalikes); incremental sub-modules each sub-ms; follows numpy's own monolithic shape | Larger crate as M7 progresses; CI builds slow if one of M7.x changes | **Yes** |
| Sub-crate per area (`cobrust-numpy-array`, `cobrust-numpy-ufunc`, …) | Independent CI per area; smaller failure blast radius | Cross-crate `Array` type would either duplicate or live in a third crate (`cobrust-numpy-core`); M7.1 ufuncs would need to depend on `cobrust-numpy-array`; users would import from many crates instead of one — friction | No |
| One crate per dtype | Pathologically over-fragmented | — | No |

**Pick**: parent `cobrust-numpy` crate that grows. M7.0 lands
modules `dtype` + `array` + `constructors` + `print` + `error`. M7.1
adds `ufunc` + `broadcast`. M7.2 adds `index`. M7.3 adds `reduce`.
M7.4 + M7.5 may add `linalg` + `random` as feature-gated sub-modules
to keep the dependency graph manageable (`ndarray-linalg` is a
heavyweight transitive dep; `rand` brings RNG state machinery).

### 2. `ndarray` crate version pin

| Option | Notes | Selected? |
|---|---|---|
| `ndarray = "0.16"` | Latest stable as of 2026-04; supports `ArrayD<T>` (dynamic-rank), `ArrayView`, `Zip`, `axis_iter`; MSRV 1.64 (well below our 1.94) | **Yes** |
| `ndarray = "0.15"` | Older; missing some M7.4 linalg integration improvements | No |
| `ndarray-master` (git) | Reproducibility hostile | No |

`ndarray = "0.16"` is also what ADR-0012 §"Evidence" names. License:
MIT OR Apache-2.0 — compatible per ADR-0001.

### 3. Dtype tier — M7.0 scope

ADR-0012 §"Sub-milestones" M7.0 row enumerates `int32`, `int64`,
`f32`, `f64`, `bool`. M7.0 binds the Python-string ↔ Rust-type
mapping table:

| Python dtype string | Rust type | `Dtype` enum variant | Notes |
|---|---|---|---|
| `"int32"` / `"i4"` | `i32` | `Dtype::Int32` | `numpy.int32` exact width |
| `"int64"` / `"i8"` | `i64` | `Dtype::Int64` | M7.0 default integer dtype on 64-bit hosts (matches upstream numpy) |
| `"float32"` / `"f4"` | `f32` | `Dtype::Float32` | `numpy.float32` exact width |
| `"float64"` / `"f8"` | `f64` | `Dtype::Float64` | M7.0 default float dtype (matches upstream numpy) |
| `"bool"` / `"?"` | `bool` | `Dtype::Bool` | numpy `np.bool_` (1 byte; not Rust's bit-packed `bool` of `bitvec`) |

Out-of-scope for M7.0 (M7.1+ may widen): `int8`, `int16`, `uint*`,
`float16`, `complex*`, `datetime64`, `timedelta64`, `object`, `str`,
`void`. The dtype enum is closed at M7.0 — adding a variant is a
deliberate ADR-bumpable decision so we don't accrete dtypes silently.

### 4. Ownership model — newtype around `ArrayD<T>` vs. tagged union

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| Tagged-union enum `Array { I32(ArrayD<i32>), I64(ArrayD<i64>), F32(ArrayD<f32>), F64(ArrayD<f64>), Bool(ArrayD<bool>) }` | One opaque user type; mirrors Python's "an array is just an array, dtype is a runtime attribute" model; pattern-match per dtype is natural | Constitution §2.2 "no `dyn` in cobrust-numpy public API" still satisfied (`enum` is structural, not dynamic dispatch); slight per-op overhead from match arms but compiler typically inlines | **Yes** |
| Generic `Array<T>` parametrised on element type | Zero-cost; users pick the type at compile time | Doesn't model Python's `np.array(...)`, which returns "an array" without the user picking T at the call site; would force generics on every caller | No (deferred to internal use only) |
| `Box<dyn Trait>` heterogeneous elements | Constitution §2.2 forbids `dyn` as default; this would land at the public API | — | No |

**Pick**: tagged-union `Array` enum at the public API; internally
each variant holds an `ArrayD<T>` so all `ndarray` algorithms (Zip,
fold, axis_iter) are still generic-monomorphised. The shape /
ndim / size methods dispatch via match, but they're trivial and
return early.

Views (`ArrayView` / `ArrayViewMut`) are deferred to **M7.2 indexing**
per ADR-0012's sequencing. M7.0's Array always **owns** its
`ArrayD` storage.

### 5. Differential testing strategy

The L0..L3 closed-loop methodology applies (constitution §4.2).
For M7.0's gate path, we need to compare cobrust-numpy output
against upstream numpy on every constructor. Three options:

1. **Embed a Rust-side numpy oracle via PyO3** — would require
   libpython at test time; ADR-0011 already documented why we
   subprocess CPython for the L3 oracle. Same applies. Rejected.
2. **Ship a vendored Python script per constructor + JSON-pipe
   inputs/outputs** (chosen) — same pattern as M6 msgpack
   `corpus/msgpack/upstream/msgpack_core.py` → subprocess CPython
   oracle. cobrust-numpy serialises its array to a deterministic
   JSON shape (dtype string + shape list + data list), upstream
   numpy serialises to the same shape, and the harness compares.
3. **Static reference vectors checked into the test bank** — works
   for fixed seeds but doesn't exercise the differential gate
   directly; we use this as a fallback when CPython is missing,
   not the primary mode.

For dtype tolerance:
- **Integer / bool dtypes**: bit-identical (every byte must match).
- **`float32`**: `rtol=1e-12` (numpy's `arange` uses `f64` internally
  even for `f32` output, so the down-cast may introduce 1 ULP
  drift on the last element of long ranges; we set the floor
  generously).
- **`float64`**: `rtol=1e-12` for arithmetic-derived values,
  bit-identical for direct constructors (`zeros`, `ones`).

The differential harness lives at
`corpus/numpy/M7.0/harness/h_array.py` and is invoked from
`crates/cobrust-numpy/tests/numpy_differential.rs` (subprocess
`python3` per the M6 pattern). When `python3` is unavailable, the
harness skips with a clear message — same pattern as
`crates/cobrust-msgpack/tests/msgpack_pyo3_compiles.rs`.

## Decision

Adopt all chosen options above. Concretely:

```
docs/agent/adr/0013-m7-0-ndarray-foundation.md     ← this file

corpus/numpy/M7.0/
    UPSTREAM_VERSION              # "2.0.2"
    UPSTREAM_LICENSE              # BSD-3-Clause (license-compatible per adr:0001)
    spec.toml                     # L0 spec (4 constructors + ancillaries)
    upstream/
        array_core.py             # vendored Python reference subset
    upstream_tests/
        test_array_core.py        # vendored upstream pytest subset
    canned_llm_responses.toml     # synthetic-mode response table
    harness/
        h_array.py                # L0 differential harness (subprocess oracle)
    perf.toml                     # threshold = 0.5, pass_ratio = 1.0 (numerical tier per ADR-0010 §3 — informational at M7.0)
    README.md                     # corpus README

crates/cobrust-numpy/
    Cargo.toml                    # ndarray = "0.16"; pyo3 optional per ADR-0011
    PROVENANCE.toml
    src/
        lib.rs                    # public Rust API (Array, Dtype, constructors, print)
        dtype.rs                  # Dtype enum + Python-string parsing
        array.rs                  # Array tagged-union + shape/ndim/size
        constructors.rs           # array(), zeros(), ones(), arange()
        print.rs                  # numpy-compatible repr formatting
        error.rs                  # NumpyError enum
        pyo3_bindings.rs          # PyO3 wrapper (gated by --features pyo3)
    python/
        numpy_init.py
        setup.py
    tests/
        well_typed.rs             # ≥ 50 well-typed programs
        ill_typed.rs              # ≥ 50 ill-typed programs
        numpy_pipeline.rs         # pipeline run on the corpus
        numpy_differential.rs     # subprocess CPython oracle
        numpy_fuzz.rs             # ≥ 1000 panic-free random shapes/dtypes
```

The crate layout decision is **single parent crate**; later M7.x
sub-milestones land additional modules under `cobrust-numpy/src/`
rather than spawning sibling crates. This is documented in
`docs/agent/modules/numpy.md` so M7.1's P9 has clear inheritance.

### Public surface

```rust
// crates/cobrust-numpy/src/lib.rs
pub use crate::array::Array;
pub use crate::constructors::{arange, array, ones, zeros};
pub use crate::dtype::Dtype;
pub use crate::error::{NumpyError, NumpyErrorKind};

// crates/cobrust-numpy/src/dtype.rs
pub enum Dtype {
    Int32,
    Int64,
    Float32,
    Float64,
    Bool,
}

impl Dtype {
    pub fn from_python_string(s: &str) -> Result<Self, NumpyError>;
    pub fn to_python_string(&self) -> &'static str;
    pub fn item_size(&self) -> usize;
}

// crates/cobrust-numpy/src/array.rs
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
    pub fn repr(&self) -> String;          // numpy-compatible repr()
    pub fn to_json(&self) -> serde_json::Value;  // for the differential gate
}

// crates/cobrust-numpy/src/constructors.rs
pub fn array(values: &[f64], shape: &[usize], dtype: Dtype) -> Result<Array, NumpyError>;
pub fn zeros(shape: &[usize], dtype: Dtype) -> Result<Array, NumpyError>;
pub fn ones(shape: &[usize], dtype: Dtype) -> Result<Array, NumpyError>;
pub fn arange(start: f64, stop: f64, step: f64, dtype: Dtype) -> Result<Array, NumpyError>;
```

`array(values, shape, dtype)` takes a flat `f64` buffer — caller
controls shape and dtype. Matches Python's
`np.array(list, dtype=...).reshape(shape)` semantics for the M7.0
scope (1-D + reshape; full N-dim nested-list parsing is M7.1).

### Synthetic provider — task field stays `translate`

M7.0 uses the same `task = "translate"` value the M4/M5/M6 tomli +
dateutil + msgpack pure-Python translations used. No new task value
is introduced at M7.0. (M7.1+ may extend if numpy's C-core surface
needs a new prompt template.)

### M7.0 manifest fields — `gates` for the numerical tier

`PROVENANCE.toml` `gates` for cobrust-numpy:

```toml
l2_perf = "skipped (M7.0 records, M7.1+ gates per ADR-0013 §3 informational tier)"
l3_pyo3_wrapper = "pass (subprocess CPython numpy oracle); --features pyo3 build path per ADR-0011"
l3_downstream_dependents = "deferred to M7.6+ (numpy ecosystem too large for M7.0)"

[gates.dependents]
covered = []
deferred = ["scipy", "pandas", "matplotlib"]
deferred_reason = "numpy is the foundation; downstream validation lands at M7.6+ when the M7.0..M7.5 surface is complete"
skipped = []
skipped_reason = ""
```

L2.perf is **informational only at M7.0** — pure-Rust matching
hand-tuned BLAS-routed C+SIMD numpy on simple constructors is a
near-impossible target without binding into BLAS itself. The
constructors are O(n) memory allocation; speed is dominated by
allocator and zero-fill, not algorithm. M7.1 ufuncs is where perf
becomes a real gate.

### M7.0 scope window

- **In scope**:
  - `Array` tagged-union with 5 dtype variants (i32/i64/f32/f64/bool).
  - `Dtype` enum + Python-string mapping (10 strings: long form +
    type-char form).
  - `array(values, shape, dtype)` — flat-buffer construction.
  - `zeros(shape, dtype)` — zero-fill.
  - `ones(shape, dtype)` — one-fill.
  - `arange(start, stop, step, dtype)` — half-open range.
  - `Array::shape() / ndim() / size() / dtype() / repr()` — observers.
  - L2.behavior fuzz gate (≥ 1000 inputs across constructors, all
    panic-free; differential vs upstream numpy bit-identical for
    int/bool, `rtol=1e-12` for float).
  - L0..L1 pipeline run on `corpus/numpy/M7.0/`.
  - PyO3-shaped wrapper (subprocess CPython numpy oracle).
  - `--features pyo3` build path per ADR-0011.
  - ≥ 50 well-typed programs accepted, ≥ 50 ill-typed programs
    rejected (covers shape-mismatch, dtype-mismatch, bad arange step,
    etc.).

- **Out of scope (M7.1+)**:
  - Universal functions / element-wise math (M7.1).
  - Broadcasting (M7.1).
  - Indexing / views / slicing (M7.2).
  - Reductions (M7.3).
  - `linspace`, `logspace`, `geomspace` (M7.1+).
  - Multi-D nested-list parsing for `np.array([[1,2],[3,4]])` (M7.1+).
  - Additional dtypes (i8/i16/u8/u16/u32/u64, f16, complex) — M7.1+.
  - `numpy.empty`, `numpy.full`, `numpy.eye`, `numpy.identity` —
    deferred (M7.1 covers `full`; the rest land opportunistically).
  - L3 downstream dependents (numpy is the foundation; dependent
    libraries are M7.6+).

## Consequences

- **Positive**
  - The M7.0 deliverable is delivery-shaped: ~5 modules in one
    crate, with a 5-variant dtype enum that's explicit and closed.
  - "Translate the surface, bind the core" lands concretely:
    cobrust-numpy's constructors call `ndarray`'s constructors;
    we don't reimplement `ArrayD::zeros` in Rust. The translated
    layer is the **dispatch + dtype + Python-shaped contract**.
  - The differential strategy is a clean port of M6's msgpack
    pattern; the curator (next P9 / a future M7.1 P9) can reuse
    the same skeleton.
  - The dtype enum is closed at 5 variants — adding `int8` etc. is
    an explicit ADR-0014+ decision later, not silent accretion.
  - Constitution §2.2 "no `dyn` in cobrust-numpy public API" is
    satisfied: `Array` is a closed enum, not a trait object.

- **Negative**
  - Tagged-union dispatch adds a match per public-API call; for
    constructors this is negligible, but for M7.1 ufuncs the cost
    will compound. M7.1's ADR will likely revisit and adopt
    monomorphic dispatch via macros for the hot path.
  - cobrust-numpy now depends on `ndarray = "0.16"`; transitive
    dependency surface grows. Mitigated: ndarray is MIT/Apache-2.0
    and has no native deps.
  - L2.perf is informational at M7.0 — we deliberately defer the
    numerical-tier perf gate to M7.1 ufuncs. Documented above.

- **Neutral / unknown**
  - The M7.0..M7.5 parent-crate decision means a future "split the
    crate" refactor (if M7.4 linalg gets too heavy) is possible but
    requires its own ADR. We commit now and revisit only if the
    crate becomes unwieldy.
  - `array(values: &[f64], ...)` taking f64 input forces every
    caller to f64-cast their integer inputs. M7.1 will add
    typed constructors (`array_i32`, `array_i64`, …) once ufuncs
    are in. M7.0 keeps the surface tight.

## Evidence

- ADR-0012 (M7 sub-milestone plan) — strategic backend decision
  this ADR refines for M7.0.
- ADR-0007 / ADR-0008 / ADR-0010 / ADR-0011 — pipeline +
  closed-loop + native-ext + PyO3 precedent.
- Constitution `CLAUDE.md` §2.2 (no `dyn` default), §2.4
  (`@py_compat` numerical tier), §4.2 (L0..L3 gates), §7 (M7+
  scope).
- `ndarray` crate — https://crates.io/crates/ndarray (MIT OR
  Apache-2.0; license-compatible per ADR-0001).
- numpy upstream — https://github.com/numpy/numpy (BSD-3-Clause;
  license-compatible per ADR-0001).
- M6 msgpack precedent — `crates/cobrust-msgpack/`,
  `corpus/msgpack/`.
