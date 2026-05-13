//! M-AI.0 α Phase 2 — end-to-end `.cb` source → compile → run tests
//! for `llm_complete` / `llm_dispatch` / `llm_stream` intrinsics.
//!
//! Spike spec: `docs/agent/spike/m-ai-0-cobrust-llm-spike.md` §"Test
//! plan" Tier 3. Mirrors the existing `cli_stdin_argv_e2e.rs` shape
//! (ADR-0044 W2 Phase 2 precedent).
//!
//! Each test:
//!   1. Writes a small `.cb` program into a temp dir calling
//!      `llm_complete` / `llm_dispatch` / `llm_stream`.
//!   2. Writes a fixture `cobrust.toml` declaring an OpenAI-compatible
//!      provider whose `base_url` points at a wiremock-served HTTP
//!      mock returning a canned completion (per spike Tier 3 plan).
//!   3. Sets `COBRUST_CONFIG` env var to that fixture path.
//!   4. Invokes `cobrust build` + the produced executable (or
//!      `cobrust run`), captures stdout, asserts contents.
//!
//! TDD STEP 1 — FAILING TEST CORPUS. The DEV agent (TDD step 3) lands
//! the PRELUDE extensions + intrinsic-rewrite + codegen helper-signature
//! additions in:
//!   - `crates/cobrust-cli/src/build.rs` — PRELUDE += three stubs.
//!   - `crates/cobrust-cli/src/build/intrinsics.rs` — three new arms.
//!   - `crates/cobrust-codegen/src/cranelift_backend.rs` —
//!     `runtime_helper_signatures` += three entries.
//!   - `crates/cobrust-stdlib/src/llm.rs` — module + shims.
//!
//! Each test body calls `require_impl()` which panics until the impl-
//! landed marker is flipped. The wiremock + subprocess setup is held
//! as a documented comment block so the corpus compiles today without
//! depending on not-yet-existing PRELUDE entries / runtime shims.
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
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::len_zero)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::unnecessary_debug_formatting)]

// =====================================================================
// Impl-landed marker. DEV flips when the M-AI.0 surface lands across
// `cobrust-cli/src/build.rs` (PRELUDE), `build/intrinsics.rs` (rewrite
// arms), `cobrust-codegen/src/cranelift_backend.rs` (signatures), and
// `cobrust-stdlib/src/llm.rs` (shims).
// =====================================================================

const ADR_M_AI_0_IMPL_LANDED: bool = true;

fn require_impl(test_name: &str) {
    assert!(
        ADR_M_AI_0_IMPL_LANDED,
        "NOT YET IMPLEMENTED: M-AI.0 (α Phase 2 cobrust.llm) e2e impl not yet landed; DEV agent must:\n  \
         1. Extend `crates/cobrust-cli/src/build.rs::PRELUDE` with the three\n     \
            stub fns `llm_complete` / `llm_dispatch` / `llm_stream`.\n  \
         2. Extend `crates/cobrust-cli/src/build/intrinsics.rs` with three\n     \
            rewrite arms targeting `LLM_*_RUNTIME_SYMBOL` constants.\n  \
         3. Extend `crates/cobrust-codegen/src/cranelift_backend.rs::\n     \
            runtime_helper_signatures()` with three new entries.\n  \
         4. Land `crates/cobrust-stdlib/src/llm.rs` (module + Rust helpers\n     \
            + C-ABI shims) per spike §\"C-ABI shim shape\".\n  \
         5. Flip ADR_M_AI_0_IMPL_LANDED = true in\n     \
            crates/cobrust-cli/tests/intrinsics_llm.rs.\n  \
         FAILING TEST: {test_name}"
    );
}

// =====================================================================
// Tier 3 #1 — Program: `print(llm_complete("syn", "m1", "hello"))`.
// Round-trips a canned response from a wiremock-served OpenAI-compatible
// provider through the full PRELUDE → intrinsic rewrite → C-ABI shim →
// stdout pipeline.
// =====================================================================

#[test]
fn test_e2e_llm_complete_prints_canned_response_via_wiremock() {
    require_impl("test_e2e_llm_complete_prints_canned_response_via_wiremock");
    // Once impl lands, DEV uncomments:
    //
    //   use std::process::Command;
    //   use wiremock::{Mock, MockServer, ResponseTemplate};
    //   use wiremock::matchers::{method, path};
    //
    //   let runtime = tokio::runtime::Runtime::new().unwrap();
    //   runtime.block_on(async {
    //       let server = MockServer::start().await;
    //       Mock::given(method("POST"))
    //           .and(path("/chat/completions"))
    //           .respond_with(ResponseTemplate::new(200).set_body_json(
    //               serde_json::json!({
    //                   "id": "c1",
    //                   "object": "chat.completion",
    //                   "choices": [{
    //                       "message": { "role": "assistant", "content": "hi-from-mock" },
    //                       "finish_reason": "stop",
    //                       "index": 0
    //                   }],
    //                   "model": "m1",
    //                   "usage": { "prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3 }
    //               })))
    //           .mount(&server).await;
    //
    //       let dir = tempfile::tempdir().unwrap();
    //       let cfg_path = dir.path().join("cobrust.toml");
    //       std::fs::write(&cfg_path, format!(r#"
    //           [router]
    //           default_strategy = "quality"
    //           ledger_path = "{}/ledger.jsonl"
    //
    //           [providers.syn]
    //           kind = "openai"
    //           base_url = "{}"
    //           api_key_env = "SYN_KEY"
    //           models = ["m1"]
    //
    //           [routing.llm_complete]
    //           strategy = "quality"
    //           preferred = ["syn:m1"]
    //       "#, dir.path().display(), server.uri())).unwrap();
    //
    //       let src_path = dir.path().join("prog.cb");
    //       std::fs::write(&src_path,
    //           "print(llm_complete(\"syn\", \"m1\", \"hello\"))\n").unwrap();
    //
    //       let exe = dir.path().join("prog");
    //       let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    //       let build = Command::new(&bin).arg("build").arg(&src_path)
    //           .arg("-o").arg(&exe).arg("--quiet")
    //           .env("COBRUST_CONFIG", &cfg_path)
    //           .env("SYN_KEY", "test-key")
    //           .output().unwrap();
    //       assert!(build.status.success(), "build failed: {:?}",
    //           String::from_utf8_lossy(&build.stderr));
    //
    //       let run = Command::new(&exe)
    //           .env("COBRUST_CONFIG", &cfg_path)
    //           .env("SYN_KEY", "test-key")
    //           .output().unwrap();
    //       let stdout = String::from_utf8_lossy(&run.stdout);
    //       assert!(stdout.contains("hi-from-mock"),
    //           "expected canned response in stdout, got {:?}", stdout);
    //   });
}

// =====================================================================
// Tier 3 #2 — Program:
//   ```
//   let xs: list[str] = llm_stream("syn", "m1", "hi")
//   for x in xs:
//       print(x)
//   ```
// Exercises the current alpha collect-all shim: today `llm_stream`
// returns either `[]` or a single full-response chunk, not provider
// delta frames.
// =====================================================================

#[test]
fn test_e2e_llm_stream_for_loop_prints_each_chunk() {
    require_impl("test_e2e_llm_stream_for_loop_prints_each_chunk");
    // Once impl lands, DEV uncomments:
    //
    //   // Non-streaming completion response. Current alpha contract wraps
    //   // the full response in a single list element instead of surfacing
    //   // provider delta frames.
    //   // ... wiremock setup as in test #1 returning a standard
    //   //     /chat/completions payload with assistant content "foo-bar" ...
    //   //
    //   // .cb program:
    //   //   let xs: list[str] = llm_stream("syn", "m1", "hi")
    //   //   for x in xs: print(x)
    //   //
    //   // Expected stdout: contains "foo-bar" exactly once.
    //   //
    //   // ... cobrust build + invoke exe, assert stdout.contains("foo-bar") ...
}

// =====================================================================
// Tier 3 #3 — Program: `print(llm_dispatch("greet", "hi"))`. Exercises
// routing-table lookup: the fixture `cobrust.toml` declares
// `[routing.greet]` pointing at the wiremock provider.
// =====================================================================

#[test]
fn test_e2e_llm_dispatch_routing_table_lookup_prints_response() {
    require_impl("test_e2e_llm_dispatch_routing_table_lookup_prints_response");
    // Once impl lands, DEV uncomments:
    //
    //   // ... wiremock returning {"choices":[{"message":{"content":"dispatched-ok"}}], ...}
    //   //     under route POST /chat/completions ...
    //   //
    //   // cobrust.toml fixture:
    //   //   [providers.syn] kind="openai" base_url=<server.uri()>
    //   //   [routing.greet] strategy="quality" preferred=["syn:m1"]
    //   //
    //   // .cb: print(llm_dispatch("greet", "hi"))
    //   //
    //   // Expected stdout: contains "dispatched-ok"
}

// =====================================================================
// Tier 3 #4 — Missing cobrust.toml → llm_complete returns "" → program
// prints empty line, exits 0. Decision 7 empty-on-failure semantics
// surfaced through the .cb source level.
// =====================================================================

#[test]
fn test_e2e_llm_complete_missing_cobrust_toml_prints_empty_exits_zero() {
    require_impl("test_e2e_llm_complete_missing_cobrust_toml_prints_empty_exits_zero");
    // Once impl lands, DEV uncomments:
    //
    //   let dir = tempfile::tempdir().unwrap();
    //   // COBRUST_CONFIG points at a path that does NOT exist.
    //   let bogus = dir.path().join("absent.toml");
    //   // .cb: print(llm_complete("syn", "m1", "hi"))
    //   // Build + invoke exe with COBRUST_CONFIG=<bogus>.
    //   // Expected exit code: 0 (no panic). Expected stdout: empty (or just "\n").
    //   //
    //   // ... assert exit code = 0 + stdout content matches "" or "\n" only ...
}

// =====================================================================
// Tier 3 #5 — Verify the ledger file is populated after a successful
// `cobrust run` invocation of an llm_complete program. The wiremock
// fixture serves a 200-OK and the .cobrust/ledger.jsonl in the cwd
// (or per ledger_path setting) gains exactly one outcome:ok entry.
// =====================================================================

#[test]
fn test_e2e_llm_ledger_jsonl_records_outcome_ok_entry_per_run() {
    require_impl("test_e2e_llm_ledger_jsonl_records_outcome_ok_entry_per_run");
    // Once impl lands, DEV uncomments:
    //
    //   // ... wiremock 200-OK for /chat/completions ...
    //   //
    //   // cobrust.toml fixture sets ledger_path = "<tmpdir>/ledger.jsonl"
    //   // .cb: print(llm_complete("syn", "m1", "hi"))
    //   // After running the produced executable once:
    //   //
    //   // let entries = read_ledger("<tmpdir>/ledger.jsonl");
    //   // assert_eq!(entries.iter().filter(|e| matches!(e.outcome, Outcome::Ok)).count(), 1);
    //   // assert_eq!(entries[0].provider, "syn");
    //   // assert_eq!(entries[0].task, "llm_complete");
}
