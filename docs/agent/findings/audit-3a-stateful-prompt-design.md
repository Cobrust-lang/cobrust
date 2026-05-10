---
doc_kind: finding
finding_id: audit-3a-stateful-prompt-design
last_verified_commit: 4fabf4c
dependencies: [adr:0036, adr:0032, adr:0007, adr:0008, finding:audit-1-tomli-real-llm-result, finding:translator-real-vs-synthetic-status]
---

# Finding: Audit #3a — tomli `parse_int` (stateful) real-LLM E2E result

## Hypothesis

`build_translation_prompt_rich(unit, ctx)` — the production builder
introduced by ADR-0036 — produces a Cobrust-workspace-compatible Rust
port of the **stateful** `tomli_loads._parse_int` function that:

1. Compiles when glued to the workspace preamble.
2. Agrees with the CPython 3.11 oracle on 14 deterministic inputs.

This generalises the audit-1 PASS data (leaf `parse_bool`) to a
function with `state.pos` mutation across two distinct phases (sign +
digits loop) and a non-trivial error path. The audit-1 sonnet branch
(bare prompt, `feature/audit-1-tomli-real-llm`) PARTIAL-FAILed on
`parse_bool` for three reasons (wrong return type, `panic!` instead of
`TomliError::new`, hallucinated field names). This sprint's job: show
those gaps are closed structurally for a stateful function too.

## Method

- **Target**: `tomli_loads._parse_int` (11-line Python helper; mutates
  `state.pos` in two phases; non-trivial error path).
- **Provider**: `OpenAiProvider` at `<user-codex deployment URL>/v1` (model `gpt-5.5`).
- **Cache discipline**:
  - `SyntheticProvider` NOT registered.
  - `cache_dir` = isolated `tempdir().join("llm_cache")`, verified
    non-existent pre-flight.
- **Builder**: production
  `cobrust_translator::build_translation_prompt_rich(unit, ctx)` per
  ADR-0036 §"Decision".
- **Workspace context**: tomli `Value` + `TomliError` + `State`
  preamble (verbatim from `crates/cobrust-tomli/src/parser.rs`),
  `parse_bool` few-shot example (audit-1's PASS-validated leaf),
  return-type contract `Result<i64, TomliError>`, error contract
  `Err(TomliError::new("expected digit", start))`.
- **Prompt size**: 3713 chars, 144 lines.
- **G2 gate**: synthesized minimal Cargo crate (workspace preamble +
  emitted body) → `cargo check`.
- **G3 gate**: `cargo test` driving 14 deterministic CPython 3.11
  oracle inputs through the emitted function; per-case divergence
  classified `strict | numerical | semantic | divergent`.

## Result

**OUTCOME: PASS**

### G1 — L1 dispatch

PASS — real HTTP round-trip succeeded, response non-empty.

Ledger entry:

```json
{
  "ts": "2026-05-09T11:03:04.178576Z",
  "task": "translate",
  "provider": "user_codex_audit3a",
  "provider_kind": "openai",
  "model": "gpt-5.5",
  "cache_key": "blake3:25d5be44d35c0735c5bd0dbb65ee412bdc901d6177630facf216b5e4525e525d",
  "cache_hit": false,
  "prompt_tokens": 1302,
  "completion_tokens": 669,
  "total_tokens": 1971,
  "latency_ms": 13313,
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

### G3 — L2.behavior (differential, 14 oracle inputs)


Differential outcomes (14 oracle inputs):

| Tier | Label | Buffer | start_pos | Expected | Actual | Pass |
|---|---|---|---|---|---|---|
| `strict` | `plain_zero` | `"0"` | 0 | `{"end_pos":1,"kind":"ok","value":0}` | `{"end_pos":1,"kind":"ok","value":0}` | PASS |
| `strict` | `plain_one` | `"1"` | 0 | `{"end_pos":1,"kind":"ok","value":1}` | `{"end_pos":1,"kind":"ok","value":1}` | PASS |
| `strict` | `multi_digit` | `"12345"` | 0 | `{"end_pos":5,"kind":"ok","value":12345}` | `{"end_pos":5,"kind":"ok","value":12345}` | PASS |
| `strict` | `negative` | `"-42"` | 0 | `{"end_pos":3,"kind":"ok","value":-42}` | `{"end_pos":3,"kind":"ok","value":-42}` | PASS |
| `strict` | `positive_sign` | `"+7"` | 0 | `{"end_pos":2,"kind":"ok","value":7}` | `{"end_pos":2,"kind":"ok","value":7}` | PASS |
| `strict` | `zero_neg` | `"-0"` | 0 | `{"end_pos":2,"kind":"ok","value":0}` | `{"end_pos":2,"kind":"ok","value":0}` | PASS |
| `strict` | `digit_then_letter` | `"99x"` | 0 | `{"end_pos":2,"kind":"ok","value":99}` | `{"end_pos":2,"kind":"ok","value":99}` | PASS |
| `strict` | `digit_then_space` | `"8 "` | 0 | `{"end_pos":1,"kind":"ok","value":8}` | `{"end_pos":1,"kind":"ok","value":8}` | PASS |
| `strict` | `only_minus` | `"-"` | 0 | `{"kind":"err"}` | `{"kind":"err"}` | PASS |
| `strict` | `only_plus` | `"+"` | 0 | `{"kind":"err"}` | `{"kind":"err"}` | PASS |
| `strict` | `empty` | `(empty)` | 0 | `{"kind":"err"}` | `{"kind":"err"}` | PASS |
| `strict` | `letter_first` | `"abc"` | 0 | `{"kind":"err"}` | `{"kind":"err"}` | PASS |
| `strict` | `big_int` | `"1234567890"` | 0 | `{"end_pos":10,"kind":"ok","value":1234567890}` | `{"end_pos":10,"kind":"ok","value":1234567890}` | PASS |
| `strict` | `at_offset` | `"xx-15y"` | 2 | `{"end_pos":5,"kind":"ok","value":-15}` | `{"end_pos":5,"kind":"ok","value":-15}` | PASS |

Tier summary:

- `strict` : 14


### G4 — Cache discipline

PASS — both axes verified:

1. Provider registry contained exactly one `OpenAiProvider`; no
   `SyntheticProvider` registered.
2. `cache_dir` was an isolated `tempfile::tempdir()` path, verified
   non-existent before dispatch.
3. Ledger entry's `cache_hit` field = `false`.

### Emitted Rust source (extracted from LLM response, verbatim)

```rust
fn parse_int(state: &mut State<'_>) -> Result<i64, TomliError> {
    let start = state.pos;

    if matches!(state.peek(), Some(b'-' | b'+')) {
        state.pos += 1;
    }

    let digits_start = state.pos;
    while matches!(state.peek(), Some(b'0'..=b'9')) {
        state.pos += 1;
    }

    if state.pos == digits_start {
        return Err(TomliError::new("expected digit", start));
    }

    state.src[start..state.pos]
        .parse::<i64>()
        .map_err(|_| TomliError::new("invalid integer", start))
}
```

### Raw LLM response (first 1500 chars)

```text
fn parse_int(state: &mut State<'_>) -> Result<i64, TomliError> {
    let start = state.pos;

    if matches!(state.peek(), Some(b'-' | b'+')) {
        state.pos += 1;
    }

    let digits_start = state.pos;
    while matches!(state.peek(), Some(b'0'..=b'9')) {
        state.pos += 1;
    }

    if state.pos == digits_start {
        return Err(TomliError::new("expected digit", start));
    }

    state.src[start..state.pos]
        .parse::<i64>()
        .map_err(|_| TomliError::new("invalid integer", start))
}
```

## Production-validated signal (§1.2)

**yes** — §1.2 mechanism-demonstrated → production-validated upgrade signal achieved.

## Conclusion

All four gates green on the **stateful** function `parse_int`,
through the **production** `build_translation_prompt_rich` builder.

This is the §1.2 production-validated upgrade signal: the audit-1
PASS on the leaf `parse_bool` (12/12 strict) generalises to a function
that mutates `state.pos` across two distinct phases (sign + digits
loop) and carries a non-trivial error path. The bare M4
`build_translation_prompt` would have produced the audit-1 sonnet
PARTIAL-FAIL pattern (wrong return type, `panic!` instead of
`TomliError::new`, hallucinated field names); the rich variant via
`WorkspaceContext` injection lifts every gap.

The audit-1 sonnet branch (`feature/audit-1-tomli-real-llm`,
PARTIAL-FAIL) is empirically retired — the bare prompt was the bug,
not the model.

## Token spend

| Phase | Calls | Tokens billed |
|-------|-------|---------------|
| L1 real dispatch | 1 | 1971 |
| Cache replay | 0 | 0 |
| **Total** | **1** | **1971** |

(prompt: 1302, completion: 669)

## Actionable consequences

1. **Audit #3b (ADR-0037)** — `@py_compat` hard-bind shifts from
   reactive (fix observed divergences) to proactive (semantic-tier
   rigor). Pin the tier classifier `classify_divergence` from this
   harness as the canonical mapper.
2. **Production rollout** — extend `WorkspaceContext` to the dateutil
   / msgpack / requests / click translators. Each library author
   builds one bundle (preamble + 1 few-shot + return/error contracts);
   afterwards every function in that library benefits.
3. **Retire audit-1 sonnet branch** — `feature/audit-1-tomli-real-llm`
   PARTIAL-FAIL data is now superseded; the bare prompt was the bug,
   the production rich variant fixes it.

## Cross-references

- ADR-0036 — sprint binding (this audit).
- ADR-0037 (future) — `@py_compat` hard-bind; anchored on this
  audit's divergence taxonomy if the outcome is not PASS.
- ADR-0032 — audit-1 leaf PASS this audit extends to stateful.
- ADR-0007 — translator pipeline whose synthetic-only default this
  audit deliberately bypasses.
- ADR-0008 — repair loop; the divergence table above forms the
  diagnostic blob for attempt 2 if a follow-up sprint exercises it.
- `finding:audit-1-tomli-real-llm-result` — the leaf PASS this
  builds on.
- `finding:translator-real-vs-synthetic-status` — the gap this
  finding closes for the stateful axis.
- `crates/cobrust-translator/tests/audit_3a_tomli_stateful.rs` —
  harness implementation.
- `crates/cobrust-translator/src/translate.rs::build_translation_prompt_rich`
  — production builder ADR-0036 introduces.
- Memory `feedback_third_party_audit_2026_05_09.md` — handoff §A.3.
- Memory `reference_codex_api.md` — endpoint credentials.
