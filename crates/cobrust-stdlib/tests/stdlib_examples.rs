//! Examples integration test (per ADR-0025 §"Examples (binding)").
//!
//! Drives `cobrust build && run` on each of the 10 representative
//! example programs (plus `hello.cb` for M10 regression) and asserts
//! the headline acceptance bar: each program builds + runs + matches
//! its expected stdout + exits 0.
//!
//! Tests in this file are gated behind `--ignored` so they don't run
//! in the default test pass (they require `cargo build -p
//! cobrust-stdlib` to have produced the staticlib + `cargo build -p
//! cobrust-cli` to have produced the binary). The CI gate runs them
//! explicitly via `cargo test -p cobrust-stdlib -- --ignored
//! stdlib_examples`.

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
#![allow(clippy::default_constructed_unit_structs)]
#![allow(clippy::stable_sort_primitive)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::box_default)]
#![allow(clippy::manual_pattern_char_comparison)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::missing_assert_message)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::unnecessary_debug_formatting)]
#![allow(clippy::manual_assert)]
#![allow(clippy::expect_fun_call)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

fn cobrust_binary() -> PathBuf {
    let workspace = workspace_root();
    // Locate the cobrust binary: prefer the workspace's debug
    // target. (cargo-build the cli first via the gate command.)
    let candidate = workspace.join("target/debug/cobrust");
    if !candidate.exists() {
        panic!(
            "cobrust binary missing at {}; run `cargo build -p cobrust-cli` first",
            candidate.display()
        );
    }
    candidate
}

/// Compile + run an example, return (stdout, exit_code).
/// F63 (2026-05-27): RAII tempdir replaces the legacy
/// `std::env::temp_dir().join(...)` leak. The guard drops at function
/// exit — after the produced binary has been run and stdout captured.
fn build_and_run(example: &str) -> (String, i32) {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let cb_path = workspace.join("examples").join(format!("{example}.cb"));
    assert!(cb_path.exists(), "examples/{example}.cb missing");

    let exe_dir_guard = tempfile::tempdir().expect("create tempdir for example exe");
    let exe_path = exe_dir_guard.path().join(example);

    // Build.
    let build_output = Command::new(&bin)
        .arg("build")
        .arg(&cb_path)
        .arg("-o")
        .arg(&exe_path)
        .arg("--quiet")
        .current_dir(&workspace)
        .output()
        .expect("invoke cobrust build");
    assert!(
        build_output.status.success(),
        "build failed for {example}: stderr={}",
        String::from_utf8_lossy(&build_output.stderr)
    );
    assert!(
        exe_path.exists(),
        "build produced no exe for {example} at {exe_path:?}"
    );

    // Run.
    let run_output = Command::new(&exe_path)
        .output()
        .expect("invoke produced executable");
    let stdout = String::from_utf8_lossy(&run_output.stdout).to_string();
    let code = run_output.status.code().unwrap_or(-1);
    (stdout, code)
}

/// Helper for the 10-example acceptance check.
fn assert_example(example: &str, expected_stdout_substr: &str) {
    let (stdout, code) = build_and_run(example);
    assert_eq!(
        code, 0,
        "{example} exited with code {code}; stdout={stdout:?}"
    );
    assert!(
        stdout.contains(expected_stdout_substr),
        "{example} stdout {stdout:?} does not contain expected {expected_stdout_substr:?}"
    );
}

// =====================================================================
// Headline acceptance gate — 10 examples + hello.cb regression.
//
// ADR-0027 §"Example rewrites" lifts every #[ignore] marker; the
// 8 deferred M11 examples now ship as real Cobrust source exercising
// the M12.x lowering surface.
// =====================================================================

#[test]
fn stdlib_examples_hello() {
    assert_example("hello", "hello, world");
}

#[test]
fn stdlib_examples_fizzbuzz() {
    let (stdout, code) = build_and_run("fizzbuzz");
    assert_eq!(code, 0);
    // Spot-check key fizzbuzz markers.
    assert!(stdout.contains("Fizz"));
    assert!(stdout.contains("Buzz"));
    assert!(stdout.contains("FizzBuzz"));
    assert!(stdout.contains("1"));
    assert!(stdout.contains("14"));
}

#[test]
fn stdlib_examples_fib() {
    // ADR-0030 M11.1: fib.cb now uses an iterative algorithm.
    // Output is "fib(10) =\n55\n" (two lines); check both substrings.
    let (stdout, code) = build_and_run("fib");
    assert_eq!(code, 0, "fib exited with code {code}; stdout={stdout:?}");
    assert!(
        stdout.contains("fib(10) ="),
        "fib stdout {stdout:?} does not contain 'fib(10) ='"
    );
    assert!(
        stdout.contains("55"),
        "fib stdout {stdout:?} does not contain '55'"
    );
}

#[test]
fn stdlib_examples_wc() {
    assert_example("wc", "wc:");
}

#[test]
fn stdlib_examples_cat() {
    assert_example("cat", "cat:");
}

#[test]
fn stdlib_examples_echo() {
    assert_example("echo", "echo:");
}

#[test]
fn stdlib_examples_sort() {
    assert_example("sort", "sort:");
}

#[test]
fn stdlib_examples_unique_lines() {
    assert_example("unique_lines", "unique_lines:");
}

#[test]
fn stdlib_examples_regex_grep() {
    assert_example("regex_grep", "regex_grep:");
}

#[test]
fn stdlib_examples_csv_sum() {
    assert_example("csv_sum", "csv_sum:");
}

#[test]
fn stdlib_examples_json_pretty() {
    assert_example("json_pretty", "json_pretty:");
}

// =====================================================================
// Per-stub runtime ABI tests — exercise the stdlib API behind each
// stub. These run in the default pass (no #[ignore]) and verify the
// runtime ABI shipped with the .cb stub is actually correct for the
// source-level intent.
// =====================================================================

use cobrust_stdlib::{Dict, List, Set, collections, env, fmt as cb_fmt, io, math, string};

#[test]
fn runtime_wc_word_count_intent() {
    // The `wc.cb` source-level intent: read file, split, count.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("input.txt");
    io::write_file(p.to_str().unwrap(), "alpha beta gamma delta").unwrap();
    let contents = io::read_file(p.to_str().unwrap()).unwrap();
    let words = string::split(&contents, " ");
    assert_eq!(words.len(), 4);
}

#[test]
fn runtime_cat_concat_intent() {
    // `cat.cb` intent: read file → print contents.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    io::write_file(p.to_str().unwrap(), "line one\nline two\n").unwrap();
    let read = io::read_file(p.to_str().unwrap()).unwrap();
    assert!(read.contains("line one"));
    assert!(read.contains("line two"));
}

#[test]
fn runtime_echo_args_intent() {
    // `echo.cb` intent: read argv, print joined.
    let args = env::args();
    // The test harness always provides at least argv[0].
    assert!(!args.is_empty());
}

#[test]
fn runtime_sort_intent() {
    // `sort.cb` intent: collect lines into a List, sort, print.
    let lines = vec!["c", "a", "b"];
    let mut l: List<&str> = lines.into_iter().collect();
    l.sort();
    let v: Vec<&str> = l.iter().copied().collect();
    assert_eq!(v, vec!["a", "b", "c"]);
}

#[test]
fn runtime_unique_lines_intent() {
    // `unique_lines.cb` intent: dedupe via Set.
    let lines = vec!["a", "b", "a", "c", "b"];
    let s: Set<&str> = lines.into_iter().collect();
    assert_eq!(s.len(), 3);
}

#[test]
fn runtime_regex_grep_substring_intent() {
    // `regex_grep.cb` intent: filter lines matching a pattern.
    let haystack = "alpha\nbeta\ngamma\nalpha2";
    let lines: Vec<&str> = haystack.lines().collect();
    let matches: Vec<&str> = lines
        .iter()
        .filter(|l| string::find(l, "alpha").is_some())
        .copied()
        .collect();
    assert_eq!(matches, vec!["alpha", "alpha2"]);
}

#[test]
fn runtime_csv_sum_intent() {
    // `csv_sum.cb` intent: sum a column.
    let csv = "1\n2\n3\n4\n5";
    let total: i64 = csv.lines().filter_map(|l| l.parse::<i64>().ok()).sum();
    assert_eq!(total, 15);
}

#[test]
fn runtime_json_pretty_intent() {
    // `json_pretty.cb` intent: read file, parse JSON. M11 ships
    // a hand-written shim test that demonstrates the I/O round-trip
    // (the stub stays in .cb until pure-Cobrust JSON parsing lands).
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("data.json");
    io::write_file(p.to_str().unwrap(), "{\"key\":\"value\"}").unwrap();
    let read = io::read_file(p.to_str().unwrap()).unwrap();
    assert!(read.contains("\"key\""));
}

// =====================================================================
// Cross-module — math + fmt + collections compose for example use.
// =====================================================================

#[test]
fn runtime_math_collection_compose() {
    let mut l: List<f64> = List::new();
    for i in 1..=5 {
        l.push(math::sqrt(f64::from(i)));
    }
    assert_eq!(l.len(), 5);
    let strs: Vec<String> = l.iter().map(|x| cb_fmt::format_float(*x)).collect();
    assert_eq!(strs.len(), 5);
}

#[test]
fn runtime_dict_format_compose() {
    let mut d: Dict<String, i64> = Dict::new();
    d.insert("answer".into(), 42);
    let v = d.get("answer").unwrap();
    let s = cb_fmt::format_int(*v);
    assert_eq!(s, "42");
}

#[test]
fn runtime_collections_module_path_smoke() {
    let _: collections::List<i64> = collections::List::new();
    let _: collections::Dict<String, i64> = collections::Dict::new();
    let _: collections::Set<i64> = collections::Set::new();
}
