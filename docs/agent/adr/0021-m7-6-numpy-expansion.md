---
doc_kind: adr
adr_id: 0021
title: M7.6 numpy expansion — Complex dtype widening, FFT + polynomial bindings, reduction extensions (cumsum/median/percentile/nan*/tuple-axis)
status: accepted
date: 2026-04-30
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0021: M7.6 numpy expansion — Complex dtype widening, FFT + polynomial bindings, reduction extensions

## Context

ADR-0012 §"Sub-milestones" M7.6+ row was deliberately left open-ended:

> M7.6+ FFT (`rustfft`), polynomial, datetime64, structured arrays —
> open-ended.

ADR-0013 §3 closed the M7.0 dtype tier at five variants
(`Int32 / Int64 / Float32 / Float64 / Bool`) and explicitly named
**`complex*`** as out-of-scope. ADR-0017 §"Out of scope (M7.x deferred)"
named **complex dtypes** as a deferred linalg item. ADR-0016 §"Out of
scope (M7.x deferred)" named **`cumsum / cumprod / median / percentile
/ nansum / nanmean / nanmin / nanmax`** plus **tuple-axis reduction
(`axis=(0, 2)`)** as deferred reduction items.

This ADR collects three deferral buckets into one M7.6 sub-milestone:

- **Bucket A — FFT + polynomial.** ADR-0012 §"Backend strategy" already
  pinned `rustfft` as the FFT backend. M7.6 binds `rustfft = "6"` for
  `fft / ifft / rfft / irfft` (1-D real and complex). Adds a
  `polynomial.fit / polyval / poly` minimal subset reusing M7.4's
  linear-solve kernel.
- **Bucket B — Complex dtype.** Widens the `Dtype` enum from 5 to 7
  variants by adding `Complex64` (two `f32`) and `Complex128` (two
  `f64`), backed by `num_complex = "0.4"`. Extends `result_type` per
  NEP 50 so `float + complex → complex`. Re-routes M7.1 ufuncs to
  handle complex variants. Re-routes M7.4 `eigh` to a Hermitian path
  when the input dtype is complex.
- **Bucket C — Reduction extensions.** Adds `cumsum / cumprod`
  (axis-aware), `median / percentile(q)` (axis-aware), `nansum /
  nanmean / nanmin / nanmax` (skip-NaN variants), and tuple-axis
  reductions for `sum / prod / mean / min / max` (`axis=(0, 2)`
  style).

This ADR pins the decisions for all three buckets in one document so
the M7.6 P9 sprint has a single binding source. Per ADR-0013 the
parent-crate strategy holds; M7.6 lands two new modules
(`fft.rs`, `poly.rs`) inside `crates/cobrust-numpy/src/` and extends
`dtype.rs / promote.rs / array.rs / ufunc.rs / linalg.rs / reduce.rs /
error.rs`.

## Options considered

### 1. Bucket A — FFT backend pin

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| **`rustfft = "6"` (latest stable)** | matches ADR-0012 §"Backend strategy" pin; 1-D + N-D; pure-Rust; SIMD-aware Cooley-Tukey | adds one transitive dep (`num-complex`) — but Bucket B already brings that in | **Yes** |
| Hand-rolled 1-D Cooley-Tukey | zero new deps | reinventing the wheel; ADR-0012 already named `rustfft` | No |
| `realfft` separate crate | smaller surface | inconsistent API with main `rustfft`; redundant | No |

`rustfft = "6"` is also what ADR-0012 §"Evidence" names. License: MIT
OR Apache-2.0 — compatible per ADR-0001.

### 2. Bucket A — Polynomial subset surface

NumPy's `numpy.polynomial.polynomial` module is large (~30 functions).
M7.6 picks a minimal load-bearing subset:

| Op | Signature | Backend | Notes |
|---|---|---|---|
| `polyval(p, x)` | `(coeffs: &Array, x: &Array) -> Array` | Horner's method on `ndarray` | matches `numpy.polynomial.polynomial.polyval` |
| `polyfit(x, y, deg)` | `(x: &Array, y: &Array, deg: usize) -> Result<Array, NumpyError>` | Vandermonde + M7.4 `solve` | matches `numpy.polyfit` (least-squares fit) |
| `poly(roots)` | `(roots: &Array) -> Array` | iterative convolution | matches `numpy.poly` (root → coefficients) |

Closed at 3 ops; widening to `polyder / polyint / polymul / polyroots
/ chebfit` is an explicit ADR-bumpable decision later.

`polyfit` reuses M7.4's `solve` kernel (LU partial pivot) on the
normal-equation Vandermonde matrix — no new linalg primitive needed.

### 3. Bucket B — Dtype enum widening 5 → 7

The constitution §2.4 `@py_compat` rule and ADR-0013 §3 declared the
M7.0 enum closed; **adding variants is an explicit ADR-bumpable
decision**. M7.6 widens to 7:

| Variant | Rust type | Python string(s) | NEP 50 promotion id |
|---|---|---|---|
| `Int32` | `i32` | `"int32"` / `"i4"` | (existing) |
| `Int64` | `i64` | `"int64"` / `"i8"` | (existing) |
| `Float32` | `f32` | `"float32"` / `"f4"` | (existing) |
| `Float64` | `f64` | `"float64"` / `"f8"` | (existing) |
| `Bool` | `bool` | `"bool"` / `"?"` | (existing) |
| **`Complex64`** | `num_complex::Complex<f32>` | `"complex64"` / `"c8"` | float-pair, base width 4 |
| **`Complex128`** | `num_complex::Complex<f64>` | `"complex128"` / `"c16"` | float-pair, base width 8 |

`item_size` (per ADR-0013 §"Public surface"):

- `Complex64` → 8 bytes (two `f32`).
- `Complex128` → 16 bytes (two `f64`).

The seven-variant `Array` enum is closed again at M7.6; further
widening (e.g., `Int8 / UInt32 / Float16`) is another ADR.

### 4. Bucket B — `result_type` extension (NEP 50 for complex)

Per NumPy 2.x NEP 50, complex is the "top of the lattice" for any
finite operand mix. The 49-entry table (was 25 at M7.1) is:

| Operand kinds | Result |
|---|---|
| `Complex128 + anything (other than Complex128)` | `Complex128` |
| `Complex64 + Float64 / Int64 / Int32` | `Complex128` (mantissa wider than `f32`) |
| `Complex64 + Float32 / Bool` | `Complex64` |
| `Complex64 + Complex64` | `Complex64` |
| `Float64 + Complex64` | `Complex128` |
| `Int64 + Complex64` | `Complex128` |
| `Float32 + Complex64` | `Complex64` |
| `Bool + Complex64` | `Complex64` |
| `Bool + Complex128` | `Complex128` |
| (rest) | per ADR-0014 / M7.1 |

**Pick**: hand-coded 49-entry match table in `promote.rs`. Same
auditable, fast pattern as M7.1.

### 5. Bucket B — Ufunc routing for complex

| Op family | Complex behavior | Notes |
|---|---|---|
| Binary arithmetic (`add / sub / mul / div / pow`) | natural via `num_complex` | `pow` uses `Complex::powc` |
| Comparison (`eq / ne`) | element-wise complex equality (real == real && imag == imag) | matches numpy |
| Comparison (`lt / le / gt / ge`) | **`ComplexNotOrderable` error** | matches numpy: `TypeError: '<' not supported between complex` |
| Element-wise math (`sin / cos / exp / log / sqrt`) | complex versions | `Complex::sin / cos / exp / ln / sqrt` |

A new error variant `ComplexNotOrderable` lands.

### 6. Bucket B — Linalg routing for complex

M7.4 ADR-0017 §3 declared float-only. M7.6 lifts the float-only rule
**only for `eigh`** — the symmetric-matrix Jacobi path. With complex
input, `eigh` becomes the **Hermitian** path:

- A Hermitian matrix `H` satisfies `H == H^H` (conjugate transpose).
- `eigh(H_complex)` returns real eigenvalues and complex eigenvectors.
- Algorithm: convert the `n × n` complex Hermitian to a real `2n × 2n`
  symmetric matrix and run M7.4's Jacobi path; extract complex
  eigenvectors and real eigenvalues from the result.

`matmul / dot / det / solve / inv / svd / cholesky` remain
**float-only at M7.6**. Widening these is M7.7+. Documented as known
M7.6 scope-window.

### 7. Bucket C — `cumsum / cumprod` (axis-aware)

| Signature | Returns | Backend |
|---|---|---|
| `cumsum(arr, axis: Option<i64>) -> Array` | `Array` (same shape, dtype preserved per `sum`) | scan loop on `ndarray::Axis` |
| `cumprod(arr, axis: Option<i64>) -> Array` | same | scan loop |

`axis=None` flattens then scans; `axis=k` scans along axis k.

### 8. Bucket C — `median / percentile`

| Signature | Returns | Backend |
|---|---|---|
| `median(arr, axis) -> Array` | float | sort-based median; matches numpy linear interpolation for even-length |
| `percentile(arr, q: f64, axis) -> Array` | float | sort-based; `q ∈ [0, 100]`; linear interpolation matching numpy default |

`q < 0 || q > 100` → `PercentileOutOfRange` error.

### 9. Bucket C — `nansum / nanmean / nanmin / nanmax`

Skip-NaN variants. For float dtypes, NaN is filtered before the
reduction. For int/bool/complex dtypes, behaves identically to the
non-`nan*` form (no NaN in those dtypes, except complex where NaN-real
or NaN-imag is treated as NaN).

| Op | Empty-after-filter behavior |
|---|---|
| `nansum` | additive identity (0) |
| `nanmean` | NaN |
| `nanmin / nanmax` | `NanMinMaxAllNaN` error (matches numpy `RuntimeWarning + NaN`; we err for explicit) — **wait**: numpy actually returns NaN+warning. M7.6 follows numpy: returns NaN, no error. **Decision**: match numpy. |

### 10. Bucket C — Tuple-axis reductions

NumPy's `axis=(0, 2)` syntax — reduce over multiple axes simultaneously.
M7.6 adds tuple-axis support to `sum / prod / mean / min / max`
(but NOT to `argmin / argmax` — they don't have a meaningful tuple-axis
semantics in numpy either).

| Signature | Notes |
|---|---|
| `sum(arr, axis_tuple: &[i64])` | new free function variant |
| `Array::sum_axes(&[i64])` | method form |

`axis_tuple` empty → `EmptyAxisTuple` error (numpy raises `ValueError`).
Negative axes normalised; duplicates → `EmptyAxisTuple` (matches numpy
`AxisError`).

The implementation is "fold-axis sequentially in descending order" so
each fold preserves the indices of unfolded axes.

### 11. Error taxonomy additions

Three new `NumpyErrorKind` variants land:

| Variant | Trigger | Match numpy |
|---|---|---|
| `ComplexNotOrderable` | `lt / le / gt / ge` on complex dtype | `TypeError: '<' not supported between complex` |
| `PercentileOutOfRange` | `percentile(q)` with `q < 0 || q > 100` | `ValueError: Percentiles must be in the range [0, 100]` |
| `EmptyAxisTuple` | `axis=()` or duplicate axes | `numpy.AxisError` |

### 12. Differential gate tolerances

| Dtype | Tolerance | Rationale |
|---|---|---|
| `Int32 / Int64 / Bool` | bit-identical | matches M7.0..M7.5 |
| `Float32 / Float64` | `rtol=1e-7` | matches M7.1..M7.3 |
| `Complex64 / Complex128` | `rtol=1e-5` | FFT round-trip error budget |

`rtol=1e-5` for complex is chosen because:
1. FFT round-trip (`ifft(fft(x))` vs `x`) accumulates O(N log N × eps)
   in single precision; `rtol=1e-7` is too tight.
2. Polynomial fit on conditioned data agrees within `rtol=1e-5`.
3. NumPy's `numpy.testing.assert_allclose` defaults to `rtol=1e-7,
   atol=0`; we relax to `1e-5` for complex per the FFT precedent.

### 13. Cargo.toml additions

```toml
# Bucket A — FFT (rustfft) — bound to numpy.fft surface per ADR-0012.
rustfft = "6"
# Bucket B — Complex dtype storage. num_complex provides Complex<T>.
num_complex = "0.4"
```

Both `rustfft` and `num_complex` are MIT OR Apache-2.0; license-compatible
per ADR-0001.

## Decision

Adopt all 13 options.

### Public surface (M7.6 additions)

```rust
// crates/cobrust-numpy/src/dtype.rs (extended)
pub enum Dtype {
    Int32, Int64, Float32, Float64, Bool,
    Complex64,    // num_complex::Complex<f32>; M7.6 per ADR-0021
    Complex128,   // num_complex::Complex<f64>; M7.6 per ADR-0021
}

// crates/cobrust-numpy/src/array.rs (extended)
pub enum Array {
    Int32(ndarray::ArrayD<i32>),
    Int64(ndarray::ArrayD<i64>),
    Float32(ndarray::ArrayD<f32>),
    Float64(ndarray::ArrayD<f64>),
    Bool(ndarray::ArrayD<bool>),
    Complex64(ndarray::ArrayD<num_complex::Complex<f32>>),
    Complex128(ndarray::ArrayD<num_complex::Complex<f64>>),
}

// crates/cobrust-numpy/src/fft.rs (NEW)
pub fn fft(arr: &Array) -> Result<Array, NumpyError>;     // 1-D forward complex
pub fn ifft(arr: &Array) -> Result<Array, NumpyError>;    // 1-D inverse complex
pub fn rfft(arr: &Array) -> Result<Array, NumpyError>;    // 1-D forward real → complex
pub fn irfft(arr: &Array, n: usize) -> Result<Array, NumpyError>;  // 1-D inverse complex → real

// crates/cobrust-numpy/src/poly.rs (NEW)
pub fn polyval(p: &Array, x: &Array) -> Result<Array, NumpyError>;
pub fn polyfit(x: &Array, y: &Array, deg: usize) -> Result<Array, NumpyError>;
pub fn poly(roots: &Array) -> Result<Array, NumpyError>;

// crates/cobrust-numpy/src/reduce.rs (extended)
pub fn cumsum(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn cumprod(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn median(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn percentile(arr: &Array, q: f64, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn nansum(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn nanmean(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn nanmin(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn nanmax(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn sum_axes(arr: &Array, axes: &[i64]) -> Result<Array, NumpyError>;
pub fn prod_axes(arr: &Array, axes: &[i64]) -> Result<Array, NumpyError>;
pub fn mean_axes(arr: &Array, axes: &[i64]) -> Result<Array, NumpyError>;
pub fn min_axes(arr: &Array, axes: &[i64]) -> Result<Array, NumpyError>;
pub fn max_axes(arr: &Array, axes: &[i64]) -> Result<Array, NumpyError>;

// crates/cobrust-numpy/src/error.rs (extended)
pub enum NumpyErrorKind {
    // ... M7.0..M7.5 variants ...
    ComplexNotOrderable,        // lt/le/gt/ge on complex dtype
    PercentileOutOfRange,       // percentile(q) q < 0 || q > 100
    EmptyAxisTuple,             // axis=() or duplicate axes in tuple-axis
}
```

### Crate layout

Per ADR-0013 §"Decision" the parent-crate strategy holds. M7.6 lands
two new modules **inside** `crates/cobrust-numpy/src/`:

```
crates/cobrust-numpy/src/
  array.rs            — extended with Complex64 / Complex128 variants
                        (tagged-union widening 5 → 7)
  broadcast.rs        — unchanged (broadcast_shape is dtype-agnostic)
  constructors.rs     — extended with array_complex64 / array_complex128
                        + complex zeros/ones/arange dispatch
  dtype.rs            — extended with Complex64 / Complex128 variants
  error.rs            — extended with 3 new variants
  fft.rs              — NEW: fft / ifft / rfft / irfft (rustfft binding)
  index.rs            — extended for Complex variants in match arms
  lib.rs              — extended re-exports
  linalg.rs           — extended: eigh accepts complex (Hermitian path)
  poly.rs             — NEW: polyval / polyfit / poly
  print.rs            — extended for Complex variants in repr
  promote.rs          — extended NEP 50 table for complex
  pyo3_bindings.rs    — extended (Complex64/Complex128 numpy interop)
  random.rs           — unchanged at M7.6 (random complex distributions
                        deferred to M7.7+)
  reduce.rs           — extended with cumsum/cumprod/median/percentile/
                        nan*/tuple-axis ops
  ufunc.rs            — extended for complex routing
  view.rs             — extended for Complex variants in match arms
```

### Cargo.toml additions

```toml
[dependencies]
# ... existing deps ...
rustfft = "6"           # M7.6 FFT backend per ADR-0021 §1
num_complex = "0.4"     # M7.6 Complex dtype storage per ADR-0021 §3
```

### M7.6 scope window

**In scope**:

- **Bucket A — FFT + polynomial.**
  - 4 FFT ops: `fft / ifft / rfft / irfft`.
  - 3 poly ops: `polyval / polyfit / poly`.
- **Bucket B — Complex dtype.**
  - `Dtype` enum widening: 5 → 7 variants.
  - `Array` enum widening: 5 → 7 variants.
  - `result_type` extended NEP 50 table for complex.
  - Ufunc routing for complex (binary arithmetic + element-wise math
    + comparison errors).
  - M7.4 `eigh` Hermitian path for complex inputs.
  - 1 new error variant: `ComplexNotOrderable`.
- **Bucket C — Reduction extensions.**
  - 5 new reductions: `cumsum / cumprod / median / percentile /
    nansum / nanmean / nanmin / nanmax` (last 4 are nan-skip
    variants; total 8 new ops).
  - Tuple-axis reduction for 5 ops: `sum / prod / mean / min /
    max` (`sum_axes`, etc.).
  - 2 new error variants: `PercentileOutOfRange`, `EmptyAxisTuple`.
- ≥ 30 well-typed + ≥ 20 ill-typed programs per bucket suite (3 × ≥ 50
  total).
- ≥ 200 differential inputs per new op (FFT, poly, complex
  arithmetic, reduction extensions) against upstream numpy 2.0.2 at
  the dtype-tier tolerances above (bit-identical for int/bool,
  `rtol=1e-7` for float, `rtol=1e-5` for complex).
- ADR-0021 lands; doc tree updated; doc-coverage extended.

**Out of scope (M7.7+ deferred)**:

- N-D FFT (`fft2 / fftn / ifft2 / ifftn`) — extends the same backend.
- Polynomial Chebyshev / Legendre / Laguerre / Hermite bases.
- Complex `matmul / dot / det / solve / inv / svd / cholesky` (only
  `eigh` is widened at M7.6).
- `keepdims=True` parameter for cumsum / median / percentile / nan*.
- `out=` parameter for any reduction.
- `where=` parameter for any reduction.
- `nanstd / nanvar / nanargmin / nanargmax / nanpercentile / nanmedian`.
- `dtype=` parameter (forced result dtype) on cumsum/cumprod.
- Complex datetime64 / timedelta64 / structured arrays — separate ADR.

## Consequences

- **Positive**
  - Closes three deferral buckets in one milestone — cleaner
    bookkeeping than three separate M7.x sub-milestones.
  - Complex dtype widening unblocks downstream numerical workflows
    (FFT, eigendecomposition with complex eigenvectors).
  - FFT + polynomial fill the constitution §7 "numpy core subset"
    promise — the post-M7.5 ecosystem can now do signal processing.
  - Reduction extensions match the most-asked-for numpy ops not
    covered by M7.3.
  - Closed seven-dtype set + closed reduction taxonomy + closed error
    enum keep the surface auditable; further widening is
    ADR-bumpable.

- **Negative**
  - Tagged-union widening `Array { 5 → 7 variants }` is a ripple
    edit: every match site in `array.rs / index.rs / view.rs /
    print.rs / reduce.rs / linalg.rs / promote.rs / random.rs /
    pyo3_bindings.rs` must add `Complex64 / Complex128` arms.
    Mitigated by exhaustive-match compiler errors — adding the
    variant immediately surfaces every site that needs updating.
  - `num_complex` adds one transitive dep; mitigated by `rustfft`
    already requiring it (cost shared).
  - `eigh` for complex Hermitian via the `2n × 2n` real reduction is
    O(8N⁴) — eight times slower than the float-only path. Documented
    as a known M7.6 perf consequence; M7.7+ may add direct complex
    Jacobi (Wilkinson 1965).

- **Neutral / unknown**
  - FFT round-trip accuracy is `rtol=1e-5` for complex; this is
    looser than the rest of the codebase. If real-world workloads
    need tighter, M7.7+ may switch backends to `realfft` or hand-roll
    Kahan accumulation in the FFT inner loop.
  - `nanmin / nanmax` returning NaN (not error) on all-NaN input
    matches numpy but diverges from M7.3's `min / max` raising
    `ReductionEmptyArray` on empty. Documented in the module spec.
  - Tuple-axis fold-in-descending-order is one of two reasonable
    strategies; the other is fold-in-ascending-order. Numpy uses
    descending; we follow.

## Evidence

- ADR-0012 §"Sub-milestones" M7.6+ row (open-ended scope).
- ADR-0013 §3 (closed dtype tier; widening is ADR-bumpable).
- ADR-0014 §3 (NEP 50 promotion table — extended in this ADR).
- ADR-0016 §"Out of scope" (cumsum / median / percentile / nan* /
  tuple-axis deferred).
- ADR-0017 §"Out of scope" (complex dtypes deferred).
- ADR-0010 §3 (numerical-tier perf floor 0.5x — inherits ENFORCED).
- Constitution `CLAUDE.md` §2.4 (`@py_compat numerical(rtol=…)`),
  §4.2 (L0..L3 gates), §7 (M7+ "the big one"), §5.1 (elegant), §5.3
  (efficient).
- NumPy FFT docs — https://numpy.org/doc/stable/reference/routines.fft.html.
- NumPy polynomial docs —
  https://numpy.org/doc/stable/reference/routines.polynomials.classes.html.
- NumPy NEP 50 — https://numpy.org/neps/nep-0050-scalar-promotion.html.
- NumPy complex eigendecomposition —
  https://numpy.org/doc/stable/reference/generated/numpy.linalg.eigh.html.
- Upstream `rustfft` 6.x — https://crates.io/crates/rustfft (MIT OR
  Apache-2.0; license-compatible per ADR-0001).
- Upstream `num_complex` 0.4 — https://crates.io/crates/num-complex
  (MIT OR Apache-2.0; license-compatible per ADR-0001).
- Wilkinson, "The Algebraic Eigenvalue Problem", 1965 — Hermitian
  Jacobi reference for M7.7+.
