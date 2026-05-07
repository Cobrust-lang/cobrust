---
doc_kind: adr
adr_id: 0018
title: M7.5 random — Generator type, PCG64 backend, seed semantics, distribution surface, KS-test acceptance gate
status: accepted
date: 2026-04-30
last_verified_commit: f10af13fc92ba7918f47b1f973a9f374d64c1f1b
supersedes: []
superseded_by: []
---

# ADR-0018: M7.5 random — Generator type, PCG64 backend, seed semantics, distribution surface, KS-test acceptance gate

## Context

ADR-0012 fixed the M7 sub-milestone breakdown; ADR-0013 / 0014 / 0015 /
0016 landed M7.0..M7.3. M7.5's mandate from ADR-0012 §"Sub-milestones":

> `np.random.Generator`: `default_rng`, `seed`, `integers`, `random`,
> `normal`, `uniform`, `choice`. Backend: `rand` + `rand_distr`.
> Acceptance gate: seed reproducibility (same seed → same stream
> across machines); KS-test agreement in distribution.

M7.5 is parallel-allowed with M7.4 (linalg) per ADR-0012 §"Sequencing
rules" — both build on M7.3 reductions. M7.4 lands ADR-0017; M7.5
lands ADR-0018.

This ADR pins five M7.5-binding decisions:

1. **Generator type** — closed `Generator` struct wrapping a single
   PRNG state machine (no `dyn`, per constitution §2.2).
2. **PRNG backend** — `rand_pcg::Pcg64` to match numpy's
   `np.random.default_rng()` algorithm family (PCG64). Seed
   reproducibility within Cobrust is bit-exact across machines (PCG64
   is a deterministic algebraic PRNG); bit-identical reproducibility
   against numpy's specific PCG64 stream is **not** a hard requirement
   because numpy's exact internal state layout is implementation
   detail.
3. **Seed semantics** — `Option<u64>`: `None` seeds from the OS; `Some(s)`
   produces a deterministic stream that is reproducible across runs of
   the same binary on any host architecture.
4. **Distribution surface** — closed seven-method set per ADR-0012:
   `default_rng`, `seed`, `integers`, `random`, `normal`, `uniform`,
   `choice`. Returns `Array` for shape-bearing samples, scalar
   primitives for 0-d shapes.
5. **Acceptance gate** — KS-test agreement against numpy 2.0.2 for
   continuous distributions (`normal`, `uniform`, `random`) at p > 0.01;
   chi-square / mean-bin agreement for discrete (`integers`, `choice`).
   Within Cobrust, two `cargo test` runs of the same seed produce
   identical streams (table-driven).

## Options considered

### 1. Generator type — newtype struct vs. enum vs. trait object

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| **Newtype struct over `Pcg64`** | constitution §2.2 (no `dyn`) clean; one PRNG family at a time; cheap clone via `Pcg64: Clone` | binding to one algorithm — switching to ChaCha later requires ADR | **Yes** |
| Tagged-union enum `Generator { Pcg64(Pcg64), ChaCha(ChaChaRng), … }` | Multi-algorithm support | YAGNI; numpy's default is PCG64 — pick one and document | No |
| `Box<dyn RngCore>` | Polymorphic | Constitution §2.2 forbids `dyn` as default | No |

**Pick**: newtype struct `Generator { rng: Pcg64, has_seed: bool, seed:
Option<u64> }`. The seed field is preserved for diagnostics (numpy
does this too via `Generator.bit_generator.state`; we expose
`seed_value() -> Option<u64>`).

### 2. PRNG backend — `rand_pcg::Pcg64`

| Option | Algorithm | numpy default? | Cobrust selected? |
|---|---|---|---|
| **`rand_pcg::Pcg64`** | PCG64 (PCG-XSL-RR-128/64) | Yes | **Yes** |
| `rand_chacha::ChaCha20Rng` | ChaCha20 | No (numpy uses PCG64 by default) | No |
| `rand::rngs::StdRng` | ChaCha12 (current rand 0.8 default) | No | No |
| `rand::rngs::ThreadRng` | unspecified, non-reproducible | No | No |

**Pick**: `rand_pcg::Pcg64`. Reasoning:

- Matches numpy's algorithm family (`np.random.default_rng()` returns
  a `Generator` backed by `BitGenerator(PCG64)`).
- `rand_pcg = "0.3"` is MIT-OR-Apache-2.0 (license-compatible per
  ADR-0001).
- Deterministic across hosts: PCG64's transition function is
  algebraic (no host endianness or floating-point in the state),
  so a `u64` seed produces an identical `u64` stream on aarch64,
  x86_64, and any future architecture.
- Cheap to construct + clone (16-byte state).

**Note on bit-exactness vs numpy**: numpy's PCG64 uses a specific
seed-spreading scheme (SeedSequence) and 128-bit state layout; even
if our underlying PCG64 algorithm matches, the **stream from a
given seed `s` is not byte-identical between cobrust-numpy and
numpy**. What we promise:

- Within Cobrust: same `u64` seed → same stream, every time, on every
  host.
- Vs numpy: distribution-level agreement (KS-test) at p > 0.01
  threshold for continuous distributions; mean / variance /
  goodness-of-fit for discrete.

This trade-off is documented in the M7.5 known-divergence list in
`PROVENANCE.toml`.

### 3. Seed semantics — `Option<u64>` parameter

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| **`default_rng(seed: Option<u64>)`** | minimal closed surface; `None` means "OS-seed"; `Some` is deterministic | only `u64` seeds — numpy accepts arrays / SeedSequence too | **Yes** |
| `default_rng(seed: SeedSpec)` enum w/ `None`, `U64(u64)`, `Vec(Vec<u64>)`, `String(String)` | matches numpy's full surface | YAGNI for M7.5 | No |
| Two functions: `default_rng()` + `default_rng_seeded(u64)` | explicit | doubles surface for marginal clarity | No |

**Pick**: `default_rng(seed: Option<u64>) -> Generator`. Multi-word /
SeedSequence seeds are deferred to M7.x. `Generator::seed(&mut self,
seed: u64)` re-seeds in place — matches numpy's `gen.bit_generator.seed(s)`
(but simpler).

### 4. Distribution surface — closed seven-method set

Per ADR-0012 §"Sub-milestones" M7.5 row:

| Method | Signature | Returns | Distribution / behavior |
|---|---|---|---|
| `default_rng(seed)` | `(Option<u64>) -> Generator` | `Generator` | Construct (free function) |
| `Generator::seed` | `(&mut self, seed: u64)` | `()` | Re-seed in place |
| `Generator::integers` | `(&mut self, low: i64, high: i64, size: &[usize])` | `Array(Int64)` | uniform integers in `[low, high)` |
| `Generator::random` | `(&mut self, size: &[usize])` | `Array(Float64)` | uniform floats in `[0, 1)` |
| `Generator::normal` | `(&mut self, loc: f64, scale: f64, size: &[usize])` | `Array(Float64)` | Gaussian `N(loc, scale²)` |
| `Generator::uniform` | `(&mut self, low: f64, high: f64, size: &[usize])` | `Array(Float64)` | uniform floats in `[low, high)` |
| `Generator::choice` | `(&mut self, values: &Array, size: &[usize], replace: bool, p: Option<&[f64]>)` | `Array` (matches input dtype) | uniform / weighted selection from `values` |

**Surface closure**: deliberate. M7.x widening (e.g., `binomial`,
`poisson`, `exponential`, `gamma`, `beta`, `dirichlet`,
`multivariate_normal`) is an ADR-bumpable decision. Same closure
discipline as M7.0 dtype tier and M7.3 reduction set.

**Shape semantics**: `size = []` returns a 0-d Array (scalar in numpy
parlance — but in Cobrust, all M7.5 returns are Arrays for closure).
`size = [n]` returns 1-D. `size = [r, c]` returns 2-D, etc. Matches
numpy's `Generator.normal(size=...)` argument.

**`integers` half-open**: `[low, high)`. Matches numpy 2.x default
(`endpoint=False`). The optional `endpoint=True` flag is deferred
to M7.x.

**`choice` constraints**:
- `replace=true`: standard sampling-with-replacement.
- `replace=false`: requires `size.product() <= values.size()`;
  uses Fisher-Yates partial shuffle.
- `p`: optional probability vector; must sum to 1.0 within
  `1e-8` and have length == `values.size()`. Otherwise
  `RandomError::InvalidProbabilities`.

### 5. KS-test acceptance gate

| Option | Threshold | Selected? |
|---|---|---|
| **KS-test, p > 0.01, n=10000 samples** | continuous distributions | **Yes** |
| KS-test, p > 0.05 (more permissive) | continuous | No (looser than scientific norm) |
| KS-test, p > 0.001 (stricter) | continuous | No (false negatives at 10⁴ samples) |
| Mean / std bound (for normal) | parametric | No (insufficient — tests parameters, not distribution shape) |

**Pick**: 2-sample KS-test against numpy 2.0.2 with the same parameters
& seed family, at significance level α = 0.01 (i.e., we accept any
p-value > 0.01 as "indistinguishable from numpy"). For discrete
distributions (`integers`, `choice`):

- **Mean within ±2σ** of expected mean.
- **Empirical CDF χ²** test at α = 0.01.

The KS-test is implemented in pure Rust inside the test harness
(`tests/random_differential.rs`) using the standard Kolmogorov
statistic; we don't depend on `statrs` for one function. ≥ 10000
samples per distribution per gate (per ADR-0007 + ADR-0016 fuzz
budget).

## Decision

Adopt all five options:

1. Closed `Generator` newtype struct over `rand_pcg::Pcg64`.
2. `rand_pcg::Pcg64` as the PRNG backend; document non-bit-identical
   stream vs numpy.
3. `Option<u64>` seed parameter; `default_rng(None)` OS-seeds,
   `default_rng(Some(s))` is deterministic.
4. Closed seven-method distribution surface per the table above.
5. KS-test at p > 0.01 against numpy 2.0.2 for continuous; mean-bin
   χ² at α = 0.01 for discrete.

### Public surface (M7.5 additions)

```rust
// crates/cobrust-numpy/src/random.rs (NEW)

/// Random number generator state. Wraps `rand_pcg::Pcg64` (matches
/// numpy's `default_rng()` algorithm family).
///
/// Per ADR-0018 §1: closed newtype, no `dyn` (constitution §2.2).
/// Same seed → identical stream across runs of the same binary on
/// any host architecture (PCG64 is algebraic).
pub struct Generator {
    rng: rand_pcg::Pcg64,
    seed_value: Option<u64>,
}

impl Generator {
    pub fn seed(&mut self, seed: u64);
    pub fn seed_value(&self) -> Option<u64>;
    pub fn integers(&mut self, low: i64, high: i64, size: &[usize])
        -> Result<Array, NumpyError>;
    pub fn random(&mut self, size: &[usize])
        -> Result<Array, NumpyError>;
    pub fn normal(&mut self, loc: f64, scale: f64, size: &[usize])
        -> Result<Array, NumpyError>;
    pub fn uniform(&mut self, low: f64, high: f64, size: &[usize])
        -> Result<Array, NumpyError>;
    pub fn choice(&mut self, values: &Array, size: &[usize],
                  replace: bool, p: Option<&[f64]>)
        -> Result<Array, NumpyError>;
}

/// Construct a `Generator` from an optional seed. `None` seeds from
/// the OS; `Some(s)` produces a deterministic stream.
pub fn default_rng(seed: Option<u64>) -> Generator;

// New error variants (per ADR-0018 §"Error variants").
pub enum NumpyErrorKind {
    // ... M7.0..M7.3 variants ...
    /// `integers(low, high, ...)` with `low >= high` (numpy:
    /// `ValueError: low >= high`).
    InvalidIntegerRange,
    /// `uniform(low, high, ...)` with `low >= high` or non-finite
    /// bounds; or `normal(loc, scale, ...)` with `scale <= 0` /
    /// non-finite. Matches numpy's `ValueError` for these cases.
    InvalidDistributionParams,
    /// `choice(p=...)` with `p` not summing to 1, length mismatch,
    /// negative entries, or `replace=false` requesting more samples
    /// than `values` has. Matches numpy's `ValueError`.
    InvalidProbabilities,
    /// `choice(values, ...)` with `values` of zero size. Matches
    /// numpy's `ValueError: a must be non-empty`.
    EmptyChoicePopulation,
}
```

### Error variants (M7.5 additions)

Four new `NumpyErrorKind` variants. Per the cross-milestone conflict
note in the dispatch prompt, M7.4 P9 lands its `LinalgError` variants
first (e.g., `SingularMatrix`, `LinalgShapeMismatch`, …) immediately
after `ReductionEmptyArray`. M7.5 P9 (this branch) appends its
variants **after** M7.4's. If a merge conflict occurs on the
`NumpyErrorKind` enum, CTO arbitrates. The four variant names above
(`InvalidIntegerRange`, `InvalidDistributionParams`,
`InvalidProbabilities`, `EmptyChoicePopulation`) are chosen to not
collide with any plausible M7.4 linalg names.

### Crate layout

Per ADR-0013 §"Decision" the parent-crate strategy holds. M7.5 lands
one new module **inside** `crates/cobrust-numpy/src/`:

```
crates/cobrust-numpy/src/
  array.rs            — unchanged at M7.5 (Generator is free-standing)
  broadcast.rs        — unchanged
  constructors.rs     — unchanged
  dtype.rs            — unchanged
  error.rs            — extended with 4 new variants (per §"Error variants")
  index.rs            — unchanged
  lib.rs              — extended re-exports (Generator, default_rng)
  print.rs            — unchanged
  promote.rs          — unchanged
  pyo3_bindings.rs    — unchanged for M7.5 (PyO3 surface frozen at M7.0)
  random.rs           — NEW: Generator + 5 distribution methods
  reduce.rs           — unchanged
  ufunc.rs            — unchanged
  view.rs             — unchanged
```

Cargo.toml grows three new deps:

```toml
[dependencies]
rand = "0.8"
rand_pcg = "0.3"
rand_distr = "0.4"
```

All three are MIT-OR-Apache-2.0 (license-compatible per ADR-0001).

### M7.5 scope window

**In scope**:

- 7 distributions: `default_rng`, `seed`, `integers`, `random`,
  `normal`, `uniform`, `choice`.
- `Generator` struct over `rand_pcg::Pcg64`.
- `Option<u64>` seed; deterministic stream within Cobrust.
- 4 new `NumpyErrorKind` variants: `InvalidIntegerRange`,
  `InvalidDistributionParams`, `InvalidProbabilities`,
  `EmptyChoicePopulation`.
- L0..L1..L2.behavior gates per ADR-0007 + ADR-0008.
- L2.perf at numerical-tier 0.5x (per ADR-0010 §3); reports under
  `target/cobrust-bench/numpy-M7.5/<commit>/`. Bench-test pattern
  matches M7.1..M7.4.
- ≥ 50 well-typed + ≥ 50 ill-typed programs.
- ≥ 10000 samples per distribution against numpy 2.0.2 KS-test /
  goodness-of-fit; per-run seed-reproducibility table.
- Pairwise reproducibility test: same seed → identical stream
  across two `cargo test` runs of the same binary.
- Pipeline integration test (`tests/random_pipeline.rs`).

**Out of scope (M7.x deferred)**:

- Other distributions: `binomial`, `poisson`, `exponential`,
  `gamma`, `beta`, `dirichlet`, `multivariate_normal`,
  `multinomial`, `negative_binomial`, `chi_square`, `f`, `t`,
  `lognormal`, `pareto`, `triangular`, `weibull`, `geometric`,
  `hypergeometric`, `noncentral_chi_square`, `vonmises`,
  `wald`, `zipf`. (numpy ships ~30+ distributions.)
- `permutation` / `shuffle` (in-place + out-of-place).
- `BitGenerator` polymorphism (PCG64 only at M7.5; ChaCha /
  Philox / SFC64 deferred).
- SeedSequence multi-seed initialisation.
- `Generator.bit_generator.state` round-trip (state save/load).
- Stream advancement (`.advance(n)` / `.jumped()`).
- `endpoint=True` for `integers`.
- Bit-identical reproducibility against numpy's PCG64 stream
  (different seed-spreading scheme).

## Consequences

- **Positive**
  - Closes the random surface that future numpy-using libraries
    expect (scikit-learn, pandas's `sample`, etc., though those land
    M7.6+).
  - PCG64 backend matches numpy's algorithm family — distribution
    statistics will agree within KS-test threshold.
  - Closed seven-method set is auditable; widening is an
    ADR-bumpable decision.
  - `Generator` newtype struct keeps `dyn` out of the public API.
  - Cobrust seed reproducibility is a first-class promise (algebraic
    PRNG → host-independent).

- **Negative**
  - Cobrust `Generator` does **not** produce a byte-identical stream
    vs numpy for the same seed. Users porting test fixtures that
    depend on specific numpy output values must regenerate
    fixtures with cobrust-numpy. Documented as a known divergence.
  - Bound to `rand` 0.8 + `rand_pcg` 0.3 + `rand_distr` 0.4. If
    upstream rand 0.9 brings breaking changes, we'll need an ADR.
  - KS-test in pure Rust is a small implementation tax (~50 LOC);
    accepted instead of pulling `statrs`.

- **Neutral / unknown**
  - Real perf ratio for `random.normal(1024)` vs numpy's SIMD-driven
    Box-Muller is unknown. The 0.5× floor leaves headroom; if
    perf fails, repair loop applies (matches M7.1+ pattern).
  - Multi-threaded use of `Generator` is not supported — it's
    `!Sync` by design (state mutation). Users wanting parallel
    streams can construct multiple `Generator`s with different
    seeds. Documented in the module spec.

## Evidence

- ADR-0012 §"Sub-milestones" M7.5 row + §"Backend strategy" table.
- ADR-0013 §"Decision" — parent-crate layout we extend.
- ADR-0016 §"Decision" — closed-set discipline precedent.
- ADR-0010 §3 — numerical-tier perf floor 0.5×.
- Constitution `CLAUDE.md` §2.2 (no `dyn`), §2.4 (`@py_compat`
  numerical), §4.2 (L0..L3 gates), §5.1 (elegant), §5.3 (efficient).
- NumPy random module —
  https://numpy.org/doc/stable/reference/random/generator.html.
- NumPy default_rng / PCG64 —
  https://numpy.org/doc/stable/reference/random/bit_generators/pcg64.html.
- Upstream `rand` 0.8 — https://crates.io/crates/rand (MIT/Apache-2.0).
- Upstream `rand_pcg` 0.3 — https://crates.io/crates/rand_pcg (MIT/Apache-2.0).
- Upstream `rand_distr` 0.4 — https://crates.io/crates/rand_distr (MIT/Apache-2.0).
- O'Neill, M.E. "PCG: A Family of Simple Fast Space-Efficient
  Statistically Good Algorithms for Random Number Generation" (2014).
- Kolmogorov-Smirnov test —
  https://en.wikipedia.org/wiki/Kolmogorov%E2%80%93Smirnov_test.
