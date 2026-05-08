//! Provider trait and shared completion types.
//!
//! Pinned by `adr:0004`. The `LlmProvider` trait is the only interface the
//! [`Router`](crate::Router) speaks to; concrete adapters
//! ([`AnthropicProvider`](crate::anthropic::AnthropicProvider) and
//! [`OpenAiProvider`](crate::openai::OpenAiProvider)) live in sibling modules.

use std::pin::Pin;

use futures::stream::Stream;

/// Conversation role. Mirrors the Cobrust translation subsystem's three-role
/// abstraction, intentionally narrower than provider-native role sets.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// System-level instructions; sent as `system` to providers that support it.
    System,
    /// User turn.
    User,
    /// Assistant turn (used in few-shot or multi-turn contexts).
    Assistant,
}

/// One conversational turn.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// Provider-agnostic sampling parameters.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SamplingParams {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    #[serde(default)]
    pub stop: Vec<String>,
}

impl Eq for SamplingParams {}

/// A canonical request as seen by [`LlmProvider`]. The router owns the
/// translation from a `Task` + `Prompt` into this; the cache key is computed
/// from the canonical bytes of this request (see [`crate::cache`]).
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub params: SamplingParams,
}

/// Token usage as reported by the provider. Both fields default to zero when
/// the provider does not surface them.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

impl TokenUsage {
    #[must_use]
    pub fn total(self) -> u32 {
        self.prompt_tokens.saturating_add(self.completion_tokens)
    }
}

/// A non-streaming completion as returned by [`LlmProvider::complete`].
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompletionResponse {
    pub text: String,
    pub model: String,
    #[serde(default)]
    pub usage: TokenUsage,
}

/// One streamed event produced by [`LlmProvider::complete_stream`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Chunk {
    /// Incremental text delta.
    Delta(String),
    /// Final usage frame; emitted exactly once at end-of-stream.
    Done(TokenUsage),
}

/// All errors the provider layer can surface. Variants carry enough metadata
/// for the router's retry classifier to decide between retry, fall-through,
/// and permanent failure (see [`LlmError::is_transient`]).
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// Network-level transport failure (DNS, TCP, TLS, idle timeout). Transient.
    #[error("transport error: {0}")]
    Transport(String),
    /// Provider rate-limit (HTTP 429, or analogue). Transient; honour the
    /// embedded `Retry-After` value if non-zero.
    #[error("rate-limited (retry after {retry_after_ms} ms)")]
    RateLimit { retry_after_ms: u64 },
    /// Provider-side server error (HTTP 5xx). Transient.
    #[error("server error {status}: {body}")]
    Server { status: u16, body: String },
    /// Client-side malformed request (HTTP 4xx other than 401/403/429).
    /// Permanent — the prompt itself is the bug.
    #[error("bad request {status}: {body}")]
    BadRequest { status: u16, body: String },
    /// Authentication or authorization failure (HTTP 401/403). Permanent for
    /// this provider; router falls through.
    #[error("auth failure")]
    Auth,
    /// Failure to decode a provider response body. Permanent; indicates a
    /// schema drift on the provider side.
    #[error("decode error: {0}")]
    Decode(String),
    /// SSE-stream level failure (truncated, malformed event, etc.). Transient.
    #[error("stream error: {0}")]
    Stream(String),
    /// User cancelled the request. Permanent for this dispatch, never retried.
    #[error("cancelled")]
    Cancelled,
    /// Provider-application error not otherwise classifiable.
    #[error("provider error {code}: {message}")]
    Provider { code: String, message: String },
}

impl Clone for LlmError {
    fn clone(&self) -> Self {
        match self {
            Self::Transport(s) => Self::Transport(s.clone()),
            Self::RateLimit { retry_after_ms } => Self::RateLimit {
                retry_after_ms: *retry_after_ms,
            },
            Self::Server { status, body } => Self::Server {
                status: *status,
                body: body.clone(),
            },
            Self::BadRequest { status, body } => Self::BadRequest {
                status: *status,
                body: body.clone(),
            },
            Self::Auth => Self::Auth,
            Self::Decode(s) => Self::Decode(s.clone()),
            Self::Stream(s) => Self::Stream(s.clone()),
            Self::Cancelled => Self::Cancelled,
            Self::Provider { code, message } => Self::Provider {
                code: code.clone(),
                message: message.clone(),
            },
        }
    }
}

impl LlmError {
    /// Whether the router should retry the same provider on this error.
    /// Per `adr:0004`: transport, rate-limit, 5xx, stream errors are transient.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Transport(_) | Self::RateLimit { .. } | Self::Server { .. } | Self::Stream(_)
        )
    }

    /// Whether responsibility for the failure lies with the provider rather
    /// than the caller. Drives the fall-through-to-next-preferred decision.
    #[must_use]
    pub fn is_provider_fault(&self) -> bool {
        !matches!(self, Self::BadRequest { .. } | Self::Cancelled)
    }

    /// Short kebab-case tag used in the ledger's `error_code` field. Stable.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Transport(_) => "transport",
            Self::RateLimit { .. } => "rate-limit",
            Self::Server { .. } => "server",
            Self::BadRequest { .. } => "bad-request",
            Self::Auth => "auth",
            Self::Decode(_) => "decode",
            Self::Stream(_) => "stream",
            Self::Cancelled => "cancelled",
            Self::Provider { .. } => "provider",
        }
    }
}

/// Provider abstraction. Concrete adapters (Anthropic, OpenAI-compatible) and
/// the synthetic test double all implement this trait.
///
/// Instances must be cheap to clone via `Arc` because the router holds a
/// shared reference and dispatches concurrently for `Strategy::Consensus`.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Stable provider key as registered in `cobrust.toml`
    /// (e.g. `"anthropic_official"`). The router uses this to record ledger
    /// entries and to deduplicate consensus shards.
    fn name(&self) -> &str;

    /// Wire-protocol kind. The router records this in each ledger entry so
    /// historical analysis (cost, incident postmortems, differential
    /// debugging) can reason about provider protocol without
    /// cross-referencing `cobrust.toml`. Pinned by `adr:0031`.
    fn kind(&self) -> crate::config::ProviderKind;

    /// Issue a single non-streaming completion.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError>;

    /// Issue a streaming completion. The returned stream emits any number of
    /// [`Chunk::Delta`] frames followed by **exactly one** [`Chunk::Done`]
    /// frame; if the provider does not surface usage data, the `Done` frame
    /// carries [`TokenUsage::default`].
    fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn token_usage_total_is_sum() {
        let usage = TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 12,
        };
        assert_eq!(usage.total(), 22);
    }

    #[test]
    fn token_usage_total_saturates() {
        let usage = TokenUsage {
            prompt_tokens: u32::MAX,
            completion_tokens: 5,
        };
        assert_eq!(usage.total(), u32::MAX);
    }

    #[test]
    fn llm_error_transient_classification_matches_adr0004() {
        assert!(LlmError::Transport("dns".into()).is_transient());
        assert!(
            LlmError::RateLimit {
                retry_after_ms: 500
            }
            .is_transient()
        );
        assert!(
            LlmError::Server {
                status: 503,
                body: String::new(),
            }
            .is_transient()
        );
        assert!(LlmError::Stream("eof".into()).is_transient());
        assert!(!LlmError::Auth.is_transient());
        assert!(
            !LlmError::BadRequest {
                status: 400,
                body: String::new(),
            }
            .is_transient()
        );
        assert!(!LlmError::Decode("bad".into()).is_transient());
        assert!(!LlmError::Cancelled.is_transient());
    }

    #[test]
    fn llm_error_provider_fault_excludes_caller_errors() {
        assert!(LlmError::Auth.is_provider_fault());
        assert!(
            LlmError::Server {
                status: 500,
                body: String::new()
            }
            .is_provider_fault()
        );
        assert!(
            !LlmError::BadRequest {
                status: 400,
                body: String::new()
            }
            .is_provider_fault()
        );
        assert!(!LlmError::Cancelled.is_provider_fault());
    }

    #[test]
    fn llm_error_code_is_stable_kebab() {
        assert_eq!(
            LlmError::RateLimit { retry_after_ms: 0 }.code(),
            "rate-limit"
        );
        assert_eq!(
            LlmError::BadRequest {
                status: 400,
                body: String::new()
            }
            .code(),
            "bad-request"
        );
        assert_eq!(LlmError::Cancelled.code(), "cancelled");
    }
}
