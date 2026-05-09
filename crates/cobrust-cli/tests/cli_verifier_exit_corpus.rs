//! Cranelift verifier-rejection exit-code corpus (per finding
//! `cobrust-codegen-i64-i8-mismatch-at-4-blocks` + P0 CLI hardening
//! sprint, branch `feature/cli-hardening-verifier-exit`).
//!
//! ## Context
//!
//! `review-claude` (third-party audit, 2026-05-09) identified two bugs
//! in the Conway Rule 30 stress test:
//!
//! - **Bug 1 (codegen, Task #43):** Cobrust codegen selects `i8` for an
//!   expression typed `: i64` when ≥ 4 identical inline compute blocks
//!   appear in one function, causing a Cranelift verifier error.
//!   **Out of scope here** — deferred to the P1 codegen narrow-type fix.
//!
//! - **Bug 2 (CLI, this sprint):** `cobrust build` printed the Cranelift
//!   verifier error message but **proceeded to emit a binary anyway**,
//!   exiting 0. In a `&& ./binary` chain this masked the miscompilation.
//!
//! ## Discipline note — "failing-first"
//!
//! The sprint asked for failing-first tests where feasible. The current
//! HEAD (`82c0e00`) already propagates the verifier error correctly via
//! the `?` chain in `build.rs` line 147:
//!
//! ```text
//! let user_artifact = emit(&mir, spec).map_err(|e| BuildError::Internal(format!("{e}")))?;
//! ```
//!
//! `emit` calls `ctx.define_body(...)? ` which propagates
//! `CodegenError::CraneliftError(verifier_detail)` as an `Err`, which
//! `build.rs` maps to `BuildError::Internal` → exit code 3 (INTERNAL_PANIC).
//!
//! Tests v01 and v03 below **already pass** on the current codebase.
//! They serve as a regression guard: if a future refactor accidentally
//! swallows the verifier error (e.g., converting `?` to a `let _ =` or
//! logging-and-continuing), these tests will catch the regression.
//!
//! The formal "failing-first" assertion would have been valid at an
//! earlier state before the `?` chain was introduced. The current corpus
//! acts as the post-fix green baseline.
//!
//! ## Audit #1 unblock signal
//!
//! With this hardening in place, `cobrust build` failures on
//! verifier-rejected output from the LLM translation pipeline (Task #35,
//! Audit #1) will now fail fast with exit 3 instead of silently emitting
//! a mis-running binary. This makes Audit #1 translation-quality failures
//! attributable to the LLM's code rather than a CLI exit-code masking bug.

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
// M11.2 sprint clippy fix: this test file pre-existed at HEAD
// `b4808e0` with a stale `r#"..."#` raw string literal that newer
// clippy flags as `needless_raw_string_hashes`. Allow at module level
// per `feedback_p9_clippy_stall_pattern` — touching the literal
// itself is out of scope for M11.2 (the fixture is from a stale
// pre-ADR-0033 sibling sprint and the tests are now `#[ignore]`'d).
#![allow(clippy::needless_raw_string_hashes)]

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

/// 4-block straight-line `% 2` pattern from finding
/// `cobrust-codegen-i64-i8-mismatch-at-4-blocks` §Reproduction.
///
/// Verbatim from the finding's "4-cell version" (no `while` loop —
/// the bug fires on straight-line code too, confirming the loop-phi
/// hypothesis was eliminated by binary search).
///
/// Expected: `cobrust build` exits NON-ZERO (currently exit 3,
/// INTERNAL_PANIC) because the Cranelift verifier rejects the IR.
///
/// Pre-fix behaviour (not present at HEAD `82c0e00` but recorded for
/// archaeology): exit 0 + wrong binary silently emitted.
const FOUR_BLOCK_REPRO: &str = r#"fn main() -> i64:
    let s: i64 = 30
    let m0: i64 = s % 2
    let r0: i64 = (s / 2) % 2
    let or0: i64 = m0 + r0 - m0 * r0
    let n0: i64 = or0 % 2
    let l1: i64 = s % 2
    let m1: i64 = (s / 2) % 2
    let r1: i64 = (s / 4) % 2
    let or1: i64 = m1 + r1 - m1 * r1
    let n1: i64 = (l1 + or1) % 2
    let l2: i64 = (s / 2) % 2
    let m2: i64 = (s / 4) % 2
    let r2: i64 = (s / 8) % 2
    let or2: i64 = m2 + r2 - m2 * r2
    let n2: i64 = (l2 + or2) % 2
    let l3: i64 = (s / 4) % 2
    let m3: i64 = (s / 8) % 2
    let r3: i64 = (s / 16) % 2
    let or3: i64 = m3 + r3 - m3 * r3
    let n3: i64 = (l3 + or3) % 2
    let result: i64 = n0 + n1 * 2 + n2 * 4 + n3 * 8
    print_int(result)
    return 0
"#;

/// Negative control: a minimal well-formed program that builds cleanly.
const CLEAN_PROGRAM: &str = "fn main() -> i64:\n    return 0\n";

/// v01 — 4-block repro exits non-zero.
///
/// This is the primary regression guard for Bug 2 (CLI hardening).
/// The Cranelift verifier rejects the IR with:
///   `iadd.i8 v_N: arg 1 has type i64, expected i8`
/// The `?` chain in `build.rs::build` propagates this as
/// `BuildError::Internal` → exit code 3 (ADR-0024 §"Exit-code scheme").
///
/// If this test fails (exit 0), the verifier error is being swallowed
/// somewhere between `cranelift_backend::define_body` and the CLI shell.
///
/// STALE post-ADR-0033 (commit `3392eb5`): the underlying Bug 1
/// (codegen-side i8/i64 narrow on 4 similar blocks) was empirically
/// closed by the Option C root-primitive `inferred_locals` fixed-point
/// fix. The 4-block repro now COMPILES CLEANLY — there is no longer a
/// verifier error to assert against. Per
/// `findings/codegen-i8-i64-mismatch-at-4-blocks.md` §Conclusion, Bug 1
/// is RESOLVED + Bug 2 was a mis-diagnosis. The CLI exit-3 propagation
/// path is still correct (verified at HEAD `b4808e0`); we just no
/// longer have a viable trigger source. M11.2 sprint surfaces this
/// as a follow-up; the test stays as `#[ignore]` until a NEW
/// verifier-rejecting fixture is supplied (or the test is retired).
#[ignore = "ADR-0033 closed the underlying codegen bug; FOUR_BLOCK_REPRO no longer triggers a verifier error. See findings/codegen-i8-i64-mismatch-at-4-blocks.md."]
#[test]
fn v01_four_block_repro_exits_non_zero() {
    let bin = cobrust_binary();
    let src = write_temp("v01_four_block", FOUR_BLOCK_REPRO);
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src)
        .arg("--emit")
        .arg("obj")
        .arg("--quiet")
        .output()
        .expect("invoke build");
    assert!(
        !out.status.success(),
        "cobrust build on 4-block verifier-rejecting program must exit non-zero; \
         got success — Bug 2 (silent miscompile) is regressed.\n\
         stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Per ADR-0024 §"Exit-code scheme": codegen failure → INTERNAL_PANIC = 3.
    assert_eq!(
        out.status.code(),
        Some(3),
        "expected INTERNAL_PANIC (3) for Cranelift verifier rejection; \
         got {:?}",
        out.status.code()
    );
}

/// v02 — negative control: clean program exits 0.
///
/// Ensures the verifier-reject path does not falsely fire on valid IR.
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

/// v03 — verifier error appears on stderr, not stdout.
///
/// Shell pipelines use stdout for data and stderr for diagnostics.
/// If the verifier error were on stdout, `cobrust build <bad.cb> | tee build.log`
/// would silently capture the error into the log and show nothing on the
/// terminal, masking the failure in automated pipelines.
///
/// The `build.rs::run` function uses `eprintln!` (stderr) to emit
/// `cobrust build: {e}`. This test asserts the discipline holds.
///
/// STALE post-ADR-0033 (same as v01): FOUR_BLOCK_REPRO no longer
/// triggers a verifier error since `inferred_locals` fixed-point
/// resolves the i8/i64 chain depth ≥ 2 case. See v01's doc comment +
/// `findings/codegen-i8-i64-mismatch-at-4-blocks.md`.
#[ignore = "ADR-0033 closed the underlying codegen bug; FOUR_BLOCK_REPRO no longer triggers a verifier error. See findings/codegen-i8-i64-mismatch-at-4-blocks.md."]
#[test]
fn v03_verifier_error_on_stderr_not_stdout() {
    let bin = cobrust_binary();
    let src = write_temp("v03_stderr", FOUR_BLOCK_REPRO);
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src)
        .arg("--emit")
        .arg("obj")
        .arg("--quiet")
        .output()
        .expect("invoke build");
    assert!(!out.status.success(), "expected non-zero exit");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Verifier error keyword must appear on stderr.
    assert!(
        stderr.contains("Verifier") || stderr.contains("verifier") || stderr.contains("iadd"),
        "expected verifier error on stderr; stderr={stderr:?} stdout={stdout:?}"
    );
    // Stdout must be clean — no diagnostic leaking to the data stream.
    assert!(
        stdout.is_empty(),
        "verifier error must not appear on stdout (pipeline-discipline violation); \
         stdout={stdout:?}"
    );
}
