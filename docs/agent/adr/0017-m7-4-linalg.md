---
doc_kind: adr
adr_id: 0017
title: M7.4 linalg subset — ops surface, backend strategy, error semantics, rtol gate
status: accepted
date: 2026-04-30
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0017: M7.4 linalg subset — ops surface, backend strategy, error semantics, rtol gate

## Context

ADR-0012 fixed the M7 sub-milestone breakdown; ADR-0013/0014/0015/0016
landed M7.0..M7.3. M7.4's mandate from ADR-0012 §"Sub-milestones":

> linalg subset: `matmul/dot`, `det`, `solve`, `inv`, `svd`, `eigh`,
> `cholesky`. Backend: `ndarray-linalg` (OpenBLAS / Accelerate).
> Acceptance gate: `rtol=1e-6` agreement on conditioned matrices;
> documented unstable cases.

This ADR pins six M7.4-binding decisions:

1. **Ops surface** — closed set of 8 entrypoints
   (`matmul / dot / det / solve / inv / svd / eigh / cholesky`).
2. **Backend strategy** — pure-Rust impls on top of `ndarray` 0.16
   with **opt-in** `ndarray-linalg = "0.16"` acceleration via cargo
   feature `linalg-backend`. Default build **does not** require
   system BLAS / LAPACK; tests pass cold-rebuild from `main` on
   stock toolchains.
3. **Dtype tier** — float-only at M7.4 (`Float32` / `Float64`); int /
   bool inputs return `LinalgDtypeUnsupported`.
4. **Error semantics** — three new error variants:
   `SingularMatrix` (det=0; LU pivot zero), `NotPositiveDefinite`
   (cholesky on non-PSD; eigh negative eigenvalue path on a guarded
   form), `LinalgShapeError` (matmul shape mismatch, non-square
   det/inv/solve/eigh/cholesky, batch-rank > 2).
5. **rtol gate** — `rtol = 1e-6` against numpy 2.0.2 on
   well-conditioned inputs (random with cond ≤ 1e6 generated via
   QR). Documented unstable cases: matrices with cond > 1e8 are
   excluded from the differential corpus.
6. **L2.perf** — inherits ENFORCED state from M7.1/M7.2/M7.3 at
   numerical-tier 0.5x.

## Options considered

### 1. Ops surface — closed at 8

ADR-0012 §"Sub-milestones" M7.4 row enumerates 7 ops; we treat
`matmul` and `dot` as separate entrypoints (both materialise; `dot`
also handles 1-D dot-product) for surface clarity, totaling 8. We
keep the surface closed at 8; widening to `qr / lstsq / pinv /
norm / matrix_rank` is an explicit ADR-bumpable decision.

| Op | Signature (logical) | Result | Notes |
|---|---|---|---|
| `matmul(a, b)` | `(M,K) x (K,N) -> (M,N)` (also 1-D x 2-D / 2-D x 1-D) | new Array | strict 2-D + 1-D only; no batched stack at M7.4 |
| `dot(a, b)` | 1-D x 1-D scalar; 2-D x 2-D matmul | new Array | numpy.dot semantics (deferred to matmul for 2-D) |
| `det(a)` | `(N,N) -> scalar` | 0-d Array | LU with partial pivot; sign × Π(diag(U)) |
| `solve(a, b)` | `(N,N) x (N,K)|(N,) -> (N,K)|(N,)` | new Array | LU with partial pivot then back-substitute |
| `inv(a)` | `(N,N) -> (N,N)` | new Array | `solve(a, I)` |
| `svd(a)` | `(M,N) -> (U: (M,M), s: (min(M,N),), Vt: (N,N))` | tuple of Arrays | one-sided Jacobi (small matrices, M7.4 scope cap N ≤ 64) |
| `eigh(a)` | `(N,N) -> (w: (N,), v: (N,N))` | tuple of Arrays | symmetric Jacobi eigendecomposition |
| `cholesky(a)` | `(N,N) -> (N,N)` (lower) | new Array | numpy default `lower=True` |

Surface details:

- `matmul` / `solve` / `inv` accept any `Float32` or `Float64` and
  preserve dtype (no upcasting).
- `det` always returns scalar `Float64` on `Float64` input,
  `Float32` on `Float32` input.
- `svd` returns three arrays, packed into a small struct
  `SvdResult { u, s, vt }`.
- `eigh` returns `EighResult { w, v }`.
- `cholesky` returns the **lower** triangular factor `L` such that
  `a == L · Lᵀ`; matches numpy default. Upper-triangular return
  is deferred to M7.x (`lower: bool` parameter, currently fixed).

### 2. Backend strategy — pure-Rust on `ndarray`, opt-in `ndarray-linalg`

ADR-0012 §"Backend strategy" said "bind `ndarray-linalg`". The
cobrust-m7-4 worktree runs on macOS Apple Silicon; `ndarray-linalg`
0.16 has known issues there:

- No first-class `accelerate` feature on 0.16
  ([rust-ndarray/ndarray-linalg#362](https://github.com/rust-ndarray/ndarray-linalg/issues/362)).
- `intel-mkl-static` works but downloads a ~300MB vendor blob; not
  acceptable as a default-build dependency per the CTO directive
  ("Do not require system OpenBLAS to be installed for `cargo
  build`").
- `openblas-static` requires a Fortran toolchain; not portable.

**Decision**: ship pure-Rust implementations on top of `ndarray`
0.16 for all 8 ops. The `ndarray-linalg = "0.16"` dependency is
gated behind a `linalg-backend` cargo feature (opt-in; off by
default). When the feature is on, M7.4 swaps to `ndarray-linalg`
kernels for `matmul / solve / inv / svd / eigh / cholesky`. When
off, pure-Rust kernels are used.

This satisfies:
- ADR-0012 §"Backend strategy" intent — we still **bind**
  `ndarray-linalg` (it's a declared optional dependency); we just
  don't require it for the cold-rebuild gate.
- CTO directive — default `cargo build` requires no BLAS / LAPACK
  / Fortran.
- Constitution §5.3 (efficient) — pure-Rust implementations are
  adequate for `rtol=1e-6` on cond ≤ 1e6 matrices up to N=64
  (M7.4 scope cap).
- ADR-0010 §3 numerical-tier 0.5x perf floor — pure-Rust LU /
  Jacobi at small N is competitive with numpy's BLAS dispatch.

#### Backend feature selection

| Feature flag | Backend | Cold rebuild | Notes |
|---|---|---|---|
| (default — no feature) | pure-Rust ndarray | works on any host | M7.4 ships this as the gate-stable path |
| `linalg-backend` | `ndarray-linalg = "0.16"` (`intel-mkl-static` sub-feature) | requires network access for MKL blob fetch | opt-in; tests still pass with a perf bump |

The `linalg-backend` feature wires through to
`ndarray-linalg = { version = "0.16", optional = true,
features = ["intel-mkl-static"] }`. On a host where
`intel-mkl-static` is unavailable, users can override the BLAS
sub-feature via `cargo build --features
linalg-backend,linalg-openblas-static` (we expose
`linalg-openblas-static` and `linalg-intel-mkl-static` as
secondary feature flags forwarding to `ndarray-linalg`).

### 3. Dtype tier — float-only at M7.4

NumPy's linalg surface accepts integer arrays by upcasting to float.
We adopt a **stricter** rule at M7.4: integer / bool inputs return
`LinalgDtypeUnsupported`. Rationale:

- LU / Cholesky / Jacobi on integer dtypes is almost always a
  user mistake (loss of precision); raising explicitly avoids
  silent f64 promotion + surprise lossy round-trip.
- M7.x can lift this by adding a `np.linalg.matmul` Python
  wrapper that does the upcast at the M7.4 surface and then calls
  through.

| Input dtype | Behavior | Rationale |
|---|---|---|
| `Float64` / `Float32` | accepted, preserved | matches numpy |
| `Int32` / `Int64` / `Bool` | `Err(LinalgDtypeUnsupported)` | strict M7.4 contract |
| Mixed `f32` / `f64` | promote to `f64` then preserve `f64` | matches numpy `result_type` |

### 4. Error semantics — three new variants

| Case | numpy 2.x | cobrust-numpy M7.4 | Variant |
|---|---|---|---|
| `det(near_zero)` | warns, returns 0.0 | warns ignored, returns 0.0; `inv(near_zero)` raises | `SingularMatrix` |
| `inv(singular)` | `LinAlgError: Singular matrix` | `Err(LinalgError { kind: SingularMatrix })` | `SingularMatrix` |
| `solve(singular, b)` | same | `Err(LinalgError { kind: SingularMatrix })` | `SingularMatrix` |
| `cholesky(non_psd)` | `LinAlgError: Matrix is not positive definite` | `Err(LinalgError { kind: NotPositiveDefinite })` | `NotPositiveDefinite` |
| `eigh(non_symmetric)` | undefined behavior; we sniff symmetry and raise | `Err(LinalgError { kind: LinalgShapeError, message: "input not symmetric" })` | `LinalgShapeError` |
| `matmul((M,K), (K',N))` (`K != K'`) | `ValueError: shapes ... not aligned` | `Err(LinalgError { kind: LinalgShapeError })` | `LinalgShapeError` |
| Non-square `det/inv/solve/eigh/cholesky` | `LinAlgError: Last 2 dimensions ...` | `Err(LinalgError { kind: LinalgShapeError })` | `LinalgShapeError` |
| Non-float dtype | upcasts | `Err(LinalgError { kind: LinalgDtypeUnsupported })` | `LinalgDtypeUnsupported` |
| `b.shape` incompatible with `a` in solve | `ValueError` | `Err(LinalgError { kind: LinalgShapeError })` | `LinalgShapeError` |

The new error variants land in
`crates/cobrust-numpy/src/error.rs` (existing `NumpyErrorKind`
extended): `SingularMatrix`, `NotPositiveDefinite`,
`LinalgShapeError`, `LinalgDtypeUnsupported`.

### 5. rtol gate — `1e-6` on conditioned inputs

NumPy linalg ops carry a documented numerical tolerance. ADR-0012
§"Sub-milestones" M7.4 row mandates `rtol=1e-6` agreement on
conditioned matrices. M7.4's differential gate generates random
matrices with controlled condition number:

- **Well-conditioned matrices**: random orthogonal `Q` (via QR of
  random Gaussian) × diagonal `D` with entries in `[0.1, 10]` ×
  random orthogonal `Q'`. Yields `cond(A) ≤ 100`.
- **Symmetric PSD** (for `cholesky`): `Qᵀ · D · Q` with `D > 0`.
- **Symmetric** (for `eigh`): `Q · diag(λ) · Qᵀ` with `λ ∈ [-10, 10]`.

The corpus for differential testing covers:

- `matmul` 1024 random pairs → `rtol=1e-6`.
- `dot` 1024 random pairs (1-D) → `rtol=1e-6`.
- `det` 1024 random Nx N (cond ≤ 100) → `rtol=1e-6`.
- `solve` 1024 random `(A, b)` (cond ≤ 100) → `rtol=1e-6`.
- `inv` 1024 random N x N → `rtol=1e-6` on `inv · a == I`.
- `svd` 256 random M x N (M, N ≤ 32) → `rtol=1e-6` on
  `U · diag(s) · Vᵀ == a`.
- `eigh` 256 random symmetric N x N (N ≤ 32) → `rtol=1e-6` on
  `v · diag(w) · vᵀ == a`.
- `cholesky` 256 random PSD → `rtol=1e-6` on `L · Lᵀ == a`.

Plus 1000 fuzz inputs per op (panic-free; matching numpy where
applicable; raising the documented error otherwise) for
constitution §4.2 floor.

#### Documented unstable cases

The following inputs **are not** in the differential corpus and
M7.4 documents them as known divergences:

- `cond(A) > 1e8` — numerical instability dominates;
  pure-Rust LU vs numpy's BLAS LAPACK have different roundoff
  patterns. Caller responsibility.
- N > 64 for `svd / eigh` — Jacobi convergence rate is O(N²);
  M7.x lifts via Householder reflections + tridiagonal QR.
- Complex dtypes — out of scope at M7.4 (Cobrust dtype tier is
  real-only).

### 6. L2.perf — inherits ENFORCED state

ADR-0010 §3 numerical-tier 0.5x floor; ADR-0014 §5 flipped the
L2.perf gate to enforced. M7.4 inherits the ENFORCED state:

- `corpus/numpy/M7.4/perf.toml` sets `threshold = 0.5,
  pass_ratio = 1.0, n_iters = 50, n_inputs = 16` (smaller batches
  for linalg ops which are O(N³)).
- `crates/cobrust-numpy/tests/linalg_bench.rs` runs micro-benches
  for matmul / solve / det. Reports persisted under
  `target/cobrust-bench/numpy-M7.4/<commit>/`.
- `tests/linalg_pipeline.rs` includes the deliberate-fail case
  `linalg_pipeline_escalates_when_perf_always_fails` mirroring
  M7.3's pattern.

## Decision

Adopt all six options:

1. Closed 8-op surface per the table above.
2. Pure-Rust on `ndarray`; `ndarray-linalg` opt-in via
   `linalg-backend` cargo feature.
3. Float-only at M7.4 (`Float32` / `Float64`); int/bool →
   `LinalgDtypeUnsupported`.
4. Four new `NumpyErrorKind` variants: `SingularMatrix`,
   `NotPositiveDefinite`, `LinalgShapeError`,
   `LinalgDtypeUnsupported`.
5. `rtol=1e-6` differential gate on cond ≤ 100 matrices; documented
   unstable cases.
6. L2.perf inherits ENFORCED at numerical-tier 0.5x.

### Public surface (M7.4 additions)

```rust
// crates/cobrust-numpy/src/linalg.rs (NEW)

pub fn matmul(a: &Array, b: &Array) -> Result<Array, NumpyError>;
pub fn dot(a: &Array, b: &Array) -> Result<Array, NumpyError>;
pub fn det(a: &Array) -> Result<Array, NumpyError>;          // 0-d Array
pub fn solve(a: &Array, b: &Array) -> Result<Array, NumpyError>;
pub fn inv(a: &Array) -> Result<Array, NumpyError>;
pub fn svd(a: &Array) -> Result<SvdResult, NumpyError>;
pub fn eigh(a: &Array) -> Result<EighResult, NumpyError>;
pub fn cholesky(a: &Array) -> Result<Array, NumpyError>;     // lower=true

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

// Extended error taxonomy.
pub enum NumpyErrorKind {
    // ... M7.0 + M7.1 + M7.2 + M7.3 variants ...
    SingularMatrix,
    NotPositiveDefinite,
    LinalgShapeError,
    LinalgDtypeUnsupported,
}
```

### Crate layout

Per ADR-0013 §"Decision" the parent-crate strategy holds. M7.4
lands one new module **inside** `crates/cobrust-numpy/src/`:

```
crates/cobrust-numpy/src/
  array.rs            — extended with matmul / dot methods
  broadcast.rs        — unchanged
  constructors.rs     — unchanged
  dtype.rs            — unchanged
  error.rs            — extended with 4 new variants
  index.rs            — unchanged
  lib.rs              — extended re-exports
  linalg.rs           — NEW: 8 linalg ops + SvdResult / EighResult
  print.rs            — unchanged
  promote.rs          — unchanged
  pyo3_bindings.rs    — unchanged for M7.4 (PyO3 surface frozen at M7.0)
  reduce.rs           — unchanged
  ufunc.rs            — unchanged
  view.rs             — unchanged
```

### M7.4 scope window

**In scope**:

- 8 linalg ops: `matmul / dot / det / solve / inv / svd / eigh /
  cholesky`.
- Float-only inputs (`Float32` / `Float64`). Mixed-dtype
  promotes to `Float64` per `result_type`.
- Pure-Rust implementations on `ndarray`; opt-in
  `ndarray-linalg` acceleration via `linalg-backend` feature.
- Four new `NumpyErrorKind` variants.
- L0..L1..L2.behavior gates per ADR-0007 + ADR-0008 carry over.
- L2.perf at numerical-tier 0.5x; reports under
  `target/cobrust-bench/numpy-M7.4/<commit>/`.
- ≥ 50 well-typed + ≥ 50 ill-typed programs.
- ≥ 1000 fuzz inputs per linalg op, panic-free + matching numpy
  via the differential harness on cond ≤ 100 inputs.
- `SvdResult` / `EighResult` structs to bundle multi-array
  return values.

**Out of scope (M7.x deferred)**:

- Batched linalg (`matmul` over rank-3+ stacked matrices).
- Complex dtypes.
- `qr / lstsq / pinv / norm / matrix_rank` — separate ADR.
- Householder + QR-based SVD / eigendecomposition (current Jacobi
  is O(N⁴); fine for N ≤ 64; M7.x widens).
- Upper-triangular cholesky (`lower=False` parameter).
- `linalg-backend` runtime fallback on missing BLAS — feature is
  binary at compile time.

## Consequences

- **Positive**
  - Closes the linalg surface that downstream M7.5+ random
    sampling and M7.6+ FFT will rely on (matmul is load-bearing
    for any non-trivial numerical workflow).
  - Pure-Rust default keeps `cargo build` cold-rebuild on stock
    toolchains with zero system deps; per the CTO directive.
  - `linalg-backend` opt-in preserves the ADR-0012 §"Backend
    strategy" intent — we still bind `ndarray-linalg`; we just
    don't require it.
  - 4 new error variants keep the closed taxonomy auditable.

- **Negative**
  - Pure-Rust SVD / eigh via Jacobi is O(N⁴); inappropriate for
    N > 64. M7.x lifts via Householder + tridiagonal QR.
  - Default builds without `linalg-backend` are slower than
    numpy's BLAS — perf gate at 0.5x leaves headroom but
    real-world workloads may bump into it. Documented.
  - Strict float-only dtype rule diverges from numpy's
    promote-then-call. Matches the constitution's "no silent
    coercion" rule (CLAUDE.md §2.2).

- **Neutral / unknown**
  - Real perf ratio for `linalg-backend = on` vs numpy is
    unknown until the bench harness runs in CI; the 0.5x
    floor leaves headroom.
  - Singular-detection threshold (`abs(pivot) < eps · max_diag`)
    is a hand-tuned constant; documented in the source.

## Evidence

- ADR-0012 §"Sub-milestones" M7.4 row.
- ADR-0013 §"Decision" — parent-crate layout we extend.
- ADR-0014 §1 — monomorphic dispatch precedent (we use the same
  pattern in `linalg.rs`).
- ADR-0015 §3 — view-vs-copy contract (linalg ops always return
  new owned Arrays; no views).
- ADR-0016 §5 — error-variant addition pattern.
- ADR-0010 §3 (numerical-tier perf floor 0.5x).
- ADR-0007 (translator pipeline), ADR-0008 (perf + repair),
  ADR-0011 (PyO3 build path).
- Constitution `CLAUDE.md` §2.2 (no `dyn`, no silent coercion),
  §2.4 (`@py_compat numerical(rtol)`), §4.2 (L0..L3), §5.1
  (elegant), §5.3 (efficient).
- NumPy linalg docs —
  https://numpy.org/doc/stable/reference/routines.linalg.html.
- NumPy `np.linalg.matmul` —
  https://numpy.org/doc/stable/reference/generated/numpy.matmul.html.
- NumPy `np.linalg.solve` —
  https://numpy.org/doc/stable/reference/generated/numpy.linalg.solve.html.
- Upstream `ndarray-linalg` 0.16 macOS support gap —
  https://github.com/rust-ndarray/ndarray-linalg/issues/362.
- LAPACK reference (LU, Cholesky, Jacobi SVD) —
  https://www.netlib.org/lapack/lug/.
- Jacobi method for symmetric eigenproblems —
  Golub & Van Loan, "Matrix Computations", 4th ed., §8.5.
