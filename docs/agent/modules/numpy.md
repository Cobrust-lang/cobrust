---
doc_kind: module
module_id: mod:numpy
crate: cobrust-numpy
last_verified_commit: bcff3c3
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
