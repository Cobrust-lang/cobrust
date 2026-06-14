//! F89 end-to-end corpus: `continue` inside a `for` loop must SKIP the
//! current element and TERMINATE (never infinite-loop).
//!
//! Background (docs/agent/findings/f89-continue-in-for-loop-hangs.md):
//! the `for` loop lowers to length-bound index iteration. Before F89 the
//! `__idx += 1` increment lived ONLY in the body fall-through, and
//! `continue` gotoed the loop header directly — bypassing the increment.
//! Result: `continue` made `__idx` stay fixed, `__idx < len` stayed true,
//! and the loop SPUN FOREVER (no diagnostic, no exit — worse than a crash).
//! `for x in [1,2,3,4]: if x == 2: continue; print(x)` printed `1` then hung.
//!
//! Fix: a per-`for`-loop increment LATCH block is the `continue` target;
//! both the body fall-through and `continue` route through it, so `__idx`
//! advances on EVERY re-entry path.
//!
//! WATCHDOG DISCIPLINE (critical): a regression to the hang must FAIL the
//! test, NOT stall CI for hours. Every run goes through `run_with_timeout`,
//! which spawns the produced exe and kills + FAILS it if it does not exit
//! within `RUN_TIMEOUT`. A correct run of these tiny loops finishes in
//! milliseconds; only a hang trips the bound. NO unbounded `.wait()` here.
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
use std::time::{Duration, Instant};

/// Watchdog bound. The loops in this corpus iterate at most a handful of
/// elements; a correct exe exits in milliseconds. A hang (F89 regression)
/// will exceed this and FAIL the test instead of stalling CI.
const RUN_TIMEOUT: Duration = Duration::from_secs(10);

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

/// Spawn `exe`, poll for exit, and KILL + FAIL if it does not terminate
/// within `RUN_TIMEOUT`. Returns trimmed stdout on success.
///
/// This is the F89 watchdog: a hanging exe (the regression we guard
/// against) cannot stall the test runner — it is killed and the assertion
/// below makes the test red.
fn run_with_timeout(name: &str, exe: &Path) -> String {
    use std::fs::File;
    let temp = exe.parent().expect("exe parent");
    let stdout_path = temp.join(format!("{name}.stdout"));
    let stderr_path = temp.join(format!("{name}.stderr"));
    let out_file = File::create(&stdout_path).expect("create stdout file");
    let err_file = File::create(&stderr_path).expect("create stderr file");

    let mut child = Command::new(exe)
        .stdout(out_file)
        .stderr(err_file)
        .spawn()
        .expect("spawn exe");

    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            let stdout = std::fs::read_to_string(&stdout_path).unwrap_or_default();
            let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
            assert!(
                status.success(),
                "{name}: exe exited non-zero ({status:?})\nstdout={stdout}\nstderr={stderr}"
            );
            return stdout.trim_end_matches('\n').to_string();
        }
        if start.elapsed() >= RUN_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "{name}: exe did NOT terminate within {:?} — HANG (F89 regression: \
                 `continue` in a `for` loop is bypassing the index increment)",
                RUN_TIMEOUT
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Build, run-under-watchdog, and assert the exact stdout. Termination is
/// implicitly asserted by `run_with_timeout` (a hang panics).
fn assert_program(name: &str, src: &str, expected: &str) {
    let built = build_program(name, src);
    let stdout = run_with_timeout(name, &built.exe);
    assert_eq!(
        stdout,
        expected.trim_end_matches('\n'),
        "{name}: stdout mismatch\nsrc:\n{src}"
    );
}

// =====================================================================
// Section A — single-loop `continue` skip-filter (the textbook idiom)
// =====================================================================

#[test]
fn c01_skip_evens_prints_odds_and_terminates() {
    // CPython: 1,3,5. Must SKIP evens (continue) and EXIT 0.
    let src = "fn main() -> i64:\n    for x in [1, 2, 3, 4, 5]:\n        if x % 2 == 0:\n            continue\n        print(x)\n    return 0\n";
    assert_program("c01_skip_evens", src, "1\n3\n5");
}

#[test]
fn c02_continue_on_first_element() {
    // Skip the first element (1), print the rest.
    let src = "fn main() -> i64:\n    for x in [1, 2, 3]:\n        if x == 1:\n            continue\n        print(x)\n    return 0\n";
    assert_program("c02_continue_first", src, "2\n3");
}

#[test]
fn c03_continue_on_last_element() {
    // Skip the last element (3): the `continue` fires on the final
    // iteration. The latch must still bump `__idx` past `len` so the
    // loop exits (pre-F89 this hung on the last element).
    let src = "fn main() -> i64:\n    for x in [1, 2, 3]:\n        if x == 3:\n            continue\n        print(x)\n    print(99)\n    return 0\n";
    assert_program("c03_continue_last", src, "1\n2\n99");
}

#[test]
fn c04_continue_every_element_empty_output_terminates() {
    // `continue` on EVERY iteration: body prints nothing, but the loop
    // must still advance and TERMINATE. Pre-F89 this was the purest hang.
    let src = "fn main() -> i64:\n    for x in [1, 2, 3, 4]:\n        continue\n        print(x)\n    print(100)\n    return 0\n";
    assert_program("c04_continue_all", src, "100");
}

#[test]
fn c05_continue_in_elif_branch() {
    let src = "fn main() -> i64:\n    for x in [1, 2, 3, 4, 5]:\n        if x == 2:\n            print(20)\n        elif x == 4:\n            continue\n        else:\n            print(x)\n    return 0\n";
    assert_program("c05_continue_elif", src, "1\n20\n3\n5");
}

// =====================================================================
// Section B — `continue` + `break` in the same loop
// =====================================================================

#[test]
fn c06_continue_then_break() {
    // Skip 3 (continue), stop at 5 (break). Prints 1,2,4.
    let src = "fn main() -> i64:\n    for x in [1, 2, 3, 4, 5, 6]:\n        if x == 3:\n            continue\n        if x == 5:\n            break\n        print(x)\n    return 0\n";
    assert_program("c06_continue_break", src, "1\n2\n4");
}

#[test]
fn c07_multiple_skips_then_break() {
    let src = "fn main() -> i64:\n    for x in [1, 2, 3, 4, 5, 6, 7, 8]:\n        if x == 2:\n            continue\n        if x == 4:\n            continue\n        if x == 7:\n            break\n        print(x)\n    return 0\n";
    assert_program("c07_multi_skip_break", src, "1\n3\n5\n6");
}

#[test]
fn c08_continue_after_some_prints() {
    // The print precedes the conditional continue: print runs each
    // iteration, then odd values continue (no second print).
    let src = "fn main() -> i64:\n    for x in [1, 2, 3]:\n        print(x)\n        if x % 2 == 1:\n            continue\n        print(x * 100)\n    return 0\n";
    assert_program("c08_continue_after_print", src, "1\n2\n200\n3");
}

// =====================================================================
// Section C — nested loops: each `continue` targets its OWN inner latch
// =====================================================================

#[test]
fn c09_nested_inner_continue_targets_innermost() {
    // CPython: 12, 22. Inner `continue` (b==1) must advance ONLY the
    // inner index, not the outer — verifies the loop-context stack is
    // scoped so each loop's continue hits its own increment latch.
    let src = "fn main() -> i64:\n    for a in [1, 2]:\n        for b in [1, 2]:\n            if b == 1:\n                continue\n            print(a * 10 + b)\n    return 0\n";
    assert_program("c09_nested_inner", src, "12\n22");
}

#[test]
fn c10_nested_outer_continue() {
    // Outer `continue` (a==2) skips the entire inner loop for a==2.
    // a=1 → inner prints 11,12; a=2 → continue (skip inner); a=3 → 31,32.
    let src = "fn main() -> i64:\n    for a in [1, 2, 3]:\n        if a == 2:\n            continue\n        for b in [1, 2]:\n            print(a * 10 + b)\n    return 0\n";
    assert_program("c10_nested_outer", src, "11\n12\n31\n32");
}

#[test]
fn c11_nested_continue_in_both_levels() {
    // Outer skips a==2; inner skips b==2. a=1: print 11,13; a=3: 31,33.
    let src = "fn main() -> i64:\n    for a in [1, 2, 3]:\n        if a == 2:\n            continue\n        for b in [1, 2, 3]:\n            if b == 2:\n                continue\n            print(a * 10 + b)\n    return 0\n";
    assert_program("c11_nested_both", src, "11\n13\n31\n33");
}

// =====================================================================
// Section D — regression: plain for + break still correct alongside fix
// =====================================================================

#[test]
fn c12_plain_for_no_continue_iterates_all() {
    // Sanity: the increment latch did not change straight-line iteration.
    let src = "fn main() -> i64:\n    for x in [10, 20, 30]:\n        print(x)\n    return 0\n";
    assert_program("c12_plain_for", src, "10\n20\n30");
}

#[test]
fn c13_break_still_exits_loop_early() {
    // `break` still gotos the exit block (not the latch).
    let src = "fn main() -> i64:\n    for x in [1, 2, 3, 4, 5]:\n        if x == 3:\n            break\n        print(x)\n    print(99)\n    return 0\n";
    assert_program("c13_break", src, "1\n2\n99");
}

#[test]
fn c14_continue_in_for_over_range() {
    // The same fix applies to `range`-backed lists (same lowering path).
    let src = "fn main() -> i64:\n    for i in range(0, 6):\n        if i % 2 == 1:\n            continue\n        print(i)\n    return 0\n";
    assert_program("c14_range_continue", src, "0\n2\n4");
}

#[test]
fn c15_continue_accumulator_value_correct() {
    // Skip 3; sum the rest of 1..=5 → 1+2+4+5 = 12. Verifies the body's
    // side effects and the index advance compose correctly.
    let src = "fn main() -> i64:\n    let s: i64 = 0\n    for x in [1, 2, 3, 4, 5]:\n        if x == 3:\n            continue\n        s = s + x\n    print(s)\n    return 0\n";
    assert_program("c15_accumulator", src, "12");
}
