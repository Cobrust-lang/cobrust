//! M11.1.1 control-flow combination corpus.
//!
//! Per review-claude 二次审计 2026-05-09 §3 — extends the M11.1
//! while_if_corpus to cover the broader control-flow surface that
//! M12.x's for-protocol + Aggregate/Ref/Cast lowering opened up.
//! Goal: ≥30 cases such that audit #1 (tomli real-LLM E2E)
//! failures can be attributed to translation quality, not codegen
//! blind spots.

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
#![allow(clippy::approx_constant)]
#![allow(clippy::stable_sort_primitive)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn cobrust_binary() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root from CARGO_MANIFEST_DIR");
    let debug_bin = workspace.join("target/debug/cobrust");
    if debug_bin.exists() {
        return debug_bin;
    }
    let release_bin = workspace.join("target/release/cobrust");
    if release_bin.exists() {
        return release_bin;
    }
    PathBuf::from("cobrust")
}

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

/// Write a `.cb` source file to a temp dir; return its path.
fn write_temp(name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cobrust-m11-1-1-corpus-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let p = dir.join(format!("{name}.cb"));
    std::fs::write(&p, contents).expect("write temp .cb");
    p
}

/// Build the source file with the `cobrust` binary; return the path to the
/// produced executable. Panics with a helpful message on failure.
fn build(name: &str, src_path: &Path) -> PathBuf {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let exe_dir = std::env::temp_dir().join(format!(
        "cobrust-m11-1-1-exe-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&exe_dir);
    let exe_path = exe_dir.join(name);

    let out = Command::new(&bin)
        .arg("build")
        .arg(src_path)
        .arg("-o")
        .arg(&exe_path)
        .arg("--quiet")
        .current_dir(&workspace)
        .output()
        .expect("invoke cobrust build");
    assert!(
        out.status.success(),
        "cobrust build failed for {name}:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    exe_path
}

/// Run a produced executable; return its stdout as a String.
fn run(exe_path: &Path) -> String {
    let out = Command::new(exe_path)
        .output()
        .expect("invoke produced executable");
    assert!(
        out.status.success(),
        "binary {} exited non-zero ({:?})\nstderr={}",
        exe_path.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// =====================================================================
// Category A: Nested loops (4 cases)
// =====================================================================

// A1: while inside while — sum of nested loops.
// Outer i=0..2 (3 iters), inner j=0..3 (4 iters): total increments = 12.
#[test]
fn a1_while_inside_while_sum() {
    let src = write_temp(
        "a1_nested_while_sum",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let total: i64 = 0\n\
         \x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20while i < 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let j: i64 = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while j < 4:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20total = total + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20j = j + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(total)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("a1_nested_while_sum", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "12\n", "a1 stdout mismatch: {stdout:?}");
}

// A2: 3-deep nested while.
// i=0..1 (2), j=0..2 (3), k=0..3 (4): count = 2*3*4 = 24.
#[test]
fn a2_three_deep_nested_while() {
    let src = write_temp(
        "a2_three_deep",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let count: i64 = 0\n\
         \x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20while i < 2:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let j: i64 = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while j < 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20let k: i64 = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20while k < 4:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20count = count + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20k = k + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20j = j + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(count)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("a2_three_deep", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "24\n", "a2 stdout mismatch: {stdout:?}");
}

// A3: while-while with mutation flowing through both levels.
// outer_sum += i for i in 0..2 → 0+1+2 = 3.
// inner_sum += j for j in 0..i: i=0 → 0, i=1 → 0, i=2 → 0+1=1. Total inner=1.
#[test]
fn a3_nested_while_mutation_both_levels() {
    let src = write_temp(
        "a3_nested_mutation",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let outer_sum: i64 = 0\n\
         \x20\x20\x20\x20let inner_sum: i64 = 0\n\
         \x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20while i < 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20outer_sum = outer_sum + i\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let j: i64 = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while j < i:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20inner_sum = inner_sum + j\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20j = j + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(outer_sum)\n\
         \x20\x20\x20\x20print(inner_sum)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("a3_nested_mutation", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "3\n1\n", "a3 stdout mismatch: {stdout:?}");
}

// A4: while-while — inner uses flag to stop counting early, outer continues.
// Outer i=0..2; inner j=0..3; flag fires when j==2, halting total_inner increments.
// Per outer iteration: j=0 (+1), j=1 (+1), j=2 (done=1), j=3 (skip) → 2 increments.
// outer_count=3, total_inner=6.
#[test]
fn a4_nested_while_inner_flag_exit() {
    let src = write_temp(
        "a4_nested_flag",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let outer_count: i64 = 0\n\
         \x20\x20\x20\x20let total_inner: i64 = 0\n\
         \x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20while i < 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20outer_count = outer_count + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let j: i64 = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let done: i64 = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while j < 4:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20if done == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20if j == 2:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20done = 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20total_inner = total_inner + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20j = j + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(outer_count)\n\
         \x20\x20\x20\x20print(total_inner)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("a4_nested_flag", &src);
    let stdout = run(&exe);
    // outer_count=3; per iteration inner: j=0→total+1, j=1→total+1, j=2→done=1, j=3→skip → +2 each
    assert_eq!(stdout, "3\n6\n", "a4 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Category B: Loop + branching (5 cases)
// =====================================================================

// B1: if-else inside while — count even vs odd numbers in 1..6.
// evens={2,4,6}→3, odds={1,3,5}→3.
#[test]
fn b1_if_else_inside_while_even_odd() {
    let src = write_temp(
        "b1_if_else_while",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let evens: i64 = 0\n\
         \x20\x20\x20\x20let odds: i64 = 0\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 6:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if n % 2 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20evens = evens + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20odds = odds + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20print(evens)\n\
         \x20\x20\x20\x20print(odds)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("b1_if_else_while", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "3\n3\n", "b1 stdout mismatch: {stdout:?}");
}

// B2: if/elif/else inside while — classify n=1..9 into low/mid/high.
// low (≤3): 3, mid (4..6): 3, high (7..9): 3.
#[test]
fn b2_if_elif_else_inside_while() {
    let src = write_temp(
        "b2_elif_while",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let low: i64 = 0\n\
         \x20\x20\x20\x20let mid: i64 = 0\n\
         \x20\x20\x20\x20let high: i64 = 0\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 9:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if n <= 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20low = low + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20elif n <= 6:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20mid = mid + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20high = high + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20print(low)\n\
         \x20\x20\x20\x20print(mid)\n\
         \x20\x20\x20\x20print(high)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("b2_elif_while", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "3\n3\n3\n", "b2 stdout mismatch: {stdout:?}");
}

// B3: while inside if (only execute loop on a branch).
// n=5 ≥ 0 → sum 0+1+2+3+4 = 10; else path prints 0.
#[test]
fn b3_while_inside_if_branch() {
    let src = write_temp(
        "b3_while_in_if",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 5\n\
         \x20\x20\x20\x20let sum: i64 = 0\n\
         \x20\x20\x20\x20if n >= 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while i < n:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20sum = sum + i\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20sum = 0\n\
         \x20\x20\x20\x20print(sum)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("b3_while_in_if", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "10\n", "b3 stdout mismatch: {stdout:?}");
}

// B4: while inside else (loop only runs on the else branch).
// n=4 ≥ 0 → else branch: sum 1+2+3+4 = 10.
#[test]
fn b4_while_inside_else_branch() {
    let src = write_temp(
        "b4_while_in_else",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 4\n\
         \x20\x20\x20\x20let result: i64 = 0\n\
         \x20\x20\x20\x20if n < 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20result = 0\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let i: i64 = 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while i <= n:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20result = result + i\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(result)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("b4_while_in_else", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "10\n", "b4 stdout mismatch: {stdout:?}");
}

// B5: nested if inside while inside if.
// mode=1 → outer if taken; inner loop 1..5, add even (2+4=6).
#[test]
fn b5_nested_if_while_if() {
    let src = write_temp(
        "b5_nested_if_while_if",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let mode: i64 = 1\n\
         \x20\x20\x20\x20let result: i64 = 0\n\
         \x20\x20\x20\x20if mode == 1:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while n <= 5:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20if n % 2 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20result = result + n\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20result = 99\n\
         \x20\x20\x20\x20print(result)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("b5_nested_if_while_if", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "6\n", "b5 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Category C: break + continue (6 cases)
// (Cobrust supports both: KwBreak/KwContinue → HIR Break/Continue → MIR terminators)
// =====================================================================

// C1: break inside while (early exit).
// i=0..9; break at i==5. count incremented for i=0..4 → 5.
#[test]
fn c1_break_inside_while() {
    let src = write_temp(
        "c1_break",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let count: i64 = 0\n\
         \x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20while i < 10:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if i == 5:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20break\n\
         \x20\x20\x20\x20\x20\x20\x20\x20count = count + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(count)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("c1_break", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "5\n", "c1 stdout mismatch: {stdout:?}");
}

// C2: break inside while inside if (conditional early exit).
// flag=1 → first branch: break at i==3, count=3 (i=0,1,2).
#[test]
fn c2_break_inside_while_inside_if() {
    let src = write_temp(
        "c2_break_in_if",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let flag: i64 = 1\n\
         \x20\x20\x20\x20let count: i64 = 0\n\
         \x20\x20\x20\x20if flag == 1:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while i < 10:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20if i == 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20break\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20count = count + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while i < 10:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20if i == 7:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20break\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20count = count + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(count)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("c2_break_in_if", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "3\n", "c2 stdout mismatch: {stdout:?}");
}

// C3: break inside nested while — only inner breaks; outer continues.
// Outer i=0..2; inner j=0..8 breaks at j==2.
// Per outer iteration: j=0,1 counted (+2), then j==2 breaks. outer_count=3, inner_count=6.
#[test]
fn c3_break_inner_only_nested_while() {
    let src = write_temp(
        "c3_break_inner",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let outer_count: i64 = 0\n\
         \x20\x20\x20\x20let inner_count: i64 = 0\n\
         \x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20while i < 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20outer_count = outer_count + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let j: i64 = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while j < 9:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20if j == 2:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20break\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20inner_count = inner_count + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20j = j + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(outer_count)\n\
         \x20\x20\x20\x20print(inner_count)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("c3_break_inner", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "3\n6\n", "c3 stdout mismatch: {stdout:?}");
}

// C4: continue inside while (skip multiples of 3).
// Sum 1..10 excluding {3,6,9}: 55 - 18 = 37.
#[test]
fn c4_continue_inside_while() {
    let src = write_temp(
        "c4_continue",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let sum: i64 = 0\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 10:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if n % 3 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20continue\n\
         \x20\x20\x20\x20\x20\x20\x20\x20sum = sum + n\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20print(sum)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("c4_continue", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "37\n", "c4 stdout mismatch: {stdout:?}");
}

// C5: continue inside if-inside-while.
// Sum 1..8 excluding {3,6}: 1+2+4+5+7+8 = 27.
#[test]
fn c5_continue_inside_if_inside_while() {
    let src = write_temp(
        "c5_continue_if",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let sum: i64 = 0\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 8:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if n % 3 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20continue\n\
         \x20\x20\x20\x20\x20\x20\x20\x20sum = sum + n\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20print(sum)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("c5_continue_if", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "27\n", "c5 stdout mismatch: {stdout:?}");
}

// C6: break followed by post-loop computation.
// i=0..8; break at i==4; result = i * 10 = 40.
#[test]
fn c6_break_then_post_loop_computation() {
    let src = write_temp(
        "c6_break_post",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20while i < 9:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if i == 4:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20break\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20let result: i64 = i * 10\n\
         \x20\x20\x20\x20print(result)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("c6_break_post", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "40\n", "c6 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Category D: early return from inside loop (3 cases)
// =====================================================================

// D1: return inside while body (early termination).
// First n where n*n > 50: 7*7=49 ≤ 50, 8*8=64 > 50 → prints 8.
#[test]
fn d1_return_inside_while() {
    let src = write_temp(
        "d1_return_while",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 100:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if n * n > 50:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(n)\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20return 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20print(\"not found\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("d1_return_while", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "8\n", "d1 stdout mismatch: {stdout:?}");
}

// D2: return inside if-inside-while (early termination on condition).
// Sum 1..N; stop when sum > 15. 1+2+3+4+5+6=21 > 15 → prints 6.
#[test]
fn d2_return_inside_if_inside_while() {
    let src = write_temp(
        "d2_return_if_while",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let sum: i64 = 0\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 100:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20sum = sum + n\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if sum > 15:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(n)\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20return 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20print(0)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("d2_return_if_while", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "6\n", "d2 stdout mismatch: {stdout:?}");
}

// D3: return inside nested while (returns from both inner and outer).
// Find first pair (i,j) with i*j==12 and i < j (i ≥ 1).
// i=1, j=2..12: 1*12=12 ✓ → prints 1 then 12.
#[test]
fn d3_return_inside_nested_while() {
    let src = write_temp(
        "d3_return_nested",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let i: i64 = 1\n\
         \x20\x20\x20\x20while i < 12:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let j: i64 = i + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while j <= 12:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20if i * j == 12:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(i)\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(j)\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20return 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20j = j + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(\"none\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("d3_return_nested", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "1\n12\n", "d3 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Category E: mutation patterns (4 cases)
// =====================================================================

// E1: counter accumulator — sum 1..100 = 5050.
#[test]
fn e1_counter_accumulator() {
    let src = write_temp(
        "e1_accumulator",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let sum: i64 = 0\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 100:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20sum = sum + n\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20print(sum)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("e1_accumulator", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "5050\n", "e1 stdout mismatch: {stdout:?}");
}

// E2: two-variable swap inside while (Fibonacci-like).
// fib(10) = 55 (a=0, b=1, 10 iterations).
#[test]
fn e2_two_variable_fibonacci_like() {
    let src = write_temp(
        "e2_fib_like",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: i64 = 0\n\
         \x20\x20\x20\x20let b: i64 = 1\n\
         \x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20while i < 10:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let c: i64 = a + b\n\
         \x20\x20\x20\x20\x20\x20\x20\x20a = b\n\
         \x20\x20\x20\x20\x20\x20\x20\x20b = c\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(a)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("e2_fib_like", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "55\n", "e2 stdout mismatch: {stdout:?}");
}

// E3: nested counter (i, j double-loop accumulation).
// sum of i+j for i in 0..1, j in 0..1 = (0+0)+(0+1)+(1+0)+(1+1) = 4.
#[test]
fn e3_nested_counter_double_loop() {
    let src = write_temp(
        "e3_nested_counter",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let total: i64 = 0\n\
         \x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20while i < 2:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let j: i64 = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20while j < 2:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20total = total + i + j\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20j = j + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(total)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("e3_nested_counter", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "4\n", "e3 stdout mismatch: {stdout:?}");
}

// E4: conditional mutation (running max).
// Sequence [3,1,4,1,5,9,2,6]: running max after all = 9.
#[test]
fn e4_conditional_mutation_running_max() {
    let src = write_temp(
        "e4_running_max",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let mx: i64 = 0\n\
         \x20\x20\x20\x20let n: i64 = 3\n\
         \x20\x20\x20\x20if n > mx:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20mx = n\n\
         \x20\x20\x20\x20n = 1\n\
         \x20\x20\x20\x20if n > mx:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20mx = n\n\
         \x20\x20\x20\x20n = 4\n\
         \x20\x20\x20\x20if n > mx:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20mx = n\n\
         \x20\x20\x20\x20n = 1\n\
         \x20\x20\x20\x20if n > mx:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20mx = n\n\
         \x20\x20\x20\x20n = 5\n\
         \x20\x20\x20\x20if n > mx:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20mx = n\n\
         \x20\x20\x20\x20n = 9\n\
         \x20\x20\x20\x20if n > mx:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20mx = n\n\
         \x20\x20\x20\x20n = 2\n\
         \x20\x20\x20\x20if n > mx:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20mx = n\n\
         \x20\x20\x20\x20n = 6\n\
         \x20\x20\x20\x20if n > mx:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20mx = n\n\
         \x20\x20\x20\x20print(mx)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("e4_running_max", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "9\n", "e4 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Category F: modulo + control flow (3 cases)
// =====================================================================

// F1: modulo cascade inside while — FizzBuzz extended to 1..20.
#[test]
fn f1_fizzbuzz_extended_to_20() {
    let src = write_temp(
        "f1_fizzbuzz20",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 20:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if n % 15 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(\"FizzBuzz\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20elif n % 3 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(\"Fizz\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20elif n % 5 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(\"Buzz\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(n)\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("f1_fizzbuzz20", &src);
    let stdout = run(&exe);
    let expected = "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n16\n17\nFizz\n19\nBuzz\n";
    assert_eq!(
        stdout, expected,
        "f1 stdout mismatch:\ngot:      {stdout:?}\nexpected: {expected:?}"
    );
}

// F2: modulo + if + break — find first multiple of 7 in 1..100 (= 7).
#[test]
fn f2_modulo_if_break_find_first_multiple() {
    let src = write_temp(
        "f2_find_mult7",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 100:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if n % 7 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(n)\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20break\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("f2_find_mult7", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "7\n", "f2 stdout mismatch: {stdout:?}");
}

// F3: nested modulo — count divisors of 12.
// Divisors: 1,2,3,4,6,12 → count = 6.
#[test]
fn f3_nested_modulo_count_divisors() {
    let src = write_temp(
        "f3_divisors",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let target: i64 = 12\n\
         \x20\x20\x20\x20let count: i64 = 0\n\
         \x20\x20\x20\x20let d: i64 = 1\n\
         \x20\x20\x20\x20while d <= target:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if target % d == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20count = count + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20d = d + 1\n\
         \x20\x20\x20\x20print(count)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("f3_divisors", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "6\n", "f3 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Category G: ill-formed / negative regression checks (5 cases)
// =====================================================================

// G1: while with pass body that never executes (always-false condition).
// n=0, while n > 10 → false immediately. print 42.
#[test]
fn g1_empty_while_body_pass() {
    let src = write_temp(
        "g1_empty_while",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 0\n\
         \x20\x20\x20\x20while n > 10:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20pass\n\
         \x20\x20\x20\x20print(42)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("g1_empty_while", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "42\n", "g1 stdout mismatch: {stdout:?}");
}

// G2: while with always-false condition — body never executes; sentinel stays 99.
#[test]
fn g2_while_always_false_condition() {
    let src = write_temp(
        "g2_always_false_while",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let sentinel: i64 = 99\n\
         \x20\x20\x20\x20let i: i64 = 0\n\
         \x20\x20\x20\x20while i > 100:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20sentinel = 0\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(sentinel)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("g2_always_false_while", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "99\n", "g2 stdout mismatch: {stdout:?}");
}

// G3: if-elif chain with unreachable else branch.
// n=10: first two branches miss, third (elif n==10) fires → "ten".
#[test]
fn g3_if_elif_unreachable_else() {
    let src = write_temp(
        "g3_unreachable_else",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 10\n\
         \x20\x20\x20\x20if n < 5:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"low\")\n\
         \x20\x20\x20\x20elif n < 8:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"mid\")\n\
         \x20\x20\x20\x20elif n == 10:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"ten\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"other\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("g3_unreachable_else", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "ten\n", "g3 stdout mismatch: {stdout:?}");
}

// G4: while that runs exactly once (tight boundary condition).
// n=5, condition n < 6: true once (n=5), then n=6 → false. count=1.
#[test]
fn g4_while_runs_exactly_once() {
    let src = write_temp(
        "g4_once",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 5\n\
         \x20\x20\x20\x20let count: i64 = 0\n\
         \x20\x20\x20\x20while n < 6:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20count = count + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20print(count)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("g4_once", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "1\n", "g4 stdout mismatch: {stdout:?}");
}

// G5: nested if with all branches covered inside while.
// n=1..6, n%3 cycles: 1→1, 2→2, 3→0, 4→1, 5→2, 6→0.
// count_zero=2, count_one=2, count_two=2.
#[test]
fn g5_nested_if_all_branches_covered() {
    let src = write_temp(
        "g5_all_branches",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let count_zero: i64 = 0\n\
         \x20\x20\x20\x20let count_one: i64 = 0\n\
         \x20\x20\x20\x20let count_two: i64 = 0\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 6:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if n % 3 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20count_zero = count_zero + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20elif n % 3 == 1:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20count_one = count_one + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20count_two = count_two + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20print(count_zero)\n\
         \x20\x20\x20\x20print(count_one)\n\
         \x20\x20\x20\x20print(count_two)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("g5_all_branches", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "2\n2\n2\n", "g5 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Additional cases X1..X3 to ensure ≥30 total
// =====================================================================

// X1: doubly-nested if inside while — count i>5 AND even in 1..10.
// {6, 8, 10} → 3.
#[test]
fn x1_compound_condition_doubly_nested_if() {
    let src = write_temp(
        "x1_compound_cond",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let count: i64 = 0\n\
         \x20\x20\x20\x20let i: i64 = 1\n\
         \x20\x20\x20\x20while i <= 10:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if i > 5:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20if i % 2 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20count = count + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20i = i + 1\n\
         \x20\x20\x20\x20print(count)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("x1_compound_cond", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "3\n", "x1 stdout mismatch: {stdout:?}");
}

// X2: two sequential loops where second uses result of first.
// Loop1: n=1..4 → sum=10. Loop2: m=1..10, count m%3==0: {3,6,9} → 3.
#[test]
fn x2_sequential_loops_result_chaining() {
    let src = write_temp(
        "x2_sequential_loops",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let sum: i64 = 0\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 4:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20sum = sum + n\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20let count: i64 = 0\n\
         \x20\x20\x20\x20let m: i64 = 1\n\
         \x20\x20\x20\x20while m <= sum:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if m % 3 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20count = count + 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20m = m + 1\n\
         \x20\x20\x20\x20print(sum)\n\
         \x20\x20\x20\x20print(count)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("x2_sequential_loops", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "10\n3\n", "x2 stdout mismatch: {stdout:?}");
}

// X3: while with if-no-else accumulating only when condition is met.
// n*(n+1) for n=1..5 → 2+6+12+20+30 = 70. Condition n*(n+1)%2==0 always true.
#[test]
fn x3_while_conditional_accumulation_always_fires() {
    let src = write_temp(
        "x3_cond_accum",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let total: i64 = 0\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 5:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let t: i64 = n * n + n\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if t % 2 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20total = total + t\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20print(total)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("x3_cond_accum", &src);
    let stdout = run(&exe);
    // n*(n+1) is always even; t = n^2+n for n=1..5: 2+6+12+20+30 = 70.
    assert_eq!(stdout, "70\n", "x3 stdout mismatch: {stdout:?}");
}
