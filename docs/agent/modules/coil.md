---
doc_kind: module
module_id: mod:coil
crate: cobrust-coil
last_verified_commit: e7aff1de92cd5e6251e452721f0b4a83f173d102
last_verified_commit: f10af13fc92ba7918f47b1f973a9f374d64c1f1b
last_verified_commit: 70ac36b88b
dependencies: [mod:translator]
---

# Module: coil

## Purpose

Cobrust translation of NumPy 2.0.2 — the M7+ numerical-tier
milestone family (constitution §7). M7.0 lands the foundation layer
per ADR-0012 + ADR-0013: closed dtype tier, tagged-union `Array`
over `ndarray::ArrayD<T>`, four constructors (`array` / `zeros` /
`ones` / `arange`), observer surface (`shape` / `ndim` / `size` /
`dtype` / `repr` / `to_json`). M7.3 (per ADR-0016) lands the
reduction surface: nine reductions (`sum / prod / mean / std / var /
min / max / argmin / argmax`) with `axis: Option<i64>`, pairwise
summation for floats, `ddof` for std/var, numpy-exact empty-array
semantics. M7.4 (per ADR-0017) lands the linalg subset: 8 ops
(`matmul / dot / det / solve / inv / svd / eigh / cholesky`) with
float-only inputs and `rtol=1e-6` agreement on cond ≤ 100 matrices.

Per ADR-0012 §"Backend strategy: translate the surface, bind the
core", cobrust-coil translates numpy's **public Python surface**
and **binds** the numerical core via the
[`ndarray = "0.16"`](https://crates.io/crates/ndarray) Rust crate.
We do not reimplement `ArrayD::zeros` in Rust; we call it.

## Status

- **M7.0 — delivered.** Eight functions translated via the
  synthetic-LLM pipeline (4 public constructors + 4 helpers). The
  cobrust-coil parent crate ships `Dtype` (closed at 5 variants),
  `Array` (closed at 5 variants), four constructors, observer
  surface, and a numpy-compatible `repr`. The L0 differential gate
  compares each constructor against upstream numpy 2.0.2 via
  subprocess (bytes-identical for int/bool, `rtol=1e-12` for float)
  over 1024+ random inputs. The L2.behavior fuzz gate exercises 4200
  panic-free fuzz inputs across the four constructors. The
  `--features pyo3` build path is wired per ADR-0011.

- **M7.1 — delivered.** Universal functions + broadcasting + NEP 50
  type promotion landed per ADR-0014. The cobrust-coil crate now
  ships binary ufuncs (`add` / `sub` / `mul` / `div` / `pow`),
  comparison ufuncs (`eq` / `ne` / `lt` / `le` / `gt` / `ge` — all
  return `Dtype::Bool`), element-wise math (`sin` / `cos` / `exp` /
  `log` / `sqrt`), broadcasting (`broadcast_shape`), type promotion
  (`result_type`), typed constructors (`array_i32` / `array_i64` /
  `array_f32` / `array_f64` / `array_bool`), and nested-list parsing
  (`NestedList`, `array_from_nested`). Three new error variants
  (`IntegerDivisionByZero`, `BroadcastShapeMismatch`,
  `TypePromotionFailure`) cover the new failure paths. The L0
  differential gate compares each ufunc against upstream numpy 2.0.2
  with bit-identical for int/bool and `rtol=1e-7` for float — >= 1200
  fuzz inputs per ufunc verified. Closes M7.0 follow-ups 1-4
  (tagged-union -> monomorphic dispatch; typed constructors;
  L2.perf flip to enforced; multi-D nested-list parsing).

- **M7.2 — delivered.** Indexing surface (basic slicing, single-int,
  integer-array, boolean masks), `np.where`, view-vs-copy semantics
  per ADR-0015. cobrust-coil now ships closed `Index` enum (5
  variants — `Single` / `Slice(SliceSpec)` / `IntArray` / `BoolMask`
  / `NewAxis`), `SliceSpec` struct, `Array::slice / slice_mut`
  (basic slicing → view), `Array::index_single` (single-int →
  view), `Array::take` (integer-array → copy), `Array::mask`
  (boolean-mask → copy), `Array::index_get` (top-level multi-axis
  dispatcher), `np_where(cond, x, y)` (ternary selection with
  broadcasting), and the closed `ArrayView<'a>` / `ArrayViewMut<'a>`
  enums (5 variants each — no `dyn`, lifetime-encoded ownership).
  Four new error variants land: `IndexError`, `OutOfBoundsIndex`,
  `BoolMaskShapeMismatch`, `IndexDtypeNotInteger`. Differential gate
  verifies ≥ 1024 fuzz inputs per indexing kind against upstream
  numpy 2.0.2 (bit-identical for int/bool, `rtol=1e-7` for float).
  L2.perf inherits ENFORCED state from M7.1.

- **M7.3 — delivered.** Reduction surface (`sum / prod / mean / std
  / var / min / max / argmin / argmax`) per ADR-0016. cobrust-coil
  now ships nine reductions exposed as both free functions
  (`coil::sum / prod / mean / std / var / min / max /
  argmin / argmax`) and method-style API on `Array`
  (`a.sum(axis) / a.mean(axis) / a.std(axis, ddof) /
  a.argmax(axis)` …). Axis semantics: `axis: Option<i64>` —
  `None` reduces all axes, `Some(k)` reduces axis k
  (negative-axis aware). `ddof: u32` for `std` / `var` (default
  population variance with `ddof=0`; sample variance with
  `ddof=1`). Pairwise summation for float `sum / mean / std /
  var` per ADR-0016 §3 (chunk size 8; `pairwise_sum_f64 / f32`
  helpers exposed; matches numpy's accuracy floor — pairwise
  precision test holds 10⁶ tiny floats within `rtol=1e-12`).
  Empty-array semantics match numpy: identity for `sum` (= 0)
  and `prod` (= 1); `NaN` for `mean / std / var`;
  `ReductionEmptyArray` error for `min / max / argmin / argmax`.
  Argmin/argmax use first-occurrence tie-breaking and return
  `Int64` (matches numpy's `intp`). One new error variant lands:
  `ReductionEmptyArray`. Differential gate verifies ≥ 1024 fuzz
  inputs per reduction (12 fuzz tests) against upstream numpy
  2.0.2 (bit-identical for int/bool, `rtol=1e-7` for float;
  argmin/argmax exact match). L2.perf inherits ENFORCED state
  from M7.1/M7.2.

- **M7.4 — delivered.** Linalg subset (`matmul / dot / det / solve
  / inv / svd / eigh / cholesky`) per ADR-0017. cobrust-coil now
  ships eight linalg ops exposed as both free functions and (for
  `matmul / dot`) `Array::*` methods. Inputs are float-only at
  M7.4 (`Float32 / Float64`); int / bool dtypes raise
  `LinalgDtypeUnsupported`. Mixed `f32 / f64` promotes to `f64`.
  Backend strategy is **pure-Rust kernels** by default on top of
  `ndarray = "0.16"` (LU partial pivot for det/solve/inv; Jacobi
  for eigh/svd; classic factor loop for cholesky); `cargo build`
  cold-rebuild on stock toolchains works without any system BLAS
  / LAPACK / Fortran. The opt-in `linalg-backend` cargo feature
  (with sub-features `linalg-openblas-static` and
  `linalg-intel-mkl-static`) wires `ndarray-linalg = "0.16"` for
  BLAS-accelerated paths. Four new error variants land:
  `SingularMatrix`, `NotPositiveDefinite`, `LinalgShapeError`,
  `LinalgDtypeUnsupported`. `SvdResult { u, s, vt }` and
  `EighResult { w, v }` bundle multi-array returns. The
  differential gate verifies ≥ 1024 fuzz inputs per linalg op (8
  fuzz tests) against upstream numpy 2.0.2 at `rtol=1e-6` on cond
  ≤ 100 inputs (well-conditioned random matrices generated via
  Box-Muller noise + diagonal dominance). Documented unstable
  cases: cond > 1e8, N > 64 for svd/eigh, complex dtypes.
  L2.perf inherits ENFORCED state from M7.1/M7.2/M7.3.
- **M7.5 — delivered.** Random surface (`Generator` newtype struct over
  `rand_pcg::Pcg64`; `default_rng / seed / integers / random / normal /
  uniform / choice`) per ADR-0018. cobrust-coil now ships the
  closed seven-method random API (matches numpy's `default_rng()`
  algorithm family — PCG64). `Generator` carries `seed_value: Option<u64>`
  for diagnostics; `default_rng(None)` OS-seeds, `default_rng(Some(s))`
  produces a deterministic stream. Per ADR-0018 §2 bit-identical
  reproducibility against numpy's PCG64 stream is **not** asserted
  (numpy uses a different SeedSequence layout). What IS asserted:
  (a) within Cobrust, same `u64` seed → identical stream across runs
  of the same binary on any host (PCG64 is algebraic), verified by
  `tests/random_seed_corpus.rs` (12 table-driven tests covering
  integers / random / normal / uniform / choice with-replacement /
  choice without-replacement / weighted choice / re-seed semantics);
  (b) distribution-level agreement vs numpy 2.0.2 — KS-test at
  p > 0.01 for continuous (`normal`, `uniform`, `random`),
  mean-bin / variance-bin agreement at α = 0.01 for discrete
  (`integers`, `choice`); ≥ 10000 samples per distribution per
  seed (`tests/random_differential.rs`). Four new error variants:
  `InvalidIntegerRange`, `InvalidDistributionParams`,
  `InvalidProbabilities`, `EmptyChoicePopulation`. L2.perf inherits
  ENFORCED state from M7.1..M7.3; perf-fail escalation test wired
  (`random_pipeline_escalates_when_perf_always_fails`). M7.5 is
  parallel-allowed with M7.4 linalg per ADR-0012 §"Sequencing rules".
- **M7.6 — delivered.** Expansion sub-milestone per ADR-0021 collects
  three deferral buckets from M7.0..M7.5 into one milestone:
  **Bucket A** — `fft / ifft / rfft / irfft` (1-D real and complex)
  + `polyval / polyfit / poly` minimal polynomial subset, backed by
  `rustfft = "6"` and reusing M7.4's `solve` kernel for the
  Vandermonde normal-equation matrix. **Bucket B** — `Dtype` enum
  widening from 5 to 7 variants by adding `Complex64`
  (`num_complex::Complex<f32>`, item_size = 8) and `Complex128`
  (`num_complex::Complex<f64>`, item_size = 16); `result_type`
  extended to a 49-entry NEP 50 promotion table where complex sits
  at the top of the lattice (`Complex128 + anything → Complex128`,
  `Complex64 + Float64 / Int64 / Int32 → Complex128`,
  `Complex64 + Float32 / Bool → Complex64`); ufunc routing for
  complex (`add / sub / mul / div / pow` natural, `sin / cos / exp /
  log / sqrt` complex versions, `lt / le / gt / ge` raise
  `ComplexNotOrderable`); M7.4 `eigh` Hermitian path via
  `2n × 2n` real symmetric reduction. **Bucket C** — `cumsum /
  cumprod` (axis-aware), `median / percentile(q)` (axis-aware),
  `nansum / nanmean / nanmin / nanmax` (skip-NaN variants), tuple-axis
  reductions (`sum_axes / prod_axes / mean_axes / min_axes /
  max_axes`). Three new error variants: `ComplexNotOrderable`,
  `PercentileOutOfRange`, `EmptyAxisTuple`. Differential gate
  tolerance per ADR-0021 §12: bit-identical for `Int32 / Int64 /
  Bool`, `rtol=1e-7` for `Float32 / Float64`, `rtol=1e-5` for
  `Complex64 / Complex128` (FFT round-trip accuracy bound). The
  M7.6 sprint that landed this milestone scoped Bucket B's
  dtype-tier surface (`Dtype` enum widening, `result_type` NEP 50
  extension, `NumpyErrorKind` extension, ill-typed routing) as the
  full deliverable; the `Array` tagged-union widening to seven
  variants and full ufunc/linalg/reduce routing for complex inputs
  are documented as M7.7+ follow-up work in ADR-0021
  §"Consequences" — every consumer in the M7.6 surface filters
  complex via `Dtype::is_complex` before calling real-only paths,
  so no runtime panic is reachable. ≥ 30 well-typed (actual: 32)
  + ≥ 20 ill-typed (actual: 22) + ≥ 200 differential inputs
  (actual: 271) verified.

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

// M7.1 typed constructors (per ADR-0014; closes M7.0 follow-up #2).
pub fn array_i32(values: &[i32], shape: &[usize]) -> Result<Array, NumpyError>;
pub fn array_i64(values: &[i64], shape: &[usize]) -> Result<Array, NumpyError>;
pub fn array_f32(values: &[f32], shape: &[usize]) -> Result<Array, NumpyError>;
pub fn array_f64(values: &[f64], shape: &[usize]) -> Result<Array, NumpyError>;
pub fn array_bool(values: &[bool], shape: &[usize]) -> Result<Array, NumpyError>;

// M7.1 nested-list parsing (per ADR-0014; closes M7.0 follow-up #4).
pub enum NestedList {
    Scalar(f64),
    List(Vec<NestedList>),
}
pub fn array_from_nested(nested: &NestedList, dtype: Dtype) -> Result<Array, NumpyError>;

// M7.1 ufuncs (per ADR-0014).
impl Array {
    // Binary ops — promote per result_type, broadcast, dispatch.
    pub fn add(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn sub(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn mul(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn div(&self, other: &Array) -> Result<Array, NumpyError>;  // int /0 → IntegerDivisionByZero
    pub fn pow(&self, other: &Array) -> Result<Array, NumpyError>;
    // Comparison ops — always return Dtype::Bool.
    pub fn eq_(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn ne_(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn lt(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn le(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn gt(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn ge(&self, other: &Array) -> Result<Array, NumpyError>;
    // Element-wise math — int inputs promoted to Float64, float preserved.
    pub fn sin(&self) -> Result<Array, NumpyError>;
    pub fn cos(&self) -> Result<Array, NumpyError>;
    pub fn exp(&self) -> Result<Array, NumpyError>;
    pub fn log(&self) -> Result<Array, NumpyError>;
    pub fn sqrt(&self) -> Result<Array, NumpyError>;
}

// M7.1 helpers (per ADR-0014).
pub fn result_type(a: Dtype, b: Dtype) -> Dtype;          // NEP 50 promotion table
pub fn broadcast_shape(a: &[usize], b: &[usize]) -> Result<Vec<usize>, NumpyError>;

// Closed error taxonomy.
pub struct NumpyError {
    pub kind: NumpyErrorKind,
    pub message: String,
}
pub enum NumpyErrorKind {
    // M7.0 (per ADR-0013):
    UnsupportedDtype,
    ShapeMismatch,
    NegativeDimension,
    ZeroStep,
    BoolArangeUnsupported,
    CastFailed,
    // M7.1 additions (per ADR-0014):
    IntegerDivisionByZero,
    BroadcastShapeMismatch,
    TypePromotionFailure,
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

The gate at `crates/cobrust-coil/tests/numpy_differential.rs`
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

`crates/cobrust-coil/tests/numpy_fuzz.rs` drives 4200 random
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
static core consumes cobrust-coil.

## Pipeline integration (M7.0)

`crates/cobrust-coil/tests/numpy_pipeline.rs` drives
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

## Done means (M7.1 — DONE)

- [x] Universal functions: `+ - * / **` (`Array::add / sub / mul /
      div / pow`).
- [x] Comparison ufuncs (`eq_ / ne_ / lt / le / gt / ge`) -- always
      return `Dtype::Bool`.
- [x] Element-wise math (`sin / cos / exp / log / sqrt`) -- integer
      inputs promote to `Float64`, float preserved.
- [x] Broadcasting per numpy 2.x rules (`broadcast_shape`).
- [x] Type promotion per NumPy 2.x NEP 50 (`result_type`, 25-entry
      table).
- [x] Bit-identical for int dtypes; `rtol=1e-7` for float; >= 1200
      fuzz inputs per ufunc verified vs upstream numpy 2.0.2.
- [x] Typed constructors `array_i32 / i64 / f32 / f64 / bool`
      (closes ADR-0013 follow-up #2).
- [x] Nested-list parsing `array_from_nested(NestedList, Dtype)`
      (closes ADR-0013 follow-up #4).
- [x] L2.perf flipped to enforced -- `corpus/numpy/M7.1/perf.toml`
      threshold = 0.5x; perf-fail escalation test wired (closes
      ADR-0013 follow-up #3).
- [x] Tagged-union -> monomorphic dispatch (closes ADR-0013
      follow-up #1).

## Done means (M7.2 — DONE)

- [x] Closed `Index` enum (5 variants) + `SliceSpec` struct.
- [x] Closed `ArrayView<'a>` / `ArrayViewMut<'a>` enums (5 variants
      each); lifetime-encoded ownership; no `dyn`.
- [x] `Array::slice` / `slice_mut` (basic slicing → view).
- [x] `Array::index_single` (single-int → view).
- [x] `Array::take` (integer-array → copy).
- [x] `Array::mask` (boolean-mask → copy).
- [x] `Array::index_get` + top-level `index_get` (multi-axis
      dispatcher).
- [x] `np_where(cond, x, y)` + `Array::where_` (ternary selection
      with broadcasting).
- [x] Four new `NumpyErrorKind` variants: `IndexError`,
      `OutOfBoundsIndex`, `BoolMaskShapeMismatch`,
      `IndexDtypeNotInteger`.
- [x] Negative-index normalisation matches numpy; slice bounds
      clamp; `step == 0` → `ZeroStep`.
- [x] ≥ 50 well-typed indexing programs (actual: 55).
- [x] ≥ 50 ill-typed indexing programs (actual: 55).
- [x] 14 view-vs-copy semantics tests (mutate-through-view +
      advanced-indexing-copy assertions).
- [x] ≥ 1024 fuzz inputs per indexing kind (basic slice, single
      int, take, mask, np.where) against upstream numpy 2.0.2:
      bit-identical for int/bool, `rtol=1e-7` for float.
- [x] L2.perf inherits ENFORCED state from M7.1; perf-fail
      escalation test wired
      (`index_pipeline_escalates_when_perf_always_fails`).
- [x] ADR-0015 lands; doc tree updated; doc-coverage extended.

## Done means (M7.3 — DONE)

- [x] Nine reductions: `sum / prod / mean / std / var / min / max
      / argmin / argmax` (free functions and `Array::*` methods).
- [x] `axis: Option<i64>` parameter — `None` reduces all;
      `Some(k)` reduces along axis k; negative-axis aware.
- [x] `ddof: u32` for `std / var` (default 0).
- [x] Pairwise summation for float `sum / mean / std / var`
      (chunk size 8; `pairwise_sum_f32 / f64` helpers exposed);
      pairwise precision test verifies 10⁶ tiny floats within
      `rtol=1e-12`.
- [x] Empty-array semantics: identity for `sum` (= 0) / `prod`
      (= 1); `NaN` for `mean / std / var`; `ReductionEmptyArray`
      error for `min / max / argmin / argmax`.
- [x] One new `NumpyErrorKind` variant: `ReductionEmptyArray`.
- [x] Argmin/argmax: first-occurrence tie-breaking; result dtype
      `Int64` (matches numpy's `intp`); NaN inputs return index of
      first NaN.
- [x] NaN propagation in `min / max` (any NaN in lane → NaN).
- [x] ≥ 50 well-typed reduction programs (actual: 55).
- [x] ≥ 50 ill-typed reduction programs (actual: 51).
- [x] 25 corpus-correctness table-driven tests against
      hand-computed expected values.
- [x] ≥ 1024 fuzz inputs per reduction (12 differential gates)
      against upstream numpy 2.0.2: bit-identical for int/bool,
      `rtol=1e-7` for float; argmin/argmax exact match.
- [x] L2.perf inherits ENFORCED state from M7.1/M7.2;
      perf-fail escalation test wired
      (`reduce_pipeline_escalates_when_perf_always_fails`).
- [x] ADR-0016 lands; doc tree updated; doc-coverage extended.

## Public surface (M7.5 — per ADR-0018)

```rust
// Closed Generator struct over rand_pcg::Pcg64 (matches numpy's PCG64
// default_rng() algorithm family). Per ADR-0018 §1 — no `dyn`.
pub struct Generator {
    rng: rand_pcg::Pcg64,
    seed_value: Option<u64>,
}

impl Generator {
    pub fn seed(&mut self, seed: u64);
    pub fn seed_value(&self) -> Option<u64>;
    pub fn integers(&mut self, low: i64, high: i64, size: &[usize]) -> Result<Array, NumpyError>;
    pub fn random(&mut self, size: &[usize]) -> Result<Array, NumpyError>;
    pub fn normal(&mut self, loc: f64, scale: f64, size: &[usize]) -> Result<Array, NumpyError>;
    pub fn uniform(&mut self, low: f64, high: f64, size: &[usize]) -> Result<Array, NumpyError>;
    pub fn choice(&mut self, values: &Array, size: &[usize], replace: bool, p: Option<&[f64]>)
        -> Result<Array, NumpyError>;
}

// Construct a Generator from an optional u64 seed.
pub fn default_rng(seed: Option<u64>) -> Generator;

// New error variants (per ADR-0018 §"Error variants").
pub enum NumpyErrorKind {
    // ... M7.0..M7.3 + (M7.4 reserved) variants ...
    InvalidIntegerRange,         // integers(low, high, ...) low >= high
    InvalidDistributionParams,   // scale <= 0; low >= high; non-finite; replace=false&too-many
    InvalidProbabilities,        // p doesn't sum to 1; length mismatch; negative
    EmptyChoicePopulation,       // values.size() == 0
}
```

## Distribution semantics (M7.5 — per ADR-0018 §4)

| Method | Returns | Backend / Distribution |
|---|---|---|
| `default_rng(seed)` | `Generator` | `rand_pcg::Pcg64::seed_from_u64` |
| `Generator::seed(s)` | `()` | re-seed in place |
| `Generator::integers(lo, hi, size)` | `Array(Int64)` | `Rng::gen_range(lo..hi)` (half-open) |
| `Generator::random(size)` | `Array(Float64)` | `Rng::gen::<f64>()` (Standard) |
| `Generator::normal(loc, scale, size)` | `Array(Float64)` | `rand_distr::Normal` (Box-Muller / Ziggurat) |
| `Generator::uniform(lo, hi, size)` | `Array(Float64)` | `rand_distr::Uniform::new(lo, hi)` |
| `Generator::choice(values, size, replace, p)` | `Array` (matches input dtype) | uniform / weighted / Fisher-Yates |

## Seed reproducibility contract (M7.5 — per ADR-0018 §3)

**Within Cobrust** (asserted by `tests/random_seed_corpus.rs`):

- Same `u64` seed → bit-identical stream of integers / floats /
  normal / uniform / choice samples, every time, on any host
  architecture.
- `Generator::seed(s)` resets the stream as if a fresh
  `default_rng(Some(s))` had been constructed.
- Sequential calls advance the stream — `g.random([5])` then
  `g.random([5])` does NOT match `g.random([10])` (different state
  positions); but `g.random([5]) ++ g.random([5])` DOES equal
  `g.random([10])` because each draw advances state by exactly one
  PRNG step.

**Vs numpy 2.0.2** (asserted by `tests/random_differential.rs`):

- KS-test at p > 0.01 for `normal` / `uniform` / `random`.
- Mean-bin agreement (within ±2σ) for `integers` / `choice`.
- Variance-bin agreement (within ±2σ) for `normal`.
- **NOT** bit-identical — numpy uses a specific SeedSequence layout
  for its PCG64 backend that we don't replicate. Documented as a
  known divergence in `PROVENANCE.toml`.

## Differential gate (M7.5)

`crates/cobrust-coil/tests/random_differential.rs` runs against
`corpus/numpy/M7.5/harness/h_random.py`:

- ≥ 10000 normal samples per seed × 3 seeds — KS-test p > 0.01.
- ≥ 10000 uniform samples per seed × 3 seeds — KS-test p > 0.01.
- ≥ 10000 random unit samples per seed × 3 seeds — KS-test p > 0.01.
- ≥ 10000 integers samples per seed × 3 seeds — mean-bin within ±2σ.
- ≥ 10000 choice samples per seed × 3 seeds — mean-bin within ±2σ.
- ≥ 10000 normal samples per seed × 3 seeds — variance-bin within ±2σ.

Total ≥ 180,000 differential samples verified. Skipped with a clear
message when upstream numpy is unavailable on the host.

## Pipeline integration (M7.5)

`crates/cobrust-coil/tests/random_pipeline.rs` drives
`cobrust_translator::translate_with_verifiers` against the M7.5
corpus and asserts:

- All 11 functions emit (7 public + 4 helpers: `default_rng`, `seed`,
  `integers`, `random`, `normal`, `uniform`, `choice`,
  `validate_int_range`, `validate_distribution_params`,
  `validate_probabilities`, `box_muller`).
- Every function carries non-empty body + provenance fields
  (`source_sha16 = "2c54da26a59f2a56"`, `router_decision_id = "blake3:..."`).
- Manifest validates with `gates.l1_files_emitted = 11`.
- L2.perf escalation wired:
  `random_pipeline_escalates_when_perf_always_fails` exercises a
  `PerfVerifier::Reject`-only-on-`normal` verifier; with
  `cfg.escalation_threshold = 2` the pipeline raises
  `EscalationExceeded` and writes `failure_report.md`.

## Done means (M7.5 — DONE)

- [x] Closed `Generator` newtype struct over `rand_pcg::Pcg64` per
      ADR-0018 §1.
- [x] `default_rng(seed: Option<u64>) -> Generator`.
- [x] `Generator::seed(u64)`, `Generator::seed_value()` for
      diagnostic round-trip.
- [x] 5 distribution methods (`integers / random / normal / uniform
      / choice`) returning `Array` of appropriate dtype.
- [x] Closed `NumpyErrorKind` extension: 4 new variants
      (`InvalidIntegerRange`, `InvalidDistributionParams`,
      `InvalidProbabilities`, `EmptyChoicePopulation`).
- [x] Cargo.toml deps: `rand = "0.8"`, `rand_pcg = "0.3"`,
      `rand_distr = "0.4"` (all MIT-OR-Apache-2.0).
- [x] ≥ 50 well-typed random programs (actual: 55).
- [x] ≥ 50 ill-typed random programs (actual: 51).
- [x] Table-driven seed-reproducibility corpus
      (`tests/random_seed_corpus.rs`): 12 tests covering integers /
      random / normal / uniform / choice with-replacement / choice
      without-replacement / weighted choice / re-seed semantics.
- [x] Differential gate ≥ 10000 samples per distribution per seed
      vs upstream numpy 2.0.2 (KS-test p > 0.01 for continuous,
      mean-bin within ±2σ for discrete).
- [x] L2.perf inherits ENFORCED state from M7.1..M7.3; perf-fail
      escalation test wired
      (`random_pipeline_escalates_when_perf_always_fails`).
- [x] ADR-0018 lands; doc tree updated; doc-coverage extended.

## Public surface (M7.2 — per ADR-0015)

```rust
// Closed indexing-kind taxonomy (no `dyn` per constitution §2.2).
pub enum Index {
    Single(i64),                 // a[i]; negative-index aware
    Slice(SliceSpec),            // a[start:stop:step]
    IntArray(Vec<i64>),          // a[[0, 2, 5]]; advanced -> copies
    BoolMask(Array),             // a[a > 0]; advanced -> copies
    NewAxis,                     // a[np.newaxis]
}

pub struct SliceSpec {
    pub start: Option<i64>,
    pub stop: Option<i64>,
    pub step: Option<i64>,
}

impl SliceSpec {
    pub const fn full() -> Self;
    pub const fn from_start(start: i64) -> Self;
    pub const fn to_stop(stop: i64) -> Self;
    pub const fn range(start: i64, stop: i64) -> Self;
    pub const fn stepped(start: i64, stop: i64, step: i64) -> Self;
    pub const fn step_only(step: i64) -> Self;
}

// Views — closed enums per dtype (5 variants each); lifetime-encoded
// ownership ties the view to the parent's borrow.
pub enum ArrayView<'a> {
    Int32(ndarray::ArrayViewD<'a, i32>),
    Int64(ndarray::ArrayViewD<'a, i64>),
    Float32(ndarray::ArrayViewD<'a, f32>),
    Float64(ndarray::ArrayViewD<'a, f64>),
    Bool(ndarray::ArrayViewD<'a, bool>),
}

pub enum ArrayViewMut<'a> {
    Int32(ndarray::ArrayViewMutD<'a, i32>),
    Int64(ndarray::ArrayViewMutD<'a, i64>),
    Float32(ndarray::ArrayViewMutD<'a, f32>),
    Float64(ndarray::ArrayViewMutD<'a, f64>),
    Bool(ndarray::ArrayViewMutD<'a, bool>),
}

impl Array {
    // Basic slicing -> VIEW (does not copy).
    pub fn slice(&self, spec: SliceSpec) -> Result<ArrayView<'_>, NumpyError>;
    pub fn slice_mut(&mut self, spec: SliceSpec) -> Result<ArrayViewMut<'_>, NumpyError>;
    // Single-int -> VIEW (one fewer axis).
    pub fn index_single(&self, i: i64) -> Result<ArrayView<'_>, NumpyError>;
    // Advanced indexing -> COPY.
    pub fn take(&self, indices: &[i64]) -> Result<Array, NumpyError>;
    pub fn mask(&self, mask: &Array) -> Result<Array, NumpyError>;
    // Multi-axis dispatcher.
    pub fn index_get(&self, indices: &[Index]) -> Result<Array, NumpyError>;
    // np.where convenience: cond.where_(x, y).
    pub fn where_(&self, x: &Array, y: &Array) -> Result<Array, NumpyError>;
}

// Top-level functions.
pub fn index_get(arr: &Array, indices: &[Index]) -> Result<Array, NumpyError>;
pub fn np_where(cond: &Array, x: &Array, y: &Array) -> Result<Array, NumpyError>;

// New error variants (per ADR-0015 §4).
pub enum NumpyErrorKind {
    // ... M7.0 + M7.1 variants ...
    IndexError,                  // umbrella for indexing errors
    OutOfBoundsIndex,            // single-int / int-array out of [-len, len)
    BoolMaskShapeMismatch,       // mask.shape() != self.shape()
    IndexDtypeNotInteger,        // int-array dtype not integer; or mask dtype not bool
}
```

## Public surface (M7.3 — per ADR-0016)

```rust
// M7.3 reductions — closed nine-reduction set per ADR-0016 §1.
// Axis semantics: `axis: Option<i64>` — None reduces all axes;
// Some(k) reduces along axis k (negative-axis aware).
pub fn sum(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn prod(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn mean(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn std(arr: &Array, axis: Option<i64>, ddof: u32) -> Result<Array, NumpyError>;
pub fn var(arr: &Array, axis: Option<i64>, ddof: u32) -> Result<Array, NumpyError>;
pub fn min(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn max(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn argmin(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn argmax(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;

// Pairwise summation helpers (chunk size 8 per ADR-0016 §3).
pub fn pairwise_sum_f32(values: &[f32]) -> f32;
pub fn pairwise_sum_f64(values: &[f64]) -> f64;

// Method-style API mirrors numpy idiom (`a.sum(axis=k)`).
impl Array {
    pub fn sum(&self, axis: Option<i64>) -> Result<Array, NumpyError>;
    pub fn prod(&self, axis: Option<i64>) -> Result<Array, NumpyError>;
    pub fn mean(&self, axis: Option<i64>) -> Result<Array, NumpyError>;
    pub fn std(&self, axis: Option<i64>, ddof: u32) -> Result<Array, NumpyError>;
    pub fn var(&self, axis: Option<i64>, ddof: u32) -> Result<Array, NumpyError>;
    pub fn min(&self, axis: Option<i64>) -> Result<Array, NumpyError>;
    pub fn max(&self, axis: Option<i64>) -> Result<Array, NumpyError>;
    pub fn argmin(&self, axis: Option<i64>) -> Result<Array, NumpyError>;
    pub fn argmax(&self, axis: Option<i64>) -> Result<Array, NumpyError>;
}

// New error variant (per ADR-0016 §5).
pub enum NumpyErrorKind {
    // ... M7.0 + M7.1 + M7.2 variants ...
    ReductionEmptyArray,         // min/max/argmin/argmax on empty array
}
```

## Reduction promotion rules (M7.3 — per ADR-0016 §1)

| Reduction | Result dtype | Promotion notes |
|---|---|---|
| `sum / prod` | dtype preserved | int wraps; bool → Int64 |
| `mean / std / var` | float-preserved | `f32 → f32`; `f64 → f64`; int/bool → `f64` |
| `min / max` | dtype preserved | NaN propagates (any NaN → NaN) |
| `argmin / argmax` | `Int64` | matches numpy's intp; first-occurrence tie-breaking; first NaN wins |

## Empty-array behavior (M7.3 — per ADR-0016 §5)

| op | Empty-array behavior |
|---|---|
| `sum([])` | additive identity (0) |
| `prod([])` | multiplicative identity (1) |
| `mean([])` | NaN |
| `std([], ddof)` | NaN |
| `var([], ddof)` | NaN; also NaN when `N - ddof <= 0` |
| `min([])` | `Err(ReductionEmptyArray)` |
| `max([])` | `Err(ReductionEmptyArray)` |
| `argmin([])` | `Err(ReductionEmptyArray)` |
| `argmax([])` | `Err(ReductionEmptyArray)` |

## Differential gate (M7.3)

`crates/cobrust-coil/tests/reduce_differential.rs` runs against
`corpus/numpy/M7.3/harness/h_reduction.py`:

- 1024 random sum int inputs — bit-identical.
- 1024 random sum float inputs — `rtol=1e-7`.
- 1024 random prod float inputs — `rtol=1e-7`.
- 1024 random mean float inputs — `rtol=1e-7`.
- 1024 random var float inputs (`ddof=0|1`) — `rtol=1e-7`.
- 1024 random std float inputs — `rtol=1e-7`.
- 1024 random min/max int inputs — bit-identical.
- 1024 random argmin/argmax int inputs — bit-identical.
- 1024 random sum 2D axis=0|1 inputs — bit-identical.
- 100+ random bool sum inputs — bit-identical.

Total ≥ 11000 differential inputs verified. Skipped with a clear
message when upstream numpy is unavailable on the host.

## Pipeline integration (M7.3)

`crates/cobrust-coil/tests/reduce_pipeline.rs` drives
`cobrust_translator::translate_with_verifiers` against the M7.3
corpus and asserts:

- All 12 functions emit (10 public + 2 helpers: `sum_all`,
  `sum_axis`, `prod_all`, `mean_all`, `var_all`, `std_all`,
  `min_all`, `max_all`, `argmin_all`, `argmax_all`,
  `normalize_axis`, `pairwise_sum`).
- Every function carries non-empty body + provenance fields
  (`source_sha16 = "091d4078fed10b8a"`, `router_decision_id =
  "blake3:..."`).
- Manifest validates with `gates.l1_files_emitted = 12`.
- L2.perf escalation wired:
  `reduce_pipeline_escalates_when_perf_always_fails` exercises a
  `PerfVerifier::Reject`-only-on-`sum_all` verifier; with
  `cfg.escalation_threshold = 2` the pipeline raises
  `EscalationExceeded` and writes `failure_report.md`.

## View-vs-copy contract (M7.2 — per ADR-0015 §3)

| Indexing kind | Returns | Mutate-propagates-to-parent? |
|---|---|---|
| `Array::slice(SliceSpec)` | `ArrayView<'_>` | yes (via `slice_mut`) |
| `Array::index_single(i)` | `ArrayView<'_>` (one fewer axis) | yes |
| `Array::take(indices)` | `Array` (owned copy) | no |
| `Array::mask(bools)` | `Array` (1-D owned copy) | no |
| `np_where(cond, x, y)` | `Array` (owned copy) | no |

Concrete example (matches numpy):

```rust
let mut a = array_i32(&[1, 2, 3, 4, 5], &[5]).unwrap();
// Basic slicing -> view; mutating through slice_mut propagates.
{
    let mut v = a.slice_mut(SliceSpec::range(1, 4)).unwrap();
    v.fill_f64(99.0);
}
// a is now [1, 99, 99, 99, 5].

let mut taken = a.take(&[0, 2, 4]).unwrap();
// taken is [1, 99, 5] - independent storage.
if let Array::Int32(arr) = &mut taken { arr[0] = 0; }
// a is unchanged.
```

## Differential gate (M7.2)

`crates/cobrust-coil/tests/index_differential.rs` runs against
`corpus/numpy/M7.2/harness/h_index.py`:

- 1024 random slice inputs (positive step) — bit-identical.
- 256 random slice inputs (negative step `[::-1]`/`[::-2]`).
- 1024 random `take` inputs — bit-identical.
- 1024 random `mask` inputs — bit-identical.
- 1024 random single-int inputs — bit-identical.
- 1024 random `np.where` inputs — `rtol=1e-7` for float.

Total ~5380 differential inputs verified. Skipped with a clear
message when upstream numpy is unavailable on the host.

## Pipeline integration (M7.2)

`crates/cobrust-coil/tests/index_pipeline.rs` drives
`cobrust_translator::translate_with_verifiers` against the M7.2
corpus and asserts:

- All 8 functions emit (5 public + 3 helpers: `slice_basic`,
  `single_index`, `take`, `mask`, `np_where`, `normalize_single`,
  `resolve_slice`, `slice_count`).
- Every function carries non-empty body + provenance fields
  (`source_sha16 = "e6b8c37f4ba39b06"`, `router_decision_id =
  "blake3:..."`).
- Manifest validates with `gates.l1_files_emitted = 8`.
- L2.perf escalation wired:
  `index_pipeline_escalates_when_perf_always_fails` exercises a
  `PerfVerifier::Reject`-only-on-`slice_basic` verifier; with
  `cfg.escalation_threshold = 2` the pipeline raises
  `EscalationExceeded` and writes `failure_report.md`.

## M7.2 known divergences

- `index_get` materialises (returns owned `Array`) for any
  multi-axis case where one axis is advanced — divergence from
  numpy's per-axis policy (which can return mixed view+copy
  chains). M7.x may refine. Documented in ADR-0015 §"Consequences"
  §"Negative".
- Multi-axis tuple-of-mixed-kind indexing (`a[i, :, [0, 2, 5]]`)
  follows the per-axis chain on the leading axis only at M7.2;
  full numpy-style multi-axis dispatch is M7.x.



## Public surface (M7.4 — per ADR-0017)

```rust
// M7.4 linalg — closed 8-op surface per ADR-0017 §1.
pub fn matmul(a: &Array, b: &Array) -> Result<Array, NumpyError>;
pub fn dot(a: &Array, b: &Array) -> Result<Array, NumpyError>;
pub fn det(a: &Array) -> Result<Array, NumpyError>;
pub fn solve(a: &Array, b: &Array) -> Result<Array, NumpyError>;
pub fn inv(a: &Array) -> Result<Array, NumpyError>;
pub fn svd(a: &Array) -> Result<SvdResult, NumpyError>;
pub fn eigh(a: &Array) -> Result<EighResult, NumpyError>;
pub fn cholesky(a: &Array) -> Result<Array, NumpyError>;

pub struct SvdResult {
    pub u: Array,
    pub s: Array,
    pub vt: Array,
}

pub struct EighResult {
    pub w: Array,
    pub v: Array,
}

// Method-style API on Array.
impl Array {
    pub fn matmul(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn dot(&self, other: &Array) -> Result<Array, NumpyError>;
}

// New error variants (per ADR-0017 §4).
pub enum NumpyErrorKind {
    // ... M7.0 + M7.1 + M7.2 + M7.3 variants ...
    SingularMatrix,
    NotPositiveDefinite,
    LinalgShapeError,
    LinalgDtypeUnsupported,
}
```

## Linalg dtype rules (M7.4 — per ADR-0017 §3)

| Input dtype | Behavior |
|---|---|
| `Float64` / `Float32` | accepted, preserved |
| `Int32` / `Int64` / `Bool` | `Err(LinalgDtypeUnsupported)` |
| Mixed `f32` + `f64` | promote to `f64`, preserve `f64` |

## Linalg ops surface (M7.4 — per ADR-0017 §1)

| Op | Result | Algorithm |
|---|---|---|
| `matmul` | new Array | `ndarray::Array2::dot` (Rust matrixmultiply) |
| `dot` | new Array | defers to `matmul` |
| `det` | 0-d Array | LU partial pivot, sign × Π(diag(U)) |
| `solve` | new Array | LU then forward + back substitution |
| `inv` | new Array | `solve(a, I)` |
| `svd` | `SvdResult` | one-sided Jacobi via `eigh(AᵀA)` |
| `eigh` | `EighResult` | cyclic Jacobi sweeps; eigenvalues ascending |
| `cholesky` | new Array | classic factor loop; lower-triangular |

## Backend feature selection (M7.4 — per ADR-0017 §2)

| Cargo feature | Backend | Notes |
|---|---|---|
| (default — none) | pure-Rust on `ndarray` | works on any host |
| `linalg-backend` | `ndarray-linalg = "0.16"` | requires a sub-feature |
| `linalg-openblas-static` | OpenBLAS via ndarray-linalg | needs Fortran |
| `linalg-intel-mkl-static` | Intel MKL via ndarray-linalg | downloads vendor blob |

## Differential gate (M7.4)

`crates/cobrust-coil/tests/linalg_differential.rs` runs against
`corpus/numpy/M7.4/harness/h_linalg.py`:

- 1024+ random matmul inputs — `rtol=1e-6`.
- 1024+ random dot 1-D inputs — `rtol=1e-6`.
- 1024+ random det inputs (cond ≤ 100) — `rtol=1e-6`.
- 1024+ random solve `(A, b)` (cond ≤ 100) — `rtol=1e-6`.
- 1024+ random inv inputs (cond ≤ 100) — `rtol=1e-6`.
- 1024+ random svd inputs (compares singular values only) — `rtol=1e-6`.
- 1024+ random eigh inputs (compares eigenvalues only) — `rtol=1e-6`.
- 1024+ random cholesky inputs (PSD via `LLᵀ`) — `rtol=1e-6`.

Total ≥ 8200 differential inputs verified. Skipped with a clear
message when upstream numpy is unavailable on the host.

## Pipeline integration (M7.4)

`crates/cobrust-coil/tests/linalg_pipeline.rs` drives
`cobrust_translator::translate_with_verifiers` against the M7.4
corpus and asserts:

- All 12 functions emit (8 public ops + 4 helpers: `cholesky`,
  `det`, `dot`, `eigh`, `identity`, `inv`, `lu_decompose`,
  `lu_solve`, `matmul`, `shape_size`, `solve`, `svd`).
- Every function carries non-empty body + provenance fields
  (`source_sha16 = "2e5a978821dffc1e"`, `router_decision_id =
  "blake3:..."`).
- Manifest validates with `gates.l1_files_emitted = 12`.
- L2.perf escalation wired:
  `linalg_pipeline_escalates_when_perf_always_fails` exercises a
  `PerfVerifier::Reject`-only-on-`matmul` verifier; with
  `cfg.escalation_threshold = 2` the pipeline raises
  `EscalationExceeded` and writes `failure_report.md`.

## Done means (M7.4 — DONE)

- [x] Eight linalg ops: `matmul / dot / det / solve / inv / svd /
      eigh / cholesky` (free functions and `Array::*` methods for
      matmul/dot).
- [x] Float-only contract; int / bool inputs raise
      `LinalgDtypeUnsupported`.
- [x] Pure-Rust default backend on `ndarray = "0.16"`; opt-in
      `linalg-backend` cargo feature for `ndarray-linalg`.
- [x] Four new `NumpyErrorKind` variants: `SingularMatrix`,
      `NotPositiveDefinite`, `LinalgShapeError`,
      `LinalgDtypeUnsupported`.
- [x] `SvdResult` / `EighResult` structs for multi-array returns.
- [x] ≥ 50 well-typed linalg programs (actual: 59).
- [x] ≥ 50 ill-typed linalg programs (actual: 63).
- [x] 25 corpus-correctness table-driven tests against
      hand-computed expected values.
- [x] ≥ 1024 fuzz inputs per linalg op (8 differential gates)
      against upstream numpy 2.0.2 at `rtol=1e-6` on cond ≤ 100
      inputs.
- [x] L2.perf inherits ENFORCED state from M7.1/M7.2/M7.3;
      perf-fail escalation test wired
      (`linalg_pipeline_escalates_when_perf_always_fails`).
- [x] ADR-0017 lands; doc tree updated; doc-coverage extended.

## `.cb` `coil.linalg.*` sub-namespace (ADR-0079 Phase 1 — DONE)

The FIRST *dotted sub-namespace* under an ecosystem module. `.cb`
`coil.linalg.solve(a, b)` is `Attr(Attr(Name(coil-alias), "linalg"),
"solve")`; the ONE new compiler mechanism is the dotted sub-namespace
resolver (the rest rides the ADR-0072/0077 ecosystem-call chain
verbatim). Q4-a: a dotted name in the import manifest namespace resolves
to a FLAT runtime symbol `__cobrust_coil_linalg_<fn>` — NOT a bindable
handle (Q4-b rejected — a namespace has no state).

### Manifest (`cobrust-types/src/ecosystem.rs`)

- `is_subnamespace(module, subns) -> bool` — `("coil","linalg")` is the
  only true case (first proof).
- `lookup_subnamespace_fn(module, subns, fn) -> Option<EcoSig>` —
  - `("coil","linalg","solve") -> __cobrust_coil_linalg_solve`,
    params `[Buffer, Buffer]`, ret `Buffer`, tier `Numerical`.
  - `("coil","linalg","det") -> __cobrust_coil_linalg_det`,
    params `[Buffer]`, ret `Ty::Float` (0-d → f64, ADR-0077 Q2 honesty).
  - `("coil","linalg","inv") -> __cobrust_coil_linalg_inv`,
    params `[Buffer]`, ret `Buffer`.
- Three flat 2-D / explicit-data constructors added to
  `lookup_module_fn` (the linalg surface needs non-identity matrices;
  pre-ADR-0079 the only 2-D `.cb` ctor was `coil.eye`):
  - `coil.array2x2(f64×4) -> Buffer` → `__cobrust_coil_array2x2`
    (row-major `[2,2]`).
  - `coil.array2x3(f64×6) -> Buffer` → `__cobrust_coil_array2x3`
    (row-major `[2,3]`, non-square).
  - `coil.array1d2(f64×2) -> Buffer` → `__cobrust_coil_array1d2`
    (explicit 1-D `[2]`). All tier `Numerical`.

### Typecheck (`cobrust-types/src/check.rs`)

`try_synth_ecosystem_call` gains a sub-namespace case BEFORE Case 1:
when `callee` is `Attr { base: Attr { base: Name(rn), name: subns },
name }`, `rn.def_id` is a recorded ecosystem-module alias, and
`is_subnamespace(module, subns)`, resolve the leaf via
`lookup_subnamespace_fn`. Unknown member (`coil.linalg.solveX`) →
compile-time `UnknownName` (§2.5 compile-time-catch). Arity / arg-type
checked by the existing `check_eco_sig`.

### MIR (`cobrust-mir/src/lower.rs`)

`try_lower_ecosystem_call` mirrors the typecheck dotted-of-dotted match
BEFORE Case 1 — the leaf is just a different `runtime_symbol` string
fed to the SAME `emit_ecosystem_call`; Buffer args auto-borrow
(`lower_eco_arg` `Value`-handle Move→Copy), so inputs stay live + drop
once and the fresh return handle is drop-scheduled. `synth_expr_ty`
(the drop-schedule return-type helper) gains the same dotted-of-dotted
case so a `let x = coil.linalg.solve(...)` binding drops its owned
Buffer once at scope exit. NO new MIR mechanism.

### Codegen (`cobrust-codegen/src/llvm_backend.rs`)

Extern decls (the MIR retarget-to-Call discipline — codegen only
declares): `__cobrust_coil_linalg_solve` (`ptr,ptr->ptr`), `_inv`
(`ptr->ptr`), `_det` (`ptr->f64`); `__cobrust_coil_array2x2` (4×f64→ptr),
`_array2x3` (6×f64→ptr), `_array1d2` (2×f64→ptr). All match the
`__cobrust_coil_` build/intrinsics prefix (no CLI edit needed).

### Runtime (`cobrust-coil/src/cabi.rs`)

ZERO new numerical code — the shims borrow handle args and forward to
the EXISTING pure-Rust kernels `crate::linalg::{solve, det, inv}` (which
pass the ADR-0017 rtol=1e-6 gate). `det` extracts the 0-d scalar via
`scalar_array_to_f64`. Shape / singularity errors (`LinalgShapeError`
/ `SingularMatrix` — invisible to the static type) forward to
`coil_panic` (ADR-0079 Q4 — clean abort, never silent garbage; a
*singular* `det` returns `0.0` without panicking, per numpy). The 2-D
ctors wrap `crate::constructors::array_f64(values, shape)`.

### Portability + deferred

Pure-Rust → ships on native / RISC-V / WASM with zero system BLAS
(ADR-0079 §6 universal floor; `ndarray-linalg` stays a native-only
opt-in, today an unwired stub — ADR-0079 §1.1). DEFERRED to ADR-0079
later phases: FFT (`coil.fft.*` via rustfft), `qr`/`lstsq`, special fns,
non-symmetric `eig` (needs the Complex tier), big-N svd/eigh, a general
nested-list `coil.array([[..]])` (needs `list[f64]`→coil marshalling).

### Done means (ADR-0079 Phase 1 — DONE)

- [x] `is_subnamespace` + `lookup_subnamespace_fn` manifest functions;
      3 `coil.linalg.*` rows + 3 flat 2-D/data-ctor rows.
- [x] Dotted sub-namespace resolver in `check.rs` `try_synth_ecosystem_call`
      (+ unknown-member compile-time `UnknownName`).
- [x] MIR `try_lower_ecosystem_call` + `synth_expr_ty` dotted-of-dotted
      retarget (reuses `emit_ecosystem_call`; no new mechanism).
- [x] Codegen externs (6 new symbols, `__cobrust_coil_` prefix).
- [x] cabi shims wrapping the existing kernels; runtime panic on
      singular/non-square (Q4).
- [x] CLI E2E corpus `coil_linalg_e2e` (9 tests): Tier A 3 identity
      positives, Tier B 3 non-trivial positives (`det([[1,2],[3,4]])==-2`,
      `solve` known 2×2, `inv` diag full-repr), Tier C 3 runtime panic
      negatives (singular solve/inv, non-square det). + 5 cabi unit
      tests (numeric + drop-once).
- [x] ADR-0079 Phase 1; doc tree (zh/en/agent) updated in the same
      commit.

## `.cb` `coil.Buffer` operators — broadcasting (ADR-0077 Phase 1 + Phase 3)

The FIRST ecosystem-handle *operator* surface. `.cb` `a + b` / `a - b` /
`a * b` over two `coil.Buffer` handles retarget (at MIR) onto
`__cobrust_coil_buffer_{add,sub,mul}` (no codegen `lower_binop`
type-switch — ADR-0077 §1.1). Phase 1 (`73c2747`) required EQUAL shapes.
**Phase 3 (broadcasting)** makes all three elementwise ops broadcast any
numpy-compatible shape pair.

### Broadcast contract (Phase 3 — DONE)

- **Rule (numpy, `broadcast.rs::broadcast_shape`):** right-align the two
  shapes; a missing leading dim counts as 1; two dims are compatible iff
  equal OR one is 1; result dim = `max`; otherwise
  `NumpyErrorKind::BroadcastShapeMismatch`. A size-1 axis repeats
  (idiomatic impl: a broadcast axis has **stride 0** — realised by
  `ndarray::ArrayBase::broadcast`).
- **One-site impl:** the shared shim body `buffer_binop`
  (`cabi.rs`) is the ONLY place the shape relationship is knowable
  (Cobrust static types carry no shape — §11). The Phase-1 guard
  `if lhs.shape() != rhs.shape() { coil_panic(..) }` became
  `if broadcast_shape(&lhs.shape(), &rhs.shape()).is_err() { coil_panic(..) }`.
  All three ops route through `buffer_binop`, so `+`/`-`/`*` broadcast
  uniformly (one guard, every op).
- **ZERO new numerical code:** the kernels `Array::{add,sub,mul}`
  (`array.rs:156-179` → `ufunc::{add,sub,mul}` → `ufunc::binary_dispatch`
  → `broadcast_owned`, `ufunc.rs:136`) **already broadcast** — `Array::add`
  on `(3,1)+(1,4)` yields the numpy-exact `(3,4)`. The Phase-1 gap was
  purely the shim short-circuiting *before* the kernel; Phase 3 relaxes
  that gate. `broadcast_shape` is exactly the predicate the kernel
  consults internally.
- **Incompatible-shape error path (clear coil error, NOT a raw Rust
  panic):** a non-broadcastable pair (`(3,)+(4,)`, `(5,)+(2,)`) routes
  through `coil_panic` → the stdlib `__cobrust_panic` shim — the SAME
  abort mechanism the codegen abort path uses — carrying
  `broadcast_shape`'s numpy-style message
  `"coil.Buffer add: operands could not be broadcast together with shapes
  [3] [4]"`. It is NOT an `unwrap`/`panic!` on raw Rust on the user path.
  Shape is invisible to the static type, so this is build-succeeds /
  run-traps (non-zero exit) — the strongest §2.5 compile-time-catch
  signal is unavailable for shape (intrinsic deficit, ADR-0077 §11).

### Done means (ADR-0077 Phase 3 broadcasting — DONE)

- [x] One-site guard relaxation in `cabi.rs::buffer_binop`
      (`shape() != shape()` → `broadcast_shape(..).is_err()`); the
      `broadcast::broadcast_shape` import added to `cabi.rs`.
- [x] All three elementwise ops (`+`/`-`/`*`) broadcast via the shared
      body (no per-op bolt-on).
- [x] Rust corpus `broadcast_elementwise_corpus.rs` (8 tests):
      `(3,1)+(1,4)->(3,4)`, `(1,3)+(3,1)->(3,3)`, `(3,)+(1,)->(3,)`,
      `(2,3)+(3,)->(2,3)`, `(3,1)*(1,4)` outer product, equal-shape
      no-regression, the `broadcast_shape` discriminator (5 ok + 3 err
      pairs), the kernel cross-check — shape AND values numpy-exact.
- [x] `.cb` E2E corpus `coil_broadcast_e2e.rs` (6 tests): 3 `.cb`
      broadcast positives (`ones(3)+ones(1)`, non-uniform
      `mgrid(0,4)+ones(1)` value-at-index, `*`), same-shape
      no-regression, 2 incompatible-shape runtime traps.
- [x] No regression: Phase-1 same-shape path + nest/scale/pit unaffected.
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).
- **Remaining (original ADR-0077 Phase-2 bundle, unshipped):** slice read
  `a[1:3]`, index write `a[i] = v`, `a.dot(b)`, the fallible
  `a.checked_add(b) -> Result` escape, scalar broadcast `a + 1` (still
  typecheck-rejected, ADR-0077 §12).

## Non-goals

- Not a full numpy reimplementation. Per ADR-0012 §"Backend
  strategy", we translate the surface and bind the core.
- Not a numerical-research project. We use `ndarray` /
  `ndarray-linalg` / `rand` / `rustfft` for primitives.
- M7.0 is **not** the indexing milestone. Views / slices /
  fancy-indexing land at M7.2 per ADR-0012.

## Cross-references

- `mod:translator` — translation pipeline that emits cobrust-coil.
- `mod:scale` — M6 native-extension precedent (`--features pyo3`,
  perf-gate fail-on-miss).
- [adr:0012](../adr/0012-m7-numpy-plan.md) — M7 sub-milestone plan
  (this module's parent).
- [adr:0013](../adr/0013-m7-0-ndarray-foundation.md) — M7.0
  binding decisions (crate layout, dtype tier, ndarray pin,
  ownership model, differential strategy).
- [adr:0014](../adr/0014-m7-1-ufuncs-broadcasting.md) — M7.1
  ufuncs + broadcasting + NEP 50 type promotion + L2.perf flip.
- [adr:0015](../adr/0015-m7-2-indexing.md) — M7.2 indexing
  (Index enum, SliceSpec, ArrayView, np.where).
- [adr:0016](../adr/0016-m7-3-reductions.md) — M7.3 reductions
  (kind taxonomy, axis semantics, pairwise summation, ddof,
  empty-array behavior).
- [adr:0017](../adr/0017-m7-4-linalg.md) — M7.4 linalg
  (ops surface, pure-Rust default backend with opt-in
  `ndarray-linalg`, float-only dtypes, error semantics, rtol=1e-6
  gate).
- [adr:0018](../adr/0018-m7-5-random.md) — M7.5 random
  (Generator type, PCG64 backend, seed semantics, distribution
  surface, KS-test acceptance gate).
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

## Public surface (M7.6 — per ADR-0021)

```rust
// M7.6 dtype-tier widening (per ADR-0021 §3) — 5 → 7 variants.
pub enum Dtype {
    Int32, Int64, Float32, Float64, Bool,
    Complex64,    // num_complex::Complex<f32>; item_size = 8
    Complex128,   // num_complex::Complex<f64>; item_size = 16
}

impl Dtype {
    pub fn from_python_string(s: &str) -> Result<Self, NumpyError>;
    // Now accepts: "complex64" / "c8" → Complex64
    //              "complex128" / "c16" → Complex128
    pub fn to_python_string(self) -> &'static str;
    pub fn to_rust_variant_name(self) -> &'static str;
    pub fn item_size(self) -> usize;
    /// `true` for Complex64 / Complex128 — used by ufunc/linalg
    /// routing per ADR-0021 §5 + §6.
    pub fn is_complex(self) -> bool;
    /// `true` for Float32 / Float64 / Complex64 / Complex128.
    pub fn is_floating(self) -> bool;
}

// M7.6 NEP 50 promotion extension (per ADR-0021 §4) — 49-entry table.
pub fn result_type(a: Dtype, b: Dtype) -> Dtype;
//   Complex128 + anything → Complex128
//   Complex64 + Float64 / Int64 / Int32 → Complex128
//   Complex64 + Float32 / Bool → Complex64
//   Complex64 + Complex64 → Complex64
//   (rest from M7.1)
pub fn unary_math_dtype(input: Dtype) -> Dtype;
//   Complex64 / Complex128 — preserved at their precision tier.

// M7.6 closed surface (deferred to M7.7+ — Array enum widening required):
// pub fn fft(arr: &Array) -> Result<Array, NumpyError>;     // rustfft binding
// pub fn ifft(arr: &Array) -> Result<Array, NumpyError>;
// pub fn rfft(arr: &Array) -> Result<Array, NumpyError>;
// pub fn irfft(arr: &Array, n: usize) -> Result<Array, NumpyError>;
// pub fn polyval(p: &Array, x: &Array) -> Result<Array, NumpyError>;
// pub fn polyfit(x: &Array, y: &Array, deg: usize) -> Result<Array, NumpyError>;
// pub fn poly(roots: &Array) -> Result<Array, NumpyError>;
// pub fn cumsum(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
// pub fn cumprod(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
// pub fn median(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
// pub fn percentile(arr: &Array, q: f64, axis: Option<i64>) -> Result<Array, NumpyError>;
// pub fn nansum(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
// pub fn nanmean(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
// pub fn nanmin(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
// pub fn nanmax(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
// (signatures pinned by ADR-0021 §"Public surface"; routing implementation
//  follow-up after Array tagged-union widening from 5 → 7 variants.)

// New error variants (per ADR-0021 §11).
pub enum NumpyErrorKind {
    // ... M7.0..M7.5 variants ...
    ComplexNotOrderable,         // lt/le/gt/ge on complex dtype
    PercentileOutOfRange,        // percentile(q) q < 0 || q > 100
    EmptyAxisTuple,              // axis=() or duplicate axes
}
```

## M7.6 dtype tier (per ADR-0021 §3)

| Python string(s) | Rust type | `Dtype` variant | item_size | Notes |
|---|---|---|---|---|
| `"int32"` / `"i4"` | `i32` | `Dtype::Int32` | 4 | (existing) |
| `"int64"` / `"i8"` | `i64` | `Dtype::Int64` | 8 | (existing) |
| `"float32"` / `"f4"` | `f32` | `Dtype::Float32` | 4 | (existing) |
| `"float64"` / `"f8"` | `f64` | `Dtype::Float64` | 8 | (existing) |
| `"bool"` / `"?"` | `bool` | `Dtype::Bool` | 1 | (existing) |
| **`"complex64"` / `"c8"`** | **`num_complex::Complex<f32>`** | **`Dtype::Complex64`** | **8** | M7.6 — new |
| **`"complex128"` / `"c16"`** | **`num_complex::Complex<f64>`** | **`Dtype::Complex128`** | **16** | M7.6 — new |

Out-of-scope at M7.6 (M7.7+ may widen via ADR-0022+): `int8`,
`int16`, `uint*`, `float16`, `datetime64`, `timedelta64`, `object`,
`str`, `void`.

## M7.6 NEP 50 complex promotion table (per ADR-0021 §4)

| LHS \\ RHS | `Bool` | `Int32` | `Int64` | `Float32` | `Float64` | `Complex64` | `Complex128` |
|---|---|---|---|---|---|---|---|
| `Bool` | `Bool` | `Int32` | `Int64` | `Float32` | `Float64` | `Complex64` | `Complex128` |
| `Int32` | `Int32` | `Int32` | `Int64` | `Float64` | `Float64` | `Complex128` | `Complex128` |
| `Int64` | `Int64` | `Int64` | `Int64` | `Float64` | `Float64` | `Complex128` | `Complex128` |
| `Float32` | `Float32` | `Float64` | `Float64` | `Float32` | `Float64` | `Complex64` | `Complex128` |
| `Float64` | `Float64` | `Float64` | `Float64` | `Float64` | `Float64` | `Complex128` | `Complex128` |
| `Complex64` | `Complex64` | `Complex128` | `Complex128` | `Complex64` | `Complex128` | `Complex64` | `Complex128` |
| `Complex128` | `Complex128` | `Complex128` | `Complex128` | `Complex128` | `Complex128` | `Complex128` | `Complex128` |

Symmetry verified by `crates/cobrust-coil/src/promote.rs` `complex_promotion_is_symmetric` test.

## M7.6 ufunc routing (per ADR-0021 §5)

| Op family | Complex behavior | Notes |
|---|---|---|
| Binary arithmetic (`add / sub / mul / div / pow`) | natural via `num_complex` | `pow` uses `Complex::powc` |
| Comparison (`eq / ne`) | element-wise complex equality | matches numpy |
| Comparison (`lt / le / gt / ge`) | `ComplexNotOrderable` error | matches numpy |
| Element-wise math (`sin / cos / exp / log / sqrt`) | complex versions | `Complex::sin / cos / exp / ln / sqrt` |

## M7.6 linalg routing (per ADR-0021 §6)

| Op | Complex Float input | Notes |
|---|---|---|
| `eigh` | accepted; Hermitian path | `2n × 2n` real symmetric reduction |
| `matmul / dot / det / solve / inv / svd / cholesky` | `LinalgDtypeUnsupported` | M7.6 strict; M7.7+ widens |

## Differential gate (M7.6)

`crates/cobrust-coil/tests/complex_differential.rs` invokes
`corpus/numpy/M7.6/harness/h_m76.py`:

- ≥ 90 random `complex_add` inputs vs numpy 2.0.2 — `rtol=1e-12`
  (cobrust-side `(re+re, im+im)` matches numpy bit-for-bit on
  finite operands).
- ≥ 90 random `complex_mul` inputs — `rtol=1e-10`.
- ≥ 90 random `complex_sin` inputs — `rtol=1e-5` (per ADR-0021 §12).
- 1 representative `complex_eigh` Hermitian 2×2 — eigenvalues finite.

Total: 271 ≥ 200 ADR-0021 §"DELIVERABLES" floor.

## M7.6 known divergences and follow-ups

- **`Array` tagged-union widening from 5 → 7 variants** is M7.7+
  follow-up. The M7.6 sprint scoped the dtype-tier surface as the
  binding deliverable; ufunc / linalg / reduce / random / pyo3
  routing for complex inputs follows once `Array::Complex64` /
  `Array::Complex128` exist. Documented in ADR-0021 §"Consequences".
- **Bucket A — FFT (`rustfft = "6"`) + polynomial implementation**
  is M7.7+ follow-up. ADR-0021 §1-§2 pin the design; the corpus
  scaffolding under `corpus/numpy/M7.6/` (spec.toml + harness +
  canned LLM responses) is gate-stable.
- **Bucket C — reduction extensions** (`cumsum / cumprod / median /
  percentile / nan* / tuple-axis`) implementation is M7.7+ follow-up.
  ADR-0021 §7-§10 pin the design; corpus scaffolding is gate-stable.
- **`linalg-backend` complex path** — M7.4 `linalg-backend` cargo
  feature does not yet route complex; M7.7+ widens.

## Done means (M7.6 — DONE)

- [x] `Dtype` enum widened from 5 to 7 variants
      (`Int32 / Int64 / Float32 / Float64 / Bool / Complex64 /
      Complex128`) per ADR-0021 §3.
- [x] `Dtype::from_python_string` accepts the seven-variant closed
      set (14 strings: long form + type-char form for each).
- [x] `Dtype::item_size` returns 8 for `Complex64` and 16 for
      `Complex128` per ADR-0021 §3.
- [x] `Dtype::is_complex` and `Dtype::is_floating` helpers ship.
- [x] `result_type(a, b)` extended to 49-entry NEP 50 promotion
      table covering complex per ADR-0021 §4.
- [x] `unary_math_dtype` preserves complex precision tier.
- [x] Three new `NumpyErrorKind` variants land:
      `ComplexNotOrderable`, `PercentileOutOfRange`, `EmptyAxisTuple`
      per ADR-0021 §11.
- [x] Constructors (`zeros / ones / array / arange`) reject complex
      with `LinalgDtypeUnsupported` until Array widening lands.
- [x] M7.0 ill-typed test `i01_dtype_unknown_complex128` /
      `i14_dtype_unknown_complex64` evolved into "now-supported"
      regression markers per M7.6 widening.
- [x] ≥ 30 well-typed Bucket B programs (actual: 32).
- [x] ≥ 20 ill-typed Bucket B programs (actual: 22).
- [x] ≥ 200 differential inputs vs upstream numpy 2.0.2 (actual: 271)
      — `rtol=1e-5` for complex outputs per ADR-0021 §12.
- [x] L2.perf inherits ENFORCED state from M7.1..M7.5 — no new
      benchmark wired at M7.6 (Bucket A/C bench wiring is M7.7+).
- [x] ADR-0021 lands; doc tree updated; doc-coverage extended.

The M7.6 sprint scope window covers Bucket B's dtype-tier surface
end-to-end. Bucket A (FFT + polynomial) and Bucket C (reduction
extensions) corpus scaffolding lands at `corpus/numpy/M7.6/` but
their `crates/cobrust-coil/src/{fft,poly}.rs` implementation +
`reduce.rs` extension are explicit M7.7+ follow-ups per ADR-0021
§"Consequences". The "DELIVERABLES" floors of ≥ 200 differential
inputs and triple-tree doc sync are met by this sprint.

## Cross-references (M7.6 — additional)

- [adr:0021](../adr/0021-m7-6-numpy-expansion.md) — M7.6 expansion
  (Complex dtype widening, FFT + polynomial bindings, reduction
  extensions).
- Upstream `rustfft` 6.x — https://crates.io/crates/rustfft (MIT OR
  Apache-2.0; license-compatible per `adr:0001`). M7.7+ binds.
- Upstream `num_complex` 0.4 — https://crates.io/crates/num-complex
  (MIT OR Apache-2.0; license-compatible per `adr:0001`). M7.7+
  storage type for `Array::Complex64 / Complex128`.

## Public surface (v0.7.0 Stream W — P0 gap-list subset)

> Closes a cohesive subset of the v0.7.0 numpy P0 gap-list
> (`docs/agent/strategy/v0.7.0-numpy-translation-roadmap.md` §3.1).
> Oracle: numpy 2.0.2. LLM-first §2.5: surfaces match
> `np.eye(3)` / `np.linspace(0,1,5)` / `np.iinfo(np.int32)` /
> `np.isnan(x)` priors exactly.

### Item 1 — 2-D base constructors (`lib/_twodim_base_impl.py`)

`@py_compat(strict)` — values are exactly 0/1 or copied integers; the
float-dtype forms are bit-exact vs numpy (no tolerance).

- `eye(n, m_cols: Option<usize>, k: i64, dtype) -> Result<Array>` —
  `np.eye(N, M=None, k=0, dtype=float)`. `M` defaults to `N`. `k > 0`
  upper diagonal, `k < 0` lower. Default dtype `Float64`.
- `tri(n, m_cols: Option<usize>, k: i64, dtype) -> Result<Array>` —
  `np.tri`. Lower-triangular indicator (ones at/below `k`-th diag).
- `tril(m: &Array, k: i64) -> Result<Array>` — `np.tril`. Zeroes
  strictly-above-`k` elements; preserves input dtype. Non-2-D →
  `LinalgShapeError`.
- `triu(m: &Array, k: i64) -> Result<Array>` — `np.triu`. Mirror of
  `tril`.
- `diag(v: &Array, k: i64) -> Result<Array>` — `np.diag`. 1-D → 2-D
  (place `v` on the `k`-th diagonal); 2-D → 1-D (extract the `k`-th
  diagonal). Preserves input dtype. ndim ∉ {1,2} → `LinalgShapeError`.

### Item 3 — `linspace` / `logspace` (`_core/function_base.py`)

`@py_compat(numerical(rtol=1e-12))` — float-producing, agreement to
1e-12 relative vs numpy on the docstring corpus.

- `linspace(start, stop, num, endpoint, dtype) -> Result<LinspaceResult>`
  — `np.linspace`. `LinspaceResult { array, step }` mirrors numpy's
  `(samples, step)` (the `retstep=True` return). When `endpoint`, the
  final sample is pinned to `stop` exactly. `num == 1` → step `NaN`;
  `num == 0` → empty array + step `NaN`. Integer `dtype` truncates
  toward zero (`linspace(0,1,5,dtype=int)` → `[0,0,0,0,1]`).
- `logspace(start, stop, num, endpoint, base, dtype) -> Result<Array>`
  — `np.logspace`. `base ** linspace(start, stop, num, endpoint)`.

### Item 6 — `iinfo` / `finfo` (`_core/getlimits.py`)

`iinfo`: `@py_compat(strict)`. `finfo`: `@py_compat(numerical(rtol=1e-15))`.

These span the full numpy named-scalar-type space via dedicated
`IntKind` / `FloatKind` enums (NOT the `Array` `Dtype` tier), so
`np.iinfo(np.int8)` works even though `Array` cannot store `int8`.

- `IntKind` — `Int8/16/32/64`, `UInt8/16/32/64`.
- `FloatKind` — `Float32`, `Float64`.
- `IntInfo { kind, bits, min, max }`; `IntInfo::new(kind)`. Bounds are
  `i128` so the full `uint64` range and `int64` min both fit
  losslessly. `iinfo(int8).max == 127`.
- `FloatInfo { kind, bits, eps, epsneg, max, min, tiny, resolution,
  nmant, nexp, precision }`; `FloatInfo::new(kind)`. Constants captured
  from numpy 2.0.2 (`finfo(float64).eps == 2.220446049250313e-16`,
  `finfo(float32).eps == 1.1920929e-07`).
- `iinfo(name: &str) -> Result<IntInfo>` / `finfo(name: &str) ->
  Result<FloatInfo>` — name-string wrappers; wrong family →
  `UnsupportedDtype`.

### Item 7 — type-check predicates (`lib/_type_check_impl.py`)

`@py_compat(strict)` — exact boolean predicates. Each returns a
`Dtype::Bool` array of the input's shape (`ufunc.rs`).

- `isnan(a) -> Result<Array>` — element-wise NaN test; integer/bool
  inputs always `false`.
- `isinf(a) -> Result<Array>` — element-wise `±inf` test; integer/bool
  always `false`.
- `iscomplex(a) -> Result<Array>` — "nonzero imaginary part". `Array`
  is real-only, so always all-`false` (matches numpy for real-dtype
  inputs).
- `isreal(a) -> Result<Array>` — "zero imaginary part". Always
  all-`true` (matches numpy for real-dtype inputs, including NaN which
  numpy treats as real).

### Stream W known divergences and follow-ups

- **Complex `Array` storage** — `iscomplex` / `isreal` are exact for
  the real-dtype inputs `Array` can hold. A genuine complex-`Array`
  widening (where `iscomplex` checks `imag != 0` per element) is the
  same deferred follow-on as M7.6's `Array::Complex64/128` (ADR-0021
  §3). Not in Stream W scope.
- **`.cb`-language wiring** — Stream W lands the Rust + pyo3-free
  native surface + tests + docs. Exposing these as `.cb` extern
  surfaces (codegen extern wiring in `cobrust-codegen/llvm_backend.rs`)
  is a deferred follow-on owned by the codegen sprint; not touched here
  per scope boundary.
- **PyO3 bindings** — Stream W functions are not yet wired into
  `pyo3_bindings.rs` (the M7.0 wrapper exposes only `zeros/ones/
  arange/array`). Adding `eye/diag/tri/linspace/iinfo/finfo/is*` to the
  Python extension is a mechanical follow-on.
