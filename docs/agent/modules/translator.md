---
doc_kind: module
module_id: mod:translator
crate: cobrust-translator
last_verified_commit: 62ef6bd
dependencies: [mod:llm_router, mod:frontend, mod:types]
---

# Module: translator

## Purpose

L0 → L3 closed-loop translation of Python (and later C / C++ / Fortran)
libraries into Cobrust, with full provenance.

## Status

M0 — empty stub. First delivery at M4 (end-to-end on `tomli`).

## Pipeline

```
target source
      ↓
   L0 spec extraction
      ↓
   L1 translation
      ↓
   L2 verification ──┐
      ↓              │ retry (diagnostic-driven)
   L3 integration    │
      ↓              │
   Cobrust registry  │
                     │
   gate failure ─────┘
```

## Gates (none optional)

| Stage | Gate | Pass criteria |
|---|---|---|
| L0 | spec produced | `spec.toml` + `harness/` directory committed |
| L1 | code emitted | every file has provenance header |
| L2.build | `cargo build --release` | zero warnings |
| L2.behavior | testsuite + property + L0 differential | tolerance per `@py_compat`; ≥ 1000 fuzzed inputs per public fn |
| L2.perf | benchmark | ≥ 0.8× original on representative bench (per-library override allowed) |
| L3 | downstream validation | top-5 dependents' testsuites pass |

Failure at any L2 gate routes diagnostic back to L1. Default escalation
threshold: 50 retries → human-readable failure report → function marked
`@py_compat(none)` with explanation.

## Provenance manifest (target shape)

Every translated module carries a manifest:

```toml
[source]
library = "tomli"
version = "2.0.1"
sha256 = "..."

[oracle]
runtime = "cpython"
runtime_version = "3.12.5"

[verification]
seeds = [42, 1337, 0xdeadbeef]
divergences = []
known_failures = []

[router]
strategy = "consensus"
models_used = [
    "anthropic_official:claude-opus-4-7",
    "deepseek:deepseek-v3",
]

[build]
toolchain = "rustc 1.94.1"
deterministic_id = "blake3:..."
```

## Public surface (target — M4)

```rust
pub fn translate(
    source: PyLibrary,
    cfg: &TranslatorConfig,
) -> Result<TranslatedCrate, TranslatorError>;

pub struct TranslatedCrate {
    pub manifest: ProvenanceManifest,
    pub crate_dir: PathBuf,
    pub pyo3_wrapper_dir: PathBuf,
}
```

## Invariants

- **No silent translations.** Every translated file carries a provenance
  header.
- **Determinism.** Given identical `(source, toolchain, router decisions)`,
  the resulting crate is byte-identical.
- **Closed loop.** Every gate failure feeds a diagnostic back into L1.
  No gate is bypassable.
- **Token cost is not a constraint.** Correctness, elegance,
  reproducibility are.
- **Differential fuzzing** is necessary, not sufficient, for translation
  acceptance — minimum 1000 random inputs per public function before any
  translation is accepted.

## Done means (M4)

- [ ] `cobrust translate <tomli-source>` produces a buildable Cobrust
      crate + provenance manifest.
- [ ] PyO3 wrapper exposes Python-compatible API.
- [ ] `tomli`'s testsuite passes against the wrapper.
- [ ] Manifest captures: source SHA, oracle versions, fuzz seeds,
      router decisions, deterministic build ID.

## Done means (M5)

- [ ] L2 + L3 gates wired up in CI.
- [ ] Second library translated (`python-dateutil` core).
- [ ] Differential-test failures auto-route to repair task in
      `mod:llm_router`.
- [ ] Benchmark harness reports against original.

## Non-goals

- Not a general-purpose Python-to-Rust transpiler. Targets Cobrust
  specifically; emitted code uses Cobrust's static core.
- Not interactive — translation is a batch pipeline.

## Cross-references

- `mod:llm_router` — translation subsystem dispatches via the router.
- `mod:frontend` — emitted Cobrust must lex+parse via the frontend.
- `mod:types` — emitted Cobrust must type-check.
- `adr:0001` — license inheritance for translated artifacts.
- Constitution `CLAUDE.md` §4.2 — pipeline definition.
