//! LC-100 Tier A Sprint 2 TEST — end-to-end repro corpus for the
//! Pattern A `.rodata` literal misalignment defect.
//!
//! Spec source: `docs/agent/findings/lc100-pattern-a-rodata-literal-misalignment.md`
//!
//! ## Today (HEAD ~9f8ebc4, Sprint 2 TEST authoring)
//!
//! Tests 1, 2, 4, and 5 are EXPECTED TO FAIL with the same runtime panic:
//!
//! ```text
//! thread '<unnamed>' panicked at crates/cobrust-stdlib/src/fmt.rs:194:22:
//! misaligned pointer dereference: address must be a multiple of 0x8 but is 0x...
//! thread caused non-unwinding panic. aborting.
//! ```
//!
//! Test 3 (`print_no_nl_from_stdin`) is EXPECTED TO PASS today and continue
//! passing post-DEV — it exercises the runtime-str path that uses a properly
//! 8-byte-aligned StringBuffer, NOT a `.rodata` pointer.
//!
//! ## After Sprint 2 DEV ships
//!
//! All 5 tests pass. Test 4 specifically validates that one of the 8
//! affected LC-100 programs (LC-056 level-order-traversal) flips from
//! RUNTIME-FAIL to GREEN once the codegen routes `print_no_nl(Constant::Str)`
//! to the new `__cobrust_print_no_nl_lit(ptr, len)` shim.
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09: module-level
//! 18-lint test-only allow header at the top.

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
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::len_zero)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::unnecessary_debug_formatting)]
#![allow(clippy::needless_raw_string_hashes)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// =====================================================================
// Shared helpers — modeled on crates/cobrust-cli/tests/lc100_stress_e2e_b1.rs
// =====================================================================

fn cobrust_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cobrust"))
}

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

fn pattern_a_fixture(name: &str) -> PathBuf {
    workspace_root()
        .join("examples/lc100_pattern_a_fixtures")
        .join(name)
}

fn stress_dir() -> PathBuf {
    workspace_root().join("examples/leetcode-stress")
}

fn stress_src(slug: &str) -> PathBuf {
    stress_dir().join(slug).join("solution.cb")
}

fn build_seq() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Build a `.cb` source to an exe under a unique temp dir.
/// Returns `(exe_path_or_empty, build_stderr)`. Empty exe path = build failed.
fn build_cb(src: &Path, tag: &str) -> (PathBuf, String) {
    assert!(src.exists(), "fixture .cb not found at {:?}", src);
    let bin = cobrust_binary();
    let exe_dir = std::env::temp_dir().join(format!(
        "cobrust-lc100-patternA-{}-{}-{}",
        tag,
        std::process::id(),
        build_seq()
    ));
    let _ = std::fs::create_dir_all(&exe_dir);
    let exe = exe_dir.join("solution");
    let out = Command::new(&bin)
        .arg("build")
        .arg(src)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .current_dir(workspace_root())
        .output()
        .expect("invoke cobrust build");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        return (PathBuf::new(), stderr);
    }
    (exe, stderr)
}

/// Run a built exe with stdin and return `(exit_code, stdout, stderr)`.
fn run_exe(exe: &Path, stdin_bytes: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn fixture exe");
    {
        let stdin = child.stdin.as_mut().expect("stdin handle");
        let _ = stdin.write_all(stdin_bytes);
    }
    let out = child.wait_with_output().expect("wait_with_output");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Build + run with required exit-zero + assertable stdout. Panics with
/// the failing details on either build failure or non-zero exit.
fn build_and_run_ok(src: &Path, tag: &str, stdin_bytes: &[u8]) -> String {
    let (exe, build_stderr) = build_cb(src, tag);
    assert!(
        !exe.as_os_str().is_empty(),
        "cobrust build failed for '{}'; stderr=\n{}",
        tag,
        build_stderr
    );
    let (code, stdout, run_stderr) = run_exe(&exe, stdin_bytes);
    assert_eq!(
        code, 0,
        "exe '{}' exited with code {}; stderr=\n{}",
        tag, code, run_stderr
    );
    stdout
}

// =====================================================================
// Test 1 — minimal print_no_nl("hello") + print("") outputs "hello\n"
// =====================================================================
//
// Today: FAILS — build succeeds, exe panics at fmt.rs:194 with misaligned
// pointer dereference on the "hello" literal's .rodata pointer. Exit code
// will be non-zero (SIGABRT from non-unwinding panic) → assert_eq!(code, 0)
// fires.
//
// Post-Sprint-2-DEV: PASSES — codegen routes the Constant::Str arg through
// the new __cobrust_print_no_nl_lit(ptr, len) shim, no StringBuffer cast,
// no alignment fault. Stdout = "hello\n".

#[test]
fn test_pattern_a_minimal_literal_outputs_hello() {
    let src = pattern_a_fixture("minimal_print_no_nl_literal.cb");
    let stdout = build_and_run_ok(&src, "minimal-literal", b"");
    assert_eq!(
        stdout, "hello\n",
        "expected 'hello\\n' from print_no_nl(\"hello\") + print(\"\")"
    );
}

// =====================================================================
// Test 2 — two consecutive print_no_nl("hi") calls concatenate to "hihi\n"
// =====================================================================
//
// Today: FAILS — crashes on the first print_no_nl("hi") call (and even if
// codegen happens to alignment-round the first literal, the second call
// still fires).
//
// Post-Sprint-2-DEV: PASSES — both calls route through the _lit shim;
// stdout = "hihi\n".

#[test]
fn test_pattern_a_two_consecutive_literals_concatenate() {
    let src = pattern_a_fixture("two_consecutive_literals.cb");
    let stdout = build_and_run_ok(&src, "two-consecutive", b"");
    assert_eq!(
        stdout, "hihi\n",
        "expected 'hihi\\n' from two print_no_nl(\"hi\") calls"
    );
}

// =====================================================================
// Test 3 — print_no_nl(input("")) on a runtime str — REGRESSION GUARD
// =====================================================================
//
// Today: PASSES — `input("")` returns a heap-allocated 8-byte-aligned
// StringBuffer pointer, so the existing __cobrust_print_no_nl(buf) shim's
// `buf.cast::<StringBuffer>()` is safe.
//
// Post-Sprint-2-DEV: MUST CONTINUE TO PASS. The DEV agent must NOT
// remove the runtime-str code path when adding the _lit variant — both
// paths coexist, and intrinsic-rewrite chooses between them based on the
// argument's IR shape (Constant::Str vs. anything else).

#[test]
fn test_pattern_a_runtime_str_path_still_works_regression() {
    let src = pattern_a_fixture("print_no_nl_from_stdin.cb");
    let stdout = build_and_run_ok(&src, "from-stdin", b"runtime-string\n");
    assert_eq!(
        stdout, "runtime-string\n",
        "expected the stdin line echoed back via print_no_nl + print(\"\")"
    );
}

// =====================================================================
// Test 4 — LC-056 level-order-traversal flips from RUNTIME-FAIL to GREEN
// =====================================================================
//
// Today: FAILS — LC-056 is one of the 8 affected programs (per finding
// lc100-pattern-a-rodata-literal-misalignment.md). It currently has an
// `#[ignore]` annotation in `lc100_stress_e2e_b2.rs::test_lc056_level_order_traversal`
// for exactly this reason. This test deliberately re-runs it WITHOUT
// `#[ignore]` so the failing corpus surfaces in `cargo test` output today.
//
// Post-Sprint-2-DEV: PASSES — `print_no_nl(" ")` / `print_no_nl("0")` /
// etc. all route through the new _lit shim. The BFS algorithm itself is
// correct (verified by sibling test LC-060 which uses BFS without
// print_no_nl literals), so once Pattern A is fixed, LC-056 emits its
// level-order output verbatim.
//
// After this test goes green, Sprint 2 DEV must additionally un-ignore
// the corresponding `lc100_stress_e2e_b{2,3,4}.rs` tests for LC-056, 069,
// 072, 090, 093, 099, 100 (and LC-024 modulo Pattern B — see finding
// lc100-pattern-b-list-of-str-gap.md).

#[test]
fn test_pattern_a_lc056_level_order_traversal_unignored() {
    // Reuse the LC-056 fixture from examples/leetcode-stress/056-... — do
    // NOT modify the bucket B2 test file (Sprint 2 DEV un-ignores the
    // entry there as part of the impl PR).
    let src = stress_src("056-level-order-traversal");
    let stdout = build_and_run_ok(
        &src,
        "lc056-level-order",
        b"7\n1 1 2\n2 3 4\n3 5 6\n4 -1 -1\n5 -1 -1\n6 -1 -1\n7 -1 -1\n",
    );
    assert_eq!(
        stdout, "1\n2 3\n4 5 6 7\n",
        "LC-056 7-node complete binary tree level-order"
    );

    // Also exercise the 1-node base case from test.toml — same fixture,
    // different stdin, in the same test fn to keep failure count tidy:
    let stdout1 = build_and_run_ok(&src, "lc056-base", b"1\n5 -1 -1\n");
    assert_eq!(stdout1, "5\n", "LC-056 1-node tree level-order");
}

// =====================================================================
// Test 5 — mixed literal + runtime str in one program
// =====================================================================
//
// Today: FAILS — crashes on the print_no_nl("prefix=") call before the
// runtime-str print_no_nl gets a chance to run.
//
// Post-Sprint-2-DEV: PASSES — DEV must distinguish between literal and
// runtime-str arguments at the intrinsic-rewrite site (intrinsics.rs
// `Kind::PrintNoNl` arm). Only the literal arg gets rewritten to the _lit
// shim; the runtime-str arg continues to use the original shim. Within a
// single program, both code paths must work.
//
// This is the most semantically interesting test in the corpus — it
// proves the intrinsic-rewrite dispatch logic is operand-aware, not a
// blanket symbol swap.

#[test]
fn test_pattern_a_mixed_literal_and_runtime_in_one_program() {
    let src = pattern_a_fixture("mixed_literal_and_runtime.cb");
    let stdout = build_and_run_ok(&src, "mixed", b"world\n");
    assert_eq!(
        stdout, "prefix=world\n",
        "expected 'prefix=' (literal) + 'world' (runtime str from stdin) + '\\n'"
    );
}

// =====================================================================
// Test 6 — non-ASCII UTF-8 literal in print_no_nl (regression guard
//          for the _lit shim's byte/char-boundary safety)
// =====================================================================
//
// Today: FAILS — even multi-byte UTF-8 literals hit the same .rodata
// alignment defect. The 9-byte "日本語" lowers to a 9-byte .rodata payload,
// pointer alignment 1. (Empirical observation: on this platform the
// silent-fail mode emits only the trailing newline from `print("")`,
// because the misalignment cast at fmt.rs:194 produces undefined output
// from the bogus StringBuffer.len read before — sometimes — the abort
// fires. The assertion catches both modes.)
//
// Post-Sprint-2-DEV: PASSES — the _lit shim writes raw bytes (already
// validated UTF-8 by the compiler, no re-validation needed in the shim);
// stdout = "日本語\n".

#[test]
fn test_pattern_a_non_ascii_literal_writes_cleanly() {
    let src = pattern_a_fixture("non_ascii_literal.cb");
    let stdout = build_and_run_ok(&src, "nonascii", b"");
    assert_eq!(
        stdout, "日本語\n",
        "expected non-ASCII literal to print verbatim"
    );
}

// =====================================================================
// Test 7 — many short literals in sequence (alignment-roulette stress)
// =====================================================================
//
// Today: FAILS — each literal is an independent .rodata pointer, and at
// least one of 10 will land mis-aligned even with linker placement luck.
// The first failure aborts the process or emits partial/empty output;
// the strict assert_eq! catches both modes.
//
// Post-Sprint-2-DEV: PASSES — _lit shim is byte-pointer-safe; 10
// consecutive calls compose cleanly.
//
// This test guards against a partial-fix where the codegen only allocates
// alignment-padding for one or two literals at a time but breaks at scale.

#[test]
fn test_pattern_a_many_short_literals_alignment_roulette() {
    let src = pattern_a_fixture("many_short_literals.cb");
    let stdout = build_and_run_ok(&src, "many", b"");
    assert_eq!(
        stdout, "abcdefghij\n",
        "expected 10 single-char literals concatenated"
    );
}
