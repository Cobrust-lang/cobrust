---
doc_kind: module
module_id: mod:llm_router
crate: cobrust-llm-router
last_verified_commit: TBD
dependencies: [adr:0004]
---

# Module: llm_router

## Purpose

Provider-agnostic LLM dispatch + content-addressed cache + append-only
token ledger for the Cobrust compiler's translation subsystem. **First-
class compiler component, not a tool.** Treated as seriously as the type
checker.

## Status

M3 delivered. All gates green:

- 40 unit tests + 8 synthetic-provider integration tests + 8
  HTTP-adapter (`wiremock`) tests = **56 tests pass**.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo build --workspace --all-targets`,
  `cargo test --workspace`, `bash scripts/doc-coverage.sh` all green.

All load-bearing decisions pinned by [adr:0004](../adr/0004-llm-router-architecture.md).

## Public surface

```rust
// crate root re-exports.
pub use cobrust_llm_router::{
    AnthropicProvider, OpenAiProvider,                       // adapters
    LlmProvider, CompletionRequest, CompletionResponse,
    Chunk, Message, Role, SamplingParams, TokenUsage,        // provider types
    LlmError,                                                // error taxonomy
    Cache, CacheKey,                                         // cache
    Ledger, LedgerEntry, Outcome,                            // ledger
    RouterConfig, ProviderConfig, ProviderKind, ProviderModel,
    RoutingEntry, StrategyName, DefaultStrategy,             // config
    Router, RouterBuilder, RouterError, RouterResponse,
    RetryPolicy, Strategy, Task,                             // dispatch
};

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError>;
    fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>;
}

pub struct Router { /* private fields */ }

impl Router {
    pub fn builder() -> RouterBuilder;
    pub async fn dispatch(
        &self,
        task: Task,
        req: CompletionRequest,
    ) -> Result<RouterResponse, RouterError>;
}
```

`Router` is constructed via [`RouterBuilder`] which lets callers register
concrete `Arc<dyn LlmProvider>` instances against a parsed
[`RouterConfig`] before the cache and ledger handles are opened.

## Error taxonomy (binding)

| Variant | HTTP analogue | Transient? | Provider fault? | Ledger code |
|---|---|---|---|---|
| `LlmError::Transport(..)` | DNS/TCP/TLS/idle | yes | yes | `transport` |
| `LlmError::RateLimit { retry_after_ms }` | 429 | yes | yes | `rate-limit` |
| `LlmError::Server { status, body }` | 5xx | yes | yes | `server` |
| `LlmError::BadRequest { status, body }` | 4xx (≠ 401/403/429) | no | **no** | `bad-request` |
| `LlmError::Auth` | 401 / 403 | no | yes | `auth` |
| `LlmError::Decode(..)` | invalid response | no | yes | `decode` |
| `LlmError::Stream(..)` | SSE-level fault | yes | yes | `stream` |
| `LlmError::Cancelled` | n/a | no | **no** | `cancelled` |
| `LlmError::Provider { code, message }` | provider-app | no | yes | `provider` |

Transient errors trigger retry per [`RetryPolicy`] (default: 5 attempts,
250 ms base, factor 2, full jitter, 30 s cap, `Retry-After` honoured).
Permanent errors fall through to the next preferred provider in the
routing table.

## Invariants

- **Cache key** = `BLAKE3(canonical_request_bytes)` per `adr:0004`. Same
  request hashes to the same key bit-for-bit across machines.
- **Ledger** entries are append-only; concurrent writers serialise
  through a process-wide `tokio::sync::Mutex<File>` opened with
  `O_APPEND`.
- **Provider failure isolation**: a permanent error from one provider
  triggers fall-through; the dispatch never halts on a single 5xx.
- **All token spend is observable** via `.cobrust/ledger.jsonl`.
- **Consensus deterministic tie-breaking** (per `adr:0004`): largest
  group on `BLAKE3(NFC(text))` → lexicographic-smallest hash →
  preferred-list index ascending.
- **Streaming round-trips end-to-end** for both adapters; exactly one
  `Chunk::Done` frame is emitted per stream.

## Configuration shape

See [`cobrust.toml.example`](../../../cobrust.toml.example) at the repo
root. Sections:

- `[router]` — global defaults (`default_strategy`, `cache_dir`,
  `ledger_path`).
- `[providers.<name>]` — `kind` (`anthropic` | `openai`),
  `base_url`, `api_key_env`, `models`.
- `[routing.<task>]` — `strategy`, `n` (consensus only), `preferred`
  (ordered list of `provider:model` pairs).

`RouterConfig::validate()` enforces:

1. Every `routing.<task>.preferred` references a declared provider.
2. `consensus` strategies have `n >= 2` and `n <= preferred.len()`.

## Ledger schema (`.cobrust/ledger.jsonl`)

| Field | Type | Meaning |
|---|---|---|
| `ts` | RFC3339 string | UTC timestamp |
| `task` | string | task name from routing table |
| `provider` | string | provider key from `cobrust.toml` |
| `model` | string | model id as sent to provider |
| `cache_key` | string | `blake3:<hex>` of canonical request |
| `cache_hit` | bool | true if served from cache |
| `prompt_tokens` | u32 | from provider response |
| `completion_tokens` | u32 | from provider response |
| `total_tokens` | u32 | sum |
| `latency_ms` | u32 | wall-clock for the provider call |
| `attempt` | u8 | retry attempt number (1-indexed) |
| `outcome` | enum | `ok` / `error_transient` / `error_permanent` |
| `error_code` | string \| null | `LlmError::code()` when not ok |
| `consensus_group` | string \| null | UUIDv4 for consensus shards |

## Strategies

| `strategy` | Behavior |
|---|---|
| `cost` | Walks `preferred` in submitted order; M3 placeholder for cost-table-driven selection. |
| `quality` | First entry in `preferred`, fall through on error. |
| `latency` | Sorts `preferred` by EWMA latency (alpha=0.2); cold-start ties resolve to submission order. |
| `consensus` | Issues `n` parallel calls (first `n` distinct entries in `preferred`); tie-breaks per `adr:0004`. |

## Adapter requirements (delivered)

Both adapters live in this crate.

- **Anthropic adapter** (`anthropic.rs`)
  - Endpoint: `POST {base_url}/v1/messages`
  - Auth: `x-api-key` header (value from `api_key_env`) +
    `anthropic-version: 2023-06-01`
  - Streaming: SSE `event: content_block_delta` frames; `message_delta`
    carries usage; `message_stop` terminates.
- **OpenAI-compatible adapter** (`openai.rs`)
  - Endpoint: `POST {base_url}/chat/completions`
  - Auth: `Authorization: Bearer ${api_key_env}`
  - Streaming: SSE `data: {chunk}\n\n` lines, terminated by
    `data: [DONE]`. `stream_options.include_usage = true` is set so the
    final chunk carries usage.
  - Model name passes through unchanged so DeepSeek / vLLM / Together /
    OpenRouter all work without further code.

## Done means (M3) — verified

- [x] Anthropic adapter: round-trip against any `base_url`
      (`tests/adapters_http.rs::anthropic_complete_round_trips_a_text_response`).
- [x] OpenAI adapter: round-trip against any `base_url`
      (`tests/adapters_http.rs::openai_complete_round_trips_a_text_response`,
      `tests/adapters_http.rs::openai_compatible_works_against_arbitrary_base_url`).
- [x] Cache hit/miss path tested through the public `Router` API
      (`tests/synthetic_provider.rs::cache_hit_after_first_dispatch`).
- [x] Ledger writes after every completion, entries parseable as JSONL
      (`tests/synthetic_provider.rs::ledger_is_append_only_across_router_lifecycle`).
- [x] Consensus mode tested deterministically
      (`tests/synthetic_provider.rs::consensus_tie_breaking_is_deterministic`,
      `consensus_quorum_lost_when_too_many_fail`).
- [x] Streaming round-trips end-to-end for both adapters
      (`anthropic_streams_content_block_delta_events`,
      `openai_streams_chat_chunk_data_lines`).
- [x] Provider failure isolation
      (`provider_failure_falls_through_to_next_preferred`).
- [x] Cache-key reproducibility across processes
      (`cache_key_is_deterministic_across_runs`).

## Non-goals

- **Not** a chat UI.
- **Not** a long-running agent loop driver — `mod:translator` owns that.
- **Not** a prompt template store; templates live next to the consumer
  (typically in `mod:translator`).

## Cross-references

- `mod:translator` — primary consumer (M4+).
- [adr:0004](../adr/0004-llm-router-architecture.md) — `LlmProvider` trait shape, error taxonomy, retry policy, cache key, ledger schema, consensus tie-breaking.
- [`cobrust.toml.example`](../../../cobrust.toml.example) — public configuration shape.
- Constitution `CLAUDE.md` §4.3.
