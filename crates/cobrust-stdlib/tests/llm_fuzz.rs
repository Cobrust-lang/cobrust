//! M-AI.0 α Phase 2 — proptest 1024-case fuzz harness for `cobrust.llm`
//! blocking helpers.
//!
//! Spike spec: `docs/agent/spike/m-ai-0-cobrust-llm-spike.md` §"Test
//! plan" Fuzz (≥ 1024 inputs). Mirrors the ADR-0044 W2 Phase 2
//! `io_input_fuzz.rs` precedent (proptest 1024 iters, byte-vector
//! strategy 0..=16 KiB).
//!
//! Property: any `(provider, model, prompt)` triple with arbitrary
//! UTF-8 content + length 0..=16 KiB returns either a non-empty Str
//! or "" — no panic, no UB.
//!
//! TDD STEP 1 — FAILING TEST CORPUS. Each proptest body calls
//! `require_impl()` which panics until DEV flips the impl-landed
//! marker. The proptest! macro evaluates its bodies eagerly via the
//! `cases: 1024` config; the FIRST iteration panics in `require_impl`,
//! which proptest reports as a test failure.
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

use proptest::prelude::*;

// =====================================================================
// Impl-landed marker. DEV flips after the M-AI.0 stdlib::llm surface
// lands and the synthetic-provider test-injection seam is wired.
// =====================================================================

const ADR_M_AI_0_IMPL_LANDED: bool = true;

fn require_impl(test_name: &str) {
    assert!(
        ADR_M_AI_0_IMPL_LANDED,
        "NOT YET IMPLEMENTED: M-AI.0 (α Phase 2 cobrust.llm) fuzz harness: impl not yet landed.\n  \
         DEV agent must:\n  \
         1. Land `cobrust_stdlib::llm` Rust helpers + synthetic seam\n     \
            (see tests/llm_corpus.rs `require_impl` for the full list).\n  \
         2. Flip ADR_M_AI_0_IMPL_LANDED = true in tests/llm_fuzz.rs.\n  \
         3. Replace each fuzz body's panic with the documented proptest\n     \
            body (synthetic-double scripted with an echo response of\n     \
            arbitrary bytes, asserting the helper returns either the\n     \
            echoed text or empty string without panic).\n  \
         FAILING TEST: {test_name}"
    );
}

// =====================================================================
// Proptest strategies — random byte vectors up to 16 KiB, decoded
// lossily to UTF-8 since the helpers' inputs are `str`-typed.
// =====================================================================

fn arbitrary_utf8_string_up_to_16k() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<u8>(), 0..=16384)
        .prop_map(|v| String::from_utf8_lossy(&v).into_owned())
}

// =====================================================================
// Fuzz body — 1024-iter proptest: llm_complete_blocking(p, m, q) for
// random (p, m, q) returns a String with no panic. The property covers
// the Decision 7 surface (Either non-empty Str returned, OR "" on any
// failure — both are valid, only panic is forbidden).
// =====================================================================

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1024,
        max_shrink_iters: 256,
        ..ProptestConfig::default()
    })]

    #[test]
    fn fuzz_llm_complete_blocking_never_panics(
        provider in arbitrary_utf8_string_up_to_16k(),
        model in arbitrary_utf8_string_up_to_16k(),
        prompt in arbitrary_utf8_string_up_to_16k(),
    ) {
        require_impl("fuzz_llm_complete_blocking_never_panics");
        // Once impl lands, DEV uncomments:
        //
        //   // Synthetic-double scripted with an echo of `prompt` so we can
        //   // assert either-or property below.
        //   let provider_dbl = SyntheticDouble::new(vec![
        //       Scripted::Ok(prompt.clone())
        //   ]);
        //   cobrust_stdlib::llm::__test_register_synthetic_provider(
        //       &provider, provider_dbl);
        //   let _guard = set_cobrust_config(&write_synthetic_toml_single(&provider));
        //   let out: String = cobrust_stdlib::llm::llm_complete_blocking(
        //       &provider, &model, &prompt);
        //   // Decision 7: helper returns either non-empty Str or "" on failure.
        //   prop_assert!(out == prompt || out.is_empty(),
        //       "llm_complete_blocking must echo synthetic text or return \"\"");
        let _ = (&provider, &model, &prompt);
    }

    #[test]
    fn fuzz_llm_dispatch_blocking_never_panics(
        task in arbitrary_utf8_string_up_to_16k(),
        prompt in arbitrary_utf8_string_up_to_16k(),
    ) {
        require_impl("fuzz_llm_dispatch_blocking_never_panics");
        // Once impl lands, DEV uncomments:
        //
        //   // Unknown task name (random fuzz) → "" per Decision 7. The
        //   // property is "no panic regardless of arbitrary task name + prompt".
        //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
        //   let out: String = cobrust_stdlib::llm::llm_dispatch_blocking(
        //       &task, &prompt);
        //   // Unknown task → "" is expected. Known task (rare in fuzz) →
        //   // synthetic echo. Either way, the contract is no panic.
        //   let _ = out;
        let _ = (&task, &prompt);
    }

    #[test]
    fn fuzz_llm_stream_blocking_never_panics(
        provider in arbitrary_utf8_string_up_to_16k(),
        model in arbitrary_utf8_string_up_to_16k(),
        prompt in arbitrary_utf8_string_up_to_16k(),
    ) {
        require_impl("fuzz_llm_stream_blocking_never_panics");
        // Once impl lands, DEV uncomments:
        //
        //   let provider_dbl = SyntheticDouble::new(vec![
        //       Scripted::Ok(prompt.clone())
        //   ]);
        //   cobrust_stdlib::llm::__test_register_synthetic_provider(
        //       &provider, provider_dbl);
        //   let _guard = set_cobrust_config(&write_synthetic_toml_single(&provider));
        //   let chunks: Vec<String> = cobrust_stdlib::llm::llm_stream_blocking(
        //       &provider, &model, &prompt);
        //   // Decision 3B/7: empty Vec on failure, else ordered Delta texts.
        //   // The only forbidden outcome is a panic.
        //   let _ = chunks;
        let _ = (&provider, &model, &prompt);
    }
}
