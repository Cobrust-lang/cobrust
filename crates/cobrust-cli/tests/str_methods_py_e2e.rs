//! ADR-0085 — Python-named str-method end-to-end corpus.
//!
//! Cobrust is a Python successor (CLAUDE.md §2.1) and the language LLM
//! agents write correctly on the first try (§2.5). An LLM writing Python
//! reaches for `s.strip()` / `s.startswith()` / `s.endswith()`, not the
//! Rust-named `s.trim()` / `s.starts_with()` / `s.ends_with()`. ADR-0085
//! adds the Python-canonical names as the surface spelling.
//!
//! Each test writes a `.cb` program using a Python-named str method,
//! invokes `cobrust build`, runs the produced executable, and asserts
//! stdout is byte-identical to the CPython 3.11 oracle for the same
//! input. Methods covered:
//!
//! - `strip` / `startswith` / `endswith` — Python aliases that route to
//!   the SAME runtime symbol as `trim` / `starts_with` / `ends_with`.
//! - `lstrip` / `rstrip` — NEW one-sided whitespace strips. The l/r
//!   tests are asymmetric so a swapped implementation FAILS (F36).
//! - `count` — NEW non-overlapping occurrence count.
//! - A regression family asserting the Rust-named `trim` / `starts_with`
//!   / `ends_with` STILL compile and run (non-breaking per ADR-0085).
//!
//! Per `feedback_p9_clippy_stall_pattern.md`: module-level test-only
//! clippy allow header below.

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

struct TempPath {
    _temp_dir: tempfile::TempDir,
    path: PathBuf,
}

impl std::ops::Deref for TempPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

fn write_cb(name: &str, contents: &str) -> TempPath {
    let dir = tempfile::tempdir().expect("create temp source dir");
    let path = dir.path().join(format!("{name}.cb"));
    std::fs::write(&path, contents).expect("write temp .cb");
    TempPath {
        _temp_dir: dir,
        path,
    }
}

struct BuiltExe {
    _temp_dir: tempfile::TempDir,
    path: PathBuf,
}

impl std::ops::Deref for BuiltExe {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

fn run_build_exe(src: &Path) -> (i32, BuiltExe, String) {
    let bin = cobrust_binary();
    let exe_dir = tempfile::tempdir().expect("create temp exe dir");
    let exe = exe_dir.path().join(src.file_stem().unwrap());
    let out = Command::new(&bin)
        .arg("build")
        .arg(src)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .current_dir(workspace_root())
        .output()
        .expect("invoke cobrust build");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (
        code,
        BuiltExe {
            _temp_dir: exe_dir,
            path: exe,
        },
        stderr,
    )
}

fn run_exe(exe: &Path, stdin_bytes: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn produced exe");
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

fn assert_build_run(name: &str, src: &str, stdin: &[u8], expected_stdout: &str) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "{name}: build failed; stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    let (run_code, stdout, run_stderr) = run_exe(&exe, stdin);
    assert_eq!(run_code, 0, "{name}: run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, expected_stdout,
        "{name}: stdout mismatch vs CPython oracle\nstderr={run_stderr}"
    );
}

// =====================================================================
// strip — the §2.5 headline win. CPython: '  hi  '.strip() == 'hi'
// (whitespace, both ends; identical to Rust trim). Before ADR-0085 this
// errored with `method 'strip' not found on 'str'`.
// =====================================================================

#[test]
fn e0085_strip_both_ends() {
    // input "  hi  \n" → read_line strips the trailing newline → "  hi  ",
    // then .strip() → "hi". print appends one '\n'.
    let src = "fn main() -> i64:\n    let s: str = input(\"\")\n    let t: str = s.strip()\n    print(t)\n    return 0\n";
    assert_build_run("e0085_strip", src, b"  hi  \n", "hi\n");
}

// =====================================================================
// startswith / endswith — Python aliases, both `-> bool`. CPython:
// 'hello'.startswith('he') == True; 'hello'.endswith('xx') == False.
// =====================================================================

#[test]
fn e0085_startswith_true_and_false() {
    // "hello" startswith "he" → 1; startswith "xx" → 0. Prints "1\n0\n".
    let src = "fn main() -> i64:\n    let s: str = \"hello\"\n    let a: bool = s.startswith(\"he\")\n    let b: bool = s.startswith(\"xx\")\n    if a:\n        print(1)\n    else:\n        print(0)\n    if b:\n        print(1)\n    else:\n        print(0)\n    return 0\n";
    assert_build_run("e0085_startswith", src, b"", "1\n0\n");
}

#[test]
fn e0085_endswith_true_and_false() {
    // "hello" endswith "lo" → 1; endswith "xx" → 0. Prints "1\n0\n".
    let src = "fn main() -> i64:\n    let s: str = \"hello\"\n    let a: bool = s.endswith(\"lo\")\n    let b: bool = s.endswith(\"xx\")\n    if a:\n        print(1)\n    else:\n        print(0)\n    if b:\n        print(1)\n    else:\n        print(0)\n    return 0\n";
    assert_build_run("e0085_endswith", src, b"", "1\n0\n");
}

// =====================================================================
// lstrip / rstrip — NEW one-sided strips. Asymmetric input so a swapped
// l/r implementation FAILS (F36 anti-swap). CPython:
//   '  hi  '.lstrip() == 'hi  '  (right whitespace KEPT)
//   '  hi  '.rstrip() == '  hi'  (left  whitespace KEPT)
// We mark the kept side with a sentinel '|' to make stdout unambiguous.
// =====================================================================

#[test]
fn e0085_lstrip_left_only() {
    // s = "  hi  " (from stdin minus newline). lstrip() → "hi  ".
    // Concatenate a '|' sentinel: "hi  |" — proves the RIGHT spaces stay.
    let src = "fn main() -> i64:\n    let s: str = input(\"\")\n    let t: str = s.lstrip()\n    let m: str = t + \"|\"\n    print(m)\n    return 0\n";
    assert_build_run("e0085_lstrip", src, b"  hi  \n", "hi  |\n");
}

#[test]
fn e0085_rstrip_right_only() {
    // s = "  hi  ". rstrip() → "  hi". Sentinel prefix "|" → "|  hi"
    // proves the LEFT spaces stay. A swapped impl would emit "|hi".
    let src = "fn main() -> i64:\n    let s: str = input(\"\")\n    let t: str = s.rstrip()\n    let m: str = \"|\" + t\n    print(m)\n    return 0\n";
    assert_build_run("e0085_rstrip", src, b"  hi  \n", "|  hi\n");
}

// =====================================================================
// count — NEW non-overlapping occurrence count. CPython:
//   'banana'.count('a') == 3
//   'aaa'.count('aa')   == 1  (NON-overlapping, NOT 2)
// =====================================================================

#[test]
fn e0085_count_non_overlapping() {
    // Prints 'banana'.count('a')="3" then 'aaa'.count('aa')="1".
    let src = "fn main() -> i64:\n    let a: i64 = \"banana\".count(\"a\")\n    print(a)\n    let b: i64 = \"aaa\".count(\"aa\")\n    print(b)\n    return 0\n";
    assert_build_run("e0085_count", src, b"", "3\n1\n");
}

// =====================================================================
// REGRESSION — the Rust-named methods remain non-breaking per ADR-0085.
// trim / starts_with / ends_with must still compile and run identically
// to their Python aliases. Existing .cb programs and the corpus use
// these spellings.
// =====================================================================

#[test]
fn e0085_regression_rust_names_still_work() {
    // The Rust-named spellings trim / starts_with / ends_with must still
    // compile and run. Each value is used once (the str-stdlib surface is
    // borrow-on-receiver but `print` consumes; mirror the single-use
    // idiom of the M-F.3.5 corpus). "  hi  ".trim() → "hi";
    // "hi".starts_with("h") → 1 (true); "hi".ends_with("x") → 0 (false).
    let src = "fn main() -> i64:\n    let s: str = input(\"\")\n    print(s.trim())\n    let a: bool = \"hi\".starts_with(\"h\")\n    if a:\n        print(1)\n    else:\n        print(0)\n    let b: bool = \"hi\".ends_with(\"x\")\n    if b:\n        print(1)\n    else:\n        print(0)\n    return 0\n";
    assert_build_run("e0085_regression", src, b"  hi  \n", "hi\n1\n0\n");
}

// =====================================================================
// Equivalence — the Python alias and the Rust twin produce identical
// output on the same input, proving the alias routes to the SAME symbol.
// =====================================================================

#[test]
fn e0085_strip_equals_trim() {
    let strip_src =
        "fn main() -> i64:\n    let s: str = input(\"\")\n    print(s.strip())\n    return 0\n";
    let trim_src =
        "fn main() -> i64:\n    let s: str = input(\"\")\n    print(s.trim())\n    return 0\n";
    let p1 = write_cb("e0085_eq_strip", strip_src);
    let p2 = write_cb("e0085_eq_trim", trim_src);
    let (c1, e1, se1) = run_build_exe(&p1);
    let (c2, e2, se2) = run_build_exe(&p2);
    assert_eq!(c1, 0, "strip build failed: {se1}");
    assert_eq!(c2, 0, "trim build failed: {se2}");
    let (_, o1, _) = run_exe(&e1, b"  spaced  \n");
    let (_, o2, _) = run_exe(&e2, b"  spaced  \n");
    assert_eq!(o1, "spaced\n");
    assert_eq!(o1, o2, "strip and trim must produce identical stdout");
}
