//! ADR-0047 LC-100 Tier A — Bucket B1 e2e harness (Arrays + Two Pointers + Hash maps).
//!
//! TDD step 3: DEV implementations landed; each test builds the corresponding
//! `examples/leetcode-stress/NNN-slug/solution.cb` and pipes each `test.toml`
//! case through the binary, asserting oracle-match.
//!
//! Tests with `#[ignore]` have `failure.md` in their directory; see that file
//! for root cause and fix tier.
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
// Shared helpers (modelled on leetcode_corpus_e2e.rs)
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

struct BuiltStress {
    _temp_dir: tempfile::TempDir,
    exe: PathBuf,
    stderr: String,
}

fn build_stress(slug: &str) -> BuiltStress {
    let src = stress_src(slug);
    assert!(src.exists(), "solution.cb not found at {:?}", src);
    let bin = cobrust_binary();
    let exe_dir = tempfile::tempdir().expect("create temp exe dir");
    let exe = exe_dir.path().join("solution");
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
        return BuiltStress {
            _temp_dir: exe_dir,
            exe: PathBuf::new(),
            stderr,
        };
    }
    BuiltStress {
        _temp_dir: exe_dir,
        exe,
        stderr,
    }
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
    let built = build_stress(slug);
    assert!(
        built.exe.as_os_str().len() > 0,
        "cobrust build failed for '{}'; stderr=\n{}",
        slug,
        built.stderr
    );
    let (code, stdout, run_stderr) = run_stress(&built.exe, stdin_bytes);
    assert_eq!(
        code, 0,
        "exe '{}' exited with code {}; stderr=\n{}",
        slug, code, run_stderr
    );
    stdout
}

// =====================================================================
// LC-001 — Array Running Sum
// Input: N on line 1; N space-separated ints on line 2
// Oracle: prefix sum at each position
// =====================================================================

#[test]
fn test_lc001_array_running_sum() {
    let out = build_and_run_stress("001-array-running-sum", b"5\n1 2 3 4 5\n");
    assert_eq!(out, "1\n3\n6\n10\n15\n", "running sum case 1");
    let out2 = build_and_run_stress("001-array-running-sum", b"3\n10 20 30\n");
    assert_eq!(out2, "10\n30\n60\n", "running sum case 2");
    let out3 = build_and_run_stress("001-array-running-sum", b"1\n7\n");
    assert_eq!(out3, "7\n", "running sum case 3");
}

// =====================================================================
// LC-002 — Array Contains Duplicate
// Input: N; N space-separated ints
// Oracle: "true" if any value appears twice
// =====================================================================

#[test]
fn test_lc002_array_contains_duplicate() {
    let out = build_and_run_stress("002-array-contains-duplicate", b"4\n1 2 3 1\n");
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress("002-array-contains-duplicate", b"4\n1 2 3 4\n");
    assert_eq!(out2, "false\n");
    let out3 = build_and_run_stress("002-array-contains-duplicate", b"1\n5\n");
    assert_eq!(out3, "false\n");
    let out4 = build_and_run_stress("002-array-contains-duplicate", b"2\n99 99\n");
    assert_eq!(out4, "true\n");
}

// =====================================================================
// LC-003 — Array Find Disappeared Numbers
// Input: N; N ints each in [1,N]
// Oracle: missing values 1..N in ascending order
// =====================================================================

#[test]
fn test_lc003_array_find_disappeared() {
    let out = build_and_run_stress("003-array-find-disappeared", b"8\n4 3 2 7 8 2 3 1\n");
    assert_eq!(out, "5\n6\n");
    let out2 = build_and_run_stress("003-array-find-disappeared", b"4\n1 1 1 1\n");
    assert_eq!(out2, "2\n3\n4\n");
    let out3 = build_and_run_stress("003-array-find-disappeared", b"3\n1 2 3\n");
    assert_eq!(out3, "");
}

// =====================================================================
// LC-004 — Array Single Number Count
// Input: N (odd); N ints where all except one appear twice
// Oracle: the single value
// =====================================================================

#[test]
fn test_lc004_array_single_number_count() {
    let out = build_and_run_stress("004-array-single-number-count", b"5\n2 2 1 4 4\n");
    assert_eq!(out, "1\n");
    let out2 = build_and_run_stress("004-array-single-number-count", b"3\n3 1 3\n");
    assert_eq!(out2, "1\n");
    let out3 = build_and_run_stress("004-array-single-number-count", b"1\n7\n");
    assert_eq!(out3, "7\n");
    let out4 = build_and_run_stress("004-array-single-number-count", b"7\n4 1 2 1 2 5 4\n");
    assert_eq!(out4, "5\n");
}

// =====================================================================
// LC-005 — Array Move Zeroes
// Input: N; N ints
// Oracle: non-zeros in original order, then zeros
// =====================================================================

#[test]
fn test_lc005_array_move_zeroes() {
    let out = build_and_run_stress("005-array-move-zeroes", b"5\n0 1 0 3 12\n");
    assert_eq!(out, "1\n3\n12\n0\n0\n");
    let out2 = build_and_run_stress("005-array-move-zeroes", b"3\n0 0 1\n");
    assert_eq!(out2, "1\n0\n0\n");
    let out3 = build_and_run_stress("005-array-move-zeroes", b"2\n0 0\n");
    assert_eq!(out3, "0\n0\n");
}

// =====================================================================
// LC-006 — Array Plus One
// Input: N; N digits MSB first
// Oracle: digits of number+1, one per line
// =====================================================================

#[test]
fn test_lc006_array_plus_one() {
    let out = build_and_run_stress("006-array-plus-one", b"3\n1 2 3\n");
    assert_eq!(out, "1\n2\n4\n");
    let out2 = build_and_run_stress("006-array-plus-one", b"3\n9 9 9\n");
    assert_eq!(out2, "1\n0\n0\n0\n");
    let out3 = build_and_run_stress("006-array-plus-one", b"1\n0\n");
    assert_eq!(out3, "1\n");
    let out4 = build_and_run_stress("006-array-plus-one", b"2\n9 9\n");
    assert_eq!(out4, "1\n0\n0\n");
}

// =====================================================================
// LC-007 — Array Sorted Intersection
// Input: M N; M sorted ints; N sorted ints
// Oracle: intersection values in non-decreasing order
// =====================================================================

#[test]
fn test_lc007_array_sorted_intersection() {
    let out = build_and_run_stress("007-array-sorted-intersection", b"4 2\n1 1 2 2\n2 2\n");
    assert_eq!(out, "2\n2\n");
    let out2 = build_and_run_stress("007-array-sorted-intersection", b"3 4\n1 3 5\n1 2 3 6\n");
    assert_eq!(out2, "1\n3\n");
    let out3 = build_and_run_stress("007-array-sorted-intersection", b"2 2\n1 2\n3 4\n");
    assert_eq!(out3, "");
}

// =====================================================================
// LC-008 — Array Third Maximum
// Corpus corrected in Sprint 1; see corpus-corrected.md. All cases pass.
// =====================================================================

#[test]
fn test_lc008_array_third_maximum() {
    let out = build_and_run_stress("008-array-third-maximum", b"3\n3 2 1\n");
    assert_eq!(out, "1\n");
    let out2 = build_and_run_stress("008-array-third-maximum", b"4\n2 2 3 1\n");
    assert_eq!(out2, "1\n");
    let out3 = build_and_run_stress("008-array-third-maximum", b"2\n1 2\n");
    assert_eq!(out3, "2\n");
    let out4 = build_and_run_stress("008-array-third-maximum", b"5\n5 2 5 1 3\n");
    assert_eq!(out4, "2\n");
}

// =====================================================================
// LC-009 — Array Max Consecutive Ones
// Input: N; N binary (0/1) values
// Oracle: longest run of 1s
// =====================================================================

#[test]
fn test_lc009_array_max_consecutive_ones() {
    let out = build_and_run_stress(
        "009-array-max-consecutive-ones",
        b"10\n1 1 0 1 1 1 0 1 1 1\n",
    );
    assert_eq!(out, "3\n");
    let out2 = build_and_run_stress("009-array-max-consecutive-ones", b"5\n1 0 1 1 0\n");
    assert_eq!(out2, "2\n");
    let out3 = build_and_run_stress("009-array-max-consecutive-ones", b"3\n0 0 0\n");
    assert_eq!(out3, "0\n");
    let out4 = build_and_run_stress("009-array-max-consecutive-ones", b"4\n1 1 1 1\n");
    assert_eq!(out4, "4\n");
}

// =====================================================================
// LC-010 — Array Prefix Product
// Input: N; N ints
// Oracle: product-except-self at each index
// =====================================================================

#[test]
fn test_lc010_array_prefix_product() {
    let out = build_and_run_stress("010-array-prefix-product", b"4\n1 2 3 4\n");
    assert_eq!(out, "24\n12\n8\n6\n");
    let out2 = build_and_run_stress("010-array-prefix-product", b"3\n2 3 4\n");
    assert_eq!(out2, "12\n8\n6\n");
    let out3 = build_and_run_stress("010-array-prefix-product", b"2\n5 6\n");
    assert_eq!(out3, "6\n5\n");
}

// =====================================================================
// LC-011 — Two Pointers Reverse In Place
// Input: N; N ints
// Oracle: reversed sequence
// =====================================================================

#[test]
fn test_lc011_twoptr_reverse_in_place() {
    let out = build_and_run_stress("011-twoptr-reverse-in-place", b"5\n1 2 3 4 5\n");
    assert_eq!(out, "5\n4\n3\n2\n1\n");
    let out2 = build_and_run_stress("011-twoptr-reverse-in-place", b"4\n10 20 30 40\n");
    assert_eq!(out2, "40\n30\n20\n10\n");
    let out3 = build_and_run_stress("011-twoptr-reverse-in-place", b"1\n7\n");
    assert_eq!(out3, "7\n");
    let out4 = build_and_run_stress("011-twoptr-reverse-in-place", b"2\n3 4\n");
    assert_eq!(out4, "4\n3\n");
}

// =====================================================================
// LC-012 — Two Pointers Valid Palindrome
// Input: one string line
// Oracle: "true" or "false"
// =====================================================================

#[test]
fn test_lc012_twoptr_valid_palindrome() {
    let out = build_and_run_stress(
        "012-twoptr-valid-palindrome",
        b"A man a plan a canal Panama\n",
    );
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress("012-twoptr-valid-palindrome", b"race a car\n");
    assert_eq!(out2, "false\n");
    let out3 = build_and_run_stress(
        "012-twoptr-valid-palindrome",
        b"Was it a car or a cat I saw\n",
    );
    assert_eq!(out3, "true\n");
    let out4 = build_and_run_stress("012-twoptr-valid-palindrome", b"hello\n");
    assert_eq!(out4, "false\n");
    let out5 = build_and_run_stress("012-twoptr-valid-palindrome", b" \n");
    assert_eq!(out5, "true\n");
}

// =====================================================================
// LC-013 — Two Pointers Squares of Sorted Array
// Input: N; N sorted ints (may include negatives)
// Oracle: squares in non-decreasing order
// =====================================================================

#[test]
fn test_lc013_twoptr_squares_sorted() {
    let out = build_and_run_stress("013-twoptr-squares-sorted", b"5\n-4 -1 0 3 10\n");
    assert_eq!(out, "0\n1\n9\n16\n100\n");
    let out2 = build_and_run_stress("013-twoptr-squares-sorted", b"4\n-3 -2 -1 0\n");
    assert_eq!(out2, "0\n1\n4\n9\n");
    let out3 = build_and_run_stress("013-twoptr-squares-sorted", b"3\n1 2 3\n");
    assert_eq!(out3, "1\n4\n9\n");
}

// =====================================================================
// LC-014 — Two Pointers Remove Duplicates In Place
// Input: N; N sorted ints
// Oracle: count K, then K unique values
// =====================================================================

#[test]
fn test_lc014_twoptr_remove_duplicates() {
    let out = build_and_run_stress("014-twoptr-remove-duplicates", b"5\n1 1 2 2 3\n");
    assert_eq!(out, "3\n1\n2\n3\n");
    let out2 = build_and_run_stress("014-twoptr-remove-duplicates", b"5\n0 0 1 1 2\n");
    assert_eq!(out2, "3\n0\n1\n2\n");
    let out3 = build_and_run_stress("014-twoptr-remove-duplicates", b"3\n1 2 3\n");
    assert_eq!(out3, "3\n1\n2\n3\n");
    let out4 = build_and_run_stress("014-twoptr-remove-duplicates", b"4\n1 1 1 1\n");
    assert_eq!(out4, "1\n1\n");
}

// =====================================================================
// LC-015 — Two Pointers Remove Element
// Input: N; N ints; target value
// Oracle: count K, then K remaining elements
// =====================================================================

#[test]
fn test_lc015_twoptr_remove_element() {
    let out = build_and_run_stress("015-twoptr-remove-element", b"4\n3 2 2 3\n3\n");
    assert_eq!(out, "2\n2\n2\n");
    let out2 = build_and_run_stress("015-twoptr-remove-element", b"5\n0 1 2 2 3\n2\n");
    assert_eq!(out2, "3\n0\n1\n3\n");
    let out3 = build_and_run_stress("015-twoptr-remove-element", b"3\n1 1 1\n1\n");
    assert_eq!(out3, "0\n");
}

// =====================================================================
// LC-016 — Two Pointers Container With Most Water
// Input: N; N bar heights
// Oracle: max water volume
// =====================================================================

#[test]
fn test_lc016_twoptr_container_water() {
    let out = build_and_run_stress("016-twoptr-container-water", b"9\n1 8 6 2 5 4 8 3 7\n");
    assert_eq!(out, "49\n");
    let out2 = build_and_run_stress("016-twoptr-container-water", b"2\n1 1\n");
    assert_eq!(out2, "1\n");
    let out3 = build_and_run_stress("016-twoptr-container-water", b"4\n1 2 1 4\n");
    assert_eq!(out3, "4\n");
}

// =====================================================================
// LC-017 — Two Pointers Sort Colors
// Input: N; N values each 0/1/2
// Oracle: 0s then 1s then 2s
// =====================================================================

#[test]
fn test_lc017_twoptr_sort_colors() {
    let out = build_and_run_stress("017-twoptr-sort-colors", b"6\n2 0 2 1 1 0\n");
    assert_eq!(out, "0\n0\n1\n1\n2\n2\n");
    let out2 = build_and_run_stress("017-twoptr-sort-colors", b"3\n2 0 1\n");
    assert_eq!(out2, "0\n1\n2\n");
    let out3 = build_and_run_stress("017-twoptr-sort-colors", b"1\n0\n");
    assert_eq!(out3, "0\n");
    let out4 = build_and_run_stress("017-twoptr-sort-colors", b"3\n1 2 0\n");
    assert_eq!(out4, "0\n1\n2\n");
}

// =====================================================================
// LC-018 — Two Pointers Trapping Rain Water
// Input: N; N non-negative heights
// Oracle: total trapped water units
// =====================================================================

#[test]
fn test_lc018_twoptr_trapping_rain() {
    let out = build_and_run_stress("018-twoptr-trapping-rain", b"12\n0 1 0 2 1 0 1 3 2 1 2 1\n");
    assert_eq!(out, "6\n");
    let out2 = build_and_run_stress("018-twoptr-trapping-rain", b"6\n4 2 0 3 2 5\n");
    assert_eq!(out2, "9\n");
    let out3 = build_and_run_stress("018-twoptr-trapping-rain", b"3\n3 0 2\n");
    assert_eq!(out3, "2\n");
}

// =====================================================================
// LC-019 — Two Pointers Pair Sum in Sorted Array
// Input: N; N sorted ints; target
// Oracle: two 1-based indices summing to target
// =====================================================================

#[test]
fn test_lc019_twoptr_pair_sum_sorted() {
    let out = build_and_run_stress("019-twoptr-pair-sum-sorted", b"4\n2 7 11 15\n9\n");
    assert_eq!(out, "1\n2\n");
    let out2 = build_and_run_stress("019-twoptr-pair-sum-sorted", b"3\n2 3 4\n6\n");
    assert_eq!(out2, "1\n3\n");
    let out3 = build_and_run_stress("019-twoptr-pair-sum-sorted", b"2\n1 2\n3\n");
    assert_eq!(out3, "1\n2\n");
}

// =====================================================================
// LC-020 — Two Pointers Backspace String Compare
// Input: string s; string t
// Oracle: "true" if both produce same typed result
// =====================================================================

#[test]
fn test_lc020_twoptr_backspace_compare() {
    let out = build_and_run_stress("020-twoptr-backspace-compare", b"ab#c\nad#c\n");
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress("020-twoptr-backspace-compare", b"a##c\n#a#c\n");
    assert_eq!(out2, "true\n");
    let out3 = build_and_run_stress("020-twoptr-backspace-compare", b"a#c\nb\n");
    assert_eq!(out3, "false\n");
    let out4 = build_and_run_stress("020-twoptr-backspace-compare", b"abc\nabc\n");
    assert_eq!(out4, "true\n");
}

// =====================================================================
// LC-021 — Hash Map Contains Duplicate via Set Emulation
// Input: N; N ints
// Oracle: "true" if duplicate exists
// =====================================================================

#[test]
fn test_lc021_hashmap_contains_dup_set() {
    let out = build_and_run_stress("021-hashmap-contains-dup-set", b"4\n1 2 3 1\n");
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress("021-hashmap-contains-dup-set", b"3\n1 2 3\n");
    assert_eq!(out2, "false\n");
    let out3 = build_and_run_stress("021-hashmap-contains-dup-set", b"2\n5 5\n");
    assert_eq!(out3, "true\n");
    let out4 = build_and_run_stress("021-hashmap-contains-dup-set", b"1\n0\n");
    assert_eq!(out4, "false\n");
}

// =====================================================================
// LC-022 — Hash Map Valid Anagram
// Input: string s; string t
// Oracle: "true" if anagram
// =====================================================================

#[test]
fn test_lc022_hashmap_valid_anagram() {
    let out = build_and_run_stress("022-hashmap-valid-anagram", b"anagram\nnagaram\n");
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress("022-hashmap-valid-anagram", b"rat\ncar\n");
    assert_eq!(out2, "false\n");
    let out3 = build_and_run_stress("022-hashmap-valid-anagram", b"ab\na\n");
    assert_eq!(out3, "false\n");
    let out4 = build_and_run_stress("022-hashmap-valid-anagram", b"a\na\n");
    assert_eq!(out4, "true\n");
    let out5 = build_and_run_stress("022-hashmap-valid-anagram", b"abc\ncba\n");
    assert_eq!(out5, "true\n");
}

// =====================================================================
// LC-023 — Hash Map Majority Element
// Input: N; N ints (one value appears >N/2 times)
// Oracle: majority value
// =====================================================================

#[test]
fn test_lc023_hashmap_majority_element() {
    let out = build_and_run_stress("023-hashmap-majority-element", b"5\n3 2 3 1 3\n");
    assert_eq!(out, "3\n");
    let out2 = build_and_run_stress("023-hashmap-majority-element", b"3\n2 2 1\n");
    assert_eq!(out2, "2\n");
    let out3 = build_and_run_stress("023-hashmap-majority-element", b"1\n7\n");
    assert_eq!(out3, "7\n");
    let out4 = build_and_run_stress("023-hashmap-majority-element", b"5\n1 1 1 1 2\n");
    assert_eq!(out4, "1\n");
}

// =====================================================================
// LC-024 — Hash Map Group Anagrams
// NOTE: RUNTIME-FAIL; see failure.md for root cause.
// str_at on string-literal variables produces misaligned pointer;
// list[str] not available for storing multiple input strings.
// =====================================================================

#[test]
#[ignore = "LC-024 hashmap-group-anagrams: RUNTIME-FAIL; see failure.md (str_at on literal vars misaligned + missing list[str])"]
fn test_lc024_hashmap_group_anagrams() {
    let out = build_and_run_stress(
        "024-hashmap-group-anagrams",
        b"6\neat\ntea\ntan\nate\nnat\nbat\n",
    );
    assert_eq!(out, "eat\ntea\nate\n\ntan\nnat\n\nbat\n");
    let out2 = build_and_run_stress("024-hashmap-group-anagrams", b"1\nabc\n");
    assert_eq!(out2, "abc\n");
    let out3 = build_and_run_stress("024-hashmap-group-anagrams", b"2\nab\nba\n");
    assert_eq!(out3, "ab\nba\n");
}

// =====================================================================
// LC-025 — Hash Map First Unique Character
// Input: lowercase string
// Oracle: 0-based index of first unique char, or -1
// =====================================================================

#[test]
fn test_lc025_hashmap_first_unique_char() {
    let out = build_and_run_stress("025-hashmap-first-unique-char", b"abcabd\n");
    assert_eq!(out, "2\n");
    let out2 = build_and_run_stress("025-hashmap-first-unique-char", b"aabb\n");
    assert_eq!(out2, "-1\n");
    let out3 = build_and_run_stress("025-hashmap-first-unique-char", b"z\n");
    assert_eq!(out3, "0\n");
    let out4 = build_and_run_stress("025-hashmap-first-unique-char", b"leetcode\n");
    assert_eq!(out4, "0\n");
}

// =====================================================================
// LC-026 — Hash Map Isomorphic Strings
// Input: string s; string t
// Oracle: "true" if isomorphic
// =====================================================================

#[test]
fn test_lc026_hashmap_isomorphic_strings() {
    let out = build_and_run_stress("026-hashmap-isomorphic-strings", b"egg\nadd\n");
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress("026-hashmap-isomorphic-strings", b"foo\nbar\n");
    assert_eq!(out2, "false\n");
    let out3 = build_and_run_stress("026-hashmap-isomorphic-strings", b"paper\ntitle\n");
    assert_eq!(out3, "true\n");
    let out4 = build_and_run_stress("026-hashmap-isomorphic-strings", b"ab\naa\n");
    assert_eq!(out4, "false\n");
}

// =====================================================================
// LC-027 — Hash Map Longest Substring Without Repeating Characters
// Input: one string
// Oracle: length of longest no-repeat window
// =====================================================================

#[test]
fn test_lc027_hashmap_longest_no_repeat() {
    let out = build_and_run_stress("027-hashmap-longest-no-repeat", b"abcabcbb\n");
    assert_eq!(out, "3\n");
    let out2 = build_and_run_stress("027-hashmap-longest-no-repeat", b"bbbbb\n");
    assert_eq!(out2, "1\n");
    let out3 = build_and_run_stress("027-hashmap-longest-no-repeat", b"pwwkew\n");
    assert_eq!(out3, "3\n");
    let out4 = build_and_run_stress("027-hashmap-longest-no-repeat", b"abcd\n");
    assert_eq!(out4, "4\n");
}

// =====================================================================
// LC-028 — Hash Map Word Pattern
// Input: pattern string; space-separated words
// Oracle: "true" if bijective match
// =====================================================================

#[test]
fn test_lc028_hashmap_word_pattern() {
    let out = build_and_run_stress("028-hashmap-word-pattern", b"abba\ndog cat cat dog\n");
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress("028-hashmap-word-pattern", b"abba\ndog cat cat fish\n");
    assert_eq!(out2, "false\n");
    let out3 = build_and_run_stress("028-hashmap-word-pattern", b"aaaa\ndog dog dog dog\n");
    assert_eq!(out3, "true\n");
    let out4 = build_and_run_stress("028-hashmap-word-pattern", b"abba\ndog dog dog dog\n");
    assert_eq!(out4, "false\n");
}

// =====================================================================
// LC-029 — Hash Map Subarray Sum Equals K
// Input: N; N ints; target K
// Oracle: count of subarrays summing to K
// =====================================================================

#[test]
fn test_lc029_hashmap_subarray_sum_k() {
    let out = build_and_run_stress("029-hashmap-subarray-sum-k", b"5\n1 1 1 1 1\n2\n");
    assert_eq!(out, "4\n");
    let out2 = build_and_run_stress("029-hashmap-subarray-sum-k", b"4\n1 2 3 0\n3\n");
    assert_eq!(out2, "3\n");
    let out3 = build_and_run_stress("029-hashmap-subarray-sum-k", b"3\n1 2 3\n6\n");
    assert_eq!(out3, "1\n");
    let out4 = build_and_run_stress("029-hashmap-subarray-sum-k", b"2\n1 -1\n0\n");
    assert_eq!(out4, "1\n");
}

// =====================================================================
// LC-030 — Hash Map Two Sum with Index Tracking
// Corpus corrected in Sprint 1; see corpus-corrected.md. All cases pass.
// =====================================================================

#[test]
fn test_lc030_hashmap_two_sum_indices() {
    let out = build_and_run_stress("030-hashmap-two-sum-indices", b"4\n2 7 11 15\n9\n");
    assert_eq!(out, "0\n1\n");
    let out2 = build_and_run_stress("030-hashmap-two-sum-indices", b"3\n3 2 4\n6\n");
    assert_eq!(out2, "1\n2\n");
    let out3 = build_and_run_stress("030-hashmap-two-sum-indices", b"2\n3 3\n6\n");
    assert_eq!(out3, "0\n1\n");
    let out4 = build_and_run_stress("030-hashmap-two-sum-indices", b"4\n1 5 7 10\n11\n");
    assert_eq!(out4, "0\n3\n");
}
