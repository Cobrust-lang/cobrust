// ADR-0089 end-to-end corpus for the type-PRESERVING free-function
// `abs(x)` builtin and the Python-canonical 1-arg `range(stop)` form —
// two §2.5 LLM-first builtin fixes (the spellings a Python-trained LLM
// reaches for constantly).
//
// Per ADR-0089 §3/§5: bare `abs(x)` is now type-preserving like Python's
// — `abs(-5) == 5` (an `int`, usable in int arithmetic) and
// `abs(-5.0) == 5.0` (a `float`), where the pre-ADR-0089 type-checker
// rejected `abs(-5)` with the misleading `type mismatch: expected f64,
// found i64`. Per ADR-0089 §4: `range(5)` now means `range(0, 5)` (the
// 1-arg form), where the pre-fix path rejected it with `wrong number of
// arguments: expected 2, got 1`.
//
// These tests REAL-compile -> link -> spawn a `.cb` program and assert
// the produced executable's stdout / exit code, differentially against
// python3.11 semantics (the printed values match Python — modulo
// cobrust's whole-float print formatting, which drops the trailing `.0`
// so `abs(-5.0)` prints `5`, the same value 5.0 Python prints as `5.0`).
//
// Per `feedback_p9_clippy_stall_pattern.md`: module-level test-only lint
// allow header.

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
#![allow(clippy::similar_names)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::needless_pass_by_value)]

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

/// Invoke `cobrust build`; return `(exit_code, exe, stderr)`.
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

fn run_exe(exe: &Path, args: &[&str], stdin_bytes: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(exe)
        .args(args)
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

/// Build + run; assert build succeeds, run exits 0, stdout matches.
fn assert_build_run(name: &str, src: &str, expected_stdout: &str) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "{name}: build failed; stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    let (run_code, stdout, run_stderr) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0, "{name}: run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, expected_stdout,
        "{name}: stdout mismatch\nstderr={run_stderr}"
    );
}

// =====================================================================
// abs_e2e_01 — `print(abs(-5))` -> "5\n". The arg is an i64; the result
// is an i64 (type-preserving). Lowers to __cobrust_int_abs. ADR-0089 §3.
// =====================================================================

#[test]
fn abs_e2e_01_neg_int_returns_int() {
    let src = "fn main() -> i64:\n    print(abs(-5))\n    return 0\n";
    assert_build_run("abs_e2e_01", src, "5\n");
}

// =====================================================================
// abs_e2e_02 — `print(abs(5))` -> "5\n" (already-positive int).
// =====================================================================

#[test]
fn abs_e2e_02_pos_int_returns_int() {
    let src = "fn main() -> i64:\n    print(abs(5))\n    return 0\n";
    assert_build_run("abs_e2e_02", src, "5\n");
}

// =====================================================================
// abs_e2e_03 — `print(abs(-5.0))` -> "5\n". The arg is an f64; the
// result is an f64 (5.0). cobrust prints whole floats without the
// trailing `.0`, so stdout is "5\n" (the value 5.0 — Python's `5.0`).
// Routes to __cobrust_math_abs (the float regression path). ADR-0089 §5.
// =====================================================================

#[test]
fn abs_e2e_03_neg_float_returns_float() {
    let src = "fn main() -> i64:\n    print(abs(-5.0))\n    return 0\n";
    assert_build_run("abs_e2e_03", src, "5\n");
}

// =====================================================================
// abs_e2e_04 — `abs(0)` -> "0\n" (zero, int).
// =====================================================================

#[test]
fn abs_e2e_04_zero_int_returns_zero() {
    let src = "fn main() -> i64:\n    print(abs(0))\n    return 0\n";
    assert_build_run("abs_e2e_04", src, "0\n");
}

// =====================================================================
// abs_e2e_04b — `abs` of a COMPUTED integer expression (`a - b`, `a * b`).
// The load-bearing regression for the silent-miscompile fix: a binary-op
// result temp synths to `Int` (lower.rs synth_expr_ty Bin arm) so the
// int-abs dest+lowering fire. Before the fix `abs(a - b)` re-interpreted
// the i64 bits as a double and printed NaN/garbage. ADR-0089 §5.
// =====================================================================

#[test]
fn abs_e2e_04b_int_binary_expr_returns_int() {
    // abs(3 - 10) == 7, abs(-2 * 4) == 8 (NOT NaN).
    let src = "fn main() -> i64:\n    let a: i64 = 3\n    let b: i64 = 10\n    print(abs(a - b))\n    print(abs(-2 * 4))\n    return 0\n";
    assert_build_run("abs_e2e_04b", src, "7\n8\n");
}

#[test]
fn abs_e2e_04c_float_binary_expr_returns_float() {
    // abs(1.5 - 4.0) == 2.5 (the float path stays correct).
    let src = "fn main() -> i64:\n    let x: f64 = 1.5\n    let y: f64 = 4.0\n    print(abs(x - y))\n    return 0\n";
    assert_build_run("abs_e2e_04c", src, "2.5\n");
}

// =====================================================================
// abs_e2e_05 — `abs(-5) + 1` -> "6\n": the int result is usable in int
// arithmetic (the §2.5 first-try win — `abs` returns an int, not a
// float, so the `+ 1` stays integer).
// =====================================================================

#[test]
fn abs_e2e_05_int_result_in_arithmetic() {
    let src = "fn main() -> i64:\n    let r: i64 = abs(-5) + 1\n    print(r)\n    return 0\n";
    assert_build_run("abs_e2e_05", src, "6\n");
}

// =====================================================================
// abs_e2e_06 — `abs(n)` on an i64 variable (the local-typed path, not a
// literal) -> "7\n".
// =====================================================================

#[test]
fn abs_e2e_06_abs_of_int_var() {
    let src = "fn main() -> i64:\n    let n: i64 = -7\n    print(abs(n))\n    return 0\n";
    assert_build_run("abs_e2e_06", src, "7\n");
}

// =====================================================================
// range_e2e_01 — `for i in range(5):` sums to 10 (0+1+2+3+4). The 1-arg
// `range(stop)` form (== `range(0, stop)`). ADR-0089 §4.
// =====================================================================

#[test]
fn range_e2e_01_one_arg_sums_to_ten() {
    let src = "fn main() -> i64:\n    let total: i64 = 0\n    for i in range(5):\n        total = total + i\n    print(total)\n    return 0\n";
    assert_build_run("range_e2e_01", src, "10\n");
}

// =====================================================================
// range_e2e_02 — `range(0)` is empty: the loop body never runs, the
// accumulator stays 0.
// =====================================================================

#[test]
fn range_e2e_02_zero_is_empty() {
    let src = "fn main() -> i64:\n    let total: i64 = 0\n    for i in range(0):\n        total = total + i\n    print(total)\n    return 0\n";
    assert_build_run("range_e2e_02", src, "0\n");
}

// =====================================================================
// range_e2e_03 — 2-arg `range(start, stop)` REGRESSION: `range(2, 5)`
// sums to 9 (2+3+4). The 2-arg form is NOT intercepted by the 1-arg
// special-case and keeps its existing lowering.
// =====================================================================

#[test]
fn range_e2e_03_two_arg_regression_sums_to_nine() {
    let src = "fn main() -> i64:\n    let total: i64 = 0\n    for i in range(2, 5):\n        total = total + i\n    print(total)\n    return 0\n";
    assert_build_run("range_e2e_03", src, "9\n");
}

// =====================================================================
// range_e2e_04 — 1-arg `range(n)` with a variable stop -> sums 0..n-1.
// `range(4)` -> 0+1+2+3 == 6.
// =====================================================================

#[test]
fn range_e2e_04_one_arg_var_stop() {
    let src = "fn main() -> i64:\n    let n: i64 = 4\n    let total: i64 = 0\n    for i in range(n):\n        total = total + i\n    print(total)\n    return 0\n";
    assert_build_run("range_e2e_04", src, "6\n");
}
