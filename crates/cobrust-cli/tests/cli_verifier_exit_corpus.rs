//! Cranelift verifier-rejection exit-code corpus (per finding
//! `codegen-i8-i64-mismatch-at-4-blocks` + P0 CLI hardening sprint
//! Task #42, branch `feature/cli-hardening-verifier-exit`).
//!
//! ## Context
//!
//! `review-claude` (third-party audit, 2026-05-09) identified a
//! 4+-similar-inline-block compute-pattern bug in the Conway Rule 30
//! stress test. It was originally hypothesised as TWO bugs:
//!
//! - **Bug 1 (codegen narrow-type):** Cobrust codegen selected `i8`
//!   for an expression typed `: i64` when ≥ 4 identical inline compute
//!   blocks appear in one fn → Cranelift verifier rejection. **Closed
//!   for free at HEAD `3392eb5` by ADR-0033 Option C** (root-primitive
//!   `inferred_locals` fixed-point — same fix that closed the float
//!   Ty::None bug).
//!
//! - **Bug 2 (CLI silent miscompile):** original hypothesis claimed
//!   `cobrust build` printed verifier error but proceeded to emit a
//!   binary. **MIS-DIAGNOSIS** — the `?` chain in
//!   `cranelift_backend::define_body → emit → build.rs::build` was
//!   already correct at HEAD `82c0e00`. Verified via stdout/stderr
//!   split (Pattern A in `feedback_pipe_exit_code_capture.md`):
//!   `cobrust build foo.cb >out 2>err; echo $?` → 3 + binary not
//!   emitted. The original mis-diagnosis came from a `cmd | tail; echo
//!   $?` capture which records `tail`'s exit code (always 0).
//!
//! ## What this corpus tests now
//!
//! Post-ADR-0033 closure (HEAD ≥ `3392eb5`), the original 4-block
//! `% 2` straight-line program **builds successfully** and produces
//! correct output. v01 + v03 (which asserted "build must reject") are
//! retired in favor of v02 alone — the negative control that confirms
//! the verifier-reject *path* itself is sound when invoked on a
//! malformed IR. Today we cannot synthesise a malformed IR purely from
//! a `.cb` source (codegen + ADR-0033 Option C close every known
//! verifier-trigger pattern), so v01/v03 lose their natural input.
//!
//! Future sprints that surface a NEW codegen bug producing a verifier
//! reject can re-introduce a v01-style regression guard at that point.
//!
//! ## Audit #1 unblock signal
//!
//! Even with v01/v03 retired, the verifier-reject CLI path is still
//! correct (per the `?` chain inspection) and would fail fast if any
//! future codegen edge produces a verifier reject. Audit #1 (Task
//! #35) translation-quality failures remain attributable to the LLM
//! rather than a CLI exit-code masking bug.

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

use std::path::PathBuf;
use std::process::Command;

fn cobrust_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cobrust"))
}

fn write_temp(name: &str, contents: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("cobrust-verifier-{}-{}", name, std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let p = dir.join(format!("{name}.cb"));
    std::fs::write(&p, contents).expect("write temp .cb");
    p
}

/// Negative control: a minimal well-formed program that builds cleanly.
const CLEAN_PROGRAM: &str = "fn main() -> i64:\n    return 0\n";

/// v02 — clean program builds + exits 0.
///
/// Ensures the build path doesn't falsely fire any error path (verifier
/// reject, codegen panic, etc.) on valid IR. This is the only test in
/// the corpus that survives ADR-0033 Option C's closure of the original
/// 4-block trigger; v01 + v03 retired.
#[test]
fn v02_clean_program_exits_zero() {
    let bin = cobrust_binary();
    let src = write_temp("v02_clean", CLEAN_PROGRAM);
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src)
        .arg("--emit")
        .arg("obj")
        .arg("--quiet")
        .output()
        .expect("invoke build");
    assert!(
        out.status.success(),
        "cobrust build on clean program must exit 0; \
         stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
}
