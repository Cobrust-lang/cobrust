# corpus/numpy/M7.4/ — Linalg subset (matmul/dot/det/solve/inv/svd/eigh/cholesky)

M7.4 sub-milestone deliverable per ADR-0012 + ADR-0017. Lands the
linalg surface on top of M7.0 (foundation) + M7.1 (ufuncs) + M7.2
(indexing) + M7.3 (reductions).

## Scope window (M7.4 per ADR-0017)

In scope:

- 8 linalg ops: `matmul / dot / det / solve / inv / svd / eigh /
  cholesky`.
- Float-only inputs (`Float32` / `Float64`); int / bool dtypes
  return `LinalgDtypeUnsupported`.
- Pure-Rust implementations on `ndarray`; opt-in `ndarray-linalg`
  via the `linalg-backend` cargo feature (off by default per the
  CTO directive).
- Four new `NumpyErrorKind` variants: `SingularMatrix`,
  `NotPositiveDefinite`, `LinalgShapeError`,
  `LinalgDtypeUnsupported`.
- L0..L1..L2.behavior gates per ADR-0007 + ADR-0008 carry over.
- L2.perf at numerical-tier 0.5x (per ADR-0010 §3); reports under
  `target/cobrust-bench/numpy-M7.4/<commit>/`.
- ≥ 50 well-typed + ≥ 50 ill-typed programs.
- ≥ 1000 fuzz inputs per linalg op, panic-free + matching numpy
  on cond ≤ 100 inputs at `rtol=1e-6`.
- `SvdResult` / `EighResult` structs to bundle multi-array
  return values.

Out of scope (deferred to later sub-milestones):

- Batched linalg over rank-3+ stacked matrices.
- Complex dtypes.
- `qr / lstsq / pinv / norm / matrix_rank`.
- Householder + tridiagonal-QR SVD / eigendecomposition (current
  Jacobi is fine for N ≤ 64 only).
- Upper-triangular Cholesky (`lower=False`).

## Files

- `upstream/linalg_core.py` — pure-Python reference for the 8 ops
  + helpers; matches the cobrust-numpy semantics one-to-one.
- `harness/h_linalg.py` — JSON-stdin / JSON-stdout differential
  harness that drives upstream `numpy.linalg`.
- `spec.toml` — L0 spec for the 8 ops + 4 helpers.
- `canned_llm_responses.toml` — synthetic-LLM mode canned
  responses for each function, keyed by source-SHA.
- `perf.toml` — L2.perf threshold (0.5x, numerical tier).
- `UPSTREAM_VERSION` / `UPSTREAM_LICENSE` — pin to numpy 2.0.2 +
  BSD-3-Clause.

## Documented unstable cases

The following inputs are **not** in the differential corpus and
M7.4 documents them as known divergences:

- `cond(A) > 1e8` — numerical instability dominates; pure-Rust
  LU vs numpy's BLAS LAPACK have different roundoff patterns.
- N > 64 for `svd / eigh` — Jacobi convergence rate is O(N²);
  M7.x lifts via Householder + tridiagonal QR.
- Complex dtypes — out of scope at M7.4 (Cobrust dtype tier is
  real-only).
