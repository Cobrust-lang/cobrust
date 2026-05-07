---
doc_kind: module
module_id: mod:numpy
crate: cobrust-numpy
last_verified_commit: 18c5c1d93dbf5b5a4f9d8015aa64b96805bdcc38
dependencies: [mod:translator]
---

# Module: numpy

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

- **M7.1 — delivered.** Universal functions + broadcasting + NEP 50
  type promotion landed per ADR-0014. The cobrust-numpy crate now
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
  per ADR-0015. cobrust-numpy now ships closed `Index` enum (5
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
  / var / min / max / argmin / argmax`) per ADR-0016. cobrust-numpy
  now ships nine reductions exposed as both free functions
  (`cobrust_numpy::sum / prod / mean / std / var / min / max /
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
  / inv / svd / eigh / cholesky`) per ADR-0017. cobrust-numpy now
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

`crates/cobrust-numpy/tests/reduce_differential.rs` runs against
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

`crates/cobrust-numpy/tests/reduce_pipeline.rs` drives
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

`crates/cobrust-numpy/tests/index_differential.rs` runs against
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

`crates/cobrust-numpy/tests/index_pipeline.rs` drives
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

`crates/cobrust-numpy/tests/linalg_differential.rs` runs against
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

`crates/cobrust-numpy/tests/linalg_pipeline.rs` drives
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
