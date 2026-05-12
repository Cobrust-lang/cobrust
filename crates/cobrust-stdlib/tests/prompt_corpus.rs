//! M-AI.1 α Phase 3 — Rust-side stdlib unit tests for `cobrust.prompt`
//! blocking helpers `prompt_render_helper` / `prompt_format_few_shot_helper` /
//! `prompt_format_system_user_helper` / `prompt_escape_braces_helper` /
//! `llm_complete_structured_helper`.
//!
//! Spike spec: `docs/agent/spike/m-ai-1-cobrust-prompt-spike.md` §"Test
//! plan (binding for P7-TEST)" Tier 1. P10 ratified the spike's three
//! open questions (OQ-1 3-arg list[str] / OQ-2 hardcoded "structured" /
//! OQ-3 "Output:" no-whitespace trailer).
//!
//! TDD STEP 1 — FAILING TEST CORPUS. The DEV agent (TDD step 3)
//! implements the stdlib + CLI + codegen surface until every test
//! passes; the impl scope is enumerated in the spike §"Implementation
//! map (binding for P7-DEV)".
//!
//! Per ADR-0044 W2 Phase 2 TDD-step-1 precedent (`llm_corpus.rs` shape),
//! each test body calls `require_impl()` which panics with a clear "NOT
//! YET IMPLEMENTED" message until DEV flips `ADR_M_AI_1_IMPL_LANDED` to
//! `true`. Live calls to the future surface are held as documentation
//! comments so the corpus compiles today without referencing not-yet-
//! existing symbols. DEV uncomments the bodies once stdlib::prompt lands.
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
//   - `cobrust_stdlib::prompt::prompt_render_helper`
//   - `cobrust_stdlib::prompt::prompt_format_few_shot_helper`
//   - `cobrust_stdlib::prompt::prompt_format_system_user_helper`
//   - `cobrust_stdlib::prompt::prompt_escape_braces_helper`
//   - `cobrust_stdlib::prompt::llm_complete_structured_helper` (llm-router feature)
//   - the `pub mod prompt;` declaration in `cobrust_stdlib::lib.rs`
// are landed in `crates/cobrust-stdlib/src/prompt.rs`.
// =====================================================================

const ADR_M_AI_1_IMPL_LANDED: bool = false;

fn require_impl(test_name: &str) {
    assert!(
        ADR_M_AI_1_IMPL_LANDED,
        "NOT YET IMPLEMENTED: M-AI.1 (α Phase 3 cobrust.prompt) impl not yet landed; DEV agent must:\n  \
         1. Add `crates/cobrust-stdlib/src/prompt.rs` module with\n     \
            `prompt_render_helper(system, user, vars) -> String`,\n     \
            `prompt_format_few_shot_helper(examples_in, examples_out, current_input) -> String`,\n     \
            `prompt_format_system_user_helper(system, user) -> String`,\n     \
            `prompt_escape_braces_helper(text) -> String`,\n     \
            `llm_complete_structured_helper(prompt, schema_json) -> String` (llm-router feature).\n  \
         2. Add `pub mod prompt;` (unconditional) in `crates/cobrust-stdlib/src/lib.rs`.\n  \
         3. Add five C-ABI shims: `__cobrust_prompt_render`, `__cobrust_prompt_format_few_shot`,\n     \
            `__cobrust_prompt_format_system_user`, `__cobrust_prompt_escape_braces`,\n     \
            `__cobrust_llm_complete_structured` per spike §\"C-ABI shim shape\".\n  \
         4. Flip ADR_M_AI_1_IMPL_LANDED = true in tests/prompt_corpus.rs +\n     \
            tests/prompt_cabi_corpus.rs + tests/prompt_fuzz.rs +\n     \
            crates/cobrust-cli/tests/intrinsics_prompt.rs.\n  \
         FAILING TEST: {test_name}"
    );
}

// =====================================================================
// Tier 1 #1 — prompt_render_helper with empty vars returns
// format!("{system}\n{user}").
// =====================================================================

#[test]
fn test_prompt_render_helper_empty_vars_returns_system_newline_user() {
    require_impl("test_prompt_render_helper_empty_vars_returns_system_newline_user");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_render_helper;
    //   let result = prompt_render_helper("sys", "usr", &[]);
    //   assert_eq!(result, "sys\nusr");
}

// =====================================================================
// Tier 1 #2 — prompt_render_helper with single key/value pair substitutes
// the placeholder correctly in the user template.
// =====================================================================

#[test]
fn test_prompt_render_helper_single_key_value_substitutes_correctly() {
    require_impl("test_prompt_render_helper_single_key_value_substitutes_correctly");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_render_helper;
    //   let vars = vec!["code".to_string(), "def foo(): pass".to_string()];
    //   let result = prompt_render_helper(
    //       "You are an expert.",
    //       "Translate: {code}",
    //       &vars,
    //   );
    //   assert_eq!(result, "You are an expert.\nTranslate: def foo(): pass");
}

// =====================================================================
// Tier 1 #3 — prompt_render_helper with multiple key/value pairs
// substitutes all placeholders.
// =====================================================================

#[test]
fn test_prompt_render_helper_multiple_pairs_substitutes_all() {
    require_impl("test_prompt_render_helper_multiple_pairs_substitutes_all");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_render_helper;
    //   let vars = vec![
    //       "lang".to_string(), "python".to_string(),
    //       "code".to_string(), "x = 1".to_string(),
    //   ];
    //   let result = prompt_render_helper(
    //       "Convert {lang} code.",
    //       "Input: {code}",
    //       &vars,
    //   );
    //   assert_eq!(result, "Convert python code.\nInput: x = 1");
}

// =====================================================================
// Tier 1 #4 — prompt_render_helper with unknown placeholder keeps the
// literal `{unknown}` in the output (Decision 4: silent forwarding).
// =====================================================================

#[test]
fn test_prompt_render_helper_unknown_placeholder_keeps_literal() {
    require_impl("test_prompt_render_helper_unknown_placeholder_keeps_literal");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_render_helper;
    //   let result = prompt_render_helper("sys", "Hello {unknown}", &[]);
    //   assert_eq!(result, "sys\nHello {unknown}");
}

// =====================================================================
// Tier 1 #5 — prompt_render_helper with `{{` and `}}` escape sequences
// renders literal `{` and `}` in the output (Decision 4).
// =====================================================================

#[test]
fn test_prompt_render_helper_double_brace_escape_renders_literal_braces() {
    require_impl("test_prompt_render_helper_double_brace_escape_renders_literal_braces");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_render_helper;
    //   let result = prompt_render_helper("sys", "{{literal}} braces", &[]);
    //   assert_eq!(result, "sys\n{literal} braces");
}

// =====================================================================
// Tier 1 #6 — prompt_render_helper with odd-length vars list drops the
// trailing unmatched key silently (Decision 3 + 7).
// =====================================================================

#[test]
fn test_prompt_render_helper_odd_length_vars_drops_trailing_key_silently() {
    require_impl("test_prompt_render_helper_odd_length_vars_drops_trailing_key_silently");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_render_helper;
    //   // 3 elements: ["key1", "val1", "orphan"] — "orphan" has no value.
    //   let vars = vec![
    //       "key1".to_string(), "val1".to_string(), "orphan".to_string(),
    //   ];
    //   let result = prompt_render_helper("sys", "Value: {key1} orphan: {orphan}", &vars);
    //   // {key1} gets substituted; {orphan} remains literal (no value).
    //   assert_eq!(result, "sys\nValue: val1 orphan: {orphan}");
}

// =====================================================================
// Tier 1 #7 — prompt_render_helper with empty system + empty user +
// non-empty vars returns "\n" (no substitution opportunities in empty
// combined string, just the separator).
// =====================================================================

#[test]
fn test_prompt_render_helper_empty_system_user_returns_newline() {
    require_impl("test_prompt_render_helper_empty_system_user_returns_newline");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_render_helper;
    //   let vars = vec!["k".to_string(), "v".to_string()];
    //   let result = prompt_render_helper("", "", &vars);
    //   assert_eq!(result, "\n");
}

// =====================================================================
// Tier 1 #8 — prompt_render_helper with UTF-8 multi-byte template +
// UTF-8 multi-byte value substitutes byte-correctly.
// =====================================================================

#[test]
fn test_prompt_render_helper_utf8_multibyte_substitutes_correctly() {
    require_impl("test_prompt_render_helper_utf8_multibyte_substitutes_correctly");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_render_helper;
    //   let vars = vec!["greeting".to_string(), "こんにちは".to_string()];
    //   let result = prompt_render_helper(
    //       "你好",
    //       "Say: {greeting}",
    //       &vars,
    //   );
    //   assert_eq!(result, "你好\nSay: こんにちは");
}

// =====================================================================
// Tier 1 #9 — prompt_render_helper with later same-key override returns
// the latest binding (Decision 3: later same-key overrides earlier).
// =====================================================================

#[test]
fn test_prompt_render_helper_same_key_later_binding_overrides_earlier() {
    require_impl("test_prompt_render_helper_same_key_later_binding_overrides_earlier");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_render_helper;
    //   let vars = vec![
    //       "k".to_string(), "first".to_string(),
    //       "k".to_string(), "second".to_string(),
    //   ];
    //   let result = prompt_render_helper("sys", "Val: {k}", &vars);
    //   assert_eq!(result, "sys\nVal: second",
    //       "later same-key binding must override earlier one (BTreeMap insert)");
}

// =====================================================================
// Tier 1 #10 — prompt_format_few_shot_helper with one example pair +
// current input produces the canonical "Input: ...\nOutput: ...\n\n
// Input: ...\nOutput:" format (Decision 5).
// =====================================================================

#[test]
fn test_prompt_format_few_shot_helper_one_example_produces_canonical_format() {
    require_impl("test_prompt_format_few_shot_helper_one_example_produces_canonical_format");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_format_few_shot_helper;
    //   let xin  = vec!["x = 1".to_string()];
    //   let xout = vec!["let x: i64 = 1".to_string()];
    //   let result = prompt_format_few_shot_helper(&xin, &xout, "y = 2");
    //   let expected = "Input: x = 1\nOutput: let x: i64 = 1\n\nInput: y = 2\nOutput:";
    //   assert_eq!(result, expected);
}

// =====================================================================
// Tier 1 #11 — prompt_format_few_shot_helper with multiple example pairs
// + current input emits N blocks + canonical trailer.
// =====================================================================

#[test]
fn test_prompt_format_few_shot_helper_multiple_examples_emits_n_blocks_plus_trailer() {
    require_impl("test_prompt_format_few_shot_helper_multiple_examples_emits_n_blocks_plus_trailer");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_format_few_shot_helper;
    //   let xin  = vec!["x = 1".to_string(), "y = 2".to_string()];
    //   let xout = vec!["let x: i64 = 1".to_string(), "let y: i64 = 2".to_string()];
    //   let result = prompt_format_few_shot_helper(&xin, &xout, "z = 3");
    //   let expected = concat!(
    //       "Input: x = 1\nOutput: let x: i64 = 1\n\n",
    //       "Input: y = 2\nOutput: let y: i64 = 2\n\n",
    //       "Input: z = 3\nOutput:",
    //   );
    //   assert_eq!(result, expected);
}

// =====================================================================
// Tier 1 #12 — prompt_format_few_shot_helper with empty examples lists +
// non-empty current input emits just the `Input: <current>\nOutput:` trailer
// (Decision 5: empty examples list → just trailer).
// =====================================================================

#[test]
fn test_prompt_format_few_shot_helper_empty_examples_emits_just_trailer() {
    require_impl("test_prompt_format_few_shot_helper_empty_examples_emits_just_trailer");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_format_few_shot_helper;
    //   let result = prompt_format_few_shot_helper(&[], &[], "z = 3");
    //   assert_eq!(result, "Input: z = 3\nOutput:");
}

// =====================================================================
// Tier 1 #13 — prompt_format_few_shot_helper with mismatched list lengths
// truncates to min(len(in), len(out)) (Decision 5 + 7).
// =====================================================================

#[test]
fn test_prompt_format_few_shot_helper_mismatched_lengths_truncates_to_min() {
    require_impl("test_prompt_format_few_shot_helper_mismatched_lengths_truncates_to_min");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_format_few_shot_helper;
    //   // 3 inputs, 2 outputs — only 2 pairs should appear.
    //   let xin  = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    //   let xout = vec!["A".to_string(), "B".to_string()];
    //   let result = prompt_format_few_shot_helper(&xin, &xout, "d");
    //   // "c" has no matching output — truncated.
    //   let expected = "Input: a\nOutput: A\n\nInput: b\nOutput: B\n\nInput: d\nOutput:";
    //   assert_eq!(result, expected);
}

// =====================================================================
// Tier 1 #14 — prompt_format_few_shot_helper with UTF-8 multi-byte
// content preserves bytes exactly.
// =====================================================================

#[test]
fn test_prompt_format_few_shot_helper_utf8_multibyte_content_preserved() {
    require_impl("test_prompt_format_few_shot_helper_utf8_multibyte_content_preserved");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_format_few_shot_helper;
    //   let xin  = vec!["你好".to_string()];
    //   let xout = vec!["こんにちは".to_string()];
    //   let result = prompt_format_few_shot_helper(&xin, &xout, "안녕");
    //   let expected = "Input: 你好\nOutput: こんにちは\n\nInput: 안녕\nOutput:";
    //   assert_eq!(result, expected);
}

// =====================================================================
// Tier 1 #15 — prompt_format_system_user_helper produces the canonical
// "<system>\n\n<user>" concatenation (Decision 1B flat-fn).
// =====================================================================

#[test]
fn test_prompt_format_system_user_helper_produces_canonical_format() {
    require_impl("test_prompt_format_system_user_helper_produces_canonical_format");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_format_system_user_helper;
    //   let result = prompt_format_system_user_helper(
    //       "You are a Cobrust expert.",
    //       "Translate this code.",
    //   );
    //   assert_eq!(result, "You are a Cobrust expert.\n\nTranslate this code.");
}

// =====================================================================
// Tier 1 #16 — prompt_escape_braces_helper escapes `{` to `{{` and `}`
// to `}}` (Decision 4: escape mechanism for literal braces).
// =====================================================================

#[test]
fn test_prompt_escape_braces_helper_escapes_braces_correctly() {
    require_impl("test_prompt_escape_braces_helper_escapes_braces_correctly");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_escape_braces_helper;
    //   let result = prompt_escape_braces_helper("hello {world}");
    //   assert_eq!(result, "hello {{world}}");
}

// =====================================================================
// Tier 1 #17 — prompt_escape_braces_helper round-trips through
// prompt_render_helper: escape → render leaves the original literal text
// (Decision 4: escape is the symmetric pre-pass for interpolation).
// =====================================================================

#[test]
fn test_prompt_escape_braces_helper_round_trips_through_render() {
    require_impl("test_prompt_escape_braces_helper_round_trips_through_render");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::{prompt_escape_braces_helper, prompt_render_helper};
    //   let original = "value: {x}";
    //   // Escape the literal braces so they survive the interpolation pass.
    //   let escaped = prompt_escape_braces_helper(original);
    //   assert_eq!(escaped, "value: {{x}}");
    //   // After rendering with an empty vars map, the output must equal original.
    //   let rendered = prompt_render_helper("", &escaped, &[]);
    //   // rendered = "\n" + "value: {x}"
    //   assert!(rendered.ends_with(original),
    //       "escape → render round-trip must restore original literal text");
}

// =====================================================================
// Tier 1 #18 — llm_complete_structured_helper (gated by llm-router
// feature): with synthetic provider routes through "structured" task
// and returns the canned response text.
// =====================================================================

#[test]
#[cfg(feature = "llm-router")]
fn test_llm_complete_structured_helper_synthetic_provider_returns_canned_response() {
    require_impl("test_llm_complete_structured_helper_synthetic_provider_returns_canned_response");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::llm_complete_structured_helper;
    //   // Stand up a synthetic double that always replies with "structured-canned".
    //   // Per M-AI.0 OQ-3 WRAP pattern: use the test-injection seam on
    //   // cobrust_stdlib::llm to register an in-process synthetic provider.
    //   let canned = "structured-canned";
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok(canned.into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(fixture_path("syn_structured.toml"));
    //   // syn_structured.toml must declare [routing.structured] pointing at syn:m1.
    //   let schema = r#"{"type":"object","properties":{"code":{"type":"string"}}}"#;
    //   let out = llm_complete_structured_helper("Translate this code.", schema);
    //   assert_eq!(out, canned, "structured helper must return synthetic canned text");
}

// =====================================================================
// Tier 1 #19 — llm_complete_structured_helper (gated): with missing
// cobrust.toml or missing [routing.structured] returns "" (Decision 7).
// =====================================================================

#[test]
#[cfg(feature = "llm-router")]
fn test_llm_complete_structured_helper_missing_config_returns_empty() {
    require_impl("test_llm_complete_structured_helper_missing_config_returns_empty");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::llm_complete_structured_helper;
    //   // Point COBRUST_CONFIG at a path that does not exist.
    //   let _guard = set_cobrust_config("/nonexistent/cobrust.toml.does-not-exist");
    //   let schema = r#"{"type":"object"}"#;
    //   let out = llm_complete_structured_helper("hello", schema);
    //   assert_eq!(out, "", "missing config must return empty string (Decision 7)");
}

// =====================================================================
// Tier 1 #20 — verify.py oracle (ADR-0047a mandate): the pure-fn
// deterministic cases must match what `tests/prompt_corpus_verify.py
// <case>` prints. This is the independent oracle confirmation per F23-A.
// Test shells out to the Python oracle for the representative case.
// =====================================================================

#[test]
fn test_verify_py_oracle_matches_prompt_render_helper_output() {
    require_impl("test_verify_py_oracle_matches_prompt_render_helper_output");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::prompt::prompt_render_helper;
    //   use std::process::Command;
    //   let out = Command::new("python3")
    //       .arg("tests/prompt_corpus_verify.py")
    //       .arg("test_prompt_render_helper_single_key_value_substitutes_correctly")
    //       .output()
    //       .unwrap();
    //   assert!(out.status.success(), "verify.py exited non-zero: {:?}",
    //       String::from_utf8_lossy(&out.stderr));
    //   let py_text = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    //   // Compare to Rust-side prompt_render_helper output for the same fixture.
    //   let vars = vec!["code".to_string(), "def foo(): pass".to_string()];
    //   let cb_out = prompt_render_helper(
    //       "You are an expert.",
    //       "Translate: {code}",
    //       &vars,
    //   );
    //   assert_eq!(cb_out, py_text,
    //       "Cobrust prompt_render_helper must match verify.py oracle for fixture case");
}
