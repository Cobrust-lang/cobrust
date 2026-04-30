//! HTTP-level adapter tests using `wiremock` to mock real provider endpoints.
//!
//! Verifies:
//! - Anthropic adapter posts the right body and parses the response.
//! - OpenAI-compatible adapter posts the right body and parses the response.
//! - Both adapters drain SSE streams and emit exactly one `Done` frame.
//! - Both adapters classify HTTP errors into the right `LlmError` variants.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)]

use cobrust_llm_router::{
    AnthropicProvider, Chunk, CompletionRequest, LlmError, LlmProvider, Message, OpenAiProvider,
    Role, SamplingParams,
};
use futures::StreamExt;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn req(model: &str) -> CompletionRequest {
    CompletionRequest {
        model: model.into(),
        messages: vec![
            Message {
                role: Role::System,
                content: "system".into(),
            },
            Message {
                role: Role::User,
                content: "hello".into(),
            },
        ],
        params: SamplingParams {
            max_tokens: Some(64),
            temperature: Some(0.0),
            top_p: None,
            stop: vec![],
        },
    }
}

#[tokio::test]
async fn anthropic_complete_round_trips_a_text_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "secret"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude-opus-4-7",
            "content": [
                {"type": "text", "text": "Hello!"}
            ],
            "usage": {"input_tokens": 5, "output_tokens": 2}
        })))
        .mount(&server)
        .await;
    let adapter = AnthropicProvider::new("anthropic_official", server.uri(), "secret").unwrap();
    let resp = adapter.complete(req("claude-opus-4-7")).await.unwrap();
    assert_eq!(resp.text, "Hello!");
    assert_eq!(resp.model, "claude-opus-4-7");
    assert_eq!(resp.usage.prompt_tokens, 5);
    assert_eq!(resp.usage.completion_tokens, 2);
}

#[tokio::test]
async fn anthropic_streams_content_block_delta_events() {
    let server = MockServer::start().await;
    let sse = "event: message_start\n\
               data: {\"type\":\"message_start\",\"message\":{\"id\":\"m1\",\"role\":\"assistant\"}}\n\n\
               event: content_block_start\n\
               data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
               event: content_block_delta\n\
               data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n\
               event: content_block_delta\n\
               data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\", world\"}}\n\n\
               event: content_block_stop\n\
               data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
               event: message_delta\n\
               data: {\"type\":\"message_delta\",\"delta\":{},\"usage\":{\"output_tokens\":4}}\n\n\
               event: message_stop\n\
               data: {\"type\":\"message_stop\"}\n\n";
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;
    let adapter = AnthropicProvider::new("anthropic_official", server.uri(), "secret").unwrap();
    let mut text = String::new();
    let mut done_count = 0;
    let mut stream = adapter.complete_stream(req("claude-opus-4-7"));
    while let Some(item) = stream.next().await {
        match item.unwrap() {
            Chunk::Delta(s) => text.push_str(&s),
            Chunk::Done(_) => done_count += 1,
        }
    }
    assert_eq!(text, "Hello, world");
    assert_eq!(done_count, 1, "exactly one Done frame");
}

#[tokio::test]
async fn anthropic_classifies_401_as_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorised"))
        .mount(&server)
        .await;
    let adapter = AnthropicProvider::new("p", server.uri(), "bad").unwrap();
    let err = adapter.complete(req("any")).await.unwrap_err();
    assert!(matches!(err, LlmError::Auth), "expected Auth, got {err:?}");
}

#[tokio::test]
async fn anthropic_classifies_503_as_server_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(503).set_body_string("upstream"))
        .mount(&server)
        .await;
    let adapter = AnthropicProvider::new("p", server.uri(), "bad").unwrap();
    let err = adapter.complete(req("any")).await.unwrap_err();
    assert!(
        matches!(err, LlmError::Server { status: 503, .. }),
        "expected Server 503, got {err:?}"
    );
    assert!(err.is_transient());
}

#[tokio::test]
async fn openai_complete_round_trips_a_text_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "cmpl-1",
            "object": "chat.completion",
            "model": "gpt-5",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Greetings."},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10}
        })))
        .mount(&server)
        .await;
    let adapter = OpenAiProvider::new("openai_official", server.uri(), "secret").unwrap();
    let resp = adapter.complete(req("gpt-5")).await.unwrap();
    assert_eq!(resp.text, "Greetings.");
    assert_eq!(resp.model, "gpt-5");
    assert_eq!(resp.usage.prompt_tokens, 7);
    assert_eq!(resp.usage.completion_tokens, 3);
}

#[tokio::test]
async fn openai_streams_chat_chunk_data_lines() {
    let server = MockServer::start().await;
    let sse = "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"}}]}\n\n\
               data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\", world\"}}]}\n\n\
               data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":4,\"total_tokens\":11}}\n\n\
               data: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;
    let adapter = OpenAiProvider::new("openai_official", server.uri(), "k").unwrap();
    let mut stream = adapter.complete_stream(req("gpt-5"));
    let mut text = String::new();
    let mut done_count = 0;
    let mut last_usage = None;
    while let Some(item) = stream.next().await {
        match item.unwrap() {
            Chunk::Delta(s) => text.push_str(&s),
            Chunk::Done(u) => {
                done_count += 1;
                last_usage = Some(u);
            }
        }
    }
    assert_eq!(text, "Hello, world");
    assert_eq!(done_count, 1);
    let u = last_usage.expect("done frame");
    assert_eq!(u.prompt_tokens, 7);
    assert_eq!(u.completion_tokens, 4);
}

#[tokio::test]
async fn openai_classifies_429_as_rate_limit() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
        .mount(&server)
        .await;
    let adapter = OpenAiProvider::new("openai_official", server.uri(), "k").unwrap();
    let err = adapter.complete(req("gpt-5")).await.unwrap_err();
    assert!(matches!(err, LlmError::RateLimit { .. }));
    assert!(err.is_transient());
}

#[tokio::test]
async fn openai_compatible_works_against_arbitrary_base_url() {
    // Simulate DeepSeek (or any OpenAI-compatible endpoint with a different
    // base URL). The adapter must work uniformly.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer ds-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "deepseek-v3",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "from-deepseek"}
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
        })))
        .mount(&server)
        .await;
    let adapter = OpenAiProvider::new("deepseek", server.uri(), "ds-key").unwrap();
    let resp = adapter.complete(req("deepseek-v3")).await.unwrap();
    assert_eq!(resp.text, "from-deepseek");
    assert_eq!(resp.usage.completion_tokens, 2);
}
