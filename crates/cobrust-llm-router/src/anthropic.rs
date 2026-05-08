//! Anthropic Messages-API adapter.
//!
//! Endpoint shape: `POST {base_url}/v1/messages` with an `x-api-key` header
//! and `anthropic-version: 2023-06-01`. Streaming uses SSE events of the
//! form
//! ```text
//! event: content_block_delta
//! data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"…"}}
//! ```
//! See `adr:0004` and the official Anthropic streaming spec
//! (https://docs.anthropic.com/en/api/messages-streaming).

use std::pin::Pin;
use std::time::Duration;

use futures::stream::{Stream, StreamExt};
use serde::Deserialize;

use crate::provider::{
    Chunk, CompletionRequest, CompletionResponse, LlmError, LlmProvider, Role, TokenUsage,
};

/// Anthropic Messages-API adapter.
pub struct AnthropicProvider {
    name: String,
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for AnthropicProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicProvider")
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl AnthropicProvider {
    /// Construct a new adapter. `api_key` is the value pulled from the
    /// configured environment variable.
    ///
    /// # Errors
    /// Returns the underlying `reqwest::Error` if the HTTP client cannot be
    /// constructed.
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;
        Ok(Self {
            name: name.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            client,
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/messages", self.base_url)
    }

    fn build_body(req: &CompletionRequest, stream: bool) -> serde_json::Value {
        // Anthropic separates `system` from `messages`. We extract the first
        // contiguous block of System-role messages and concatenate.
        let mut system_chunks: Vec<&str> = Vec::new();
        let mut convo: Vec<serde_json::Value> = Vec::new();
        for m in &req.messages {
            match m.role {
                Role::System => system_chunks.push(&m.content),
                Role::User => convo.push(serde_json::json!({"role": "user", "content": m.content})),
                Role::Assistant => {
                    convo.push(serde_json::json!({"role": "assistant", "content": m.content}));
                }
            }
        }
        let mut body = serde_json::json!({
            "model": req.model,
            "messages": convo,
            "max_tokens": req.params.max_tokens.unwrap_or(1024),
        });
        if !system_chunks.is_empty() {
            body["system"] = serde_json::Value::String(system_chunks.join("\n\n"));
        }
        if let Some(t) = req.params.temperature {
            body["temperature"] = serde_json::json!(t);
        }
        if let Some(p) = req.params.top_p {
            body["top_p"] = serde_json::json!(p);
        }
        if !req.params.stop.is_empty() {
            body["stop_sequences"] = serde_json::json!(req.params.stop);
        }
        if stream {
            body["stream"] = serde_json::Value::Bool(true);
        }
        body
    }

    fn classify_status(status: u16, body: String) -> LlmError {
        match status {
            401 | 403 => LlmError::Auth,
            429 => LlmError::RateLimit { retry_after_ms: 0 },
            400..=499 => LlmError::BadRequest { status, body },
            _ => LlmError::Server { status, body },
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> crate::config::ProviderKind {
        crate::config::ProviderKind::Anthropic
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let body = Self::build_body(&req, false);
        let resp = self
            .client
            .post(self.endpoint())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        if !(200..300).contains(&status) {
            return Err(Self::classify_status(
                status,
                String::from_utf8_lossy(&bytes).into_owned(),
            ));
        }
        let parsed: AnthropicMessage =
            serde_json::from_slice(&bytes).map_err(|e| LlmError::Decode(e.to_string()))?;
        let text: String = parsed
            .content
            .into_iter()
            .map(|b| match b {
                AnthropicBlock::Text { text } => text,
            })
            .collect();
        let usage = TokenUsage {
            prompt_tokens: parsed.usage.input_tokens,
            completion_tokens: parsed.usage.output_tokens,
        };
        Ok(CompletionResponse {
            text,
            model: parsed.model.unwrap_or(req.model),
            usage,
        })
    }

    fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>> {
        let endpoint = self.endpoint();
        let api_key = self.api_key.clone();
        let body = Self::build_body(&req, true);
        Box::pin(anthropic_stream(
            self.client.clone(),
            endpoint,
            api_key,
            body,
        ))
    }
}

/// Drives the SSE GET and yields `Chunk`s. Emits exactly one `Chunk::Done`
/// when the upstream stream ends.
fn anthropic_stream(
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    body: serde_json::Value,
) -> impl Stream<Item = Result<Chunk, LlmError>> + Send {
    async_stream_yield(async move {
        let resp = client
            .post(&endpoint)
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            let err_body = resp
                .text()
                .await
                .unwrap_or_else(|_| String::from("(no body)"));
            return Err(AnthropicProvider::classify_status(status, err_body));
        }
        let mut buf = String::new();
        let mut byte_stream = resp.bytes_stream();
        let mut total_usage = TokenUsage::default();
        let mut yielded: Vec<Chunk> = Vec::new();
        while let Some(item) = byte_stream.next().await {
            let bytes = item.map_err(|e| LlmError::Stream(e.to_string()))?;
            buf.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(idx) = buf.find("\n\n") {
                let frame = buf[..idx].to_string();
                buf.drain(..=idx + 1);
                let mut event_name: Option<&str> = None;
                let mut data_payload: Option<&str> = None;
                for line in frame.lines() {
                    if let Some(rest) = line.strip_prefix("event:") {
                        event_name = Some(rest.trim());
                    } else if let Some(rest) = line.strip_prefix("data:") {
                        data_payload = Some(rest.trim());
                    }
                }
                let (Some(name), Some(payload)) = (event_name, data_payload) else {
                    continue;
                };
                match name {
                    "content_block_delta" => {
                        let parsed: SseDelta = serde_json::from_str(payload)
                            .map_err(|e| LlmError::Decode(e.to_string()))?;
                        if let Some(text) = parsed.delta.text {
                            yielded.push(Chunk::Delta(text));
                        }
                    }
                    "message_delta" => {
                        let parsed: SseMessageDelta = serde_json::from_str(payload)
                            .map_err(|e| LlmError::Decode(e.to_string()))?;
                        if let Some(u) = parsed.usage {
                            total_usage.completion_tokens = total_usage
                                .completion_tokens
                                .saturating_add(u.output_tokens);
                            if let Some(it) = u.input_tokens {
                                total_usage.prompt_tokens = it;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        yielded.push(Chunk::Done(total_usage));
        Ok(yielded)
    })
}

/// Adapter for a "yield Vec at once at the end" model into a stream interface.
/// We accumulate chunks during the SSE drain, then emit them in order.
fn async_stream_yield<F>(fut: F) -> impl Stream<Item = Result<Chunk, LlmError>> + Send
where
    F: std::future::Future<Output = Result<Vec<Chunk>, LlmError>> + Send + 'static,
{
    futures::stream::once(fut).flat_map(|res| match res {
        Ok(chunks) => futures::stream::iter(chunks.into_iter().map(Ok)).left_stream(),
        Err(e) => futures::stream::once(async move { Err(e) }).right_stream(),
    })
}

// ---- Anthropic JSON ---------------------------------------------------------

#[derive(Deserialize)]
struct AnthropicMessage {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    content: Vec<AnthropicBlock>,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicBlock {
    Text { text: String },
}

#[derive(Deserialize, Default)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[derive(Deserialize)]
struct SseDelta {
    delta: SseDeltaInner,
}

#[derive(Deserialize)]
struct SseDeltaInner {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct SseMessageDelta {
    #[serde(default)]
    usage: Option<SseMessageDeltaUsage>,
}

#[derive(Deserialize)]
struct SseMessageDeltaUsage {
    #[serde(default)]
    input_tokens: Option<u32>,
    #[serde(default)]
    output_tokens: u32,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::provider::{Message, Role, SamplingParams};

    #[test]
    fn build_body_extracts_system_and_keeps_messages() {
        let req = CompletionRequest {
            model: "claude-opus-4-7".into(),
            messages: vec![
                Message {
                    role: Role::System,
                    content: "be terse".into(),
                },
                Message {
                    role: Role::User,
                    content: "hi".into(),
                },
            ],
            params: SamplingParams {
                max_tokens: Some(64),
                temperature: Some(0.2),
                top_p: None,
                stop: vec!["END".into()],
            },
        };
        let body = AnthropicProvider::build_body(&req, true);
        assert_eq!(body["model"], "claude-opus-4-7");
        assert_eq!(body["system"], "be terse");
        assert_eq!(body["max_tokens"], 64);
        assert_eq!(body["stream"], true);
        let temp = body["temperature"].as_f64().expect("number");
        assert!((temp - 0.2).abs() < 1e-3, "temperature drift {temp}");
        assert_eq!(body["stop_sequences"], serde_json::json!(["END"]));
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "hi");
    }

    #[test]
    fn classify_status_maps_to_correct_variants() {
        assert!(matches!(
            AnthropicProvider::classify_status(401, String::new()),
            LlmError::Auth
        ));
        assert!(matches!(
            AnthropicProvider::classify_status(429, String::new()),
            LlmError::RateLimit { .. }
        ));
        assert!(matches!(
            AnthropicProvider::classify_status(400, String::new()),
            LlmError::BadRequest { .. }
        ));
        assert!(matches!(
            AnthropicProvider::classify_status(503, String::new()),
            LlmError::Server { .. }
        ));
    }
}
