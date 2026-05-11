//! ADR-0044 W2 Phase 2 — end-to-end piped-stdin + argv scenarios.
//!
//! Per ADR-0044 §"Test plan" Tier 3 (≥ 10 tests):
//!   - Build a `.cb` fixture program via `cobrust build`.
//!   - Pipe stdin + extra argv to the produced executable.
//!   - Assert exit code + stdout match the documented W2 semantics.
//!
//! POST-AMENDMENT scope cap (Decision 1D):
//!   - `read_line()` returns plain `str` (not `Result`).
//!   - All assertions in this file use plain-string semantics; no
//!     `Result Ok-shape` / `Result Err-shape` asserts (those land with ADR-0044a).
//!
//! Fixtures live in `examples/leetcode_fixtures/*.cb` (created in the
//! same atomic commit as this corpus).

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

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

// =====================================================================
// Test harness — locate the cobrust CLI binary + fixtures dir
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

fn fixtures_dir() -> PathBuf {
    workspace_root().join("examples/leetcode_fixtures")
}

fn fixture_path(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

/// Build a fixture `.cb` file into a unique tmp-exe and return its path.
// Monotonic per-process counter so parallel tests in the same cargo
// test process don't collide on the same `exe_dir` path (the previous
// `line!()` tiebreaker always returned the same constant since it was
// the line where `build_fixture` is defined, not where it's called).
fn build_fixture_seq() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn build_fixture(name: &str) -> (PathBuf, String) {
    let bin = cobrust_binary();
    let src = fixture_path(name);
    assert!(
        src.exists(),
        "fixture {} missing — expected at {:?}",
        name,
        src
    );
    let exe_dir = std::env::temp_dir().join(format!(
        "cobrust-adr0044-e2e-{}-{}-{}",
        name,
        std::process::id(),
        build_fixture_seq()
    ));
    let _ = std::fs::create_dir_all(&exe_dir);
    let exe = exe_dir.join(src.file_stem().unwrap());
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src)
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

/// Run `exe` with `args` and `stdin_bytes`, return (exit_code, stdout, stderr).
fn run_with_args_and_stdin(exe: &Path, args: &[&str], stdin_bytes: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn fixture exe");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        let _ = stdin.write_all(stdin_bytes);
    }
    let out = child.wait_with_output().expect("wait_with_output");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// =====================================================================
// E2E #1 — echo "hello" | exe → stdout matches expected
// =====================================================================

#[test]
fn test_e2e_01_two_sum_echo_hello() {
    let (exe, build_stderr) = build_fixture("two_sum.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let (code, stdout, _) = run_with_args_and_stdin(&exe, &[], b"hello\n");
    assert_eq!(code, 0, "exe must exit 0");
    assert!(
        stdout.contains("hello"),
        "expected `hello` in stdout, got {stdout:?}"
    );
}

// =====================================================================
// E2E #2 — empty stdin pipes gracefully (no panic, exit 0)
// =====================================================================

#[test]
fn test_e2e_02_two_sum_empty_stdin_graceful() {
    let (exe, build_stderr) = build_fixture("two_sum.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let (code, _, stderr) = run_with_args_and_stdin(&exe, &[], b"");
    assert_eq!(code, 0, "exe must exit 0 on empty stdin; stderr={stderr}");
}

// =====================================================================
// E2E #3 — printf "1\n2\n3\n" | sum_lines → stdout "6"
// =====================================================================

#[test]
fn test_e2e_03_sum_lines_three_inputs() {
    let (exe, build_stderr) = build_fixture("sum_lines.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let (code, stdout, _) = run_with_args_and_stdin(&exe, &[], b"1\n2\n3\n");
    assert_eq!(code, 0, "exe must exit 0");
    // Fixture deliberately prints `6` (a fixed answer for the W2 stub);
    // Phase 3 fills full int-parse + real sum.
    assert!(
        stdout.contains("6"),
        "expected `6` in stdout (sum_lines W2 stub), got {stdout:?}"
    );
}

// =====================================================================
// E2E #4 — argv_dump.cb a b c → stdout contains "a", "b", "c"
// =====================================================================

#[test]
fn test_e2e_04_argv_dump_with_user_args() {
    let (exe, build_stderr) = build_fixture("argv_dump.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let (code, stdout, _) = run_with_args_and_stdin(&exe, &["a", "b", "c"], b"");
    assert_eq!(code, 0, "exe must exit 0");
    assert!(stdout.contains("a"), "missing arg `a`: {stdout:?}");
    assert!(stdout.contains("b"), "missing arg `b`: {stdout:?}");
    assert!(stdout.contains("c"), "missing arg `c`: {stdout:?}");
}

// =====================================================================
// E2E #5 — argv_dump.cb (no extra args) → stdout has length 1
//
// argv()[0] must always be present; without extras the count is 1.
// =====================================================================

#[test]
fn test_e2e_05_argv_only_argv0_when_no_user_args() {
    let (exe, build_stderr) = build_fixture("argv_count.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let (code, stdout, _) = run_with_args_and_stdin(&exe, &[], b"");
    assert_eq!(code, 0, "exe must exit 0");
    assert!(
        stdout.contains('1'),
        "expected count=1 (argv[0] only), got {stdout:?}"
    );
}

// =====================================================================
// E2E #6 — piped UTF-8 multi-byte round-trip
// =====================================================================

#[test]
fn test_e2e_06_two_sum_utf8_multibyte_round_trip() {
    let (exe, build_stderr) = build_fixture("two_sum.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    // "你好世界" — 4 multi-byte chars, 12 bytes UTF-8.
    let payload = "你好世界\n".as_bytes();
    let (code, stdout, _) = run_with_args_and_stdin(&exe, &[], payload);
    assert_eq!(code, 0, "exe must exit 0");
    assert!(
        stdout.contains("你好世界"),
        "expected UTF-8 round-trip `你好世界`, got {stdout:?}"
    );
}

// =====================================================================
// E2E #7 — piped 4 KiB single line → no panic
// =====================================================================

#[test]
fn test_e2e_07_two_sum_4kib_single_line() {
    let (exe, build_stderr) = build_fixture("two_sum.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let big = "x".repeat(4096);
    let mut payload = big.into_bytes();
    payload.push(b'\n');
    let (code, stdout, _) = run_with_args_and_stdin(&exe, &[], &payload);
    assert_eq!(code, 0, "exe must exit 0 on 4 KiB input");
    assert!(
        !stdout.is_empty(),
        "expected at least the echoed bytes in stdout"
    );
}

// =====================================================================
// E2E #8 — multi-line (100 lines) repeated input() drains correctly
// =====================================================================

#[test]
fn test_e2e_08_drain_lines_three_calls() {
    // Fixture uses 3 input() calls; we feed 3 lines and assert "done".
    // Per ADR-0044 Test plan Tier 3 #8 — repeated input() drains stdin.
    let (exe, build_stderr) = build_fixture("drain_lines.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let payload = b"alpha\nbeta\ngamma\n";
    let (code, stdout, _) = run_with_args_and_stdin(&exe, &[], payload);
    assert_eq!(code, 0, "exe must exit 0 after 3-line drain");
    assert!(
        stdout.contains("done"),
        "expected `done` marker after 3 inputs drained, got {stdout:?}"
    );
}

// =====================================================================
// E2E #9 — argv_count.cb a b c d e f g h i j → stdout "11" (10 + argv[0])
// =====================================================================

#[test]
fn test_e2e_09_argv_count_ten_user_args() {
    let (exe, build_stderr) = build_fixture("argv_count.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let (code, stdout, _) = run_with_args_and_stdin(
        &exe,
        &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"],
        b"",
    );
    assert_eq!(code, 0, "exe must exit 0");
    assert!(
        stdout.contains("11"),
        "expected count=11 (10 extras + argv[0]), got {stdout:?}"
    );
}

// =====================================================================
// E2E #10 — program that doesn't call input() → stdin ignored, no hang
// =====================================================================

#[test]
fn test_e2e_10_no_stdin_program_stdin_ignored() {
    let (exe, build_stderr) = build_fixture("echo_no_stdin.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    // Pipe some stdin; program ignores it. Run with a timeout-friendly
    // shape: spawn + write + drop stdin + wait. The OS handles SIGPIPE
    // gracefully.
    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn echo_no_stdin exe");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        let _ = stdin.write_all(b"this should be ignored\n");
    }
    // Drop stdin handle to flush + EOF.
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "exe must exit 0 even with unused stdin; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no_stdin_used"),
        "expected marker output, got {stdout:?}"
    );
}

// =====================================================================
// E2E #11 — `cobrust run` forwards extra args to user program
//
// Smoke: dispatch §Test plan Tier 3 says
// `cobrust run argv_dump.cb a b c → stdout contains "a", "b", "c"`.
// This requires `cobrust run` to forward args to the produced executable,
// which today's `run.rs` does NOT do (verified at HEAD 50b95ee). The dev
// impl extends `cobrust run` accordingly OR documents that argv
// e2e tests use `cobrust build` + invoke-exe (as #4 already does).
// =====================================================================

#[test]
fn test_e2e_11_cobrust_run_forwards_argv_to_program() {
    let bin = cobrust_binary();
    let fixture = fixture_path("argv_dump.cb");
    assert!(fixture.exists(), "fixture missing");
    let out = Command::new(&bin)
        .arg("run")
        .arg(&fixture)
        .arg("--quiet")
        .arg("--")
        .arg("foo")
        .arg("bar")
        .arg("baz")
        .current_dir(workspace_root())
        .output()
        .expect("invoke cobrust run");
    assert!(
        out.status.success(),
        "cobrust run failed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("foo"),
        "expected `foo` forwarded via cobrust run, got {stdout:?}"
    );
    assert!(
        stdout.contains("bar"),
        "expected `bar` forwarded via cobrust run, got {stdout:?}"
    );
    assert!(
        stdout.contains("baz"),
        "expected `baz` forwarded via cobrust run, got {stdout:?}"
    );
}

// =====================================================================
// E2E #12 — echo_stdin fixture round-trips a piped line
// =====================================================================

#[test]
fn test_e2e_12_echo_stdin_round_trip() {
    let (exe, build_stderr) = build_fixture("echo_stdin.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let (code, stdout, _) = run_with_args_and_stdin(&exe, &[], b"echoed-input-line\n");
    assert_eq!(code, 0);
    assert!(
        stdout.contains("echoed-input-line"),
        "expected piped line in stdout, got {stdout:?}"
    );
}

// =====================================================================
// E2E #13 — bounded-time check: argv-only program completes promptly
// =====================================================================

#[test]
fn test_e2e_13_argv_program_completes_quickly() {
    let (exe, build_stderr) = build_fixture("argv_dump.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let start = std::time::Instant::now();
    let (code, _, _) = run_with_args_and_stdin(&exe, &["x"], b"");
    let elapsed = start.elapsed();
    assert_eq!(code, 0);
    assert!(
        elapsed < Duration::from_secs(5),
        "argv-only program took {elapsed:?} — expected < 5s"
    );
}

// =====================================================================
// E2E #14 — UTF-8 user-supplied args round-trip via argv()
// =====================================================================

#[test]
fn test_e2e_14_argv_utf8_round_trip() {
    let (exe, build_stderr) = build_fixture("argv_dump.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let (code, stdout, _) = run_with_args_and_stdin(&exe, &["你好", "世界"], b"");
    assert_eq!(code, 0);
    assert!(
        stdout.contains("你好") && stdout.contains("世界"),
        "expected UTF-8 args in stdout, got {stdout:?}"
    );
}

// =====================================================================
// E2E #15 — exit code propagates from W2 fixture (returns 0 normally)
// =====================================================================

#[test]
fn test_e2e_15_fixture_exit_code_zero() {
    let (exe, build_stderr) = build_fixture("two_sum.cb");
    assert!(
        exe.as_os_str().len() > 0,
        "build failed; stderr={build_stderr}"
    );
    let (code, _, _) = run_with_args_and_stdin(&exe, &[], b"x\n");
    assert_eq!(code, 0, "fixture must exit 0 on normal path");
}
