//! ADR-0044 W2 Phase 3 — LeetCode oracle-match corpus (TDD step 1).
//!
//! These are **failing** tests until Phase 3 DEV creates the 10 `.cb`
//! programs under `examples/leetcode/`. Do NOT write any `.cb` code here.
//!
//! Test shape: `build_and_run_leetcode(name, stdin_bytes, argv) -> String`
//!   1. Locates `examples/leetcode/{name}` (must exist; panics → FAILED if not).
//!   2. Compiles it with `cobrust build -o <tmp_exe>`.
//!   3. Runs the exe with `stdin_bytes` piped.
//!   4. Asserts exit 0 + returns stdout.
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

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// =====================================================================
// Harness — shared helpers
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

fn leetcode_dir() -> PathBuf {
    workspace_root().join("examples/leetcode")
}

fn leetcode_src(name: &str) -> PathBuf {
    leetcode_dir().join(name)
}

/// Built executable plus its tempdir guard.
struct BuiltLeetcode {
    _temp_dir: tempfile::TempDir,
    exe: PathBuf,
    stderr: String,
}

/// Build `examples/leetcode/{name}` into a unique tmp-exe.
/// Panics with a descriptive message if the source file does not exist
/// (expected during Phase 3 TDD step 1 — DEV hasn't created the .cb yet).
fn build_leetcode(name: &str) -> BuiltLeetcode {
    let src = leetcode_src(name);
    assert!(
        src.exists(),
        "LeetCode fixture '{}' not found at {:?} — Phase 3 DEV must create it",
        name,
        src
    );
    let bin = cobrust_binary();
    let exe_dir = tempfile::tempdir().expect("create temp exe dir");
    let exe = exe_dir.path().join(src.file_stem().unwrap());
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
        return BuiltLeetcode {
            _temp_dir: exe_dir,
            exe: PathBuf::new(),
            stderr,
        };
    }
    BuiltLeetcode {
        _temp_dir: exe_dir,
        exe,
        stderr,
    }
}

/// Run `exe` piping `stdin_bytes` and optional `argv` extras.
/// Returns (exit_code, stdout, stderr).
fn run_leetcode(exe: &Path, stdin_bytes: &[u8], argv: &[&str]) -> (i32, String, String) {
    let mut child = Command::new(exe)
        .args(argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn leetcode exe");
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

/// Build + run in one call; assert exit 0 and return stdout.
fn build_and_run_leetcode(name: &str, stdin_bytes: &[u8], argv: &[&str]) -> String {
    let built = build_leetcode(name);
    assert!(
        built.exe.as_os_str().len() > 0,
        "cobrust build failed for '{}'; stderr=\n{}",
        name,
        built.stderr
    );
    let (code, stdout, run_stderr) = run_leetcode(&built.exe, stdin_bytes, argv);
    assert_eq!(
        code, 0,
        "exe '{}' exited with code {}; stderr=\n{}",
        name, code, run_stderr
    );
    stdout
}

// =====================================================================
// LC-01 — Two Sum
//
// Input format (stdin):
//   Line 1: N   (number of elements)
//   Lines 2..=N+1: one integer each
//   Line N+2: target
//
// Oracle: [2, 7, 11, 15], target=9 → "0\n1\n"
// =====================================================================

#[test]
fn test_lc01_two_sum_oracle_match() {
    // N=4, elements=[2,7,11,15], target=9
    let stdout = build_and_run_leetcode("two_sum.cb", b"4\n2\n7\n11\n15\n9\n", &[]);
    assert_eq!(
        stdout, "0\n1\n",
        "two_sum: expected indices 0 and 1, got {:?}",
        stdout
    );
}

// =====================================================================
// LC-02 — Reverse String
//
// Input format (stdin): one line with the string to reverse.
// Oracle: "hello" → "olleh\n"
// =====================================================================

#[test]
fn test_lc02_reverse_string_oracle_match() {
    let stdout = build_and_run_leetcode("reverse_string.cb", b"hello\n", &[]);
    assert_eq!(
        stdout, "olleh\n",
        "reverse_string: expected 'olleh', got {:?}",
        stdout
    );
}

// =====================================================================
// LC-03 — Fibonacci
//
// Input format (stdin): one line with integer N.
// Oracle: N=10 → "55\n"  (F(0)=0, F(1)=1, …, F(10)=55)
// =====================================================================

#[test]
fn test_lc03_fibonacci_oracle_match() {
    let stdout = build_and_run_leetcode("fibonacci.cb", b"10\n", &[]);
    assert_eq!(
        stdout, "55\n",
        "fibonacci: expected F(10)=55, got {:?}",
        stdout
    );
}

// =====================================================================
// LC-04 — Valid Parentheses
//
// Input format (stdin): one line with the bracket string.
// Oracle #1: "()[]{}" → "true\n"
// Oracle #2: "(]"     → "false\n"
// =====================================================================

#[test]
fn test_lc04_valid_parentheses_oracle_match_true() {
    let stdout = build_and_run_leetcode("valid_parentheses.cb", b"()[]{}\n", &[]);
    assert_eq!(
        stdout, "true\n",
        "valid_parentheses: expected true for '()[]{{}}', got {:?}",
        stdout
    );
}

#[test]
fn test_lc04_valid_parentheses_oracle_match_false() {
    let stdout = build_and_run_leetcode("valid_parentheses.cb", b"(]\n", &[]);
    assert_eq!(
        stdout, "false\n",
        "valid_parentheses: expected false for '(]', got {:?}",
        stdout
    );
}

// =====================================================================
// LC-05 — Merge Two Sorted Lists
//
// Input format (stdin):
//   Line 1: N M   (element counts of list 1 and list 2)
//   Line 2: N space-separated ints (sorted ascending)
//   Line 3: M space-separated ints (sorted ascending)
//
// Oracle: N=3 M=3, [1,3,5] [2,4,6] → "1\n2\n3\n4\n5\n6\n"
// =====================================================================

#[test]
fn test_lc05_merge_two_sorted_lists_oracle_match() {
    let stdout = build_and_run_leetcode("merge_two_sorted_lists.cb", b"3 3\n1 3 5\n2 4 6\n", &[]);
    assert_eq!(
        stdout, "1\n2\n3\n4\n5\n6\n",
        "merge_two_sorted_lists: expected merged sorted output, got {:?}",
        stdout
    );
}

// =====================================================================
// LC-06 — Maximum Subarray (Kadane's)
//
// Input format (stdin):
//   Line 1: N
//   Line 2: N space-separated ints
//
// Oracle: N=9, [-2,1,-3,4,-1,2,1,-5,4] → "6\n"
// =====================================================================

#[test]
fn test_lc06_maximum_subarray_oracle_match() {
    let stdout = build_and_run_leetcode("maximum_subarray.cb", b"9\n-2 1 -3 4 -1 2 1 -5 4\n", &[]);
    assert_eq!(
        stdout, "6\n",
        "maximum_subarray: expected max-sum=6, got {:?}",
        stdout
    );
}

// =====================================================================
// LC-07 — Binary Search
//
// Input format (stdin):
//   Line 1: N
//   Line 2: N space-separated sorted ints
//   Line 3: target
//
// Oracle: N=6, [-1,0,3,5,9,12], target=9 → "4\n"
// =====================================================================

#[test]
fn test_lc07_binary_search_oracle_match() {
    let stdout = build_and_run_leetcode("binary_search.cb", b"6\n-1 0 3 5 9 12\n9\n", &[]);
    assert_eq!(
        stdout, "4\n",
        "binary_search: expected index 4 for target=9, got {:?}",
        stdout
    );
}

// =====================================================================
// LC-08 — Climbing Stairs
//
// Input format (stdin): one line with integer N.
// Oracle: N=5 → "8\n"  (ways to climb 5 stairs taking 1 or 2 steps)
// =====================================================================

#[test]
fn test_lc08_climbing_stairs_oracle_match() {
    let stdout = build_and_run_leetcode("climbing_stairs.cb", b"5\n", &[]);
    assert_eq!(
        stdout, "8\n",
        "climbing_stairs: expected 8 ways for N=5, got {:?}",
        stdout
    );
}

// =====================================================================
// LC-09 — Best Time to Buy and Sell Stock
//
// Input format (stdin):
//   Line 1: N
//   Line 2: N space-separated ints (prices)
//
// Oracle: N=6, [7,1,5,3,6,4] → "5\n"  (buy at 1, sell at 6)
// =====================================================================

#[test]
fn test_lc09_stock_best_time_oracle_match() {
    let stdout = build_and_run_leetcode("stock_best_time.cb", b"6\n7 1 5 3 6 4\n", &[]);
    assert_eq!(
        stdout, "5\n",
        "stock_best_time: expected max-profit=5, got {:?}",
        stdout
    );
}

// =====================================================================
// LC-10 — Roman to Integer
//
// Input format (stdin): one line with the roman numeral string.
// Oracle: "MCMXCIV" → "1994\n"
// =====================================================================

#[test]
fn test_lc10_roman_to_integer_oracle_match() {
    let stdout = build_and_run_leetcode("roman_to_integer.cb", b"MCMXCIV\n", &[]);
    assert_eq!(
        stdout, "1994\n",
        "roman_to_integer: expected 1994 for MCMXCIV, got {:?}",
        stdout
    );
}

// =====================================================================
// Compile-all gate
//
// Iterates examples/leetcode/*.cb and asserts each compiles successfully.
// Today (Phase 3 TDD step 1) the directory does not exist → this test
// fails with a clear message. Phase 3 DEV must create the directory and
// all 10 .cb files.
// =====================================================================

#[test]
fn test_lc_all_compile() {
    let dir = leetcode_dir();
    let read = std::fs::read_dir(&dir);
    assert!(
        read.is_ok(),
        "examples/leetcode/ directory does not exist at {:?} — Phase 3 DEV must create it",
        dir
    );
    let bin = cobrust_binary();
    let mut compiled = 0usize;
    for entry in read.unwrap().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("cb") {
            continue;
        }
        let exe_dir = tempfile::tempdir().expect("create temp exe dir");
        let exe = exe_dir.path().join(path.file_stem().unwrap());
        let out = Command::new(&bin)
            .arg("build")
            .arg(&path)
            .arg("-o")
            .arg(&exe)
            .arg("--quiet")
            .current_dir(workspace_root())
            .output()
            .expect("invoke cobrust build");
        assert!(
            out.status.success(),
            "compile-all gate: {} failed to compile; stderr=\n{}",
            path.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        compiled += 1;
    }
    assert!(
        compiled >= 10,
        "compile-all gate: expected ≥10 .cb files in examples/leetcode/, found only {}",
        compiled
    );
}
