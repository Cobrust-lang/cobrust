//! Closed-set exit codes for the `cobrust` driver (ADR-0024 §"Exit-code scheme").
//!
//! Every subcommand exits with one of the codes below. Shell scripts may
//! dispatch on the failure category; the codes are stable across releases.

#![allow(clippy::missing_docs_in_private_items)]

/// `0` — success.
pub const SUCCESS: u8 = 0;
/// `1` — user error: bad CLI usage, missing input file, malformed flag.
pub const USER_ERROR: u8 = 1;
/// `2` — type-check error: lex / parse / HIR-lower / type-check failure.
pub const TYPE_ERROR: u8 = 2;
/// `3` — internal panic: codegen / linker / unexpected error.
///
/// Also covers Cranelift verifier rejection: `cobrust build` on a
/// program whose generated IR fails the Cranelift verifier exits 3.
/// The verifier error detail is written to stderr; stdout is empty.
/// See finding `cobrust-codegen-i64-i8-mismatch-at-4-blocks` (Bug 2)
/// and ADR-0024 §"Exit code 3 — Cranelift verifier rejection".
///
/// `VERIFIER_REJECTED` is an alias for documentation clarity; the
/// numeric value (3) is intentionally the same as `INTERNAL_PANIC`
/// because a Cranelift verifier rejection is an internal codegen failure.
pub const INTERNAL_PANIC: u8 = 3;
/// Alias for `INTERNAL_PANIC` specific to Cranelift verifier rejection.
/// Value = 3. Prefer this name when documenting verifier-reject paths.
#[allow(dead_code)]
pub const VERIFIER_REJECTED: u8 = INTERNAL_PANIC;
/// `4` — runtime panic propagated from the invoked program (`cobrust run`).
pub const RUNTIME_PANIC: u8 = 4;
/// `5` — format diff under `--check` (`cobrust fmt --check`).
pub const FMT_DIFF: u8 = 5;
/// `6` — test failures (`cobrust test`).
pub const TEST_FAILURE: u8 = 6;
/// `100` — translator-pipeline base. `cobrust translate` failures use
/// `100..127`; the exact code carries the L0..L3 stage that failed.
pub const TRANSLATOR_BASE: u8 = 100;
/// `127` — translator-pipeline maximum (inclusive).
#[allow(dead_code)]
pub const TRANSLATOR_MAX: u8 = 127;
