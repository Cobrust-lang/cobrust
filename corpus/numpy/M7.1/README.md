# corpus/numpy/M7.1/ — Universal functions + broadcasting

M7.1 sub-milestone deliverable per ADR-0012 + ADR-0014. Lands the
ufunc surface (binary + comparison + unary math), broadcasting, and
NEP 50 type promotion on top of M7.0's ndarray foundation.

## Scope window (M7.1 per ADR-0014)

In scope:

- Binary ufuncs: `add / subtract / multiply / divide / power`.
- Comparison ufuncs: `eq / ne / lt / le / gt / ge` — all return
  `Dtype::Bool` regardless of input dtypes.
- Element-wise math: `sin / cos / exp / log / sqrt` — integer inputs
  promote to `Float64` first; float inputs preserve dtype.
- Broadcasting per numpy 2.x rules
  (https://numpy.org/doc/stable/user/basics.broadcasting.html).
- Type promotion per NumPy 2.x NEP 50
  (https://numpy.org/neps/nep-0050-scalar-promotion.html).
- Closes ADR-0013 follow-ups: tagged-union → monomorphic dispatch
  (#1); typed constructors (#2); L2.perf flip to enforced (#3);
  multi-D nested-list parsing (#4).

Out of scope (deferred to later sub-milestones):

- Indexing / slicing / views (M7.2).
- Reductions (`sum / mean / max / argmin / ...`) (M7.3).
- Linalg (`matmul / det / solve / ...`) (M7.4).
- Random (M7.5).
- Additional dtypes (`int8 / int16 / uint* / float16 / complex*`) —
  M7.x ADR-bumpable.
- Out-parameter ufuncs (`np.add(a, b, out=c)`) — M7.x.
- `where=` parameter on ufuncs — M7.x.

## Files

- `UPSTREAM_VERSION` / `UPSTREAM_LICENSE` — provenance.
- `spec.toml` — L0 spec; 12 entries (10 public ufuncs + 2 helpers).
- `upstream/ufunc_core.py` — pipeline-time pure-Python reference.
- `upstream_tests/` — vendored upstream pytest subset (placeholder; M7.1
  uses the differential harness as the primary L2.behavior gate).
- `harness/h_ufunc.py` — L0 differential harness driver (subprocess
  CPython oracle). Same pattern as M7.0 / M6 msgpack.
- `canned_llm_responses.toml` — synthetic-LLM mode response table; 12
  entries with stub bodies (the production multi-file crate at
  `crates/cobrust-numpy/src/` is the gate-stable byte snapshot).
- `perf.toml` — L2.perf gate config; threshold = 0.5x (numerical tier
  per ADR-0010 §3 + ADR-0014 §5); flipped to ENFORCED at M7.1.

## Pipeline behaviour

The synthetic translator pipeline drives this corpus end-to-end via
`crates/cobrust-numpy/tests/ufunc_pipeline.rs`. Every entry in
`spec.toml` matches an entry in `canned_llm_responses.toml`; the
pipeline emits a flat-file Rust skeleton with stub bodies — the
production cobrust-numpy at `crates/cobrust-numpy/src/` is the
hand-curated byte snapshot.

## Differential gate

`crates/cobrust-numpy/tests/ufunc_differential.rs` invokes
`harness/h_ufunc.py` per request and bytewise-compares the upstream
numpy 2.0.2 result against `cobrust_numpy::Array::<op>(...).to_json()`
for ≥ 1000 fuzz inputs per ufunc. Skipped with a clear message when
upstream numpy is unavailable.

## L2.perf gate

`crates/cobrust-numpy/tests/ufunc_bench.rs` drives criterion benches
against an upstream numpy oracle subprocess. Reports persisted under
`target/cobrust-bench/numpy-M7.1/<commit>/`. Threshold: 0.5x
(numerical tier per ADR-0010 §3 + ADR-0014 §5). Failure triggers
the M5+ repair loop. The pipeline integration test
`tests/ufunc_pipeline.rs` includes a deliberate-fail case
(`PerfVerifier::Reject` exhausts repair → `EscalationExceeded`),
demonstrating the gate is wired (mirrors M6's
`msgpack_pipeline_escalates_when_perf_always_fails`).
