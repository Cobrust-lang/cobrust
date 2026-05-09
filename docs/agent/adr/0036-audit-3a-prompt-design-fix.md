---
doc_kind: adr
adr_id: "0036"
title: Audit #3a — production prompt-design fix (workspace-context-injection in build_translation_prompt)
status: accepted
date: 2026-05-09
last_verified_commit: 4fabf4c
supersedes: []
superseded_by: []
dependencies: [adr:0007, adr:0032, adr:0035, finding:audit-1-tomli-real-llm-result, finding:translator-real-vs-synthetic-status]
---

# ADR-0036: Audit #3a — production prompt-design fix (workspace-context-injection in `build_translation_prompt`)

## Context

Per **review-claude handoff §A.3**, Audit #3 splits into:

- **#3a** — production prompt-design fix in
  `crates/cobrust-translator/src/translate.rs::build_translation_prompt`.
  ADR-0035 §"Consequences/Neutral" §3 pinned this ADR slot.
- **#3b** — `@py_compat` hard-bind (queued; ADR-0037 reserved).

The `audit-1` sonnet branch (`feature/audit-1-tomli-real-llm`,
eafe617) ran the bare M4 builder verbatim against
`tomli_loads._parse_bool`. PARTIAL-FAIL across two runs: emitted
`fn _parse_bool(state: _State) -> bool { ... panic!(...) }` — wrong
return type (`bool` not `Result<bool, TomliError>`), wrong error path
(`panic!` not `Err(TomliError::new(...))`), wholly hallucinated field
names (`state.source[state.index..]`, `state.remaining()`). Diagnosis
(verbatim from sonnet's finding):

> "These gaps are attributable to insufficient context in the L1
> prompt. The current template ... provides only the function
> signature + description + py_compat tier. It does not include the
> `State` struct definition, the `TomliError` type, or examples of
> the surrounding module API."

The `audit-1` Opus authoritative branch (merged at `dfba6e9`)
hand-built a richer inline prompt
(`tests/audit_1_tomli_real_llm.rs::build_rich_prompt`, line 298) that
injected: (1) verbatim Python source; (2) full workspace preamble
(`Value` enum + `TomliError` + `State`); (3) `parse_basic_string`
few-shot; (4) explicit numbered output-requirements (signature MUST
return `Result<bool, TomliError>`, errors MUST use `Err(TomliError::
new(...))`, no fences). PASS on all four gates, 12/12 strict tier
(see `findings/audit-1-tomli-real-llm-result.md`).

The gap: the rich design lives only in one inline test. Production
`build_translation_prompt` cannot serve it for another tomli function
or another library. This sprint lifts the audit-1 design into the
production builder so any tomli-class function gets the same
treatment automatically.

### Constraints

1. **Additive API only**: existing callers must continue to compile.
   The bare prompt remains the no-context fallback; rich variant is
   opt-in via a new `build_translation_prompt_rich(...)`.
2. **Honest fail accepted**: this sprint runs a stateful-function
   E2E. If the stateful gate fails, the failure data anchors
   ADR-0037; the implementation does NOT re-tune to mask failure.
3. **Cache discipline mandatory**: `SyntheticProvider` OFF +
   isolated tempdir LLM cache (per `adr:0032 §4 Constraints`).

## Options considered

1. **Manual prompt per function** — one bespoke template per Python
   function. Doesn't scale (tomli has 12 functions, dateutil 30,
   numpy thousands). Violates `adr:0007 §"L1 translation contract"`.
   Rejected.
2. **Workspace-context-injection in `build_translation_prompt`** —
   extend the production builder to consume an optional workspace
   bundle (preamble + ≥ 1 few-shot + return-type + error-construction
   contract). Existing callers pass nothing → bare prompt. New callers
   pass a `WorkspaceContext` → rich prompt. Library-author cost =
   "name your common types + pick a few-shot example" (one-time per
   library). **Chosen.**
3. **RAG-style retrieval over workspace** — embed all
   `crates/cobrust-*/src/*.rs`; pick K nearest-neighbor functions as
   few-shot. Over-engineered for this sprint (audit-1 PASS shows N=1
   hand-picked is sufficient for leaves). Defer to post-#3b if
   Option 2 hits a recurring under-fit. Rejected for this sprint.

## Decision

**Option 2.** Concrete API:

```rust
// crates/cobrust-translator/src/translate.rs

/// Workspace context the rich prompt builder consumes.
pub struct WorkspaceContext {
    /// Library's already-translated common types (e.g. for tomli, the
    /// `Value` enum + `TomliError` + `State` + State helpers). Verbatim
    /// into the rich prompt's "Workspace API contract" section.
    pub module_preamble: String,
    /// Already-translated functions presented as few-shot examples.
    /// Each entry is `(name, full Rust source)`.
    pub fewshot_examples: Vec<(String, String)>,
    /// Verbatim Python source of the target function (from the corpus
    /// upstream — kept on the context so spec.toml schema stays put).
    pub target_python_source: String,
    /// Cobrust idiomatic return-type contract, e.g.
    /// `"Result<bool, TomliError>"`.
    pub return_type_contract: String,
    /// Cobrust idiomatic error-construction contract, e.g.
    /// `"Err(TomliError::new(\"…\", state.pos))"`. Forbids `panic!`.
    pub error_construction_contract: String,
}

/// Rich-prompt builder. Bridges existing `FunctionUnit` (from
/// spec.toml) + library-specific `WorkspaceContext` into a prompt
/// that carries audit-1's full design.
pub fn build_translation_prompt_rich(
    unit: &FunctionUnit,
    ctx: &WorkspaceContext,
) -> String;

// Existing private `build_translation_prompt(unit)` retained as
// the bare fallback; existing callers (M4 tomli synthetic, etc.)
// unchanged.
```

The rich prompt's structural shape mirrors `audit-1`'s
`build_rich_prompt` verbatim: target Python source → workspace API
contract → few-shot example → numbered output requirements (return
type, error construction, no redefinitions, no fences, spec
description, py-compat tier).

## Acceptance gate (Done means)

A stateful tomli function that audit-1 sonnet's bare prompt would
have failed PARTIAL passes through `build_translation_prompt_rich`
against the CPython 3.11 oracle on ≥ 10 differential inputs.

| Gate | Check |
|------|-------|
| G1 — L1 dispatch | Real HTTP round-trip non-empty; ledger `cache_hit=false, outcome="ok"`. |
| G2 — L2.build | Synthesized crate (preamble + emission) `cargo check`s with zero errors. |
| G3 — L2.behavior (≥ 10 inputs) | Emitted output matches CPython oracle on ≥ 10 inputs; divergences classified by tier (`audit-1`'s `classify_divergence`). |
| G4 — Cache discipline | `cache_hit=false`; one `OpenAiProvider` only; isolated tempdir cache. |

**Outcome reporting**:
- 4 green → §1.2 production-validated. Audit-1 sonnet branch retired.
  ADR-0037 (#3b) anchored on observed semantic-tier classification.
- G1+G2+G4 green, G3 red → strongest anchor for ADR-0037 (rich
  closes structural gaps but a semantic divergence remains).
- G2 red → rich insufficient even structurally; revisit Option 3.
- G1 / G4 red → infra bug. Stop.

The fail signal IS the deliverable per `audit-1` framing. This sprint
emits a finding regardless.

## Consequences

### Positive

- Production builder absorbs `audit-1` Opus's empirically PASSing
  prompt design. Future translations (any library, any function) can
  use the rich shape without per-test scaffolding.
- The `audit-1` sonnet branch (PARTIAL-FAIL with bare prompt) is
  empirically retired as "supplementary scaffolding" — the bare
  prompt itself was the bug, and the rich variant is the production
  default for new callers.
- §1.2 "mechanism-demonstrated → production-validated" upgrade is
  honest: audit-1 measured one leaf (`parse_bool`); this sprint
  measures a stateful function, establishing the design generalises
  beyond leaves.
- Library-author cost is one-time (build a `WorkspaceContext`);
  after that every function in the library gets the rich treatment.
- The bare `build_translation_prompt` is retained; no existing
  M4..M-batch canned-response pipeline breaks.

### Negative

- One real LLM API call per gated test run. Token spend ~1k–3k per
  run (stateful prompt ~1.5× leaf prompt size).
- `WorkspaceContext` is a per-library hand-maintained bundle. If
  preamble drifts (refactor renames a field), bundle goes stale.
  Mitigation: bundle is just strings; drift surfaces as G2 fail on
  next E2E run.
- N=1 few-shot may under-fit libraries with multiple idiomatic
  shapes. M5+ extension: allow N>1; not in this sprint.

### Neutral / unknown

- Whether `gpt-5.5` handles the rich prompt's ~3-4k char size for a
  stateful function (audit-1 leaf served fine at 4547 chars / 1528
  prompt tokens; stateful should grow ~1.5×, within model context).
- Whether few-shot must be same-library or can cross libraries. This
  sprint constrains to same-library to match audit-1's empirical
  setup.

## Evidence

- `crates/cobrust-translator/src/translate.rs::build_translation_prompt`
  (line 175) — fix site (the bare M4 builder).
- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs::build_rich_prompt`
  (line 298) — the PASS-validated rich design this ADR generalises.
- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs:182-260`
  — `WORKSPACE_PREAMBLE` (verbatim Cobrust workspace preamble).
- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs:269-296`
  — `PARSE_BASIC_STRING_REF` (the few-shot).
- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs:806-844`
  — `classify_divergence` (the tier classifier this sprint reuses).
- `crates/cobrust-tomli/src/parser.rs` — already-translated tomli
  source the `WorkspaceContext` mirrors.
- `corpus/tomli/upstream/tomli_loads.py:104-114` — `_parse_int`
  upstream Python (Step C target candidate).
- `findings/audit-1-tomli-real-llm-result.md` — PASS (12/12 strict).
- `findings/translator-real-vs-synthetic-status.md` — the gap this
  sprint closes for the stateful axis.
- Memory `feedback_third_party_audit_2026_05_09.md` — handoff §A.3.
- Memory `reference_codex_api.md` — endpoint credentials.

## Cross-references

- `adr:0007` — translator pipeline + bare-prompt default this ADR
  generalises.
- `adr:0032` — `audit-1`; immediate predecessor.
- `adr:0035` — predecessor (M11.3); reserved this slot.
- ADR-0037 (future) — `@py_compat` hard-bind; queued.
- `finding:audit-1-tomli-real-llm-result` — empirical PASS motivating
  this lift.
- `finding:translator-real-vs-synthetic-status` — gap closure.
- `finding:audit-3a-stateful-prompt-design` (this sprint) — concrete
  PASS / FAIL data for the stateful function in Step C.
