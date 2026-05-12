//! M-AI.1 α Phase 3 — end-to-end `.cb` source → compile → run tests
//! for `prompt_render` / `prompt_format_few_shot` /
//! `prompt_format_system_user` / `prompt_escape_braces` /
//! `llm_complete_structured` intrinsics.
//!
//! Spike spec: `docs/agent/spike/m-ai-1-cobrust-prompt-spike.md` §"Test
//! plan" Tier 3. Mirrors `crates/cobrust-cli/tests/intrinsics_llm.rs`
//! (M-AI.0 Tier 3 precedent) shape:
//!   1. Writes a small `.cb` program calling one of the five prompt fns.
//!   2. Writes a fixture `cobrust.toml` (for test #5 only; tests #1-#4
//!      are pure-fn with no network I/O — no cobrust.toml needed).
//!   3. Sets `COBRUST_CONFIG` env var (test #5 only).
//!   4. Invokes `cobrust build` + the produced executable, captures
//!      stdout, asserts contents.
//!
//! For tests #1-#4 (pure-fn cases), the `.cb` program is a simple
//! `print(prompt_*(...))` form and no wiremock is needed.
//!
//! Test #5 (`llm_complete_structured`) needs the wiremock setup +
//! a `[routing.structured]` cobrust.toml fixture. Mirrors
//! `intrinsics_llm.rs:103-162`'s wiremock JSON-response pattern.
//!
//! TDD STEP 1 — FAILING TEST CORPUS. Each test body calls
//! `require_impl()` which panics until the impl-landed marker is
//! flipped.
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
// Impl-landed marker. DEV flips when the M-AI.1 surface lands across:
//   - `crates/cobrust-cli/src/build.rs` (PRELUDE += five prompt stubs)
//   - `crates/cobrust-cli/src/build/intrinsics.rs` (five new rewrite arms)
//   - `crates/cobrust-codegen/src/cranelift_backend.rs`
//     (`runtime_helper_signatures` += five entries)
//   - `crates/cobrust-stdlib/src/prompt.rs` (module + helpers + shims)
// =====================================================================

const ADR_M_AI_1_IMPL_LANDED: bool = false;

fn require_impl(test_name: &str) {
    assert!(
        ADR_M_AI_1_IMPL_LANDED,
        "NOT YET IMPLEMENTED: M-AI.1 (α Phase 3 cobrust.prompt) e2e impl not yet landed; DEV agent must:\n  \
         1. Extend `crates/cobrust-cli/src/build.rs::PRELUDE` with five stub fns:\n     \
            `prompt_render` / `prompt_format_few_shot` / `prompt_format_system_user` /\n     \
            `prompt_escape_braces` / `llm_complete_structured`.\n  \
         2. Extend `crates/cobrust-cli/src/build/intrinsics.rs` with five\n     \
            rewrite arms targeting `PROMPT_*_RUNTIME_SYMBOL` constants.\n  \
         3. Extend `crates/cobrust-codegen/src/cranelift_backend.rs::\n     \
            runtime_helper_signatures()` with five new entries per spike\n     \
            §\"Runtime helper signatures (codegen amendment)\".\n  \
         4. Land `crates/cobrust-stdlib/src/prompt.rs` (module + pure-Rust\n     \
            helpers + C-ABI shims) per spike §\"C-ABI shim shape\".\n  \
         5. Flip ADR_M_AI_1_IMPL_LANDED = true in\n     \
            crates/cobrust-cli/tests/intrinsics_prompt.rs.\n  \
         FAILING TEST: {test_name}"
    );
}

// =====================================================================
// Tier 3 #1 — Program: `print(prompt_render("You are an expert.",
//   "Translate: {code}", ["code", "def foo(): pass"]))`.
// Exercises prompt_render PRELUDE stub → intrinsic rewrite → C-ABI shim
// → stdout pipeline. No wiremock needed (pure-fn).
// =====================================================================

#[test]
fn test_e2e_prompt_render_prints_interpolated_output() {
    require_impl("test_e2e_prompt_render_prints_interpolated_output");
    // Once impl lands, DEV uncomments:
    //
    //   use std::process::Command;
    //   let dir = tempfile::tempdir().unwrap();
    //   let src_path = dir.path().join("prog.cb");
    //   std::fs::write(&src_path, concat!(
    //       "print(prompt_render(",
    //       "\"You are an expert.\",",
    //       "\"Translate: {code}\",",
    //       "[\"code\", \"def foo(): pass\"]",
    //       "))\n",
    //   )).unwrap();
    //   let exe = dir.path().join("prog");
    //   let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    //   let build = Command::new(&bin)
    //       .arg("build").arg(&src_path).arg("-o").arg(&exe).arg("--quiet")
    //       .output().unwrap();
    //   assert!(build.status.success(), "build failed: {:?}",
    //       String::from_utf8_lossy(&build.stderr));
    //   let run = Command::new(&exe).output().unwrap();
    //   let stdout = String::from_utf8_lossy(&run.stdout);
    //   assert!(
    //       stdout.contains("You are an expert.\nTranslate: def foo(): pass"),
    //       "expected interpolated output in stdout, got {:?}", stdout,
    //   );
}

// =====================================================================
// Tier 3 #2 — Program using `prompt_format_few_shot` with two examples
// + current input → asserts canonical few-shot format in stdout.
// No wiremock needed (pure-fn).
// =====================================================================

#[test]
fn test_e2e_prompt_format_few_shot_prints_canonical_format() {
    require_impl("test_e2e_prompt_format_few_shot_prints_canonical_format");
    // Once impl lands, DEV uncomments:
    //
    //   use std::process::Command;
    //   let dir = tempfile::tempdir().unwrap();
    //   let src_path = dir.path().join("prog.cb");
    //   // .cb program that calls prompt_format_few_shot with 2 examples.
    //   std::fs::write(&src_path, concat!(
    //       "print(prompt_format_few_shot(",
    //       "[\"x = 1\", \"y = 2\"],",
    //       "[\"let x: i64 = 1\", \"let y: i64 = 2\"],",
    //       "\"z = 3\"",
    //       "))\n",
    //   )).unwrap();
    //   let exe = dir.path().join("prog");
    //   let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    //   let build = Command::new(&bin)
    //       .arg("build").arg(&src_path).arg("-o").arg(&exe).arg("--quiet")
    //       .output().unwrap();
    //   assert!(build.status.success(), "build failed: {:?}",
    //       String::from_utf8_lossy(&build.stderr));
    //   let run = Command::new(&exe).output().unwrap();
    //   let stdout = String::from_utf8_lossy(&run.stdout);
    //   assert!(
    //       stdout.contains("Input: x = 1\nOutput: let x: i64 = 1"),
    //       "expected canonical few-shot format in stdout, got {:?}", stdout,
    //   );
    //   assert!(stdout.contains("Input: z = 3\nOutput:"),
    //       "expected trailer in stdout, got {:?}", stdout);
}

// =====================================================================
// Tier 3 #3 — Program: `print(prompt_format_system_user("sys", "usr"))`.
// Exercises prompt_format_system_user → asserts joined "<sys>\n\n<usr>".
// No wiremock needed (pure-fn).
// =====================================================================

#[test]
fn test_e2e_prompt_format_system_user_prints_joined_string() {
    require_impl("test_e2e_prompt_format_system_user_prints_joined_string");
    // Once impl lands, DEV uncomments:
    //
    //   use std::process::Command;
    //   let dir = tempfile::tempdir().unwrap();
    //   let src_path = dir.path().join("prog.cb");
    //   std::fs::write(&src_path,
    //       "print(prompt_format_system_user(\"sys\", \"usr\"))\n",
    //   ).unwrap();
    //   let exe = dir.path().join("prog");
    //   let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    //   let build = Command::new(&bin)
    //       .arg("build").arg(&src_path).arg("-o").arg(&exe).arg("--quiet")
    //       .output().unwrap();
    //   assert!(build.status.success(), "build failed: {:?}",
    //       String::from_utf8_lossy(&build.stderr));
    //   let run = Command::new(&exe).output().unwrap();
    //   let stdout = String::from_utf8_lossy(&run.stdout);
    //   assert!(stdout.contains("sys\n\nusr"),
    //       "expected joined string in stdout, got {:?}", stdout);
}

// =====================================================================
// Tier 3 #4 — Program: `print(prompt_escape_braces("hello {world}"))`.
// Exercises prompt_escape_braces → asserts `"hello {{world}}"` in stdout.
// No wiremock needed (pure-fn).
// =====================================================================

#[test]
fn test_e2e_prompt_escape_braces_prints_escaped_output() {
    require_impl("test_e2e_prompt_escape_braces_prints_escaped_output");
    // Once impl lands, DEV uncomments:
    //
    //   use std::process::Command;
    //   let dir = tempfile::tempdir().unwrap();
    //   let src_path = dir.path().join("prog.cb");
    //   std::fs::write(&src_path,
    //       "print(prompt_escape_braces(\"hello {world}\"))\n",
    //   ).unwrap();
    //   let exe = dir.path().join("prog");
    //   let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    //   let build = Command::new(&bin)
    //       .arg("build").arg(&src_path).arg("-o").arg(&exe).arg("--quiet")
    //       .output().unwrap();
    //   assert!(build.status.success(), "build failed: {:?}",
    //       String::from_utf8_lossy(&build.stderr));
    //   let run = Command::new(&exe).output().unwrap();
    //   let stdout = String::from_utf8_lossy(&run.stdout);
    //   assert!(stdout.contains("hello {{world}}"),
    //       "expected escaped output in stdout, got {:?}", stdout);
}

// =====================================================================
// Tier 3 #5 — Program: `print(llm_complete_structured(prompt, schema))`.
// Exercises llm_complete_structured with wiremock-backed
// `[routing.structured]` cobrust.toml fixture. Mirrors
// `intrinsics_llm.rs:103-162`'s wiremock JSON-response pattern.
// =====================================================================

#[test]
fn test_e2e_llm_complete_structured_prints_canned_json_response_via_wiremock() {
    require_impl("test_e2e_llm_complete_structured_prints_canned_json_response_via_wiremock");
    // Once impl lands, DEV uncomments:
    //
    //   use std::process::Command;
    //   use wiremock::{Mock, MockServer, ResponseTemplate};
    //   use wiremock::matchers::{method, path};
    //
    //   let runtime = tokio::runtime::Runtime::new().unwrap();
    //   runtime.block_on(async {
    //       let server = MockServer::start().await;
    //       // The structured shim augments the prompt then dispatches via
    //       // llm_dispatch(task="structured", ...). The mock serves a
    //       // canned OpenAI-compatible response.
    //       Mock::given(method("POST"))
    //           .and(path("/chat/completions"))
    //           .respond_with(ResponseTemplate::new(200).set_body_json(
    //               serde_json::json!({
    //                   "id": "s1",
    //                   "object": "chat.completion",
    //                   "choices": [{
    //                       "message": {
    //                           "role": "assistant",
    //                           "content": "{\"code\":\"let x: i64 = 1\"}"
    //                       },
    //                       "finish_reason": "stop",
    //                       "index": 0
    //                   }],
    //                   "model": "m1",
    //                   "usage": {
    //                       "prompt_tokens": 10,
    //                       "completion_tokens": 5,
    //                       "total_tokens": 15
    //                   }
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
    //           [routing.structured]
    //           strategy = "quality"
    //           preferred = ["syn:m1"]
    //       "#, dir.path().display(), server.uri())).unwrap();
    //
    //       let schema = r#"{\"type\":\"object\",\"properties\":{\"code\":{\"type\":\"string\"}}}"#;
    //       let src_path = dir.path().join("prog.cb");
    //       std::fs::write(&src_path, format!(
    //           "print(llm_complete_structured(\"Translate x = 1 to Cobrust.\", \"{schema}\"))\n",
    //       )).unwrap();
    //
    //       let exe = dir.path().join("prog");
    //       let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    //       let build = Command::new(&bin)
    //           .arg("build").arg(&src_path).arg("-o").arg(&exe).arg("--quiet")
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
    //       assert!(stdout.contains("let x: i64 = 1"),
    //           "expected canned structured JSON response in stdout, got {:?}", stdout);
    //   });
}
