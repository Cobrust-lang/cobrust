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

**OUTCOME: PARTIAL-PASS** (confirmed across two independent runs)

### G1 — L1 dispatch

Real HTTP round-trip succeeded on both runs. Representative ledger entry
(run 1, 2026-05-09T02:53:05):

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

`cache_hit`: false — confirmed first call to isolated tempdir cache on both runs.

### G2 — L2.build (textual)

**PASS** on both runs: emitted text contains `fn` keyword and `bool` type —
syntactic structure is a valid Rust function body.

### G3 — L2.behavior (12 oracle inputs)

**PARTIAL-FAIL** on both runs: the emitted code has the correct structural
shape (prefix matching on `"true"` / `"false"`, pos advance) but returns
`bool` instead of `Result<bool, TomliError>`. The L2.behavior textual check
looks for `Ok(true)`, `Ok(false)`, and `Err(...)` — none of which appear in
the emitted code since the LLM translated to the simpler (non-Result) signature.

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

**PASS** on both runs: `cache_hit=false` confirmed in ledger; `cache_dir` was
an isolated tempdir that did not exist before each test run; no
`SyntheticProvider` was registered.

### Emitted Rust source (verbatim from LLM response, run 1)

The LLM returned a markdown code block with the following content:

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

Run 2 produced a different but structurally equivalent variant:

```rust
fn _parse_bool(state: &mut _State) -> bool {
    if state.remaining().starts_with("true") {
        state.advance(4);
        true
    } else if state.remaining().starts_with("false") {
        state.advance(5);
        false
    } else {
        panic!("expected boolean literal");
    }
}
```

Both variants confirm: the LLM understands the algorithm (prefix match + advance)
but invents different API shapes for `_State` in the absence of the struct definition.

### Diagnosis of G3 failure

Three distinct correctness gaps vs. the canned-response version:

1. **Wrong return type**: `bool` instead of `Result<bool, TomliError>`.
   The spec signature says `_parse_bool(state: _State) -> bool` (Python
   convention), but the Cobrust workspace requires `Result<bool, TomliError>`
   for error propagation. The L1 prompt did not carry the workspace's error
   contract.

2. **Wrong error handling**: `panic!(...)` instead of
   `Err(TomliError::new("expected bool", state.pos))`. Without `TomliError`
   in the prompt context, the LLM defaulted to `panic!`.

3. **Invented field/method names**: run 1 used `state.source[state.index..]`
   (wrong fields); run 2 used `state.remaining()` / `state.advance(N)` (invented
   methods). The actual `State` struct uses `state.src`, `state.bytes`, `state.pos`.
   Without the struct definition, the LLM hallucinated plausible but wrong names.

These gaps are **attributable to insufficient context in the L1 prompt**.
The current template in `crates/cobrust-translator/src/translate.rs:build_translation_prompt`
provides only the function signature + description + py_compat tier. It does
not include the `State` struct definition, the `TomliError` type, or examples
of the surrounding module API. The canned responses embedded all of this by
construction — the gap between canned and real was invisible until this audit.

## Conclusion

G1 (real dispatch) and G4 (cache discipline) passed. This is the **first
time** the translation pipeline has run against a real LLM on a real Python
function without canned responses. The pipeline infrastructure is correct —
the router, cache, ledger, and `run_l1` all worked as specified.

G2 passed: the LLM produced syntactically valid Rust on both runs. G3 failed
due to insufficient context in the prompt: the LLM did not have access to the
`State` struct, `TomliError` definition, or the Cobrust module's error
contract.

This PARTIAL-PASS outcome is the **honest audit deliverable**. The three
specific gaps above give ADR-0033 a concrete anchor: the L1 prompt must carry
the full module context (existing types, error API, State struct) so the LLM
can emit contextually correct code rather than inventing its own type API.

The LLM correctly understood the algorithm in both runs — prefix matching on
`"true"` / `"false"` with the right byte advances (4 and 5). The repair loop
(ADR-0008) with the diff as feedback would very likely fix gaps #1 and #2,
since the algorithmic logic is sound.

## Token spend

| Phase | Run 1 tokens | Run 2 tokens |
|-------|-------------|-------------|
| L1 real dispatch | 1059 | 1044 |
| Cache replay | 0 | 0 |

Both runs used isolated tempdirs so neither could replay the other's cache.

## Actionable consequences

1. **ADR-0033** (`@py_compat` hard-bind to L2 verifier): anchor on gaps #1
   and #2 above. The verifier must check that emitted code uses `Result<T,E>`
   returns, not `T` + `panic!`. The `State` struct and `TomliError` definitions
   must be added to the L1 prompt context before Audit #2.

2. **L1 prompt engineering**: extend `build_translation_prompt` in
   `crates/cobrust-translator/src/translate.rs` to include the module preamble
   (existing types + error contract) in the prompt. This is the primary
   fix required before attempting a full tomli E2E under real LLM.

3. **Repair loop validation**: the LLM's algorithmic correctness (correct
   prefix matching, correct byte advances) confirms that ADR-0008's repair loop
   is viable. With the textual diff as feedback, one repair iteration would
   likely fix the return type and field names. Audit #2 should wire this.

## Cross-references

- ADR-0032 — sprint binding decision (this audit).
- ADR-0033 — `@py_compat` hard-bind to L2 verifier (anchored by gaps #1 + #2).
- `finding:translator-real-vs-synthetic-status` — the gap this audit addresses;
  now updated with first empirical data.
- `finding:m5-m7-real-llm-validation` — the M3 wire-protocol smoke; this
  finding extends it to a real translation attempt.
- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs` — harness.
- `crates/cobrust-translator/src/translate.rs:build_translation_prompt` —
  the L1 prompt template that needs the module-context extension.
</content>
</invoke>