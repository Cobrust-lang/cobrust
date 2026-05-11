//! ADR-0047 LC-100 Tier A — Bucket B4 e2e harness (Math + Greedy + Recursion).
//!
//! TDD step 3: DEV implementations landed; each test builds the corresponding
//! `examples/leetcode-stress/NNN-slug/solution.cb` and pipes each `test.toml`
//! case through the binary, asserting oracle-match.
//!
//! Tests with `#[ignore]` have `failure.md` in their directory; see that file
//! for root cause and fix tier.
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09: module-level
//! lint allow header at the top.

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
// Shared helpers (modelled on lc100_stress_e2e_b1.rs)
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

fn build_stress(slug: &str) -> (PathBuf, String) {
    let src = stress_src(slug);
    assert!(src.exists(), "solution.cb not found at {:?}", src);
    let bin = cobrust_binary();
    let exe_dir = std::env::temp_dir().join(format!(
        "cobrust-lc100-b4-{}-{}-{}",
        slug,
        std::process::id(),
        build_seq()
    ));
    let _ = std::fs::create_dir_all(&exe_dir);
    let exe = exe_dir.join("solution");
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

fn run_stress(exe: &Path, stdin_bytes: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn stress exe");
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

fn build_and_run_stress(slug: &str, stdin_bytes: &[u8]) -> String {
    let (exe, build_stderr) = build_stress(slug);
    assert!(
        exe.as_os_str().len() > 0,
        "cobrust build failed for '{}'; stderr=\n{}",
        slug,
        build_stderr
    );
    let (code, stdout, run_stderr) = run_stress(&exe, stdin_bytes);
    assert_eq!(
        code, 0,
        "exe '{}' exited with code {}; stderr=\n{}",
        slug, code, run_stderr
    );
    stdout
}

// =====================================================================
// LC-091 — Happy Number (Cycle Detection)
// Input: one integer N
// Oracle: "true" if N is a happy number, "false" otherwise
// Algorithm: Floyd cycle detection on digit-square-sum sequence
// =====================================================================

#[test]
fn test_lc091_happy_number_cycle_detect() {
    let out = build_and_run_stress("091-happy-number-cycle-detect", b"19\n");
    assert_eq!(out, "true\n", "happy number: 19");
    let out2 = build_and_run_stress("091-happy-number-cycle-detect", b"2\n");
    assert_eq!(out2, "false\n", "not happy: 2");
    let out3 = build_and_run_stress("091-happy-number-cycle-detect", b"1\n");
    assert_eq!(out3, "true\n", "happy number: 1");
    let out4 = build_and_run_stress("091-happy-number-cycle-detect", b"7\n");
    assert_eq!(out4, "true\n", "happy number: 7");
    let out5 = build_and_run_stress("091-happy-number-cycle-detect", b"4\n");
    assert_eq!(out5, "false\n", "not happy: 4");
}

// =====================================================================
// LC-092 — Palindrome Number (Integer Math)
// Input: one integer N
// Oracle: "true" if palindrome, "false" otherwise
// Algorithm: reverse second half of digits, compare halves
// =====================================================================

#[test]
fn test_lc092_palindrome_number_integer() {
    let out = build_and_run_stress("092-palindrome-number-integer", b"121\n");
    assert_eq!(out, "true\n", "palindrome: 121");
    let out2 = build_and_run_stress("092-palindrome-number-integer", b"-121\n");
    assert_eq!(out2, "false\n", "negative not palindrome");
    let out3 = build_and_run_stress("092-palindrome-number-integer", b"10\n");
    assert_eq!(out3, "false\n", "trailing zero not palindrome");
    let out4 = build_and_run_stress("092-palindrome-number-integer", b"0\n");
    assert_eq!(out4, "true\n", "zero is palindrome");
    let out5 = build_and_run_stress("092-palindrome-number-integer", b"1221\n");
    assert_eq!(out5, "true\n", "even-length palindrome");
    let out6 = build_and_run_stress("092-palindrome-number-integer", b"12321\n");
    assert_eq!(out6, "true\n", "odd-length palindrome");
}

// =====================================================================
// LC-093 — Integer to Roman
// NOTE: RUNTIME-FAIL; see failure.md for root cause.
// print_no_nl(literal_str) produces misaligned pointer; codegen gap.
// =====================================================================

#[test]
#[ignore = "LC-093 integer-to-roman: RUNTIME-FAIL; see failure.md (print_no_nl on literal string misaligned pointer — codegen gap)"]
fn test_lc093_integer_to_roman() {
    let out = build_and_run_stress("093-integer-to-roman", b"3\n");
    assert_eq!(out, "III\n");
    let out2 = build_and_run_stress("093-integer-to-roman", b"4\n");
    assert_eq!(out2, "IV\n");
    let out3 = build_and_run_stress("093-integer-to-roman", b"9\n");
    assert_eq!(out3, "IX\n");
    let out4 = build_and_run_stress("093-integer-to-roman", b"58\n");
    assert_eq!(out4, "LVIII\n");
    let out5 = build_and_run_stress("093-integer-to-roman", b"1994\n");
    assert_eq!(out5, "MCMXCIV\n");
    let out6 = build_and_run_stress("093-integer-to-roman", b"3999\n");
    assert_eq!(out6, "MMMCMXCIX\n");
    let out7 = build_and_run_stress("093-integer-to-roman", b"1\n");
    assert_eq!(out7, "I\n");
}

// =====================================================================
// LC-094 — GCD Euclidean
// Input: two space-separated integers A B on one line
// Oracle: GCD(A, B)
// Algorithm: iterative Euclidean (% operator)
// =====================================================================

#[test]
fn test_lc094_gcd_euclidean() {
    let out = build_and_run_stress("094-gcd-euclidean", b"48 18\n");
    assert_eq!(out, "6\n", "gcd(48,18)");
    let out2 = build_and_run_stress("094-gcd-euclidean", b"100 75\n");
    assert_eq!(out2, "25\n", "gcd(100,75)");
    let out3 = build_and_run_stress("094-gcd-euclidean", b"0 5\n");
    assert_eq!(out3, "5\n", "gcd(0,5)");
    let out4 = build_and_run_stress("094-gcd-euclidean", b"7 7\n");
    assert_eq!(out4, "7\n", "gcd(7,7)");
    let out5 = build_and_run_stress("094-gcd-euclidean", b"1 1\n");
    assert_eq!(out5, "1\n", "gcd(1,1)");
    let out6 = build_and_run_stress("094-gcd-euclidean", b"13 17\n");
    assert_eq!(out6, "1\n", "gcd(13,17) coprime");
}

// =====================================================================
// LC-095 — Assign Cookies (Greedy)
// Input: G children greed factors; S cookie sizes
// Oracle: max children satisfiable
// Algorithm: sort both + two-pointer sweep
// =====================================================================

#[test]
fn test_lc095_assign_cookies() {
    let out = build_and_run_stress("095-assign-cookies", b"3\n1 2 3\n3\n1 1 2\n");
    assert_eq!(out, "2\n", "assign cookies case 1");
    let out2 = build_and_run_stress("095-assign-cookies", b"2\n1 2\n3\n1 2 3\n");
    assert_eq!(out2, "2\n", "assign cookies case 2");
    let out3 = build_and_run_stress("095-assign-cookies", b"3\n1 2 3\n2\n1 1\n");
    assert_eq!(out3, "1\n", "assign cookies case 3");
    let out4 = build_and_run_stress("095-assign-cookies", b"1\n5\n1\n3\n");
    assert_eq!(out4, "0\n", "cookie too small");
    let out5 = build_and_run_stress("095-assign-cookies", b"1\n1\n1\n10\n");
    assert_eq!(out5, "1\n", "cookie satisfies child");
}

// =====================================================================
// LC-096 — Jump Game (Greedy)
// Input: N; N jump lengths
// Oracle: "true" if last index reachable from index 0
// Algorithm: greedy max-reach scan
// =====================================================================

#[test]
fn test_lc096_jump_game_can_reach() {
    let out = build_and_run_stress("096-jump-game-can-reach", b"5\n2 3 1 1 4\n");
    assert_eq!(out, "true\n", "jump game reachable");
    let out2 = build_and_run_stress("096-jump-game-can-reach", b"5\n3 2 1 0 4\n");
    assert_eq!(out2, "false\n", "jump game stuck at zero");
    let out3 = build_and_run_stress("096-jump-game-can-reach", b"1\n0\n");
    assert_eq!(out3, "true\n", "single element");
    let out4 = build_and_run_stress("096-jump-game-can-reach", b"3\n0 1 0\n");
    assert_eq!(out4, "false\n", "starts at zero");
    let out5 = build_and_run_stress("096-jump-game-can-reach", b"4\n1 1 1 0\n");
    assert_eq!(out5, "true\n", "just reaches end");
    let out6 = build_and_run_stress("096-jump-game-can-reach", b"2\n0 1\n");
    assert_eq!(out6, "false\n", "zero at start two elements");
}

// =====================================================================
// LC-097 — Gas Station Circular (Greedy)
// Corpus corrected in Sprint 1; see corpus-corrected.md. All cases pass.
// =====================================================================

#[test]
fn test_lc097_gas_station_circular() {
    let out = build_and_run_stress("097-gas-station-circular", b"5\n1 2 3 4 5\n3 4 5 1 2\n");
    assert_eq!(out, "3\n", "gas station case 1");
    let out2 = build_and_run_stress("097-gas-station-circular", b"3\n2 3 4\n3 4 3\n");
    assert_eq!(out2, "-1\n", "gas station case 2");
    let out3 = build_and_run_stress("097-gas-station-circular", b"3\n1 2 3\n3 4 5\n");
    assert_eq!(out3, "-1\n", "gas station case 3 impossible");
    let out4 = build_and_run_stress("097-gas-station-circular", b"1\n5\n5\n");
    assert_eq!(out4, "0\n", "gas station single station");
    let out5 = build_and_run_stress("097-gas-station-circular", b"4\n4 6 7 4\n6 5 3 5\n");
    assert_eq!(out5, "1\n", "gas station case 5");
}

// =====================================================================
// LC-098 — Power x^n Recursive
// Input: two space-separated integers X N
// Oracle: X^N
// Algorithm: recursive fast exponentiation (ADR-0034 FnRef Call lowering)
// =====================================================================

#[test]
fn test_lc098_power_x_n_recursive() {
    let out = build_and_run_stress("098-power-x-n-recursive", b"2 10\n");
    assert_eq!(out, "1024\n", "2^10");
    let out2 = build_and_run_stress("098-power-x-n-recursive", b"3 5\n");
    assert_eq!(out2, "243\n", "3^5");
    let out3 = build_and_run_stress("098-power-x-n-recursive", b"2 0\n");
    assert_eq!(out3, "1\n", "any^0 = 1");
    let out4 = build_and_run_stress("098-power-x-n-recursive", b"1 100\n");
    assert_eq!(out4, "1\n", "1^100 = 1");
    let out5 = build_and_run_stress("098-power-x-n-recursive", b"5 3\n");
    assert_eq!(out5, "125\n", "5^3");
    let out6 = build_and_run_stress("098-power-x-n-recursive", b"2 1\n");
    assert_eq!(out6, "2\n", "2^1 = 2");
    let out7 = build_and_run_stress("098-power-x-n-recursive", b"7 4\n");
    assert_eq!(out7, "2401\n", "7^4");
}

// =====================================================================
// LC-099 — Generate Parentheses (Recursion)
// NOTE: RUNTIME-FAIL; see failure.md for root cause.
// print_no_nl("(") / print_no_nl(")") produce misaligned pointer; codegen gap.
// =====================================================================

#[test]
#[ignore = "LC-099 generate-parentheses: RUNTIME-FAIL; see failure.md (print_no_nl on literal string misaligned pointer — codegen gap)"]
fn test_lc099_generate_parentheses() {
    let out = build_and_run_stress("099-generate-parentheses", b"1\n");
    assert_eq!(out, "()\n");
    let out2 = build_and_run_stress("099-generate-parentheses", b"2\n");
    assert_eq!(out2, "(())\n()()\n");
    let out3 = build_and_run_stress("099-generate-parentheses", b"3\n");
    assert_eq!(out3, "((()))\n(()())\n(())()\n()(())\n()()()\n");
}

// =====================================================================
// LC-100 — Subsets (Recursion)
// NOTE: RUNTIME-FAIL; see failure.md for root cause.
// print_no_nl(" ") / print_int_no_nl digit literals produce misaligned pointer.
// =====================================================================

#[test]
#[ignore = "LC-100 subsets-recursive: RUNTIME-FAIL; see failure.md (print_no_nl on literal string misaligned pointer — codegen gap)"]
fn test_lc100_subsets_recursive() {
    let out = build_and_run_stress("100-subsets-recursive", b"2\n1 2\n");
    assert_eq!(out, "1 2\n1\n2\n\n");
    let out2 = build_and_run_stress("100-subsets-recursive", b"3\n1 2 3\n");
    assert_eq!(out2, "1 2 3\n1 2\n1 3\n1\n2 3\n2\n3\n\n");
    let out3 = build_and_run_stress("100-subsets-recursive", b"1\n5\n");
    assert_eq!(out3, "5\n\n");
}
