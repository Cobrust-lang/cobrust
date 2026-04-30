---
doc_kind: module
module_id: mod:llm_router
crate: cobrust-llm-router
last_verified_commit: TBD
dependencies: []
---

# Module: llm_router

## Purpose

Provider-agnostic LLM dispatch + content-addressed cache + append-only
token ledger for the Cobrust compiler's translation subsystem. **First-
class compiler component, not a tool.** Treated as seriously as the type
checker.

## Status

M0 — empty stub. First delivery at M3.

## Public surface (target — M3)

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn complete(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionResponse, LlmError>;

    fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>;
}

pub struct Router {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    table: RoutingTable,
    cache: Cache,
    ledger: Ledger,
}

impl Router {
    pub fn from_config(cfg: &RouterConfig) -> Result<Self, RouterError>;

    pub async fn dispatch(
        &self,
        task: Task,
        prompt: Prompt,
    ) -> Result<RouterResponse, RouterError>;
}

pub enum Task {
    SpecExtract,
    Translate,
    Repair,
    Custom(String),
}

pub enum Strategy {
    Cost,
    Quality,
    Latency,
    Consensus { n: u8 },
}
```

## Invariants

- **Cache key** = `BLAKE3(canonicalize(prompt) || model_id || canonicalize(params))`.
  Identical inputs produce identical cache keys, across machines.
- **Ledger** entries are append-only; never rewritten.
- **Provider failure isolation**: one provider returning an error never
  halts the pipeline; router falls through to the next preferred entry
  in the routing table.
- **All token spend is observable** via `.cobrust/ledger.jsonl`.
- **Consensus deterministic tie-breaking**: documented and tested rule
  (TBD: lexicographic on response hash, or first-listed wins).
- **Streaming round-trips end-to-end** for both adapters.

## Configuration shape

See `cobrust.toml.example` at the repo root. Public schema sections:

- `[router]` — global defaults (`default_strategy`, `cache_dir`,
  `ledger_path`).
- `[providers.<name>]` — `kind` (`anthropic` | `openai`),
  `base_url`, `api_key_env`, `models`.
- `[routing.<task>]` — `strategy`, `n` (consensus only), `preferred`
  (ordered list of `provider:model` pairs).

## Strategies

| `strategy` | Behavior |
|---|---|
| `cost` | Pick cheapest model from `preferred`. |
| `quality` | Pick first model from `preferred`. |
| `latency` | Pick fastest historical responder; ties → `quality`. |
| `consensus` | Issue `n` parallel calls; combine via majority / structured-diff / verifier-judged best-of-N. |

## Adapter requirements

Both adapters live in this crate (`anthropic.rs`, `openai.rs`).

- **Anthropic adapter**
  - Endpoint: `POST {base_url}/v1/messages`
  - Auth: `x-api-key` header (value from `api_key_env`)
  - Streaming: SSE `event: content_block_delta`
- **OpenAI-compatible adapter**
  - Endpoint: `POST {base_url}/chat/completions`
  - Auth: `Authorization: Bearer ${api_key_env}`
  - Streaming: SSE `data: {chunk}\n\n`
  - Model name passes through unchanged so DeepSeek / vLLM / Together /
    OpenRouter all work without further code.

## Done means (M3)

- [ ] Anthropic adapter: round-trip against `https://api.anthropic.com`
      and any Anthropic-compatible base_url.
- [ ] OpenAI adapter: round-trip against `https://api.openai.com/v1`
      and any OpenAI-compatible base_url.
- [ ] Cache hit/miss path tested against synthetic provider double.
- [ ] Ledger writes after every completion; entries parseable as JSONL.
- [ ] Consensus mode tested with `n=2`; deterministic tie-breaking rule
      documented and tested.
- [ ] Streaming round-trips end-to-end for both adapters.
- [ ] Provider failure isolation tested (one provider 500s → router
      retries next preferred → pipeline continues).

## Non-goals

- **Not** a chat UI.
- **Not** a long-running agent loop driver — `mod:translator` owns that.
- **Not** a prompt template store; templates live next to the consumer
  (typically in `mod:translator`).

## Cross-references

- `mod:translator` — primary consumer.
- [adr:0004](../adr/0004-llm-router-architecture.md) — `LlmProvider` trait shape, error taxonomy, retry policy, cache key, ledger schema, consensus tie-breaking.
- `cobrust.toml.example` — public configuration shape.
- Constitution `CLAUDE.md` §4.3.
