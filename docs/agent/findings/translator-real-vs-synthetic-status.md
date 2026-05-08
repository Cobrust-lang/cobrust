---
doc_kind: finding
finding_id: translator-real-vs-synthetic-status
last_verified_commit: cc15f0b
dependencies: [adr:0007, adr:0008, adr:0009, adr:0010, adr:0022]
---

# Finding: Translation subsystem real-LLM end-to-end has never run on a real Python library

## Hypothesis

When CTO claimed the AI translation subsystem (constitution §1.2) was
"delivered" through M4 (tomli), M5 (dateutil + L2.perf + repair), M6
(msgpack + native ext), and M-batch (requests + click), the implicit
claim was that the L0 → L1 → L2.{build,behavior,perf} → L3 closed
loop has been exercised end-to-end at least once on a real Python
library through a real LLM dispatch.

## Method

Third-party audit (`review-claude` 2026-05-09) read:
- `crates/cobrust-translator/src/{lib,synthetic}.rs`
- `findings/m5-m7-real-llm-validation.md`
- All M4..M-batch translated crates (cobrust-tomli, dateutil, msgpack,
  requests, click) — their `corpus/<lib>/canned_llm_responses.toml`
  files.
- `crates/cobrust-llm-router/tests/real_llm_smoke.rs`.
- `cobrust-translator/Cargo.toml` for the `--features real-llm` gate.

CTO independently re-confirms with `find crates -name '*real*' -type f`:

```
crates/cobrust-llm-router/tests/real_llm_smoke.rs
docs/agent/findings/m5-m7-real-llm-validation.md
```

(plus per-test scaffolding files inherited from M5/M6).

## Result

1. **The translation pipeline default is `SyntheticProvider`.** Per
   ADR-0007 §"Synthetic-LLM mode default" + every M4..M-batch
   PROVENANCE.toml's `[router] strategy = "synthetic"`.

2. **The "canned LLM responses" are hand-written by the P9 agent (or
   by CTO during recovery) and committed into `corpus/<lib>/canned_llm_responses.toml`.**
   These are NOT actual LLM outputs replayed for determinism — they
   are human-authored Cobrust source pretending to be LLM outputs,
   keyed by the canonical-prompt-hash that the synthetic provider
   uses for lookup.

3. **The "real-LLM smoke test"
   (`crates/cobrust-llm-router/tests/real_llm_smoke.rs`) is a single
   hello-world dispatch.** It dispatches `{ task: Task::Translate,
   prompt: "Reply with the single word: ok" }` to the user-codex
   endpoint. It validates the wire protocol (provider response,
   ledger entry, cache replay, transport-failure isolation) — but
   not a translation.

4. **Therefore the L0 → L1 → L2 → L3 closed loop has never run
   end-to-end against a real Python library through a real LLM.**
   M4 tomli "passes" because the canned responses were authored to
   pass the gates. The L2.behavior gate compares the cobrust-tomli
   output to `corpus/tomli/upstream_tests/test_*.py` — which
   succeeds because the canned Cobrust source is a hand-written
   port of the Python source, by definition behaviorally equivalent.

5. **The user-codex endpoint at `http://104.244.92.250:8317/v1`
   (per `reference_codex_api.md`) has been live + reachable since
   2026-05-08 with `gpt-5.5` model exposed.** No translation pipeline
   run has been dispatched against it.

## Conclusion

The constitution's §1.2 dual mandate ("AI-native compiler with
translation subsystem that uses LLMs as a first-class component to
convert Python libraries into Cobrust under closed-loop verification")
has shipped:

- Pipeline plumbing ✓ (router + cache + ledger + ADR-0007 gate
  contract).
- Closed-loop verification mechanism ✓ (L2.behavior + L2.perf + L3
  + repair loop + escalation per ADR-0008).
- Translation outputs ✓ (5 cobrust-* libraries shipping; tests pass).

But the **integration claim** — that the pipeline can take a real
Python library it has never seen, dispatch real LLM calls to translate
it, run the L2.behavior + L2.perf gates against the live LLM output,
trigger L1 retry on gate failures, and converge on a passing
translation — **has not been demonstrated**. It might work. It might
not. The synthetic-mode-default architecture means the project hasn't
needed to find out.

This is a clean instance of constitution §5.2 "Negative results are
documented under findings/, not hidden." It is being documented now
(this commit) because the third-party audit surfaced it.

## Actionable consequences

1. **Land an end-to-end real-LLM tomli translation as a separate
   sprint.** Recommended after M12.x merges (so the language can
   actually express the verification harness without literal-print
   workarounds).
   - Use `tomli` as the target (smallest, pure-Python, no native ext
     — minimal surface for the first real run).
   - Configure `cobrust-translator` with `[providers.user_codex]` per
     `reference_codex_api.md`.
   - Run `cobrust translate corpus/tomli` with `--features real-llm`.
   - Allow gates to fail. Document the failures.
   - Output: `findings/m4-tomli-real-llm-end-to-end.md` with full
     ledger + LLM-emitted Rust diff vs the existing canned Rust +
     gate verdict. Pass or fail, the result is publishable.

2. **Provider taxonomy in ledger.** The current `ledger.jsonl`
   schema (per ADR-0004) records `provider` (the registered name)
   but NOT `provider_kind` (synthetic vs anthropic vs openai vs
   user_codex). When mixing synthetic and real providers in a single
   pipeline run, post-hoc analysis cannot tell which calls were real.
   Schema bump: add `provider_kind` field; bump ADR-0004 to v2.
   Sprint cost: ~1h.

3. **Public messaging alignment.** Until #1 lands, internal docs
   (this file + ADR-0007 §"Synthetic-LLM mode default" + the M4..M-
   batch PROVENANCE.toml manifests) should be the authoritative
   source. README.md and any external messaging should describe the
   pipeline as "designed and instrumented; first real-LLM end-to-end
   run pending finding `m4-tomli-real-llm-end-to-end.md`."

4. **Repair loop validation in the wild.** ADR-0008's repair loop
   has been validated synthetically (`crates/cobrust-translator/tests/
   dateutil_pipeline.rs::dateutil_pipeline_repair_loop_recovers_on_attempt_2`).
   It has NOT been validated under real-LLM gate failures, where the
   diagnostic feedback to the LLM is genuinely opaque-typed
   (text-prompt-engineering rather than canned-response-table
   lookup). The first tomli E2E run will produce this evidence.

## Cross-references

- Constitution `CLAUDE.md` §1.2 (AI-native compiler), §5.2 (negative
  results under findings/).
- ADR-0007 §"Synthetic-LLM mode default" — the deferral was always
  intended to be temporary; this finding is the first formal
  acknowledgment that "temporary" has lasted M4 → M-batch.
- ADR-0008 (L2.perf + repair loop) — repair exists, hasn't seen real
  diagnostic feedback.
- `findings/m5-m7-real-llm-validation.md` — the wire-protocol smoke
  test (single hello-world dispatch).
- `reference_codex_api.md` (memory) — live user-codex endpoint, ready
  for the first real translation dispatch.
- Third-party audit `review-claude` 2026-05-09.
