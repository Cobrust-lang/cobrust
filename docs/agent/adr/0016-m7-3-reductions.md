---
doc_kind: adr
adr_id: 0016
title: M7.3 reductions — kind taxonomy, axis semantics, pairwise summation, ddof, empty-array behavior
status: accepted
date: 2026-04-30
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0016: M7.3 reductions — kind taxonomy, axis semantics, pairwise summation, ddof, empty-array behavior

## Context

ADR-0012 fixed the M7 sub-milestone breakdown; ADR-0013/0014/0015
landed M7.0 (ndarray foundation), M7.1 (ufuncs + broadcasting + NEP
50 promotion) and M7.2 (indexing + views). M7.3's mandate from
ADR-0012 §"Sub-milestones":

> Reductions: `sum/prod/mean/std/var/min/max/argmin/argmax` with
> `axis=None` and `axis=k`. Backend: `ndarray::Zip` + `fold_axis`.
> Acceptance gate: numerical agreement; pairwise summation for
> floats.

This ADR pins five M7.3-binding decisions:

1. **Reduction-kind taxonomy** — closed set of nine reductions; how
   each disposes of its input dtype.
2. **Axis semantics** — `axis: Option<i64>` parameter, with
   `None` → reduce-all and `Some(k)` → reduce-along-axis-k.
3. **Pairwise summation** — for float `sum` / `mean` / `var` / `std`,
   we use the `ndarray::ArrayBase::sum` (which delegates to a
   pairwise tree via the iterator's `Sum` impl over fold-axis
   chunks) plus our own pairwise reducer for the axis path.
4. **ddof parameter for std/var** — default 0; clamped against the
   reduction length.
5. **Empty-array behavior** — `sum=0`, `prod=1`, `mean=NaN`,
   `min/max=ReductionEmptyArray`, `argmin/argmax=ReductionEmptyArray`,
   `std/var=NaN`. Matches numpy 2.x.

## Options considered

### 1. Reduction-kind taxonomy — closed set of nine

ADR-0012 §"Sub-milestones" M7.3 row enumerates exactly nine. We
keep the surface closed at nine; widening to `cumsum / cumprod /
ptp / median / percentile` is an explicit ADR-bumpable decision in
M7.x.

| Reduction | Returns | Promotion |
|---|---|---|
| `sum(arr)` | `Array` | int dtypes preserved (no promotion); float preserved; bool → Int64 (matches numpy) |
| `prod(arr)` | `Array` | same as `sum` |
| `mean(arr)` | `Array(Float64)` | always Float64 (numpy: int → float64; float preserved as Float64 for f32 too — we follow that) — wait, numpy keeps `f32 → f32` for mean. We follow that exactly: `f32 → f32`, `f64 → f64`, int/bool → `f64`. |
| `std(arr, ddof)` | `Array(Float64-ish)` | same as mean |
| `var(arr, ddof)` | `Array(Float64-ish)` | same as mean |
| `min(arr)` | `Array` | dtype preserved |
| `max(arr)` | `Array` | dtype preserved |
| `argmin(arr)` | `Array(Int64)` | always Int64 (matches numpy intp) |
| `argmax(arr)` | `Array(Int64)` | always Int64 (matches numpy intp) |

Per-reduction promotion rules tabulated above. Notably `mean / std
/ var` over `Float32` returns `Float32` (matches numpy), not
`Float64` — important for memory-usage parity.

### 2. Axis semantics — `axis: Option<i64>`

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| **`axis: Option<i64>`** with `None` → reduce-all, `Some(k)` → reduce-axis-k (negative-axis aware) | matches numpy's `axis=` kwarg one-to-one; simple closed signature | tuple-axis (`axis=(0, 2)`) deferred to M7.x | **Yes** |
| `Vec<i64>` (general tuple of axes) | matches numpy's full kwarg | YAGNI for M7.3 — tuple-axis reduction is M7.x; full numpy support requires multi-axis fold | No |
| Separate `_all` / `_axis` functions | explicit | API surface bloat 2× | No |

**Pick**: `axis: Option<i64>`. `None` reduces every axis (returns
0-d array, except `mean / std / var` which collapse to scalar
Float64; `argmin / argmax` flattened-index Int64). `Some(k)` reduces
along axis k only; out-of-bounds raises `IndexError`. Negative axes
normalise (`k < 0` → `k + ndim`).

Tuple-axis (`axis=(0, 2)`) is deferred to M7.x — explicit
non-goal in this ADR.

### 3. Pairwise summation for floats

NumPy uses pairwise summation in `np.sum / np.add.reduce` to
suppress floating-point error for long reductions. The asymptotic
error is O(log n × eps) instead of naive O(n × eps).

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| Naive accumulator `let mut acc = 0.0; for x in iter { acc += x; }` | Simplest. | O(n × eps) error; fails `rtol=1e-7` for n > 10⁶. | No |
| **Pairwise tree** — partition into chunks of size 8, sum each chunk, then sum chunks pairwise | Matches numpy's algorithm; O(log n × eps) error. | One extra Vec allocation for the chunked partials. | **Yes** |
| Kahan compensated sum | Best accuracy (O(eps)). | ~2× slower than pairwise; more code. | No (overkill for M7.3) |
| Use `ndarray::ArrayBase::sum` directly | Zero new code. | ndarray's `sum` uses `Iterator::sum` which is naive `Add::add` fold → O(n × eps). | No |

**Pick**: pairwise tree, with chunk size 8 (matches numpy's
`PW_BLOCKSIZE=128` cascaded down to 8 for the leaf). Implementation
in `crates/cobrust-numpy/src/reduce.rs::pairwise_sum_f64` /
`pairwise_sum_f32`. For integer reductions, use naive (no precision
issues — wraps per Rust's `wrapping_add`).

### 4. ddof for std/var

NumPy 2.x: `std(arr, ddof=0)` computes `sqrt(sum((x-mean)^2) / N)`;
`std(arr, ddof=1)` divides by `N-1` (Bessel correction). Same for
`var`.

| Case | numpy 2.x | cobrust-numpy M7.3 |
|---|---|---|
| `std(empty, ddof=0)` | `RuntimeWarning: Degrees of freedom <= 0; results will be NaN`; returns NaN | returns NaN |
| `std(arr, ddof=N)` (where N = len(arr)) | NaN (denominator = 0) | NaN |
| `std(arr, ddof=N+1)` | NaN (denominator < 0) | NaN |

We accept any `ddof: u32` (numpy: `ddof: int`, but negative is
nonsensical and we type-block). Default is 0. The result is NaN
when `N - ddof <= 0`.

### 5. Empty-array behavior

NumPy 2.x:

| op | empty-array behavior |
|---|---|
| `sum([])` | 0 (additive identity) |
| `prod([])` | 1 (multiplicative identity) |
| `mean([])` | NaN + RuntimeWarning |
| `std([], ddof)` | NaN + RuntimeWarning |
| `var([], ddof)` | NaN + RuntimeWarning |
| `min([])` | `ValueError: zero-size array to reduction operation` |
| `max([])` | same |
| `argmin([])` | `ValueError: attempt to get argmin of an empty sequence` |
| `argmax([])` | same |

We follow numpy:

- `sum / prod` on empty arrays: return the identity (0 / 1) per the
  source-array dtype.
- `mean / std / var` on empty arrays: return NaN. (For int dtypes,
  the result is `Float64(NaN)` since the promotion takes us out of
  int-land.)
- `min / max / argmin / argmax` on empty arrays: return
  `Err(NumpyErrorKind::ReductionEmptyArray)` with a numpy-shaped
  message.

A new error variant `ReductionEmptyArray` is added.

## Decision

Adopt all five options:

1. Closed nine-reduction set per the table above with documented
   promotion rules.
2. `axis: Option<i64>` parameter; negative-axis aware.
3. Pairwise summation for float `sum / mean / std / var` (chunk size
   8, recursive bisection); naive int (Rust wrapping). Used both
   for `axis=None` and `axis=k` paths.
4. `ddof: u32` for std/var (default 0). NaN when `N - ddof <= 0`.
5. Empty-array behavior matches numpy: identity for sum/prod, NaN
   for mean/std/var, `ReductionEmptyArray` error for
   min/max/argmin/argmax.

### Public surface (M7.3 additions)

```rust
// crates/cobrust-numpy/src/reduce.rs (NEW)

pub fn sum(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn prod(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn mean(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn std(arr: &Array, axis: Option<i64>, ddof: u32) -> Result<Array, NumpyError>;
pub fn var(arr: &Array, axis: Option<i64>, ddof: u32) -> Result<Array, NumpyError>;
pub fn min(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn max(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn argmin(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;
pub fn argmax(arr: &Array, axis: Option<i64>) -> Result<Array, NumpyError>;

// Method-style API on Array (terse, mirrors numpy's a.sum() etc.)
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

// crates/cobrust-numpy/src/error.rs (extended)
pub enum NumpyErrorKind {
    // ... M7.0 + M7.1 + M7.2 variants ...
    /// Empty-array passed to min/max/argmin/argmax. Matches numpy's
    /// `ValueError: zero-size array to reduction operation`.
    ReductionEmptyArray,
}
```

### Crate layout

Per ADR-0013 §"Decision" the parent-crate strategy holds. M7.3 lands
one new module **inside** `crates/cobrust-numpy/src/`:

```
crates/cobrust-numpy/src/
  array.rs            — extended with reduction methods (9 methods)
  broadcast.rs        — unchanged
  constructors.rs     — unchanged
  dtype.rs            — unchanged
  error.rs            — extended with 1 new variant (ReductionEmptyArray)
  index.rs            — unchanged
  lib.rs              — extended re-exports
  print.rs            — unchanged
  promote.rs          — unchanged
  pyo3_bindings.rs    — unchanged for M7.3 (PyO3 surface frozen at M7.0)
  reduce.rs           — NEW: 9 reductions + pairwise summation
  ufunc.rs            — unchanged
  view.rs             — unchanged
```

### M7.3 scope window

**In scope**:

- 9 reductions: `sum / prod / mean / std / var / min / max /
  argmin / argmax`.
- `axis: Option<i64>` parameter; `None` = reduce-all,
  `Some(k)` = reduce-axis-k (negative-axis aware).
- `ddof: u32` for `std / var` (default 0).
- Pairwise summation for float `sum / mean / std / var`.
- One new `NumpyErrorKind` variant: `ReductionEmptyArray`.
- L0..L1..L2.behavior gates per ADR-0007 + ADR-0008 carry over.
- L2.perf at numerical-tier 0.5× (per ADR-0010 §3); reports under
  `target/cobrust-bench/numpy-M7.3/<commit>/`. Bench-test pattern
  matches M7.1 / M7.2.
- ≥ 50 well-typed + ≥ 50 ill-typed programs.
- ≥ 1000 fuzz inputs per reduction, panic-free + matching numpy
  via the differential harness.
- Pairwise-summation accuracy: a test with N=10⁶ tiny floats
  matches numpy within `rtol=1e-12`.

**Out of scope (M7.x deferred)**:

- Tuple-axis reduction (`axis=(0, 2)`).
- `keepdims=True` parameter.
- `out=` parameter.
- `where=` parameter (selective reduction).
- `cumsum / cumprod / median / percentile / nanmin / nanmax /
  nansum / nanmean` — all M7.x.
- `dtype=` parameter (forced result dtype).

## Consequences

- **Positive**
  - Closes the reduction surface that downstream M7.4 linalg needs
    (`matmul` is an outer-product + sum reduction; we avoid having
    to inline that elsewhere).
  - Pairwise summation matches numpy's accuracy floor — users get
    drop-in numerical fidelity.
  - Closed nine-reduction set is auditable; expansion is an
    ADR-bumpable decision.
  - Method-style API (`a.sum()`) keeps user code idiomatic.

- **Negative**
  - Pairwise summation needs a recursive helper; the chunk-size-8
    constant is hand-tuned to match numpy's behavior. Documented in
    the source.
  - `mean / std / var` over `Float32` returns `Float32` (not
    `Float64`); users used to "promote everything to f64" mental
    models will need to adjust. Matches numpy.
  - Empty-array `min / max / argmin / argmax` returning `Err(...)`
    rather than panicking means callers must handle the `Result`.
    Matches our constitution §2.2.

- **Neutral / unknown**
  - Real perf ratio for `axis=k` reductions vs numpy's
    SIMD-optimised `axis_iter` is unknown until the bench runs.
    The 0.5× floor leaves headroom.
  - `bool` array reductions: `sum(bool) -> Int64` (matches numpy:
    counts true values). `mean(bool) -> Float64` (matches numpy:
    fraction of true values).

## Evidence

- ADR-0012 §"Sub-milestones" M7.3 row.
- ADR-0013 §"Decision" — parent-crate layout we extend.
- ADR-0014 §1 — monomorphic dispatch precedent (we use the same
  pattern in `reduce.rs`).
- ADR-0015 §3 — view-vs-copy contract (axis-reductions return a new
  Array, never a view; matches numpy).
- ADR-0010 §3 (numerical-tier perf floor 0.5×).
- ADR-0007 (translator pipeline), ADR-0008 (perf + repair),
  ADR-0011 (PyO3 build path).
- Constitution `CLAUDE.md` §2.2 (no `dyn`), §2.4 (`@py_compat
  numerical(rtol)`), §4.2 (L0..L3), §5.1 (elegant), §5.3
  (efficient).
- NumPy reduction docs —
  https://numpy.org/doc/stable/reference/routines.statistics.html
  and https://numpy.org/doc/stable/reference/generated/numpy.sum.html.
- NumPy pairwise summation —
  https://numpy.org/doc/stable/release/1.9.0-notes.html#numpy-sum-uses-pairwise-summation.
- Upstream `ndarray` 0.16 `Zip` + `fold_axis` —
  https://docs.rs/ndarray/0.16/ndarray/struct.Zip.html.
