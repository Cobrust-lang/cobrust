---
doc_kind: adr
adr_id: 0004
title: LLM Router architecture — provider trait, error taxonomy, retry, cache key, ledger schema, consensus tie-breaking
status: accepted
date: 2026-04-30
last_verified_commit: 62ef6bd
supersedes: []
superseded_by: []
---

# ADR-0004: LLM Router architecture — provider trait, error taxonomy, retry, cache key, ledger schema, consensus tie-breaking

## Context

`mod:llm_router` (crate `cobrust-llm-router`, see `docs/agent/modules/llm-router.md`) is a **first-class compiler subsystem** per constitution `CLAUDE.md` §4.3. It is treated as seriously as the type checker. Its M3 deliverable must produce **deterministic, reproducible, observable** dispatch across heterogeneous LLM providers, with an on-disk content-addressed cache and an append-only token ledger.

To make M4 (`mod:translator`) build on top of this without thrash, six load-bearing decisions have to be pinned now:

1. The exact `LlmProvider` async trait shape and request/response types.
2. The error taxonomy (which errors retry, which fail-fast, which fall through to the next preferred provider).
3. The retry policy (backoff curve, jitter, retry budget).
4. The cache key canonicalisation rule (so the same prompt on two machines hashes to the same key).
5. The ledger JSONL schema (so consumers can mine token spend after the fact).
6. The consensus-mode tie-breaking rule (so `n=2` runs are bit-for-bit reproducible).

Constitution §6 (atomic commits) and §8 (default to proceed via ADR) require this to land before any code that relies on it.

## Options considered

### Provider trait shape

1. **Sync `complete` + custom polling for stream** — simpler trait but forces every adapter to spin its own runtime; rejected: violates §5.3 ("LLM Router caches aggressively; a redundant prompt hitting the network is a bug" — cannot be efficient under sync).
2. **Async `complete` returning `Result<CompletionResponse, LlmError>` and `complete_stream` returning a pinned boxed `Stream<Item = Result<Chunk, LlmError>>`** *(chosen)* — matches the constitution §4.3 sketch verbatim and is dyn-compatible (boxed stream avoids associated types in a `dyn LlmProvider`).
3. **Generic `complete<S: Sink<Chunk>>`** — most efficient, but breaks `dyn LlmProvider` (associated-type method-receiver issues with current Rust). Rejected to keep router heterogeneous-collection-friendly.

### Error taxonomy

1. **Single opaque `LlmError(String)`** — easy but loses retry semantics; rejected.
2. **Layered enum with retry-classification accessor** *(chosen)*: `Transport`, `RateLimit { retry_after }`, `Server { status, body }`, `BadRequest { status, body }`, `Auth`, `Decode`, `Stream`, `Cancelled`, `Provider { code, message }`. Each variant has `is_transient(&self) -> bool` and `is_provider_fault(&self) -> bool` so the router decides retry vs fall-through without leaking adapter internals.
3. **Wrap `reqwest::Error` directly** — leaks transport detail to consumers; rejected.

### Retry policy

1. **Fixed N retries with constant delay** — wastes budget, hammers rate-limited providers; rejected.
2. **Exponential backoff with full jitter, hard cap on attempts and total elapsed** *(chosen)*: base 250 ms, factor 2.0, full jitter `[0, base·2^attempt]`, max 5 attempts, max 30 s total elapsed. `Retry-After` from a `RateLimit` overrides the computed delay. Non-transient errors skip retry and fall through to the next preferred entry.
3. **Token-bucket per provider** — proper but premature; resurface in M5+ ADR if observed needed.

### Cache key canonicalisation

1. **Hash the raw `CompletionRequest` JSON** — order of struct fields and HashMap iteration produce different bytes on different machines; rejected.
2. **BLAKE3 of a canonicalised byte stream** *(chosen)*: serialise via `serde_json::to_vec` after sorting all map keys and rounding floating-point parameters to a documented decimal precision (we keep them as `Option<f32>` text, not floats, for the router-visible params). Concretely, the canonical bytes are:
   ```text
   blake3(
     b"cobrust-llm-router/v1\n"           ||
     model_id_utf8                  || b"\n" ||
     canon_params_json              || b"\n" ||
     canon_messages_json
   )
   ```
   `canon_params_json` is a JSON object whose keys are sorted lexicographically (`max_tokens`, `temperature`, `top_p`, `stop`, `extra`). `temperature` and `top_p` are serialised as their lossless `ryu` representation (matches `serde_json` default for f32). `canon_messages_json` is a JSON array of `{"role": ..., "content": ...}` objects in submitted order. This is reproducible bit-for-bit across machines provided the input request is.
3. **SHA-256 of the same canonical stream** — equivalent semantics; BLAKE3 chosen because it is already a workspace dependency and is faster.

### Ledger schema (`.cobrust/ledger.jsonl`)

Each line is one JSON object with a fixed field set. Append-only, never rewritten. Schema:

| Field | Type | Meaning |
|---|---|---|
| `ts` | RFC3339 string | UTC timestamp at completion observed time |
| `task` | string | `spec_extract` / `translate` / `repair` / `custom:<name>` |
| `provider` | string | provider key from `cobrust.toml` |
| `model` | string | model id as sent to provider |
| `cache_key` | string | `blake3:<hex>` of the canonical request bytes |
| `cache_hit` | bool | `true` if served from on-disk cache |
| `prompt_tokens` | u32 | from provider response (or 0 if unknown) |
| `completion_tokens` | u32 | from provider response (or 0 if unknown) |
| `total_tokens` | u32 | `prompt + completion` (computed if missing) |
| `latency_ms` | u32 | wall-clock for the provider call (excludes cache hit time) |
| `attempt` | u8 | retry attempt number (1-indexed; consensus shards each have their own attempt counter) |
| `outcome` | enum string | `ok` / `error_transient` / `error_permanent` |
| `error_code` | string \| null | structured tag from `LlmError::code()` when outcome ≠ ok |
| `consensus_group` | string \| null | nonzero only for consensus-mode dispatches; opaque per-dispatch UUIDv4 |

Writes go through a single `tokio::sync::Mutex<File>` opened with `O_APPEND` so concurrent writers cannot tear lines. Each line ends with `\n`. Files are line-addressable (`ledger.jsonl` parses as JSONL even if a line was partially written and the process died — at most the final line is partial; readers must tolerate that).

### Consensus tie-breaking

Consensus mode dispatches `n` parallel calls and picks one winner. Tie-breaking must be deterministic so a re-run with identical inputs picks the same winner. Options:

1. **First-listed wins** — easy, but ignores model agreement.
2. **Lexicographic on response hash** — deterministic but arbitrary.
3. **Vote on `BLAKE3(canonical_response_text)`; ties broken by lexicographic ascending hash; second-level tie broken by index in `preferred` list (lower index wins)** *(chosen)*. Algorithm:
   - For each successful shard, compute `h = BLAKE3(response.text.normalize_nfc())` truncated to 16 bytes hex.
   - Group shards by `h`. The largest group wins.
   - If multiple groups tie on size, pick the group whose `h` sorts smallest lexicographically.
   - Within the winning group, pick the shard whose `preferred[index]` is smallest.
   - If fewer than `ceil(n/2)` shards succeeded, return `LlmError::Server { status: 0, body: "consensus quorum lost" }` (router treats this as permanent for the consensus dispatch but does not retry the consensus as a whole).

The router records every shard's outcome in the ledger. Failed shards still bill latency but have `outcome: error_*` and `consensus_group` set so post-hoc analysis is possible.

## Decision

Adopt all chosen options above. The crate `cobrust-llm-router` has the following module layout at `crates/cobrust-llm-router/src/`:

```
lib.rs        — public re-exports + crate-level docs
provider.rs   — LlmProvider trait + CompletionRequest / CompletionResponse / Chunk / LlmError + Message + Role
config.rs     — RouterConfig parsing (matches cobrust.toml.example exactly)
cache.rs      — content-addressed on-disk cache (BLAKE3 canonical key)
ledger.rs     — append-only JSONL ledger
router.rs     — Router struct + dispatch + Strategy + RoutingTable
anthropic.rs  — Anthropic adapter (POST /v1/messages, x-api-key, SSE event: content_block_delta)
openai.rs     — OpenAI-compatible adapter (POST /chat/completions, Authorization: Bearer, SSE data:)
```

All public items live behind `pub use` re-exports in `lib.rs`. Internal helpers stay private.

The exact public surface:

```rust
// provider.rs
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError>;
    fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>;
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub params: SamplingParams,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Message { pub role: Role, pub content: String }

#[derive(Copy, Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role { System, User, Assistant }

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SamplingParams {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub stop: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompletionResponse {
    pub text: String,
    pub model: String,
    pub usage: TokenUsage,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage { pub prompt_tokens: u32, pub completion_tokens: u32 }

#[derive(Clone, Debug)]
pub enum Chunk { Delta(String), Done(TokenUsage) }

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("transport error: {0}")] Transport(String),
    #[error("rate-limited (retry after {retry_after_ms} ms)")]
    RateLimit { retry_after_ms: u64 },
    #[error("server error {status}: {body}")] Server { status: u16, body: String },
    #[error("bad request {status}: {body}")] BadRequest { status: u16, body: String },
    #[error("auth failure")] Auth,
    #[error("decode error: {0}")] Decode(String),
    #[error("stream error: {0}")] Stream(String),
    #[error("cancelled")] Cancelled,
    #[error("provider error {code}: {message}")]
    Provider { code: String, message: String },
}

impl LlmError {
    pub fn is_transient(&self) -> bool { /* RateLimit + 5xx Server + Transport => true */ }
    pub fn is_provider_fault(&self) -> bool { /* everything except Auth + BadRequest + Cancelled */ }
    pub fn code(&self) -> &'static str { /* short kebab-case tag for ledger */ }
}

// router.rs
pub enum Task { SpecExtract, Translate, Repair, Custom(String) }

pub enum Strategy { Cost, Quality, Latency, Consensus { n: u8 } }

pub struct Router { /* see config.rs for the constructor */ }

impl Router {
    pub fn from_config(cfg: &RouterConfig) -> Result<Self, RouterError>;
    pub async fn dispatch(&self, task: Task, req: CompletionRequest)
        -> Result<RouterResponse, RouterError>;
}

pub struct RouterResponse { pub response: CompletionResponse, pub provider: String, pub cache_hit: bool }

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("config: {0}")] Config(String),
    #[error("no provider for task")] NoProvider,
    #[error("all providers failed: {0:?}")] AllFailed(Vec<(String, LlmError)>),
    #[error("consensus quorum lost (need {need}, got {got})")]
    ConsensusQuorumLost { need: u8, got: u8 },
    #[error("io: {0}")] Io(String),
    #[error("llm: {0}")] Llm(LlmError),
}
```

`RouterConfig` mirrors `cobrust.toml.example` field-for-field. Parsing failures yield `RouterError::Config` with the offending key path.

The router instantiates each provider's `Arc<dyn LlmProvider>` once and reuses it. Cache and ledger handles are also long-lived `Arc`-shared.

Strategy semantics:

- **Cost**: walks `preferred` and picks the first model annotated with the lowest `cost_per_1k_tokens` (M3: lacking real cost data, ties broken by `preferred` order; ADR follow-up will bind real prices).
- **Quality**: first entry in `preferred`, fall through on transient error.
- **Latency**: first entry in `preferred` whose 30-call EWMA latency is lowest; cold-start ties resolve to `Quality`. Implemented as in-memory map keyed by `provider:model`.
- **Consensus { n }**: dispatches the first `n` distinct entries in `preferred` in parallel; tie-breaks per the rule above.

## Consequences

- **Positive**
  - Constitution §4.3 sketch is now binding and concrete.
  - Cache reproducibility across machines is provable (canonical bytes documented).
  - Ledger schema is mineable by simple tools (`jq`, `duckdb`).
  - Consensus is deterministic given identical shard outputs — no flakiness in CI.
  - Retry policy bounded → predictable wall-clock under provider faults.

- **Negative**
  - Provider trait pins `async_trait` macro and `Pin<Box<dyn Stream<...>>>`; both have small allocation cost. Acceptable for an LLM call dwarfed by network latency.
  - BLAKE3 was chosen over SHA-256 for speed; reviewers familiar with SHA-256 will need to read the canonical-bytes section to verify.
  - Ledger uses a process-wide mutex on the file. Multiple compiler processes sharing the same ledger could still interleave at the OS-syscall level; mitigated by `O_APPEND` semantics on Linux/macOS but not strictly guaranteed for arbitrary filesystems. M5+ may gate this behind a lockfile if needed.

- **Neutral / unknown**
  - Real cost-per-token tables don't exist yet; M3 `cost` strategy is best-effort (falls back to `quality`).
  - "Verifier-judged best-of-N" consensus is **not** in M3; only majority-with-tiebreak. Verifier judging waits for `mod:translator` (M4+) which knows what "good" means.

## Evidence

- Constitution `CLAUDE.md` §4.3 (router requirements + Rust API sketch).
- `docs/agent/modules/llm-router.md` (M3 done-means + invariants).
- `cobrust.toml.example` (configuration shape).
- Anthropic streaming spec — `event: content_block_delta` framing — see https://docs.anthropic.com/en/api/messages-streaming
- OpenAI chat-completions streaming spec — `data: <json>\n\n` lines, `data: [DONE]` terminator — see https://platform.openai.com/docs/api-reference/chat-streaming/streaming
- BLAKE3 1.5 documentation (workspace dep).
- ADR-0001 (license) and ADR-0002 (multi-agent topology) — context for delivery model.
