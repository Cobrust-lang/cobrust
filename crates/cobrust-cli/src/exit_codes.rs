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
pub const INTERNAL_PANIC: u8 = 3;
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
