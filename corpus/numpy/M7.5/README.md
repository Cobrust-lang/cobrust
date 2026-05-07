# corpus/numpy/M7.5/ — Random (default_rng/seed/integers/random/normal/uniform/choice)

M7.5 sub-milestone deliverable per ADR-0012 + ADR-0018. Lands the
random surface on top of M7.0 (foundation) + M7.1 (ufuncs) + M7.2
(indexing) + M7.3 (reductions). Parallel with M7.4 (linalg) per
ADR-0012 §"Sequencing rules".

## Scope window (M7.5 per ADR-0018)

In scope:

- 7 distributions: `default_rng`, `seed`, `integers`, `random`,
  `normal`, `uniform`, `choice`.
- `Generator` newtype struct over `rand_pcg::Pcg64` (matches numpy's
  default `default_rng()` algorithm family).
- `Option<u64>` seed parameter; `None` OS-seeds, `Some(s)` produces a
  deterministic stream reproducible across runs of the same binary
  on any host architecture.
- 4 new `NumpyErrorKind` variants: `InvalidIntegerRange`,
  `InvalidDistributionParams`, `InvalidProbabilities`,
  `EmptyChoicePopulation`.
- L0..L1..L2.behavior gates per ADR-0007 + ADR-0008 carry over.
- L2.perf at numerical-tier 0.5x (per ADR-0010 §3); reports under
  `target/cobrust-bench/numpy-M7.5/<commit>/`. Bench-test pattern
  matches M7.1..M7.4.
- ≥ 50 well-typed + ≥ 50 ill-typed programs.
- ≥ 10000 samples per distribution KS-test / goodness-of-fit vs numpy 2.0.2.
- Seed reproducibility: same seed → identical stream within Cobrust
  (table-driven across runs of the same binary).

Out of scope (deferred to later sub-milestones):

- Other distributions: `binomial`, `poisson`, `exponential`,
  `gamma`, `beta`, `dirichlet`, `multivariate_normal`,
  `multinomial`, `chi_square`, `f`, `t`, `lognormal`, `pareto`,
  `triangular`, `weibull`, `geometric`, `hypergeometric`,
  `vonmises`, `wald`, `zipf` — M7.x.
- `permutation` / `shuffle` — M7.x.
- `BitGenerator` polymorphism (only PCG64 at M7.5; ChaCha / Philox /
  SFC64 — M7.x).
- `SeedSequence` multi-seed initialisation — M7.x.
- `Generator.bit_generator.state` round-trip (state save/load) — M7.x.
- Stream advancement (`.advance(n)` / `.jumped()`) — M7.x.
- `endpoint=True` for `integers` — M7.x.
- Bit-identical reproducibility against numpy's PCG64 stream
  (numpy uses a different seed-spreading scheme; documented divergence).

## Files

- `UPSTREAM_VERSION` / `UPSTREAM_LICENSE` — provenance.
- `spec.toml` — L0 spec; 11 entries (7 public + 4 helpers).
- `upstream/random_core.py` — pipeline-time pure-Python reference.
- `upstream_tests/` — vendored upstream pytest subset (placeholder; M7.5
  uses the differential harness as the primary L2.behavior gate).
- `harness/h_random.py` — L0 differential harness driver (subprocess
  CPython oracle). Same pattern as M7.0..M7.3 / M6 msgpack.
- `canned_llm_responses.toml` — synthetic-LLM mode response table; 11
  entries with stub bodies (the production multi-file crate at
  `crates/cobrust-numpy/src/` is the gate-stable byte snapshot).
- `perf.toml` — L2.perf gate config; threshold = 0.5x (numerical tier
  per ADR-0010 §3 + ADR-0014 §5 + ADR-0018); inherits ENFORCED from
  M7.1..M7.3.

## Pipeline behaviour

The synthetic translator pipeline drives this corpus end-to-end via
`crates/cobrust-numpy/tests/random_pipeline.rs`. Every entry in
`spec.toml` matches an entry in `canned_llm_responses.toml`; the
pipeline emits a flat-file Rust skeleton with stub bodies — the
production cobrust-numpy at `crates/cobrust-numpy/src/` is the
hand-curated byte snapshot.

## Differential gate

`crates/cobrust-numpy/tests/random_differential.rs` invokes
`harness/h_random.py` per request and statistically compares the
upstream numpy 2.0.2 sample against `cobrust_numpy::random::Generator`
output:

- **`normal` / `uniform` / `random`**: 2-sample Kolmogorov-Smirnov test
  (p > 0.01); ≥ 10000 samples per side.
- **`integers`**: empirical CDF χ² test at α = 0.01; mean within ±2σ.
- **`choice`**: empirical frequency distribution within ±2σ of expected.

Skipped with a clear message when upstream numpy is unavailable.

**Note on bit-identical reproducibility**: cobrust-numpy's PCG64
stream is not byte-identical to numpy's PCG64 stream (numpy uses a
specific SeedSequence layout). Within Cobrust, same seed → identical
stream (table-driven test in `tests/random_seed_corpus.rs`).

## L2.perf gate

`crates/cobrust-numpy/tests/random_bench.rs` drives an in-process
timing harness against an upstream numpy oracle subprocess. Reports
persisted under `target/cobrust-bench/numpy-M7.5/<commit>/`.
Threshold: 0.5x (numerical tier per ADR-0010 §3 + ADR-0014 §5 +
ADR-0018). Failure triggers the M5+ repair loop. The pipeline
integration test `tests/random_pipeline.rs` includes a deliberate-fail
case (`PerfVerifier::Reject` exhausts repair → `EscalationExceeded`),
demonstrating the gate is wired (mirrors M6's
`msgpack_pipeline_escalates_when_perf_always_fails`, M7.1's
`ufunc_pipeline_escalates_when_perf_always_fails`, M7.2's
`index_pipeline_escalates_when_perf_always_fails`, and M7.3's
`reduce_pipeline_escalates_when_perf_always_fails`).
