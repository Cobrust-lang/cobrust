---
doc_kind: adr
adr_id: "0032"
title: audit-1 tomli real-LLM E2E (first closed-loop run)
status: accepted
date: 2026-05-09
last_verified_commit: TBD
supersedes: []
superseded_by: []
dependencies: [adr:0007, adr:0004, finding:translator-real-vs-synthetic-status, finding:m5-m7-real-llm-validation]
---

# ADR-0032: Audit #1 — tomli real-LLM E2E (first closed-loop translation run)

## Context

### Audit provenance

Third-party audit (`review-claude`, 2026-05-09, `originSessionId:
96e2d0dc-a026-485b-a4bf-3ea3b21d1b5d`) issued a 7-point review of
Cobrust at HEAD `cc15f0b`. Finding #2 (captured in
`finding:translator-real-vs-synthetic-status`) identified the
critical honesty gap: the L0 → L1 → L2 → L3 closed loop has
**never** run end-to-end on a real Python library through a real LLM
dispatch. All M4 through M-batch "passes" are synthetically authored
canned responses that were designed to pass the gates they're
tested against.

The M3 real-LLM smoke (`finding:m5-m7-real-llm-validation`) validated
the wire protocol with a single hello-world dispatch. It did not
translate a Python library.

### The honesty gap

Constitution §5.2 defines "scientific" as:

> Every claim of "faster" or "safer" cites the experiment file.
> All AI translation outputs include a verification manifest.
> Negative results are documented under `docs/agent/findings/`, not hidden.

The §1.2 dual mandate claims an "AI-native compiler that closed-loop
translates the entire Python ecosystem." The pipeline plumbing is real
(router + cache + ledger + ADR-0007 gate contract) but the integration
claim — that a real LLM can translate a real Python function, pass the
gates, and converge — has zero empirical evidence.

### Why now

M12.x (ADR-0027) has landed: the language compiler now handles real
Cobrust programs. The audit sprint is therefore unblocked and has been
prioritised per the audit's recommended execution order (item #3).

### Constraints from review-claude

1. Both caches must be bypassed:
   - `SyntheticProvider` must NOT be used (no canned responses).
   - LLM disk cache (`~/.cobrust/llm_cache/` or `.cobrust/llm_cache/`)
     must point to an isolated tempdir so prior entries are invisible.
2. Fail signal is the value: if the real LLM produces wrong code or
   fails to compile, that honest failure becomes the anchor for
   ADR-0033 (`@py_compat` hard-bind to L2 verifier). A pass
   demonstrates §1.2 is real, not synthetic.
3. Do not modify the translator to hide a failure. The finding must
   record the actual gate outcome.

## Options considered

### 1. Translate the full 12-function tomli spec via real LLM

- Pros: exercises the full L0..L2 loop end-to-end.
- Cons: 12 functions × 1 real-LLM call each = ~12 API calls, ~15–20
  min wall-clock, high token spend. High blast radius if the first
  real-LLM call produces unusable code — the pipeline would stall
  waiting for 12 functions to all fail.
- Verdict: too wide for a first audit sprint.

### 2. Translate one large, stateful function (`loads` or `parse_value`)

- Pros: exercises the full dispatch + L2.behavior path.
- Cons: `loads` calls 8+ helpers — the generated stub won't compile
  unless the helpers are provided by the canned-responses file too.
  We'd be back to mixing synthetic + real providers.
- Verdict: caller-side complexity too high for a clean audit.

### 3. Translate one small, leaf, deterministic function (`parse_bool`)

`parse_bool` is the clearest candidate:

- **Pure function**: takes a state cursor, returns `bool`.
- **Leaf node**: no calls to other helper functions.
- **Deterministic**: input `"true"` → `true`, `"false"` → `false`,
  anything else → error. Zero floating-point, zero I/O.
- **10+ oracle inputs trivially generated**: true/false/TRUE/FALSE/
  yes/no/1/0/"" and position-offset variants give unambiguous
  expected values.
- **Short Python source** (7 lines): fits cleanly in a single
  completion request without overflow.
- **Clear signature**: `_parse_bool(state: _State) -> bool`.

This is **Decision #3**.

## Decision

Translate **`tomli::parse_bool`** (Python qualname:
`tomli_loads._parse_bool`) via one real LLM call to the user-codex
endpoint (`http://104.244.92.250:8317/v1`, model `gpt-5.5`,
OpenAI-compatible wire), with:

1. **No `SyntheticProvider`** — construct `OpenAiProvider` directly
   with the codex endpoint credentials from `USER_CODEX_API_KEY` env.
2. **Isolated cache_dir** — `tempfile::tempdir().join("llm_cache")`,
   cleared per test invocation, so prior hello-world entries cannot
   be replayed.
3. **Isolated ledger_path** — same tempdir scope.
4. **L0 spec**: read from `corpus/tomli/spec.toml` (existing).
5. **L1**: dispatch via `run_l1` — one real-LLM completion request
   carrying the function signature + description from spec.toml.
6. **L2.build**: the emitted Rust text must parse as valid
   `fn parse_bool(...)` — verified by attempting to inject it into a
   minimal harness that compiles.
7. **L2.behavior**: compare the emitted function's output against
   CPython 3.11 `tomllib` oracle on 10+ inputs (all deterministic).

## Acceptance gate

All four sub-gates must be reported (pass or honest fail):

| Gate | What it checks |
|------|---------------|
| G1 — L1 dispatch | Real HTTP round-trip to codex endpoint; response non-empty; ledger records `cache_hit=false`. |
| G2 — L2.build | Emitted text is syntactically valid Rust (no obvious parse errors); compiles in a harness. |
| G3 — L2.behavior (10 inputs) | `parse_bool` agrees with CPython oracle on 10 deterministic inputs. |
| G4 — Cache discipline | `cache_hit=false` in ledger; `SyntheticProvider` not registered; cache_dir is an isolated tempdir path. |

**Pass**: all four green → §1.2 dual mandate demonstrated for a single
leaf function via a real LLM round-trip.

**Partial pass / fail**: any gate red → honest failure. The failure
diff (expected vs actual on failing inputs) becomes the anchor for
ADR-0033 (`@py_compat` hard-bind to L2 verifier).

## Consequences

### Positive

- First empirical data point on whether the translation pipeline can
  produce correct code from a real LLM (versus canned responses).
- Finding `audit-1-tomli-real-llm-result.md` produced regardless of
  pass/fail — §5.2 honesty preserved.
- If PASS: ADR-0033 can anchor on the observed "works for a leaf
  function; scale to wider functions in Audit #2."
- If FAIL: ADR-0033 anchors on the concrete diff and maps which gate
  fails first, giving the repair-loop team a real diagnostic to fix.

### Negative

- One real LLM API call per test run (when `USER_CODEX_API_KEY` set);
  default `cargo test --workspace` is unaffected (skip discipline).
- Token spend: ~1 call × ~1000 tokens (prompt + completion) per run.
- The chosen function (`parse_bool`) is the smallest in the spec;
  a PASS here does not guarantee larger functions will pass.

### Neutral / unknown

- Whether the `gpt-5.5` proxy at `104.244.92.250:8317` can translate
  Python to idiomatic Rust without additional few-shot examples. This
  is the whole question being answered.
- If the endpoint is unreachable, the test skips cleanly and the
  finding stub records `OUTCOME: SKIP (endpoint unreachable)` — still
  honest.

## Evidence

- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs` —
  the test harness for this ADR.
- `docs/agent/findings/audit-1-tomli-real-llm-result.md` — the
  concrete result (populated by the test run).
- `finding:translator-real-vs-synthetic-status` — the gap this ADR
  closes.
- `finding:m5-m7-real-llm-validation` — the M3 wire-protocol smoke
  this ADR extends to a real translation.
- Memory: `feedback_third_party_audit_2026_05_09.md` — audit that
  mandated this sprint.
- Memory: `reference_codex_api.md` — codex endpoint credentials.
</content>
</invoke>