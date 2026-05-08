---
doc_kind: adr
adr_id: 0031
title: Audit #5 — bump ledger schema to carry `provider_kind`
status: accepted
date: 2026-05-09
last_verified_commit: ac5636a
supersedes: []
superseded_by: []
---

# ADR-0031: Audit #5 — bump ledger schema to carry `provider_kind`

## Context

External Claude audit (review-claude window, 2026-05-09) recommended
bumping the LLM Router's `.cobrust/ledger.jsonl` schema to record the
**wire protocol kind** (Anthropic vs OpenAI-compatible) in addition to
the existing `provider` (config-section name).

Why this matters: the existing schema only records
`provider: "deepseek"` (the config name), not `kind: "openai"` (the
wire protocol). When inspecting historical ledgers — for cost analysis,
incident postmortems, or differential debugging — you cannot tell
whether `provider: "openrouter_or_unknown"` spoke OpenAI- or Anthropic-
compatible without cross-referencing the original config. Configs
mutate over time. The ledger should be self-describing.

ADR-0004 (LLM Router architecture) defined the original schema. This
ADR amends that schema, additively — no field removed.

## Options considered

1. **Add `provider_kind: Option<ProviderKind>` to `LedgerEntry`** — Option
   so legacy entries (`provider_kind` absent in JSON) deserialize cleanly
   to `None`; new entries always populate it via `Some(kind)`.

2. **Add a separate `ProviderKindV2` enum with `Unknown` variant** — explicit
   "we don't know the kind for legacy entries" distinct from "this is None".
   More verbose; same outcome.

3. **Bump schema version (`schema_version: 2`) field** — full structured
   versioning. Overkill for one additive field; postpone until a real
   non-additive change.

4. **Don't add it; require future analysis tools to look up config** — the
   audit's whole point. Rejected.

## Decision

Choose **Option 1**. Concrete shape:

```rust
// crates/cobrust-llm-router/src/ledger.rs
pub struct LedgerEntry {
    pub ts: String,
    pub task: String,
    pub provider: String,
    pub provider_kind: Option<ProviderKind>,   // NEW
    pub model: String,
    // ... rest unchanged ...
}
```

Backward compat: `#[serde(default)]` on the new field. Legacy
`ledger.jsonl` lines without `provider_kind` deserialize to `None`. New
emissions always populate `Some(provider.kind())`.

Wiring path: extend the `LlmProvider` trait with one new method.

```rust
// crates/cobrust-llm-router/src/provider.rs
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    fn kind(&self) -> crate::config::ProviderKind;   // NEW
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError>;
    fn complete_stream(...) -> ...;
}
```

Concrete impls:
- `AnthropicProvider::kind() -> ProviderKind::Anthropic`
- `OpenAiProvider::kind() -> ProviderKind::Openai`

Router callsite changes: at the 3 ledger-emission points in
`router.rs::dispatch_one_shot` (cache-hit branch + ok branch + err
branch), pass `Some(provider.kind())` to `LedgerEntry::ok` /
`LedgerEntry::err`.

## Consequences

### Positive

- **Self-describing ledger**: `cat ledger.jsonl | jq '.provider_kind'`
  immediately tells you each call's wire protocol.
- **Surgical change**: 5 files (ledger.rs / provider.rs / anthropic.rs /
  openai.rs / router.rs) + ADR + 3 doc trees. No public-API removal.
- **Backward compat preserved**: existing user-on-disk ledgers continue
  to deserialize without modification.

### Negative

- Trait now has 4 methods instead of 3. Negligible.
- Future schema bumps (audit #N+1) might still need a `schema_version`.
  That's a future-future-ADR's problem.

### Neutral

- Audit #5 acceptance criterion: this ADR + the impl + 1 new ledger
  test covering the round-trip with `provider_kind` populated.

## Acceptance gate (Done means)

1. ✅ `LedgerEntry::provider_kind: Option<ProviderKind>` field added
   with `#[serde(default)]`.
2. ✅ `LlmProvider::kind() -> ProviderKind` method on trait.
3. ✅ Both adapters (`AnthropicProvider`, `OpenAiProvider`) implement it.
4. ✅ All 3 callsites in `router.rs::dispatch_one_shot` pass
   `Some(provider.kind())`.
5. ✅ Ledger doc comment updated to show the new field in the schema
   example.
6. ✅ ≥ 1 new test verifying round-trip serialization with
   `provider_kind: Some(ProviderKind::Anthropic)` AND legacy
   `provider_kind: None` (absent) line deserializes cleanly.
7. ✅ Existing 4 ledger tests still pass (additive change).
8. ✅ `cargo test --package cobrust-llm-router` green.
9. ✅ Doc-coverage gate green; zh + en + agent doc trees updated under
   the LLM Router section to mention the new field.
10. ✅ ADR-0031 stamped with `last_verified_commit`.

## Cross-references

- ADR-0004 (LLM Router architecture) — original schema definition.
- `feedback_third_party_audit_2026_05_09.md` — the audit message that
  proposed this bump.
- `docs/agent/modules/llm_router.md` — agent-facing module spec to
  update.
- Source files touched:
  - `crates/cobrust-llm-router/src/ledger.rs`
  - `crates/cobrust-llm-router/src/provider.rs`
  - `crates/cobrust-llm-router/src/anthropic.rs`
  - `crates/cobrust-llm-router/src/openai.rs`
  - `crates/cobrust-llm-router/src/router.rs`
