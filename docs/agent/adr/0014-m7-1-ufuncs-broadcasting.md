---
doc_kind: adr
adr_id: 0014
title: M7.1 universal functions, broadcasting, type promotion — dispatch model + numpy-compat semantics
status: accepted
date: 2026-04-30
last_verified_commit: 1f34acd
supersedes: []
superseded_by: []
---

# ADR-0014: M7.1 universal functions, broadcasting, type promotion — dispatch model + numpy-compat semantics

## Context

ADR-0012 fixed the M7 sub-milestone breakdown; ADR-0013 landed M7.0
(ndarray foundation, closed dtype tier, tagged-union `Array`,
constructors, observer surface, differential gate). ADR-0013
explicitly flagged four follow-ups for M7.1:

1. **Tagged-union dispatch overhead** — M7.0's `match` on
   `Array::Int32(...) | ...` is fine for O(1)-per-call constructors
   but compounds for ufuncs (where the inner element-wise loop
   touches every element). Revisit at M7.1 and pick monomorphic
   dispatch via macros for the hot path.
2. **Typed constructors** — M7.0's `array(values: &[f64], shape, dtype)`
   forces every caller to f64-cast integer inputs. M7.1 adds typed
   constructors (`array_i32`, `array_i64`, …) once the ufunc surface
   is in.
3. **L2.perf flip** — M7.0 perf was informational; M7.1 ufuncs is
   where SIMD competition is real (per ADR-0010 §3 numerical-tier
   0.5× floor). Flip to enforced.
4. **Multi-D nested-list parsing** — M7.0 only accepts a flat values
   buffer + shape. M7.1 adds `array(NestedList) → Array` for 2D/3D
   inputs that match `numpy.array([[1,2],[3,4]])`.

Plus M7.1's mandate from ADR-0012 §"Sub-milestones":
- Universal functions: `+ - * / **`, `np.add/subtract/...`.
- Broadcasting rules.
- Element-wise math: `sin / cos / exp / log / sqrt`.
- Backend: `ndarray` element-wise + own broadcasting impl.
- Acceptance gate: bit-identical for int dtypes; `rtol=1e-7` for
  float; 1000-input differential corpus.

This ADR pins the M7.1-binding decisions across five axes:
**ufunc dispatch model**, **broadcasting algorithm**,
**type-promotion rules**, **error semantics on overflow / NaN /
divide-by-zero**, and **L2.perf gate flip**.

## Options considered

### 1. Ufunc dispatch model — closes M7.0 follow-up #1

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| **Monomorphic via `for_each_dtype!` macro** — hand-write the dispatch table once; expand per-arm at the API boundary; the inner element-wise loop runs on a concrete `ndarray::ArrayD<T>`, so LLVM can inline + auto-vectorise. | Zero runtime dispatch cost on the hot path; SIMD-friendly; matches `ndarray` idiomatic patterns. | Macro is one indirection layer; per-op variant explosion (5 dtypes × 3 shape combinations = 15 arms × N ufuncs). | **Yes** |
| Tagged-union dispatch at every call site (M7.0 status quo) | One `match` per call site, no macro indirection. | M7.0's M7.1 follow-up flagged this as the perf hole; an inner loop `match`-per-element kills auto-vectorisation. | No |
| Trait-object `Box<dyn Ufunc>` | Plug-in extensibility. | Constitution §2.2 forbids `dyn` as default; would also break the inner-loop optimiser. | No |
| Generic `Array<T>` parametrised on T | Zero-cost. | Doesn't fit numpy's "an array is just an array" UX; users would need `Array::<i32>::sin(&a)`. | No |

**Pick**: monomorphic via `for_each_dtype!` macro. The macro lives
in `crates/cobrust-numpy/src/ufunc.rs`. The public-API `Array::add`
etc. matches once on `(self.dtype(), other.dtype())`, picks the
promoted dtype via `result_type` (option 3 below), and dispatches
into a per-dtype monomorphic helper. The inner helper calls
`ndarray::Zip::from(...).and(...).map_collect(...)` on a concrete
`ArrayD<T>`; LLVM inlines and the Zip iterator vectorises naturally.

This satisfies:
- Constitution §2.2 (no `dyn` in cobrust-numpy public API — the
  match arms are all on closed enum variants).
- Constitution §5.3 (efficient — auto-vectorised inner loops).
- ADR-0013 follow-up #1 (tagged-union → monomorphic).

### 2. Broadcasting algorithm — own impl on top of `ndarray`

ADR-0012 §"Sub-milestones" M7.1 row says "`ndarray` element-wise +
own broadcasting impl". `ndarray::ArrayBase::broadcast` exists but
returns `Option<ArrayView>` and uses ndarray's own rules; we own
the **numpy-compat** rules, which differ in two important corners:

1. **Right-aligned dimension matching**. Both align right, both
   expand size-1, but error messages and edge-case behavior
   (empty-shape vs scalar-shape) need to match numpy 2.x.
2. **Result shape is the broadcast of both inputs**, returned
   alongside the broadcast views. We need both the shape and the
   views.

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| Use `ndarray::Zip::from(a).and_broadcast(b)` directly | Zero new code. | `and_broadcast` only handles 1-direction broadcasting (b broadcast to a's shape); numpy broadcasts both. | No |
| **Own `broadcast_shape(&a_shape, &b_shape) -> Result<Vec<usize>>`, then `ndarray::ArrayBase::broadcast(target_shape)` per side, then `Zip::from(...).and(...)`** | Numpy-exact rules; clear error path; reuses ndarray's broadcast view machinery for the actual stride math. | One more layer. | **Yes** |
| Flatten then index manually | Defeats the entire ndarray backend. | — | No |

**Pick**: own `broadcast_shape` (numpy-exact rules) + delegate to
`ArrayBase::broadcast` for each operand to get the broadcast view.
Lives in `crates/cobrust-numpy/src/broadcast.rs`.

**Numpy-exact rules** (cite https://numpy.org/doc/stable/user/basics.broadcasting.html):

```
Given shapes A = (a_n, ..., a_1) and B = (b_m, ..., b_1):
  1. Right-align: pad the shorter shape on the LEFT with 1s.
  2. For each axis: if a_k == b_k OR a_k == 1 OR b_k == 1 → output is max(a_k, b_k).
  3. Otherwise → BroadcastShapeMismatch.
  4. Empty shape () broadcasts against any shape (treated as (1,)).
```

### 3. Type-promotion rules — `result_type` per numpy 2.x

NumPy 2.x landed NEP 50 promotion semantics (https://numpy.org/neps/nep-0050-scalar-promotion.html).
For the M7.0 5-dtype tier, the relevant table is:

| LHS \ RHS | Bool | Int32 | Int64 | Float32 | Float64 |
|---|---|---|---|---|---|
| **Bool** | Bool | Int32 | Int64 | Float32 | Float64 |
| **Int32** | Int32 | Int32 | Int64 | Float64 | Float64 |
| **Int64** | Int64 | Int64 | Int64 | Float64 | Float64 |
| **Float32** | Float32 | Float64 | Float64 | Float32 | Float64 |
| **Float64** | Float64 | Float64 | Float64 | Float64 | Float64 |

Notable rows:
- `Int32 + Float32 → Float64` — matches NEP 50 ("can't fit i32 in f32 mantissa, promote to f64").
- `Bool` is a 1-byte integer-ish; promotion treats it as smaller than int32.
- `Float32 + Float64 → Float64` (standard width-up).
- Same-dtype operations preserve dtype.

For unary ops (sin/cos/exp/log/sqrt) on integer inputs: promote the
input to `Float64` first, then apply (matches numpy).

| Option | Notes | Selected? |
|---|---|---|
| **Hand-coded 25-entry table** in `crates/cobrust-numpy/src/promote.rs` | Explicit, auditable, fast. | **Yes** |
| Compute via dtype "kind" + width | Less code but harder to deviate from numpy where needed. | No |
| Defer to `ndarray::result_type` | Doesn't exist; ndarray is generic over T. | No |

**Pick**: hand-coded 25-entry table. The function `result_type(a:
Dtype, b: Dtype) -> Dtype` is a `match` over both. Tested via
the differential corpus (`ufunc_differential.rs` walks all 25
pairs).

**Comparison-op promotion**: `eq / ne / lt / le / gt / ge` all
return `Dtype::Bool` regardless of input dtypes — matches numpy.

### 4. Error semantics — overflow / NaN / div-by-zero

| Case | Numpy 2.x behavior | Cobrust M7.1 behavior | Rationale |
|---|---|---|---|
| `int + int` overflow | wraps (two's-complement); raises `RuntimeWarning` (filterable). | wraps (Rust `wrapping_add` etc.); no warning. | Matches Rust semantics + numpy default. Differential gate sets seed range tight enough to avoid overflow noise; documented divergence. |
| `float + float` overflow | produces `inf`. | produces `inf` (IEEE 754 default). | Matches numpy. |
| Float NaN propagation | NaN propagates through every op. | NaN propagates (IEEE 754 default). | Matches numpy. |
| Float div-by-zero | `0.0 / 0.0 → NaN`, `+x / 0.0 → +inf`, `-x / 0.0 → -inf`. | Same (IEEE 754). | Matches numpy. |
| **Int div-by-zero** | raises `ZeroDivisionError` (Python exception). | returns `Err(NumpyError { kind: IntegerDivisionByZero, ... })`. | Constitution §2.2 — Result default. Matches numpy outcome (operation fails); shape of failure is Cobrust-native. |
| `0 ** 0` | returns 1 (matches Python). | returns 1. | numpy + Python convention. |
| `negative ** non-integer` (float) | returns NaN. | returns NaN. | IEEE 754 default. |

The single new error variant `IntegerDivisionByZero` lands in
`crates/cobrust-numpy/src/error.rs` (existing `NumpyErrorKind`
extended). Plus `BroadcastShapeMismatch` and `TypePromotionFailure`
for the new fail paths.

### 5. L2.perf gate flip — closes M7.0 follow-up #3

ADR-0010 §3 set the numerical tier 0.5× floor; ADR-0013 deferred
the flip to M7.1. M7.1 flips:

- `corpus/numpy/M7.1/perf.toml` sets `threshold = 0.5,
  pass_ratio = 1.0, n_iters = 100, n_inputs = 32`.
- `crates/cobrust-numpy/tests/ufunc_bench.rs` runs criterion benches
  against a numpy oracle subprocess (same pattern as msgpack).
  Reports persisted under `target/cobrust-bench/numpy-M7.1/<commit>/`.
- `tests/ufunc_pipeline.rs` includes a deliberate-fail case
  (mirroring M6's `msgpack_pipeline_escalates_when_perf_always_fails`):
  a `PerfVerifier` that always returns `Reject` exhausts repair and
  raises `EscalationExceeded`, demonstrating the gate is wired.

**Note on realism**: pure-Rust + `ndarray` element-wise vs numpy's
hand-tuned C+SIMD on the inner loop is a real contest. The 0.5×
floor is intentionally generous (numerical tier per ADR-0010 §3);
the bench harness reports the actual ratios so future M7.1.x
sub-milestones can tighten if warranted.

## Decision

Adopt all five options:

1. Monomorphic ufunc dispatch via `for_each_dtype!` macro.
2. Own `broadcast_shape` (numpy-exact rules) on top of
   `ndarray::ArrayBase::broadcast`.
3. Hand-coded 25-entry `result_type` table per NumPy 2.x NEP 50.
4. Error semantics: integer overflow wraps, float follows IEEE 754,
   integer div-by-zero → `Err(NumpyErrorKind::IntegerDivisionByZero)`,
   broadcast/promotion failures → typed errors.
5. L2.perf flipped to enforced at numerical-tier 0.5× floor; failure
   triggers repair loop.

### Public surface (M7.1 additions)

```rust
// crates/cobrust-numpy/src/array.rs (extended)
impl Array {
    // Binary ops — promote per result_type, broadcast, dispatch.
    pub fn add(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn sub(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn mul(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn div(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn pow(&self, other: &Array) -> Result<Array, NumpyError>;

    // Comparison ops — always return Dtype::Bool.
    pub fn eq(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn ne(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn lt(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn le(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn gt(&self, other: &Array) -> Result<Array, NumpyError>;
    pub fn ge(&self, other: &Array) -> Result<Array, NumpyError>;

    // Element-wise math — promote integer inputs to Float64 first.
    pub fn sin(&self) -> Result<Array, NumpyError>;
    pub fn cos(&self) -> Result<Array, NumpyError>;
    pub fn exp(&self) -> Result<Array, NumpyError>;
    pub fn log(&self) -> Result<Array, NumpyError>;
    pub fn sqrt(&self) -> Result<Array, NumpyError>;
}

// crates/cobrust-numpy/src/promote.rs
pub fn result_type(a: Dtype, b: Dtype) -> Dtype;

// crates/cobrust-numpy/src/broadcast.rs
pub fn broadcast_shape(a: &[usize], b: &[usize]) -> Result<Vec<usize>, NumpyError>;

// crates/cobrust-numpy/src/constructors.rs (extended — closes M7.0 follow-up #2)
pub fn array_i32(values: &[i32], shape: &[usize]) -> Result<Array, NumpyError>;
pub fn array_i64(values: &[i64], shape: &[usize]) -> Result<Array, NumpyError>;
pub fn array_f32(values: &[f32], shape: &[usize]) -> Result<Array, NumpyError>;
pub fn array_f64(values: &[f64], shape: &[usize]) -> Result<Array, NumpyError>;
pub fn array_bool(values: &[bool], shape: &[usize]) -> Result<Array, NumpyError>;

// crates/cobrust-numpy/src/constructors.rs (extended — closes M7.0 follow-up #4)
pub enum NestedList {
    Scalar(f64),
    List(Vec<NestedList>),
}
pub fn array_from_nested(nested: &NestedList, dtype: Dtype) -> Result<Array, NumpyError>;

// crates/cobrust-numpy/src/error.rs (extended)
pub enum NumpyErrorKind {
    // M7.0 variants:
    UnsupportedDtype, ShapeMismatch, NegativeDimension, ZeroStep,
    BoolArangeUnsupported, CastFailed,
    // M7.1 additions:
    IntegerDivisionByZero,
    BroadcastShapeMismatch,
    TypePromotionFailure,
}
```

### Crate layout

Per ADR-0013 §"Decision" the parent-crate strategy holds. M7.1 lands
new modules **inside** `crates/cobrust-numpy/src/`:

```
crates/cobrust-numpy/src/
  array.rs            — extended with binary-op + math methods
  broadcast.rs        — NEW: broadcast_shape + broadcast views
  constructors.rs     — extended with array_<dtype> + array_from_nested
  dtype.rs            — unchanged
  error.rs            — extended with 3 new variants
  lib.rs              — extended re-exports
  print.rs            — unchanged
  promote.rs          — NEW: result_type table
  pyo3_bindings.rs    — extended with binary-op exports (gated)
  ufunc.rs            — NEW: for_each_dtype! macro + per-op inner loops
```

## Consequences

- **Positive**
  - Closes all four M7.0 follow-ups in one milestone.
  - Inner-loop perf is auto-vectorisable; the 0.5× numerical-tier
    floor is achievable on `ndarray::Zip` paths.
  - Numpy-exact broadcasting + promotion rules give users a
    drop-in mental model.
  - The `for_each_dtype!` macro pattern is reusable for M7.2
    (indexing per dtype) + M7.3 (reductions per dtype).

- **Negative**
  - Macro expansion increases compile time; mitigated by keeping
    the macro body small (one inner-loop closure per ufunc).
  - 25-entry `result_type` table is hand-maintained; if M7.x
    widens the dtype set (e.g. adds `int8`, `complex64`), the
    table grows quadratically. ADR-0014.1 will revisit.
  - Integer overflow wraps silently — divergence from numpy's
    `RuntimeWarning`. Documented in `docs/agent/modules/numpy.md`
    "Known divergences" section.

- **Neutral / unknown**
  - Comparison ops returning `Dtype::Bool` (rather than the
    promoted dtype) means `(a < b)` always yields a bool array
    even for int / float inputs. Matches numpy; documented.
  - Real perf ratio against numpy's SIMD is unknown until the
    bench harness runs in CI; the 0.5× floor leaves headroom.

## Evidence

- ADR-0012 §"Sub-milestones" M7.1 row.
- ADR-0013 §"M7.0 manifest fields" + §"Consequences" §"Negative" —
  flagged the four follow-ups this ADR closes.
- ADR-0010 §3 (numerical-tier perf floor 0.5×).
- ADR-0007 (translator pipeline), ADR-0008 (perf + repair),
  ADR-0011 (PyO3 build path) — pipeline + gate inheritance.
- Constitution `CLAUDE.md` §2.2 (no `dyn`), §2.4 (`@py_compat
  numerical(rtol)`), §4.2 (L0..L3), §5.1 (elegant), §5.3
  (efficient).
- NumPy broadcasting docs — https://numpy.org/doc/stable/user/basics.broadcasting.html.
- NumPy NEP 50 (scalar promotion) — https://numpy.org/neps/nep-0050-scalar-promotion.html.
- Upstream `ndarray` 0.16 — https://docs.rs/ndarray/0.16/ndarray/struct.ArrayBase.html#method.broadcast.
