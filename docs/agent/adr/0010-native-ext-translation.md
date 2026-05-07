---
doc_kind: adr
adr_id: 0010
title: Native-extension translation methodology — msgpack-python, Cython sources, perf threshold relaxation, perf-gate fail-on-threshold-miss routing, downstream widening
status: accepted
date: 2026-04-30
last_verified_commit: 908f67c
supersedes: []
superseded_by: []
---

# ADR-0010: Native-extension translation methodology — msgpack-python, Cython sources, perf threshold relaxation, perf-gate fail-on-threshold-miss routing, downstream widening

## Context

Constitution §7 fixes M6 as "First library with native extension translated
(`orjson` or `msgpack`). Done means: demonstrates non-pure-Python
translation viability." The constitution leaves three concrete questions
to an ADR before any code lands:

1. **Library choice** — `orjson` vs `msgpack`. orjson's upstream is already
   pure-Rust internally; translating it does not exercise the
   non-pure-Python path the milestone exists to prove. msgpack-python ships
   both a pure-Python `fallback.py` and a Cython `_packer.pyx` /
   `_unpacker.pyx` accelerator, which is exactly the mixed-source shape
   real-world Python ecosystem libraries take (numpy, scipy, pandas,
   pendulum, lxml, cryptography, …). Picking msgpack makes M6 the
   load-bearing milestone the constitution describes.

2. **Methodology** — how do we translate a library that has both a
   pure-Python file and Cython annotated sources? The translator's M4/M5
   contract is "function-level, bottom-up, dependency-graph-ordered" — but
   Cython introduces a new dimension: typed annotations (`cdef int`,
   `cdef object`, `cdef inline`) must map to Cobrust types, and the
   pure-Python and Cython sources must produce **byte-identical output**
   for any input (msgpack-python's contract is `fallback.Packer().pack(x)
   == cmsgpack.Packer().pack(x)` for every supported input).

3. **Perf threshold** — the constitution §4.2 default is "≥ 0.8× of
   original on representative benchmarks (configurable per library)". For
   pure-Rust replacing pure-Python (M4 tomli, M5 dateutil), 0.8× is
   trivial — Rust beats CPython by 5–50× on parser workloads. For
   pure-Rust replacing **hand-tuned C** (msgpack's `_packer.c` after
   Cython compiles), the gap closes dramatically: a naïve Rust port
   competing against a 10-year-tuned C accelerator is unlikely to clear
   0.8× without months of micro-optimisation. M6 needs a per-library
   relaxation policy with a documented floor.

4. **Perf-gate failure routing** — M5 (ADR-0008) added `PerfTarget`
   thresholds and a `BenchmarkReport` writer, but the pipeline does not
   yet raise `TranslatorError::PerfGate` on threshold miss (the variant
   exists but is unused; the bench harness emits a JSON report and tests
   pass on threshold). M6 must close the loop: a sub-threshold perf
   number → repair route → re-translation → re-bench within retry budget.

5. **Downstream widening** — ADR-0009 §3 promised pandas + sqlalchemy +
   pendulum at M6 for dateutil's L3. M6 closes that loop with vendored
   test subsets (5–10 tests per dependent) so dateutil's L3 hits 5/5.

## Options considered

### 1. Library choice (constitution wording: "orjson or msgpack")

| Option | Pros | Cons | Selected? |
|---|---|---|---|
| **orjson** | Single-source Rust; popular | Upstream already in Rust; doesn't prove non-pure-Python translation; the milestone bar would be "translate Rust to Cobrust", not "translate native ext to Cobrust" | No |
| **msgpack-python** | Pure-Py + Cython mixed sources; widely depended on (pyspark, redis-py, msgpack-numpy); byte-identical output is a tight, falsifiable contract | Cython annotations require new translator support; subset of the spec is large enough to need scoping | **Yes** |
| Defer to M7 | — | Violates §7 M6 description | No |

We pick **msgpack-python** with a deliberately scoped subset:
- Public surface: `pack(obj) -> bytes`, `unpack(bytes) -> obj`,
  `Packer`, `Unpacker`.
- In-scope value types: `nil`, `bool`, signed integers within
  i64 range, floats (f32 + f64), str (utf-8), binary (bytes),
  fixed-size arrays, fixed-size maps. Out-of-scope (M7+):
  ext types, timestamp ext, streaming partial reads, raw=False
  legacy mode, `default=` callbacks.
- Source files vendored under `corpus/msgpack/upstream/`:
  - `fallback.py` — pure-Python encoder/decoder
  - `_packer.pyx` — Cython packer (annotated)
  - `_unpacker.pyx` — Cython unpacker (annotated)
  - `exceptions.py` — error types

### 2. Methodology — pure-Py first, then Cython

Two-stage translation:

1. **Stage A — translate `fallback.py`**. The pure-Python form is the
   reference oracle; it produces canonical bytes for every input.
   Translation is a normal M4/M5 function-level dispatch through the
   synthetic provider; canned responses are recorded in
   `corpus/msgpack/canned_llm_responses.toml` keyed by
   `(task=translate, function=<fn>, attempt=1)`.

2. **Stage B — translate `_packer.pyx` / `_unpacker.pyx`**. The Cython
   sources are richer: they carry type annotations the L1 translator
   uses to emit better Rust signatures (`cdef int n` → `n: i32`,
   `cdef object obj` → `obj: serde_json::Value`, `cdef inline` →
   `#[inline]`). The Cython prompt template ships type-mapping
   instructions. Crucially, the **emitted Rust function bodies for
   Stage A and Stage B converge** — the public-surface byte output
   must be identical. The fuzz gate (`tests/msgpack_fuzz.rs`) asserts
   this convergence on ≥ 1000 random inputs per public function.

The Cython AST shim (`crates/cobrust-translator/src/cython.rs`) is a
**lexical** parser, not a full Cython front-end:
- Recognises `cdef <type> <name>` declarations and maps types via a
  small table:
  - `int`, `long`, `Py_ssize_t` → `i64`
  - `unsigned int`, `unsigned long` → `u64`
  - `bint` → `bool`
  - `float` → `f64`
  - `double` → `f64`
  - `str`, `bytes`, `unicode` → `&str` (read) / `String` (owned)
  - `object` → `serde_json::Value` (the M6 dynamic-payload escape)
  - `list` → `Vec<serde_json::Value>`
  - `dict` → `serde_json::Map<String, serde_json::Value>`
- Recognises `cdef inline` → `#[inline]`.
- Recognises `cpdef` (Python+C wrapper) → emits `pub fn` (we do not
  generate a Python-callable wrapper at the cython.rs layer; the PyO3
  wrapper layer is responsible for that).
- Skips `cimport` lines (replaced by Rust `use` statements via the
  prompt template).

Emitted code goes through the same `run_l1` → repair-loop pipeline
M5 already supports; only the prompt-template selection differs (key:
`task = translate_cython` vs `task = translate`). The `task` is
threaded through the synthetic provider's lookup key so canned tables
can carry both stages without ambiguity.

**Why a lexical shim and not a real Cython parser**: M6's deliverable
is "native-ext translation viable", not "production-grade Cython
front-end". The shim handles 100% of the msgpack `_packer/_unpacker`
constructs and gives us a clean place to extend at M7+. A full Cython
AST would balloon the M6 surface by ~3×.

### 3. Perf threshold — per-library relaxation with documented floor

ADR-0008 §2 already supports per-library `corpus/<lib>/perf.toml` with
a `threshold` knob. M6 introduces a **documented relaxation tier**:

| Library tier | Default threshold | Rationale |
|---|---|---|
| Pure-Python upstream (tomli, dateutil core) | **0.8** | constitution §4.2 default — Rust easily beats CPython parsers |
| Native-ext upstream (msgpack, future numpy core) | **0.7** | Pure Rust against hand-tuned C is harder; 0.7 leaves room for first-pass translations to land green and the repair loop to chase the gap on follow-up sprints |
| Heavily-vectorised C/Fortran (numpy, scipy on M7+) | **0.5** (upper bound) | These libraries expose intrinsics (AVX, NEON, BLAS calls) the pure-Rust translation does not have; dropping below 0.5 should be ADR-justified per library |

For M6, msgpack's `corpus/msgpack/perf.toml` ships
`threshold = 0.7, pass_ratio = 1.0` (every public function must clear
0.7×). This is a **falsifiable** number — if the M6 translation can't
clear 0.7×, the perf gate fails and the function is repaired. We
deliberately do not relax `pass_ratio` below 1.0 for msgpack — the
public surface is small enough that "every function clears the bar"
is the right target.

### 4. Perf-gate failure routing

M5 left `TranslatorError::PerfGate(String)` as a defined-but-unfired
error. M6 wires it:

```
   L0 → L1 → L2.behavior → L2.perf
                              │ pass_ratio met
                              ▼
                          L3.pyo3 → L3.downstream → manifest
                              │ pass_ratio not met
                              ▼
                       diagnostic GateFailure { failed_gate: "l2_perf",
                                                failed_inputs: per-fn
                                                ratio table,
                                                expected: ">= threshold",
                                                actual: actual ratio }
                              │
                              ▼
                          repair loop (attempt += 1)
```

The pipeline detects perf failure by:
1. Running the bench harness after L2.behavior accepts the emission.
2. Loading `BenchmarkReport::meets_pass_ratio()`.
3. If `false`, build a `GateFailure` blob enumerating the
   sub-threshold functions and dispatch back through
   `repair_translation`.
4. The synthetic provider serves the per-attempt entry; the canned
   table for msgpack carries an attempt-1 perf-broken entry + an
   attempt-2 corrected entry for one function (`pack_uint`) so the
   M6 integration test can assert `repair_attempts >= 1` without
   relying on real LLMs.
5. Escalation threshold (constitution §4.2 default 50) applies.

Failure-routing surface (extends `BehaviorVerifier` semantics):
- The bench harness must run **after** behaviour-gate pass to avoid
  confusing perf failure with a behaviour bug.
- The `BehaviorVerifier` trait is unchanged — perf gating happens
  inside the pipeline orchestrator, not the verifier callback. We
  introduce a new `PerfVerifier` trait so callers can inject a custom
  perf gate (the M6 integration test injects one that flags the
  deliberately-broken canned response).

### 5. Downstream widening — dateutil M5 → 5/5 at M6

ADR-0009 §3 deferred pandas, sqlalchemy, pendulum to M6. M6 lands
vendored subsets:

| Dependent | M6 vendored test count | M6 status | Why pinnable |
|---|---|---|---|
| pandas | 3 (`to_datetime` ISO subset) | Pass | Uses `dateutil.parser.parse` ISO branch we translated |
| sqlalchemy | 3 (DateTime ISO subset) | Pass | Uses `dateutil.parser.isoparse`-equivalent |
| pendulum | 3 (relativedelta subset; `tz` skipped) | Skipped (3) | tz module out of M5/M6 scope; the test skips with a clear reason |

The skip path is fine — ADR-0009 §5's `Skipped { reason }` is exactly
what M6 uses to record pendulum's tz-dependency without lying about
coverage. M6 dateutil L3 reports **5/5 dependents driven**, with 4
returning `Pass` and 1 returning `Skipped { reason: "...tz out of
scope, M7+..." }` — a real signal that the L3 driver works
end-to-end, not a silent omission.

### 6. PyO3 build path — `--features pyo3`

ADR-0011 owns the full PyO3 wiring decision; for M6 the relevant
contract is:
- `cobrust-msgpack` and `cobrust-dateutil` both expose `pyo3` as a
  cargo feature.
- With `--features pyo3` the crate compiles to `cdylib` and exposes a
  Python-callable extension via PyO3.
- Without the feature flag, the crate stays `rlib` (the M5 default)
  and the gate suite uses the subprocess-CPython oracle.
- M6 ships the **build path** (Cargo.toml `[lib]` entries gated by
  `cfg(feature = "pyo3")`) but does not require the actual `.so`
  artifact in CI — the M6 done-means is "the feature compiles", not
  "every CI machine has libpython on PATH".

### 7. M6 scope window

- **In scope**:
  - msgpack-python `pack` / `unpack` for the value subset above.
  - `Packer` / `Unpacker` skeleton classes (Rust structs + methods).
  - Cython AST shim handling the `_packer.pyx` / `_unpacker.pyx`
    constructs we vendor.
  - L2.behavior bytes-identical fuzz gate (≥ 1000 inputs per public
    function).
  - L2.perf gate at 0.7× threshold; perf-gate failure repair routing.
  - L3 PyO3-shaped wrapper (subprocess CPython differential).
  - L3 dependents for msgpack: 2 of {pyspark, redis-py, msgpack-numpy}
    (pinned in ADR §"Dependent selection" below; `pyspark` is too
    heavy for M6, deferred).
  - dateutil L3 widening to 5/5 with the subset above.
  - Real-LLM smoke test (gated by env presence).
- **Out of scope (M7+)**:
  - msgpack ext types, timestamp ext.
  - msgpack streaming `Unpacker.feed()`.
  - msgpack `default` callback / `object_hook`.
  - Full Cython front-end (we ship a lexical shim).

#### Dependent selection for msgpack (M6)

| Dependent | Vendored test count | M6 status | Why |
|---|---|---|---|
| **redis-py** | 4 (msgpack-cached values subset) | Pass | Smallest dependency; common pattern (cached payloads) |
| **msgpack-numpy** | 3 (1-D arrays + scalars subset) | Pass | Exercises the binary type path and array container |
| pyspark | 0 | Deferred to M7 | Too heavy to vendor (Spark needs JVM; out of scope) |

L3 dependents for msgpack: **2/3 driven, 1/3 deferred**. This matches
constitution §4.2 "top 5" with the same partial-coverage policy
ADR-0009 set for dateutil. The policy is: a deferred dependent must
be ADR-justified and named in `gates.dependents.deferred`.

## Decision

Adopt all chosen options above. Concretely:

```
docs/agent/adr/0010-native-ext-translation.md     ← this file
docs/agent/adr/0011-pyo3-build-path.md            ← PyO3 wiring (M6)

corpus/msgpack/
    UPSTREAM_VERSION              # "1.0.8"
    UPSTREAM_LICENSE              # Apache-2.0 (license-compatible per adr:0001)
    spec.toml                     # L0 spec (pure-Py + Cython entries)
    upstream/
        fallback.py               # pure-Python encoder/decoder
        _packer.pyx               # Cython packer (annotated)
        _unpacker.pyx             # Cython unpacker (annotated)
        exceptions.py             # error types
    upstream_tests/               # vendored upstream pytest files
        test_pack.py
        test_unpack.py
    canned_llm_responses.toml     # synthetic-mode response table
                                  # (covers Stage A + Stage B)
    harness/
        h_pack.py                 # L0 differential harness for pack
        h_unpack.py               # L0 differential harness for unpack
    perf.toml                     # threshold = 0.7, pass_ratio = 1.0
    dependents/
        redis-py/
            UPSTREAM_VERSION
            LICENSE
            test_redis_subset.py
        msgpack-numpy/
            UPSTREAM_VERSION
            LICENSE
            test_msgpack_numpy_subset.py

corpus/dateutil/dependents/        ← M5 followup (3 new dependents)
    pandas/
    sqlalchemy/
    pendulum/                      # tz-dependent; tests skip cleanly

crates/cobrust-msgpack/            # generated by the pipeline
    src/
        lib.rs                    # public Rust API mirroring msgpack
        packer.rs                 # translated from fallback.py + _packer.pyx
        unpacker.rs               # translated from fallback.py + _unpacker.pyx
        exceptions.rs             # translated error types
    Cargo.toml                    # [features] pyo3 = []
    PROVENANCE.toml               # full manifest with native-ext tier
    python/
        msgpack_init.py
        setup.py
    tests/
        msgpack_pipeline.rs       # pipeline + repair loop on canned table
        msgpack_downstream.rs     # L3 (subprocess CPython oracle)
        msgpack_fuzz.rs           # ≥ 1000 panic-free + bytes-identical
        msgpack_bench.rs          # criterion-style timing → JSON
        upstream_tests/

crates/cobrust-translator/src/
    cython.rs                     # NEW: lexical Cython AST shim
    pipeline.rs                   # extended: dispatch .pyx via cython prompt; perf-gate routing
    bench.rs                      # extended: native-ext threshold tier
    error.rs                      # PerfGate raising semantics doc
    templates/
        msgpack_downstream.rs.tmpl
        msgpack_fuzz.rs.tmpl
        msgpack_bench.rs.tmpl
        cython_pack.rs.tmpl       # Cython prompt template
```

### Public surface additions

```rust
// crates/cobrust-translator/src/cython.rs
pub mod cython {
    /// Tokenised view of a Cython source. Carries enough metadata for
    /// the translator's prompt builder to emit Rust signatures with
    /// the right type mappings.
    pub struct CythonSource {
        pub functions: Vec<CythonFunction>,
        pub imports: Vec<String>,
    }

    pub struct CythonFunction {
        pub name: String,
        pub kind: CythonFunctionKind,    // cdef | cpdef | def
        pub decorators: Vec<String>,     // includes "inline" if `cdef inline`
        pub params: Vec<CythonParam>,
        pub return_type: Option<CythonType>,
        pub body: String,                // Cython body text (translator's input)
    }

    pub enum CythonFunctionKind { Cdef, Cpdef, Def }

    pub struct CythonParam {
        pub name: String,
        pub ty: Option<CythonType>,
    }

    pub enum CythonType {
        Int,
        UnsignedInt,
        PySsizeT,
        Bint,
        Float,
        Double,
        Str,
        Bytes,
        Unicode,
        Object,
        List,
        Dict,
        Custom(String),     // unknown — translator emits `serde_json::Value`
    }

    impl CythonType {
        /// Map to the Rust type the translator emits. Used by the
        /// prompt builder when synthesising the M6 Cython prompt.
        pub fn to_rust(&self) -> &str { /* ... */ }
    }

    /// Parse a Cython source via the M6 lexical shim. Whitespace-
    /// tolerant; recognises `cdef`, `cpdef`, `def`, function bodies,
    /// and the type-annotation subset enumerated above.
    pub fn parse(source: &str) -> Result<CythonSource, ShimError>;

    pub enum ShimError { /* ... */ }
}

// crates/cobrust-translator/src/pipeline.rs
pub trait PerfVerifier: Send + Sync {
    fn verify(&self, report: &BenchmarkReport) -> PerfVerdict;
}

pub enum PerfVerdict {
    Accept,
    Reject(GateFailure),
}

pub struct AcceptAllPerf;
impl PerfVerifier for AcceptAllPerf { /* ... */ }

pub async fn translate_with_verifiers(
    library: &PyLibrary,
    cfg: &TranslatorConfig,
    behavior: &dyn BehaviorVerifier,
    perf: &dyn PerfVerifier,
) -> Result<TranslatedCrate, TranslatorError>;
```

`translate_with_verifier` (M5) becomes a shim: equivalent to
`translate_with_verifiers(.., behavior, &AcceptAllPerf)`. M4
`translate(...)` is unchanged: `translate_with_verifiers(.., &AcceptAll, &AcceptAllPerf)`.

### Synthetic provider — task field extends

The synthetic header `task:` line gains a new value: `translate_cython`.
Existing `translate` entries are unaffected. The provider's lookup
key is now `(task, function, attempt)`; the table format is unchanged.

### M6 manifest fields

`PROVENANCE.toml` `gates.l2_perf` becomes:

```toml
l2_perf = "pass (3/3 ≥ 0.70×; pass_ratio=1.00; native-ext tier per ADR-0010)"
```

`gates.l3_downstream_dependents` for msgpack:

```toml
l3_downstream_dependents = "pass 2/3 (redis-py, msgpack-numpy); deferred 1/3 (pyspark) to M7 per ADR-0010"
```

`gates.dependents` block for msgpack:

```toml
[gates.dependents]
covered = ["redis-py", "msgpack-numpy"]
deferred = ["pyspark"]
deferred_reason = "M6 budget; M7 widens per ADR-0010"
```

For dateutil at M6 (the widening):

```toml
l3_downstream_dependents = "pass 4/5 (croniter, freezegun, pandas, sqlalchemy); skipped 1/5 (pendulum tz out of scope per ADR-0010 §5)"

[gates.dependents]
covered = ["croniter", "freezegun", "pandas", "sqlalchemy"]
skipped = ["pendulum"]
skipped_reason = "tz module out of M5/M6 scope; M7+ per ADR-0010 §5"
deferred = []
deferred_reason = ""
```

`DependentsSection` gains optional `skipped: Vec<String>` +
`skipped_reason: String` fields. `#[serde(default)]` for backward
compatibility with M4 tomli + M5 dateutil manifests.

## Consequences

- **Positive**
  - The M6 deliverable proves the translation pipeline handles non-
    pure-Python sources end-to-end. The byte-identical fuzz gate is
    a tight, falsifiable contract — if the cython-translated path
    diverges from the pure-Py path, fuzz fails immediately.
  - The Cython lexical shim is small (~300 LoC) and contained; M7+
    can replace it with a real Cython front-end without changing the
    pipeline surface.
  - Per-library threshold tiers make "is this library's perf gate
    fair?" an ADR-anchored, auditable question rather than a hidden
    knob.
  - Perf-gate failure routing closes the constitution §4.2 loop on
    perf — M6 is where the L2.perf gate actually fails-on-miss.
  - dateutil's L3 hits 5/5 (4 pass + 1 skipped-with-reason),
    discharging ADR-0009 §3's M6 commitment.

- **Negative**
  - The Cython shim is lexical, not semantic — Cython idioms like
    fused types or memoryviews are out of scope. M6 deliberately
    vendors only msgpack constructs we support; M7+ widens.
  - Perf at 0.7× for msgpack is generous compared to pure-Python's
    0.8×. The trade is real: if we held msgpack to 0.8×, the M6
    gate would chase micro-optimisation forever. We document the
    floor and audit it per-library.
  - We commit the generated `cobrust-msgpack/` bytes (M5 precedent
    for cobrust-dateutil); regeneration must stay deterministic.
  - The PyO3 feature is opt-in; native-extension users must run
    `cargo build -p cobrust-msgpack --features pyo3` explicitly.
    M6 documents this in `python/setup.py` + the wrapper README.

- **Neutral / unknown**
  - Real-LLM smoke is gated by env presence — when no key is in
    env, the test is a no-op. Some CI runs will skip it; that's
    fine for M6 because the pipeline is still exercised end-to-end
    via the synthetic path.
  - We do not gate on the actual PyO3-built `.so` in CI (linking
    against libpython varies by host). M7+ may add a CI matrix that
    includes a Python-installed runner.

## Evidence

- Constitution `CLAUDE.md` §4.2 (perf threshold, retry threshold, L3
  top-5 dependents), §5.2 (reproducible benchmarks), §7 (M6 scope).
- `adr:0007` — L0+L1 pipeline this ADR extends.
- `adr:0008` — L2.perf + repair loop infrastructure this ADR makes
  fail-on-miss.
- `adr:0009` — L3 partial-coverage policy this ADR widens.
- `adr:0011` — PyO3 build path companion ADR for M6.
- msgpack-python upstream — https://github.com/msgpack/msgpack-python
  (Apache-2.0; license-compatible per `adr:0001`).
- redis-py upstream — https://github.com/redis/redis-py (MIT).
- msgpack-numpy upstream — https://github.com/lebedov/msgpack-numpy (BSD).
- pandas / sqlalchemy / pendulum — license-compatible per `adr:0001`.
