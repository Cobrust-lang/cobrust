---
doc_kind: adr
adr_id: "0032"
title: Audit #1 — tomli real-LLM E2E (first closed-loop translation run)
status: accepted
date: 2026-05-09
last_verified_commit: TBD
supersedes: []
superseded_by: []
dependencies: [adr:0007, adr:0008, adr:0004, finding:translator-real-vs-synthetic-status, finding:m5-m7-real-llm-validation]
---

# ADR-0032: Audit #1 — tomli real-LLM E2E (first closed-loop translation run)

## Context

### Audit provenance

Third-party `review-claude` (2026-05-09, originSessionId
`96e2d0dc-a026-485b-a4bf-3ea3b21d1b5d`) issued a 7-point audit at
HEAD `cc15f0b`. Finding #2
(`finding:translator-real-vs-synthetic-status`) identified the
critical honesty gap: the L0 → L1 → L2 → L3 closed loop has
**never** run end-to-end against a real Python library through a
real LLM. Every "passed" translation in M4..M-batch is a
hand-authored canned response served via `SyntheticProvider`.

The M3 wire-protocol smoke (`finding:m5-m7-real-llm-validation`)
validated the router with one hello-world dispatch. It did not
translate code.

### The gap to close

Constitution §1.2 ("AI-native compiler with translation subsystem
that uses LLMs as a first-class component") and §5.2 (scientific
discipline: "All AI translation outputs include a verification
manifest") demand that the integration claim be empirically
demonstrated, not asserted. The audit memo's verdict was sharp:

> 宪法 §5.2 "scientific" 是这个项目最严肃的承诺。不要让翻译子系统的真实
> 状态和外部声明形成长期 gap——那才是真正的技术债。

This sprint closes the gap by running one real LLM translation
end-to-end with maximum cache discipline. **The fail signal IS the
deliverable**: a partial-pass or fail with concrete diff data is
strictly more valuable than a green-but-unverifiable pass would be.

### Constraints (review-claude binding)

1. **Cache discipline (both axes)**: `SyntheticProvider` must NOT
   be registered, AND the LLM disk cache must scope to a fresh
   tempdir so prior `real_llm_smoke.rs` entries cannot replay.
2. **No translator hacks to mask failure**: if the LLM produces
   wrong code, record it verbatim; do not amend `translate.rs` to
   smooth the result.
3. **Honest classification**: differential outcomes get tagged
   `strict | numerical | semantic | divergent` — this taxonomy
   becomes the anchor for ADR-0033 (`@py_compat` hard-bind).

## Options considered

### 1. Translate the full 12-function tomli spec

- **Pros**: exercises every gate end-to-end at scale.
- **Cons**: 12 real LLM calls per run (~10–15 min wall-clock,
  ~12k tokens); high blast radius — one bad emission can stall the
  pipeline indefinitely; first-fail attribution is muddied by
  cross-function dependencies (the canned `loads` calls 8 helpers).
- **Verdict**: too wide for a first audit; deferred to Audit #2.

### 2. Translate one stateful core function (`loads` or `parse_value`)

- **Pros**: exercises the dispatch path on non-trivial code.
- **Cons**: both call multiple helpers. Without those helpers
  also coming from a real LLM, the emitted code wouldn't compile;
  with them, we re-introduce synthetic mode for everything except
  the focal function — defeating the audit's purpose.
- **Verdict**: contamination risk too high.

### 3. Translate one small leaf function (`parse_bool`)

`parse_bool` is the cleanest candidate by every criterion:

- **Pure leaf**: depends only on `state.peek()` / `state.pos +=`,
  no calls to other helpers in `tomli_loads.py`.
- **Tiny source**: 8 Python lines (~120 tokens). Fits any model.
- **Deterministic, unambiguous semantics**: input `"true"` →
  `Some(true)`, input `"false"` → `Some(false)`, anything else
  → error. Zero floating-point, zero I/O, zero ambiguity.
- **Easy oracle**: 12+ deterministic CPython 3.11 inputs trivially
  enumerable.
- **Spec already exists**: `corpus/tomli/spec.toml [function.parse_bool]`
  has been pinned since M4.

**Decision**: option 3.

### 4. Prompt design: bare-bones vs. rich context

A naive prompt template would just say "Translate this Python
function." The existing `crates/cobrust-translator/src/translate.rs::build_translation_prompt`
emits exactly this. But the workspace's Rust convention for
`parse_bool` is `Result<bool, TomliError>` — not the natural Python
return type `bool`. Without context, the LLM cannot know about the
`State` struct field names, the `TomliError::new(message, pos)`
constructor, or the workspace error-propagation idiom.

Two sub-options:

- **4a (bare)**: send only signature + description.
- **4b (rich)**: send signature + description + workspace API
  context (`State` struct definition, `TomliError` constructor,
  `parse_basic_string` as a few-shot example of similar workspace
  style).

**Decision**: 4b. The bare prompt is the existing translator
default; testing it would just re-confirm sonnet's earlier finding
that it under-specifies. The rich prompt represents what production
real-LLM mode would actually use, and gives the audit its strongest
pass-vs-fail signal.

The rich prompt is built **inline in the test**, not by modifying
`translate.rs::build_translation_prompt`. This keeps the audit
non-invasive — no production code changes, just a stronger test
harness that calls a richer dispatch path.

## Decision

Translate `tomli_loads._parse_bool` via one real LLM call to the
user-codex endpoint with:

1. **Provider**: `OpenAiProvider` constructed directly with the
   codex `base_url` + `USER_CODEX_API_KEY` env. NO
   `SyntheticProvider` registered.
2. **Cache_dir + ledger_path**: `tempfile::tempdir()` — fresh per
   test invocation.
3. **Prompt (rich)**: signature + description + workspace `State`
   struct + `TomliError` constructor + `parse_basic_string` as
   few-shot example + explicit return-type contract `Result<bool,
   TomliError>`.
4. **L1**: dispatch via direct `Router::dispatch(Task::Translate,
   req)` (one round-trip).
5. **L2.build (real)**: write the emitted Rust into a synthesized
   minimal crate alongside the workspace preamble (the canned
   `State` + `TomliError` definitions). Run `cargo check
   --message-format=short`. Pass = compiles with zero errors.
6. **L2.behavior (differential)**: if L2.build passes, run
   `cargo test` against a harness that calls the emitted function
   on 12 deterministic inputs and asserts each output matches the
   CPython 3.11 oracle.
7. **Semantic-tier classification**: each diverging output gets
   classified `strict | numerical | semantic | divergent`. Even on
   PASS, the tier classification is recorded.

## Acceptance gate (Done means)

All four sub-gates reported, pass or honest fail:

| Gate | What it checks |
|------|---------------|
| G1 — L1 dispatch | Real HTTP round-trip non-empty; ledger records `cache_hit=false, outcome="ok"`. |
| G2 — L2.build | Synthesized crate `cargo check`s with zero errors. |
| G3 — L2.behavior (12 inputs) | Emitted function output matches CPython oracle on each input; divergences classified by tier. |
| G4 — Cache discipline | `cache_hit=false` in ledger; `SyntheticProvider` not registered; `cache_dir` is an isolated tempdir. |

**Outcome reporting**:
- All 4 green → §1.2 demonstrated for one leaf function. Audit #2
  can extend to a stateful function.
- G1+G4 green, G2 red → LLM produced uncompilable Rust. Anchor
  for ADR-0033 (prompt-context hard-bind).
- G1+G2+G4 green, G3 red → LLM produced compiling but
  behaviorally divergent code. Strongest possible anchor for
  ADR-0033 (semantic tier hard-bind).
- Any G1 / G4 red → infrastructure bug. Stop and fix before
  re-running.

## Consequences

### Positive

- First empirical answer to "can the AI translation subsystem
  produce correct Cobrust from a real LLM" — pass or fail.
- Finding `audit-1-tomli-real-llm-result.md` written regardless of
  outcome. §5.2 honesty is preserved structurally.
- The rich-prompt design becomes a reference for production
  real-LLM mode. The bare-prompt (current default) is documented as
  insufficient by demonstration, not assertion.
- Semantic-tier classification produces the `@py_compat` taxonomy
  that ADR-0033 will hard-bind into the L2 verifier.

### Negative

- One real LLM API call per gated test run (skipped without
  `USER_CODEX_API_KEY`). Token spend ~1k–2k per run.
- Decision is for one leaf function only. A pass here does NOT
  generalize to stateful functions; Audit #2 must extend.
- The synthesized cargo-check crate is created in a tempdir each
  run; cold cargo cache adds ~10–20 s wall-clock on top of the LLM
  round-trip.

### Neutral / unknown

- Whether `gpt-5.5` (a multi-provider proxy model on the
  user-codex endpoint) can produce idiomatic Rust on first try —
  this is the audit's central empirical question.
- If the endpoint is unreachable, the test still produces a finding
  (with `OUTCOME: SKIP`) so the harness's correctness is verified
  even on network failure.

## Evidence

- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs` —
  the integration test driving G1..G4.
- `docs/agent/findings/audit-1-tomli-real-llm-result.md` —
  populated by the test run with concrete pass/fail data.
- `finding:translator-real-vs-synthetic-status` — the gap this
  ADR closes.
- `finding:m5-m7-real-llm-validation` — the M3 wire-protocol
  smoke this ADR extends to a real translation.
- `corpus/tomli/spec.toml [function.parse_bool]` — the L0 source
  for the prompt's signature + description.
- `corpus/tomli/upstream/tomli_loads.py:117–126` — Python source
  embedded verbatim into the prompt.
- `crates/cobrust-tomli/src/parser.rs:130–155, 217–247` —
  workspace `parse_basic_string` reference used as few-shot
  example in the prompt; `State` + `TomliError` preamble copied
  into the synthesized G2 crate.
- Memory: `feedback_third_party_audit_2026_05_09.md` — audit
  mandate.
- Memory: `reference_codex_api.md` — endpoint credentials.
- Memory: `feedback_subagent_model_tier.md` — Opus-tier
  authority to author this ADR.

## Cross-references

- `adr:0007` — translator pipeline + synthetic-mode default this
  audit deliberately bypasses.
- `adr:0008` — repair loop (used as background context for ADR-0033
  scope when this audit's L2.behavior fails).
- `adr:0004` — LLM router contract (cache + ledger schema this
  audit relies on).
- ADR-0033 (future) — `@py_compat` hard-bind into L2 verifier;
  anchored on this audit's divergence taxonomy.
</content>
</invoke>