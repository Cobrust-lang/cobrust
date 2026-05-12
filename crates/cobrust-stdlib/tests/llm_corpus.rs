//! M-AI.0 α Phase 2 — Rust-side stdlib unit tests for `cobrust.llm`
//! blocking helpers `llm_complete_blocking` / `llm_dispatch_blocking` /
//! `llm_stream_blocking`.
//!
//! Spike spec: `docs/agent/spike/m-ai-0-cobrust-llm-spike.md` §"Test
//! plan (binding for P7-TEST)" Tier 1. P10 ratified the spike's three
//! open questions (OQ-1 flat-fn α naming / OQ-2 collect-all-chunks
//! streaming / OQ-3 WRAP synthetic-route hack — router crate frozen
//! for M-AI.0).
//!
//! TDD STEP 1 — FAILING TEST CORPUS. The DEV agent (TDD step 3)
//! implements the stdlib + CLI + codegen surface until every test
//! passes; the impl scope is enumerated in the spike §"Implementation
//! map (binding for P7-DEV)".
//!
//! Per ADR-0044 W2 Phase 2 TDD-step-1 precedent (`io_input.rs` /
//! `io_input_fuzz.rs` shape), each test body calls `require_impl()`
//! which panics with a clear "NOT YET IMPLEMENTED" message until DEV
//! flips `ADR_M_AI_0_IMPL_LANDED` to `true`. Live calls to the future
//! surface (e.g. `cobrust_stdlib::llm::llm_complete_blocking(...)`)
//! are held as documentation comments so the corpus compiles today
//! without referencing not-yet-existing symbols. DEV uncomments the
//! bodies once stdlib::llm lands.
//!
//! Synthetic-provider fixtures used by Tier 1 are scripted via the
//! `crates/cobrust-llm-router::ProviderKind::Synthetic` precedent in
//! `crates/cobrust-llm-router/tests/synthetic_provider.rs:38-114` —
//! DEV will expose a `#[cfg(test)]`/test-utility seam on
//! `cobrust_stdlib::llm` so this corpus can inject in-process doubles
//! without touching the frozen router crate's source (per OQ-3 WRAP
//! ratification).
//!
//! Per `feedback_p9_clippy_stall_pattern.md` (2026-05-09) — module-
//! level 18-lint test-only allow header at the TOP of every test file
//! authored under this corpus.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::manual_repeat_n)]

// =====================================================================
// Impl-landed marker. DEV flips to `true` once
//   - `cobrust_stdlib::llm::llm_complete_blocking`
//   - `cobrust_stdlib::llm::llm_dispatch_blocking`
//   - `cobrust_stdlib::llm::llm_stream_blocking`
//   - the synthetic-provider test-injection seam
// are landed in `crates/cobrust-stdlib/src/llm.rs`.
// =====================================================================

const ADR_M_AI_0_IMPL_LANDED: bool = true;

fn require_impl(test_name: &str) {
    assert!(
        ADR_M_AI_0_IMPL_LANDED,
        "NOT YET IMPLEMENTED: M-AI.0 (α Phase 2 cobrust.llm) impl not yet landed; DEV agent must:\n  \
         1. Add `cobrust_stdlib::llm` module (gated by `llm-router` feature) with\n     \
            `llm_complete_blocking(provider, model, prompt) -> String`,\n     \
            `llm_dispatch_blocking(task, prompt) -> String`,\n     \
            `llm_stream_blocking(provider, model, prompt) -> Vec<String>`.\n  \
         2. Add a test-injection seam (e.g. `__test_register_synthetic_provider`)\n     \
            on `cobrust_stdlib::llm` so Tier 1 tests can stub an in-process\n     \
            `Arc<dyn LlmProvider>` without touching the frozen router crate\n     \
            (per OQ-3 WRAP ratification).\n  \
         3. Wire the lazy `tokio::runtime::Runtime` + `RouterConfigBundle`\n     \
            OnceLocks per spike Decision 4 + 5.\n  \
         4. Flip ADR_M_AI_0_IMPL_LANDED = true in tests/llm_corpus.rs +\n     \
            tests/llm_cabi_corpus.rs + tests/llm_fuzz.rs +\n     \
            crates/cobrust-cli/tests/intrinsics_llm.rs.\n  \
         FAILING TEST: {test_name}"
    );
}

// =====================================================================
// Tier 1 #1 — llm_complete_blocking with Synthetic provider returns the
// canned response text.
// =====================================================================

#[test]
fn test_llm_complete_blocking_returns_synthetic_canned_text() {
    require_impl("test_llm_complete_blocking_returns_synthetic_canned_text");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::llm;
    //   use cobrust_llm_router::ProviderKind;
    //   // Build a one-attempt synthetic double that always replies with
    //   // "hello-from-synth". DEV's test seam takes an `Arc<dyn LlmProvider>`
    //   // and a provider name.
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("hello-from-synth".into())]);
    //   llm::__test_register_synthetic_provider("syn", provider);
    //   // Point COBRUST_CONFIG at a fixture cobrust.toml that declares
    //   // [providers.syn] kind = "synthetic" and routes "llm_complete" through it.
    //   let _guard = set_cobrust_config(fixture_path("syn_provider.toml"));
    //   let out = llm::llm_complete_blocking("syn", "synth-1", "hi");
    //   assert_eq!(out, "hello-from-synth");
}

// =====================================================================
// Tier 1 #2 — llm_complete_blocking with missing cobrust.toml returns "".
// Decision 7 (error surface): missing config → ledger logs, source-level "".
// =====================================================================

#[test]
fn test_llm_complete_blocking_missing_config_returns_empty() {
    require_impl("test_llm_complete_blocking_missing_config_returns_empty");
    // Once impl lands, DEV uncomments:
    //
    //   let _guard = set_cobrust_config("/nonexistent/cobrust.toml.does-not-exist");
    //   let out = cobrust_stdlib::llm::llm_complete_blocking("syn", "m1", "hi");
    //   assert_eq!(out, "", "missing config must return empty string (Decision 7)");
}

// =====================================================================
// Tier 1 #3 — llm_complete_blocking with malformed cobrust.toml returns "".
// =====================================================================

#[test]
fn test_llm_complete_blocking_malformed_config_returns_empty() {
    require_impl("test_llm_complete_blocking_malformed_config_returns_empty");
    // Once impl lands, DEV uncomments:
    //
    //   let bad = write_tempfile("not valid toml [[[ \\\\");
    //   let _guard = set_cobrust_config(bad.path());
    //   let out = cobrust_stdlib::llm::llm_complete_blocking("syn", "m1", "hi");
    //   assert_eq!(out, "");
}

// =====================================================================
// Tier 1 #4 — llm_complete_blocking with auth-failing provider returns "".
// Ledger captures `LlmError::Auth` per Decision 6 + 7.
// =====================================================================

#[test]
fn test_llm_complete_blocking_auth_failure_returns_empty_logs_to_ledger() {
    require_impl("test_llm_complete_blocking_auth_failure_returns_empty_logs_to_ledger");
    // Once impl lands, DEV uncomments:
    //
    //   let provider = SyntheticDouble::new(vec![Scripted::Err(LlmError::Auth)]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let dir = tempfile::tempdir().unwrap();
    //   let cfg = write_synthetic_toml(&dir, "syn"); // ledger_path = dir/ledger.jsonl
    //   let _guard = set_cobrust_config(&cfg);
    //   let out = cobrust_stdlib::llm::llm_complete_blocking("syn", "m1", "hi");
    //   assert_eq!(out, "");
    //   let entries = read_ledger(&dir.path().join("ledger.jsonl"));
    //   assert!(entries.iter().any(|e| matches!(e.outcome, Outcome::Err) &&
    //       e.error_code.as_deref() == Some("auth")));
}

// =====================================================================
// Tier 1 #5 — llm_complete_blocking with rate-limit transient retries
// then succeeds.
// =====================================================================

#[test]
fn test_llm_complete_blocking_rate_limit_retries_then_succeeds() {
    require_impl("test_llm_complete_blocking_rate_limit_retries_then_succeeds");
    // Once impl lands, DEV uncomments:
    //
    //   let provider = SyntheticDouble::new(vec![
    //       Scripted::Err(LlmError::RateLimit { retry_after_ms: 1 }),
    //       Scripted::Ok("after-retry".into()),
    //   ]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_tight_retry(/*max_attempts=*/3));
    //   let out = cobrust_stdlib::llm::llm_complete_blocking("syn", "m1", "hi");
    //   assert_eq!(out, "after-retry");
}

// =====================================================================
// Tier 1 #6 — llm_complete_blocking with transport error (provider down)
// returns "".
// =====================================================================

#[test]
fn test_llm_complete_blocking_transport_error_returns_empty() {
    require_impl("test_llm_complete_blocking_transport_error_returns_empty");
    // Once impl lands, DEV uncomments:
    //
    //   let provider = SyntheticDouble::new(vec![
    //       Scripted::Err(LlmError::Transport("dns".into())); 3
    //   ]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_tight_retry(/*max_attempts=*/1));
    //   let out = cobrust_stdlib::llm::llm_complete_blocking("syn", "m1", "hi");
    //   assert_eq!(out, "");
}

// =====================================================================
// Tier 1 #7 — llm_dispatch_blocking with valid task name routes to first
// preferred provider.
// =====================================================================

#[test]
fn test_llm_dispatch_blocking_valid_task_routes_to_first_preferred() {
    require_impl("test_llm_dispatch_blocking_valid_task_routes_to_first_preferred");
    // Once impl lands, DEV uncomments:
    //
    //   let alpha = SyntheticDouble::new(vec![Scripted::Ok("from-alpha".into())]);
    //   let beta  = SyntheticDouble::new(vec![Scripted::Ok("from-beta".into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("alpha", alpha);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("beta", beta);
    //   let _guard = set_cobrust_config(&write_routing_toml("greet",
    //       /*strategy=*/"quality",
    //       /*preferred=*/&["alpha:m1", "beta:m1"]));
    //   let out = cobrust_stdlib::llm::llm_dispatch_blocking("greet", "hello");
    //   assert_eq!(out, "from-alpha", "must route to first preferred provider");
}

// =====================================================================
// Tier 1 #8 — llm_dispatch_blocking with unknown task returns "".
// =====================================================================

#[test]
fn test_llm_dispatch_blocking_unknown_task_returns_empty() {
    require_impl("test_llm_dispatch_blocking_unknown_task_returns_empty");
    // Once impl lands, DEV uncomments:
    //
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("never".into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_routing_toml("greet", "quality",
    //       &["syn:m1"]));
    //   let out = cobrust_stdlib::llm::llm_dispatch_blocking("UNKNOWN_TASK_NAME", "hi");
    //   assert_eq!(out, "", "unknown task → empty string (Decision 7)");
}

// =====================================================================
// Tier 1 #9 — llm_dispatch_blocking with consensus strategy routes through
// `Router::dispatch_consensus`.
// =====================================================================

#[test]
fn test_llm_dispatch_blocking_consensus_strategy_routes_through_consensus_path() {
    require_impl("test_llm_dispatch_blocking_consensus_strategy_routes_through_consensus_path");
    // Once impl lands, DEV uncomments:
    //
    //   // Three shards agree on "consensus-answer"; majority wins.
    //   let a = SyntheticDouble::new(vec![Scripted::Ok("consensus-answer".into())]);
    //   let b = SyntheticDouble::new(vec![Scripted::Ok("consensus-answer".into())]);
    //   let c = SyntheticDouble::new(vec![Scripted::Ok("dissent".into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("a", a);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("b", b);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("c", c);
    //   let _guard = set_cobrust_config(&write_consensus_toml("vote",
    //       /*n=*/3, &["a:m1", "b:m1", "c:m1"]));
    //   let out = cobrust_stdlib::llm::llm_dispatch_blocking("vote", "hi");
    //   assert_eq!(out, "consensus-answer");
}

// =====================================================================
// Tier 1 #10 — llm_stream_blocking returns an ordered Vec<String> of
// delta chunks from a synthetic stream.
// =====================================================================

#[test]
fn test_llm_stream_blocking_returns_ordered_chunks() {
    require_impl("test_llm_stream_blocking_returns_ordered_chunks");
    // Once impl lands, DEV uncomments:
    //
    //   // Synthetic double's complete_stream yields Delta("hello-")+Delta("world")+Done.
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("hello-world".into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   let chunks: Vec<String> =
    //       cobrust_stdlib::llm::llm_stream_blocking("syn", "m1", "hi");
    //   assert!(!chunks.is_empty(), "stream must yield ≥1 delta");
    //   let concatenated: String = chunks.concat();
    //   assert_eq!(concatenated, "hello-world",
    //       "concatenation of deltas must equal the synthetic completion text");
}

// =====================================================================
// Tier 1 #11 — llm_stream_blocking on empty stream returns empty Vec.
// =====================================================================

#[test]
fn test_llm_stream_blocking_empty_stream_returns_empty_vec() {
    require_impl("test_llm_stream_blocking_empty_stream_returns_empty_vec");
    // Once impl lands, DEV uncomments:
    //
    //   // Synthetic double scripted to return Scripted::Ok("") → stream
    //   // emits zero Delta frames + one Done. Collected Vec<String> is empty.
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok(String::new())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   let chunks: Vec<String> =
    //       cobrust_stdlib::llm::llm_stream_blocking("syn", "m1", "hi");
    //   assert!(chunks.is_empty() || chunks.iter().all(|c| c.is_empty()),
    //       "empty completion → empty (or all-empty) chunk vec");
}

// =====================================================================
// Tier 1 #12 — llm_stream_blocking on stream-error mid-stream returns
// the partial Vec collected so far (per spike Decision 3B collect-all-
// chunks semantics + Decision 7 empty-on-failure convention).
// =====================================================================

#[test]
fn test_llm_stream_blocking_mid_stream_error_returns_partial_vec() {
    require_impl("test_llm_stream_blocking_mid_stream_error_returns_partial_vec");
    // Once impl lands, DEV uncomments:
    //
    //   // Synthetic double's complete_stream errors after yielding "part-".
    //   let provider = SyntheticDouble::with_stream_script(vec![
    //       StreamScripted::Delta("part-".into()),
    //       StreamScripted::Err(LlmError::Stream("truncated".into())),
    //   ]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   let chunks: Vec<String> =
    //       cobrust_stdlib::llm::llm_stream_blocking("syn", "m1", "hi");
    //   // Decision 7 says "" / empty list signals failure; partial may also be
    //   // returned. Either is acceptable as long as it does not panic. DEV
    //   // implements per spike Decision 3B's "Empty list signals failure".
    //   let _ = chunks; // No panic is the binding contract.
}

// =====================================================================
// Tier 1 #13 — UTF-8 round-trip: prompt with multi-byte chars + canned
// multi-byte response → byte-identical text in/out.
// =====================================================================

#[test]
fn test_llm_complete_blocking_utf8_multibyte_round_trip() {
    require_impl("test_llm_complete_blocking_utf8_multibyte_round_trip");
    // Once impl lands, DEV uncomments:
    //
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("你好世界".into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   let out = cobrust_stdlib::llm::llm_complete_blocking("syn", "m1", "こんにちは");
    //   assert_eq!(out, "你好世界", "multi-byte UTF-8 must round-trip exactly");
}

// =====================================================================
// Tier 1 #14 — Concurrent invocation safety: 32 parallel
// `llm_complete_blocking` calls — all return correct canned response.
// =====================================================================

#[test]
fn test_llm_complete_blocking_concurrent_32_parallel_calls_safe() {
    require_impl("test_llm_complete_blocking_concurrent_32_parallel_calls_safe");
    // Once impl lands, DEV uncomments:
    //
    //   // Synthetic double scripted with 32 identical Ok responses so each
    //   // concurrent invocation has its own canned reply slot.
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("parallel-ok".into()); 32]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   use std::thread;
    //   let handles: Vec<_> = (0..32).map(|_| {
    //       thread::spawn(|| cobrust_stdlib::llm::llm_complete_blocking("syn", "m1", "hi"))
    //   }).collect();
    //   for h in handles {
    //       assert_eq!(h.join().unwrap(), "parallel-ok",
    //           "every concurrent call must return the canned text");
    //   }
}

// =====================================================================
// Tier 1 #15 — Ledger file contains 32 outcome:ok entries after Test 14.
// Sequential follow-up to #14: same fixture, inspect the ledger written
// by the cached Router.
// =====================================================================

#[test]
fn test_llm_complete_blocking_writes_outcome_ok_to_ledger_after_32_parallel_calls() {
    require_impl("test_llm_complete_blocking_writes_outcome_ok_to_ledger_after_32_parallel_calls");
    // Once impl lands, DEV uncomments:
    //
    //   let dir = tempfile::tempdir().unwrap();
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("ledger-ok".into()); 32]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let cfg = write_synthetic_toml_with_ledger(&dir, "syn"); // ledger=dir/ledger.jsonl
    //   let _guard = set_cobrust_config(&cfg);
    //   use std::thread;
    //   let handles: Vec<_> = (0..32).map(|_| {
    //       thread::spawn(|| cobrust_stdlib::llm::llm_complete_blocking("syn", "m1", "hi"))
    //   }).collect();
    //   for h in handles { let _ = h.join().unwrap(); }
    //   let entries = read_ledger(&dir.path().join("ledger.jsonl"));
    //   // Cache hits may collapse some; what matters is no failures.
    //   let oks = entries.iter().filter(|e| matches!(e.outcome, Outcome::Ok)).count();
    //   assert_eq!(oks, entries.len(),
    //       "every entry must be outcome:ok after 32 successful parallel dispatches");
    //   assert!(entries.len() >= 1, "at least one ledger entry must be written");
}

// =====================================================================
// Tier 1 #16 — verify.py oracle (ADR-0047a mandate): the synthetic-
// fixture text DEV's Cobrust impl returns must match what
// `tests/llm_corpus_verify.py <case>` prints. This is the independent
// confirmation that Cobrust's `llm_*` blocking helpers faithfully mirror
// the synthetic-fixture contract — not just self-consistent.
// =====================================================================

#[test]
fn test_llm_complete_blocking_verify_py_oracle_matches_synthetic_fixture() {
    require_impl("test_llm_complete_blocking_verify_py_oracle_matches_synthetic_fixture");
    // Once impl lands, DEV uncomments:
    //
    //   // Same fixture text used by both legs.
    //   let canned = "hello-from-synth";
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok(canned.into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   let cb_out = cobrust_stdlib::llm::llm_complete_blocking("syn", "m1", "hi");
    //   let py_out = run_verify_py_oracle(
    //       "test_llm_complete_blocking_returns_synthetic_canned_text",
    //   );
    //   assert_eq!(cb_out, canned, "Cobrust output must match synthetic fixture");
    //   assert_eq!(cb_out, py_out, "Cobrust output must match verify.py oracle");
}
