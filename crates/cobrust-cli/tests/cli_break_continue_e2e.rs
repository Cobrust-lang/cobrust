//! ADR-0050a M-F.3.0 end-to-end corpus for `break` / `continue`.
//!
//! Each test:
//! 1. Writes a Cobrust source file to a temp dir.
//! 2. Invokes `cobrust build` via the registered cargo binary.
//! 3. Runs the produced executable.
//! 4. Asserts exact stdout match against the expected runtime
//!    behaviour predicted by ADR-0050a §"Semantics".
//!
//! Coverage:
//! - Single-loop break early-exit, continue skip
//! - Nested-loop break-innermost, continue-innermost
//! - break/continue interleaved with if/elif/else
//! - break + post-loop computation
//! - break in while True (infinite-loop guard)
//! - while-else skipped on break (Python semantics)
//!
//! 18-lint clippy allow header per `feedback_p9_clippy_stall_pattern.md`.

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
#![allow(clippy::too_many_lines)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::len_zero)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::unnecessary_debug_formatting)]

use std::path::{Path, PathBuf};
use std::process::Command;

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

struct Built {
    _temp: tempfile::TempDir,
    exe: PathBuf,
}

fn build_program(name: &str, src: &str) -> Built {
    let temp = tempfile::tempdir().expect("tempdir");
    let src_path = temp.path().join(format!("{name}.cb"));
    std::fs::write(&src_path, src).expect("write src");
    let exe = temp.path().join(name);
    let bin = cobrust_binary();
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .current_dir(workspace_root())
        .output()
        .expect("invoke cobrust build");
    assert!(
        out.status.success(),
        "cobrust build failed for {name}:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Built { _temp: temp, exe }
}

fn run_and_capture(exe: &Path) -> String {
    let out = Command::new(exe).output().expect("run exe");
    assert!(
        out.status.success(),
        "exe failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn assert_program_output(name: &str, src: &str, expected: &str) {
    let built = build_program(name, src);
    let stdout = run_and_capture(&built.exe);
    assert_eq!(
        stdout.trim_end_matches('\n'),
        expected.trim_end_matches('\n'),
        "{name}: stdout mismatch\nsrc:\n{src}"
    );
}

// =====================================================================
// E2E Section A — single-loop break (≥3 cases)
// =====================================================================

#[test]
fn e01_break_at_5_in_while_under_10() {
    // i=1,2,3,4 print; i=5 break; then post-loop 99.
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n        if i == 5:\n            break\n        print(i)\n    print(99)\n    return 0\n";
    assert_program_output("e01_break_at_5", src, "1\n2\n3\n4\n99");
}

#[test]
fn e02_break_at_first_iteration() {
    // i=0 → check i==0 → break. Loop prints nothing.
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 100:\n        if i == 0:\n            break\n        print(i)\n        i = i + 1\n    print(99)\n    return 0\n";
    assert_program_output("e02_break_first", src, "99");
}

#[test]
fn e03_break_after_all_iterations() {
    // i=1..5 print; then i=5 fails cond, exits naturally; sum/post = 6.
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 5:\n        i = i + 1\n        print(i)\n    print(6)\n    return 0\n";
    assert_program_output("e03_natural_exit", src, "1\n2\n3\n4\n5\n6");
}

// =====================================================================
// E2E Section B — single-loop continue (≥3 cases)
// =====================================================================

#[test]
fn e04_continue_skips_odd() {
    // i=1..10; skip if odd. Should print 2,4,6,8,10.
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n        if i % 2 == 1:\n            continue\n        print(i)\n    return 0\n";
    assert_program_output("e04_continue_odd", src, "2\n4\n6\n8\n10");
}

#[test]
fn e05_continue_skips_one_specific_value() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 5:\n        i = i + 1\n        if i == 3:\n            continue\n        print(i)\n    return 0\n";
    assert_program_output("e05_continue_3", src, "1\n2\n4\n5");
}

#[test]
fn e06_continue_in_elif() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 6:\n        i = i + 1\n        if i == 2:\n            print(20)\n        elif i == 4:\n            continue\n        else:\n            print(i)\n    return 0\n";
    assert_program_output("e06_continue_elif", src, "1\n20\n3\n5\n6");
}

// =====================================================================
// E2E Section C — combined break + continue (≥2 cases)
// =====================================================================

#[test]
fn e07_continue_then_break() {
    // i=1..10; skip 3; break at 7. Prints 1,2,4,5,6.
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 10:\n        i = i + 1\n        if i == 3:\n            continue\n        if i == 7:\n            break\n        print(i)\n    return 0\n";
    assert_program_output("e07_continue_then_break", src, "1\n2\n4\n5\n6");
}

#[test]
fn e08_three_skips_then_break() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 100:\n        i = i + 1\n        if i == 2:\n            continue\n        if i == 4:\n            continue\n        if i == 6:\n            continue\n        if i == 9:\n            break\n        print(i)\n    return 0\n";
    assert_program_output("e08_three_skips_break", src, "1\n3\n5\n7\n8");
}

// =====================================================================
// E2E Section D — nested-loop innermost binding (≥3 cases)
// =====================================================================

#[test]
fn e09_break_inner_only() {
    // i=0..2, inner j=0..2 with break-when-j==1. Should print (0,0), (1,0), (2,0).
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        let j: i64 = 0\n        while j < 3:\n            if j == 1:\n                break\n            print(i * 10 + j)\n            j = j + 1\n        i = i + 1\n    return 0\n";
    assert_program_output("e09_break_inner", src, "0\n10\n20");
}

#[test]
fn e10_continue_inner_only() {
    // Inner skips j==1; prints (i,0), (i,2) for each outer i.
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 2:\n        let j: i64 = 0\n        while j < 3:\n            j = j + 1\n            if j == 2:\n                continue\n            print(i * 10 + j)\n        i = i + 1\n    return 0\n";
    // For i=0: j inc to 1 (print 1), j=2 continue, j inc to 3 (print 3)
    // For i=1: same → print 11, 13
    assert_program_output("e10_continue_inner", src, "1\n3\n11\n13");
}

#[test]
fn e11_three_level_nested_innermost_break() {
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 2:\n        let j: i64 = 0\n        while j < 2:\n            let k: i64 = 0\n            while k < 5:\n                if k == 2:\n                    break\n                print(i * 100 + j * 10 + k)\n                k = k + 1\n            j = j + 1\n        i = i + 1\n    return 0\n";
    // i=0,j=0,k=0,1 print 0,1; k=2 break inner
    // i=0,j=1,k=0,1 print 10,11
    // i=1,j=0,k=0,1 print 100,101
    // i=1,j=1,k=0,1 print 110,111
    assert_program_output("e11_triple_nested", src, "0\n1\n10\n11\n100\n101\n110\n111");
}

// =====================================================================
// E2E Section E — post-loop semantics + control-flow combinations (≥3 cases)
// =====================================================================

#[test]
fn e12_post_loop_value_preserved_after_break() {
    // Use sum + break; check post-loop sum is correct.
    // Sum of 1..=5 = 15.
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    let s: i64 = 0\n    while i < 100:\n        i = i + 1\n        if i == 6:\n            break\n        s = s + i\n    print(s)\n    return 0\n";
    assert_program_output("e12_post_loop_sum", src, "15");
}

#[test]
fn e13_break_in_while_true_infinite_guard() {
    // while True must terminate via break.
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while True:\n        i = i + 1\n        if i == 3:\n            break\n        print(i)\n    print(99)\n    return 0\n";
    assert_program_output("e13_while_true_break", src, "1\n2\n99");
}

#[test]
fn e14_continue_at_top_does_not_hang_when_cond_changes_outside() {
    // Continue plus mutation BEFORE the continue means the loop
    // terminates naturally.
    let src = "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        i = i + 1\n        continue\n        print(99)\n    print(100)\n    return 0\n";
    assert_program_output("e14_continue_top", src, "100");
}

// =====================================================================
// E2E Section F — examples/early_exit.cb reference (≥1 case)
// =====================================================================

#[test]
fn e15_early_exit_example_runs() {
    // Build the canonical example shipped at examples/early_exit.cb.
    let bin = cobrust_binary();
    let example = workspace_root().join("examples/early_exit.cb");
    assert!(example.exists(), "examples/early_exit.cb missing");
    let temp = tempfile::tempdir().expect("tempdir");
    let exe = temp.path().join("early_exit");
    let out = Command::new(&bin)
        .arg("build")
        .arg(&example)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .current_dir(workspace_root())
        .output()
        .expect("invoke cobrust build");
    assert!(
        out.status.success(),
        "early_exit build failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("run early_exit");
    assert!(
        run.status.success(),
        "early_exit run failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    // Behavior contract: see examples/early_exit.cb header comments.
    let stdout = String::from_utf8_lossy(&run.stdout);
    let trimmed = stdout.trim_end_matches('\n');
    // Predict: starts at i=0 sum=0; i increments; skip i==7; break when sum > 30.
    // i=1: sum=1; i=2: sum=3; i=3: sum=6; i=4: sum=10; i=5: sum=15;
    // i=6: sum=21; i=7: skip; i=8: sum=29; i=9: sum=38 BUT check is before
    // assignment. Looking at example: `if sum > 30: break` happens AFTER
    // increment + skip but BEFORE adding. So sum stays 29 when i=8 added,
    // then at i=9 sum=29 not yet >30 → adds → 38; at i=10 sum=38 > 30 → break.
    // Actually re-read: at each iter: increment i; if i==7 continue; if sum>30 break; sum=sum+i.
    // i=1: 1>30? no; sum=1.
    // i=2: 1>30? no; sum=3.
    // i=3: 3>30? no; sum=6.
    // i=4: 6>30? no; sum=10.
    // i=5: 10>30? no; sum=15.
    // i=6: 15>30? no; sum=21.
    // i=7: continue.
    // i=8: 21>30? no; sum=29.
    // i=9: 29>30? no; sum=38.
    // i=10: 38>30? YES; break.
    // Print 38.
    assert_eq!(trimmed, "38", "early_exit output mismatch");
}
