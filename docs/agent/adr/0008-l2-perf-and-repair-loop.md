---
doc_kind: adr
adr_id: 0008
title: L2.perf benchmark harness, repair loop, and L2/L3 escalation pipeline
status: accepted
date: 2026-04-30
last_verified_commit: 8c477b4
supersedes: []
superseded_by: []
---

# ADR-0008: L2.perf benchmark harness, repair loop, and L2/L3 escalation pipeline

## Context

M4 (`adr:0007`) landed L0 + L1 end-to-end on `tomli`, with L2.build,
L2.behavior and an L3 PyO3-shaped wrapper differential gate already
green. Two of the constitution §4.2 obligations were deliberately
deferred to M5:

1. **L2.perf** — "≥ 0.8× of original on representative benchmarks
   (configurable per library)". M4 records benchmark numbers but does
   **not** gate on them.
2. **Closed-loop repair on gate failure** — §4.2 ("Failure at any gate
   → diagnostic feeds back to L1 → re-translate → re-verify. Loop
   until pass or escalation threshold (e.g., 50 retries)"). M4 emits
   diagnostics on disk but does not feed them back into the router as
   a follow-up translation prompt.

`mod:translator` Status section already lists both items in "Done means
(M5)". This ADR fixes their concrete shape so the implementation can
land atomically with the second translated library (`python-dateutil`,
see `adr:0009`) and the M5 gate run.

## Options considered

### 1. L2.perf benchmark harness — execution model

1. **Hand-rolled timing loops in `tests/*.rs`** — easy to write but
   provides no statistical rigor (no warmup, no outlier filtering, no
   seed control). Rejected — violates §5.2 ("Every benchmark is
   reproducible: scripted, seeded, hardware-tagged").
2. **Criterion crate `[[bench]]` targets only** — gives proper
   statistical analysis but `cargo bench` is gated behind nightly
   without `--bench` build wiring; running it from `cargo test` or a
   regular gate is awkward. Also, criterion does not natively support
   "compare against an external CPython subprocess" — its model is
   "compare two Rust closures".
3. **Hand-rolled subprocess-pinned harness with seeded inputs +
   warmup + N-run median** *(chosen, lives in
   `cobrust-translator/src/bench.rs`)* — produces a JSON report with
   per-public-fn `(cobrust_ns_median, cpython_ns_median, ratio,
   pass)`, hardware tag (`uname -srm`), and the exact input corpus
   used. The harness is invoked from the pipeline (always recorded)
   and from a per-library integration test (gated when CPython is on
   PATH). Threshold default `0.8`; per-library override via
   `PerfTarget` carried in `TranslatorConfig`. M5 keeps it lean — no
   criterion dep. Once the static-core compiler stops emerging
   features, M6+ can replace this with criterion if there's value.

### 2. L2.perf gate semantics

1. **Hard gate — fail the pipeline if any function < 0.8×** —
   blunt; one slow path nukes the whole library translation.
2. **Per-function gate with fail-fast** — same problem, plus
   diagnostic noise.
3. **Threshold `pass_ratio` (default 1.0): "at least
   `pass_ratio × public_fn_count` functions must meet 0.8×, others
   recorded as `divergences`"** *(chosen)*. For M5 we pin
   `pass_ratio = 1.0` for tomli (already meets) and override
   `pass_ratio = 0.5` for dateutil's `parse` family (synthetic-mode
   responses are placeholders; performance is not the gate-day
   priority). Per-library overrides land in
   `corpus/<library>/perf.toml` (machine-readable), pinned by ADR
   evidence.

### 3. Repair-loop architecture

1. **In-process retry inside `run_l1`** — retries one function on its
   own. Loses cross-function context. Rejected.
2. **Pipeline-level repair loop in `pipeline::translate`**, with a
   per-function retry count, diagnostic blob shipped to the next
   translation prompt as a system message *(chosen)*. After
   `escalation_threshold` (default 50) retries of the same function,
   the function is marked `@py_compat(none)`, a `failure_report.md`
   is emitted next to the manifest, and the pipeline returns a
   `TranslatorError` describing which function escalated — so a CTO
   reading the error message immediately knows which function and
   which gate.
3. **Async repair queue** — over-engineered for M5; we don't need a
   queue, we need a loop. M6+ if parallelism becomes a bottleneck.

### 4. Diagnostic blob schema

The `GateFailure` blob the repair loop ships back to L1 must be
self-contained — the LLM (or, in synthetic mode, the curator who
records a follow-up canned response) must be able to act on it
without external context.

```rust
pub struct GateFailure {
    /// Which function this diagnostic is about.
    pub function: String,
    /// The gate that failed — `l2_build`, `l2_behavior`, `l2_perf`, or `l3_downstream`.
    pub failed_gate: String,
    /// Human-readable summary, ~1 sentence.
    pub failure_summary: String,
    /// The minimal failing inputs (or build snippet), serialised as TOML.
    pub failed_inputs: Vec<String>,
    /// The expected output (CPython oracle) for the first failing input.
    pub expected: Option<String>,
    /// The actual output the translation produced.
    pub actual: Option<String>,
    /// Attempt counter (1-based — the first repair is attempt 2).
    pub attempt: u32,
}
```

Persisted at `out/<library>/diagnostics/<function>__<attempt>.toml`.

### 5. How synthetic mode interacts with repair

Synthetic mode is the M5 default gate path (no API keys on this
machine). The repair loop must still exercise its plumbing in
synthetic mode, otherwise we ship dead code. We solve this by:

1. Allowing the `corpus/<library>/canned_llm_responses.toml` to carry
   **multiple `[[entry]]` rows for the same `(task, function)`
   tuple**, distinguished by an optional `attempt` field (default 1).
   The synthetic provider returns the row whose attempt matches the
   incoming prompt's `attempt: <N>` header line. The header schema
   gains a new optional line:

   ```text
   cobrust-translator/v1
   task: translate
   function: parse
   source-sha256: <16-hex>
   attempt: 2
   ---
   <body, including diagnostic blob>
   ```

2. The first attempt has no `attempt:` line (synthetic provider
   defaults to attempt 1). Subsequent attempts pass the diagnostic
   from the prior gate failure as the prompt body, and the synthetic
   table serves the corrected response for that attempt. This is the
   M5 way to test the closed loop without real LLM keys.

3. The dateutil M5 gate run **deliberately** seeds attempt-1 with a
   broken `parse` response and attempt-2 with the correct one. The
   integration test at
   `crates/cobrust-dateutil/tests/dateutil_pipeline.rs::repair_loop_recovers`
   asserts that the pipeline lands at attempt-2 (i.e. retry count
   `> 0` and `< escalation_threshold`).

### 6. Failure report

When `escalation_threshold` is hit, the pipeline writes
`<crate_dir>/failure_report.md` with:

- The function name + qualname.
- Every diagnostic blob from every attempt.
- The final `@py_compat(none)` justification.
- A pointer back to ADR-0008 §"Failure routing".

The PROVENANCE manifest's `verification.known_failures` field is
populated with the failed function name and a one-line summary.

### 7. Ledger and benchmark report locations

- Diagnostics: `<out_dir>/<library>/diagnostics/<fn>__<attempt>.toml`.
- Benchmark report: `target/cobrust-bench/<library>/<commit>/report.json`
  where `<commit>` is the short HEAD SHA at the time the harness
  runs. Identical commit ⇒ identical filename ⇒ overwrite (latest
  wins). The report contains:

  ```json
  {
      "library": "dateutil",
      "commit": "abc1234",
      "hardware": "Darwin arm64 25.3.0",
      "rustc": "rustc 1.94.1",
      "cpython": "3.11.15",
      "threshold": 0.8,
      "pass_ratio": 1.0,
      "results": [
          {
              "function": "parse",
              "cobrust_ns_median": 1234,
              "cpython_ns_median": 1500,
              "ratio": 1.21,
              "pass": true,
              "n_inputs": 64,
              "n_iters": 100
          }
      ]
  }
  ```

## Decision

Adopt all chosen options above. Concretely:

```
crates/cobrust-translator/src/
    repair.rs          // repair_translation(failure, plan) → new TranslationOutput
    bench.rs           // run_perf_harness(library) → BenchmarkReport
    downstream.rs      // L3 downstream-dependents driver (see adr:0009 for scope)
    pipeline.rs        // extended to call L2.{behavior,perf} + L3 + repair
    error.rs           // extended with EscalationExceeded + PerfGate variants
    synthetic.rs       // header gains optional `attempt:` line
```

### New / extended public surface

```rust
/// L2.perf evidence written to disk + returned by the harness.
pub struct BenchmarkReport {
    pub library: String,
    pub commit: String,
    pub hardware: String,
    pub rustc: String,
    pub cpython: String,
    pub threshold: f64,
    pub pass_ratio: f64,
    pub results: Vec<BenchmarkResult>,
}

pub struct BenchmarkResult {
    pub function: String,
    pub cobrust_ns_median: u64,
    pub cpython_ns_median: u64,
    pub ratio: f64,
    pub pass: bool,
    pub n_inputs: u32,
    pub n_iters: u32,
}

/// L3 downstream evidence (per adr:0009).
pub struct DownstreamReport {
    pub library: String,
    pub dependents: Vec<DependentResult>,
}

pub struct DependentResult {
    pub name: String,
    pub tests_run: u32,
    pub tests_passed: u32,
    pub status: DependentStatus,
}

pub enum DependentStatus {
    Pass,
    Skipped { reason: String },
    Failed { failures: Vec<String> },
}

/// Diagnostic blob consumed by the repair loop.
pub struct GateFailure { /* see §4 above */ }

/// Pipeline-level entry point — unchanged caller-side; new behaviour
/// when gates fail (closed loop) or escalate (failure report + error).
pub async fn translate(
    library: &PyLibrary,
    cfg: &TranslatorConfig,
) -> Result<TranslatedCrate, TranslatorError>;
```

### `TranslatorError` extension

```rust
pub enum TranslatorError {
    // ...existing variants...
    /// Repair loop hit `escalation_threshold` retries on one function.
    EscalationExceeded { function: String, attempts: u32, failed_gate: String },
    /// L2.perf gate failed (cobrust < threshold × cpython on too many functions).
    PerfGate(String),
}
```

### Synthetic provider header — version 1.1

```text
cobrust-translator/v1
task: translate
function: parse
source-sha256: 9a1adcc278853b5e
attempt: 2                  # NEW; optional; default 1
---
<body>
```

The on-disk canned table gains an optional per-entry `attempt` field
(default `1`). Older M4 tomli responses are unaffected (every entry
defaults to `attempt = 1`).

### Pipeline state machine

```
   L0 spec ─→ L1 translate (attempt N)
                    │
                    ▼
            L2.build ─fail─→ diagnostic ─→ repair (attempt N+1)
                    │ pass
                    ▼
            L2.behavior ─fail─→ diagnostic ─→ repair (attempt N+1)
                    │ pass
                    ▼
            L2.perf ─fail─→ diagnostic ─→ repair (attempt N+1)
                    │ pass (or pass_ratio met)
                    ▼
            L3.pyo3_wrapper (always pass given L2)
                    │
                    ▼
            L3.downstream ─fail─→ diagnostic ─→ repair (attempt N+1)
                    │ pass
                    ▼
            Manifest write + Ok(TranslatedCrate)

   At every retry: attempt += 1. Reaching escalation_threshold ⇒
   write failure_report.md + return EscalationExceeded.
```

## Consequences

- **Positive**
  - The closed loop the constitution mandates (§4.2) is now actually
    closed. Synthetic mode tests the loop deterministically;
    real-LLM mode (M5+) inherits the same path.
  - L2.perf becomes a first-class manifest field (`gates.l2_perf =
    "pass (12/12 ≥ 0.8×)"` style), with per-library override
    documented in the corpus.
  - The benchmark report at
    `target/cobrust-bench/<library>/<commit>/report.json` makes
    "did the perf change?" a one-grep question, mirroring
    `deterministic_id`'s philosophy.
  - The diagnostic blob schema is human-readable TOML, audit-ready.
  - Per-attempt synthetic responses give us regression coverage for
    the repair path itself (without requiring real LLMs).

- **Negative**
  - Extra synthetic-table entries per repair scenario; reviewers must
    audit both attempts.
  - The harness `subprocess-pinned-CPython` model means M5 perf
    numbers are noisy (subprocess startup ~ 30ms). We mitigate with
    long N-iter counts; the threshold is set conservatively (0.8× on
    medians, not means).
  - `escalation_threshold = 50` is generous; in practice synthetic
    mode lands attempt-2 always, but the dial is there for M6+
    real-LLM noise.

- **Neutral / unknown**
  - We do not yet support cross-function repair (e.g. fix `parse`
    by also touching `parsehelper`). M6+ if needed.
  - `BenchmarkReport`'s `cpython_ns_median` is taken from a
    subprocess-pinned timer; the calling test marks the report
    `skipped` cleanly if `python3.11` is missing.

## Evidence

- Constitution `CLAUDE.md` §4.2 (gates, retry threshold, perf
  threshold), §5.2 (reproducible benchmarks), §5.3 (efficient).
- `adr:0007` — L0+L1 pipeline this ADR extends.
- `adr:0009` — L3 downstream-dependents widening (companion ADR).
- `mod:translator` Status section "Done means (M5)" item list.
- `mod:llm_router` — `Router::dispatch` is the unchanged primitive.
- M4 tomli baseline: all M4 numbers retained; M5 adds new perf JSON
  but does not regress any M4 manifest field.
