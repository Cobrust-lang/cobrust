//! OpenAI-compatible adapter (works against `api.openai.com`,
//! `api.deepseek.com`, vLLM, OpenRouter, Together, …).
//!
//! Endpoint: `POST {base_url}/chat/completions`. Auth via
//! `Authorization: Bearer <api_key>`. Streaming uses SSE
//! `data: {chunk}\n\n` lines, terminated by `data: [DONE]`.
//!
//! See `adr:0004` and the OpenAI streaming spec
//! (https://platform.openai.com/docs/api-reference/chat-streaming/streaming).

use std::pin::Pin;
use std::time::Duration;

use futures::stream::{Stream, StreamExt};
use serde::Deserialize;

use crate::provider::{
    Chunk, CompletionRequest, CompletionResponse, LlmError, LlmProvider, Role, TokenUsage,
};

/// OpenAI-compatible adapter.
pub struct OpenAiProvider {
    name: String,
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for OpenAiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiProvider")
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl OpenAiProvider {
    /// Construct a new adapter. The base URL must already include the API
    /// prefix (e.g. `https://api.openai.com/v1`) — the adapter appends
    /// `/chat/completions`.
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
        format!("{}/chat/completions", self.base_url)
    }

    fn build_body(req: &CompletionRequest, stream: bool) -> serde_json::Value {
        let messages: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };
                serde_json::json!({"role": role, "content": m.content})
            })
            .collect();
        let mut body = serde_json::json!({
            "model": req.model,
            "messages": messages,
        });
        if let Some(t) = req.params.max_tokens {
            body["max_tokens"] = serde_json::json!(t);
        }
        if let Some(t) = req.params.temperature {
            body["temperature"] = serde_json::json!(t);
        }
        if let Some(p) = req.params.top_p {
            body["top_p"] = serde_json::json!(p);
        }
        if !req.params.stop.is_empty() {
            body["stop"] = serde_json::json!(req.params.stop);
        }
        if stream {
            body["stream"] = serde_json::Value::Bool(true);
            body["stream_options"] = serde_json::json!({"include_usage": true});
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
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let body = Self::build_body(&req, false);
        let resp = self
            .client
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
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
        let parsed: ChatCompletion =
            serde_json::from_slice(&bytes).map_err(|e| LlmError::Decode(e.to_string()))?;
        let text = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.and_then(|m| m.content))
            .unwrap_or_default();
        let usage = parsed.usage.unwrap_or_default();
        Ok(CompletionResponse {
            text,
            model: parsed.model.unwrap_or(req.model),
            usage: TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
            },
        })
    }

    fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>> {
        let endpoint = self.endpoint();
        let api_key = self.api_key.clone();
        let body = Self::build_body(&req, true);
        let client = self.client.clone();
        Box::pin(async_stream_yield(async move {
            let resp = client
                .post(&endpoint)
                .bearer_auth(&api_key)
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
                return Err(OpenAiProvider::classify_status(status, err_body));
            }
            let mut buf = String::new();
            let mut stream = resp.bytes_stream();
            let mut yielded: Vec<Chunk> = Vec::new();
            let mut total_usage = TokenUsage::default();
            'outer: while let Some(item) = stream.next().await {
                let bytes = item.map_err(|e| LlmError::Stream(e.to_string()))?;
                buf.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(idx) = buf.find("\n\n") {
                    let frame = buf[..idx].to_string();
                    buf.drain(..=idx + 1);
                    for line in frame.lines() {
                        let payload = line.strip_prefix("data:").map(str::trim);
                        let Some(payload) = payload else { continue };
                        if payload == "[DONE]" {
                            break 'outer;
                        }
                        let parsed: ChatChunk = serde_json::from_str(payload)
                            .map_err(|e| LlmError::Decode(e.to_string()))?;
                        if let Some(usage) = parsed.usage {
                            total_usage = TokenUsage {
                                prompt_tokens: usage.prompt_tokens,
                                completion_tokens: usage.completion_tokens,
                            };
                        }
                        if let Some(choice) = parsed.choices.into_iter().next()
                            && let Some(delta) = choice.delta
                            && let Some(text) = delta.content
                            && !text.is_empty()
                        {
                            yielded.push(Chunk::Delta(text));
                        }
                    }
                }
            }
            yielded.push(Chunk::Done(total_usage));
            Ok(yielded)
        }))
    }
}

fn async_stream_yield<F>(fut: F) -> impl Stream<Item = Result<Chunk, LlmError>> + Send
where
    F: std::future::Future<Output = Result<Vec<Chunk>, LlmError>> + Send + 'static,
{
    futures::stream::once(fut).flat_map(|res| match res {
        Ok(chunks) => futures::stream::iter(chunks.into_iter().map(Ok)).left_stream(),
        Err(e) => futures::stream::once(async move { Err(e) }).right_stream(),
    })
}

// ---- OpenAI JSON ------------------------------------------------------------

#[derive(Deserialize)]
struct ChatCompletion {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: Option<ChatMessage>,
}

#[derive(Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize, Default)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Deserialize)]
struct ChatChunk {
    #[serde(default)]
    choices: Vec<ChatChunkChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChunkChoice {
    #[serde(default)]
    delta: Option<ChatChunkDelta>,
}

#[derive(Deserialize)]
struct ChatChunkDelta {
    #[serde(default)]
    content: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::provider::{Message, Role, SamplingParams};

    #[test]
    fn build_body_emits_chat_completions_shape() {
        let req = CompletionRequest {
            model: "gpt-5".into(),
            messages: vec![
                Message {
                    role: Role::System,
                    content: "system".into(),
                },
                Message {
                    role: Role::User,
                    content: "u".into(),
                },
                Message {
                    role: Role::Assistant,
                    content: "a".into(),
                },
            ],
            params: SamplingParams {
                max_tokens: Some(32),
                temperature: Some(0.5),
                top_p: Some(0.9),
                stop: vec!["X".into()],
            },
        };
        let body = OpenAiProvider::build_body(&req, true);
        assert_eq!(body["model"], "gpt-5");
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(body["max_tokens"], 32);
        let temp = body["temperature"].as_f64().expect("number");
        assert!((temp - 0.5).abs() < 1e-3, "temperature drift {temp}");
        let top_p = body["top_p"].as_f64().expect("number");
        assert!((top_p - 0.9).abs() < 1e-3, "top_p drift {top_p}");
        assert_eq!(body["stop"], serde_json::json!(["X"]));
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn classify_status_maps_to_correct_variants() {
        assert!(matches!(
            OpenAiProvider::classify_status(401, String::new()),
            LlmError::Auth
        ));
        assert!(matches!(
            OpenAiProvider::classify_status(429, String::new()),
            LlmError::RateLimit { .. }
        ));
        assert!(matches!(
            OpenAiProvider::classify_status(404, String::new()),
            LlmError::BadRequest { .. }
        ));
        assert!(matches!(
            OpenAiProvider::classify_status(502, String::new()),
            LlmError::Server { .. }
        ));
    }
}
