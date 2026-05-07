# corpus/numpy/M7.3/ — Reductions (sum/prod/mean/std/var/min/max/argmin/argmax)

M7.3 sub-milestone deliverable per ADR-0012 + ADR-0016. Lands the
reduction surface on top of M7.0 (foundation) + M7.1 (ufuncs) + M7.2
(indexing).

## Scope window (M7.3 per ADR-0016)

In scope:

- 9 reductions: `sum / prod / mean / std / var / min / max / argmin /
  argmax`.
- `axis: Option<i64>` parameter — `None` reduces all axes, `Some(k)`
  reduces along axis `k` (negative-axis aware).
- `ddof: u32` for `std / var` (default 0).
- Pairwise summation for float `sum / mean / std / var` per ADR-0016
  §3 (chunk size 8; matches numpy's accuracy floor).
- Empty-array semantics: identity for `sum` (= 0) / `prod` (= 1);
  `NaN` for `mean / std / var`; `ReductionEmptyArray` error for
  `min / max / argmin / argmax`.
- New error variant: `ReductionEmptyArray`.
- L0..L1..L2.behavior gates per ADR-0007 + ADR-0008 carry over.
- L2.perf at numerical-tier 0.5x (per ADR-0010 §3); reports under
  `target/cobrust-bench/numpy-M7.3/<commit>/`. Bench-test pattern
  matches M7.1 / M7.2.
- ≥ 50 well-typed + ≥ 50 ill-typed programs.
- ≥ 1000 fuzz inputs per reduction, panic-free + matching numpy
  via the differential harness.

Out of scope (deferred to later sub-milestones):

- Tuple-axis reduction (`axis=(0, 2)`) — M7.x.
- `keepdims=True` — M7.x.
- `out=` parameter — M7.x.
- `where=` parameter (selective reduction) — M7.x.
- `cumsum / cumprod / median / percentile / nanmin / nanmax / nansum
  / nanmean` — M7.x.
- `dtype=` parameter (forced result dtype) — M7.x.

## Files

- `UPSTREAM_VERSION` / `UPSTREAM_LICENSE` — provenance.
- `spec.toml` — L0 spec; 12 entries (10 public reductions × all-axis
  variants + 2 helpers).
- `upstream/reduction_core.py` — pipeline-time pure-Python reference.
- `upstream_tests/` — vendored upstream pytest subset (placeholder; M7.3
  uses the differential harness as the primary L2.behavior gate).
- `harness/h_reduction.py` — L0 differential harness driver (subprocess
  CPython oracle). Same pattern as M7.0 / M7.1 / M7.2 / M6 msgpack.
- `canned_llm_responses.toml` — synthetic-LLM mode response table; 12
  entries with stub bodies (the production multi-file crate at
  `crates/cobrust-numpy/src/` is the gate-stable byte snapshot).
- `perf.toml` — L2.perf gate config; threshold = 0.5x (numerical tier
  per ADR-0010 §3 + ADR-0014 §5 + ADR-0016); inherits ENFORCED from
  M7.1/M7.2.

## Pipeline behaviour

The synthetic translator pipeline drives this corpus end-to-end via
`crates/cobrust-numpy/tests/reduce_pipeline.rs`. Every entry in
`spec.toml` matches an entry in `canned_llm_responses.toml`; the
pipeline emits a flat-file Rust skeleton with stub bodies — the
production cobrust-numpy at `crates/cobrust-numpy/src/` is the
hand-curated byte snapshot.

## Differential gate

`crates/cobrust-numpy/tests/reduce_differential.rs` invokes
`harness/h_reduction.py` per request and bytewise-compares the upstream
numpy 2.0.2 result against `cobrust_numpy::<reduce>(...).to_json()`
for ≥ 1000 fuzz inputs per reduction. Skipped with a clear
message when upstream numpy is unavailable.

## L2.perf gate

`crates/cobrust-numpy/tests/reduce_bench.rs` drives an in-process
timing harness against an upstream numpy oracle subprocess. Reports
persisted under `target/cobrust-bench/numpy-M7.3/<commit>/`.
Threshold: 0.5x (numerical tier per ADR-0010 §3 + ADR-0014 §5 +
ADR-0016). Failure triggers the M5+ repair loop. The pipeline
integration test `tests/reduce_pipeline.rs` includes a deliberate-fail
case (`PerfVerifier::Reject` exhausts repair → `EscalationExceeded`),
demonstrating the gate is wired (mirrors M6's
`msgpack_pipeline_escalates_when_perf_always_fails`, M7.1's
`ufunc_pipeline_escalates_when_perf_always_fails`, and M7.2's
`index_pipeline_escalates_when_perf_always_fails`).
