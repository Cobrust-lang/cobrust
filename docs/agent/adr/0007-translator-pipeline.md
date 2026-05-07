---
doc_kind: adr
adr_id: 0007
title: Translator pipeline — L0 spec, L1 translation, provenance manifest, synthetic-LLM mode, PyO3 wrapper
status: accepted
date: 2026-04-30
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0007: Translator pipeline — L0 spec, L1 translation, provenance manifest, synthetic-LLM mode, PyO3 wrapper

## Context

`mod:translator` (crate `cobrust-translator`, see
`docs/agent/modules/translator.md`) is the heart of the project per
constitution `CLAUDE.md` §1.2 — **the AI-native compiler is what makes
Cobrust differ from "yet another Python-shaped Rust dialect"**. The
constitution §4.2 fixes the four-stage closed loop (L0 → L1 → L2 → L3)
but leaves several decisions for an ADR before any code lands:

1. Concrete shape of the L0 `spec.toml` + harness directory layout.
2. L1 translation contract — granularity, dependency-graph order,
   prompt anchor, emitted-file provenance header.
3. Provenance-manifest schema — fields beyond §4.2 sketch.
4. **Synthetic-LLM mode** — how the M4 gate runs without depending on
   network LLM keys, while the real-LLM path remains a one-flag flip.
5. PyO3 wrapper layout — directory structure, ABI surface, build path.
6. Failure routing — diagnostic format, retry threshold, escalation.
7. M4 scope window relative to §7 (M5 widens to dependents,
   `python-dateutil`; M4 ships only `tomli` upstream tests).

Constitution §6 (atomic commits, ADR-or-it-didn't-happen) requires this
to land before any code that relies on it. M3 (`adr:0004`) already
fixes the dispatch primitive (`Router::dispatch`); this ADR sits on
top of it and is the M4 binding contract.

## Options considered

### 1. L0 spec format

1. **Free-form Markdown spec written by an LLM** — too unstructured
   to drive a re-runnable harness. Rejected.
2. **JSON Schema for every public function** — verbose, no good way
   to express tolerances or known-divergence sets. Rejected.
3. **TOML `spec.toml` with `[function.<name>]` tables + sibling
   `harness/<name>.py`** *(chosen)* — TOML lines up with the existing
   `cobrust.toml` config style; one harness file per public function
   keeps the differential-test pattern legible. The harness is real
   Python, calls the CPython oracle, and emits exemplar pairs that
   feed directly into the L0 differential test bank.

### 2. L1 translation contract

1. **Whole-file translation in one LLM call** — exceeds context
   budgets on real libraries (numpy, scipy, etc.) and produces
   un-diffable bulk emissions. Rejected as a default.
2. **Function-level, bottom-up by dependency graph, with explicit
   provenance headers** *(chosen)* — matches constitution §4.2
   verbatim ("Granularity: function-level, bottom-up by dependency
   graph"). Each emitted Rust function (or struct/impl) carries:
   - source library + version
   - source-file SHA-256 truncated to 16 hex
   - originating Python qualified name
   - LLM router decision id (cache key + provider + model)
   - synthetic-mode flag when the response was canned
3. **Module-level translation chunks** — middle ground; deferred to
   M5+ as an optional bulk-mode for very small modules.

### 3. Provenance manifest schema

The §4.2 sketch is a starting point; M4 binds:

```toml
[source]
library = "tomli"
version = "2.0.1"
sha256 = "<64-hex of vendored source archive>"
file_count = 8

[oracle]
runtime = "cpython"
runtime_version = "3.11.15"
oracle_module = "tomllib"          # the import path used as ground truth

[verification]
seeds = [42, 1337, 0xDEADBEEF]
fuzz_inputs_per_fn = 1024          # M4 default; per-fn override allowed
divergences = []
known_failures = []

[router]
strategy = "synthetic"             # one of: synthetic | quality | consensus
models_used = ["synthetic:tomli-canned-v1"]
ledger_entries = 23                # cross-checked against .cobrust/ledger.jsonl

[build]
toolchain = "rustc 1.94.1"
deterministic_id = "blake3:<64-hex>"   # blake3(source_sha || toolchain || sorted_router_decision_ids)
crate_layout_version = 1

[gates]
l0_spec_emitted = true
l1_files_emitted = 12
l2_build = "pass"                  # or "fail:<reason>"
l2_behavior = "pass"
l2_perf = "skipped (M4 records, M5 gates)"
l3_pyo3_wrapper = "pass"
l3_downstream_dependents = "deferred to M5"
```

**`deterministic_id` is the load-bearing reproducibility token.** It is:

```text
blake3(
    source_sha256_hex                 || b"\n" ||
    toolchain_string                  || b"\n" ||
    sorted_join(router_decision_ids, "\n")
)
```

where each `router_decision_id` is itself the cache key the router
returned for that LLM call (`blake3:` form). Identical inputs ⇒
identical `deterministic_id`. This is the constitution §2.4 promise
("Deterministic build IDs") made concrete.

### 4. Synthetic-LLM mode

The constitution requires every gate to run unskipped. But the M4
machine has no API keys for Anthropic / OpenAI / DeepSeek (CTO
confirmed). Three options:

1. **Skip M4 until keys are provisioned** — violates §6 (no skipping)
   and §7 (M4 must land before M5). Rejected.
2. **Spin up a local vLLM and run a real model** — feasible but
   expands the M4 gate surface to include GPU/quant decisions and
   adds 5+ GB of model weights to CI. Out of scope for M4. Deferred
   to M5+.
3. **Synthetic-LLM mode with canned correct responses** *(chosen)* —
   an `LlmProvider` impl that serves pre-recorded `CompletionResponse`
   bytes keyed by the **canonical prompt hash** (= the same
   `BLAKE3(canonical_request_bytes)` the cache uses, but as a lookup
   key rather than a write target). The plumbing — router, cache,
   ledger, manifest, PyO3 wrapper, downstream tests — is **all real**;
   only the LLM is canned. M5+ replaces canned responses with real
   provider calls behind `--features real-llm`; the same router code
   serves both paths.

**Canned-response file format** (committed to repo at
`corpus/<library>/canned_llm_responses.toml`):

```toml
schema_version = 1
oracle_runtime = "cpython 3.11"

[[entry]]
prompt_hash = "blake3:<64-hex>"   # same canonical bytes the router cache uses
task = "spec_extract"
response_text = '''
... emitted Cobrust / Rust source or spec text ...
'''

[[entry]]
prompt_hash = "blake3:<64-hex>"
task = "translate"
response_text = '''...'''
```

**Lookup contract**: when a prompt hash is **not found** in the canned
table, the synthetic provider returns
`LlmError::Provider { code: "synthetic-miss", message: "no canned response for <hash>" }`.
This is permanent (caller must add the entry or switch to real-LLM
mode); it is **not silently swallowed**. Constitution §2.4 ("no silent
translations, ever") is preserved.

**Real-LLM mode** is enabled by:

- A `cobrust.toml` whose `[providers.*]` actually point at real
  endpoints with valid `api_key_env` entries.
- A translator-crate cargo feature `real-llm` (off by default) that
  swaps the synthetic provider for a `cobrust-llm-router` adapter
  set built from the config.

Both modes share the same `Router::dispatch` call site; only
provider registration differs.

### 5. PyO3 wrapper layout

1. **Hand-written `python/` directory with `setup.py`** — most
   familiar but locks M4 into a Python build toolchain that the gate
   suite would have to invoke. Rejected for the M4 gate path.
2. **Maturin-managed dynamic library** — works but pulls in maturin
   as a CI dep; the gate would have to run `maturin build` before
   `pytest`. Rejected for M4; revisit at M5+.
3. **Pure-Rust "PyO3-shaped wrapper" crate that subprocesses CPython
   for the differential oracle** *(chosen for M4)* — the wrapper
   crate (`cobrust-tomli`) exposes a Rust API matching the Python
   `tomllib` surface (`load(reader) -> dict`, `loads(s) -> dict`).
   The L3 gate harness (`tests/tomli_downstream.rs`) drives this
   wrapper from Rust **and** subprocesses
   `python3 -c "import tomllib; ..."` to obtain the CPython oracle
   result, comparing the two as JSON. This gives us the real
   differential test (constitution §4.2 L3) without dragging PyO3
   compilation into the M4 gate path.

   A `python/` directory is committed alongside the crate carrying:
   - `tomli_init.py` — re-exports the wrapper API for downstream
     packages (M5+ will flip this to import the PyO3-built native
     extension).
   - `setup.py` — placeholder build script citing `--features pyo3`
     for M5+; M4 leaves the actual extension build deferred.

   The `cobrust-tomli-pyo3` extension crate is **not** built in M4;
   it is scheduled for M5 along with the dependent-library tests
   (the same milestone that introduces the second translated lib).

### 6. Failure routing

- L0 failure → escalate to human (cannot translate without a spec).
- L1 failure → diagnostic written to
  `out/<library>/diagnostics/<fn>.txt` and the function name is
  pushed into the **repair queue**. The pipeline retries up to
  `escalation_threshold = 50` times (constitution §4.2). After 50,
  the function is marked `@py_compat(none)` with a human-readable
  failure report.
- L2.build failure → diagnostic = `cargo build` stderr for the
  smallest crate slice; goes to the same repair queue with the
  failing function flagged.
- L2.behavior failure → diagnostic = the failing input + expected vs
  observed output + the responsible function (last function on the
  call stack frame matching a translated module).
- L3 failure → reported but not gating in M4 (only the upstream
  testsuite runs; dependents wait for M5).

### 7. M4 scope window

M4 is intentionally narrow:

- **In scope**:
  - `tomli` library (Python 3.11+ `tomllib`-equivalent), specifically
    `loads(s) -> dict`. The full library is small; M4 vendors a
    representative subset of the parser core.
  - Synthetic-LLM mode is the **default path** for the gate suite.
  - Provenance manifest with all fields above.
  - PyO3-shaped wrapper crate (`cobrust-tomli`) and pure-Rust
    differential harness against CPython `tomllib`.
- **Out of scope (deferred to M5)**:
  - L2.perf gate (we record numbers, do not gate).
  - L3 downstream-dependents validation (no second lib at M4).
  - PyO3 native extension build (M5 introduces `--features pyo3`).
  - Real-LLM path verification (the code path is shipped, but the
    M4 gate run uses synthetic mode; M5 flips `--features real-llm`
    on at least one library).

## Decision

Adopt all chosen options above. Concretely:

```
crates/cobrust-translator/src/
    lib.rs           // public surface
    config.rs        // TranslatorConfig + parse from cobrust.toml
    spec.rs          // L0 spec extraction + spec.toml writer
    translate.rs     // L1 translation engine, function-level bottom-up
    manifest.rs      // ProvenanceManifest builder + writer + verifier
    synthetic.rs     // canned-LLM provider for the gate path
    pipeline.rs      // L0..L1 orchestrator, returns TranslatedCrate
    error.rs         // TranslatorError taxonomy
    deterministic.rs // deterministic_id computation

corpus/tomli/
    UPSTREAM_VERSION              // "2.0.1"
    UPSTREAM_LICENSE              // copy of upstream license
    spec.toml                     // L0 spec (committed reference)
    upstream/                     // vendored Python source subset
        tomli_loads.py            // representative subset of tomli's parser
    upstream_tests/               // upstream pytest files copied verbatim
        test_loads.py
    canned_llm_responses.toml     // synthetic-mode response table
    harness/                      // L0 differential harness Python files
        h_loads.py

crates/cobrust-tomli/             // generated by the pipeline (M4 commits the bytes for gate stability)
    src/
        lib.rs                    // public Rust API mirroring tomllib
        parser.rs                 // translated from upstream/tomli_loads.py
        ...                       // each file has a provenance header
    Cargo.toml
    PROVENANCE.toml               // the manifest, machine-readable
    python/
        tomli_init.py
        setup.py                  // M5 will flip the pyo3 path on
    tests/
        tomli_downstream.rs       // upstream tests + CPython differential
```

### Public surface

```rust
pub fn translate(
    source: PyLibrary,
    cfg: &TranslatorConfig,
) -> Result<TranslatedCrate, TranslatorError>;

pub struct PyLibrary {
    pub library: String,
    pub version: String,
    pub source_root: PathBuf,            // path to corpus/<lib>/upstream
    pub upstream_tests: PathBuf,
    pub canned_responses: Option<PathBuf>,// Some(...) ⇒ synthetic; None ⇒ real-LLM
    pub seeds: Vec<u64>,
    pub fuzz_inputs_per_fn: u32,
}

pub struct TranslatorConfig {
    pub router: cobrust_llm_router::RouterConfig,
    pub out_dir: PathBuf,
    pub oracle_runtime: String,          // "cpython 3.11" etc.
    pub oracle_module: String,           // import path used as ground truth
    pub escalation_threshold: u32,       // default 50
    pub synthetic_only: bool,            // M4 default true
}

pub struct TranslatedCrate {
    pub manifest: ProvenanceManifest,
    pub crate_dir: PathBuf,
    pub pyo3_wrapper_dir: PathBuf,
}

pub struct ProvenanceManifest { /* fields per the schema above */ }

pub enum TranslatorError {
    SpecExtraction(String),
    Translation { function: String, message: String },
    BuildGate(String),
    BehaviorGate(String),
    DownstreamGate(String),
    SyntheticMiss { prompt_hash: String, task: String },
    Io(std::io::Error),
    Router(cobrust_llm_router::RouterError),
}
```

`SyntheticMiss` is the canonical failure mode for synthetic-LLM
operation against an unrecorded prompt.

### Synthetic provider contract

```rust
/// Lookup-table provider: keys are prompt hashes (canonical
/// `CacheKey::compute(&provider_key, &request)` outputs).
pub struct SyntheticProvider {
    name: String,
    table: HashMap<String, String>,    // hex-only key → response_text
}

impl SyntheticProvider {
    pub fn from_canned_toml(name: &str, path: &Path) -> Result<Self, ConfigError>;
    pub fn record(&mut self, prompt_hash: &str, response_text: String);
}

#[async_trait::async_trait]
impl LlmProvider for SyntheticProvider {
    fn name(&self) -> &str;
    async fn complete(&self, req: CompletionRequest)
        -> Result<CompletionResponse, LlmError>;
    fn complete_stream(&self, req: CompletionRequest)
        -> Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>;
}
```

The synthetic provider name **must** match the provider key the
caller used when computing the prompt hash (`CacheKey::compute(name,
&req)`); otherwise lookups miss. M4 uses `name = "synthetic"`
consistently.

### Failure-routing diagnostic format (per task)

```toml
[diagnostic]
function = "tomli::parser::parse_value"
phase = "l2_behavior"
attempts = 3
last_error = "expected dict, got list"
input_seed = 1337
input_repr = '''[[a, 1], [b, 2]]'''
expected = '''{"a": 1, "b": 2}'''
observed = '''[["a", 1], ["b", 2]]'''
```

These files live under `out/<lib>/diagnostics/` and are read by the
repair task (M5+).

## Consequences

- **Positive**
  - The M4 gate is fully reproducible without API keys; CI runs the
    full L0..L3 closed loop using canned LLM responses.
  - The router from M3 is exercised end-to-end by a real consumer;
    invariants like cache reproducibility and ledger append-only are
    proven on a non-trivial workload.
  - PyO3 is staged for M5 without M4 having to fight Python build
    tooling; the L3 differential gate runs against the **real**
    CPython oracle via subprocess.
  - The canned-response file is human-readable TOML — reviewers can
    audit every byte the synthetic provider serves.
  - `deterministic_id` makes "did the translation change?" a
    one-grep question.

- **Negative**
  - Synthetic-LLM mode means M4 doesn't prove the prompt template is
    good — only that the orchestration plumbing is correct given a
    correct response. M5 closes this gap by running real-LLM on at
    least one library.
  - The PyO3-shaped wrapper crate is "Python-shaped" but not
    importable from Python in M4. M5 lights it up.
  - We commit the generated `cobrust-tomli/` bytes into the repo for
    gate stability; that increases repo size and demands strict
    determinism on every regeneration. Mitigated by the
    `deterministic_id` check in the gate.

- **Neutral / unknown**
  - The choice to subprocess CPython (rather than embed via PyO3) at
    L3 makes the gate require `python3` on PATH. This is acceptable
    for M4 (CI images have Python) and the test harness skips with
    a clear message if `python3` is missing.
  - `synthetic-miss` is escalated as `Provider` not `BadRequest` so
    the router does not retry. This matches `adr:0004` retry
    semantics (`Provider` is non-transient).

## Evidence

- Constitution `CLAUDE.md` §4.2 (four-stage closed loop, retry
  threshold, gate definitions), §2.4 ("Translation provenance",
  "Deterministic build IDs"), §6 (atomic commits, no skipping
  gates).
- `adr:0004` — `Router::dispatch`, cache-key canonicalisation,
  ledger schema, error taxonomy.
- `mod:translator` doc — M4 done-means and target manifest shape.
- `mod:llm_router` doc — public surface this ADR consumes.
- M3 router test pattern (`crates/cobrust-llm-router/tests/synthetic_provider.rs`)
  — the in-process `SyntheticProvider` precedent we extend.
- `tomli` upstream — https://github.com/hukkin/tomli (Apache-2.0 +
  MIT — license-compatible per `adr:0001`).
- CPython `tomllib` (since Python 3.11) — the L3 oracle.
