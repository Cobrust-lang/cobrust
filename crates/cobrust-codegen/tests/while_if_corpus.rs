//! M11.1 while-loop + if-statement corpus (ADR-0030 §"Acceptance gate" #1).
//!
//! Shells out to the `cobrust` binary for each test case; builds the program
//! to a host executable, runs it, and asserts stdout exactly.
//!
//! Test cases transcribed from
//! `docs/agent/findings/m12-x-while-if-codegen-regression.md` §Method:
//!
//! - test1  — top-level if/else (baseline; passes before fix)
//! - test2  — modulo + if (baseline; passes before fix)
//! - test3  — while + print + mutation, no if (baseline; passes before fix)
//! - test6  — while + if/else + mutation (FAILS before fix)
//! - test7  — while + leading-print + if-no-else + mutation (workaround; passes before fix)
//! - test8  — while + if-no-else + mutation (FAILS before fix)
//! - test_fizzbuzz_short — full FizzBuzz algorithm (FAILS before fix)
//!
//! ADR-0030 §6 (workflow): test corpus committed before the fix; CI
//! records the transition from 4-fail/3-pass → 7-pass.

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
#![allow(clippy::single_char_pattern)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::doc_overindented_list_items)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::derivable_impls)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn cobrust_binary() -> PathBuf {
    // `CARGO_BIN_EXE_cobrust` is only set when the test runner is the
    // `cobrust-cli` package (which owns the binary). In the
    // `cobrust-codegen` package we cannot declare `cobrust-cli` as a
    // dev-dependency (circular: cli depends on codegen). Instead we
    // locate the pre-built binary in the workspace target directory,
    // which Cargo has already built before running any package tests.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root from CARGO_MANIFEST_DIR");
    // Prefer a profile-matching binary if available; fall back to debug.
    // During `cargo test --workspace`, debug is always built first.
    let debug_bin = workspace.join("target/debug/cobrust");
    if debug_bin.exists() {
        return debug_bin;
    }
    let release_bin = workspace.join("target/release/cobrust");
    if release_bin.exists() {
        return release_bin;
    }
    // Last-resort: assume it is on PATH.
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
        "cobrust-m11-1-corpus-{}-{}",
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
    let exe_dir =
        std::env::temp_dir().join(format!("cobrust-m11-1-exe-{}-{}", name, std::process::id()));
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
// test1 — top-level if/else (passes at HEAD; baseline check)
// =====================================================================

#[test]
fn test1_top_level_if_else() {
    let src = write_temp(
        "test1",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 5\n\
         \x20\x20\x20\x20if n > 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"big\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"small\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("test1", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "big\n", "test1 stdout mismatch: {stdout:?}");
}

// =====================================================================
// test2 — modulo + if (passes at HEAD; baseline check)
// =====================================================================

#[test]
fn test2_modulo_if() {
    let src = write_temp(
        "test2",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 6\n\
         \x20\x20\x20\x20let r: i64 = n % 3\n\
         \x20\x20\x20\x20if r == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"Fizz\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("test2", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "Fizz\n", "test2 stdout mismatch: {stdout:?}");
}

// =====================================================================
// test3 — while + print + mutation, no if (passes at HEAD; baseline check)
// =====================================================================

#[test]
fn test3_while_no_if() {
    let src = write_temp(
        "test3",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 0\n\
         \x20\x20\x20\x20while n < 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"loop\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("test3", &src);
    let stdout = run(&exe);
    assert_eq!(
        stdout, "loop\nloop\nloop\n",
        "test3 stdout mismatch: {stdout:?}"
    );
}

// =====================================================================
// test6 — while + if/else + mutation (FAILS before M11.1 fix)
// =====================================================================

#[test]
fn test6_while_if_else() {
    let src = write_temp(
        "test6",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if n == 2:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(\"two\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(\"not-two\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("test6", &src);
    let stdout = run(&exe);
    assert_eq!(
        stdout, "not-two\ntwo\nnot-two\n",
        "test6 stdout mismatch: {stdout:?}"
    );
}

// =====================================================================
// test7 — while + leading-print + if-no-else + mutation (passes at HEAD;
//         workaround that demonstrates leading-stmt bypass)
// =====================================================================

#[test]
fn test7_while_leading_print_if() {
    let src = write_temp(
        "test7",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"loop\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if n == 2:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(\"two\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("test7", &src);
    let stdout = run(&exe);
    assert_eq!(
        stdout, "loop\nloop\ntwo\nloop\n",
        "test7 stdout mismatch: {stdout:?}"
    );
}

// =====================================================================
// test8 — while + if-no-else + mutation (FAILS before M11.1 fix)
// =====================================================================

#[test]
fn test8_while_if_no_else() {
    let src = write_temp(
        "test8",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if n == 2:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print(\"two\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("test8", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "two\n", "test8 stdout mismatch: {stdout:?}");
}

// =====================================================================
// test_fizzbuzz_short — full FizzBuzz 1..15 (FAILS before M11.1 fix)
// =====================================================================

#[test]
fn test_fizzbuzz_short() {
    // FizzBuzz 1..=15 using while + if/elif/elif/else + modulo.
    // Uses polymorphic print(n) for the plain-number case (ADR-0064).
    let src = write_temp(
        "fizzbuzz_short",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 1\n\
         \x20\x20\x20\x20while n <= 15:\n\
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
    let exe = build("fizzbuzz_short", &src);
    let stdout = run(&exe);
    let expected = "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n";
    assert_eq!(
        stdout, expected,
        "fizzbuzz stdout mismatch:\ngot:      {stdout:?}\nexpected: {expected:?}"
    );
}
