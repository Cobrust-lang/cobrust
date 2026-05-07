# corpus/numpy/M7.2/ — Indexing (basic / advanced / boolean / np.where)

M7.2 sub-milestone deliverable per ADR-0012 + ADR-0015. Lands the
indexing surface (basic slicing, single-int, integer-array, boolean
mask, `np.where`) on top of M7.1's ufunc layer.

## Scope window (M7.2 per ADR-0015)

In scope:

- Basic slicing: `Array::slice(SliceSpec) -> ArrayView<'_>` — VIEW
  (does not copy). Negative bounds, negative step, clamped
  out-of-range bounds — all numpy-exact.
- Single-int indexing: covered by `Array::index_get(&[Index::Single(i)])`
  and the harness `single_index` reference. Negative-index aware;
  out-of-bounds → `OutOfBoundsIndex`.
- Integer-array indexing: `Array::take(&[i64]) -> Array` — COPY.
- Boolean mask: `Array::mask(&Array) -> Array` — COPY. Mask shape
  must match self.shape().
- `np_where(cond, x, y) -> Array` — COPY. Broadcasts per ADR-0014.
- `Index` enum + `SliceSpec` struct — closed taxonomy per ADR-0015.
- `ArrayView<'a>` + `ArrayViewMut<'a>` — closed enums (5 variants
  each), no `dyn`, lifetime-encoded ownership.
- 4 new error variants: `IndexError`, `OutOfBoundsIndex`,
  `BoolMaskShapeMismatch`, `IndexDtypeNotInteger`.

Out of scope (deferred to later sub-milestones):

- Ellipsis indexing (`a[...]`).
- Multi-axis tuple-of-mixed-kind indexing materialises (does not
  preserve mixed view+copy chain) — M7.x.
- Setitem (`a[1:3] = ...`) ergonomic API — M7.x (the surface
  `slice_mut` is shipped).
- One-arg `np.where(cond)` returning indices — M7.x.
- Out-parameter (`np.take(a, idx, out=b)`) — M7.x.

## Files

- `UPSTREAM_VERSION` / `UPSTREAM_LICENSE` — provenance.
- `spec.toml` — L0 spec; 8 entries (5 public + 3 helpers).
- `upstream/index_core.py` — pipeline-time pure-Python reference.
- `upstream_tests/` — vendored upstream pytest subset (placeholder; M7.2
  uses the differential harness as the primary L2.behavior gate).
- `harness/h_index.py` — L0 differential harness driver (subprocess
  CPython oracle). Same pattern as M7.0 / M7.1 / M6 msgpack.
- `canned_llm_responses.toml` — synthetic-LLM mode response table; 8
  entries with stub bodies (the production multi-file crate at
  `crates/cobrust-numpy/src/` is the gate-stable byte snapshot).
- `perf.toml` — L2.perf gate config; threshold = 0.5x (numerical tier
  per ADR-0010 §3 + ADR-0014 §5 + ADR-0015); inherits ENFORCED from
  M7.1.

## Pipeline behaviour

The synthetic translator pipeline drives this corpus end-to-end via
`crates/cobrust-numpy/tests/index_pipeline.rs`. Every entry in
`spec.toml` matches an entry in `canned_llm_responses.toml`; the
pipeline emits a flat-file Rust skeleton with stub bodies — the
production cobrust-numpy at `crates/cobrust-numpy/src/` is the
hand-curated byte snapshot.

## Differential gate

`crates/cobrust-numpy/tests/index_differential.rs` invokes
`harness/h_index.py` per request and bytewise-compares the upstream
numpy 2.0.2 result against `cobrust_numpy::Array::<op>(...).to_json()`
for ≥ 1000 fuzz inputs per indexing kind (basic slice, single int,
integer-array, boolean mask, np.where). Skipped with a clear
message when upstream numpy is unavailable.

## L2.perf gate

`crates/cobrust-numpy/tests/index_bench.rs` drives an in-process
timing harness against an upstream numpy oracle subprocess. Reports
persisted under `target/cobrust-bench/numpy-M7.2/<commit>/`.
Threshold: 0.5x (numerical tier per ADR-0010 §3 + ADR-0014 §5 +
ADR-0015). Failure triggers the M5+ repair loop. The pipeline
integration test `tests/index_pipeline.rs` includes a deliberate-fail
case (`PerfVerifier::Reject` exhausts repair → `EscalationExceeded`),
demonstrating the gate is wired (mirrors M6's
`msgpack_pipeline_escalates_when_perf_always_fails` and M7.1's
`ufunc_pipeline_escalates_when_perf_always_fails`).
