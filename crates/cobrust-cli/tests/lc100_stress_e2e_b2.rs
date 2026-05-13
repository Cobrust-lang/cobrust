//! ADR-0047 LC-100 Tier A — Bucket B2 e2e harness (Stack/Queue + Linked list + Binary tree).
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
// LC-031 — Bracket Balancer Extended (includes < >)
// Input: one line of bracket string
// Oracle: "true" if balanced, "false" otherwise
// =====================================================================

#[test]
fn test_lc031_bracket_balancer_extended() {
    let out = build_and_run_stress("031-bracket-balancer-extended", b"({[<>]})\n");
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress("031-bracket-balancer-extended", b"({[}])\n");
    assert_eq!(out2, "false\n");
    let out3 = build_and_run_stress("031-bracket-balancer-extended", b"\n");
    assert_eq!(out3, "true\n");
    let out4 = build_and_run_stress("031-bracket-balancer-extended", b"<<<>>>\n");
    assert_eq!(out4, "true\n");
    let out5 = build_and_run_stress("031-bracket-balancer-extended", b"(<)>\n");
    assert_eq!(out5, "false\n");
}

// =====================================================================
// LC-032 — Min Stack Pair (two parallel stacks tracking running min)
// Input: N ops; "push X", "pop", "min"
// Oracle: one int per "min" query
// =====================================================================

#[test]
fn test_lc032_min_stack_pair() {
    let out = build_and_run_stress(
        "032-min-stack-pair",
        b"5\npush 3\npush 5\nmin\npush 1\nmin\n",
    );
    assert_eq!(out, "3\n1\n");
    let out2 = build_and_run_stress("032-min-stack-pair", b"3\npush 7\npush 2\nmin\n");
    assert_eq!(out2, "2\n");
    let out3 = build_and_run_stress(
        "032-min-stack-pair",
        b"6\npush 4\npush 4\nmin\npop\nmin\nmin\n",
    );
    assert_eq!(out3, "4\n4\n4\n");
}

// =====================================================================
// LC-033 — Next Greater Element (monotonic stack)
// Input: N; N space-separated ints
// Oracle: next greater to the right for each element (-1 if none)
// =====================================================================

#[test]
fn test_lc033_next_greater_element() {
    let out = build_and_run_stress("033-next-greater-element", b"5\n2 1 5 3 4\n");
    assert_eq!(out, "5\n5\n-1\n4\n-1\n");
    let out2 = build_and_run_stress("033-next-greater-element", b"4\n4 3 2 1\n");
    assert_eq!(out2, "-1\n-1\n-1\n-1\n");
    let out3 = build_and_run_stress("033-next-greater-element", b"4\n1 2 3 4\n");
    assert_eq!(out3, "2\n3\n4\n-1\n");
    let out4 = build_and_run_stress("033-next-greater-element", b"1\n42\n");
    assert_eq!(out4, "-1\n");
}

// =====================================================================
// LC-034 — Stack Sort Ascending (insertion sort via auxiliary stack)
// Input: N; N space-separated ints
// Oracle: values in ascending order
// =====================================================================

#[test]
fn test_lc034_stack_sort_ascending() {
    let out = build_and_run_stress("034-stack-sort-ascending", b"5\n5 2 7 1 4\n");
    assert_eq!(out, "1\n2\n4\n5\n7\n");
    let out2 = build_and_run_stress("034-stack-sort-ascending", b"3\n3 1 2\n");
    assert_eq!(out2, "1\n2\n3\n");
    let out3 = build_and_run_stress("034-stack-sort-ascending", b"1\n9\n");
    assert_eq!(out3, "9\n");
    let out4 = build_and_run_stress("034-stack-sort-ascending", b"4\n4 3 2 1\n");
    assert_eq!(out4, "1\n2\n3\n4\n");
}

// =====================================================================
// LC-035 — Queue via Two Stacks
// Input: N ops; "enqueue X", "dequeue", "peek"
// Oracle: one int per "dequeue" or "peek"
// =====================================================================

#[test]
fn test_lc035_queue_via_two_stacks() {
    let out = build_and_run_stress(
        "035-queue-via-two-stacks",
        b"6\nenqueue 1\nenqueue 2\nenqueue 3\ndequeue\npeek\ndequeue\n",
    );
    assert_eq!(out, "1\n2\n2\n");
    let out2 = build_and_run_stress(
        "035-queue-via-two-stacks",
        b"4\nenqueue 10\nenqueue 20\ndequeue\ndequeue\n",
    );
    assert_eq!(out2, "10\n20\n");
    let out3 = build_and_run_stress(
        "035-queue-via-two-stacks",
        b"5\nenqueue 5\ndequeue\nenqueue 7\nenqueue 3\ndequeue\n",
    );
    assert_eq!(out3, "5\n7\n");
}

// =====================================================================
// LC-036 — Daily Temperatures (monotonic stack for days until warmer)
// Input: N; N space-separated temperatures
// Oracle: days until warmer temp for each day (0 if none)
// =====================================================================

#[test]
fn test_lc036_daily_temperatures() {
    let out = build_and_run_stress("036-daily-temperatures", b"8\n73 74 75 71 69 72 76 73\n");
    assert_eq!(out, "1\n1\n4\n2\n1\n1\n0\n0\n");
    let out2 = build_and_run_stress("036-daily-temperatures", b"3\n30 40 50\n");
    assert_eq!(out2, "1\n1\n0\n");
    let out3 = build_and_run_stress("036-daily-temperatures", b"3\n30 60 90\n");
    assert_eq!(out3, "1\n1\n0\n");
    let out4 = build_and_run_stress("036-daily-temperatures", b"1\n55\n");
    assert_eq!(out4, "0\n");
}

// =====================================================================
// LC-037 — Reverse Polish Notation Evaluator
// Corpus corrected in Sprint 1; see corpus-corrected.md. All cases pass.
// =====================================================================

#[test]
fn test_lc037_reverse_polish_eval() {
    let out = build_and_run_stress("037-reverse-polish-eval", b"9\n5\n1\n2\n+\n4\n*\n+\n3\n-\n");
    assert_eq!(out, "14\n");
    let out2 = build_and_run_stress("037-reverse-polish-eval", b"3\n3\n4\n+\n");
    assert_eq!(out2, "7\n");
    let out3 = build_and_run_stress("037-reverse-polish-eval", b"5\n2\n1\n+\n3\n*\n");
    assert_eq!(out3, "9\n");
    let out4 = build_and_run_stress("037-reverse-polish-eval", b"5\n2\n3\n4\n*\n+\n");
    assert_eq!(out4, "14\n");
    let out5 = build_and_run_stress("037-reverse-polish-eval", b"5\n4\n13\n5\n/\n+\n");
    assert_eq!(out5, "6\n");
}

// =====================================================================
// LC-038 — Sliding Window Maximum (monotonic deque)
// Input: "N K" on line 1; N space-separated ints
// Oracle: max of each K-wide window
// =====================================================================

#[test]
fn test_lc038_sliding_window_max() {
    let out = build_and_run_stress("038-sliding-window-max", b"8 3\n1 3 -1 -3 5 3 6 7\n");
    assert_eq!(out, "3\n3\n5\n5\n6\n7\n");
    let out2 = build_and_run_stress("038-sliding-window-max", b"1 1\n1\n");
    assert_eq!(out2, "1\n");
    let out3 = build_and_run_stress("038-sliding-window-max", b"5 2\n1 2 3 4 5\n");
    assert_eq!(out3, "2\n3\n4\n5\n");
    let out4 = build_and_run_stress("038-sliding-window-max", b"5 3\n5 4 3 2 1\n");
    assert_eq!(out4, "5\n4\n3\n");
}

// =====================================================================
// LC-039 — Decode Nested Score by Depth
// Corpus corrected in Sprint 1; see corpus-corrected.md. All cases pass.
// =====================================================================

#[test]
fn test_lc039_decode_nested_depth() {
    let out = build_and_run_stress("039-decode-nested-depth", b"[]\n");
    assert_eq!(out, "1\n");
    let out2 = build_and_run_stress("039-decode-nested-depth", b"[[]]\n");
    assert_eq!(out2, "2\n");
    let out3 = build_and_run_stress("039-decode-nested-depth", b"[[][]]\n");
    assert_eq!(out3, "4\n");
    let out4 = build_and_run_stress("039-decode-nested-depth", b"[[]][]\n");
    assert_eq!(out4, "3\n");
    let out5 = build_and_run_stress("039-decode-nested-depth", b"[[[]]]\n");
    assert_eq!(out5, "4\n");
}

// =====================================================================
// LC-040 — Largest Rectangle in Histogram (monotonic stack)
// Input: N; N space-separated bar heights
// Oracle: maximum rectangle area
// =====================================================================

#[test]
fn test_lc040_largest_rectangle_histogram() {
    let out = build_and_run_stress("040-largest-rectangle-histogram", b"6\n2 1 5 6 2 3\n");
    assert_eq!(out, "10\n");
    let out2 = build_and_run_stress("040-largest-rectangle-histogram", b"2\n2 4\n");
    assert_eq!(out2, "4\n");
    let out3 = build_and_run_stress("040-largest-rectangle-histogram", b"1\n5\n");
    assert_eq!(out3, "5\n");
    let out4 = build_and_run_stress("040-largest-rectangle-histogram", b"5\n6 2 5 4 5\n");
    assert_eq!(out4, "12\n");
}

// =====================================================================
// LC-041 — Reverse Linked List (iterative pointer reversal)
// Input: N; N lines "val next_idx"
// Oracle: reversed values one per line
// =====================================================================

#[test]
fn test_lc041_reverse_linked_list() {
    let out = build_and_run_stress("041-reverse-linked-list", b"5\n1 1\n2 2\n3 3\n4 4\n5 -1\n");
    assert_eq!(out, "5\n4\n3\n2\n1\n");
    let out2 = build_and_run_stress("041-reverse-linked-list", b"2\n1 1\n2 -1\n");
    assert_eq!(out2, "2\n1\n");
    let out3 = build_and_run_stress("041-reverse-linked-list", b"1\n42 -1\n");
    assert_eq!(out3, "42\n");
    let out4 = build_and_run_stress("041-reverse-linked-list", b"3\n10 1\n20 2\n30 -1\n");
    assert_eq!(out4, "30\n20\n10\n");
}

// =====================================================================
// LC-042 — Linked List Cycle Detection (Floyd tortoise-hare)
// Input: "N tail_connect_idx"; N values one per line
// Oracle: "true" if cycle, "false" otherwise
// =====================================================================

#[test]
fn test_lc042_linked_list_cycle_detect() {
    let out = build_and_run_stress("042-linked-list-cycle-detect", b"4 1\n3\n2\n0\n-4\n");
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress("042-linked-list-cycle-detect", b"4 -1\n1\n2\n3\n4\n");
    assert_eq!(out2, "false\n");
    let out3 = build_and_run_stress("042-linked-list-cycle-detect", b"1 -1\n7\n");
    assert_eq!(out3, "false\n");
    let out4 = build_and_run_stress("042-linked-list-cycle-detect", b"2 0\n5\n6\n");
    assert_eq!(out4, "true\n");
}

// =====================================================================
// LC-043 — Remove Nth Node From End
// Input: "N K"; N values one per line
// Oracle: remaining values; empty list prints blank line
// =====================================================================

#[test]
fn test_lc043_remove_nth_from_end() {
    let out = build_and_run_stress("043-remove-nth-from-end", b"5 2\n1\n2\n3\n4\n5\n");
    assert_eq!(out, "1\n2\n3\n5\n");
    let out2 = build_and_run_stress("043-remove-nth-from-end", b"1 1\n9\n");
    assert_eq!(out2, "\n");
    let out3 = build_and_run_stress("043-remove-nth-from-end", b"2 1\n1\n2\n");
    assert_eq!(out3, "1\n");
    let out4 = build_and_run_stress("043-remove-nth-from-end", b"5 5\n1\n2\n3\n4\n5\n");
    assert_eq!(out4, "2\n3\n4\n5\n");
}

// =====================================================================
// LC-044 — Middle of Linked List (fast/slow pointer)
// Input: N; N values one per line
// Oracle: values from middle (second middle for even N) to end
// =====================================================================

#[test]
fn test_lc044_middle_of_linked_list() {
    let out = build_and_run_stress("044-middle-of-linked-list", b"5\n1\n2\n3\n4\n5\n");
    assert_eq!(out, "3\n4\n5\n");
    let out2 = build_and_run_stress("044-middle-of-linked-list", b"6\n1\n2\n3\n4\n5\n6\n");
    assert_eq!(out2, "4\n5\n6\n");
    let out3 = build_and_run_stress("044-middle-of-linked-list", b"1\n7\n");
    assert_eq!(out3, "7\n");
    let out4 = build_and_run_stress("044-middle-of-linked-list", b"2\n4\n8\n");
    assert_eq!(out4, "8\n");
}

// =====================================================================
// LC-045 — Linked List Palindrome Check
// Input: N; N values one per line
// Oracle: "true" if palindrome, "false" otherwise
// =====================================================================

#[test]
fn test_lc045_linked_list_palindrome() {
    let out = build_and_run_stress("045-linked-list-palindrome", b"5\n1\n2\n3\n2\n1\n");
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress("045-linked-list-palindrome", b"5\n1\n2\n3\n4\n5\n");
    assert_eq!(out2, "false\n");
    let out3 = build_and_run_stress("045-linked-list-palindrome", b"4\n1\n2\n2\n1\n");
    assert_eq!(out3, "true\n");
    let out4 = build_and_run_stress("045-linked-list-palindrome", b"1\n1\n");
    assert_eq!(out4, "true\n");
    let out5 = build_and_run_stress("045-linked-list-palindrome", b"2\n1\n2\n");
    assert_eq!(out5, "false\n");
}

// =====================================================================
// LC-046 — Remove Duplicates from Sorted Linked List
// Input: N; N values one per line (sorted)
// Oracle: unique values in order
// =====================================================================

#[test]
fn test_lc046_remove_duplicates_linked_list() {
    let out = build_and_run_stress(
        "046-remove-duplicates-linked-list",
        b"6\n1\n1\n2\n3\n3\n3\n",
    );
    assert_eq!(out, "1\n2\n3\n");
    let out2 = build_and_run_stress("046-remove-duplicates-linked-list", b"4\n1\n1\n2\n2\n");
    assert_eq!(out2, "1\n2\n");
    let out3 = build_and_run_stress("046-remove-duplicates-linked-list", b"3\n1\n2\n3\n");
    assert_eq!(out3, "1\n2\n3\n");
    let out4 = build_and_run_stress("046-remove-duplicates-linked-list", b"1\n5\n");
    assert_eq!(out4, "5\n");
}

// =====================================================================
// LC-047 — Merge K Sorted Lists (selection sort via K pointers)
// Input: K; K lines each "count v0 v1..."
// Oracle: all values merged sorted
// =====================================================================

#[test]
fn test_lc047_merge_k_sorted_lists() {
    let out = build_and_run_stress("047-merge-k-sorted-lists", b"3\n3 1 4 5\n3 1 3 4\n2 2 6\n");
    assert_eq!(out, "1\n1\n2\n3\n4\n4\n5\n6\n");
    let out2 = build_and_run_stress("047-merge-k-sorted-lists", b"2\n2 1 3\n2 2 4\n");
    assert_eq!(out2, "1\n2\n3\n4\n");
    let out3 = build_and_run_stress("047-merge-k-sorted-lists", b"1\n3 5 7 9\n");
    assert_eq!(out3, "5\n7\n9\n");
    let out4 = build_and_run_stress("047-merge-k-sorted-lists", b"3\n1 0\n1 0\n1 0\n");
    assert_eq!(out4, "0\n0\n0\n");
}

// =====================================================================
// LC-048 — Reorder Linked List (interleave front and back)
// Input: N; N values one per line
// Oracle: L0→Ln→L1→Ln-1→...
// =====================================================================

#[test]
fn test_lc048_reorder_linked_list() {
    let out = build_and_run_stress("048-reorder-linked-list", b"5\n1\n2\n3\n4\n5\n");
    assert_eq!(out, "1\n5\n2\n4\n3\n");
    let out2 = build_and_run_stress("048-reorder-linked-list", b"4\n1\n2\n3\n4\n");
    assert_eq!(out2, "1\n4\n2\n3\n");
    let out3 = build_and_run_stress("048-reorder-linked-list", b"1\n7\n");
    assert_eq!(out3, "7\n");
    let out4 = build_and_run_stress("048-reorder-linked-list", b"2\n3\n5\n");
    assert_eq!(out4, "3\n5\n");
}

// =====================================================================
// LC-049 — Intersection of Two Linked Lists
// Input: "pre1 pre2 shared"; then pre1+pre2+shared values
// Oracle: first shared node value (-1 if no intersection)
// =====================================================================

#[test]
fn test_lc049_intersection_two_lists() {
    let out = build_and_run_stress(
        "049-intersection-two-lists",
        b"2 3 3\n4\n1\n5\n6\n1\n8\n4\n5\n",
    );
    assert_eq!(out, "8\n");
    let out2 = build_and_run_stress("049-intersection-two-lists", b"1 1 2\n2\n4\n1\n8\n");
    assert_eq!(out2, "1\n");
    let out3 = build_and_run_stress("049-intersection-two-lists", b"2 3 0\n1\n9\n3\n2\n4\n");
    assert_eq!(out3, "-1\n");
    let out4 = build_and_run_stress("049-intersection-two-lists", b"0 0 3\n7\n8\n9\n");
    assert_eq!(out4, "7\n");
}

// =====================================================================
// LC-050 — Rotate Linked List Right by K
// Input: "N K"; N values one per line
// Oracle: rotated values
// =====================================================================

#[test]
fn test_lc050_rotate_linked_list() {
    let out = build_and_run_stress("050-rotate-linked-list", b"5 2\n1\n2\n3\n4\n5\n");
    assert_eq!(out, "4\n5\n1\n2\n3\n");
    let out2 = build_and_run_stress("050-rotate-linked-list", b"5 5\n1\n2\n3\n4\n5\n");
    assert_eq!(out2, "1\n2\n3\n4\n5\n");
    let out3 = build_and_run_stress("050-rotate-linked-list", b"3 1\n0\n1\n2\n");
    assert_eq!(out3, "2\n0\n1\n");
    let out4 = build_and_run_stress("050-rotate-linked-list", b"1 99\n7\n");
    assert_eq!(out4, "7\n");
}

// =====================================================================
// LC-051 — Binary Tree Maximum Depth (iterative DFS)
// Input: N; N lines "val left_idx right_idx"
// Oracle: maximum depth
// =====================================================================

#[test]
fn test_lc051_binary_tree_max_depth() {
    let out = build_and_run_stress(
        "051-binary-tree-max-depth",
        b"5\n1 1 2\n2 3 4\n3 -1 -1\n4 -1 -1\n5 -1 -1\n",
    );
    assert_eq!(out, "3\n");
    let out2 = build_and_run_stress("051-binary-tree-max-depth", b"1\n42 -1 -1\n");
    assert_eq!(out2, "1\n");
    let out3 = build_and_run_stress("051-binary-tree-max-depth", b"3\n1 1 2\n2 -1 -1\n3 -1 -1\n");
    assert_eq!(out3, "2\n");
    let out4 = build_and_run_stress(
        "051-binary-tree-max-depth",
        b"4\n1 1 -1\n2 2 -1\n3 3 -1\n4 -1 -1\n",
    );
    assert_eq!(out4, "4\n");
}

// =====================================================================
// LC-052 — Invert Binary Tree (BFS traversal output after swap)
// Input: N; N lines "val left_idx right_idx"
// Oracle: BFS values after inverting
// =====================================================================

#[test]
fn test_lc052_invert_binary_tree() {
    let out = build_and_run_stress("052-invert-binary-tree", b"3\n4 1 2\n2 -1 -1\n7 -1 -1\n");
    assert_eq!(out, "4\n7\n2\n");
    let out2 = build_and_run_stress("052-invert-binary-tree", b"1\n1 -1 -1\n");
    assert_eq!(out2, "1\n");
    let out3 = build_and_run_stress(
        "052-invert-binary-tree",
        b"7\n4 1 2\n2 3 4\n7 5 6\n9 -1 -1\n6 -1 -1\n3 -1 -1\n1 -1 -1\n",
    );
    assert_eq!(out3, "4\n7\n2\n1\n3\n6\n9\n");
}

// =====================================================================
// LC-053 — Symmetric Tree Check
// Corpus corrected in Sprint 1; see corpus-corrected.md. All cases pass.
// =====================================================================

#[test]
fn test_lc053_symmetric_tree() {
    let out = build_and_run_stress(
        "053-symmetric-tree",
        b"7\n1 1 2\n2 3 4\n2 5 6\n3 -1 -1\n4 -1 -1\n4 -1 -1\n3 -1 -1\n",
    );
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress(
        "053-symmetric-tree",
        b"5\n1 1 2\n2 3 -1\n2 -1 4\n3 -1 -1\n3 -1 -1\n",
    );
    assert_eq!(out2, "true\n");
    let out3 = build_and_run_stress("053-symmetric-tree", b"1\n5 -1 -1\n");
    assert_eq!(out3, "true\n");
    let out4 = build_and_run_stress("053-symmetric-tree", b"3\n1 1 2\n2 -1 -1\n2 -1 -1\n");
    assert_eq!(out4, "true\n");
}

// =====================================================================
// LC-054 — Path Sum Exists (root-to-leaf DFS)
// Corpus corrected in Sprint 1; see corpus-corrected.md. All cases pass.
// =====================================================================

#[test]
fn test_lc054_path_sum_exists() {
    let out = build_and_run_stress(
        "054-path-sum-exists",
        b"8 22\n5 1 2\n4 3 -1\n8 4 5\n11 6 7\n13 -1 -1\n4 -1 -1\n7 -1 -1\n2 -1 -1\n",
    );
    assert_eq!(out, "true\n");
    let out2 = build_and_run_stress(
        "054-path-sum-exists",
        b"8 5\n5 1 2\n4 3 -1\n8 4 5\n11 6 7\n13 -1 -1\n4 -1 -1\n7 -1 -1\n2 -1 -1\n",
    );
    assert_eq!(out2, "false\n");
    let out3 = build_and_run_stress("054-path-sum-exists", b"1 0\n0 -1 -1\n");
    assert_eq!(out3, "true\n");
    let out4 = build_and_run_stress("054-path-sum-exists", b"3 1\n1 1 2\n2 -1 -1\n3 -1 -1\n");
    assert_eq!(out4, "false\n");
}

// =====================================================================
// LC-055 — Count Nodes in Binary Tree
// Input: N; N lines "val left_idx right_idx"
// Oracle: N (total node count)
// =====================================================================

#[test]
fn test_lc055_count_nodes() {
    let out = build_and_run_stress(
        "055-count-nodes",
        b"7\n1 1 2\n2 3 4\n3 5 6\n4 -1 -1\n5 -1 -1\n6 -1 -1\n7 -1 -1\n",
    );
    assert_eq!(out, "7\n");
    let out2 = build_and_run_stress("055-count-nodes", b"1\n1 -1 -1\n");
    assert_eq!(out2, "1\n");
    let out3 = build_and_run_stress("055-count-nodes", b"3\n1 1 2\n2 -1 -1\n3 -1 -1\n");
    assert_eq!(out3, "3\n");
    let out4 = build_and_run_stress(
        "055-count-nodes",
        b"5\n1 1 2\n2 3 4\n3 -1 -1\n4 -1 -1\n5 -1 -1\n",
    );
    assert_eq!(out4, "5\n");
}

// =====================================================================
// LC-056 — Binary Tree Level Order Traversal
// NOTE: RUNTIME-FAIL; print_no_nl panics with misaligned pointer on all cases.
// see failure.md for root cause (codegen/stdlib gap: short string literal alignment).
// =====================================================================

#[test]
fn test_lc056_level_order_traversal() {
    let out = build_and_run_stress(
        "056-level-order-traversal",
        b"7\n1 1 2\n2 3 4\n3 5 6\n4 -1 -1\n5 -1 -1\n6 -1 -1\n7 -1 -1\n",
    );
    assert_eq!(out, "1\n2 3\n4 5 6 7\n");
    let out2 = build_and_run_stress("056-level-order-traversal", b"1\n5 -1 -1\n");
    assert_eq!(out2, "5\n");
    let out3 = build_and_run_stress("056-level-order-traversal", b"3\n1 1 2\n2 -1 -1\n3 -1 -1\n");
    assert_eq!(out3, "1\n2 3\n");
    let out4 = build_and_run_stress(
        "056-level-order-traversal",
        b"4\n1 1 -1\n2 2 -1\n3 3 -1\n4 -1 -1\n",
    );
    assert_eq!(out4, "1\n2\n3\n4\n");
}

// =====================================================================
// LC-057 — Lowest Common Ancestor
// Corpus corrected in Sprint 1; see corpus-corrected.md. All cases pass.
// =====================================================================

#[test]
fn test_lc057_lowest_common_ancestor() {
    let out = build_and_run_stress(
        "057-lowest-common-ancestor",
        b"7 3 4\n6 1 2\n2 3 4\n8 -1 -1\n0 5 6\n7 -1 -1\n4 -1 -1\n5 -1 -1\n",
    );
    assert_eq!(out, "2\n");
    let out2 = build_and_run_stress(
        "057-lowest-common-ancestor",
        b"7 1 4\n6 1 2\n2 3 4\n8 -1 -1\n0 5 6\n7 -1 -1\n4 -1 -1\n5 -1 -1\n",
    );
    assert_eq!(out2, "2\n");
    let out3 = build_and_run_stress(
        "057-lowest-common-ancestor",
        b"3 0 2\n1 1 2\n2 -1 -1\n3 -1 -1\n",
    );
    assert_eq!(out3, "1\n");
    let out4 = build_and_run_stress(
        "057-lowest-common-ancestor",
        b"3 1 2\n1 1 2\n2 -1 -1\n3 -1 -1\n",
    );
    assert_eq!(out4, "1\n");
}

// =====================================================================
// LC-058 — Diameter of Binary Tree (iterative post-order DFS)
// Input: N; N lines "val left_idx right_idx"
// Oracle: longest path in edges
// =====================================================================

#[test]
fn test_lc058_diameter_of_tree() {
    let out = build_and_run_stress(
        "058-diameter-of-tree",
        b"5\n1 1 2\n2 3 4\n3 -1 -1\n4 -1 -1\n5 -1 -1\n",
    );
    assert_eq!(out, "3\n");
    let out2 = build_and_run_stress("058-diameter-of-tree", b"1\n1 -1 -1\n");
    assert_eq!(out2, "0\n");
    let out3 = build_and_run_stress("058-diameter-of-tree", b"3\n1 1 2\n2 -1 -1\n3 -1 -1\n");
    assert_eq!(out3, "2\n");
    let out4 = build_and_run_stress(
        "058-diameter-of-tree",
        b"4\n1 1 -1\n2 2 -1\n3 3 -1\n4 -1 -1\n",
    );
    assert_eq!(out4, "3\n");
}

// =====================================================================
// LC-059 — Flatten Binary Tree to Linked List (pre-order DFS)
// Corpus corrected in Sprint 1; see corpus-corrected.md. All cases pass.
// =====================================================================

#[test]
fn test_lc059_flatten_tree_to_list() {
    let out = build_and_run_stress(
        "059-flatten-tree-to-list",
        b"6\n1 1 2\n2 3 4\n5 -1 5\n3 -1 -1\n4 -1 -1\n6 -1 -1\n",
    );
    assert_eq!(out, "1\n2\n3\n4\n5\n6\n");
    let out2 = build_and_run_stress("059-flatten-tree-to-list", b"1\n0 -1 -1\n");
    assert_eq!(out2, "0\n");
    let out3 = build_and_run_stress("059-flatten-tree-to-list", b"3\n1 1 2\n2 -1 -1\n3 -1 -1\n");
    assert_eq!(out3, "1\n2\n3\n");
    let out4 = build_and_run_stress(
        "059-flatten-tree-to-list",
        b"4\n1 1 -1\n2 2 -1\n3 3 -1\n4 -1 -1\n",
    );
    assert_eq!(out4, "1\n2\n3\n4\n");
}

// =====================================================================
// LC-060 — Binary Tree Right Side View (BFS level-end tracking)
// Input: N; N lines "val left_idx right_idx"
// Oracle: rightmost value at each level
// =====================================================================

#[test]
fn test_lc060_right_side_view() {
    let out = build_and_run_stress(
        "060-right-side-view",
        b"5\n1 1 2\n2 3 4\n3 -1 -1\n5 -1 -1\n4 -1 -1\n",
    );
    assert_eq!(out, "1\n3\n4\n");
    let out2 = build_and_run_stress("060-right-side-view", b"3\n1 1 2\n2 -1 -1\n3 -1 -1\n");
    assert_eq!(out2, "1\n3\n");
    let out3 = build_and_run_stress("060-right-side-view", b"1\n1 -1 -1\n");
    assert_eq!(out3, "1\n");
    let out4 = build_and_run_stress(
        "060-right-side-view",
        b"4\n1 1 -1\n2 2 -1\n3 3 -1\n4 -1 -1\n",
    );
    assert_eq!(out4, "1\n2\n3\n4\n");
}
