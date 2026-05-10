---
doc_kind: finding
finding_id: audit-1-tomli-real-llm-result
last_verified_commit: c5292fc
dependencies: [adr:0032, adr:0007, adr:0008, adr:0004, finding:translator-real-vs-synthetic-status, finding:m5-m7-real-llm-validation]
---

# Finding: Audit #1 — tomli `parse_bool` real-LLM E2E result

## Hypothesis

L0 → L1 → L2.build → L2.behavior with a real LLM (user-codex `gpt-5.5`)
and a **rich prompt** carrying the Cobrust workspace API contract +
a few-shot example produces a port of `tomli_loads._parse_bool` that:

1. Compiles when glued to the workspace preamble.
2. Agrees with the CPython 3.11 oracle on 12 deterministic inputs.

## Method

- **Target**: `tomli_loads._parse_bool` (8-line Python leaf).
- **Provider**: `OpenAiProvider` at `<user-codex deployment URL>/v1` (model `gpt-5.5`).
- **Cache discipline**:
  - `SyntheticProvider` NOT registered (review-claude #1).
  - `cache_dir` = isolated `tempdir().join("llm_cache")`, verified
    non-existent pre-flight (review-claude #2).
- **Prompt** (rich, ADR-0032 §4b):
  - Verbatim Python source of `_parse_bool`.
  - Workspace API contract: `State` struct + `TomliError` constructor +
    `Value` enum (verbatim from `crates/cobrust-tomli/src/parser.rs`).
  - Few-shot example: `parse_basic_string` workspace helper.
  - Explicit return-type contract: `Result<bool, TomliError>`.
  - Prompt size: 4547 chars, 164 lines.
- **G2 gate**: synthesized minimal Cargo crate (workspace preamble +
  emitted body) → `cargo check`.
- **G3 gate**: `cargo test` driving 12 deterministic CPython 3.11
  oracle inputs through the emitted function; per-case divergence
  classified under the `@py_compat` taxonomy from constitution §2.4
  (`strict | numerical | semantic | divergent`).

## Result

**OUTCOME: PASS**

### G1 — L1 dispatch

PASS — real HTTP round-trip succeeded, response non-empty.

Ledger entry:

```json
{
  "ts": "2026-05-09T04:22:09.112594Z",
  "task": "translate",
  "provider": "user_codex_audit1",
  "provider_kind": "openai",
  "model": "gpt-5.5",
  "cache_key": "blake3:00b7e20d3720426d3e205f2ba9eb9bdf053ac19fa7a8bb95f425f83b8e5944f9",
  "cache_hit": false,
  "prompt_tokens": 1528,
  "completion_tokens": 98,
  "total_tokens": 1626,
  "latency_ms": 3313,
  "attempt": 1,
  "outcome": "ok",
  "error_code": null,
  "consensus_group": null
}
```

Cache discipline confirmed: `cache_hit` = false, `cache_dir` was an
isolated tempdir.

### G2 — L2.build (real `cargo check`)

PASS — `cargo check` exited 0; the synthesized crate (workspace preamble + LLM emission) compiles cleanly.

### G3 — L2.behavior (differential, 12 oracle inputs)


Differential outcomes (12 oracle inputs):

| Tier | Label | Buffer | Expected | Actual | Pass |
|---|---|---|---|---|---|
| `strict` | `true_at_zero` | `"true"` | `{"end_pos":4,"kind":"ok","value":true}` | `{"end_pos":4,"kind":"ok","value":true}` | PASS |
| `strict` | `false_at_zero` | `"false"` | `{"end_pos":5,"kind":"ok","value":false}` | `{"end_pos":5,"kind":"ok","value":false}` | PASS |
| `strict` | `true_then_space` | `"true "` | `{"end_pos":4,"kind":"ok","value":true}` | `{"end_pos":4,"kind":"ok","value":true}` | PASS |
| `strict` | `false_then_newline` | `"false\n"` | `{"end_pos":5,"kind":"ok","value":false}` | `{"end_pos":5,"kind":"ok","value":false}` | PASS |
| `strict` | `trueX_consumes_prefix` | `"trueX"` | `{"end_pos":4,"kind":"ok","value":true}` | `{"end_pos":4,"kind":"ok","value":true}` | PASS |
| `strict` | `falseX_consumes_prefix` | `"falseX"` | `{"end_pos":5,"kind":"ok","value":false}` | `{"end_pos":5,"kind":"ok","value":false}` | PASS |
| `strict` | `TRUE_uppercase_rejected` | `"TRUE"` | `{"kind":"err"}` | `{"kind":"err"}` | PASS |
| `strict` | `True_titlecase_rejected` | `"True"` | `{"kind":"err"}` | `{"kind":"err"}` | PASS |
| `strict` | `FALSE_uppercase_rejected` | `"FALSE"` | `{"kind":"err"}` | `{"kind":"err"}` | PASS |
| `strict` | `digit_rejected` | `"1"` | `{"kind":"err"}` | `{"kind":"err"}` | PASS |
| `strict` | `empty_rejected` | `(empty)` | `{"kind":"err"}` | `{"kind":"err"}` | PASS |
| `strict` | `true_at_offset` | `"xxtruey"` | `{"end_pos":6,"kind":"ok","value":true}` | `{"end_pos":6,"kind":"ok","value":true}` | PASS |

Tier summary:

- `strict` : 12


### G4 — Cache discipline

PASS — both axes verified:

1. Provider registry contained exactly one `OpenAiProvider`; no
   `SyntheticProvider` registered.
2. `cache_dir` was an isolated `tempfile::tempdir()` path, verified
   non-existent before dispatch.
3. Ledger entry's `cache_hit` field = `false`.

### Emitted Rust source (extracted from LLM response, verbatim)

```rust
fn parse_bool(state: &mut State<'_>) -> Result<bool, TomliError> {
    if state.bytes[state.pos..].starts_with(b"true") {
        state.pos += 4;
        return Ok(true);
    }
    if state.bytes[state.pos..].starts_with(b"false") {
        state.pos += 5;
        return Ok(false);
    }
    Err(TomliError::new("expected bool", state.pos))
}
```

### Raw LLM response (first 1500 chars)

```text
fn parse_bool(state: &mut State<'_>) -> Result<bool, TomliError> {
    if state.bytes[state.pos..].starts_with(b"true") {
        state.pos += 4;
        return Ok(true);
    }
    if state.bytes[state.pos..].starts_with(b"false") {
        state.pos += 5;
        return Ok(false);
    }
    Err(TomliError::new("expected bool", state.pos))
}
```

## Conclusion

All four gates green. The L0 → L1 → L2 closed loop, when driven by
a real LLM (`gpt-5.5` via the user-codex proxy) using a rich prompt
that includes the workspace API contract (`State`, `TomliError`) plus
a few-shot example (`parse_basic_string`), produces a Cobrust-
compatible `parse_bool` implementation that compiles AND matches
the CPython 3.11 oracle on 12 deterministic inputs.

This is the **first time** the constitution §1.2 dual mandate
("AI-native compiler that closed-loop translates the entire Python
ecosystem") has been demonstrated end-to-end — for one leaf
function, with a fresh real-LLM round-trip and zero canned-response
contamination.

## Token spend

| Phase | Calls | Tokens billed |
|-------|-------|---------------|
| L1 real dispatch | 1 | 1626 |
| Cache replay | 0 | 0 |
| **Total** | **1** | **1626** |

(prompt: 1528, completion: 98)

## Actionable consequences

1. **Audit #2** — extend the same harness to a stateful function
   (e.g. `parse_inline_table` or `parse_array`) that calls 2-3 helper
   functions. This tests whether the LLM can carry workspace context
   across a dependency chain.
2. **Production prompt design** — the rich prompt design used here
   (workspace API context + few-shot example) should replace the
   bare-bones `build_translation_prompt` in `crates/cobrust-translator/
   src/translate.rs`. Land via separate ADR; do not refactor production
   code in the audit sprint itself.
3. **ADR-0033 scope shift** — with empirical PASS data, ADR-0033
   becomes proactive (semantic-tier rigor) rather than reactive
   (fixing observed divergences). Pin the tier-classifier (`classify_divergence`)
   from this test as the canonical mapper.

## Cross-references

- ADR-0032 — sprint binding (this audit).
- ADR-0033 (future) — `@py_compat` hard-bind to L2 verifier; anchored
  on the divergence table above when this audit's outcome is not PASS.
- ADR-0007 — translator pipeline whose synthetic-only default this
  audit deliberately bypasses.
- ADR-0008 — repair loop; the divergence table above would form the
  diagnostic blob for attempt 2 if a follow-up sprint exercises it.
- `finding:translator-real-vs-synthetic-status` — the honesty gap
  this finding closes with empirical data.
- `finding:m5-m7-real-llm-validation` — the M3 wire-protocol smoke
  this audit extends to a real translation.
- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs` —
  harness implementation.
- Memory `feedback_third_party_audit_2026_05_09.md` — audit mandate.
- Memory `reference_codex_api.md` — endpoint credentials.
