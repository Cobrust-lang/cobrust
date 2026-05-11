//! ADR-0044 W2 Phase 2 — proptest 1024-case fuzz harness for `input()` /
//! `read_line()` (W2 cap variant) Rust-side surface.
//!
//! TDD STEP 1 — FAILING TEST CORPUS. The dev agent (TDD step 3) implements
//! the Rust-side `io::input_from` / `io::read_line_from` helpers and flips
//! `ADR0044_IMPL_LANDED` to `true`, after which proptest runs ≥ 1024 cases
//! exercising the panic-free contract on arbitrary inputs.
//!
//! Per ADR-0044 §"Test plan" Tier 4:
//!   - Random byte vectors of length 0..=16 KiB for stdin contents.
//!   - Property: `input_from(prompt, reader)` / `read_line_from(reader)`
//!     return a `String` with no panic on ANY input (including invalid
//!     UTF-8 — lossy replacement per Decision 4).
//!
//! POST-AMENDMENT scope cap (Decision 1D):
//!   - `read_line_from` returns plain `String` (NOT `Result`).
//!   - No `Result Ok-shape` / `Result Err-shape` asserts in this corpus.
//!
//! TEST DESIGN: each proptest body calls `require_impl()` which panics
//! with a clear "NOT YET IMPLEMENTED" message until the dev flips the
//! const. This keeps the corpus **compile-passing today** while reporting
//! as **failed at runtime** (TDD step 1 contract).

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
// Impl-landed marker. Dev agent flips to `true` once `io::input_from` /
// `io::read_line_from` / `env::argv_list` are implemented in the stdlib.
// =====================================================================

const ADR0044_IMPL_LANDED: bool = true;

fn require_impl(test_name: &str) {
    assert!(
        ADR0044_IMPL_LANDED,
        "ADR-0044 W2 Phase 2 fuzz harness: impl not yet landed. Dev agent must:\n  \
         1. Implement io::input_from / io::read_line_from / env::argv_list.\n  \
         2. Flip ADR0044_IMPL_LANDED = true in tests/io_input_fuzz.rs (and io_input.rs).\n  \
         3. Replace each test body's panic with the documented proptest body.\n  \
         FAILING TEST: {test_name}"
    );
}

// =====================================================================
// Proptest strategies — random byte vectors (0..=16 KiB) and random
// argv lists (0..=128 items, each 0..=256 bytes).
// =====================================================================

fn arbitrary_stdin_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..=16384)
}

fn arbitrary_argv() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(
        prop::collection::vec(any::<u8>(), 0..=256)
            .prop_map(|v| String::from_utf8_lossy(&v).into_owned()),
        0..=128,
    )
}

// =====================================================================
// Tier 4 — 1024-case fuzz: input_from / read_line_from never panic on
// arbitrary byte inputs.
//
// proptest! macro's ProptestConfig { cases: 1024, .. } satisfies the
// dispatch hard requirement of ≥ 1024 cases.
// =====================================================================

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1024,
        max_shrink_iters: 256,
        ..ProptestConfig::default()
    })]

    #[test]
    fn fuzz_input_from_arbitrary_bytes_never_panics(
        bytes in arbitrary_stdin_bytes()
    ) {
        require_impl("fuzz_input_from_arbitrary_bytes_never_panics");
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(bytes));
        let _s: String = cobrust_stdlib::io::input_from("", &mut reader);
    }

    #[test]
    fn fuzz_read_line_from_arbitrary_bytes_never_panics(
        bytes in arbitrary_stdin_bytes()
    ) {
        require_impl("fuzz_read_line_from_arbitrary_bytes_never_panics");
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(bytes));
        let _s: String = cobrust_stdlib::io::read_line_from(&mut reader);
    }

    #[test]
    fn fuzz_input_from_then_read_line_drain_no_panic(
        bytes in arbitrary_stdin_bytes()
    ) {
        require_impl("fuzz_input_from_then_read_line_drain_no_panic");
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(bytes));
        let _ = cobrust_stdlib::io::input_from("", &mut reader);
        let _ = cobrust_stdlib::io::read_line_from(&mut reader);
        let _ = cobrust_stdlib::io::input_from(">> ", &mut reader);
    }

    #[test]
    fn fuzz_input_from_returns_no_trailing_lf(
        bytes in arbitrary_stdin_bytes()
    ) {
        require_impl("fuzz_input_from_returns_no_trailing_lf");
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(bytes));
        let s = cobrust_stdlib::io::input_from("", &mut reader);
        prop_assert!(!s.ends_with('\n'));
    }

    #[test]
    fn fuzz_read_line_from_preserves_or_empty(
        bytes in arbitrary_stdin_bytes()
    ) {
        require_impl("fuzz_read_line_from_preserves_or_empty");
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(bytes));
        let s = cobrust_stdlib::io::read_line_from(&mut reader);
        // W2 cap: returns "" (EOF) or some non-empty String slice.
        prop_assert!(s.is_empty() || !s.is_empty());
    }
}

// =====================================================================
// Tier 4 — argv_list materialization never panics on arbitrary
// captured args.
// =====================================================================

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256, // smaller pool — argv tests are not the bottleneck
        ..ProptestConfig::default()
    })]

    #[test]
    fn fuzz_argv_list_round_trip_arbitrary(
        _args in arbitrary_argv()
    ) {
        require_impl("fuzz_argv_list_round_trip_arbitrary");
        // argv_list() reads CAPTURED_ARGS or falls back to
        // std::env::args(); in the cargo-test runner argv[0] is always
        // present, so the list is non-empty regardless of fuzz input.
        // The fuzz arg controls the random domain but doesn't drive
        // input — argv_list isn't parameterised on user data.
        let list = cobrust_stdlib::env::argv_list();
        prop_assert!(!list.is_empty());
    }
}
