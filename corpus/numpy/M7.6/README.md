# corpus/numpy/M7.6/ — M7.6 expansion (FFT + poly + Complex dtype + reduction extensions)

M7.6 sub-milestone deliverable per ADR-0012 + ADR-0021. Collects
three deferral buckets from M7.0..M7.5 into one milestone.

## Scope window (M7.6 per ADR-0021)

Three buckets land together:

### Bucket A — FFT + polynomial

- 4 FFT ops: `fft / ifft / rfft / irfft` (1-D real and complex).
- 3 poly ops: `polyval / polyfit / poly`.
- `rustfft = "6"` backend (per ADR-0012 §"Backend strategy" + ADR-0021 §1).
- `polyfit` reuses M7.4's `solve` kernel (LU partial pivot) on the
  Vandermonde normal-equation matrix.

### Bucket B — Complex dtype

- `Dtype` enum widening 5 → 7 variants (`Complex64 / Complex128`).
- `Array` enum widening 5 → 7 variants.
- `result_type` extended NEP 50 table for complex (49 entries).
- Ufunc routing for complex (binary arithmetic, element-wise math, comparison errors).
- M7.4 `eigh` Hermitian path for complex inputs (`H == H^H`).
- 1 new error variant: `ComplexNotOrderable`.
- `num_complex = "0.4"` storage type.

### Bucket C — Reduction extensions

- 8 new reductions: `cumsum / cumprod / median / percentile / nansum / nanmean / nanmin / nanmax`.
- Tuple-axis reduction for 5 ops: `sum_axes / prod_axes / mean_axes / min_axes / max_axes`.
- 2 new error variants: `PercentileOutOfRange`, `EmptyAxisTuple`.

## Out of scope (deferred to M7.7+)

- N-D FFT (`fft2 / fftn / ifft2 / ifftn`).
- Polynomial Chebyshev / Legendre / Laguerre / Hermite bases.
- Complex `matmul / dot / det / solve / inv / svd / cholesky` (only
  `eigh` is widened at M7.6).
- `keepdims=True` parameter on cumsum / median / percentile / nan*.
- `out=` / `where=` / `dtype=` parameters.
- `nanstd / nanvar / nanargmin / nanargmax / nanpercentile / nanmedian`.

## Files

- `UPSTREAM_VERSION` / `UPSTREAM_LICENSE` — provenance.
- `spec.toml` — L0 spec (15 entries: 4 fft + 3 poly + 8 reductions).
- `upstream/m76_core.py` — pipeline-time pure-Python reference subset.
- `upstream_tests/` — placeholder; M7.6 uses the differential harness
  as the primary L2.behavior gate (mirrors M7.5).
- `harness/h_m76.py` — L0 differential harness driver (subprocess
  CPython oracle).
- `canned_llm_responses.toml` — synthetic-LLM mode response table.
- `perf.toml` — L2.perf gate config; threshold = 0.5x (numerical tier
  per ADR-0010 §3); inherits ENFORCED from M7.1..M7.5.

## Differential gate

`crates/cobrust-numpy/tests/{fft,poly,complex,reduce_extensions}_differential.rs`
invokes `harness/h_m76.py` per request and compares cobrust-numpy
output against upstream numpy 2.0.2:

- **Bit-identical** for `Int32 / Int64 / Bool` outputs.
- **`rtol = 1e-7`** for `Float32 / Float64` outputs.
- **`rtol = 1e-5`** for `Complex64 / Complex128` outputs (FFT
  round-trip accuracy bound; per ADR-0021 §12).

≥ 200 inputs per new op. Skipped with a clear message when upstream
numpy is unavailable.

## L2.perf gate

`crates/cobrust-numpy/tests/{fft,poly,complex,reduce_extensions}_bench.rs`
drives in-process timing harness against upstream numpy oracle
subprocess. Reports persisted under
`target/cobrust-bench/numpy-M7.6/<commit>/`. Threshold: 0.5x
(numerical tier per ADR-0010 §3 + ADR-0021 §"Inherits ENFORCED").
