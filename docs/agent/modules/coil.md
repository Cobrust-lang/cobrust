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
      div / pow`). `Array::div` is the integer-floor surface (numpy `//`:
      int/int floor-divides, int/0 raises); `Array::true_div` (ADR-0077
      Phase-1 completion) is the numpy-`/` true-division surface (int/bool
      promote to float, int/0 → IEEE inf) — the `.cb` `/` operator.
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
  `a.checked_add(b) -> Result` escape. (Scalar broadcast `a + 1` and
  true-division `a / b` SHIPPED — see below.)

## `.cb` `coil.Buffer` — true-division `a / b` + scalar `a ⊕ k` (ADR-0077 Phase-1 completion)

Completes the elementwise-arithmetic surface: the `/` operator (numpy
**true division**) and the scalar-broadcast forms `a + 1` / `a - 1` /
`a * 2` / `a / 2` (`coil.Buffer ⊕ python int|float`). Both ship through the
SAME MIR-retarget-to-`Terminator::Call` discipline as `+`/`-`/`*` — codegen
declares the externs only, no `lower_binop` type-switch.

### `a / b` — true-division (numpy `/` = `true_divide`)

- **Surface symbol:** `__cobrust_coil_buffer_div(a, b: *mut Buffer) -> *mut Buffer`
  (`cabi.rs`), routed through the shared broadcast-aware `buffer_binop`
  body onto **`Array::true_div`** (`array.rs` → `ufunc::true_div`).
  Broadcasts free like `+`/`-`/`*` (one guard, every op).
- **TRUE division, NOT floor-division — the heart of the gap.** numpy's `/`
  is `true_divide`: it ALWAYS yields a FLOAT result. `ufunc::true_div`
  promotes BOTH operands to the float dtype (`promote::true_div_dtype`:
  int/bool → `Float64`; `float32/float32 → float32`; any `float64 →
  float64`) BEFORE dividing, so:
  - `int / int → float64` (`[1,2,3]/[2] → [0.5,1.0,1.5]`, NOT integer
    floor `[0,1,1]`);
  - `int / 0 → IEEE +inf`, `0 / 0 → NaN` (numpy RuntimeWarning, NEVER a
    `coil_panic`/error).
- **DISTINCT from `Array::div`.** `Array::div` (`ufunc::div`) is the
  dtype-preserving integer-floor surface (numpy `//`): int/int floor-divides
  in the integer dtype and raises `IntegerDivisionByZero` on int/0 — pinned
  by `ufunc_well_typed::t14_div_int_int_returns_int` + the `ufunc_ill_typed`
  `IntegerDivisionByZero` corpus. The completion adds `true_div` as the
  numpy-`/` operator surface and leaves `div` UNCHANGED (no regression).
  Only `true_div` is wired into the `/` operator.
- **Manifest:** `lookup_buffer_binop(COIL_BUFFER_ADT, BinOp::Div)` →
  `__cobrust_coil_buffer_div` (`ecosystem.rs`). The typecheck `synth_bin`
  Buffer arm now enumerates `+`/`-`/`*`/`/` (resolved via
  `lookup_buffer_binop`); `//`/`%`/`**`/`@` on a Buffer still reject with
  the §2.5-B fix-printing diagnostic.

### `a ⊕ k` — scalar broadcast (`+`/`-`/`*`/`/` with a python scalar)

- **Surface symbols:** `__cobrust_coil_buffer_{add,sub,mul,div}_scalar(a:
  *mut Buffer, k: f64) -> *mut Buffer` (`cabi.rs`). The shared body
  `buffer_binop_scalar` materialises `k` as a 1-element `Float64` buffer
  (`array_f64(&[k], &[1])`) and forwards to the SAME broadcast kernel the
  array-array ops use — numpy's `array ⊕ scalar` IS exactly a `(1,)`
  broadcast. So all four ops get scalar support through one path (and `/`
  correctly true-divides). The `(1,)`-vs-`(N,)` broadcast is always
  compatible → the only abort is a kernel error (never for IEEE division).
- **Typecheck (`check.rs::synth_bin`):** a NEW arm BEFORE the
  `unify(lt, rt)` step (a Buffer never unifies with Int/Float, so `a + 1`
  would otherwise fail at `unify` — the pre-completion rejection). When the
  LHS resolves to the Buffer handle (bare or `&`-borrowed) AND the RHS is a
  numeric scalar (`Ty::Int`/`Ty::Float`, bare or `&`) AND
  `lookup_buffer_scalar_binop(op).is_some()` (the four arith ops), it
  returns `coil_buffer_ty()`. A non-numeric RHS (`a + s`, `s: str`) does NOT
  match and falls through to `unify`, which rejects it
  (`test_neg_buffer_plus_str_rejected` stays red).
- **MIR (`lower.rs::lower_bin`):** a NEW scalar guard BEFORE the array-array
  Buffer guard (the array-array guard keys only on the LHS type, so `a + 1`
  would otherwise wrongly route to the `(a, b: *Buffer)` shim with `1`
  lowered as i64). It retargets to `__cobrust_coil_buffer_<op>_scalar(a,
  k: f64)`: the Buffer is a BORROWED handle (Move→Copy upgrade — survives +
  drops once at scope exit), and the scalar is passed as `f64` (an `Int`
  operand is cast i64→f64 via `CastKind::IntToFloat`; a `Float` is already
  f64). Manifest helper: `lookup_buffer_scalar_binop` (`ecosystem.rs`).

### Done means (ADR-0077 Phase-1 completion — DONE)

- [x] `Array::true_div` (`ufunc::true_div` + `promote::true_div_dtype`):
      int/bool promote to float, IEEE division, total (no error path except
      broadcast-shape) — int/int → float64, int/0 → inf, 0/0 → nan.
- [x] cabi `__cobrust_coil_buffer_div` (true-division) + the four
      `__cobrust_coil_buffer_<op>_scalar` shims (length-1-broadcast reuse).
- [x] Manifest `(COIL_BUFFER_ADT, BinOp::Div)` row + `lookup_buffer_scalar_binop`.
- [x] Typecheck `synth_bin`: `/` accepted via `lookup_buffer_binop`; scalar
      arm admits `Buffer ⊕ Int/Float`; `//`/`%`/`**`/`@` + `a + str` still
      reject.
- [x] MIR `lower_bin`: scalar retarget (i64→f64 cast) before the array-array
      guard; `/` array-array retarget free via the existing guard.
- [x] Codegen externs: `__cobrust_coil_buffer_div` (binop type) + four
      `*_scalar` shims (`(ptr, f64) -> ptr`).
- [x] Rust corpus `div_scalar_elementwise_corpus.rs` (13 tests): int/int →
      float true-division (`[1,2,3]/[2]→[0.5,1,1.5]`, `[7,3]/[2,2]→[3.5,1.5]`),
      int/0 → inf + 0/0 → nan, f64 value/broadcast/div-by-zero oracles,
      scalar `+`/`-`/`*`/`/` value oracles, `+`/`*` shim no-regression.
- [x] `.cb` E2E corpus `coil_div_scalar_e2e.rs` (10 tests): `/` exact
      (`[10,20]/[2,4]→[5,5]`), fractional discriminator (`0.5` present),
      broadcast, div-by-zero → `inf` (build + run, NOT trap); scalar `a+1`,
      `a*2`, `a-1`, `a/2`; same-shape `+` + broadcast `*` no-regression.
- [x] Inverted the two now-obsolete negatives in `coil_ops_e2e.rs`
      (`a + 1` + `a / b` now ACCEPTED) + a NEW `a // b` (floor-div) negative
      pinning the op-set boundary.
- [x] No regression: `Array::div` integer-floor surface UNCHANGED (43
      cobrust-coil suites green incl. `ufunc_well_typed`/`ufunc_ill_typed`);
      `+`/`-`/`*` + broadcast E2E green; types/mir unit corpora green.
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## `.cb` `coil.Buffer` — left-scalar `k ⊕ a` + comparison `a cmp b` (ADR-0077 Phase-2/3)

Two additions reusing the EXISTING runtime (zero new numerics). Same mechanism
as Phase-1: `synth_bin` guard → `lower_bin` retarget-to-`Call` → cabi shim →
`Array` kernel (codegen `lower_binop` never reached — ADR-0077 §1.1).

### (A) Left-scalar `k ⊕ a` — mirror of right-scalar `a ⊕ k`

The scalar on the LHS (`2 * a`, `6 / a`), the form numpy users write (§2.5).
Dispatch turns on whether `⊕` commutes:

- **`+` / `*` commute** (`k + a == a + k`) → REUSE the right-scalar shims
  `__cobrust_coil_buffer_{add,mul}_scalar(a, k)`, no new symbol. The MIR retarget
  passes the buffer as the handle arg, the scalar as `k: f64`.
- **`-` / `/` do NOT commute** → NEW REVERSED shims
  `__cobrust_coil_buffer_{rsub,rdiv}_scalar(a, k)` computing `k - a[i]` / `k / a[i]`.
  The cabi `buffer_binop_scalar_rev` puts `k` on the LEFT (`f(&array([k]), a)`),
  forwarding to the SAME `Array::sub` / `Array::true_div` kernels (so `/` is
  numpy true-division — `k/0 → inf`, never a trap). **Decision rationale:** the
  reversed shim keeps the `(ptr, f64) -> ptr` ABI (reusing `coil_scalar_binop_ty`);
  the alternative (materialise `k` as a buffer at MIR time + array-array path)
  would mint a fresh handle to drop for no benefit.

- **Manifest:** `lookup_buffer_left_scalar_binop(op)` — commute → `*_scalar`,
  non-commute → `r*_scalar` (`ecosystem.rs`).
- **Typecheck (`synth_bin` arithmetic arm):** a left-scalar block BEFORE `unify`:
  LHS `Int`/`Float` (bare or `&`), RHS the Buffer handle, op has a left-scalar
  shim → `coil_buffer_ty()`. `1 + str` still rejects (non-Buffer RHS falls to
  `unify`).
- **MIR (`lower_bin`):** a left-scalar block (buffer = handle via Move→Copy,
  scalar cast i64→f64, symbol via `lookup_buffer_left_scalar_binop`).
- **Codegen:** 2 extern rows `__cobrust_coil_buffer_{rsub,rdiv}_scalar`
  (`coil_scalar_binop_ty`).

### (B) Buffer-buffer comparison `a cmp b` → Bool-dtype `coil.Buffer`

The six `<`/`<=`/`>`/`>=`/`==`/`!=`. **Load-bearing semantic:** the result is a
`coil.Buffer` of dtype **Bool** (a NumPy mask), NOT a Cobrust `bool` scalar —
`np.array([1,5]) < np.array([3,2])` is `array([True, False])`. Binds as
`let m: coil.Buffer = a < b`, prints `array([True, False], dtype=bool)`.

- **Manifest:** six arms added to `lookup_buffer_binop` (the SAME table as
  `+`/`-`/`*`/`/`) → `__cobrust_coil_buffer_{lt,le,gt,ge,eq,ne}`, `ret` =
  `coil_buffer_ty()` (the static handle carries no dtype; the bool-mask vs.
  float-buffer distinction is the deferred dtype-parameterized-handle).
- **Typecheck (`synth_bin` COMPARISON arm, NOT arithmetic):** a Buffer-vs-Buffer
  guard BEFORE `unify` returning `coil_buffer_ty()` instead of `Ty::Bool` —
  required because a Buffer DOES unify with a Buffer, so the arm would otherwise
  mis-type the mask as a scalar bool and mis-compile.
- **MIR:** NO new arm — comparison ops reach the existing `lookup_buffer_binop`
  guard in `lower_bin` unintercepted (the `str ==` guard is gated on `Ty::Str`).
- **cabi:** six shims forward through the shared `buffer_binop` body onto
  `Array::{lt,le,gt,ge,eq_,ne_}` (array.rs:210-259 — UNCHANGED; always
  `Dtype::Bool`). NB the trailing-underscore `eq_`/`ne_` (the `eq`/`ne` idents
  collide with `PartialEq`); `lt`/`le`/`gt`/`ge` do not.
- **Codegen:** 6 extern rows (`coil_binop_ty`, the `(ptr, ptr) -> ptr` shape).
- Broadcasts via the shared body (`(3,)` vs `(1,)` → a length-3 mask).

### Out of scope (NOT shipped — reject with §2.5 FIX)

- **Buffer-vs-SCALAR comparison `a < 1`** — the comparison arm detects a Buffer
  on EITHER side with a non-Buffer other operand and rejects with a fix-printing
  diagnostic ("comparing a coil.Buffer with a scalar is not yet supported …
  compare against a same-shape buffer, e.g. `a < b`"), not the bare `unify`
  mismatch. Follow-up: a scalar-comparison shim + admit.
- **`@` matmul** — SHIPPED in the next increment below (the arithmetic-arm
  reject set is now `//`/`%`/`**`; `@` is accepted between two buffers).

### Done means (ADR-0077 Phase-2/3 — DONE)

- [x] cabi: `buffer_binop_scalar_rev` + `__cobrust_coil_buffer_{rsub,rdiv}_scalar`;
      `__cobrust_coil_buffer_{lt,le,gt,ge,eq,ne}` (via shared `buffer_binop`).
- [x] `array.rs` UNCHANGED (comparison kernels pre-existed; reversed reuses
      `sub`/`true_div`).
- [x] Manifest: 6 comparison arms in `lookup_buffer_binop` +
      `lookup_buffer_left_scalar_binop`; 6 ecosystem unit tests.
- [x] Typecheck: left-scalar arm; Buffer-buffer comparison guard; Buffer-vs-scalar
      §2.5 reject; arithmetic reject names comparison.
- [x] MIR: left-scalar retarget block; comparison needs no new arm.
- [x] Codegen: 2 reversed-scalar + 6 comparison extern rows.
- [x] `.cb` E2E: `coil_left_scalar_e2e.rs` (8 — incl. REVERSED discriminators
      `10 - [2,4]=[8,6]`, `8 / [2,4]=[4,2]`, commute `3 * a`/`1 + a`, float
      `0.5 * a`, div-by-0 → inf, `1 + str` reject) + `coil_compare_e2e.rs`
      (10 — one mask per op, `<=`/`>=` equal-boundary, `==`-is-a-mask, `!=`
      inverse, broadcast, `&a < &b`, `a < 1` / `1 < a` §2.5 rejects).
- [x] No regression: 9 coil E2E suites green (72 tests, env-override path);
      touched-crate unit corpora green; clippy clean on touched crates.
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## `.cb` `coil.Buffer` — matrix-multiply `a @ b` (ADR-0077 §"@-operator")

The `@` operator (`BinOp::MatMul`) on two buffers, reusing the EXISTING
runtime matmul (ZERO new numerics). Same mechanism as every prior op:
`synth_bin` guard → `lower_bin` retarget-to-`Call` → cabi shim → `Array`
kernel (codegen `lower_binop` never reached — ADR-0077 §1.1).

**Load-bearing semantic:** `@` is MATRIX multiplication (`np.matmul`), NOT
element-wise (`*` is element-wise). `Buffer @ Buffer -> Buffer` ALWAYS — the
matrix `(m,k)@(k,n)->(m,n)` and matrix-vector `(m,k)@(k,)->(m,)` /
`(k,)@(k,n)->(n,)` cases yield an array; the degenerate 1-D·1-D `(k,)@(k,)`
yields numpy's 0-d scalar, but Cobrust has NO 0-d scalar type (ADR-0077 Q2),
so the f64-returning `a.dot(b)` METHOD is the surface for that case and `@`
always types to `coil.Buffer`. Shape conformability is a RUNTIME check
(panic-on-mismatch, ADR-0077 Q4 — the static handle carries no shape).

- **Manifest:** ONE arm added to `lookup_buffer_binop` (the SAME table as
  `+`/`-`/`*`/`/` + the comparisons) → `(COIL_BUFFER_ADT, BinOp::MatMul)` =>
  `__cobrust_coil_buffer_matmul`, `ret` = `coil_buffer_ty()`.
- **Typecheck (`synth_bin` ARITHMETIC arm):** `a @ b` (both Buffer) `unify`s
  (Buffer-vs-Buffer) then resolves through the existing `lookup_buffer_binop`
  accept path. A NEW guard BEFORE `unify` rejects `Buffer @ scalar` /
  `scalar @ Buffer` (XOR of the two `is_buf` flags, gated on `op == MatMul`)
  with a §2.5 fix-printing diagnostic ("matrix multiplication `@` requires
  BOTH operands to be a coil.Buffer … use `*` to scale … `a @ b` or
  `a @ coil.eye(a.size)`") — without it a one-Buffer `@` would fall to the
  bare `unify` "expected Adt, found i64" (a §2.5-B miss). The scalar-broadcast
  shims intentionally do NOT cover `@` (`lookup_buffer_{,left_}scalar_binop`
  return `None` for `MatMul`), so `+`/`-`/`*`/`/` with one scalar still take
  their shim path and never hit this guard. The reject set named by the
  arithmetic-arm message is now `//`/`%`/`**` (no longer `@`).
- **MIR:** NO new arm — `a @ b` reaches the existing `lookup_buffer_binop`
  array-array guard in `lower_bin` unintercepted (the scalar guards return
  `None` for `MatMul`; `a @ scalar` was already rejected at typecheck so MIR
  never sees it). Both operands borrowed (Move→Copy), one fresh handle out.
- **cabi:** a DEDICATED `__cobrust_coil_buffer_matmul(a, b: *mut Buffer) ->
  *mut Buffer` shim — NOT the shared `buffer_binop` body, because that runs a
  `broadcast_shape` pre-check, but matmul conformability is the inner-dim
  alignment rule (`a.shape[-1] == b.shape[-2]`), NOT broadcasting — a valid
  `(2,3)@(3,4)` is NON-broadcastable and would be wrongly aborted. The shim
  forwards STRAIGHT to `Array::matmul` (→ `linalg::matmul`, UNCHANGED) and
  `coil_panic`s on its `Err` (shape-mismatch / dtype) — NEVER unwinding across
  the C-ABI (ADR-0077 Q4 trap discipline, same abort path as `buffer_binop`).
- **Codegen:** 1 extern row `__cobrust_coil_buffer_matmul` (`coil_binop_ty`,
  the `(ptr, ptr) -> ptr` shape).

### Out of scope (DEFERRED — NOT shipped)

- **`Buffer @ scalar` / `scalar @ Buffer`** — rejected at typecheck with a
  §2.5 FIX (above); matmul needs two arrays.
- **Batched / N-D matmul, in-place `@=`, mixed-rank broadcasting matmul** —
  noted, not implemented (`linalg::matmul` is rank-1/2 at M7.4 per ADR-0017;
  a rank-≥3 input traps via the kernel's `_ => shape_err` arm).

### Perf (CLAUDE.md §5.2/§5.3 — measured, HONEST)

3-tier benchmark `crates/cobrust-coil/benches/matmul.rs` +
`docs/agent/benchmarks/coil-matmul.md` (square `N x N`, N=16/64/256). HONEST
result — coil LOSES both ratios (no fabricated win):

- **`T3/T1` (coil vs numpy) `> 1` and grows** (`1.76×`→`3.43×`→`5.90×`):
  numpy `@` is BLAS (Accelerate on the rig), coil's default backend is
  `ndarray::Array2::dot` (pure-Rust, no BLAS). The gap is ndarray-vs-BLAS, NOT
  coil's wiring — it MOTIVATES **#157** (pure-Rust BLAS-class linalg, e.g.
  faer). Proven by `T2/T1` (raw ndarray, no Cobrust) ALSO `>1` and growing.
- **`T3/T2` (coil vs its own ndarray ceiling) `> 1` and grows**
  (`1.96×`→`2.88×`→`4.25×`): this IS coil's wrapping, but NOT the FFI floor
  (that amortizes) — it is the FIVE O(N²) marshalling copies in
  `linalg::matmul` (`to_f64`×2 + `to_vec`×2 + out-`collect`). Named follow-up:
  a same-dtype 2-D fast path calling `Array2::dot` on the input views directly
  (the #166-elementwise-fast-path analogue; a numerics change, out of THIS
  task's "zero new numerics" scope).

### Done means (ADR-0077 §"@-operator" — DONE)

- [x] cabi: `__cobrust_coil_buffer_matmul` (dedicated; forwards to
      `Array::matmul`, `coil_panic` on `Err`, NO `broadcast_shape` pre-check).
- [x] `array.rs` / `linalg.rs` UNCHANGED (the matmul kernel pre-existed —
      zero new numerics).
- [x] Manifest: 1 `(COIL_BUFFER_ADT, BinOp::MatMul)` arm in
      `lookup_buffer_binop`; 2 ecosystem unit tests (resolve + behind-borrow);
      the obsolete `MatMul.is_none()` assertion removed from
      `buffer_binop_still_rejects_unsupported_ops`.
- [x] Typecheck: `a @ b` accepted via `lookup_buffer_binop`; `Buffer @ scalar`
      / `scalar @ Buffer` §2.5 reject (MatMul-gated XOR guard); arithmetic
      reject message reset to `//`/`%`/`**`.
- [x] MIR: NO new arm (existing array-array guard drives it).
- [x] Codegen: 1 `__cobrust_coil_buffer_matmul` extern row (`coil_binop_ty`).
- [x] `.cb` E2E corpus `coil_matmul_e2e.rs` (7 tests): 2x2@2x2 exact product
      `[[19,22],[43,50]]`, matrix-vector `[17,39]`, `a @ eye(2) == a`,
      `&a @ &b` borrow form, `(2,3)@(2,2)` runtime shape-mismatch TRAP (clean
      abort, "not aligned"), `a @ 2` + `2 @ a` §2.5 rejects.
- [x] Perf: 3-tier `matmul` bench + `coil-matmul.md` report (HONEST loss,
      root-caused to ndarray-vs-BLAS + matmul marshalling; motivates #157).
- [x] No regression: all coil E2E suites green; types (`ecosystem`/`well`/
      `ill`/`python_semantics`) green; touched crates build clean.
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## `.cb` `coil` scalar statistics — `ptp` / `nan*` / `percentile` (#145 — DONE)

NaN-aware + spread scalar aggregates extending the Stream-W P0 `mean` /
`median` / `std` / `var` family. Every member reduces the whole Buffer to
one `f64` on the proven `coil_agg_ty` ABI (`ptp` / `nansum` / `nanmean` /
`nanstd`), except `percentile`, which takes a trailing `f64` quantile —
the FIRST coil aggregate with a scalar arg BESIDE the handle
(`(Buffer, f64) -> f64`). All BORROW the handle (the shim never
reboxes/frees it); the `.cb`-side form is `coil.ptp(&a)` /
`coil.percentile(&a, 50.0)` (ADR-0052a explicit shared borrow; the
non-Copy handle survives for later reductions).

### Semantics (numpy 2.0.2 oracle — `coil::aggregates`)

- `coil.ptp(a) -> f64` — peak-to-peak `max(a) - min(a)`. NaN-propagating;
  single-elem → `0.0`; empty → `NaN` (numpy raises; we degrade for a
  panic-free shim).
- `coil.nansum(a) -> f64` — sum treating NaN as zero. All-NaN / empty →
  `0.0` (NOT NaN — matches `np.nansum`).
- `coil.nanmean(a) -> f64` — mean over the non-NaN elements only. All-NaN
  / empty → `NaN`.
- `coil.nanstd(a) -> f64` — population std (ddof=0) over the non-NaN
  elements. Single finite → `0.0`; all-NaN / empty → `NaN`.
- `coil.percentile(a, q) -> f64` — `q`-th percentile (`q` in `[0,100]`),
  numpy default `linear` interpolation: sort, `pos = (n-1)·q/100`,
  `sorted[⌊pos⌋] + frac·(sorted[⌈pos⌉] - sorted[⌊pos⌋])`. `q=0`→min,
  `q=100`→max, `q=50`==median. NaN-propagating; `q` clamped to `[0,100]`;
  empty → `NaN`. (NaN-SKIPPING `nanpercentile` deliberately NOT in this
  batch.) Integer / bool inputs promote to `f64` for all five.

### Manifest (`cobrust-types/src/ecosystem.rs`)

- 5 `lookup_module_fn` arms; 4 are `[coil_buffer_ty()] -> Ty::Float`,
  `percentile` is `[coil_buffer_ty(), Ty::Float] -> Ty::Float`. Tier
  `Semantic` (rtol=1e-12 vs the oracle). 5 manifest unit tests.

### Typecheck / MIR — ZERO new code

- The generic module-fn path (`try_synth_ecosystem_call` Case 1 /
  `try_lower_ecosystem_call` Case 1) already lowers any
  `lookup_module_fn` signature. `percentile`'s mixed `[handle, f64]` arg
  list rides the SAME `lower_eco_arg` per-param path the `array2x2(f64×4)`
  ctor already proved (the handle auto-borrows Move→Copy; the `f64`
  lowers verbatim). No `_ => "any"` gap, no new MIR arm.

### Codegen (`cobrust-codegen/src/llvm_backend.rs`)

- 5 extern rows: 4 reuse `coil_agg_ty` (`f64 (ptr)`); `percentile` adds
  `coil_agg2_ty` (`f64 (ptr, f64)`). Symbols ride the existing
  `__cobrust_coil_` build/intrinsics prefix (no CLI edit needed).

### Runtime (`cobrust-coil/src/cabi.rs`)

- 5 shims `__cobrust_coil_{ptp,nansum,nanmean,nanstd,percentile}`
  forwarding to `aggregates::{ptp,nansum,nanmean,nanstd,percentile}_scalar`.
  Null-handle sentinel: `nansum` → `0.0`, the other four → `NaN`. 6 cabi
  unit tests (incl. null-handle + drop-once accounting).

### Done means (#145 — DONE)

- [x] `aggregates.rs`: 5 `*_scalar` fns + shared `to_f64_vec` flatten/
      promote helper; 24 unit tests with bit-confirmed numpy-2.0.2 literal
      oracle values (incl. empty / NaN / single-elem / integer-promotion /
      out-of-range-q-clamp edges).
- [x] cabi: 5 shims + 6 cabi unit tests.
- [x] Manifest: 5 ecosystem arms + 5 manifest unit tests.
- [x] Typecheck / MIR: NO new code (generic module-fn path).
- [x] Codegen: 5 extern rows (`coil_agg_ty` ×4 + new `coil_agg2_ty`).
- [x] `.cb` E2E `coil_stats_e2e.rs` (4 tests): `mgrid+ptp+nansum+
      percentile` (`4`/`10`/`2`), `array1d2+nanmean+nanstd` (`3`/`1`),
      `percentile(_,25)` interpolation (`175` = 1.75×100), `str` quantile
      §2.5 reject. + `examples/coil_stats/main.cb`.
- [x] No regression: `coil_p0_e2e` / `coil_hello_e2e` green; types
      (`ecosystem`) green; touched crates build + clippy + fmt clean.
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## `.cb` `coil` array manipulation — `transpose` / `flatten` / `ravel` / `concatenate` / `vstack` / `hstack` (#145 BATCH 2 — DONE)

The Buffer-RETURNING combine + reshape surface — the array-manipulation
ops most-used in real numpy code (§2.5 training-data overlap). Wired
EXACTLY like the `@` matmul operator (borrow-Buffer-args →
fresh-Buffer-return), NOT the scalar-return stats. The cut line is the
ARITY CONTRACT: only the 1-arg (`transpose`/`flatten`/`ravel`) and the
2-array (`concatenate`/`vstack`/`hstack`) forms ship; the N-array
`concatenate([a,b,c,...])` and shape-tuple `reshape(a,(m,n))` forms are
DEFERRED (need `list[Buffer]` / tuple marshalling that does not exist
yet). The `.cb`-side form is `coil.transpose(a)` / `coil.concatenate(a,
b)` — module free functions (NOT a sub-namespace).

### Semantics (numpy 2.x oracle — `coil::manipulate`)

- `coil.transpose(a) -> Buffer` — reverse all axes (`a.T`). A 1-D array
  is returned UNCHANGED (numpy: `np.array([1,2,3]).T` is `(3,)`); a 2-D
  `(m,n)` becomes `(n,m)`. Dtype + values preserved. Infallible.
- `coil.flatten(a) -> Buffer` — 1-D C-order (row-major) copy. Infallible.
- `coil.ravel(a) -> Buffer` — 1-D C-order copy. numpy returns a VIEW when
  possible; the handle ABI has no view-into-parent surface, so this is an
  owned copy with IDENTICAL values (Semantic tier). Delegates to
  `flatten`. Infallible.
- `coil.concatenate(a, b) -> Buffer` — join along axis 0 (default
  `np.concatenate` axis). Same rank + matching sizes on every axis except
  axis 0; mismatch → `ShapeMismatch` (numpy `ValueError`).
- `coil.vstack(a, b) -> Buffer` — stack row-wise. 1-D `(n,)` operand
  promoted to `(1,n)` (`atleast_2d`), then concat axis 0:
  `vstack((n,),(n,)) -> (2,n)`, `vstack((r,c),(s,c)) -> (r+s,c)`.
- `coil.hstack(a, b) -> Buffer` — stack column-wise. 1-D operands concat
  axis 0 (`hstack((p,),(q,)) -> (p+q,)`); ≥2-D concat axis 1
  (`hstack((r,c1),(r,c2)) -> (r,c1+c2)`).

**Dtype contract**: 1-arg ops are dtype-generic (all five variants
preserved). The 2-array combine ops require equal dtype and raise
`ShapeMismatch` otherwise — numpy promotes a mixed pair; we keep the
clean equal-dtype contract (every `.cb` Buffer ctor emits `Float64`, so
the common path is always `f64`+`f64`; a silent cross-dtype promotion is
the §2.2-forbidden implicit coercion). Mixed-dtype promotion is a tracked
follow-up.

### Manifest (`cobrust-types/src/ecosystem.rs`)

- 6 `lookup_module_fn` arms. 3 are `[coil_buffer_ty()] -> coil_buffer_ty()`
  (`transpose`/`flatten`/`ravel`); 3 are `[coil_buffer_ty(),
  coil_buffer_ty()] -> coil_buffer_ty()` (`concatenate`/`vstack`/
  `hstack`). Tier `Semantic` (pure layout/combine, no float arithmetic;
  values/shape/dtype agree exactly, except `ravel`'s view-vs-copy + the
  equal-dtype combine contract — both documented).

### Typecheck / MIR — ZERO new code

- The generic module-fn path (`try_synth_ecosystem_call` Case 1 /
  `try_lower_ecosystem_call` Case 1) already lowers any `lookup_module_fn`
  signature. The 2-Buffer-arg → Buffer combine ops ride the SAME
  borrow-args → fresh-Buffer-return path proven by `coil.linalg.solve(a,
  b)`'s identical `(Buffer, Buffer) -> Buffer` shape: `emit_ecosystem_call`
  iterates `sig.params` regardless of arity, `lower_eco_arg` auto-borrows
  each Buffer arg (Move→Copy, so both inputs stay live + drop once), and
  the fresh return handle is drop-scheduled. NO `_ => "any"` gap, NO new
  MIR arm.

### Codegen (`cobrust-codegen/src/llvm_backend.rs`)

- 6 extern rows: the 3 single-arg reshape ops reuse `coil_shape_ty`
  (`ptr -> ptr`); the 3 two-array combine ops reuse `coil_binop_ty`
  (`ptr, ptr -> ptr`). Symbols ride the existing `__cobrust_coil_`
  build/intrinsics prefix (no CLI edit needed).

### Runtime (`cobrust-coil/src/manipulate.rs` + `cabi.rs`)

- `manipulate.rs`: 6 kernels over the closed `Array` enum
  (`transpose`/`flatten`/`ravel` dtype-generic per-variant;
  `concatenate`/`vstack`/`hstack` via a shared `concat_axis` over
  `ndarray::concatenate(Axis, &views)` with a dtype + rank + axis-bound
  pre-guard). 17 unit tests, differential vs the numpy oracle.
- `cabi.rs`: 6 shims `__cobrust_coil_{transpose,flatten,ravel,concatenate,
  vstack,hstack}`. The 1-arg shims are infallible; the 2-array shims share
  a `buffer_combine` body that `coil_panic`s on a non-conformable /
  dtype-mismatch `Err` (numpy `ValueError`) — NEVER unwinding across the
  C-ABI (mirrors `buffer_binop` + `buffer_matmul`). 7 cabi unit tests
  (round-trip + drop-once accounting).

### Deferred

- N-array `concatenate([a,b,c,...])` + shape-tuple `reshape(a,(m,n))` —
  need `list[Buffer]` / tuple marshalling (not yet present).
- Axis-parameterized `concatenate(a, b, axis=k)` — needs a keyword/scalar
  axis arg surface; today axis is fixed per-op (concat=0, vstack=0 post-
  atleast_2d, hstack=1-for-2D/0-for-1D).
- Mixed-dtype promoting combine — the equal-dtype contract is the §2.5
  honest minimum (`.cb` ctors emit f64 only).

### Done means (#145 BATCH 2 — DONE)

- [x] `manipulate.rs`: 6 kernels + `concat_axis`/`atleast_2d_row`/
      `reshape_to`/`flatten_c`/`owned_c` helpers; 17 unit tests with the
      numpy-2.x oracle (incl. 1-D-unchanged transpose, transpose∘flatten
      F-order values, non-conformable + rank-mismatch + dtype-mismatch
      errors, empty).
- [x] cabi: 6 shims (3 infallible + 3 via shared `buffer_combine` trap) +
      7 cabi unit tests (round-trip + drop-once).
- [x] Manifest: 6 ecosystem arms (3 `Buffer->Buffer`, 3 `Buffer,Buffer->
      Buffer`).
- [x] Typecheck / MIR: NO new code (generic module-fn path; 2-Buffer-arg
      proven by `linalg.solve`).
- [x] Codegen: 6 extern rows (`coil_shape_ty` ×3 + `coil_binop_ty` ×3).
- [x] `.cb` E2E `coil_manipulate_e2e.rs` (8 tests): transpose `(2,3)->
      (3,2)`, flatten/ravel `(2,2)->(4,)`, concatenate `(4,3)`, vstack
      `(4,3)`, hstack `(2,6)`, transpose∘concatenate `(3,4)` chain, +
      non-conformable concatenate RUNTIME trap (non-zero exit).
- [x] No regression: full `cobrust-coil` suite green (212 lib unit +
      every test binary); touched crates build + clippy `-D warnings` +
      fmt clean; no new dep (F64 — `ndarray` already present).
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## `.cb` `coil` unary transcendental — `exp` / `log` / `log10` / `sqrt` / `sin` / `cos` / `tan` (+ `exp2`/`log2`/`cbrt`/`sinh`/`cosh`/`tanh`) (#145 BATCH 3 — DONE)

The FLOAT-returning 1-arg elementwise ufunc family — the unary-math ops
most-used in real numpy code (§2.5 training-data overlap). Wired EXACTLY
like the BATCH-2 reshape ops (`transpose`/`flatten`/`ravel`):
borrow-Buffer-arg → fresh-Buffer-return, the `(ptr) -> ptr`
`coil_shape_ty` extern shape, NOT the scalar-return stats. The cut line
is the ARITY + RETURN CONTRACT: only the 1-arg FLOAT-returning forms ship.
The `.cb`-side form is `coil.exp(a)` — a module free function (NOT a
sub-namespace). 7 core ops + 6 trivial same-dtype-rule optionals.

### Semantics (numpy 2.4.6 oracle — `coil::elementwise`)

- `coil.exp(a) -> Buffer` — `e**x`. `exp(710) -> +inf` (IEEE-754
  overflow VALUE).
- `coil.log(a) -> Buffer` — NATURAL log (base e). `log(0) -> -inf`,
  `log(-1) -> NaN`.
- `coil.log10(a) -> Buffer` — base-10 log. `log10(0) -> -inf`,
  `log10(-1) -> NaN`.
- `coil.sqrt(a) -> Buffer` — square root. `sqrt(-1) -> NaN`.
- `coil.sin(a)` / `coil.cos(a)` / `coil.tan(a) -> Buffer` — trig
  (radians).
- (Optional, identical dtype rule:) `coil.exp2` (`2**x`) / `coil.log2`
  (`log2(0) -> -inf`) / `coil.cbrt` (cube root, defined for negatives:
  `cbrt(-8) -> -2`) / `coil.sinh` / `coil.cosh` / `coil.tanh`.

All are TOTAL — a domain-error input is an IEEE-754 special VALUE, never
an error (numpy emits a RuntimeWarning; the array value is identical).
There is NO conformability concept for a unary op, so NO `coil_panic`
path exists; the shim ALWAYS returns a fresh `Buffer`.

**Dtype contract (the #1 nuance — numpy-confirmed)**: all FLOAT-returning.
Integer input (any int dtype) PROMOTES to `Float64`
(`exp(int_array) -> Float64`); `Float32` STAYS `Float32`
(`sqrt(f32) -> Float32`); `Float64` STAYS `Float64`. Implemented via
`promote::unary_math_dtype` (the SAME promotion `Array::sin`/`exp`/… use).
**Bool**: numpy promotes `bool -> float16` for these ufuncs; the coil
`Array` tagged-union has NO `float16` variant, so coil pins
`bool -> Float64`. The VALUES are identical (`True=1.0`/`False=0.0`, so
`exp(True)=e`, `sqrt(False)=0`) — only the dtype TIER differs (`Float64`
vs numpy's `Float16`). A value-faithful divergence consistent with the
existing `unary_math_dtype` contract.

### Manifest (`cobrust-types/src/ecosystem.rs`)

- 13 `lookup_module_fn` arms (7 core + 6 optional), each
  `[coil_buffer_ty()] -> coil_buffer_ty()`. Tier `Numerical` — floating
  arithmetic ufuncs whose VALUES agree with numpy at rtol 1e-12 (f64) /
  1e-6 (f32).

### Typecheck / MIR — ZERO new code

- The generic module-fn path (`try_synth_ecosystem_call` Case 1 /
  `try_lower_ecosystem_call` Case 1, `lower.rs:2162-2182`) already lowers
  any `lookup_module_fn` signature. The 1-Buffer-arg → Buffer shape is
  STRUCTURALLY IDENTICAL to `coil.transpose(a)` (BATCH 2): the single
  Buffer arg auto-borrows (Move→Copy in `lower_eco_arg`, so the input
  stays live + drops once) and the fresh return handle is drop-scheduled
  by `emit_ecosystem_call`. NO `_ => "any"` gap, NO new MIR arm.

### Codegen (`cobrust-codegen/src/llvm_backend.rs`)

- 13 extern rows, all reusing `coil_shape_ty` (`ptr -> ptr`) — the
  IDENTICAL extern shape as the BATCH-2 `transpose`/`flatten`/`ravel`.
  Symbols ride the existing `__cobrust_coil_` build/intrinsics prefix
  recognizer (`intrinsics.rs:1389` — a pure `starts_with` match, no CLI/
  linker edit needed).

### Runtime (`cobrust-coil/src/elementwise.rs` + `cabi.rs`)

- `elementwise.rs`: 13 kernels over the closed `Array` enum via a shared
  `unary_float(arr, op_f32, op_f64)` helper (consults `unary_math_dtype`,
  `mapv`s the matching monomorphic libm kernel onto a fresh owned
  `ArrayD<T>`). 19 unit tests, differential vs the numpy 2.4.6 oracle
  (incl. int->f64 + f32-stays-f32 + bool->f64 promotion + the
  `log(0)=-inf` / `log(-1)=NaN` / `sqrt(-1)=NaN` / `exp(710)=+inf` edges +
  a `sqrt(exp(a))` chain).
- `cabi.rs`: 13 shims `__cobrust_coil_{exp,log,log10,sqrt,sin,cos,tan,
  exp2,log2,cbrt,sinh,cosh,tanh}` sharing one `buffer_unary` body (borrow
  handle → apply infallible kernel → fresh Boxed return). Total — no
  `coil_panic` path (a null handle is the only abort, mirroring the
  BATCH-2 `__cobrust_coil_transpose` guard).

### Deferred

- Scalar-returning reductions of a ufunc result (e.g. `np.sum(np.exp(a))`)
  — already composable via the existing `coil.mean`/etc.; a fused form is
  a follow-up.
- The 2-arg `np.logaddexp` / `np.hypot` and the inverse-trig family
  (`arcsin`/`arccos`/`arctan`/`arctan2`) — DEFERRED (arctan2 is 2-arg).
- An int-DTYPE `.cb` constructor — the int->f64 promotion path is pinned
  in the `elementwise.rs` Rust unit tests; the `.cb` E2E proves the
  float-RETURNING contract those promotions serve (every `.cb` ctor emits
  `Float64`).

### Done means (#145 BATCH 3 — DONE)

- [x] `elementwise.rs`: 13 kernels (7 core + 6 optional) + shared
      `unary_float`/`as_f64`/`as_f32` helpers; 19 unit tests with the
      numpy-2.4.6 oracle (int->f64, f32->f32, bool->f64, NaN/inf edges,
      shape preservation, `sqrt(exp(a))` chain).
- [x] cabi: 13 shims via shared `buffer_unary` (TOTAL — no trap path).
- [x] Manifest: 13 ecosystem arms (`Buffer -> Buffer`, tier `Numerical`).
- [x] Typecheck / MIR: NO new code (generic module-fn path; 1-Buffer-arg
      proven by `transpose`).
- [x] Codegen: 13 extern rows (`coil_shape_ty` ×13).
- [x] `.cb` E2E `coil_ufunc_e2e.rs` (9 tests): basic `exp` `[1, e]`, `sqrt`
      `(2,2)`, `sqrt(exp(a))` CHAIN, `log10` powers-of-ten `[[0,1,2],
      [3,4,5]]`, `sqrt(mgrid)` integer-valued-float, `log` NaN/inf edges
      (`[-inf, NaN]`), `exp` overflow (`[inf, 1]`), `cos(0)=1` / `sin(0)=0`.
- [x] No regression: full `cobrust-coil` suite green (231 lib unit +
      every test binary); touched crates build + clippy `-D warnings` +
      fmt clean; no new dep (F64 — `ndarray` already present;
      `Cargo.lock` unchanged).
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## `.cb` `coil` unary rounding / sign — `abs` / `floor` / `ceil` / `round` / `trunc` / `square` / `sign` (#145 BATCH 4 — DONE)

The DTYPE-PRESERVING 1-arg elementwise ufunc family — the rounding /
absolute-value / sign ops an LLM reaches for after the transcendentals
(§2.5). Wired BYTE-IDENTICALLY to the BATCH-3 transcendentals
(`exp`/`log`/…): borrow-Buffer-arg → fresh-Buffer-return, the
`(ptr) -> ptr` `coil_shape_ty` extern shape, the shared `buffer_unary`
cabi body. The ONLY difference from BATCH 3 is the kernel's DTYPE contract
(PRESERVING, not float-promoting — see below). The `.cb`-side form is
`coil.abs(a)` — a module free function (NOT a sub-namespace, and the
`coil.abs(buf)` MODULE fn is distinct from the scalar `abs` method on
`Ty::Int`/`Ty::Float` in `lookup_handle_method`). 7 ops.

### Semantics (numpy 2.4.6 oracle — `coil::elementwise`)

- `coil.abs(a) -> Buffer` — absolute value. `abs(-1.5) -> 1.5`,
  `abs(NaN) -> NaN`; `i64::MIN` wraps to itself (`wrapping_abs`, numpy
  two's-complement).
- `coil.floor(a) -> Buffer` — largest int `<= x`. `floor(-1.5) -> -2`.
- `coil.ceil(a) -> Buffer` — smallest int `>= x`. `ceil(-1.5) -> -1`.
- `coil.round(a) -> Buffer` — round to nearest, **round-half-to-EVEN**
  (banker's). `round(0.5) -> 0`, `round(1.5) -> 2`, `round(2.5) -> 2`,
  `round(-0.5) -> -0`.
- `coil.trunc(a) -> Buffer` — truncate toward zero. `trunc(-1.7) -> -1`
  (UNLIKE `floor`).
- `coil.square(a) -> Buffer` — `x * x`. `square(-3) -> 9` (integer
  wrapping on overflow per numpy two's-complement).
- `coil.sign(a) -> Buffer` — `-1` / `0` / `1`. `sign(0.0) -> 0`,
  `sign(-0.0) -> 0`, `sign(NaN) -> NaN`.

All are TOTAL — there is NO conformability concept for a unary op, so NO
`coil_panic` path exists; the shim ALWAYS returns a fresh `Buffer` (a null
handle is the only abort, mirroring the BATCH-2/3 unary guard).

**Two numpy-exact correctness nuances (the #1 + #2 places this batch could
be WRONG — both pinned in tests):**

1. **`round` = round-half-to-EVEN (banker's)** — Rust
   `f64::round_ties_even` / `f32::round_ties_even`, NOT `f64::round`
   (half-AWAY-from-zero: `round(0.5)=1`, WRONG vs numpy). numpy `np.round`:
   `0.5->0`, `1.5->2`, `2.5->2`, `3.5->4`, `-0.5->-0`.
2. **`sign(0)=0` and `sign(NaN)=NaN`** — an explicit `is_nan` / `>0` / `<0`
   branch, NOT Rust `f64::signum` (which returns `+1.0` for `0.0` and
   propagates the sign bit for `NaN`, WRONG vs numpy).

**Dtype contract (the load-bearing difference from BATCH 3 — numpy-
confirmed)**: all DTYPE-PRESERVING. `int64 -> int64`, `Float32 -> Float32`,
`Float64 -> Float64` (`np.abs(np.int64([...])).dtype == int64`,
`np.round(np.float32([...])).dtype == float32`). NO int->float promotion.
**Integer no-op** (the #1 dtype subtlety): `floor`/`ceil`/`round`/`trunc`
are NO-OPS on integer input — numpy 2.x `np.floor(int_array)` returns the
int array UNCHANGED; coil returns the int / bool `Array` as-is (clone).
`abs`/`square`/`sign` DO apply to integers (`abs(-3)=3`, `square(2)=4`,
`sign(-5)=-1`). **Bool**: numpy DIVERGES per op (`round(bool)->float16`,
`square(bool)->int8`, `sign(bool)` RAISES, `abs`/`floor`/`ceil` stay
`bool`); coil's `Array` has no `float16`/`int8` and the unary surface is
TOTAL, so coil pins a single uniform rule — **every op returns the `Bool`
array UNCHANGED** (bool is the 0/1 fixed point of all seven ops:
`round(True)=1=True`, `square(True)=1=True`, `sign(True)=1=True`). The
VALUES match what each op means on the 0/1 numeric; only the dtype TIER
differs from numpy's per-op promotion, and `sign(bool)` does NOT raise. A
`Semantic`-tier divergence consistent with the BATCH-3 `bool -> Float64`
choice.

### Manifest (`cobrust-types/src/ecosystem.rs`)

- 7 `lookup_module_fn` arms (`abs`/`floor`/`ceil`/`round`/`trunc`/`square`/
  `sign`), each `[coil_buffer_ty()] -> coil_buffer_ty()`. Tier `Numerical`
  — VALUES agree with numpy 2.x exactly (`round` banker's, `sign(0)=0`,
  `sign(NaN)=NaN`); the DTYPE is PRESERVING (NOT the BATCH-3 int->Float64
  promotion) and `floor`/`ceil`/`round`/`trunc` are int no-ops.

### Typecheck / MIR — ZERO new code

- The generic module-fn path (`try_synth_ecosystem_call` Case 1 /
  `try_lower_ecosystem_call` Case 1) already lowers any `lookup_module_fn`
  signature. The 1-Buffer-arg → Buffer shape is STRUCTURALLY IDENTICAL to
  `coil.exp(a)` (BATCH 3) / `coil.transpose(a)` (BATCH 2): the single
  Buffer arg auto-borrows (Move→Copy in `lower_eco_arg`, so the input stays
  live + drops once) and the fresh return handle is drop-scheduled by
  `emit_ecosystem_call`. NO `_ => "any"` gap, NO new MIR arm.

### Codegen (`cobrust-codegen/src/llvm_backend.rs`)

- 7 extern rows, all reusing `coil_shape_ty` (`ptr -> ptr`) — the IDENTICAL
  extern shape as the BATCH-3 transcendentals + BATCH-2 reshape ops.
  Symbols ride the existing `__cobrust_coil_` build/intrinsics prefix
  recognizer (`build/intrinsics.rs:1389` — a pure `starts_with` match, no
  CLI/linker edit needed).

### Runtime (`cobrust-coil/src/elementwise.rs` + `cabi.rs`)

- `elementwise.rs`: 7 kernels over the closed `Array` enum via two shared
  helpers — `unary_round_family(arr, op_f32, op_f64)` (int / bool arm
  returns the input unchanged; float arms `mapv` the rounding kernel) for
  `floor`/`ceil`/`round`/`trunc`, and `unary_value(arr, op_i32, op_i64,
  op_f32, op_f64)` (every numeric arm transforms; bool arm unchanged) for
  `abs`/`square`/`sign`. The numpy-exact `sign` uses explicit `sign_f64` /
  `sign_f32` / `sign_i64` / `sign_i32` helpers (NOT `f64::signum`). 25 unit
  tests, differential vs the numpy 2.4.6 oracle (incl. int->int + f32->f32
  + f64->f64 preservation, the int no-op for floor/ceil/round/trunc,
  banker's rounding `0.5->0`/`1.5->2`/`2.5->2`/`-0.5->-0`, `sign(0)=0` +
  `sign(NaN)=NaN` + signs, abs/square negatives, the bool-unchanged rule,
  shape preservation, an `abs(floor(a))` chain + a `sign(square(a))`
  chain).
- `cabi.rs`: 7 shims `__cobrust_coil_{abs,floor,ceil,round,trunc,square,
  sign}` sharing the SAME `buffer_unary` body as BATCH 3 (borrow handle →
  apply infallible kernel → fresh Boxed return). Total — no `coil_panic`
  path.

### Deferred

- The 2-arg `np.copysign` / `np.fmod` and the `np.rint` / `np.fix` /
  `np.around(decimals=k)` variants (decimal-place rounding) — DEFERRED.
- An int-DTYPE `.cb` constructor — the int->int preservation + int no-op
  contracts are pinned in the `elementwise.rs` Rust unit tests; the `.cb`
  E2E proves the float-DTYPE value contract those rules serve (every `.cb`
  ctor emits `Float64`).

### Done means (#145 BATCH 4 — DONE)

- [x] `elementwise.rs`: 7 kernels via `unary_round_family` (int no-op) +
      `unary_value` (per-dtype) + `sign_{f64,f32,i64,i32}` helpers; 25 unit
      tests with the numpy-2.4.6 oracle (dtype preservation, int no-op,
      banker's rounding, `sign(0)=0`/`sign(NaN)=NaN`, negatives, bool-
      unchanged, shape, `abs(floor(a))` + `sign(square(a))` chains).
- [x] cabi: 7 shims via shared `buffer_unary` (TOTAL — no trap path).
- [x] Manifest: 7 ecosystem arms (`Buffer -> Buffer`, tier `Numerical`,
      DTYPE-PRESERVING).
- [x] Typecheck / MIR: NO new code (generic module-fn path; 1-Buffer-arg
      proven by `exp`/`transpose`).
- [x] Codegen: 7 extern rows (`coil_shape_ty` ×7).
- [x] `.cb` E2E `coil_round_e2e.rs` (8 tests): `round` banker's
      `[[0,2],[2,-0]]`, `sign` neg/zero/pos `[[-1,0],[1,-1]]`, `abs`
      negatives `[1.5,2.5]`, `floor` `[-2,1]` / `ceil` `[-1,2]` / `trunc`
      `[-1,1]` (toward-zero contrast), `square` `(2,2)` `[[4,9],[0,16]]`,
      `abs(floor(a))` CHAIN `[2,2]`.
- [x] No regression: full `cobrust-coil` suite green (256 lib unit +
      every test binary); `coil_round_e2e` + `coil_ufunc_e2e` +
      `coil_hello_e2e` all green; touched crates build + clippy
      `-D warnings` + fmt clean; no new dep (`ndarray` already present;
      `Cargo.lock` unchanged — F64).
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## #145 numpy gap-closure BATCH 5 — the REDUCTIONS family (DONE)

The reduction surface most-used in real numpy code per §2.5, spanning
THREE return shapes on a single `coil.Buffer` arg — the FIRST coil wave to
mix Buffer-return AND scalar-return (i64 / bool) ops:

- `coil.cumsum(a)` / `coil.cumprod(a)` → `coil.Buffer` (the no-axis
  FLATTEN-to-1-D cumulative scan).
- `coil.argmin(a)` / `coil.argmax(a)` → `i64` (the flat C-order index).
- `coil.any(a)` / `coil.all(a)` → `bool`.

SHIPPED IN BATCH 7 (f64-return form, see below): `min` / `max` / `prod`
(no-axis scalar) now return an `f64` — coil's established scalar-reduction
convention (the SAME shape as `mean`/`median`/`std`/`var`/`ptp`/
`percentile`). Every `.cb` Buffer constructor yields a Float64 buffer, so
`min`/`max`/`prod -> f64` is numpy-EXACT for every `.cb`-constructible
buffer. STILL DEFERRED: the numpy int-dtype-PRESERVING form (`np.max(int)
-> int`) — that needs a tagged scalar return (or a 0-d Buffer return) and
is its own pass; the f64-return ships the common functionality now,
value-faithfully.

### numpy semantics (numpy 2.x oracle)

- **cumsum/cumprod, no axis** — FLATTEN the n-d array to 1-D (C-order) then
  accumulate → a 1-D result of length `a.size` (`np.cumsum([[1,2],[3,4]])
  == array([1,3,6,10])`). DTYPE (the accumulator): `int32` AND `int64`
  BOTH widen to `int64` (numpy's platform-default int accumulator —
  `np.cumsum(np.int32([..])).dtype == int64`); `bool` → `int64`; `float32`
  stays `float32`; `float64` stays `float64`.
- **argmin/argmax, no axis** — the FLAT (C-order) index of the FIRST
  occurrence of the min/max. Ties → first occurrence. NaN PROPAGATES (numpy
  returns the NaN's index — `np.argmax([1,nan,2]) == 1`). EMPTY input
  RAISES `ValueError` → coil `coil_panic`s (a clean abort, NEVER a Rust
  unwind across the C-ABI). Return `i64`.
- **any** — `True` iff ANY element truthy; `any([]) == False`. **all** —
  `True` iff ALL truthy; `all([]) == True` (vacuous). Truthiness: nonzero
  for numeric (`0`/`0.0` falsy), `NaN` is TRUTHY (`np.any([nan]) == True`),
  `True`/`False` for bool. Return `bool`.

### Manifest (`cobrust-types/src/ecosystem.rs`)

- 6 `lookup_module_fn` arms, tier `Semantic`. The 3 return shapes differ
  ONLY in the `EcoSig` ret `Ty` (which drives the `_ecoret` local type +
  the codegen extern return type): `cumsum`/`cumprod` →
  `coil_buffer_ty()`; `argmin`/`argmax` → `Ty::Int`; `any`/`all` →
  `Ty::Bool`.

### Typecheck / MIR — ZERO new code (the load-bearing claim, VERIFIED)

- The generic module-fn path (`try_synth_ecosystem_call` Case 1 /
  `try_lower_ecosystem_call` Case 1) already lowers ANY `lookup_module_fn`
  signature regardless of its ret `Ty`. The Buffer-return half is
  structurally identical to `coil.exp` (BATCH 3). The SCALAR-return half
  (argmin → i64, any → bool) rides the EXACT path `coil.mean(a) -> f64`
  proves: `emit_ecosystem_call` declares the `_ecoret` local with `sig.ret`
  (Float / Int / Bool) and codegen reads the extern's declared return type.
  The `.cb` E2E (incl. the scalar prints + the empty-arg traps) confirms
  argmin→Int + any→Bool need NO new MIR arm — the EcoSig ret `Ty` is the
  only driver.

### Codegen (`cobrust-codegen/src/llvm_backend.rs`)

- `cumsum`/`cumprod`: 2 extern rows reusing `coil_shape_ty` (`ptr -> ptr`)
  — the IDENTICAL shape as the transcendental / rounding ufuncs.
- `argmin`/`argmax`: 2 NEW extern rows `coil_arg_i64_ty` (`(ptr) -> i64`)
  — mirrors `coil.mean`'s `(ptr) -> f64`, adapting the return to `i64`
  (the SAME shape as the `coil.Buffer.size`/`.ndim` `coil_attr_i64`
  accessors).
- `any`/`all`: 2 NEW extern rows `coil_pred_bool_ty`
  (`bool_type().fn_type(...)` → an `i1` return, the Rust C-ABI `-> bool`)
  — the FIRST coil `-> bool` value fn, mirroring `fang.verify_password`'s
  `bool_ty.fn_type(...)`. The `i1` lands in the `.cb` `_ecoret` Bool local.
- All 6 symbols ride the existing `__cobrust_coil_` build/intrinsics prefix
  recognizer (`build/intrinsics.rs:1389`).

### Runtime (`cobrust-coil/src/reduce.rs` + `aggregates.rs` + `cabi.rs`)

- `reduce.rs`: the kernels over the closed `Array` enum. `cumsum`/`cumprod`
  via a shared `cumulative(arr, is_sum)` (int / bool arms accumulate in
  `i64`, float arms in their own width; `.iter()` is logical C-order so the
  flatten is free). `argmin_flat`/`argmax_flat` (→ `Result<usize>`) reuse
  the tested `arg_extreme_iter_*` core (NaN / ties semantics) and return
  `Err(ReductionEmptyArray)` on empty. `any`/`all` (→ `bool`) — the plain
  `!= 0.0` test treats NaN as truthy (NaN compares unequal to 0.0) with no
  special branch. ~30 unit tests, differential vs the numpy 2.4.6 oracle
  (incl. 2-D flatten, int32→int64 + bool→int64 widening, f32/f64
  preservation, flat-index + ties-first + NaN-propagation + the empty Err
  path, any/all empty + NaN-truthy).
- `aggregates.rs`: 4 thin scalar wrappers (`argmin_scalar`/`argmax_scalar`
  → `Result<i64>` propagating the empty Err; `any_scalar`/`all_scalar` →
  `Result<bool>`), mirroring `mean_scalar` — the helpers the cabi shims
  call.
- `cabi.rs`: `cumsum`/`cumprod` via the SAME `buffer_unary` body (borrow
  handle → fresh Boxed return; TOTAL — no trap). `argmin`/`argmax` →
  `i64` shims mirroring `__cobrust_coil_mean`'s scalar shape; an EMPTY (or
  null) input maps the kernel's `Err` to `coil_panic` (clean abort, NEVER
  a Rust unwind across the FFI). `any`/`all` → `bool` shims (an empty /
  null input yields `False` / `True` — vacuous, matching numpy + the f64
  aggregates' graceful-null posture).

### Done means (#145 BATCH 5 — DONE)

- [x] `reduce.rs`: `cumsum`/`cumprod` (`cumulative`), `argmin_flat`/
      `argmax_flat` (reuse `arg_extreme_iter_*`), `any`/`all`; ~30 unit
      tests vs numpy 2.4.6 (2-D flatten, int32→int64 + bool→int64, f32/f64
      preservation, ties-first + NaN-propagation + empty Err, any/all
      empty + NaN-truthy).
- [x] `aggregates.rs`: 4 scalar wrappers (`{argmin,argmax}_scalar` → i64
      w/ Err-propagation, `{any,all}_scalar` → bool), mirroring
      `mean_scalar`.
- [x] cabi: `cumsum`/`cumprod` via `buffer_unary` (TOTAL); `argmin`/
      `argmax` → i64 w/ empty → `coil_panic` (clean trap, NO unwind);
      `any`/`all` → bool.
- [x] Manifest: 6 ecosystem arms — `cumsum`/`cumprod` `Buffer -> Buffer`,
      `argmin`/`argmax` `Buffer -> Int`, `any`/`all` `Buffer -> Bool`;
      tier `Semantic`.
- [x] Typecheck / MIR: NO new code (VERIFIED via E2E — the EcoSig ret `Ty`
      drives the scalar-return; argmin→Int + any→Bool ride `coil.mean`'s
      generic path).
- [x] Codegen: 2 `coil_shape_ty` rows (cumsum/cumprod) + 2 NEW
      `(ptr) -> i64` rows (argmin/argmax, mirror `mean`'s f64) + 2 NEW
      `(ptr) -> i1` rows (any/all, mirror `fang.verify_password` bool).
- [x] `.cb` E2E `coil_reduce_e2e.rs` (9 tests): cumsum 1-D `[2,5]`, cumsum
      2-D FLATTENS to `[1,3,6,10]` (+ asserts NO nested `[[`), cumprod
      `[1,2,6,24]`, argmin/argmax flat-index + ties-first (`1`/`3`),
      argmin/argmax monotonic mgrid (`0`/`4`), any/all mixed buffer
      (`1`/`0`), all-true (`1`/`1`), and the EMPTY-argmin + EMPTY-argmax
      CLEAN-TRAP tests (non-zero exit, unreachable marker absent).
- [x] No regression: full `cobrust-coil` suite green (282 lib unit + every
      test binary); `coil_reduce_e2e` + `coil_round_e2e` + `coil_ufunc_e2e`
      + `coil_hello_e2e` all green; touched crates build + clippy
      `-D warnings` + fmt clean; no new dep (`ndarray` already present;
      `Cargo.lock` unchanged — F64).
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## #145 numpy gap-closure BATCH 6 — the SCALAR-ARG ufuncs (DONE)

The FIRST Buffer-RETURNING coil ops to take EXTRA `f64` SCALAR args beside
the handle. They ride the SAME borrow-Buffer-arg → fresh-Buffer-return
value-handle ABI as the BATCH-3/4 unary ufuncs, plus trailing f64 scalar(s)
that cross by value (the exact precedent: `coil.percentile(a, q)`, a Buffer +
f64 — except these RETURN a fresh Buffer instead of an f64).

- `coil.clip(a, lo, hi)` → `coil.Buffer` — clamp each element to `[lo, hi]`,
  **DTYPE-PRESERVING** (`np.clip(int_array, lo, hi).dtype == int64`).
- `coil.power(a, p)` → `coil.Buffer` — `a ** p`, **FLOAT-PROMOTING** with an
  f64 exponent (`np.power(int_array, 2.0).dtype == float64`).

### Semantics (numpy 2.4.6 oracle)

- **clip** — `np.clip([1,5,9], 2, 7) = [2,5,7]` (int64, dtype PRESERVED). For
  an int / bool `Array` the f64 bounds ROUND to the integer dtype
  (`round_ties_even` + saturating cast) and the clamp stays integral; for a
  float `Array` the bounds clamp in that float type (`Float32` narrows the
  bounds to f32). **PRESERVES NaN** (`np.clip(nan, 0, 1) = nan` — an explicit
  `is_nan` guard, since Rust `f64::max`/`min` DROP a NaN operand). The
  **UPPER bound wins when `lo > hi`** (`np.clip([5], 7, 2) = [2]`) — numpy is
  `minimum(maximum(a, lo), hi)`, NOT Rust `f64::clamp` (which PANICS on
  `lo > hi`); the kernel uses `x.max(lo).min(hi)`. A `Bool` `Array` returns
  UNCHANGED (0/1 fixed point; coil's documented Semantic-tier divergence —
  numpy clips bool to an int dtype).
- **power** — FLOAT-PROMOTING with an f64 exponent: int / bool → `Float64`,
  `Float32` stays `Float32` (the exponent narrows to f32 — `np.power(f32,
  2.0).dtype == float32`), `Float64` stays `Float64`. The f64 exponent is
  used DELIBERATELY: a float exponent always promotes the output to float, so
  it SIDESTEPS numpy's int**int<0 `ValueError` (an `np.power(int, int<0)`
  raise) — a negative exponent is total here. `power(x, 0.5) = sqrt(x)`,
  `power(x, 0) = 1` (even `0**0 = 1`, the `f64::powf` identity),
  `power(neg, 0.5) = NaN` (the real branch — an IEEE-754 domain VALUE, never
  a trap). Mirrors the BATCH-3 transcendental `unary_float` dtype rule.

Both are TOTAL — there is NO conformability / domain concept for these ops
(NaN / inf are VALUES), so the cabi shims have NO `coil_panic` domain path; a
null handle is the only abort.

### Manifest (`cobrust-types/src/ecosystem.rs`)

- 2 arms, tier `Numerical`. `clip` is `(Buffer, Float, Float) -> Buffer` (the
  FIRST coil fn with TWO trailing f64 scalars); `power` is `(Buffer, Float) ->
  Buffer` (the SAME `coil.percentile` arg shape, Buffer-returning).
- The generic module-fn path (`try_synth_ecosystem_call` Case 1 /
  `try_lower_ecosystem_call` Case 1) already lowers ANY `lookup_module_fn`
  signature: the Buffer arg auto-borrows (Move→Copy) in `lower_eco_arg` and
  the trailing f64 scalar(s) lower as plain operands (the MIR retarget casts
  the `.cb` int / float literal to f64, exactly as `percentile`'s `q`). NO
  new MIR arm — the Case-1 loop iterates `sig.params` regardless of arity.

### Codegen (`cobrust-codegen/src/llvm_backend.rs`)

- `power` reuses the EXISTING `coil_scalar_binop_ty` `(ptr, f64) -> ptr` (the
  `a ⊕ k` scalar-binop shape). `clip` needs a NEW `coil_clip_ty`
  `(ptr, f64, f64) -> ptr` — the FIRST coil extern with two trailing f64
  scalars.
- Both symbols ride the existing `__cobrust_coil_` build/intrinsics prefix
  recognizer (`cobrust-cli/src/build/intrinsics.rs` `starts_with`) — no
  allowlist edit, link from `libcoil.a`.

### Runtime (`cobrust-coil/src/elementwise.rs` + `cabi.rs`)

- `elementwise.rs`: `clip(arr, lo, hi)` (dtype-split: int dtypes round the
  bounds + clamp integrally via `clip_int_*`; float dtypes clamp via
  `clip_f64`/`clip_f32` with the NaN guard + `max(lo).min(hi)` hi-wins order;
  bool unchanged), `power(arr, p)` (`unary_float` with `x.powf(p)` —
  int→f64 / f32→f32 / f64→f64). ~16 differential unit tests vs numpy 2.4.6
  (clip clamp + dtype-preserve int32/int64/f32/f64, lo>hi clamps-to-hi,
  NaN-preserve, bool-unchanged; power `**0.5`=sqrt, `**0`=1, `**2`=square,
  neg`**0.5`=NaN, int→f64 + f32→f32 + bool→f64 promotion, chain).
- `cabi.rs`: `__cobrust_coil_clip(a, lo, hi)` `(ptr, f64, f64) -> ptr` +
  `__cobrust_coil_power(a, p)` `(ptr, f64) -> ptr` — borrow handle, compute,
  fresh `Box::into_raw`. TOTAL (no domain trap); `coil_panic` only on null.
  3 cabi shim tests (clip clamp + drop-once, clip lo>hi, power square +
  drop-once, power 0.5=sqrt) via 1-D `array1d2` buffers (so `getitem` reads
  flat elements).

### Done means (#145 BATCH 6 — DONE)

- [x] `elementwise.rs`: `clip` (dtype-preserving) + `power` (float-promoting,
      f64 exponent); ~16 unit tests vs numpy 2.4.6.
- [x] cabi: 2 shims (`clip` `(ptr,f64,f64)->ptr`, `power` `(ptr,f64)->ptr`);
      TOTAL — no domain trap; `coil_panic` only on null. 3 shim tests.
- [x] Manifest: 2 ecosystem arms (`clip` `(Buffer,Float,Float)->Buffer`,
      `power` `(Buffer,Float)->Buffer`), tier `Numerical`; 2 sig tests.
- [x] Typecheck / MIR: NO new code (VERIFIED via E2E — `clip`/`power` ride the
      generic ecosystem-call path that `coil.percentile`'s `(Buffer, Float)`
      already proves; the f64 scalars lower as plain operands).
- [x] Codegen: 1 row reusing `coil_scalar_binop_ty` (power) + 1 NEW
      `coil_clip_ty` `(ptr,f64,f64)->ptr` row (clip).
- [x] `.cb` E2E `coil_scalararg_e2e.rs` (8 tests): clip clamp `[2,7]`, clip
      lo>hi `[2,2]`, power square `[4,9]`, power 0.5=sqrt `[2,3]`, power 0=1
      `[1,1]`, clip(power) chain `[2,9]`, + 2 negatives (clip / power reject a
      `str` bound / exponent).
- [x] No regression: full `cobrust-coil` suite green; `coil_scalararg_e2e` +
      `coil_reduce_e2e` + `coil_round_e2e` + `coil_ufunc_e2e` +
      `coil_hello_e2e` all green; touched crates build + clippy `-D warnings`
      + fmt clean; no new dep (`Cargo.lock` unchanged — F64).
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## #145 numpy gap-closure BATCH 7 — the VALUE reductions (DONE)

Completes the scalar-reduction family with the three VALUE reductions an
LLM reaches for first in numpy: `min` / `max` / `prod`. Each is a
`(Buffer) -> Float` op — the EXACT `(ptr) -> f64` extern shape `coil.mean`
proves, so the wiring is the smallest possible increment (NO new MIR arm,
NO new codegen extern type).

- `coil.min(a)` / `coil.max(a)` → `f64` (the smallest / largest element).
- `coil.prod(a)` → `f64` (the product of all elements).

### Why f64-return now (supersedes the BATCH-5 "min/max/prod deferred" note)

BATCH 5 deferred `min`/`max`/`prod` pending a dtype-PRESERVING scalar
return. BATCH 7 reframes: coil's scalar reductions ALL return `f64`
(`mean`/`median`/`std`/`var`/`ptp`/`percentile`) — f64 IS the established
scalar-reduction convention. Every `.cb` Buffer constructor today yields a
Float64 buffer (no int-dtype `.cb` constructor exists), so `min`/`max`/
`prod -> f64` is numpy-EXACT for every `.cb`-constructible buffer
(`np.max(f64_array) -> f64`). The f64-return ships the common
functionality NOW, value-faithfully + consistent with `mean`. The numpy
int-dtype-PRESERVING form (`np.max(int) -> int`) remains the documented
deferral (it needs a tagged / 0-d-Buffer scalar return — its own pass).

### numpy semantics (numpy 2.4.6 oracle)

- **min / max, no axis** — the smallest / largest element as `f64`. NaN
  PROPAGATES (any NaN in a lane → `NaN`; `np.max([1,nan,3])` is `nan`,
  `np.min([1,nan])` is `nan`) — the SAME posture as `coil.mean`. EMPTY
  input RAISES `ValueError` → coil `coil_panic`s (a clean abort, NEVER a
  Rust unwind across the C-ABI; mirror `argmin`/`argmax`).
- **prod, no axis** — the product of all elements as `f64`. NaN
  PROPAGATES. EMPTY → `1.0` (the multiplicative identity — `np.prod([])
  == 1.0`, NOT a trap). f64 overflow → `+inf` (numpy parity, a
  RuntimeWarning not an exception — `np.prod([1e308, 1e308])` is `inf`).

### Manifest (`cobrust-types/src/ecosystem.rs`)

- 3 `lookup_module_fn` arms, tier `Semantic`, all `(Buffer) -> Float` (the
  EcoSig ret `Ty::Float` is the only driver — the SAME as `mean`/`median`/
  `std`/`var`).

### Typecheck / MIR — ZERO new code

- `min`/`max`/`prod` ride the EXACT generic ecosystem-call path
  `coil.mean(a) -> f64` already proves: `emit_ecosystem_call` declares the
  `_ecoret` local with `sig.ret` (Float) and codegen reads the extern's
  declared return type. VERIFIED via the `.cb` E2E.

### Codegen (`cobrust-codegen/src/llvm_backend.rs`)

- 3 extern rows reusing `coil_agg_ty` (`(ptr) -> f64`) — the IDENTICAL
  shape as `mean`/`median`/`std`/`var`. NO new extern type.

### Runtime (`cobrust-coil/src/aggregates.rs` + `cabi.rs`)

- `aggregates.rs`: 3 thin scalar wrappers (`min_scalar`/`max_scalar` →
  `Result<f64>` propagating the empty `Err`; `prod_scalar` → `Result<f64>`
  infallible-today, `1.0` on empty). Each REUSES the existing
  `reduce::{min,max,prod}` no-axis kernels (the SAME arms the M7.3
  reduction surface exercises — NO reduction logic re-implemented): the
  kernel produces a 0-d `Array`, the shared `scalar_to_f64` extracts the
  value. 14 unit tests vs numpy 2.4.6 (values, int→f64 promotion,
  NaN-propagate, min/max empty `Err`, prod empty `1.0`, prod overflow
  `+inf`).
- `cabi.rs`: 3 shims mirroring `__cobrust_coil_mean`'s `(ptr) -> f64`
  shape. `min`/`max` map the kernel `Err` (empty / null) to `coil_panic`
  (clean abort, NEVER a Rust unwind). `prod` is TOTAL — a null handle
  yields the identity `1.0`. 4 shim tests (value + NaN-propagate +
  drop-once + prod-null `1.0`).

### Done means (#145 BATCH 7 — DONE)

- [x] `aggregates.rs`: 3 scalar wrappers (`{min,max}_scalar` → f64 w/
      empty-`Err`, `prod_scalar` → f64 w/ empty `1.0`), reusing
      `reduce::{min,max,prod}` kernels; 14 unit tests vs numpy 2.4.6.
- [x] cabi: 3 shims (`min`/`max` empty → `coil_panic`, NO unwind; `prod`
      TOTAL, null → `1.0`); 4 shim tests.
- [x] Manifest: 3 ecosystem arms (`min`/`max`/`prod` `(Buffer) -> Float`),
      tier `Semantic`.
- [x] Typecheck / MIR: NO new code (VERIFIED via E2E — the EcoSig ret
      `Ty::Float` rides `coil.mean`'s generic path).
- [x] Codegen: 3 rows reusing `coil_agg_ty` `(ptr) -> f64` (mirror
      `mean`); NO new extern type.
- [x] `.cb` E2E `coil_valuereduce_e2e.rs` (8 tests): min `2`, max `5`,
      prod `6`, NaN-propagate `max([nan,0])=NaN`, prod-empty `1`,
      prod-overflow `inf`, + 2 negatives (min / max of empty → clean trap,
      non-zero exit + unreachable marker absent).
- [x] No regression: full `cobrust-coil` suite green; `coil_valuereduce_e2e`
      + `coil_reduce_e2e` + `coil_scalararg_e2e` + `coil_hello_e2e` all
      green (min/max/prod do NOT collide with negative placeholders);
      touched crates build + clippy `-D warnings` + fmt clean; no new dep
      (`Cargo.lock` unchanged — F64).
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## #145 numpy gap-closure BATCH 8 — `coil.where(cond, a, b)` (DONE)

The 3-arg elementwise conditional select — `result[i] = cond[i] truthy ?
a[i] : b[i]`. The LAST simple bounded numpy op of #145 (the remaining gaps
— fancy indexing, broadcasting-where, the 1-arg `np.where(cond)` index
form — are heavier deferrals). This is the FIRST coil ecosystem fn
borrowing **three** Buffer handles: it EXTENDS the 2-Buffer combine ops
(`concatenate` / `vstack` / `hstack`) and `coil.linalg.solve` to a third
borrowed arg.

### Signature

- `.cb`: `coil.where(cond: coil.Buffer, a: coil.Buffer, b: coil.Buffer) ->
  coil.Buffer`
- runtime symbol: `__cobrust_coil_where(cond, a, b: *mut Buffer) -> *mut
  Buffer`

### numpy semantics (numpy 2.4.6 oracle)

- `np.where([True,False,True],[1,2,3],[4,5,6]) == [1,5,3]`.
- `cond` truthiness: a **Bool**-dtype `cond` uses its value directly (the
  clean case — the result of a `a < b` comparison per ADR-0077 is a
  Bool-dtype Buffer); a numeric `cond` is truthy on any **nonzero** element
  (numpy: `0`/`0.0` false, every other value incl. `NaN` true). Read by the
  `where_to_bool` helper — mirrors the M7.2 `index::np_where`
  `to_bool_array` cast.
- Result dtype = `a`'s dtype (`a` and `b` must match — the equal-dtype
  contract). The selected VALUES are copied verbatim, so a `NaN` in `a`/`b`
  **flows through** as a value (it is selected, never inspected).
- `all-True cond -> a`; `all-False cond -> b`.

### Shape + dtype contract (the §2.5-honest minimal surface)

- **Shape**: all three operands must share ONE shape (`cond.shape() ==
  a.shape() == b.shape()`). numpy BROADCASTS cond/a/b; this batch keeps the
  clean equal-shape contract and raises `ShapeMismatch` (numpy's
  `ValueError`) on a non-conformable triple. Broadcasting is a tracked
  follow-up (the existing M7.2 `index::np_where` already broadcasts; this
  `manipulate::where_select` is the equal-shape ecosystem-surface form that
  wires through the C-ABI).
- **Dtype**: `a` and `b` must share a dtype (the result dtype) — the SAME
  equal-dtype rule `concatenate` uses (no silent cross-dtype coercion,
  §2.2). A mismatch raises `ShapeMismatch`. Cross-dtype promotion is a
  tracked follow-up. `cond` may be ANY dtype (its truthiness is read; it
  does not participate in the result dtype) — typically Bool from `a < b`.
- This is the 3-arg `np.where(cond, a, b)` form ONLY; the 1-arg
  `np.where(cond)` index form (variable-length index arrays) is a separate
  deferral.

### Manifest (`cobrust-types/src/ecosystem.rs`)

- One arm: `("coil","where") => EcoSig::from_values("__cobrust_coil_where",
  vec![coil_buffer_ty(); 3], coil_buffer_ty(), Semantic)`. The 3-Buffer
  param vector is the only new shape.
- Tier `Semantic` — the selected VALUES + shape + dtype agree exactly with
  numpy (`where` copies a[i]/b[i] verbatim, no floating arithmetic). The
  intentional divergences (vs numpy's broadcasting + cross-dtype promotion)
  are the equal-shape + equal-dtype contracts.

### Typecheck / MIR — ZERO new code

- `where` rides the EXACT generic ecosystem-call path. `try_synth_
  ecosystem_call` resolves the `(Buffer, Buffer, Buffer) -> Buffer` EcoSig;
  the MIR `try_lower_ecosystem_call` Case-1 loop iterates `sig.params`
  regardless of arity — 3 Buffer args is the SAME path as concatenate's 2
  (each auto-borrows via `lower_eco_arg`'s Move→Copy upgrade, the fresh
  return is drop-scheduled by `emit_ecosystem_call`). NO `_=>"any"` MIR gap.
  VERIFIED via the E2E (cobrust-mir recompiled with no source change).
- `where` is NOT a Cobrust keyword (`match_keyword` has no `where` arm), so
  `coil.where(...)` parses cleanly as `Attr { base: coil, name: "where" }`
  (attribute names after `.` are plain idents via `expect_ident`). NO
  parser accommodation needed.

### Codegen (`cobrust-codegen/src/llvm_backend.rs`)

- One row: `__cobrust_coil_where` → the NEW `coil_select3_ty` = `(ptr, ptr,
  ptr) -> ptr` — the FIRST coil extern with three ptr args. Mirrors
  `concatenate`'s `coil_binop_ty` `(ptr, ptr) -> ptr` + one more ptr. Rides
  the `__cobrust_coil_` prefix recognizer.

### Runtime (`cobrust-coil/src/manipulate.rs` + `cabi.rs`)

- Kernel `manipulate::where_select(cond, a, b) -> Result<Array>` over the
  closed `Array` enum: shape guard (all three equal) + dtype guard (a==b),
  then `Zip`s the bool mask + a + b copying the selected element. 11 unit
  tests vs numpy 2.4.6.
- cabi `__cobrust_coil_where` borrows all THREE handles (none freed),
  `coil_panic`s on a null / non-conformable / dtype-mismatch triple (clean
  abort via `__cobrust_panic`, NEVER a Rust unwind across the FFI); 3 shim
  tests (3-Buffer round-trip w/ cond from `buffer_lt`, borrow-not-free,
  non-null return).

### Done means (#145 BATCH 8 — DONE)

- [x] `manipulate.rs`: `where_select` kernel + `where_to_bool` mask helper;
      11 unit tests (bool-cond `[1,5,3]`, all-true→a, all-false→b,
      numeric-cond nonzero, NaN-flows, 2-D, int-dtype, cond-from-`a.lt(b)`,
      + 3 non-conformable/dtype-mismatch `Err`).
- [x] cabi: 1 shim borrowing 3 handles (null/non-conformable → `coil_panic`,
      NO unwind); 3 shim tests.
- [x] Manifest: 1 ecosystem arm (`where` `(Buffer, Buffer, Buffer) ->
      Buffer`), tier `Semantic`.
- [x] Typecheck / MIR: NO new code (VERIFIED via E2E — the 3-Buffer arg
      vector rides concatenate's generic Case-1 path). `where` needs NO
      parser accommodation (not a keyword).
- [x] Codegen: 1 row with the NEW `coil_select3_ty` `(ptr,ptr,ptr) -> ptr`
      (the first 3-ptr coil extern).
- [x] `.cb` E2E `coil_where_e2e.rs` (6 tests): comparison-driven `where(a<b,
      [10,20], [30,40])=[10,40]` (bool-mask integration end-to-end),
      all-true→a, all-false→b, NaN-flows-through `[NaN,NaN]`,
      transpose∘where chain, + non-conformable clean-trap (non-zero exit).
- [x] No regression: full `cobrust-coil` suite green; `coil_where_e2e` +
      `coil_hello_e2e` all green (`where` does NOT collide with negative
      placeholders); touched crates build + clippy `-D warnings` + fmt
      clean; no new dep (`Cargo.lock` unchanged — F64).
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## #145 numpy gap-closure BATCH 9 — the FLAT search / order ops (DONE)

The FLAT search / order surface — `sort` / `argsort` / `unique` /
`flatnonzero`. Four top-tier-common numpy ops, each a 1-arg `Buffer ->
Buffer` op that FLATTENS to C-order first (numpy's no-axis default). They
ride the EXACT BATCH-2/3/4 1-arg path (borrow-Buffer-arg →
fresh-Buffer-return), reusing the shared `cabi::buffer_unary` body + the
`coil_shape_ty` `(ptr) -> ptr` extern. The interesting wrinkle is the
RETURN-DTYPE split (below).

### Signatures

- `.cb`: `coil.sort(a: coil.Buffer) -> coil.Buffer`,
  `coil.argsort(a) -> coil.Buffer`, `coil.unique(a) -> coil.Buffer`,
  `coil.flatnonzero(a) -> coil.Buffer`.
- runtime symbols: `__cobrust_coil_{sort,argsort,unique,flatnonzero}(a:
  *mut Buffer) -> *mut Buffer`.

### Return-dtype split (the load-bearing contract)

- `sort` / `unique` **PRESERVE** the input dtype (numpy: same dtype as
  input).
- `argsort` / `flatnonzero` ALWAYS produce an **Int64** Buffer (the
  indices), REGARDLESS of input dtype (numpy `intp` = int64 on the 64-bit
  AOT targets). Since every `.cb`-buildable Buffer is `Float64`, the
  printed repr dtype literally FLIPS to `int64` for these two — the
  observable E2E signal.
- The split lives ENTIRELY in the Rust kernel (`manipulate.rs`); the handle
  ABI is byte-identical for all four (an opaque `*mut Buffer`), so codegen
  rides the SAME `coil_shape_ty` extern for every one.

### numpy semantics (numpy 2.4.6 oracle via `/opt/homebrew/bin/python3.11`)

- `sort`: ASCENDING; the no-axis default flattens C-order first
  (`np.sort([[3,1],[4,2]], axis=None) == [1,2,3,4]`). For floats ALL `NaN`
  sort to the END regardless of sign-bit — `np.sort([nan,1,-nan,2]) ==
  [1,2,nan,nan]`. `f64::total_cmp` ALONE is WRONG (it orders `NaN` by
  sign-bit, placing `-NaN` FIRST), so the float arm PARTITIONS non-NaN
  (sorted via `total_cmp` on the finite/inf subset) ++ the `NaN`s (input
  order). Int / bool arms are a plain ascending sort.
- `argsort`: the int64 indices that would sort `a`; **STABLE** (the
  deterministic, reproducible tie-break — numpy's default quicksort happens
  to agree on the test cases, but stable PINS the equal-key + NaN-index
  order). `np.argsort([3.,1.,2.]) == [1,2,0]`. `NaN`-bearing indices last,
  in input order — `np.argsort([5,nan,3,nan,1]) == [4,2,0,1,3]`.
- `unique`: SORTED ascending unique; `np.unique([3,1,2,1,3]) == [1,2,3]`.
  numpy 1.21+ collapses MULTIPLE `NaN` to ONE trailing `NaN` —
  `np.unique([nan,nan,1.,nan,2.]) == [1.,2.,nan]`. The non-NaN values are
  sorted + deduped; a single trailing `NaN` is appended iff any was present.
- `flatnonzero`: the int64 flat C-order indices where `a != 0`;
  `np.flatnonzero([0,5,0,2]) == [1,3]`. For floats the predicate is `a !=
  0.0`, so `NaN` (being `!= 0.0`) IS included — `np.flatnonzero([0.,nan,0.])
  == [1]`. A 2-D input flattens C-order first.

### Manifest (`cobrust-types/src/ecosystem.rs`)

- Four arms: `("coil","{sort,argsort,unique,flatnonzero}") =>
  EcoSig::from_values("__cobrust_coil_<op>", vec![coil_buffer_ty()],
  coil_buffer_ty(), Semantic)`. The return type is `coil.Buffer` for all
  four; the element-dtype split (sort/unique preserve, argsort/flatnonzero
  → int64) is invisible to typecheck (it sees only the opaque handle).
- Tier `Semantic` — VALUES + order + dtype agree exactly with numpy. The
  only intentional divergence is the absent optional `axis` arg (we always
  flatten no-axis), documented in `manipulate.rs`.

### Typecheck / MIR — ZERO new code

- All four ride the EXACT generic 1-Buffer-arg → Buffer-return path that
  `coil.transpose` / `coil.exp` already prove. `try_lower_ecosystem_call`
  Case-1 iterates `sig.params`; the single Buffer arg auto-borrows
  (Move→Copy) via `lower_eco_arg` and the fresh return is drop-scheduled by
  `emit_ecosystem_call`. NO `_=>"any"` MIR gap; VERIFIED via the E2E
  (cobrust-mir recompiled with no source change).
- None of `sort` / `argsort` / `unique` / `flatnonzero` is a Cobrust
  keyword, so they parse cleanly as `Attr { base: coil, name: <op> }`. NO
  parser accommodation needed.

### Codegen (`cobrust-codegen/src/llvm_backend.rs`)

- Four rows: `__cobrust_coil_{sort,argsort,unique,flatnonzero}` →
  `coil_shape_ty` `(ptr) -> ptr` — the IDENTICAL extern shape as the
  reshape ops + unary ufuncs. Rides the `__cobrust_coil_` prefix recognizer.

### Runtime (`cobrust-coil/src/manipulate.rs` + `cabi.rs`)

- Kernels `manipulate::{sort,argsort,unique,flatnonzero}(a: &Array) ->
  Array` over the closed `Array` enum. `sort`/`unique` match each dtype arm
  preserving the variant; `argsort`/`flatnonzero` build an `Array::Int64`
  via `int64_1d`. A tiny private `Float` trait (`is_nan` + `total_cmp` +
  `PartialEq`) abstracts the f32/f64 NaN-partition arms WITHOUT a new dep.
  24 unit tests vs numpy 2.4.6.
- cabi: 4 shims via the SAME `buffer_unary` body (TOTAL — a sort / dedupe /
  nonzero scan never fails on a valid Buffer; a null handle is the only
  abort, via `buffer_unary`'s guard). NO `coil_panic` domain trap.

### Done means (#145 BATCH 9 — DONE)

- [x] `manipulate.rs`: `sort`/`argsort`/`unique`/`flatnonzero` kernels +
      `sorted_total`/`sorted_float_nan_last`/`argsort_total`/
      `argsort_float_nan_last`/`unique_total`/`unique_float_nan_collapse`/
      `nonzero_idx`/`int64_1d` helpers + the private `Float` trait; 24 unit
      tests (sort asc + NaN-last (incl. signed-NaN) + dtype-preserve
      int/i32/f32, empty; argsort i64-indices + stable-dup + NaN-tail +
      int-input-still-i64; unique sorted-dedupe + NaN-collapse (1 + multi) +
      dtype-preserve + 2-D flatten; flatnonzero i64-indices + NaN-nonzero +
      2-D flatten + float-input-still-i64 + all-zero-empty; sort∘unique
      chain).
- [x] cabi: 4 shims via `buffer_unary` (TOTAL — null is the only abort).
- [x] Manifest: 4 ecosystem arms (each `Buffer -> Buffer`), tier `Semantic`.
- [x] Typecheck / MIR: NO new code (VERIFIED via E2E — the 1-Buffer arg
      rides the `coil.transpose` generic Case-1 path). No parser
      accommodation (none is a keyword).
- [x] Codegen: 4 `coil_shape_ty` `(ptr) -> ptr` rows.
- [x] `.cb` E2E `coil_sort_e2e.rs` (6 tests): sort a 2x2 flattened to
      `[1,2,3,4]` (float64), argsort → `[1,3,0,2]` `dtype=int64` (the
      dtype-flip signal), unique → `[1,2,3]` (float64), flatnonzero →
      `[1,3]` `dtype=int64`, NaN-last sort `[1, NaN]` (NaN built via IEEE
      0.0/0.0), + the sort∘unique chain.
- [x] No regression: full `cobrust-coil` suite green; `coil_sort_e2e` +
      `coil_hello_e2e` all green (the four ops do NOT collide with the
      `coil.no_such_function` negative placeholder); touched crates build +
      clippy `-D warnings` + fmt clean; no new dep (`Cargo.lock` unchanged —
      F64).
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

## #145 numpy gap-closure BATCH 10 — the REARRANGE / REPEAT family (DONE)

The REARRANGE / REPEAT surface — `diff` / `flip` / `roll` / `repeat` /
`tile`. Five top-tier-common numpy ops, each Buffer-RETURNING over the
C-order FLATTENED array. They split on **arity + output-shape**, all
riding proven paths (NO new MIR arm):

- `diff` / `flip` are **1-arg** `Buffer -> Buffer` — the EXACT BATCH-2/9
  1-arg path (borrow-arg → fresh-Buffer-return), reusing the shared
  `cabi::buffer_unary` body + the `coil_shape_ty` `(ptr) -> ptr` extern.
- `roll` / `repeat` / `tile` take a **trailing i64 SCALAR** —
  `(Buffer, Int) -> Buffer`. The i64-scalar mirror of the BATCH-6
  `clip(a, lo, hi)` / `power(a, p)` f64-scalar shape, but `Ty::Int` not
  `Ty::Float`. The FIRST coil module fns with a trailing `Ty::Int` scalar.

### Signatures

- `.cb`: `coil.diff(a: coil.Buffer) -> coil.Buffer`,
  `coil.flip(a) -> coil.Buffer`, `coil.roll(a, k: i64) -> coil.Buffer`,
  `coil.repeat(a, n: i64) -> coil.Buffer`, `coil.tile(a, n: i64) ->
  coil.Buffer`.
- runtime symbols: `__cobrust_coil_{diff,flip}(a: *mut Buffer) -> *mut
  Buffer`; `__cobrust_coil_{roll,repeat,tile}(a: *mut Buffer, k: i64) ->
  *mut Buffer`.

### Output-shape split (the load-bearing contract)

- ALL FIVE are **DTYPE-PRESERVING** (numpy 2.x: `diff(int) -> int`, etc.).
  The element-dtype is invisible to typecheck (the opaque handle).
- `diff` / `flip` / `repeat` / `tile` always FLATTEN to a **1-D** result;
  only `roll` PRESERVES the original (possibly multi-D) shape — it is the
  no-axis `np.roll`, which shifts on the flattened view but keeps the shape.
- Output lengths: `diff` → `max(size - 1, 0)`; `flip` → `size` (reversed);
  `roll` → `size` (SAME shape); `repeat` / `tile` → `n * size`.

### numpy semantics (numpy 2.4.6 oracle via `/opt/homebrew/bin/python3.11`)

- `diff`: `a[1:] - a[:-1]` over the flattened array —
  `np.diff([1,4,9,16]) == [3,5,7]`. A len-≤1 / empty input → EMPTY
  (`np.diff([5]) == []`). A 2-D input flattens C-order first
  (`np.diff([[1,2,3],[4,5,6]].flatten()) == [1,1,1,1,1]`). (bool input is
  the adjacent-XOR, dtype bool — value-faithful, consistent with the
  preserve rule.)
- `flip`: reverse the flattened array — `np.flip([1,2,3]) == [3,2,1]`;
  `np.flip([[1,2],[3,4]])` flat-reversed `== [4,3,2,1]`. Empty → empty.
- `roll`: cyclic shift by `k`, reshaped BACK to the ORIGINAL shape —
  `np.roll([1,2,3,4],1) == [4,1,2,3]`; `np.roll([[1,2],[3,4]],1) ==
  [[4,1],[2,3]]` (SAME (2,2) shape). Element at flat index `i` moves to
  `(i + k) mod size`; numpy's Python-floor modulo makes a NEGATIVE `k` roll
  LEFT (`np.roll([1,2,3],-1) == [2,3,1]`) and normalises `k` mod size
  (`k = 0` / `k % size == 0` → unchanged; `np.roll([1,2,3],4) == [3,1,2]`).
  An empty input → empty (no shift). The kernel uses `k.rem_euclid(n)` for
  the Python-floor normalisation.
- `repeat`: repeat EACH element `n` times — `np.repeat([1,2],2) ==
  [1,1,2,2]`; length `n * size`. `n <= 0` → empty (`np.repeat(a, 0) ==
  []`); `n == 1` → a flat copy.
- `tile`: tile the WHOLE flattened array `n` times — `np.tile([1,2],2) ==
  [1,2,1,2]`; length `n * size`. `n <= 0` → empty (`np.tile(a, 0) == []`);
  `n == 1` → a flat copy. (This is the scalar-reps `np.tile(a, n)` form,
  NOT the tuple-reps `np.tile(a, (r, c))` — a tracked follow-up once
  tuple-arg marshalling lands.)

### Manifest (`cobrust-types/src/ecosystem.rs`)

- Five arms. `diff` / `flip`: `EcoSig::from_values("__cobrust_coil_<op>",
  vec![coil_buffer_ty()], coil_buffer_ty(), Semantic)`. `roll` / `repeat` /
  `tile`: `vec![coil_buffer_ty(), Ty::Int]` — the trailing scalar is
  `Ty::Int` (the `count` / `shift`), NOT `Ty::Float` (the load-bearing
  dtype: the `.cb` int literal lowers DIRECTLY as i64, no f64 cast UNLIKE
  `percentile` / `power`).
- Tier `Semantic` — VALUES + shape + dtype agree exactly with numpy 2.x
  (integer-exact rearrange / repeat — `diff` is an exact subtract, no
  floating arithmetic). The only divergence is the absent tuple-reps `tile`
  form (scalar-reps only), documented in `manipulate.rs`.

### Typecheck / MIR — ZERO new code

- All five ride the GENERIC `try_lower_ecosystem_call` Case-1 loop. The
  1-arg ops follow `coil.transpose` (the BATCH-2 path); the i64-scalar ops
  follow `coil.clip(a, lo, hi)` / `coil.power(a, p)` (the BATCH-6 scalar-arg
  path) EXACTLY — the only delta is the scalar param's `Ty` (`Ty::Int` vs
  `Ty::Float`). `lower_eco_arg` lowers the `.cb` int literal as a plain i64
  operand (the `Value` arm; no f64 cast), and the codegen extern-call
  int-width coercion (`build_int_z_extend` at the `Constant::Str` dispatch;
  an i64-into-i64 param is a no-op) forwards it into the `(ptr, i64) -> ptr`
  extern. The single Buffer arg auto-borrows (Move→Copy); the fresh return
  is drop-scheduled by `emit_ecosystem_call`. NO `_=>"any"` MIR gap;
  VERIFIED via E2E (cobrust-mir recompiled with no source change).
- None of `diff` / `flip` / `roll` / `repeat` / `tile` is a Cobrust
  keyword, so they parse cleanly as `Attr { base: coil, name: <op> }`. NO
  parser accommodation needed.

### Codegen (`cobrust-codegen/src/llvm_backend.rs`)

- `diff` / `flip`: 2 rows on `coil_shape_ty` `(ptr) -> ptr` (the reshape /
  ufunc shape). `roll` / `repeat` / `tile`: 3 rows on a NEW
  `coil_scalar_i64_ty` `(ptr, i64) -> ptr` — the i64-scalar mirror of the
  BATCH-6 `coil_scalar_binop_ty` `(ptr, f64) -> ptr`. All ride the
  `__cobrust_coil_` prefix recognizer.

### Runtime (`cobrust-coil/src/manipulate.rs` + `cabi.rs`)

- Kernels `manipulate::{diff,flip,roll,repeat,tile}(a: &Array[, i64]) ->
  Array` over the closed `Array` enum, all dtype-preserving. Helpers:
  `diff_flat` (windows(2) adjacent-combine), `flip_flat` (collect + reverse
  — the `ArrayD` dyn iterator is NOT `DoubleEndedIterator`), `roll_keep_
  shape` (`k.rem_euclid(n)` cyclic shift + reshape-back), `repeat_each`,
  `tile_whole`, and the shared `vec_1d` 1-D builder. 19 unit tests vs numpy
  2.4.6.
- cabi: `diff` / `flip` via the SAME `buffer_unary` body; `roll` / `repeat`
  / `tile` via a new `buffer_unary_scalar_i64` shared body (the i64-scalar
  mirror of the `clip` / `power` f64-scalar borrow pattern — borrow handle,
  compute, fresh `Box::into_raw`). ALL TOTAL (a rearrange / repeat never
  fails; an empty input or `n <= 0` yields an empty Buffer) — NO
  `coil_panic` domain trap; a null handle is the only abort. 6 shim tests.

### Done means (#145 BATCH 10 — DONE)

- [x] `manipulate.rs`: `diff`/`flip`/`roll`/`repeat`/`tile` kernels +
      `diff_flat`/`flip_flat`/`roll_keep_shape`/`repeat_each`/`tile_whole`/
      `vec_1d` helpers; 19 unit tests (diff len-n-1 + values + int-preserve
      + 2-D flatten + len-1/empty; flip reversed + 2-D flat-reversed +
      empty; roll same-shape + cyclic + negative-k + k-mod-size + 2-D
      keeps-shape + empty; repeat n*size interleaved + int-preserve + 2-D
      flatten + n=0/n=1; tile n*size whole-repeat + int-preserve + 2-D
      flatten + n=0/n=1; flip∘diff chain).
- [x] cabi: `diff`/`flip` via `buffer_unary`; `roll`/`repeat`/`tile` via
      `buffer_unary_scalar_i64` (TOTAL — null is the only abort). 6 shim
      tests (drop-once on flip/roll/repeat; diff value; negative-k roll;
      whole-array tile).
- [x] Manifest: 5 ecosystem arms (diff/flip `Buffer -> Buffer`;
      roll/repeat/tile `(Buffer, Int) -> Buffer`), tier `Semantic`. 2 sig
      tests.
- [x] Typecheck / MIR: NO new code (VERIFIED via E2E — the 1-arg ops ride
      the `coil.transpose` path, the i64-scalar ops ride the `coil.clip`
      scalar-arg path; only the scalar `Ty` differs). No parser
      accommodation (none is a keyword).
- [x] Codegen: 2 `coil_shape_ty` `(ptr) -> ptr` rows (diff/flip) + 3
      `coil_scalar_i64_ty` `(ptr, i64) -> ptr` rows (roll/repeat/tile — the
      NEW i64-scalar extern shape).
- [x] `.cb` E2E `coil_rearrange_e2e.rs` (9 tests): diff `[1,4]→[3]`, flip
      `[1,2]→[2,1]`, roll `[1,2]→[2,1]` (cyclic) + negative-k left-roll,
      repeat `[1,2]→[1,1,2,2]`, tile `[1,2]→[1,2,1,2]`, the flip∘diff chain
      over a 2x2-flattened `[1,4,9,16]→[7,5,3]` (proves the i64-scalar +
      2-D-flatten + Buffer-feeds-Buffer paths), + 2 negative type-error
      proofs (`roll(a, "x")` rejects a str scalar, `tile(a, 2.5)` rejects a
      float scalar — pins the `Ty::Int` (not `Ty::Float`) param choice).
- [x] No regression: full `cobrust-coil` suite green (381 lib tests);
      `coil_rearrange_e2e` + `coil_hello_e2e` all green, run ONE `--test` at
      a time (F73 libcoil.a build-race avoidance — no negative-placeholder
      collision); touched crates (`cobrust-coil` + `cobrust-codegen` +
      `cobrust-types`) build + clippy `-D warnings` + fmt clean; no new dep
      (`Cargo.lock` unchanged — F64).
- [x] Doc tree (zh/en/agent) updated in the same commit (CLAUDE.md §3.3).

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
