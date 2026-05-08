//! Real-LLM smoke against the user-provided codex endpoint
//! (`http://104.244.92.250:8317/v1`, model `gpt-5.5`, OpenAI-compatible wire
//! format). End-to-end validation of the M3 router as a complete subsystem
//! — `OpenAiProvider` + `Router` + `Cache` + `Ledger` running over a real
//! HTTP roundtrip rather than `wiremock`.
//!
//! Validates the contract pinned by `adr:0004`:
//!
//! 1. **Live dispatch**  — POST to `/chat/completions` succeeds, response
//!    text non-empty, ledger records `cache_hit=false, outcome="ok"`.
//! 2. **Cache replay**  — repeating the identical request bypasses the
//!    network entirely, yields a bit-identical `CompletionResponse`, and
//!    appends a second ledger entry with `cache_hit=true`.
//! 3. **Transport-failure isolation** — pointing the adapter at a closed
//!    port surfaces `LlmError::Transport` through `RouterError::AllFailed`
//!    without panicking and without infinite retry.
//!
//! Gated on `USER_CODEX_API_KEY`. When the env var is absent the test
//! prints a skip message and returns cleanly — the default `cargo test
//! --workspace` invocation never makes a network call.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cobrust_llm_router::{
    CompletionRequest, LedgerEntry, LlmError, Message, OpenAiProvider, Outcome, RetryPolicy, Role,
    RouterBuilder, RouterConfig, RouterError, SamplingParams, Task,
};

const ENV_KEY: &str = "USER_CODEX_API_KEY";
const BASE_URL: &str = "http://104.244.92.250:8317/v1";
const PROVIDER_KEY: &str = "user_codex";
const MODEL: &str = "gpt-5.5";
const SMOKE_PROMPT: &str = "Reply with the single word: ok";
const SMOKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Read the API key from the env. `None` → caller skips cleanly.
fn lookup_api_key() -> Option<String> {
    std::env::var(ENV_KEY).ok().filter(|s| !s.is_empty())
}

/// `cobrust.toml` snippet that wires `user_codex:gpt-5.5` into the
/// `translate` task. Strategy = `quality` so the router walks the
/// preferred list in order; the first (and only) entry is the user
/// codex provider.
fn cfg_for_user_codex(cache: &Path, ledger: &Path) -> RouterConfig {
    let toml = format!(
        r#"
[router]
default_strategy = "quality"
cache_dir = "{cache}"
ledger_path = "{ledger}"

[providers.{PROVIDER_KEY}]
kind = "openai"
base_url = "{BASE_URL}"
api_key_env = "{ENV_KEY}"
models = ["{MODEL}"]

[routing.translate]
strategy = "quality"
preferred = ["{PROVIDER_KEY}:{MODEL}"]

[routing.real_llm_smoke]
strategy = "quality"
preferred = ["{PROVIDER_KEY}:{MODEL}"]
"#,
        cache = cache.display(),
        ledger = ledger.display(),
    );
    RouterConfig::from_toml_str(&toml).expect("smoke config must parse")
}

fn smoke_request() -> CompletionRequest {
    CompletionRequest {
        model: MODEL.into(),
        messages: vec![Message {
            role: Role::User,
            content: SMOKE_PROMPT.into(),
        }],
        params: SamplingParams {
            // Keep token usage minimal — the brief calls for 5-10 calls total.
            max_tokens: Some(16),
            temperature: Some(0.0),
            top_p: None,
            stop: vec![],
        },
    }
}

fn read_ledger(path: &Path) -> Vec<LedgerEntry> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.split('\n')
        .filter(|s| !s.is_empty())
        .map(|line| serde_json::from_str(line).expect("valid JSONL"))
        .collect()
}

// ---- Round-trip + cache replay ---------------------------------------------

/// Round-trip + cache replay in one `#[tokio::test]`. Dispatching twice
/// against the same temp ledger keeps the assertions on ledger length
/// hermetic — the test owns its `tempdir`, so concurrent tests cannot
/// interleave entries.
#[tokio::test]
async fn real_llm_round_trip_then_cache_replay_is_bit_identical() {
    let Some(api_key) = lookup_api_key() else {
        eprintln!("real-LLM smoke: {ENV_KEY} unset — skipping");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    let ledger_path = dir.path().join("ledger.jsonl");
    let cfg = cfg_for_user_codex(&cache_dir, &ledger_path);

    let provider = Arc::new(
        OpenAiProvider::new(PROVIDER_KEY, BASE_URL, api_key).expect("HTTP client must build"),
    );

    let router = RouterBuilder::new()
        .register_provider(PROVIDER_KEY, provider)
        // Conservative retry budget — one transient is fine, but the test
        // asserts that a single user-visible call occurred for the first
        // dispatch. The default 5-attempt policy would still give a single
        // ledger ok-entry on success; tightening it bounds wall-clock on
        // server flakes.
        .retry_policy(RetryPolicy {
            max_attempts: 2,
            base_delay_ms: 500,
            factor: 2.0,
            max_total_ms: 20_000,
        })
        .build(&cfg)
        .await
        .expect("router must build");

    // -------- Phase 1: live round-trip ------------------------------------
    let req = smoke_request();
    let live = tokio::time::timeout(SMOKE_TIMEOUT, router.dispatch(Task::Translate, req.clone()))
        .await
        .expect("dispatch must complete within 30s")
        .expect("live dispatch must succeed against user codex");

    assert_eq!(live.provider, PROVIDER_KEY);
    assert!(!live.cache_hit, "first dispatch must NOT be a cache hit");
    assert!(
        !live.response.text.is_empty(),
        "real-LLM response text must be non-empty (got empty)"
    );
    eprintln!(
        "real-LLM smoke: live response = {:?} (model={}, prompt_tokens={}, completion_tokens={})",
        live.response.text,
        live.response.model,
        live.response.usage.prompt_tokens,
        live.response.usage.completion_tokens,
    );

    // Single ledger entry with cache_hit=false, outcome=ok.
    let entries_after_live = read_ledger(&ledger_path);
    assert_eq!(
        entries_after_live.len(),
        1,
        "exactly one ledger entry expected after live dispatch (got {})",
        entries_after_live.len()
    );
    let live_entry = &entries_after_live[0];
    assert_eq!(live_entry.provider, PROVIDER_KEY);
    assert_eq!(live_entry.model, MODEL);
    assert_eq!(live_entry.task, "translate");
    assert!(
        !live_entry.cache_hit,
        "live entry cache_hit must be false (got true)"
    );
    assert!(
        matches!(live_entry.outcome, Outcome::Ok),
        "live entry outcome must be Ok (got {:?})",
        live_entry.outcome
    );
    assert!(
        live_entry.cache_key.starts_with("blake3:"),
        "cache_key must be wire-format blake3 (got {})",
        live_entry.cache_key
    );

    // -------- Phase 2: cache replay (no network) --------------------------
    let replay = tokio::time::timeout(
        Duration::from_secs(2), // cache hit must be near-instant
        router.dispatch(Task::Translate, req.clone()),
    )
    .await
    .expect("cache hit must complete in <2s")
    .expect("cache replay must succeed");

    assert!(replay.cache_hit, "second dispatch must be a cache hit");
    assert_eq!(
        replay.response, live.response,
        "cached response must be bit-identical to live response"
    );
    assert_eq!(replay.provider, PROVIDER_KEY);

    // Ledger now has exactly two entries; second has cache_hit=true.
    let entries_after_replay = read_ledger(&ledger_path);
    assert_eq!(
        entries_after_replay.len(),
        2,
        "replay must add exactly one ledger entry (got {})",
        entries_after_replay.len()
    );
    let replay_entry = &entries_after_replay[1];
    assert!(
        replay_entry.cache_hit,
        "second ledger entry cache_hit must be true"
    );
    assert!(matches!(replay_entry.outcome, Outcome::Ok));
    assert_eq!(
        replay_entry.cache_key, live_entry.cache_key,
        "cache key must be deterministic across calls (BLAKE3 canonical bytes)"
    );

    // Cache hits do not bill a provider call → latency_ms is recorded as 0
    // by the router (per `adr:0004` ledger schema). Don't assert exact
    // value — assert the documented invariant instead.
    assert_eq!(
        replay_entry.latency_ms, 0,
        "cache-hit ledger entry must report latency_ms=0"
    );

    // Evidence emission — full ledger contents for the finding doc.
    eprintln!(
        "real-LLM smoke: ledger.live = {}",
        serde_json::to_string(live_entry).unwrap_or_default(),
    );
    eprintln!(
        "real-LLM smoke: ledger.replay = {}",
        serde_json::to_string(replay_entry).unwrap_or_default(),
    );
}

// ---- Transport-failure isolation -------------------------------------------

/// Pointing the OpenAI adapter at a closed TCP port should surface
/// `LlmError::Transport` through `RouterError::AllFailed` without
/// panicking. With a tight retry budget the dispatch returns within
/// a few hundred milliseconds.
#[tokio::test]
async fn real_llm_transport_failure_is_isolated_not_panic() {
    let Some(_) = lookup_api_key() else {
        eprintln!("real-LLM smoke: {ENV_KEY} unset — skipping transport-failure case");
        return;
    };

    // Port 1 is reserved (TCPMUX) but never bound on a workstation;
    // connect attempts fail fast with ECONNREFUSED. We deliberately use
    // localhost so the failure mode is purely transport, no DNS surface.
    let dead_base = "http://127.0.0.1:1";

    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    let ledger_path = dir.path().join("ledger.jsonl");

    // Re-use the same config shape but swap the base_url. (We can't reuse
    // `cfg_for_user_codex` directly because it pins `BASE_URL`.)
    let toml = format!(
        r#"
[router]
default_strategy = "quality"
cache_dir = "{cache}"
ledger_path = "{ledger}"

[providers.{PROVIDER_KEY}]
kind = "openai"
base_url = "{dead_base}"
api_key_env = "{ENV_KEY}"
models = ["{MODEL}"]

[routing.translate]
strategy = "quality"
preferred = ["{PROVIDER_KEY}:{MODEL}"]
"#,
        cache = cache_dir.display(),
        ledger = ledger_path.display(),
    );
    let cfg = RouterConfig::from_toml_str(&toml).expect("smoke transport-failure config");

    let provider = Arc::new(
        // The literal API key value doesn't matter — we never reach a server.
        OpenAiProvider::new(PROVIDER_KEY, dead_base, "irrelevant-not-used")
            .expect("HTTP client must build"),
    );

    let router = RouterBuilder::new()
        .register_provider(PROVIDER_KEY, provider)
        .retry_policy(RetryPolicy {
            // One attempt — surface the failure quickly, no backoff cycle.
            max_attempts: 1,
            base_delay_ms: 1,
            factor: 1.0,
            max_total_ms: 1_500,
        })
        .build(&cfg)
        .await
        .expect("router must build");

    // Hard timeout caps total wall-clock; the transport itself bounces
    // sub-second, so a 5-second budget is safety margin for slow CI.
    let outcome = tokio::time::timeout(
        Duration::from_secs(15),
        router.dispatch(Task::Translate, smoke_request()),
    )
    .await
    .expect("transport failure must surface within 15s, no infinite retry");

    let err = outcome.expect_err("dispatch to a closed port must error");
    match err {
        RouterError::AllFailed(failures) => {
            assert_eq!(
                failures.len(),
                1,
                "AllFailed must contain exactly one provider failure"
            );
            let (provider_name, llm_err) = &failures[0];
            assert_eq!(provider_name, PROVIDER_KEY);
            assert!(
                matches!(llm_err, LlmError::Transport(_) | LlmError::Server { .. }),
                "expected Transport or Server (got {llm_err:?})"
            );
        }
        other => panic!("expected RouterError::AllFailed, got {other:?}"),
    }

    // The ledger must record the failure; no panic, just a structured
    // error_transient or error_permanent entry.
    let entries = read_ledger(&ledger_path);
    assert_eq!(
        entries.len(),
        1,
        "single failed attempt must produce exactly one ledger entry"
    );
    let entry = &entries[0];
    assert!(!entry.cache_hit);
    assert!(
        !matches!(entry.outcome, Outcome::Ok),
        "failure ledger entry must NOT be Outcome::Ok"
    );
    assert!(entry.error_code.is_some(), "error_code must be populated");
    let code = entry.error_code.as_deref().unwrap_or("");
    assert!(
        matches!(code, "transport" | "server"),
        "error_code must be transport|server (got {code})"
    );
}
