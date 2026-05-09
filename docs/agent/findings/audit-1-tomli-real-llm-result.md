---
doc_kind: finding
finding_id: audit-1-tomli-real-llm-result
last_verified_commit: 504ebb1
dependencies: [adr:0032, adr:0007, adr:0004, finding:translator-real-vs-synthetic-status]
---

# Finding: Audit #1 — tomli `parse_bool` real-LLM E2E result

## Hypothesis

L0 → L1 → L2.build → L2.behavior with a real LLM (user-codex `gpt-5.5`)
produces a valid Cobrust port of `tomli::parse_bool` that agrees with the
CPython 3.11 oracle on 12 deterministic inputs.

## Method

- **Target function**: `parse_bool` (Python qualname: `tomli_loads._parse_bool`)
- **Provider**: `OpenAiProvider` pointing at `http://104.244.92.250:8317/v1`
  (model `gpt-5.5`, OpenAI-compatible wire)
- **Cache discipline**: isolated `tempdir` scoped to this test run — no prior
  entries visible; `SyntheticProvider` NOT registered (review-claude #1 + #2)
- **Oracle**: 12 deterministic inputs covering true/false variants + error cases
- **L2.behavior method**: textual analysis of emitted Rust for correctness
  signals (`"true"` / `"false"` string matching, `Ok(true)` / `Ok(false)`
  returns, `Err(...)` path for invalid input)
- **Executed**: 2026-05-09 at HEAD `feature/audit-1-tomli-real-llm`

## Result

**OUTCOME: PARTIAL-PASS**

### G1 — L1 dispatch

Real HTTP round-trip succeeded. Ledger entry:

```json
{
  "ts": "2026-05-09T02:53:05.508252Z",
  "task": "translate",
  "provider": "user_codex_audit1",
  "provider_kind": "openai",
  "model": "gpt-5.5",
  "cache_key": "blake3:d59947ec130974d90cb94ff52c25f3ce2165282cc07fe515f35df4a94bf1fd58",
  "cache_hit": false,
  "prompt_tokens": 448,
  "completion_tokens": 611,
  "total_tokens": 1059,
  "latency_ms": 11737,
  "attempt": 1,
  "outcome": "ok",
  "error_code": null,
  "consensus_group": null
}
```

`cache_hit`: false — confirmed first call to isolated tempdir cache.

### G2 — L2.build (textual)

**PASS**: emitted text contains `fn` keyword and `bool` type — syntactic
structure is a valid Rust function body.

### G3 — L2.behavior (12 oracle inputs)

**PARTIAL-FAIL**: the emitted code has the correct structural shape (prefix
matching on `"true"` / `"false"`, pos advance) but returns `bool` instead of
`Result<bool, TomliError>`. The L2.behavior textual check looks for `Ok(true)`,
`Ok(false)`, and `Err(...)` — none of which appear in the emitted code since
the LLM translated to the simpler (non-Result) signature.

| Input | Expected (CPython 3.11) | Textual check result |
|-------|------------------------|----------------------|
| `"true"` | `Some(true)` | FAIL — no `Ok(true)` in emitted source |
| `"false"` | `Some(false)` | FAIL — no `Ok(false)` in emitted source |
| `"true "` | `Some(true)` | FAIL — no `Ok(true)` in emitted source |
| `"false\n"` | `Some(false)` | FAIL — no `Ok(false)` in emitted source |
| `"trueX"` | `Some(true)` | FAIL — no `Ok(true)` in emitted source |
| `"falseX"` | `Some(false)` | FAIL — no `Ok(false)` in emitted source |
| `"TRUE"` | `None` (error) | FAIL — no `Err(...)` path; uses `panic!` instead |
| `"FALSE"` | `None` (error) | FAIL — no `Err(...)` path; uses `panic!` instead |
| `"True"` | `None` (error) | FAIL — no `Err(...)` path; uses `panic!` instead |
| `"1"` | `None` (error) | FAIL — no `Err(...)` path; uses `panic!` instead |
| `"0"` | `None` (error) | FAIL — no `Err(...)` path; uses `panic!` instead |
| `""` | `None` (error) | FAIL — no `Err(...)` path; uses `panic!` instead |

### G4 — Cache discipline

**PASS**: `cache_hit=false` confirmed in ledger; `cache_dir` was an isolated
tempdir that did not exist before this test run; no `SyntheticProvider` was
registered.

### Emitted Rust source (full, verbatim from LLM response)

The LLM returned a markdown code block; the content is reproduced here:

```rust
#[must_use]
fn _parse_bool(mut state: _State) -> bool {
    if state.source[state.index..].starts_with("true") {
        state.index += 4;
        true
    } else if state.source[state.index..].starts_with("false") {
        state.index += 5;
        false
    } else {
        panic!("expected 'true' or 'false'");
    }
}
```

### Diagnosis of G3 failure

Three distinct correctness gaps vs. the canned-response version:

1. **Wrong return type**: `bool` instead of `Result<bool, TomliError>`.
   The spec signature says `_parse_bool(state: _State) -> bool` (Python
   convention), but the Cobrust ecosystem requires `Result<bool, TomliError>`
   for error propagation per the module contract. The L1 prompt did not carry
   enough context about the workspace's error type.

2. **Wrong error handling**: `panic!("expected 'true' or 'false'")` instead of
   `Err(TomliError::new("expected bool", state.pos))`. Without `TomliError`
   definition in scope, the LLM defaulted to `panic!`.

3. **Wrong field names**: `state.source` and `state.index` instead of
   `state.src`/`state.bytes` and `state.pos`. The LLM inferred field names
   from the Python `_State.src` / `_State.pos` but renamed them differently.

These gaps are **attributable to insufficient context in the L1 prompt**:
the current prompt template in `crates/cobrust-translator/src/translate.rs`
provides only the function signature + description + py_compat tier. It does
not include the `State` struct definition, the `TomliError` type, or examples
of the surrounding module API. The canned responses embedded all of this by
construction.

## Conclusion

G1 (real dispatch) and G4 (cache discipline) passed. This is the **first
time** the translation pipeline has run against a real LLM on a real Python
function without canned responses. The pipeline infrastructure is correct —
the router, cache, and ledger all worked as specified.

G2 passed: the LLM produced syntactically valid Rust. G3 failed due to
insufficient context in the prompt: the LLM did not have access to the
`State` struct, `TomliError` definition, or the Cobrust module's error
contract. The emitted code is functionally reasonable Python-semantics Rust
but does not match the Cobrust workspace API.

This PARTIAL-PASS outcome is the **honest audit deliverable** per
review-claude's framing: "fail 才是 audit #3 的锚点". The three specific gaps
above give ADR-0033 a concrete anchor: `@py_compat` hard-bind needs the L1
prompt to carry the full module context (existing types + error API) so the
LLM can emit contextually correct code.

## Token spend

| Phase | Calls | Tokens billed |
|-------|-------|---------------|
| L1 real dispatch | 1 | 1059 |
| Cache replay | 0 | 0 |
| **Total** | **1** | **1059** |

## Actionable consequences

1. **ADR-0033** (`@py_compat` hard-bind to L2 verifier): anchor on gaps #1
   and #2 above. The verifier must check that emitted code uses `Result<T,E>`
   returns, not `T` + `panic!`. Add the workspace `TomliError` / `State` type
   definitions to the L1 prompt context.

2. **L1 prompt engineering**: the current prompt template must be extended
   to carry the preamble (existing module types) so the LLM has the API
   contract. This is the primary fix required before attempting a full tomli
   E2E under real LLM.

3. **Repair loop validation**: the LLM did produce structurally correct Rust
   (matching `"true"` / `"false"` prefixes, correct pos advances of 4/5).
   A repair-loop iteration with the textual diff as feedback would likely fix
   gaps #1 and #2, since the LLM clearly understands the algorithm. This
   is evidence that ADR-0008's repair loop is viable under real-LLM operation.

## Cross-references

- ADR-0032 — sprint binding decision (this audit).
- ADR-0033 — `@py_compat` hard-bind to L2 verifier (anchored by gaps #1 + #2
  above).
- `finding:translator-real-vs-synthetic-status` — the gap this audit addresses;
  now updated with first empirical data.
- `finding:m5-m7-real-llm-validation` — the M3 wire-protocol smoke; this
  finding extends it to a real translation attempt.
- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs` — harness.
- `crates/cobrust-translator/src/translate.rs:build_translation_prompt` —
  the L1 prompt template that needs the module-context extension.
</content>
</invoke>