//! Cobrust LLM Router — first-class compiler subsystem.
//!
//! Entry point for the Cobrust translation pipeline's heterogeneous LLM
//! dispatch. The router is *not* a chat UI, *not* a long-running agent
//! loop driver, and *not* a prompt template store; see
//! `docs/agent/modules/llm-router.md` for the full non-goals list.
//!
//! # Architecture
//!
//! - [`provider`] — `LlmProvider` async trait + shared completion types
//!   (`CompletionRequest`, `CompletionResponse`, `Chunk`, `LlmError`).
//! - [`anthropic`] — Anthropic Messages-API adapter (POST `/v1/messages`,
//!   SSE `event: content_block_delta`).
//! - [`openai`] — OpenAI-compatible adapter (POST `/chat/completions`,
//!   SSE `data: {chunk}\n\n`). Works against any OpenAI-compatible base URL
//!   (`api.openai.com`, `api.deepseek.com`, vLLM, OpenRouter, …).
//! - [`cache`] — content-addressed on-disk cache; key =
//!   `BLAKE3(canonical_request_bytes)`.
//! - [`ledger`] — append-only JSONL token ledger.
//! - [`config`] — `cobrust.toml` parsing.
//! - [`router`] — strategy + dispatch + retry + consensus tie-breaking.
//!
//! # Architecture decision
//!
//! All load-bearing decisions are pinned by `adr:0004` (see
//! `docs/agent/adr/0004-llm-router-architecture.md`).

pub mod anthropic;
pub mod cache;
pub mod config;
pub mod ledger;
pub mod openai;
pub mod provider;
pub mod router;

// Public re-exports — keep the surface small and declarative.
pub use crate::anthropic::AnthropicProvider;
pub use crate::cache::{Cache, CacheKey};
pub use crate::config::{
    DefaultStrategy, ProviderConfig, ProviderKind, ProviderModel, RouterConfig, RoutingEntry,
    StrategyName,
};
pub use crate::ledger::{Ledger, LedgerEntry, Outcome};
pub use crate::openai::OpenAiProvider;
pub use crate::provider::{
    Chunk, CompletionRequest, CompletionResponse, LlmError, LlmProvider, Message, Role,
    SamplingParams, TokenUsage,
};
pub use crate::router::{
    RetryPolicy, Router, RouterBuilder, RouterError, RouterResponse, Strategy, Task,
};
