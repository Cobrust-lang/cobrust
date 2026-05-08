#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)]
//! Synthetic-provider integration test for the LLM Router.
//!
//! This is the M3 acceptance harness: cache hit/miss, ledger append-only,
//! consensus tie-breaking determinism, provider failure isolation,
//! streaming round-trip — all exercised through the public `Router` API
//! using in-process `LlmProvider` doubles. No network.
//!
//! See `adr:0004` for the binding decisions this harness verifies.

use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use futures::stream::{self, Stream, StreamExt};

use cobrust_llm_router::{
    Cache, CacheKey, Chunk, CompletionRequest, CompletionResponse, LedgerEntry, LlmError,
    LlmProvider, Message, Outcome, Role, RouterBuilder, RouterConfig, RouterError, RouterResponse,
    SamplingParams, Task, TokenUsage,
};

// ---- Synthetic provider double ---------------------------------------------

/// One scripted attempt: either a successful response or a specific error.
#[derive(Clone)]
enum Scripted {
    Ok(String),
    Err(LlmError),
}

/// Synthetic provider that replays a scripted sequence and counts calls.
struct SyntheticProvider {
    name: String,
    script: Mutex<Vec<Scripted>>,
    calls: AtomicU32,
}

impl SyntheticProvider {
    fn new(name: &str, script: Vec<Scripted>) -> Arc<Self> {
        Arc::new(Self {
            name: name.to_string(),
            script: Mutex::new(script),
            calls: AtomicU32::new(0),
        })
    }
}

#[async_trait::async_trait]
impl LlmProvider for SyntheticProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> cobrust_llm_router::ProviderKind {
        cobrust_llm_router::ProviderKind::Synthetic
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut script = self.script.lock().expect("synthetic script poisoned");
        let next = if script.is_empty() {
            return Err(LlmError::Provider {
                code: "exhausted".into(),
                message: "no scripted attempts left".into(),
            });
        } else {
            script.remove(0)
        };
        match next {
            Scripted::Ok(text) => Ok(CompletionResponse {
                text,
                model: req.model,
                usage: TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 2,
                },
            }),
            Scripted::Err(err) => Err(err),
        }
    }

    fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>> {
        let outcome = futures::executor::block_on(self.complete(req));
        match outcome {
            Ok(resp) => {
                // Yield the text in two halves to exercise the streaming
                // contract, then a Done frame with the usage.
                let chunks = if resp.text.len() < 2 {
                    vec![Chunk::Delta(resp.text.clone()), Chunk::Done(resp.usage)]
                } else {
                    let mid = resp.text.len() / 2;
                    let left = resp.text[..mid].to_string();
                    let right = resp.text[mid..].to_string();
                    vec![
                        Chunk::Delta(left),
                        Chunk::Delta(right),
                        Chunk::Done(resp.usage),
                    ]
                };
                Box::pin(stream::iter(chunks.into_iter().map(Ok)))
            }
            Err(err) => Box::pin(stream::once(async move { Err(err) })),
        }
    }
}

// ---- Helpers ----------------------------------------------------------------

fn req(text: &str) -> CompletionRequest {
    CompletionRequest {
        model: "synth-1".into(),
        messages: vec![Message {
            role: Role::User,
            content: text.into(),
        }],
        params: SamplingParams {
            max_tokens: Some(64),
            temperature: Some(0.0),
            top_p: None,
            stop: vec![],
        },
    }
}

fn cfg_two_provider_quality(cache: &str, ledger: &str) -> RouterConfig {
    let toml = format!(
        r#"
[router]
default_strategy = "quality"
cache_dir = "{cache}"
ledger_path = "{ledger}"

[providers.alpha]
kind = "openai"
base_url = "http://x"
api_key_env = "ALPHA_KEY"
models = ["synth-1"]

[providers.beta]
kind = "openai"
base_url = "http://x"
api_key_env = "BETA_KEY"
models = ["synth-1"]

[routing.translate]
strategy = "quality"
preferred = ["alpha:synth-1", "beta:synth-1"]
"#
    );
    RouterConfig::from_toml_str(&toml).expect("config parses")
}

fn cfg_consensus(cache: &str, ledger: &str) -> RouterConfig {
    let toml = format!(
        r#"
[router]
default_strategy = "quality"
cache_dir = "{cache}"
ledger_path = "{ledger}"

[providers.alpha]
kind = "openai"
base_url = "http://x"
api_key_env = "K"
models = ["synth-1"]

[providers.beta]
kind = "openai"
base_url = "http://x"
api_key_env = "K"
models = ["synth-1"]

[providers.gamma]
kind = "openai"
base_url = "http://x"
api_key_env = "K"
models = ["synth-1"]

[routing.translate]
strategy = "consensus"
n = 3
preferred = ["alpha:synth-1", "beta:synth-1", "gamma:synth-1"]
"#
    );
    RouterConfig::from_toml_str(&toml).expect("config parses")
}

fn read_ledger(path: &std::path::Path) -> Vec<LedgerEntry> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.split('\n')
        .filter(|s| !s.is_empty())
        .map(|line| serde_json::from_str(line).expect("valid JSONL"))
        .collect()
}

// ---- TESTS ------------------------------------------------------------------

/// Cache hit/miss path: first dispatch hits the network (cache miss),
/// subsequent dispatch with identical (provider, request) returns from cache.
/// The ledger records `cache_hit=true` on the second.
#[tokio::test]
async fn cache_hit_after_first_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    let ledger_path = dir.path().join("ledger.jsonl");
    let cfg = cfg_two_provider_quality(cache_dir.to_str().unwrap(), ledger_path.to_str().unwrap());

    let alpha = SyntheticProvider::new("alpha", vec![Scripted::Ok("hello".into())]);
    let beta = SyntheticProvider::new("beta", vec![Scripted::Ok("never reached".into())]);

    let router = RouterBuilder::new()
        .register_provider("alpha", alpha.clone())
        .register_provider("beta", beta.clone())
        .build(&cfg)
        .await
        .unwrap();

    let r1 = router.dispatch(Task::Translate, req("ping")).await.unwrap();
    assert_eq!(r1.response.text, "hello");
    assert!(!r1.cache_hit);
    assert_eq!(r1.provider, "alpha");

    // Identical request → cache hit, alpha must not be called again.
    let r2 = router.dispatch(Task::Translate, req("ping")).await.unwrap();
    assert!(r2.cache_hit, "second dispatch should be a cache hit");
    assert_eq!(alpha.calls.load(Ordering::SeqCst), 1);

    // Ledger reflects miss-then-hit.
    let entries = read_ledger(&ledger_path);
    assert_eq!(entries.len(), 2);
    assert!(!entries[0].cache_hit);
    assert!(entries[1].cache_hit);
    assert!(matches!(entries[0].outcome, Outcome::Ok));
}

/// Cache key determinism across two independent `Router` instances built
/// from the same config: identical (provider, request) hashes to the same
/// key bit-for-bit, even when the cache is freshly created on a different
/// path.
#[tokio::test]
async fn cache_key_is_deterministic_across_runs() {
    let request = req("deterministic");
    let key_a = CacheKey::compute("alpha", &request);
    let key_b = CacheKey::compute("alpha", &request);
    assert_eq!(key_a, key_b);
    assert_eq!(key_a.hex().len(), 64);

    // Plant a dummy completion under that key; a second router pointed at
    // the same cache must read it back.
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path().to_path_buf()).await.unwrap();
    let resp = CompletionResponse {
        text: "preplanted".into(),
        model: "synth-1".into(),
        usage: TokenUsage::default(),
    };
    cache.put(&key_a, &request, &resp).await.unwrap();

    // Independent `Cache` handle pointing at the same dir.
    let cache2 = Cache::new(dir.path().to_path_buf()).await.unwrap();
    let read_back = cache2.get(&key_b).await.unwrap().unwrap();
    assert_eq!(read_back, resp);
}

/// Provider failure isolation: alpha returns 5xx, router falls through to
/// beta on the same dispatch. Ledger records both attempts.
#[tokio::test]
async fn provider_failure_falls_through_to_next_preferred() {
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    let ledger_path = dir.path().join("ledger.jsonl");
    let cfg = cfg_two_provider_quality(cache_dir.to_str().unwrap(), ledger_path.to_str().unwrap());

    // Alpha: 5xx five times (exhausts retry budget) → router falls to beta.
    let alpha_script = vec![
        Scripted::Err(LlmError::Server {
            status: 503,
            body: String::new(),
        });
        5
    ];
    let alpha = SyntheticProvider::new("alpha", alpha_script);
    let beta = SyntheticProvider::new("beta", vec![Scripted::Ok("from-beta".into())]);

    let router = RouterBuilder::new()
        .register_provider("alpha", alpha.clone())
        .register_provider("beta", beta.clone())
        .retry_policy(cobrust_llm_router::RetryPolicy {
            // Tighter retry to keep test fast while still exercising fall-through.
            max_attempts: 2,
            base_delay_ms: 1,
            factor: 1.0,
            max_total_ms: 100,
        })
        .build(&cfg)
        .await
        .unwrap();

    let r = router
        .dispatch(Task::Translate, req("isolate"))
        .await
        .unwrap();
    assert_eq!(r.response.text, "from-beta");
    assert_eq!(r.provider, "beta", "must have fallen through to beta");
    assert!(beta.calls.load(Ordering::SeqCst) >= 1);
    assert!(alpha.calls.load(Ordering::SeqCst) >= 1);

    let entries = read_ledger(&ledger_path);
    assert!(
        entries.len() >= 2,
        "ledger should record at least alpha-error + beta-ok"
    );
    let last = entries.last().unwrap();
    assert!(matches!(last.outcome, Outcome::Ok));
    assert_eq!(last.provider, "beta");
}

/// All providers fail → router returns `RouterError::AllFailed` with one
/// entry per provider in order.
#[tokio::test]
async fn all_providers_failing_returns_all_failed() {
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    let ledger_path = dir.path().join("ledger.jsonl");
    let cfg = cfg_two_provider_quality(cache_dir.to_str().unwrap(), ledger_path.to_str().unwrap());
    let alpha = SyntheticProvider::new("alpha", vec![Scripted::Err(LlmError::Auth); 1]);
    let beta = SyntheticProvider::new("beta", vec![Scripted::Err(LlmError::Auth); 1]);
    let router = RouterBuilder::new()
        .register_provider("alpha", alpha)
        .register_provider("beta", beta)
        .retry_policy(cobrust_llm_router::RetryPolicy {
            max_attempts: 1,
            base_delay_ms: 1,
            factor: 1.0,
            max_total_ms: 50,
        })
        .build(&cfg)
        .await
        .unwrap();
    let err = router
        .dispatch(Task::Translate, req("nope"))
        .await
        .unwrap_err();
    match err {
        RouterError::AllFailed(list) => {
            assert_eq!(list.len(), 2);
            assert_eq!(list[0].0, "alpha");
            assert_eq!(list[1].0, "beta");
        }
        e => panic!("expected AllFailed, got {e:?}"),
    }
}

/// Consensus tie-breaking is deterministic: two runs over the same shard
/// outputs pick the same winner.
#[tokio::test]
async fn consensus_tie_breaking_is_deterministic() {
    async fn run_once(seed_text: &[&str]) -> RouterResponse {
        let dir = tempfile::tempdir().unwrap();
        let cfg = cfg_consensus(
            dir.path().join("cache").to_str().unwrap(),
            dir.path().join("ledger.jsonl").to_str().unwrap(),
        );
        let alpha = SyntheticProvider::new("alpha", vec![Scripted::Ok(seed_text[0].into())]);
        let beta = SyntheticProvider::new("beta", vec![Scripted::Ok(seed_text[1].into())]);
        let gamma = SyntheticProvider::new("gamma", vec![Scripted::Ok(seed_text[2].into())]);
        let router = RouterBuilder::new()
            .register_provider("alpha", alpha)
            .register_provider("beta", beta)
            .register_provider("gamma", gamma)
            .build(&cfg)
            .await
            .unwrap();
        router.dispatch(Task::Translate, req("c")).await.unwrap()
    }

    // Majority case: alpha and beta agree, gamma differs → majority wins.
    let majority = ["yes", "yes", "no"];
    let r1 = run_once(&majority).await;
    let r2 = run_once(&majority).await;
    assert_eq!(r1.response.text, "yes");
    assert_eq!(r1.response.text, r2.response.text);
    assert_eq!(r1.provider, r2.provider);

    // Three-way tie → deterministic by hash + preferred-index tiebreak.
    let tie = ["a", "b", "c"];
    let r3 = run_once(&tie).await;
    let r4 = run_once(&tie).await;
    assert_eq!(r3, r4, "tie-break must be deterministic across runs");
}

/// Consensus quorum-lost: too many shards fail → router surfaces
/// `ConsensusQuorumLost`.
#[tokio::test]
async fn consensus_quorum_lost_when_too_many_fail() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = cfg_consensus(
        dir.path().join("cache").to_str().unwrap(),
        dir.path().join("ledger.jsonl").to_str().unwrap(),
    );
    let alpha = SyntheticProvider::new("alpha", vec![Scripted::Err(LlmError::Auth)]);
    let beta = SyntheticProvider::new("beta", vec![Scripted::Err(LlmError::Auth)]);
    let gamma = SyntheticProvider::new("gamma", vec![Scripted::Ok("solo".into())]);
    let router = RouterBuilder::new()
        .register_provider("alpha", alpha)
        .register_provider("beta", beta)
        .register_provider("gamma", gamma)
        .retry_policy(cobrust_llm_router::RetryPolicy {
            max_attempts: 1,
            base_delay_ms: 1,
            factor: 1.0,
            max_total_ms: 50,
        })
        .build(&cfg)
        .await
        .unwrap();
    let err = router
        .dispatch(Task::Translate, req("quorum"))
        .await
        .unwrap_err();
    assert!(
        matches!(err, RouterError::ConsensusQuorumLost { need: 2, got: 1 }),
        "expected quorum-lost, got {err:?}"
    );
}

/// Streaming round-trips: synthetic provider's `complete_stream` yields
/// `Delta`+ then exactly one `Done`. Sum of deltas equals the underlying
/// completion text.
#[tokio::test]
async fn streaming_round_trips_through_provider_trait() {
    let provider = SyntheticProvider::new("synth", vec![Scripted::Ok("hello-world".into())]);
    let stream = provider.complete_stream(req("stream"));
    let chunks: Vec<Result<Chunk, LlmError>> = stream.collect().await;
    assert!(!chunks.is_empty(), "stream should emit at least Done");

    let mut text = String::new();
    let mut done_count = 0;
    let mut last_usage = TokenUsage::default();
    for ch in chunks {
        match ch.unwrap() {
            Chunk::Delta(s) => text.push_str(&s),
            Chunk::Done(u) => {
                done_count += 1;
                last_usage = u;
            }
        }
    }
    assert_eq!(text, "hello-world");
    assert_eq!(done_count, 1, "exactly one Done frame must be emitted");
    assert_eq!(last_usage.prompt_tokens, 1);
    assert_eq!(last_usage.completion_tokens, 2);
}

/// Ledger append-only invariant under the public `Router` API: dropping and
/// re-opening the router preserves prior entries.
#[tokio::test]
async fn ledger_is_append_only_across_router_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    let ledger_path = dir.path().join("ledger.jsonl");
    let cfg = cfg_two_provider_quality(cache_dir.to_str().unwrap(), ledger_path.to_str().unwrap());

    {
        let alpha = SyntheticProvider::new("alpha", vec![Scripted::Ok("first".into())]);
        let beta = SyntheticProvider::new("beta", vec![Scripted::Ok("never".into())]);
        let r1 = RouterBuilder::new()
            .register_provider("alpha", alpha)
            .register_provider("beta", beta)
            .build(&cfg)
            .await
            .unwrap();
        let _ = r1.dispatch(Task::Translate, req("first")).await.unwrap();
    }

    let entries_after_first = read_ledger(&ledger_path);
    assert_eq!(entries_after_first.len(), 1);

    {
        let alpha = SyntheticProvider::new("alpha", vec![Scripted::Ok("second".into())]);
        let beta = SyntheticProvider::new("beta", vec![Scripted::Ok("never".into())]);
        let r2 = RouterBuilder::new()
            .register_provider("alpha", alpha)
            .register_provider("beta", beta)
            .build(&cfg)
            .await
            .unwrap();
        let _ = r2.dispatch(Task::Translate, req("second")).await.unwrap();
    }

    let entries_after_second = read_ledger(&ledger_path);
    assert_eq!(
        entries_after_second.len(),
        2,
        "second router lifecycle must NOT truncate the ledger"
    );
    assert_eq!(entries_after_second[0], entries_after_first[0]);
}
