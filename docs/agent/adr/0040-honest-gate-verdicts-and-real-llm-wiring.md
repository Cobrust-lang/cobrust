---
doc_kind: adr
adr_id: 0040
title: Honest gate verdicts and real-LLM mode wiring
status: accepted
date: 2026-05-09
last_verified_commit: 36c79c5
supersedes: []
superseded_by: []
---

# ADR-0040: Honest gate verdicts and real-LLM mode wiring

## Context

The 0.1.0-beta tag shipped with two structural honesty gaps surfaced
by claude-desktop's external review (review-claude integrated handoff
2026-05-11) §1.B1 + §1.B2:

### Gap 1 — production `translate()` panics on real-LLM mode (B1)

`crates/cobrust-translator/src/pipeline.rs:529-533` (HEAD `36c79c5`):

```rust
} else {
    // Real-LLM mode. Wired at M5+ when at least one real provider has a key.
    Err(TranslatorError::Config(
        "real-LLM mode is not wired in M4 (deferred to M5 per ADR-0007)".into(),
    ))
}
```

That is technically `Err`, not `panic!()` (the handoff body uses
`panic!` shorthand for "unwired"). The actual problem is structural:
`build_router()` short-circuits the entire production path. Production
callers passing `synthetic_only = false` cannot dispatch a real LLM
call at all — yet ADR-0007 §"Synthetic-LLM mode" promises that "the
real-LLM path remains a one-flag flip" and ADR-0032 / ADR-0036 have
empirical PASS data that real-LLM mode works **outside** `translate()`
(via direct `Router::dispatch`). The gap is the `build_router` ⇄
production wiring, not the router itself.

### Gap 2 — `l2_*_summary` returns hardcoded literals (B2)

`crates/cobrust-translator/src/pipeline.rs:438-507` (HEAD `36c79c5`)
emits manifest gate strings from per-library hardcoded literals:

```rust
fn l2_build_summary(library: &PyLibrary) -> String {
    let _ = library;
    "pass (cargo build --release zero warnings)".into()
}

fn l2_behavior_summary(library: &PyLibrary, repair_attempts: u32) -> String {
    // ...
    "tomli" => format!("pass (tests/tomli_downstream.rs ...){suffix}"),
    // ...
}
```

These strings are decorative: they never consult a verifier verdict
and are returned regardless of whether any gate ran. The handoff §1.B2
calls this out as fake-pass — `gates.l2_build = "pass"` even when
`translate()` did not invoke `cargo build`, did not run the L2.behavior
fuzz harness, did not measure perf. The constitution `CLAUDE.md`
§2.4 ("no silent translations, ever") and §6 ("provenance-or-it-
didn't-happen") forbid this.

The handoff §10 interlock rule binds the two: B1 alone leaves L2
fake-PASS in real-LLM mode (the panic was masking the fake-pass);
B2 alone leaves real-LLM panic in synthetic mode. They must land in
the same PR.

## Options considered

### B1 — real-LLM router wiring

1. **Leave the `Err(Config)` short-circuit and document as "deferred
   to M-batch"** — sticks with the published "M5+ flip" promise but
   actively contradicts ADR-0032 + ADR-0036 (which already use real
   LLMs *outside* the pipeline). Rejected: production translate()
   shipping a stub is an honesty violation per constitution §2.3
   ("AI-native compiler").
2. **Wire real-LLM mode behind a cargo feature** — already in
   `Cargo.toml` as `real-llm` but the runtime gate
   (`cfg.synthetic_only`) is the load-bearing branch, not the cargo
   feature. The cargo feature changes nothing about this branch.
   Adding a `#[cfg(feature = "real-llm")]` decoration does not fix
   the runtime panic. Rejected as inadequate.
3. **Iterate `cfg.router.providers` and instantiate the matching
   adapter for each declared provider** *(chosen)* — read each
   provider's `kind` from `cobrust.toml`, register
   [`OpenAiProvider`] for `kind = "openai"` (covers DeepSeek / vLLM /
   OpenRouter / Together — they're all OpenAI-compatible per
   ADR-0004 §"Provider registry"), [`AnthropicProvider`] for
   `kind = "anthropic"`. The API key comes from each provider's
   `api_key_env`; missing or empty → structured
   `TranslatorError::Config` naming the env var. `kind = "synthetic"`
   in real-LLM mode is a Config error (per ADR-0031 §"Provider kind
   semantics" — synthetic exists only for in-process mock).

### B2 — gate verdict structure

1. **Take the verdict string as input to `l2_*_summary`** — minimal
   diff but pushes the typing burden to every caller and re-introduces
   string-typed contracts the constitution §5.1 ("Public APIs use
   newtypes, not raw primitives, where invariants exist") forbids.
   Rejected.
2. **Emit `Pass | Fail` only; treat M4-vintage "skipped (M4
   records...)" as Pass** — collapses the "no verifier was wired"
   signal. Rejected as a regression.
3. **Three-variant `GateOutcome { Pass | Fail | Skip }`** *(chosen)*
   — three structurally distinct exit paths matching how downstream
   tooling already classifies CI: green / red / yellow. `Skip` carries
   a reason so the manifest names *which* gate is wired-out-of-pipeline
   (e.g. `cargo build --release` runs at the workspace level, not
   inside `translate()`). The verifier traits (`BehaviorVerifier`,
   `PerfVerifier`) gain a `default_outcome()` method so the no-op
   `AcceptAll` / `AcceptAllPerf` advertise themselves as Skip-by-
   default rather than masquerading as Pass.

### Verdict aggregation

The repair loop already short-circuits to
`Err(TranslatorError::EscalationExceeded)` on Fail; the success path
therefore guarantees behavior + perf are Pass-or-Skip. We track
`behavior_observed_reject` / `perf_observed_reject` flags inside the
loop: a verifier that produced at least one Reject is "live" and the
final success verdict is `Pass { detail }`; a verifier that never
rejected falls back to `verifier.default_outcome()` (`Skip` for
AcceptAll, `Pass` for live no-reject verifiers — the trait default).

## Decision

Adopt both chosen options. Concrete shape:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateOutcome {
    Pass { detail: String },
    Fail { reason: String },
    Skip { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateOutcomes {
    pub l2_build: GateOutcome,
    pub l2_behavior: GateOutcome,
    pub l2_perf: GateOutcome,
    pub l3_pyo3_wrapper: GateOutcome,
    pub l3_downstream_dependents: GateOutcome,
}
```

`GateOutcome::as_manifest_str()` produces distinct prefixes per
variant (`pass (...)` / `fail: ...` / `skipped (...)`) so a downstream
parser can disambiguate without ambiguity. `GateOutcomes::worst()`
returns the worst-priority verdict (Fail > Skip > Pass) for callers
needing one aggregate exit signal.

The `BehaviorVerifier` and `PerfVerifier` traits gain a
`default_outcome()` provided method:

- Default impl returns `Pass { detail: "" }` — appropriate for live
  verifiers whose Accept-without-Reject means real Pass.
- `AcceptAll` and `AcceptAllPerf` override to return `Skip { reason:
  "<verifier name> — no <gate> gate wired" }`.

`TranslatedCrate` gains a public `gate_outcomes: GateOutcomes` field
so callers reading the typed view do not parse the manifest strings.

`build_router()` in real-LLM mode walks `cfg.router.providers` and
registers a real adapter for each declared provider:

- `kind = "openai"` → `OpenAiProvider::new(name, base_url, env(api_key_env))`
- `kind = "anthropic"` → `AnthropicProvider::new(name, base_url, env(api_key_env))`
- `kind = "synthetic"` in real-LLM mode → `TranslatorError::Config`
  per ADR-0031.
- Missing or empty `api_key_env` → `TranslatorError::Config` naming
  the env var.

The router itself remains unchanged (ADR-0004 already accepted
provider-agnostic dispatch); only the wiring inside `build_router()`
flips.

## Consequences

- **Positive**
  - The 0.1.0-stable headline contract — "production `translate()`
    can dispatch a real LLM" — is true. ADR-0032 / ADR-0036 patterns
    that ran outside the pipeline now run *through* it.
  - Manifest gate strings reflect actual verifier verdicts; the
    constitution §2.4 honesty contract holds at every gate.
  - The `GateOutcome` enum gives downstream tooling (CI, dashboards,
    `cobrust translate` CLI) a typed view that doesn't require regex
    on the manifest string.
  - `Skip` verdict cannot masquerade as `Pass`: distinct prefix means
    a missing verifier is observable in seconds with `grep -F "fail:"
    PROVENANCE.toml || grep -E "^l2_(build|behavior|perf) = \"skipped"
    PROVENANCE.toml`.

- **Negative**
  - Two existing tests (`dateutil_pipeline_repair_loop_recovers_on_attempt_2`,
    `msgpack_pipeline_perf_repair_loop_recovers_on_attempt_2`) asserted
    against the hardcoded ADR-0010 reference in
    `l3_downstream_dependents` and now assert against the typed
    `gate_outcomes.l3_downstream_dependents.is_skip()` view. The
    `manifest.gates.dependents.{covered, deferred, skipped}` structured
    section remains the authoritative dependents-coverage source of
    truth (per ADR-0009 §5).
  - Real-LLM mode now requires every declared provider to have a
    valid env var even if only one provider is on the routing path
    (we instantiate adapters eagerly to keep dispatch latency
    deterministic). Workaround: declare only the providers actually
    needed in `cobrust.toml`.

- **Neutral / unknown**
  - `default_l2_build_outcome` etc. currently return Skip for every
    library because `translate()` does not invoke `cargo build`,
    `cargo test --features pyo3`, or the L3 driver synchronously.
    A future ADR-0040.1 may add a `BuildVerifier` /
    `Pyo3WrapperVerifier` / `DownstreamVerifier` hook so these gates
    can fold into the same loop; for 0.1.0-stable, the existing
    workspace-level `cargo build` / `cargo test` runs are the
    authoritative gate, and this ADR's contract is honest about that.

## Evidence

- claude-desktop integrated handoff §1.B1 + §1.B2 + §7 + §10 —
  binding decision.
- review-claude 2026-05-11 thirteenth review §5 timeline.
- Constitution `CLAUDE.md` §2.3 ("AI-native compiler"), §2.4 ("no
  silent translations"), §5.1 ("newtypes over raw primitives"), §6
  ("provenance-or-it-didn't-happen").
- ADR-0007 §"Synthetic-LLM mode" — original "M5+ flip" promise this
  ADR redeems.
- ADR-0008 §3 + §5 — repair-loop verdict propagation pattern this
  ADR extends to all five gates.
- ADR-0009 §5 — dependents structured section that remains the
  authoritative coverage view.
- ADR-0010 §4 — `PerfVerifier` precedent for the trait shape this
  ADR generalises.
- ADR-0011 — PyO3 build path runs out-of-pipeline, justifying
  `default_l3_pyo3_outcome = Skip`.
- ADR-0031 — `ProviderKind::Synthetic` semantics that forbid
  synthetic providers in real-LLM mode.
- ADR-0032 / ADR-0036 — real-LLM evidence outside the pipeline,
  proving the router itself is sound.
- `crates/cobrust-translator/tests/pipeline_l2_gates_use_real_verdicts.rs`
  — this ADR's acceptance corpus (B1: real-LLM panic→Result; B2:
  fake-pass→Skip; B2 distinct-paths: live verifier Reject→Accept
  surfaces Pass; always-reject surfaces EscalationExceeded;
  GateOutcome serialization round-trip).
- handoff §7 acceptance command: `cargo test -p cobrust-translator
  --test pipeline_l2_gates_use_real_verdicts` must pass.
