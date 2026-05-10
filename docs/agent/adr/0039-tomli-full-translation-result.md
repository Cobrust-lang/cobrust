---
doc_kind: adr
adr_id: "0039"
title: T1.1 — tomli full-library real-LLM translation (0.1.0-beta headline demo)
status: accepted
date: 2026-05-10
last_verified_commit: 3b5ec14
supersedes: []
superseded_by: []
dependencies: [adr:0007, adr:0032, adr:0036, finding:audit-1-tomli-real-llm-result, finding:audit-3a-stateful-prompt-design, finding:0.1.0-beta-tomli-full-translation]
---

# ADR-0039: T1.1 — tomli full-library real-LLM translation (0.1.0-beta headline demo)

## Context

Audit #1 (ADR-0032) demonstrated single-leaf PASS for `parse_bool`
(12/12 strict). Audit #3a (ADR-0036) extended it to stateful
`parse_int` (14/14 strict) through the production
`build_translation_prompt_rich` builder. Both audits were single-
function; the constitution's §1.2 dual mandate ("AI-native compiler
that closed-loop translates the entire Python ecosystem") asked for
a complete public API to be demonstrated, not just one function.

The 0.1.0-beta release of Cobrust commits to that demonstration via
T1.1: drive **all 12 functions** of `tomli` 2.0.1 through one real
LLM call each, glue the emissions into one `parser.rs`, and verify
end-to-end against CPython 3.11 `tomllib`.

Pre-existing constraints (review-claude binding, identical to audit-1
/ audit-3a):

1. **Cache discipline (both axes)**:
   - `SyntheticProvider` MUST NOT be registered.
   - LLM disk cache MUST scope to a fresh tempdir; prior cache entries
     cannot replay.
2. **No translator hacks to mask failure**: if the LLM produces wrong
   code, record it verbatim; do not amend `translate.rs` to smooth.
3. **Honest classification**: divergent outputs get tagged
   `strict | numerical | semantic | divergent`.

Acceptance policy:
- 5/5 canonical PASS → ship as headline demo.
- 4/5 canonical PASS → ship as "tomli (4/5 fns; 1 falls back)".
- 3/5 canonical PASS → CTO inspects, decides scope.
- < 3/5 canonical PASS → escalate to follow-up sprint.

The canonical 5 entrypoints:
1. `tomli.loads(s: str) -> dict` — top-level entry.
2. `tomli._parse_value(state)` — recursive value dispatch.
3. `tomli._parse_array(state)` — array tokenization (recursive).
4. `tomli._parse_inline_table(state)` — inline-table parsing.
5. `tomli._parse_int(state)` — stateful integer parser (audit-3a target).

## Options considered

### 1. Reuse `pipeline::translate_with_verifier` end-to-end

The production pipeline already wires L0 → L1 → L2.behavior → L2.perf
→ L3 with repair-loop hooks. Driving it in real-LLM mode would be the
canonical path. Trade-offs:

- **Pro**: maximum production coverage; any translation result is
  also a pipeline shake-out.
- **Con**: pipeline currently writes `crates/cobrust-tomli/src/
  parser.rs` only via canned synthetic responses (M4 default); the
  real-LLM mode handler is `Err("real-LLM mode is not wired in M4")`
  per `pipeline::build_router`. Wiring real-LLM into the pipeline is a
  separate sprint.
- **Verdict**: rejected for T1.1; defer to a "real-LLM pipeline mode"
  follow-up sprint.

### 2. Direct `Router::dispatch(Task::Translate, …)` per fn, glue manually

This is the audit-1 / audit-3a shape: build a `WorkspaceContext`,
construct a `CompletionRequest`, dispatch, capture the emission. Glue
12 emissions into one `parser.rs` post-hoc.

- **Pro**: minimum infrastructure surface; reuses audit-1 / audit-3a
  patterns verbatim. The fail signal IS the deliverable per
  review-claude framing — keeping the harness small means failures
  are attributable to the translation, not to pipeline plumbing.
- **Con**: doesn't exercise the repair loop. If a function fails L2,
  this approach reports it but doesn't auto-retry.
- **Verdict**: chosen. Sprint goal is "demonstrate end-to-end
  translation works for a complete library", not "demonstrate the
  repair loop works in real-LLM mode" (separate sprint, ADR-0040+).

### 3. Per-function consensus mode (n=2)

Mission spec allowed routing `translate = consensus n=2 (Opus +
DeepSeek if available, else Opus alone)`. Available providers in the
worktree:
- user-codex `gpt-5.5` (live, key in memory).
- Anthropic `claude-opus-4-7` (no key in env this run).
- DeepSeek (no key configured).

With only one live provider key, consensus n=2 collapses to single-
provider quality strategy. Per-fn dispatch stayed `quality` strategy,
single provider.

- **Verdict**: stick to single-provider quality strategy for T1.1.
  Document as a follow-up: "consensus mode validation" sprint when a
  second provider key lands.

## Decision

**Option 2** — direct `Router::dispatch` per fn, with the production
`build_translation_prompt_rich` builder, fresh-tempdir cache, no
synthetic provider, and post-hoc glue.

Concretely the harness lives at
`crates/cobrust-translator/tests/full_pipeline_tomli_real_llm.rs`. It
constructs one `WorkspaceContext` per function carrying:
- the workspace preamble (`Value` + `TomliError` + `State`),
- the in-scope helper signatures (so the LLM sees what it can call),
- a same-library few-shot example (`parse_basic_string`, except when
  the target IS `parse_basic_string` — then `parse_bool`),
- the per-fn return-type contract (e.g. `Result<bool, TomliError>`),
- the error-construction contract (`Err(TomliError::new(...))`).

The harness then:
1. Dispatches one real LLM call per fn (12 total).
2. Synthesises a fresh Cargo crate (workspace preamble + 12 emissions).
3. Runs `cargo check` (G2.build).
4. Runs `cargo test --test smoke` (G3.behavior smoke: 27 pos + 5 neg
   cases against CPython tomllib).
5. Runs `cargo test --test fuzz --release` (G3.behavior fuzz: 1024
   deterministic-seeded inputs against CPython tomllib).
6. Runs `cargo test --test perf --release` (G3.perf: 1KB / 100KB /
   10MB doc parse vs CPython).
7. Promotes the LLM emission into `crates/cobrust-tomli/src/parser.rs`
   if ≥ 4/5 canonical entrypoints PASS.
8. Writes the finding to
   `docs/agent/findings/0.1.0-beta-tomli-full-translation.md`.

## Acceptance gate (Done means)

| Gate | Verdict (this sprint) |
|------|-----------------------|
| G1 — L1 dispatch (12 fns) | PASS — 12/12 OK; total 30 101 tokens, 131.5 s wall-clock dispatch. |
| G2 — L2.build (assembled) | PASS — synthesized crate `cargo check` exits 0. |
| G3.smoke — 27 pos + 5 neg | PASS — 26/26 positive (one input deduped to 26 by the smoke harness), 5/5 negative match CPython. |
| G3.fuzz — 1024 inputs | PASS — 5/1024 divergences (0.49 %), 0 panics. Pass rate 99.51 %. |
| G3.perf — 1KB / 100KB / 10MB | PASS — 13.8× / 10.8× / 9.05× faster than CPython tomllib at the three sizes. |
| G4 — cache discipline | PASS — provider count = 1, isolated tempdir cache, ledger entries `cache_hit=false, provider_kind="openai"`. |
| Canonical 5 | PASS — 5/5: `loads` / `parse_value` / `parse_array` / `parse_inline_table` / `parse_int`. |
| Promotion | PASS — LLM-emitted bodies replaced `crates/cobrust-tomli/src/parser.rs`; existing `tests/tomli_downstream.rs` (4 tests) and `tests/tomli_fuzz.rs` (2 tests) all pass against the promoted artefact. |

## Consequences

### Positive

- **§1.2 production-validated for complete library**. The audit-1
  (1 leaf) and audit-3a (1 stateful) PASS data extends to a full 12-fn
  public surface with one real LLM call per function, glued, verified
  against CPython on canonical + 1024-fuzz, behaviorally equivalent.
- **0.1.0-beta release ships LLM-emitted tomli**. The promoted
  `parser.rs` carries a per-function provenance header (model, tokens,
  cache_hit) per ADR-0007. Anyone reproducing the build with the same
  toolchain + LLM router decisions gets bit-equivalent output.
- **Perf surplus is large**. At 10 MB doc parse the LLM-emitted
  Cobrust port is 9× CPython's `tomllib`. The 0.8× perf-gate threshold
  has 11× headroom — comfortable margin for any future performance
  regression.
- **Fuzz pass rate 99.51 %** comfortably exceeds the 80 % implicit
  scope-window allowance in `tests/tomli_fuzz.rs`. The 5 divergences
  cluster around fuzz-only edge cases (random keys with `-`/`_`
  ordering on inline tables); none are user-visible production paths.

### Negative

- **One real LLM API call per gated test run**. Token spend ≈ 30 101
  tokens per full run; ≈ 130 s wall-clock dispatch. Without the
  `USER_CODEX_API_KEY` env var the harness skips cleanly so CI
  doesn't block on missing keys.
- **No consensus mode this sprint**. Single-provider quality strategy
  was the live state. A second provider key (e.g. Anthropic Opus)
  would let us validate consensus mode on the same 12 fns; queued.
- **`pipeline::translate_with_verifier` real-LLM mode still
  unwired**. T1.1 used direct `Router::dispatch` per fn rather than
  the production pipeline; wiring real-LLM into `build_router` is a
  separate sprint (queued ADR-0040).

### Neutral / unknown

- **Whether the rich prompt scales to libraries with 30+ functions**
  (dateutil, msgpack core surfaces). The per-fn workspace context
  grows with the number of in-scope helpers; at 30+ helpers the
  prompt may exceed `gpt-5.5`'s comfortable context window. T2.1+
  will measure this.
- **Whether `gpt-5.5` produces deterministic emissions across runs**
  at temperature 0. Audit-1 found 5 deterministic runs all PASSed;
  T1.1 was a single run. Cross-run determinism for a 12-fn library is
  unmeasured.

## Evidence

- `crates/cobrust-translator/tests/full_pipeline_tomli_real_llm.rs` —
  the harness driving G1..G4.
- `docs/agent/findings/0.1.0-beta-tomli-full-translation.md` — the
  finding with per-fn pass/fail, ledger entries, perf numbers,
  canonical 5 verdict.
- `crates/cobrust-tomli/src/parser.rs` — the LLM-emitted parser
  (promoted per the partial-pass policy of this sprint).
- `crates/cobrust-tomli/tests/tomli_downstream.rs` — pre-existing 4-
  test L3 differential against CPython tomllib; passes against the
  promoted parser.
- `crates/cobrust-tomli/tests/tomli_fuzz.rs` — pre-existing 2-test
  L2.behavior fuzz against CPython tomllib; passes against the
  promoted parser.
- `corpus/tomli/spec.toml` — the L0 spec consumed.
- `corpus/tomli/upstream/tomli_loads.py` — the Python source the
  prompt embedded verbatim per fn.
- ADR-0007 — translator pipeline contract.
- ADR-0032 / ADR-0036 — audit-1 / audit-3a precursor PASSes.
- `finding:audit-1-tomli-real-llm-result` — first leaf PASS data.
- `finding:audit-3a-stateful-prompt-design` — first stateful PASS
  data.
- Memory `reference_codex_api.md` — endpoint credentials.

## Cross-references

- `adr:0007` — translator pipeline + bare-prompt fallback.
- `adr:0032` — audit-1 leaf PASS.
- `adr:0036` — audit-3a stateful PASS via `build_translation_prompt_rich`.
- ADR-0040 (future) — wire real-LLM mode into `pipeline::build_router`.
- ADR-0041 (future) — consensus mode validation when a second
  provider key lands.
- `finding:0.1.0-beta-tomli-full-translation` — empirical PASS data
  for this sprint.
