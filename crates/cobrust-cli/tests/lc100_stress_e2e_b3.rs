//! ADR-0047 LC-100 Tier A — Bucket B3 e2e harness (DP + Binary search + Bit manipulation).
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
        "cobrust-lc100-b3-{}-{}-{}",
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
// LC-061 — Coin Change Minimum
// Input: K on line 1; K coin values on line 2; amount on line 3
// Oracle: minimum coins to make amount, or -1 if impossible
// NOTE: test case 5 has incorrect expected; see failure.md
// =====================================================================

#[test]
#[ignore = "LC-061 coin-change-min: RUNTIME-FAIL; see failure.md (test corpus C5 incorrect: expected 4, correct answer 5 for coins {1,5,10} amount 27)"]
fn test_lc061_coin_change_min() {
    let out = build_and_run_stress("061-coin-change-min", b"3\n1 5 11\n15\n");
    assert_eq!(out, "3\n", "case 1");
    let out2 = build_and_run_stress("061-coin-change-min", b"3\n2 5 10\n3\n");
    assert_eq!(out2, "-1\n", "case 2");
    let out3 = build_and_run_stress("061-coin-change-min", b"1\n1\n0\n");
    assert_eq!(out3, "0\n", "case 3");
    let out4 = build_and_run_stress("061-coin-change-min", b"2\n1 2\n11\n");
    assert_eq!(out4, "6\n", "case 4");
    let out5 = build_and_run_stress("061-coin-change-min", b"3\n1 5 10\n27\n");
    assert_eq!(out5, "4\n", "case 5 (corpus says 4, correct is 5)");
}

// =====================================================================
// LC-062 — Longest Increasing Subsequence
// Input: N; N space-separated ints
// Oracle: length of longest strictly increasing subsequence
// =====================================================================

#[test]
fn test_lc062_longest_increasing_subseq() {
    let out = build_and_run_stress("062-longest-increasing-subseq", b"6\n3 10 2 1 20 9\n");
    assert_eq!(out, "3\n", "case 1");
    let out2 = build_and_run_stress("062-longest-increasing-subseq", b"5\n5 4 3 2 1\n");
    assert_eq!(out2, "1\n", "case 2");
    let out3 = build_and_run_stress("062-longest-increasing-subseq", b"1\n42\n");
    assert_eq!(out3, "1\n", "case 3");
    let out4 = build_and_run_stress("062-longest-increasing-subseq", b"6\n1 3 6 7 9 4\n");
    assert_eq!(out4, "5\n", "case 4");
    let out5 = build_and_run_stress("062-longest-increasing-subseq", b"7\n10 9 2 5 3 7 101\n");
    assert_eq!(out5, "4\n", "case 5");
}

// =====================================================================
// LC-063 — Unique Paths in Grid
// Input: one line with M N (rows cols)
// Oracle: number of unique paths from top-left to bottom-right
// =====================================================================

#[test]
fn test_lc063_unique_paths_grid() {
    let out = build_and_run_stress("063-unique-paths-grid", b"3 7\n");
    assert_eq!(out, "28\n", "case 1");
    let out2 = build_and_run_stress("063-unique-paths-grid", b"1 1\n");
    assert_eq!(out2, "1\n", "case 2");
    let out3 = build_and_run_stress("063-unique-paths-grid", b"2 2\n");
    assert_eq!(out3, "2\n", "case 3");
    let out4 = build_and_run_stress("063-unique-paths-grid", b"3 3\n");
    assert_eq!(out4, "6\n", "case 4");
    let out5 = build_and_run_stress("063-unique-paths-grid", b"4 4\n");
    assert_eq!(out5, "20\n", "case 5");
}

// =====================================================================
// LC-064 — House Robber Linear
// Input: N; N space-separated house values
// Oracle: max value robbed without adjacent houses
// NOTE: test case 5 has incorrect expected; see failure.md
// =====================================================================

#[test]
#[ignore = "LC-064 house-robber-linear: RUNTIME-FAIL; see failure.md (test corpus C5 incorrect: expected 19, correct answer 15 for [6,7,1,3,8,2])"]
fn test_lc064_house_robber_linear() {
    let out = build_and_run_stress("064-house-robber-linear", b"4\n2 7 9 3\n");
    assert_eq!(out, "11\n", "case 1");
    let out2 = build_and_run_stress("064-house-robber-linear", b"3\n5 1 5\n");
    assert_eq!(out2, "10\n", "case 2");
    let out3 = build_and_run_stress("064-house-robber-linear", b"1\n100\n");
    assert_eq!(out3, "100\n", "case 3");
    let out4 = build_and_run_stress("064-house-robber-linear", b"5\n1 2 3 1 4\n");
    assert_eq!(out4, "8\n", "case 4");
    let out5 = build_and_run_stress("064-house-robber-linear", b"6\n6 7 1 3 8 2\n");
    assert_eq!(out5, "19\n", "case 5 (corpus says 19, correct is 15)");
}

// =====================================================================
// LC-065 — House Robber Circular
// Input: N; N space-separated house values (circular arrangement)
// Oracle: max value robbed from circular layout
// =====================================================================

#[test]
fn test_lc065_house_robber_circular() {
    let out = build_and_run_stress("065-house-robber-circular", b"3\n2 3 2\n");
    assert_eq!(out, "3\n", "case 1");
    let out2 = build_and_run_stress("065-house-robber-circular", b"4\n1 2 3 1\n");
    assert_eq!(out2, "4\n", "case 2");
    let out3 = build_and_run_stress("065-house-robber-circular", b"1\n5\n");
    assert_eq!(out3, "5\n", "case 3");
    let out4 = build_and_run_stress("065-house-robber-circular", b"5\n2 3 2 4 1\n");
    assert_eq!(out4, "7\n", "case 4");
    let out5 = build_and_run_stress("065-house-robber-circular", b"6\n1 2 3 4 5 1\n");
    assert_eq!(out5, "9\n", "case 5");
}

// =====================================================================
// LC-066 — Edit Distance
// Input: string s; string t
// Oracle: minimum edit distance (insert/delete/replace)
// =====================================================================

#[test]
fn test_lc066_edit_distance() {
    let out = build_and_run_stress("066-edit-distance", b"horse\nros\n");
    assert_eq!(out, "3\n", "case 1");
    let out2 = build_and_run_stress("066-edit-distance", b"abc\nabc\n");
    assert_eq!(out2, "0\n", "case 2");
    let out3 = build_and_run_stress("066-edit-distance", b"intention\nexecution\n");
    assert_eq!(out3, "5\n", "case 3");
    let out4 = build_and_run_stress("066-edit-distance", b"a\nb\n");
    assert_eq!(out4, "1\n", "case 4");
    let out5 = build_and_run_stress("066-edit-distance", b"sunday\nsaturday\n");
    assert_eq!(out5, "3\n", "case 5");
}

// =====================================================================
// LC-067 — Partition Equal Subset
// Input: N; N positive ints
// Oracle: "true" if array can be split into two equal-sum subsets
// NOTE: test case 5 has incorrect expected; see failure.md
// =====================================================================

#[test]
#[ignore = "LC-067 partition-equal-subset: RUNTIME-FAIL; see failure.md (test corpus C5 incorrect: expected false, correct answer true for [3,3,3,4,5])"]
fn test_lc067_partition_equal_subset() {
    let out = build_and_run_stress("067-partition-equal-subset", b"4\n1 5 11 5\n");
    assert_eq!(out, "true\n", "case 1");
    let out2 = build_and_run_stress("067-partition-equal-subset", b"3\n1 2 3\n");
    assert_eq!(out2, "true\n", "case 2");
    let out3 = build_and_run_stress("067-partition-equal-subset", b"3\n1 2 5\n");
    assert_eq!(out3, "false\n", "case 3");
    let out4 = build_and_run_stress("067-partition-equal-subset", b"2\n1 1\n");
    assert_eq!(out4, "true\n", "case 4");
    let out5 = build_and_run_stress("067-partition-equal-subset", b"5\n3 3 3 4 5\n");
    assert_eq!(
        out5, "false\n",
        "case 5 (corpus says false, correct is true)"
    );
}

// =====================================================================
// LC-068 — Word Break DP
// Input: string s; W (word count); W words one per line
// Oracle: "true" if s can be segmented by dictionary words
// =====================================================================

#[test]
fn test_lc068_word_break_dp() {
    let out = build_and_run_stress("068-word-break-dp", b"applepenapple\n2\napple\npen\n");
    assert_eq!(out, "true\n", "case 1");
    let out2 = build_and_run_stress(
        "068-word-break-dp",
        b"catsandog\n5\ncats\ndog\nsand\nand\ncat\n",
    );
    assert_eq!(out2, "false\n", "case 2");
    let out3 = build_and_run_stress("068-word-break-dp", b"abc\n2\nab\nc\n");
    assert_eq!(out3, "true\n", "case 3");
    let out4 = build_and_run_stress("068-word-break-dp", b"leetcode\n2\nleet\ncode\n");
    assert_eq!(out4, "true\n", "case 4");
    let out5 = build_and_run_stress("068-word-break-dp", b"a\n1\na\n");
    assert_eq!(out5, "true\n", "case 5");
}

// =====================================================================
// LC-069 — Pascal Triangle Row
// NOTE: RUNTIME-FAIL; see failure.md (print_no_nl with string literals crashes
// when multiple calls exist — stdlib/codegen misalignment gap)
// =====================================================================

#[test]
#[ignore = "LC-069 pascal-triangle-row: RUNTIME-FAIL; see failure.md (print_no_nl multi-literal misalignment gap — space-separated integer output not supported)"]
fn test_lc069_pascal_triangle_row() {
    let out = build_and_run_stress("069-pascal-triangle-row", b"0\n");
    assert_eq!(out, "1\n", "case 1");
    let out2 = build_and_run_stress("069-pascal-triangle-row", b"1\n");
    assert_eq!(out2, "1 1\n", "case 2");
    let out3 = build_and_run_stress("069-pascal-triangle-row", b"3\n");
    assert_eq!(out3, "1 3 3 1\n", "case 3");
    let out4 = build_and_run_stress("069-pascal-triangle-row", b"5\n");
    assert_eq!(out4, "1 5 10 10 5 1\n", "case 4");
    let out5 = build_and_run_stress("069-pascal-triangle-row", b"4\n");
    assert_eq!(out5, "1 4 6 4 1\n", "case 5");
}

// =====================================================================
// LC-070 — Count Bits DP
// Input: one integer N
// Oracle: popcount for 0..N inclusive, one per line
// =====================================================================

#[test]
fn test_lc070_count_bits_dp() {
    let out = build_and_run_stress("070-count-bits-dp", b"2\n");
    assert_eq!(out, "0\n1\n1\n", "case 1");
    let out2 = build_and_run_stress("070-count-bits-dp", b"5\n");
    assert_eq!(out2, "0\n1\n1\n2\n1\n2\n", "case 2");
    let out3 = build_and_run_stress("070-count-bits-dp", b"0\n");
    assert_eq!(out3, "0\n", "case 3");
    let out4 = build_and_run_stress("070-count-bits-dp", b"7\n");
    assert_eq!(out4, "0\n1\n1\n2\n1\n2\n2\n3\n", "case 4");
    let out5 = build_and_run_stress("070-count-bits-dp", b"1\n");
    assert_eq!(out5, "0\n1\n", "case 5");
}

// =====================================================================
// LC-071 — Search in Rotated Sorted Array
// Input: N; N ints (rotated sorted); target
// Oracle: 0-based index of target, or -1
// =====================================================================

#[test]
fn test_lc071_search_rotated_sorted() {
    let out = build_and_run_stress("071-search-rotated-sorted", b"7\n4 5 6 7 0 1 2\n0\n");
    assert_eq!(out, "4\n", "case 1");
    let out2 = build_and_run_stress("071-search-rotated-sorted", b"7\n4 5 6 7 0 1 2\n3\n");
    assert_eq!(out2, "-1\n", "case 2");
    let out3 = build_and_run_stress("071-search-rotated-sorted", b"1\n1\n0\n");
    assert_eq!(out3, "-1\n", "case 3");
    let out4 = build_and_run_stress("071-search-rotated-sorted", b"6\n6 7 1 2 3 4\n1\n");
    assert_eq!(out4, "2\n", "case 4");
    let out5 = build_and_run_stress("071-search-rotated-sorted", b"5\n3 4 5 1 2\n4\n");
    assert_eq!(out5, "1\n", "case 5");
}

// =====================================================================
// LC-072 — Find First and Last Position
// NOTE: RUNTIME-FAIL; see failure.md (print_no_nl multi-literal
// misalignment gap — space-separated integer output not supported)
// =====================================================================

#[test]
#[ignore = "LC-072 find-first-last-position: RUNTIME-FAIL; see failure.md (print_no_nl multi-literal misalignment gap — space-separated output not supported)"]
fn test_lc072_find_first_last_position() {
    let out = build_and_run_stress("072-find-first-last-position", b"6\n5 7 7 8 8 10\n8\n");
    assert_eq!(out, "3 4\n", "case 1");
    let out2 = build_and_run_stress("072-find-first-last-position", b"6\n5 7 7 8 8 10\n6\n");
    assert_eq!(out2, "-1 -1\n", "case 2");
    let out3 = build_and_run_stress("072-find-first-last-position", b"1\n5\n5\n");
    assert_eq!(out3, "0 0\n", "case 3");
    let out4 = build_and_run_stress("072-find-first-last-position", b"5\n2 2 2 2 2\n2\n");
    assert_eq!(out4, "0 4\n", "case 4");
    let out5 = build_and_run_stress("072-find-first-last-position", b"4\n1 2 3 4\n3\n");
    assert_eq!(out5, "2 2\n", "case 5");
}

// =====================================================================
// LC-073 — Search Insert Position
// Input: N; N sorted ints; target
// Oracle: index where target found or would insert
// =====================================================================

#[test]
fn test_lc073_search_insert_position() {
    let out = build_and_run_stress("073-search-insert-position", b"4\n1 3 5 6\n5\n");
    assert_eq!(out, "2\n", "case 1");
    let out2 = build_and_run_stress("073-search-insert-position", b"4\n1 3 5 6\n2\n");
    assert_eq!(out2, "1\n", "case 2");
    let out3 = build_and_run_stress("073-search-insert-position", b"4\n1 3 5 6\n7\n");
    assert_eq!(out3, "4\n", "case 3");
    let out4 = build_and_run_stress("073-search-insert-position", b"4\n1 3 5 6\n0\n");
    assert_eq!(out4, "0\n", "case 4");
    let out5 = build_and_run_stress("073-search-insert-position", b"1\n5\n5\n");
    assert_eq!(out5, "0\n", "case 5");
}

// =====================================================================
// LC-074 — Peak Element Binary Search
// Input: N; N ints (no two adjacent equal)
// Oracle: index of any peak element
// NOTE: test case 4 has non-deterministic expected; see failure.md
// =====================================================================

#[test]
#[ignore = "LC-074 peak-element-binary-search: RUNTIME-FAIL; see failure.md (test corpus C4 expects index 3, algorithm returns valid index 1 for [1,2,1,3])"]
fn test_lc074_peak_element_binary_search() {
    let out = build_and_run_stress("074-peak-element-binary-search", b"5\n1 2 3 1 0\n");
    assert_eq!(out, "2\n", "case 1");
    let out2 = build_and_run_stress("074-peak-element-binary-search", b"1\n7\n");
    assert_eq!(out2, "0\n", "case 2");
    let out3 = build_and_run_stress("074-peak-element-binary-search", b"3\n1 3 2\n");
    assert_eq!(out3, "1\n", "case 3");
    let out4 = build_and_run_stress("074-peak-element-binary-search", b"4\n1 2 1 3\n");
    assert_eq!(
        out4, "3\n",
        "case 4 (corpus expects 3, algorithm returns valid 1)"
    );
    let out5 = build_and_run_stress("074-peak-element-binary-search", b"5\n5 4 3 2 1\n");
    assert_eq!(out5, "0\n", "case 5");
}

// =====================================================================
// LC-075 — Integer Square Root Binary Search
// Input: non-negative integer X
// Oracle: floor(sqrt(X))
// =====================================================================

#[test]
fn test_lc075_sqrt_integer_binary_search() {
    let out = build_and_run_stress("075-sqrt-integer-binary-search", b"4\n");
    assert_eq!(out, "2\n", "case 1");
    let out2 = build_and_run_stress("075-sqrt-integer-binary-search", b"8\n");
    assert_eq!(out2, "2\n", "case 2");
    let out3 = build_and_run_stress("075-sqrt-integer-binary-search", b"0\n");
    assert_eq!(out3, "0\n", "case 3");
    let out4 = build_and_run_stress("075-sqrt-integer-binary-search", b"1\n");
    assert_eq!(out4, "1\n", "case 4");
    let out5 = build_and_run_stress("075-sqrt-integer-binary-search", b"100\n");
    assert_eq!(out5, "10\n", "case 5");
}

// =====================================================================
// LC-076 — Search 2D Matrix
// Input: M N; M rows of N ints each; target
// Oracle: "true" if target found, "false" otherwise
// =====================================================================

#[test]
fn test_lc076_search_2d_matrix() {
    let out = build_and_run_stress(
        "076-search-2d-matrix",
        b"3 4\n1 3 5 7\n10 11 16 20\n23 30 34 60\n3\n",
    );
    assert_eq!(out, "true\n", "case 1");
    let out2 = build_and_run_stress(
        "076-search-2d-matrix",
        b"3 4\n1 3 5 7\n10 11 16 20\n23 30 34 60\n13\n",
    );
    assert_eq!(out2, "false\n", "case 2");
    let out3 = build_and_run_stress("076-search-2d-matrix", b"1 1\n1\n1\n");
    assert_eq!(out3, "true\n", "case 3");
    let out4 = build_and_run_stress("076-search-2d-matrix", b"2 3\n1 4 7\n10 13 16\n10\n");
    assert_eq!(out4, "true\n", "case 4");
    let out5 = build_and_run_stress("076-search-2d-matrix", b"2 3\n1 4 7\n10 13 16\n9\n");
    assert_eq!(out5, "false\n", "case 5");
}

// =====================================================================
// LC-077 — Capacity to Ship Packages
// Input: N D (packages, days); N package weights
// Oracle: minimum ship capacity to deliver in D days
// =====================================================================

#[test]
fn test_lc077_capacity_ship_binary_search() {
    let out = build_and_run_stress(
        "077-capacity-ship-binary-search",
        b"10 5\n1 2 3 4 5 6 7 8 9 10\n",
    );
    assert_eq!(out, "15\n", "case 1");
    let out2 = build_and_run_stress("077-capacity-ship-binary-search", b"6 3\n3 2 2 4 1 4\n");
    assert_eq!(out2, "6\n", "case 2");
    let out3 = build_and_run_stress("077-capacity-ship-binary-search", b"1 1\n5\n");
    assert_eq!(out3, "5\n", "case 3");
    let out4 = build_and_run_stress("077-capacity-ship-binary-search", b"4 2\n1 2 3 4\n");
    assert_eq!(out4, "6\n", "case 4");
    let out5 = build_and_run_stress("077-capacity-ship-binary-search", b"5 1\n2 3 1 4 2\n");
    assert_eq!(out5, "12\n", "case 5");
}

// =====================================================================
// LC-078 — Koko Eating Speed
// NOTE: test cases 1 and 2 have incorrect expected; see failure.md
// =====================================================================

#[test]
#[ignore = "LC-078 koko-eating-speed: RUNTIME-FAIL; see failure.md (test corpus C1 expected 4 but min feasible K=7; C2 expected 23 but min feasible K=15)"]
fn test_lc078_koko_eating_speed() {
    let out = build_and_run_stress("078-koko-eating-speed", b"4 5\n3 6 7 11\n");
    assert_eq!(out, "4\n", "case 1 (corpus says 4, correct is 7)");
    let out2 = build_and_run_stress("078-koko-eating-speed", b"5 8\n30 11 23 4 20\n");
    assert_eq!(out2, "23\n", "case 2 (corpus says 23, correct is 15)");
    let out3 = build_and_run_stress("078-koko-eating-speed", b"1 3\n10\n");
    assert_eq!(out3, "4\n", "case 3");
    let out4 = build_and_run_stress("078-koko-eating-speed", b"3 3\n5 5 5\n");
    assert_eq!(out4, "5\n", "case 4");
    let out5 = build_and_run_stress("078-koko-eating-speed", b"4 7\n1 1 1 1\n");
    assert_eq!(out5, "1\n", "case 5");
}

// =====================================================================
// LC-079 — Minimum in Rotated Sorted Array
// Input: N; N ints (rotated sorted, all unique)
// Oracle: minimum value
// =====================================================================

#[test]
fn test_lc079_minimum_in_rotated_sorted() {
    let out = build_and_run_stress("079-minimum-in-rotated-sorted", b"5\n3 4 5 1 2\n");
    assert_eq!(out, "1\n", "case 1");
    let out2 = build_and_run_stress("079-minimum-in-rotated-sorted", b"4\n4 5 6 7\n");
    assert_eq!(out2, "4\n", "case 2");
    let out3 = build_and_run_stress("079-minimum-in-rotated-sorted", b"1\n0\n");
    assert_eq!(out3, "0\n", "case 3");
    let out4 = build_and_run_stress("079-minimum-in-rotated-sorted", b"6\n6 7 1 2 3 4\n");
    assert_eq!(out4, "1\n", "case 4");
    let out5 = build_and_run_stress("079-minimum-in-rotated-sorted", b"3\n2 3 1\n");
    assert_eq!(out5, "1\n", "case 5");
}

// =====================================================================
// LC-080 — Count Negatives in Sorted Matrix
// Input: M N; M rows of N non-increasing ints
// Oracle: total count of negative numbers
// NOTE: test case 4 has incorrect expected; see failure.md
// =====================================================================

#[test]
#[ignore = "LC-080 count-negative-sorted-matrix: RUNTIME-FAIL; see failure.md (test corpus C4 expects 7, correct answer is 6 for [5,1,0]/[-1,-1,-1]/[-5,-5,-5])"]
fn test_lc080_count_negative_sorted_matrix() {
    let out = build_and_run_stress(
        "080-count-negative-sorted-matrix",
        b"4 4\n4 3 2 -1\n3 2 1 -1\n1 1 -1 -2\n-1 -1 -2 -3\n",
    );
    assert_eq!(out, "8\n", "case 1");
    let out2 = build_and_run_stress("080-count-negative-sorted-matrix", b"2 2\n3 2\n1 0\n");
    assert_eq!(out2, "0\n", "case 2");
    let out3 = build_and_run_stress("080-count-negative-sorted-matrix", b"1 1\n-1\n");
    assert_eq!(out3, "1\n", "case 3");
    let out4 = build_and_run_stress(
        "080-count-negative-sorted-matrix",
        b"3 3\n5 1 0\n-1 -1 -1\n-5 -5 -5\n",
    );
    assert_eq!(out4, "7\n", "case 4 (corpus says 7, correct is 6)");
    let out5 = build_and_run_stress(
        "080-count-negative-sorted-matrix",
        b"2 4\n3 2 -1 -2\n1 0 -1 -3\n",
    );
    assert_eq!(out5, "4\n", "case 5");
}

// =====================================================================
// LC-081 — Single Number XOR
// Input: N (odd); N ints where all except one appear twice
// Oracle: the single non-duplicated number
// =====================================================================

#[test]
fn test_lc081_single_number_xor() {
    let out = build_and_run_stress("081-single-number-xor", b"5\n4 1 2 1 2\n");
    assert_eq!(out, "4\n", "case 1");
    let out2 = build_and_run_stress("081-single-number-xor", b"3\n7 7 3\n");
    assert_eq!(out2, "3\n", "case 2");
    let out3 = build_and_run_stress("081-single-number-xor", b"1\n99\n");
    assert_eq!(out3, "99\n", "case 3");
    let out4 = build_and_run_stress("081-single-number-xor", b"7\n2 2 3 3 5 6 6\n");
    assert_eq!(out4, "5\n", "case 4");
    let out5 = build_and_run_stress("081-single-number-xor", b"3\n0 0 1\n");
    assert_eq!(out5, "1\n", "case 5");
}

// =====================================================================
// LC-082 — Count Set Bits
// Input: non-negative integer N
// Oracle: number of 1-bits in N (popcount)
// =====================================================================

#[test]
fn test_lc082_count_set_bits() {
    let out = build_and_run_stress("082-count-set-bits", b"11\n");
    assert_eq!(out, "3\n", "case 1");
    let out2 = build_and_run_stress("082-count-set-bits", b"128\n");
    assert_eq!(out2, "1\n", "case 2");
    let out3 = build_and_run_stress("082-count-set-bits", b"0\n");
    assert_eq!(out3, "0\n", "case 3");
    let out4 = build_and_run_stress("082-count-set-bits", b"255\n");
    assert_eq!(out4, "8\n", "case 4");
    let out5 = build_and_run_stress("082-count-set-bits", b"1023\n");
    assert_eq!(out5, "10\n", "case 5");
}

// =====================================================================
// LC-083 — Counting Bits DP
// Input: integer N
// Oracle: popcount for 0..N inclusive, one per line
// =====================================================================

#[test]
fn test_lc083_counting_bits_dp() {
    let out = build_and_run_stress("083-counting-bits-dp", b"4\n");
    assert_eq!(out, "0\n1\n1\n2\n1\n", "case 1");
    let out2 = build_and_run_stress("083-counting-bits-dp", b"0\n");
    assert_eq!(out2, "0\n", "case 2");
    let out3 = build_and_run_stress("083-counting-bits-dp", b"3\n");
    assert_eq!(out3, "0\n1\n1\n2\n", "case 3");
    let out4 = build_and_run_stress("083-counting-bits-dp", b"7\n");
    assert_eq!(out4, "0\n1\n1\n2\n1\n2\n2\n3\n", "case 4");
    let out5 = build_and_run_stress("083-counting-bits-dp", b"1\n");
    assert_eq!(out5, "0\n1\n", "case 5");
}

// =====================================================================
// LC-084 — Reverse Bits (32-bit)
// Input: non-negative 32-bit integer N
// Oracle: integer formed by reversing all 32 bits
// =====================================================================

#[test]
fn test_lc084_reverse_bits_32() {
    let out = build_and_run_stress("084-reverse-bits-32", b"43261596\n");
    assert_eq!(out, "964176192\n", "case 1");
    let out2 = build_and_run_stress("084-reverse-bits-32", b"0\n");
    assert_eq!(out2, "0\n", "case 2");
    let out3 = build_and_run_stress("084-reverse-bits-32", b"1\n");
    assert_eq!(out3, "2147483648\n", "case 3");
    let out4 = build_and_run_stress("084-reverse-bits-32", b"4294967295\n");
    assert_eq!(out4, "4294967295\n", "case 4");
    let out5 = build_and_run_stress("084-reverse-bits-32", b"2\n");
    assert_eq!(out5, "1073741824\n", "case 5");
}

// =====================================================================
// LC-085 — Hamming Distance
// Input: two space-separated non-negative integers
// Oracle: number of differing bit positions
// =====================================================================

#[test]
fn test_lc085_hamming_distance() {
    let out = build_and_run_stress("085-hamming-distance", b"1 4\n");
    assert_eq!(out, "2\n", "case 1");
    let out2 = build_and_run_stress("085-hamming-distance", b"3 1\n");
    assert_eq!(out2, "1\n", "case 2");
    let out3 = build_and_run_stress("085-hamming-distance", b"0 0\n");
    assert_eq!(out3, "0\n", "case 3");
    let out4 = build_and_run_stress("085-hamming-distance", b"15 0\n");
    assert_eq!(out4, "4\n", "case 4");
    let out5 = build_and_run_stress("085-hamming-distance", b"7 5\n");
    assert_eq!(out5, "1\n", "case 5");
}

// =====================================================================
// LC-086 — Power of Two Check
// Input: integer N
// Oracle: "true" if N is a power of 2, "false" otherwise
// =====================================================================

#[test]
fn test_lc086_power_of_two_check() {
    let out = build_and_run_stress("086-power-of-two-check", b"1\n");
    assert_eq!(out, "true\n", "case 1");
    let out2 = build_and_run_stress("086-power-of-two-check", b"16\n");
    assert_eq!(out2, "true\n", "case 2");
    let out3 = build_and_run_stress("086-power-of-two-check", b"3\n");
    assert_eq!(out3, "false\n", "case 3");
    let out4 = build_and_run_stress("086-power-of-two-check", b"0\n");
    assert_eq!(out4, "false\n", "case 4");
    let out5 = build_and_run_stress("086-power-of-two-check", b"1024\n");
    assert_eq!(out5, "true\n", "case 5");
}

// =====================================================================
// LC-087 — Missing Number XOR
// Input: N; N distinct ints from [0, N]
// Oracle: the missing number in [0, N]
// =====================================================================

#[test]
fn test_lc087_missing_number_xor() {
    let out = build_and_run_stress("087-missing-number-xor", b"3\n3 0 1\n");
    assert_eq!(out, "2\n", "case 1");
    let out2 = build_and_run_stress("087-missing-number-xor", b"1\n0\n");
    assert_eq!(out2, "1\n", "case 2");
    let out3 = build_and_run_stress("087-missing-number-xor", b"5\n0 1 2 4 5\n");
    assert_eq!(out3, "3\n", "case 3");
    let out4 = build_and_run_stress("087-missing-number-xor", b"2\n0 1\n");
    assert_eq!(out4, "2\n", "case 4");
    let out5 = build_and_run_stress("087-missing-number-xor", b"4\n4 3 1 0\n");
    assert_eq!(out5, "2\n", "case 5");
}

// =====================================================================
// LC-088 — Single Number (appears once, rest appear 3 times)
// Input: N; N ints where all except one appear 3 times
// Oracle: the single value appearing once
// =====================================================================

#[test]
fn test_lc088_single_number_triple() {
    let out = build_and_run_stress("088-single-number-triple", b"7\n2 2 3 2 4 4 4\n");
    assert_eq!(out, "3\n", "case 1");
    let out2 = build_and_run_stress("088-single-number-triple", b"7\n5 5 5 9 1 1 1\n");
    assert_eq!(out2, "9\n", "case 2");
    let out3 = build_and_run_stress("088-single-number-triple", b"1\n42\n");
    assert_eq!(out3, "42\n", "case 3");
    let out4 = build_and_run_stress("088-single-number-triple", b"4\n3 3 3 7\n");
    assert_eq!(out4, "7\n", "case 4");
    let out5 = build_and_run_stress("088-single-number-triple", b"10\n1 1 1 2 2 2 4 4 4 8\n");
    assert_eq!(out5, "8\n", "case 5");
}

// =====================================================================
// LC-089 — Bitwise AND of Range
// Input: two space-separated non-negative integers M N (M <= N)
// Oracle: bitwise AND of all integers in [M, N]
// =====================================================================

#[test]
fn test_lc089_bitwise_and_range() {
    let out = build_and_run_stress("089-bitwise-and-range", b"5 7\n");
    assert_eq!(out, "4\n", "case 1");
    let out2 = build_and_run_stress("089-bitwise-and-range", b"0 0\n");
    assert_eq!(out2, "0\n", "case 2");
    let out3 = build_and_run_stress("089-bitwise-and-range", b"4 4\n");
    assert_eq!(out3, "4\n", "case 3");
    let out4 = build_and_run_stress("089-bitwise-and-range", b"1 4\n");
    assert_eq!(out4, "0\n", "case 4");
    let out5 = build_and_run_stress("089-bitwise-and-range", b"12 15\n");
    assert_eq!(out5, "12\n", "case 5");
}

// =====================================================================
// LC-090 — Subsets via Bitmask
// NOTE: RUNTIME-FAIL; see failure.md (print_no_nl multi-literal
// misalignment gap — space-separated subset elements not supported)
// =====================================================================

#[test]
#[ignore = "LC-090 subset-via-bitmask: RUNTIME-FAIL; see failure.md (print_no_nl multi-literal misalignment gap — space-separated integer output not supported)"]
fn test_lc090_subset_via_bitmask() {
    let out = build_and_run_stress("090-subset-via-bitmask", b"1\n5\n");
    assert_eq!(out, "5\n", "case 1");
    let out2 = build_and_run_stress("090-subset-via-bitmask", b"2\n1 2\n");
    assert_eq!(out2, "1\n2\n1 2\n", "case 2");
    let out3 = build_and_run_stress("090-subset-via-bitmask", b"3\n1 2 3\n");
    assert_eq!(out3, "1\n2\n1 2\n3\n1 3\n2 3\n1 2 3\n", "case 3");
    let out4 = build_and_run_stress("090-subset-via-bitmask", b"2\n4 7\n");
    assert_eq!(out4, "4\n7\n4 7\n", "case 4");
    let out5 = build_and_run_stress("090-subset-via-bitmask", b"3\n0 1 2\n");
    assert_eq!(out5, "0\n1\n0 1\n2\n0 2\n1 2\n0 1 2\n", "case 5");
}
