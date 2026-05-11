//! ADR-0044 W2 Phase 2 — Rust-side stdlib unit tests for `input()` /
//! `read_line()` (W2 cap variant) Rust surface + C-ABI shims.
//!
//! TDD STEP 1 — FAILING TEST CORPUS. The dev agent (TDD step 3) implements
//! until every test passes.
//!
//! Per ADR-0044 §"Implementation map" — the dev work touches:
//!   - `crates/cobrust-stdlib/src/io.rs` — add `input(prompt) -> String`
//!     Rust-side + `__cobrust_input` / `__cobrust_input_no_prompt` /
//!     `__cobrust_read_line` C-ABI shims.
//!   - `crates/cobrust-stdlib/src/env.rs` — add `__cobrust_argv` shim.
//!
//! POST-AMENDMENT scope cap (Decision 1D):
//!   - `read_line` returns plain `String` (NOT `Result<String, IoError>`).
//!   - Tests use plain-string semantics; no `Result Ok-shape` / `Result Err-shape`.
//!
//! TEST DESIGN: each test body **panics with a clear "NOT YET IMPLEMENTED"
//! message** describing the assertion that the dev agent must produce. Dev
//! replaces the panic body with real assertions invoking the new Rust-side
//! surface (e.g. `io::input_from(prompt, &mut reader)` or whatever name
//! is chosen).
//!
//! This shape ensures:
//!   1. The test corpus **compiles today** (no E0425 cannot-find-fn errors).
//!   2. `cargo test` reports each function as **failed** (it panics).
//!   3. The dev agent has a clear contract for what to assert.
//!
//! Per `feedback_p9_clippy_stall_pattern.md` — 18-lint test header at TOP.

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
// Marker: when the dev agent's impl lands, flip this to `true`. Each
// test asserts this is true before exercising the impl. This keeps the
// FAILING TEST CORPUS contract obvious: tests fail today because impl
// doesn't exist.
// =====================================================================

const ADR0044_IMPL_LANDED: bool = true;

fn require_impl(test_name: &str) {
    assert!(
        ADR0044_IMPL_LANDED,
        "ADR-0044 W2 Phase 2 impl not yet landed; dev agent must:\n  \
         1. Add `io::input_from(prompt, reader) -> String` Rust-side fn\n  \
         2. Add `io::read_line_from(reader) -> String` Rust-side fn (W2 cap)\n  \
         3. Add `__cobrust_input` / `__cobrust_input_no_prompt` / \
         `__cobrust_read_line` / `__cobrust_argv` C-ABI shims\n  \
         4. Add `env::argv_list() -> Vec<String>` Rust-side helper\n  \
         5. Flip ADR0044_IMPL_LANDED = true in tests/io_input.rs\n  \
         FAILING TEST: {test_name}"
    );
}

// =====================================================================
// Tier 1 #1 — input on empty stdin returns ""
// =====================================================================

#[test]
fn test_io_input_from_empty_reader_returns_empty() {
    require_impl("test_io_input_from_empty_reader_returns_empty");
    let mut r = std::io::BufReader::new(std::io::Cursor::new(Vec::<u8>::new()));
    let s = cobrust_stdlib::io::input_from("", &mut r);
    assert_eq!(s, "");
}

// ----- Tier 1 #3 — input strips trailing \n ------------------------

#[test]
fn test_io_input_from_strips_trailing_lf() {
    require_impl("test_io_input_from_strips_trailing_lf");
    let mut r = std::io::BufReader::new(std::io::Cursor::new(b"hello\n".to_vec()));
    let s = cobrust_stdlib::io::input_from("", &mut r);
    assert_eq!(s, "hello");
}

// ----- Tier 1 #4 — input keeps \r, strips only \n ------------------

#[test]
fn test_io_input_from_keeps_cr_strips_only_lf() {
    require_impl("test_io_input_from_keeps_cr_strips_only_lf");
    let mut r = std::io::BufReader::new(std::io::Cursor::new(b"hello\r\n".to_vec()));
    let s = cobrust_stdlib::io::input_from("", &mut r);
    assert_eq!(s, "hello\r");
}

// ----- Tier 1 — input on no-trailing-\n bytes ---------------------

#[test]
fn test_io_input_from_no_trailing_newline_returns_what_was_read() {
    require_impl("test_io_input_from_no_trailing_newline_returns_what_was_read");
    // ADR-0044 W2 Phase 2 convention: `read_until` returns whatever
    // bytes are available up to + including the delimiter (or EOF).
    // No-trailing-\n input is returned as-is (no strip happens — the
    // strip path only fires when the last byte is \n).
    let mut r = std::io::BufReader::new(std::io::Cursor::new(b"abc".to_vec()));
    let s = cobrust_stdlib::io::input_from("", &mut r);
    assert_eq!(s, "abc");
}

// ----- Tier 1 — repeated input drains reader -----------------------

#[test]
fn test_io_input_from_repeated_drains_lines() {
    require_impl("test_io_input_from_repeated_drains_lines");
    let mut r = std::io::BufReader::new(std::io::Cursor::new(b"one\ntwo\nthree\n".to_vec()));
    assert_eq!(cobrust_stdlib::io::input_from("", &mut r), "one");
    assert_eq!(cobrust_stdlib::io::input_from("", &mut r), "two");
    assert_eq!(cobrust_stdlib::io::input_from("", &mut r), "three");
}

#[test]
fn test_io_input_from_repeated_then_eof_returns_empty() {
    require_impl("test_io_input_from_repeated_then_eof_returns_empty");
    let mut r = std::io::BufReader::new(std::io::Cursor::new(b"only\n".to_vec()));
    assert_eq!(cobrust_stdlib::io::input_from("", &mut r), "only");
    assert_eq!(cobrust_stdlib::io::input_from("", &mut r), "");
}

#[test]
fn test_io_input_from_with_prompt_does_not_panic() {
    require_impl("test_io_input_from_with_prompt_does_not_panic");
    let mut r = std::io::BufReader::new(std::io::Cursor::new(b"value\n".to_vec()));
    let s = cobrust_stdlib::io::input_from(">> ", &mut r);
    assert_eq!(s, "value");
}

// ----- Tier 1 #12 — UTF-8 multi-byte round-trip --------------------

#[test]
fn test_io_input_from_utf8_multibyte_round_trip() {
    require_impl("test_io_input_from_utf8_multibyte_round_trip");
    let mut r = std::io::BufReader::new(std::io::Cursor::new("你好\n".as_bytes().to_vec()));
    let s = cobrust_stdlib::io::input_from("", &mut r);
    assert_eq!(s, "你好");
}

// ----- Tier 1 #13 — invalid UTF-8 lossy ----------------------------

#[test]
fn test_io_input_from_invalid_utf8_lossy_replacement() {
    require_impl("test_io_input_from_invalid_utf8_lossy_replacement");
    let mut r = std::io::BufReader::new(std::io::Cursor::new(vec![0xffu8, 0x0a]));
    let s = cobrust_stdlib::io::input_from("", &mut r);
    assert!(
        s.contains('\u{fffd}'),
        "expected U+FFFD replacement, got {s:?}"
    );
}

// ----- Tier 1 #6 — read_line preserves \n (W2 cap) -----------------

#[test]
fn test_io_read_line_w2_preserves_trailing_lf() {
    require_impl("test_io_read_line_w2_preserves_trailing_lf");
    let mut r = std::io::BufReader::new(std::io::Cursor::new(b"hello\n".to_vec()));
    let s = cobrust_stdlib::io::read_line_from(&mut r);
    assert_eq!(s, "hello\n");
}

// ----- Tier 1 #7 — read_line EOF → "" (W2 cap) ---------------------

#[test]
fn test_io_read_line_w2_eof_returns_empty_string() {
    require_impl("test_io_read_line_w2_eof_returns_empty_string");
    let mut r = std::io::BufReader::new(std::io::Cursor::new(Vec::<u8>::new()));
    let s = cobrust_stdlib::io::read_line_from(&mut r);
    assert_eq!(s, "");
}

// ----- Tier 1 — read_line drains lines -----------------------------

#[test]
fn test_io_read_line_w2_drains_lines() {
    require_impl("test_io_read_line_w2_drains_lines");
    let mut r = std::io::BufReader::new(std::io::Cursor::new(b"a\nb\nc\n".to_vec()));
    assert_eq!(cobrust_stdlib::io::read_line_from(&mut r), "a\n");
    assert_eq!(cobrust_stdlib::io::read_line_from(&mut r), "b\n");
    assert_eq!(cobrust_stdlib::io::read_line_from(&mut r), "c\n");
    assert_eq!(cobrust_stdlib::io::read_line_from(&mut r), "");
}

// ----- Tier 1 — input + read_line interleaved ---------------------

#[test]
fn test_io_input_and_read_line_interleaved() {
    require_impl("test_io_input_and_read_line_interleaved");
    let mut r = std::io::BufReader::new(std::io::Cursor::new(b"a\nb\nc\n".to_vec()));
    assert_eq!(cobrust_stdlib::io::input_from("", &mut r), "a");
    assert_eq!(cobrust_stdlib::io::read_line_from(&mut r), "b\n");
    assert_eq!(cobrust_stdlib::io::input_from("", &mut r), "c");
}

// =====================================================================
// C-ABI shim contract — ADR-0044 §"New runtime C-ABI surface"
// =====================================================================

#[test]
fn test_cabi_input_no_prompt_symbol_exists() {
    require_impl("test_cabi_input_no_prompt_symbol_exists");
    // Symbol existence is the contract — take an unsafe fn pointer.
    let _f: unsafe extern "C" fn() -> *mut u8 = cobrust_stdlib::io::__cobrust_input_no_prompt;
}

#[test]
fn test_cabi_input_with_prompt_symbol_exists() {
    require_impl("test_cabi_input_with_prompt_symbol_exists");
    let _f: unsafe extern "C" fn(*const u8, usize) -> *mut u8 = cobrust_stdlib::io::__cobrust_input;
}

#[test]
fn test_cabi_read_line_symbol_exists() {
    require_impl("test_cabi_read_line_symbol_exists");
    let _f: unsafe extern "C" fn() -> *mut u8 = cobrust_stdlib::io::__cobrust_read_line;
}

#[test]
fn test_cabi_argv_symbol_exists() {
    require_impl("test_cabi_argv_symbol_exists");
    let _f: unsafe extern "C" fn() -> *mut u8 = cobrust_stdlib::env::__cobrust_argv;
}

// =====================================================================
// argv() materialization
// =====================================================================

#[test]
fn test_argv_materialize_reads_captured_args() {
    require_impl("test_argv_materialize_reads_captured_args");
    // argv_list() falls back to std::env::args() when CAPTURED_ARGS is
    // not initialised; in the cargo-test runner argv[0] is always present.
    let v = cobrust_stdlib::env::argv_list();
    assert!(!v.is_empty(), "argv must have at least argv[0]");
}

// =====================================================================
// Behavioral edge cases
// =====================================================================

#[test]
fn test_io_input_from_large_4kib_input() {
    require_impl("test_io_input_from_large_4kib_input");
    let payload: Vec<u8> = std::iter::repeat(b'a')
        .take(4096)
        .chain(std::iter::once(b'\n'))
        .collect();
    let mut r = std::io::BufReader::new(std::io::Cursor::new(payload));
    let s = cobrust_stdlib::io::input_from("", &mut r);
    assert_eq!(s.len(), 4096);
}

#[test]
fn test_io_input_from_empty_and_nonempty_prompt() {
    require_impl("test_io_input_from_empty_and_nonempty_prompt");
    let mut r1 = std::io::BufReader::new(std::io::Cursor::new(b"a\n".to_vec()));
    assert_eq!(cobrust_stdlib::io::input_from("", &mut r1), "a");
    let mut r2 = std::io::BufReader::new(std::io::Cursor::new(b"b\n".to_vec()));
    assert_eq!(cobrust_stdlib::io::input_from("> ", &mut r2), "b");
}

// =====================================================================
// Existing-surface regression guards (sanity: existing read_line() shape)
// =====================================================================

#[test]
fn test_existing_read_line_returns_result_today() {
    // Sanity: today's `read_line() -> Result<String, Error>` is unaffected.
    // After ADR-0044 W2 cap impl, the EXISTING `read_line()` still returns
    // Result (the W2 cap adds a sibling helper, not a replacement — per
    // ADR-0044a future plan, the migration happens later).
    //
    // This test does NOT call read_line() at runtime (would block on stdin
    // in `cargo test`). We only verify the type by taking a fn pointer.
    let _f: fn() -> Result<String, cobrust_stdlib::Error> = cobrust_stdlib::io::read_line;
}
