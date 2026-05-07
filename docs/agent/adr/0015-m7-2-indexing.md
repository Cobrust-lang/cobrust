---
doc_kind: adr
adr_id: 0015
title: M7.2 indexing — view/copy taxonomy, ArrayView ownership, IndexError, np.where
status: accepted
date: 2026-04-30
last_verified_commit: bcff3c3
supersedes: []
superseded_by: []
---

# ADR-0015: M7.2 indexing — view/copy taxonomy, ArrayView ownership, IndexError, np.where

## Context

ADR-0012 fixed the M7 sub-milestone breakdown; ADR-0013 landed M7.0
(ndarray foundation + closed dtype tier + tagged-union `Array`,
explicitly deferring views to **M7.2 indexing**); ADR-0014 landed M7.1
(ufuncs + broadcasting + NEP 50 type promotion). M7.2's mandate from
ADR-0012 §"Sub-milestones":

> Indexing: basic slicing (`a[1:3]`), integer-array indexing
> (`a[[0,2,5]]`), boolean masks (`a[a>0]`); `np.where`; views vs
> copies. Backend: `ndarray::ArrayView` / `ArrayViewMut`. Acceptance
> gate: view semantics preserved; differential corpus covers all
> indexing forms.

This ADR pins the M7.2-binding decisions across five axes:
**indexing-kind taxonomy** (basic slice / integer-array / boolean
mask / single int / `np.where`), **view ownership model** in
Cobrust's static type system (no `dyn`; lifetimes encoded at the
type level), **view-vs-copy rules** (when does cobrust-numpy return
a view vs an owned copy — match numpy's documented rules),
**bound-checking semantics** (panic vs error; reuse `Result<_,
NumpyError>` per constitution §2.2), and **negative-index +
slice-with-step handling** (numpy-exact).

## Options considered

### 1. Indexing-kind taxonomy — closed enum at the public API

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| **Closed `Index` enum** with five variants — `Single(i64)`, `Slice(SliceSpec)`, `IntArray(Vec<i64>)`, `BoolMask(Array)`, `NewAxis` | Constitution §2.2 (no `dyn`); explicit + auditable; matches numpy's documented kinds; pattern-matchable for view-vs-copy decision | Enum grows quadratically if numpy adds kinds (e.g. ellipsis, multi-axis tuples) — but those are M7.x deferred | **Yes** |
| Trait-object `Box<dyn IndexLike>` | Plug-in extensibility | Constitution §2.2 forbids `dyn` as default; would also break the const-generic dispatch path | No |
| `Vec<IndexElem>` per axis (numpy-style multi-axis tuple) | Matches numpy's `a[i, :, [0,2,5]]` shape directly | Multi-axis-tuple indexing is **deferred to M7.x** per the scope-window section below; M7.2 ships per-axis-by-default + a single multi-axis path through `index_get(spec)` taking `&[Index]` | No (M7.x) |

**Pick**: closed `Index` enum + a top-level `index_get(arr, indices:
&[Index]) -> Result<Array, NumpyError>` API that walks the slice
per-axis. The enum has five variants: `Single(i64)` (negative-index
aware), `Slice(SliceSpec)` (start/stop/step, all `Option<i64>`),
`IntArray(Vec<i64>)` (advanced indexing — always copies),
`BoolMask(Array)` (always copies; mask must be `Bool`-dtype), and
`NewAxis` (inserts a length-1 axis — useful for `a[:, np.newaxis]`).

`SliceSpec { start: Option<i64>, stop: Option<i64>, step: Option<i64>
}` mirrors Python's `slice(start, stop, step)` triple. `None`
values default to numpy's defaults (start=0 / stop=len / step=1 with
sign-aware reverse).

### 2. View ownership model — type-level lifetimes, no `dyn`

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| **Newtype `ArrayView<'a> { inner: ArrayViewKind<'a> }`** wrapping `ndarray::ArrayViewD<'a, T>` per-dtype as a closed enum | Constitution §2.2 (no `dyn`); lifetime parameter `'a` ties the view to the parent's borrow; ergonomic for callers (`let v = a.slice(...).unwrap(); v.shape()`) | Per-variant enum match on every observer (shape/ndim/size) — same tagged-union dispatch as `Array` already does for owned arrays | **Yes** |
| Trait-object `Box<dyn ArrayViewLike + 'a>` | Single concrete type for callers | Constitution §2.2 forbids `dyn`; lifetime erasure makes view-vs-copy semantics opaque | No |
| Generic `ArrayView<'a, T>` parametrised on element type | Zero-cost | Doesn't fit numpy's "an array view is just an array view, dtype is a runtime attribute" model | No |
| `Cow<Array>`-style copy-on-write | Hides the view-vs-copy distinction | numpy's contract is **explicit** about which ops produce views vs copies; hiding it betrays user expectations | No |

**Pick**: closed `ArrayView<'a>` enum (5 variants, one per dtype)
wrapping `ndarray::ArrayViewD<'a, T>` per arm. Mutable counterpart is
`ArrayViewMut<'a>` (5 variants, one per dtype) wrapping
`ndarray::ArrayViewMutD<'a, T>`. Both carry an explicit `'a`
lifetime bound to the parent `Array`'s borrow. **Constitution §2.2
satisfied**: every dispatch arm is on a closed enum variant.

The lifetime parameter encodes ownership at the type level:
`&'a Array → ArrayView<'a>` (immutable view) and `&'a mut Array →
ArrayViewMut<'a>` (mutable view). The Rust borrow-checker
enforces: while a `ArrayViewMut<'a>` is alive, no other reference
to the parent is allowed.

### 3. View-vs-copy rules — match numpy's documented contract

Numpy distinguishes two categories of indexing
(https://numpy.org/doc/stable/user/basics.indexing.html):

| Indexing kind | numpy 2.x | cobrust-numpy M7.2 |
|---|---|---|
| Basic slicing (`a[1:3]`, `a[::2]`, `a[:, 1]`) | **View** | **View** (`Array::slice(spec) -> ArrayView<'a>`) |
| Single integer (`a[0]`, `a[-1]`) | **View** (or scalar for 1D arrays in some cases) | **View** of an arr-with-one-fewer-axis (`Array::index_axis(axis, i) -> ArrayView<'a>`) |
| Integer-array indexing (`a[[0, 2, 5]]`) | **Copy** | **Copy** (`Array::take(indices) -> Result<Array, NumpyError>`) |
| Boolean-mask indexing (`a[a > 0]`) | **Copy** | **Copy** (`Array::mask(bools) -> Result<Array, NumpyError>`) |
| `np.where(cond, x, y)` | **Copy** (always materialises) | **Copy** (`np_where(cond, x, y) -> Result<Array, NumpyError>`) |

**Rationale**: numpy's documented contract is the entire user
expectation. Implementing a "we always copy" rule would silently
divergence from numpy on the hottest path (slicing — every numpy
user expects `a[1:3]` to share memory with `a`); implementing a "we
always view" rule would force boolean-mask indexing into a stride
encoding that would not exist (the mask's true positions are
generally non-contiguous → no stride representation possible). We
follow numpy.

### 4. Bound-checking semantics — `IndexError` via `Result<_, NumpyError>`

Numpy raises `IndexError` (Python exception) on out-of-bounds.
Constitution §2.2 sets `Result<T, E>` as the default error path; the
shape of failure is Cobrust-native (`Err(NumpyError { kind:
NumpyErrorKind::OutOfBoundsIndex, ... })`). The **outcome** matches
numpy (operation fails on out-of-bounds), the **shape** is Rust.

| Case | numpy 2.x | cobrust-numpy M7.2 |
|---|---|---|
| `a[len(a)]` | `IndexError` | `Err(NumpyError { kind: OutOfBoundsIndex, ... })` |
| `a[-len(a) - 1]` | `IndexError` | `Err(NumpyError { kind: OutOfBoundsIndex, ... })` |
| `a[[0, len(a), 5]]` | `IndexError` | `Err(NumpyError { kind: OutOfBoundsIndex, ... })` |
| `a[<bool mask of wrong shape>]` | `IndexError` | `Err(NumpyError { kind: BoolMaskShapeMismatch, ... })` |
| `a[<int-array indexing with non-integer dtype>]` | `IndexError` | `Err(NumpyError { kind: IndexDtypeNotInteger, ... })` |
| `np.where(<non-bool cond>, x, y)` | numpy: silently casts cond to bool | cobrust-numpy: same — silently cast `Bool`-or-other-dtype cond to `Bool` (matches numpy "truthy" interpretation; documented divergence-of-shape only) |
| `a.slice(<step=0>)` | `ValueError` | `Err(NumpyError { kind: ZeroStep, ... })` (reuses M7.0's variant) |

Three new error variants land: `IndexError` (umbrella for
multi-axis index errors not covered by more specific variants),
`OutOfBoundsIndex`, `BoolMaskShapeMismatch`. `IndexDtypeNotInteger`
is added too.

### 5. Negative indices + slice-with-step

| Case | numpy 2.x | cobrust-numpy M7.2 |
|---|---|---|
| `a[-1]` (single int) | last element | last element (normalise `i = len + i` if `i < 0`; error if still out of bounds) |
| `a[-3:-1]` (slice with negative bounds) | works; both bounds normalise | same |
| `a[::-1]` (reverse slice) | reversed view | reversed view (`step = -1`; ndarray's `Slice { start, end, step }` supports negative step) |
| `a[::2]` (step) | every other element | every other element |
| `a[::0]` (zero step) | `ValueError` | `Err(NumpyError { kind: ZeroStep })` |
| `a[5:1:1]` (empty slice from positive direction) | empty array | empty view (length 0) |
| `a[-100:100]` (out-of-range bounds, valid direction) | clamped to `[0, len)` | clamped (matches numpy — slices clamp, not error) |

Note: out-of-range single-int access is an **error**; out-of-range
**slice bounds** are clamped to the valid range (numpy convention,
preserved). `slice` returning an empty view is a feature, not a
bug.

## Decision

Adopt all five options:

1. Closed `Index` enum (5 variants: `Single`, `Slice(SliceSpec)`,
   `IntArray`, `BoolMask`, `NewAxis`) at the public API.
2. Closed `ArrayView<'a>` + `ArrayViewMut<'a>` enums (5 variants
   each) wrapping `ndarray::ArrayViewD<'a, T>` /
   `ArrayViewMutD<'a, T>`.
3. View-vs-copy rules match numpy's documented contract: basic slice
   + single int + `NewAxis` produce views; integer-array, boolean
   mask, and `np.where` produce copies.
4. Bound-checking via `Result<_, NumpyError>`; new variants
   `IndexError` (umbrella), `OutOfBoundsIndex`, `BoolMaskShapeMismatch`,
   `IndexDtypeNotInteger`.
5. Negative indices + slice-with-step: numpy-exact normalisation +
   clamp on bounds, error on zero-step.

### Public surface (M7.2 additions)

```rust
// crates/cobrust-numpy/src/index.rs (NEW)

/// Slice spec: numpy `slice(start, stop, step)` triple. `None`
/// values use numpy defaults (start=0, stop=len, step=1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SliceSpec {
    pub start: Option<i64>,
    pub stop: Option<i64>,
    pub step: Option<i64>,
}

impl SliceSpec {
    pub const fn full() -> Self;                           // [:]
    pub const fn from_start(start: i64) -> Self;           // [start:]
    pub const fn to_stop(stop: i64) -> Self;               // [:stop]
    pub const fn range(start: i64, stop: i64) -> Self;     // [start:stop]
    pub const fn stepped(start: i64, stop: i64, step: i64) -> Self; // [start:stop:step]
    pub const fn step_only(step: i64) -> Self;             // [::step]
}

/// Closed indexing-kind taxonomy per ADR-0015 §1.
#[derive(Clone, Debug, PartialEq)]
pub enum Index {
    Single(i64),                // a[i]; negative-index aware
    Slice(SliceSpec),           // a[start:stop:step]
    IntArray(Vec<i64>),         // a[[0, 2, 5]]; advanced — copies
    BoolMask(Array),            // a[a > 0]; advanced — copies
    NewAxis,                    // a[np.newaxis]; inserts length-1 axis
}

// Top-level
pub fn index_get(arr: &Array, indices: &[Index]) -> Result<Array, NumpyError>;
pub fn np_where(cond: &Array, x: &Array, y: &Array) -> Result<Array, NumpyError>;

// crates/cobrust-numpy/src/view.rs (NEW)

/// Immutable view per ADR-0015 §2. Lifetime `'a` is tied to the
/// parent `Array`'s borrow. No `dyn` per constitution §2.2.
pub enum ArrayView<'a> {
    Int32(ndarray::ArrayViewD<'a, i32>),
    Int64(ndarray::ArrayViewD<'a, i64>),
    Float32(ndarray::ArrayViewD<'a, f32>),
    Float64(ndarray::ArrayViewD<'a, f64>),
    Bool(ndarray::ArrayViewD<'a, bool>),
}

impl<'a> ArrayView<'a> {
    pub fn dtype(&self) -> Dtype;
    pub fn shape(&self) -> Vec<usize>;
    pub fn ndim(&self) -> usize;
    pub fn size(&self) -> usize;
    pub fn to_owned(&self) -> Array;        // materialise into an owned Array
}

pub enum ArrayViewMut<'a> {
    Int32(ndarray::ArrayViewMutD<'a, i32>),
    Int64(ndarray::ArrayViewMutD<'a, i64>),
    Float32(ndarray::ArrayViewMutD<'a, f32>),
    Float64(ndarray::ArrayViewMutD<'a, f64>),
    Bool(ndarray::ArrayViewMutD<'a, bool>),
}

impl<'a> ArrayViewMut<'a> {
    pub fn dtype(&self) -> Dtype;
    pub fn shape(&self) -> Vec<usize>;
    pub fn fill_f64(&mut self, v: f64);     // mutate-through-view test seed
}

// crates/cobrust-numpy/src/array.rs (extended)
impl Array {
    /// Basic slice. Returns a VIEW (does not copy).
    pub fn slice(&self, spec: SliceSpec) -> Result<ArrayView<'_>, NumpyError>;
    pub fn slice_mut(&mut self, spec: SliceSpec) -> Result<ArrayViewMut<'_>, NumpyError>;

    /// Integer-array indexing. Returns a COPY (always materialises).
    pub fn take(&self, indices: &[i64]) -> Result<Array, NumpyError>;

    /// Boolean-mask indexing. Returns a COPY. Mask shape must match self.shape().
    pub fn mask(&self, bools: &Array) -> Result<Array, NumpyError>;

    /// numpy-style multi-axis indexing. Top-level dispatch per Index
    /// kind; returns Array (always materialised — multi-axis cases
    /// where some axes are views and others copies are handled by
    /// materialising).
    pub fn index_get(&self, indices: &[Index]) -> Result<Array, NumpyError>;

    /// Convenience: where(self, x, y) — element-wise selection.
    pub fn where_(&self, x: &Array, y: &Array) -> Result<Array, NumpyError>;
}

// crates/cobrust-numpy/src/error.rs (extended)
pub enum NumpyErrorKind {
    // ... M7.0 + M7.1 variants ...
    // M7.2 additions:
    /// Umbrella for indexing errors not covered by more specific variants.
    IndexError,
    /// Single-int or int-array index out of `[-len, len)`.
    OutOfBoundsIndex,
    /// Boolean mask passed to `mask` has shape != self.shape().
    BoolMaskShapeMismatch,
    /// Index array passed to `take`/`IntArray` is not int-dtype.
    IndexDtypeNotInteger,
}
```

### Crate layout

Per ADR-0013 §"Decision" the parent-crate strategy holds. M7.2 lands
two new modules **inside** `crates/cobrust-numpy/src/`:

```
crates/cobrust-numpy/src/
  array.rs            — extended with slice / take / mask / index_get / where_ methods
  broadcast.rs        — unchanged
  constructors.rs     — unchanged
  dtype.rs            — unchanged
  error.rs            — extended with 4 new variants
  index.rs            — NEW: Index enum, SliceSpec, index_get, np_where
  lib.rs              — extended re-exports
  print.rs            — unchanged
  promote.rs          — unchanged
  pyo3_bindings.rs    — unchanged for M7.2 (PyO3 surface stays at M7.0; views are Rust-only)
  ufunc.rs            — unchanged
  view.rs             — NEW: ArrayView, ArrayViewMut
```

### M7.2 scope window

**In scope**:

- `Index` enum (5 variants) + `SliceSpec` struct.
- `ArrayView<'a>` + `ArrayViewMut<'a>` (5 variants each).
- `Array::slice` / `slice_mut` (basic slicing → view).
- `Array::take` (integer-array → copy).
- `Array::mask` (boolean mask → copy).
- `Array::index_get` (top-level multi-axis dispatcher → copy).
- `np_where(cond, x, y)` (broadcast cond/x/y, return copy).
- 4 new `NumpyErrorKind` variants.
- L0..L1..L2.behavior gates per ADR-0007 + ADR-0008 carry over.
- L2.perf at numerical-tier 0.5× (per ADR-0010 §3); reports under
  `target/cobrust-bench/numpy-M7.2/<commit>/`. Bench-test pattern
  matches M7.1 (in-process timing harness, escalation test wired).
- ≥ 50 well-typed + ≥ 50 ill-typed programs.
- ≥ 1000 fuzz inputs per indexing kind, panic-free + matching numpy
  via the differential harness.

**Out of scope (M7.x deferred)**:

- Ellipsis indexing (`a[...]`, `a[..., 0]`).
- Multi-axis tuple-of-mixed-kind indexing (`a[i, :, [0, 2, 5]]` —
  the per-axis path inside `index_get` materialises whenever any
  axis is advanced; pure multi-axis basic slicing still produces a
  view).
- Setitem (`a[1:3] = ...`) — `slice_mut` lands the surface but the
  ergonomic `__setitem__`-style API is M7.x.
- `np.where` with one argument (numpy: returns indices); M7.2 ships
  the three-arg form only.
- Out-parameter (`np.take(a, indices, out=b)`) — M7.x.
- Fancy indexing combined with broadcasting on the index arrays
  themselves — M7.x.

## Consequences

- **Positive**
  - Closes the indexing surface that downstream M7.3 reductions need
    (`sum(axis=k)` consumes axis-iteration, which is implementation
    of basic slicing on each axis).
  - View-vs-copy rules match numpy's contract — drop-in mental model
    for users.
  - Lifetime-encoded ownership satisfies constitution §2.2 (no
    `dyn`) and gives the Rust borrow checker the leverage to
    enforce mutate-through-view safety at compile time.
  - The `Index` enum is closed at 5 variants — adding ellipsis or
    fancy multi-axis combinations is an explicit ADR-bumpable
    decision later.

- **Negative**
  - Per-variant enum match on every view observer (shape/ndim/size)
    — same tagged-union dispatch cost as `Array`. M7.x may revisit
    if profile shows the dispatch dominating.
  - The `Index::BoolMask(Array)` variant introduces a circularity
    (mask is itself an `Array`); resolved by requiring `Array::Bool`
    dtype at runtime via an early dtype check.
  - `index_get` materialises for any multi-axis case where one axis
    is advanced — divergence from numpy's per-axis policy (which
    can return mixed view+copy chains). Documented as a known
    minor divergence; M7.x can refine.

- **Neutral / unknown**
  - The `NewAxis` variant is included for `np.newaxis` /
    `np.expand_dims` composition; full `expand_dims` integration
    lands at M7.3 reductions.
  - Real perf ratio for masked indexing vs numpy's SIMD path is
    unknown until the bench harness runs. The 0.5× floor leaves
    headroom; if cobrust-numpy underperforms, M7.x repair runs.

## Evidence

- ADR-0012 §"Sub-milestones" M7.2 row.
- ADR-0013 §"Consequences" §"Negative" — flagged views deferred to
  M7.2.
- ADR-0014 — M7.1 ufunc dispatch precedent (`for_each_dtype!`-style
  inline matches).
- ADR-0010 §3 (numerical-tier perf floor 0.5×).
- ADR-0007 (translator pipeline), ADR-0008 (perf + repair),
  ADR-0011 (PyO3 build path).
- Constitution `CLAUDE.md` §2.2 (no `dyn`), §2.4 (`@py_compat
  numerical(rtol)`), §4.2 (L0..L3), §5.1 (elegant), §5.3
  (efficient).
- NumPy indexing docs —
  https://numpy.org/doc/stable/user/basics.indexing.html.
- NumPy view-vs-copy semantics —
  https://numpy.org/doc/stable/user/basics.copies.html.
- Upstream `ndarray` 0.16 SliceInfo / slice_each_axis / ArrayViewD
  — https://docs.rs/ndarray/0.16.
