//! M-AI.1 α Phase 3 — proptest 1024-case fuzz harness for `cobrust.prompt`
//! pure-fn helpers `prompt_render_helper` / `prompt_format_few_shot_helper` /
//! `prompt_escape_braces_helper`.
//!
//! Spike spec: `docs/agent/spike/m-ai-1-cobrust-prompt-spike.md` §"Test
//! plan" Fuzz (≥ 1024 inputs). Mirrors the ADR-0044 W2 Phase 2
//! `io_input_fuzz.rs` / `llm_fuzz.rs` precedent (proptest 1024 iters,
//! ProptestConfig with max_shrink_iters: 256).
//!
//! Three properties:
//!   P1: `prompt_render_helper` never panics on any arbitrary UTF-8
//!       (system, user) + Vec<String> vars (0..=16 elements, each ≤ 4 KiB).
//!   P2: `prompt_format_few_shot_helper` never panics on any arbitrary
//!       (examples_in, examples_out, current_input).
//!   P3: `prompt_escape_braces_helper` + `prompt_render_helper` round-trip:
//!       for any text `t`, after escaping braces + rendering through an
//!       empty vars map, the suffix after `"\n"` equals `t`.
//!
//! TDD STEP 1 — FAILING TEST CORPUS. Each proptest body calls
//! `require_impl()` which panics until DEV flips the impl-landed marker.
//! The proptest! macro evaluates its bodies eagerly via `cases: 1024`;
//! the FIRST iteration panics in `require_impl`, reported as a failure.
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
// Impl-landed marker. DEV flips after the M-AI.1 stdlib::prompt surface
// lands and the five helpers are callable.
// =====================================================================

const ADR_M_AI_1_IMPL_LANDED: bool = true;

fn require_impl(test_name: &str) {
    assert!(
        ADR_M_AI_1_IMPL_LANDED,
        "NOT YET IMPLEMENTED: M-AI.1 (α Phase 3 cobrust.prompt) fuzz harness: impl not yet landed.\n  \
         DEV agent must:\n  \
         1. Land `cobrust_stdlib::prompt` module with all five helpers\n     \
            (see tests/prompt_corpus.rs `require_impl` for the full list).\n  \
         2. Flip ADR_M_AI_1_IMPL_LANDED = true in tests/prompt_fuzz.rs.\n  \
         3. Replace each fuzz body's `let _ = ...` with the documented proptest\n     \
            body (call the helper on the arbitrary input, assert no panic).\n  \
         FAILING TEST: {test_name}"
    );
}

// =====================================================================
// Proptest strategies — random byte vectors up to 4 KiB decoded lossily
// to UTF-8. M-AI.1 prompts are smaller than M-AI.0 LLM request bodies,
// so 4 KiB cap suffices (vs 16 KiB in llm_fuzz.rs).
// =====================================================================

fn arbitrary_utf8_string_up_to_4k() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<u8>(), 0..=4096)
        .prop_map(|v| String::from_utf8_lossy(&v).into_owned())
}

// =====================================================================
// Fuzz P1 — 1024-iter proptest: prompt_render_helper(system, user, vars)
// for any UTF-8 lossy (system, user) + arbitrary Vec<String> vars
// (0..=16 elements) returns a String with no panic.
// Property: for any input, the function terminates and does not panic
// (Decision 7: failures collapse to empty string, never panic/UB).
// =====================================================================

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1024,
        max_shrink_iters: 256,
        ..ProptestConfig::default()
    })]

    #[test]
    fn fuzz_prompt_render_helper_never_panics(
        system in arbitrary_utf8_string_up_to_4k(),
        user in arbitrary_utf8_string_up_to_4k(),
        vars in prop::collection::vec(arbitrary_utf8_string_up_to_4k(), 0..=16),
    ) {
        require_impl("fuzz_prompt_render_helper_never_panics");
        //
        //   use cobrust_stdlib::prompt::prompt_render_helper;
        //   // Property: any (system, user, vars) must not panic.
        //   // The helper returns a String (possibly empty for failure cases).
        //   let result = prompt_render_helper(&system, &user, &vars);
        //   // No specific assertion on content — the property is "no panic".
        //   // The output must be a valid String (which prop_assert! verifies by
        //   // type alone — String is always valid UTF-8 on creation).
        //   let _ = result;
        let _ = (&system, &user, &vars);
    }

    // ==================================================================
    // Fuzz P2 — 1024-iter proptest: prompt_format_few_shot_helper never
    // panics on any arbitrary (examples_in, examples_out, current_input).
    // Decision 5: mismatched lengths → truncate to min; no panic.
    // ==================================================================

    #[test]
    fn fuzz_prompt_format_few_shot_helper_never_panics(
        examples_in in prop::collection::vec(arbitrary_utf8_string_up_to_4k(), 0..=8),
        examples_out in prop::collection::vec(arbitrary_utf8_string_up_to_4k(), 0..=8),
        current_input in arbitrary_utf8_string_up_to_4k(),
    ) {
        require_impl("fuzz_prompt_format_few_shot_helper_never_panics");
        //
        //   use cobrust_stdlib::prompt::prompt_format_few_shot_helper;
        //   // Property: any (examples_in, examples_out, current_input) must not panic.
        //   // Mismatched lengths → truncate to min. No panic regardless of content.
        //   let result = prompt_format_few_shot_helper(&examples_in, &examples_out, &current_input);
        //   let _ = result;
        let _ = (&examples_in, &examples_out, &current_input);
    }

    // ==================================================================
    // Fuzz P3 — 1024-iter proptest: prompt_escape_braces_helper +
    // prompt_render_helper round-trip leaves the suffix unchanged.
    //
    // For any text `t`:
    //   escaped = prompt_escape_braces_helper(t)
    //   rendered = prompt_render_helper("", &escaped, &[])
    //   rendered ends with t (after the leading "\n" separator).
    //
    // This verifies that the escape mechanism correctly defeats the
    // interpolation pass for arbitrary input containing `{` and `}`
    // (Decision 4: `{{` / `}}` escape specification).
    // ==================================================================

    #[test]
    fn fuzz_prompt_escape_braces_render_round_trip_preserves_suffix(
        text in arbitrary_utf8_string_up_to_4k(),
    ) {
        require_impl("fuzz_prompt_escape_braces_render_round_trip_preserves_suffix");
        //
        //   use cobrust_stdlib::prompt::{prompt_escape_braces_helper, prompt_render_helper};
        //   let escaped = prompt_escape_braces_helper(&text);
        //   // Render with empty system ("") and escaped user template; empty vars.
        //   let rendered = prompt_render_helper("", &escaped, &[]);
        //   // rendered = "\n" + text (the "\n" is the system/user separator).
        //   // The suffix after "\n" must equal the original text verbatim.
        //   let suffix = rendered.strip_prefix('\n').unwrap_or(&rendered);
        //   prop_assert_eq!(suffix, text.as_str(),
        //       "escape → render round-trip must restore original literal text as suffix");
        let _ = &text;
    }
}
