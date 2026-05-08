//! `cobrust repl` — M14 stub.
//!
//! ADR-0019 §"M14 — REPL" pinned the REPL as a separate milestone with
//! its own done-means (cold start < 200ms, multi-line input, golden
//! sessions). M10 ships a stub that prints a deferred-to-M14 message
//! and exits with `USER_ERROR`.

use crate::exit_codes;

/// Run the (stub) REPL.
#[must_use]
pub fn run() -> u8 {
    eprintln!("REPL is M14 scope; not yet implemented (see ADR-0019).");
    exit_codes::USER_ERROR
}
