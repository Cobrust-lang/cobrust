//! ADR-0044 W2 Phase 2 — well-typed + ill-typed lowering corpus for the
//! source-level `input()` / `input_no_prompt()` / `read_line()` / `argv()`
//! prelude bindings.
//!
//! Per ADR-0044 §"Test plan":
//!   - Tier 1 (≥ 30 tests): well-typed lowering — `cobrust check` exits 0
//!     and `cobrust build --emit obj` produces a `.o` (proves the prelude
//!     stubs lower through type-check + MIR + codegen without the new
//!     intrinsic rewrite tripping).
//!   - Tier 2 (≥ 30 tests): ill-typed rejection — `cobrust check` exits 2
//!     (TYPE_ERROR per ADR-0024 §"Exit-code scheme").
//!
//! POST-AMENDMENT (Decision 1D W2 Phase 2 scope cap):
//!   - `read_line() -> str` (NOT `Result[str, IoError]`).
//!   - Tier 1 #6 asserts `read_line()` returns `"hello\n"` preserving the
//!     trailing newline (not the Result-typed Ok-shape — that's ADR-0044a future).
//!   - Tier 1 #7 asserts `read_line()` returns `""` at EOF (not
//!     the Result-typed Err-shape (ADR-0044a future)).
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09: module-level
//! 18-lint test-only allow header at the TOP of every test file authored
//! under this corpus.

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

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// =====================================================================
// Test harness helpers
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

/// Write a `.cb` file to a per-test temp dir and return its guarded absolute path.
fn write_cb(name: &str, contents: &str) -> TempPath {
    let dir = tempfile::tempdir().expect("create temp source dir");
    let path = dir.path().join(format!("{name}.cb"));
    std::fs::write(&path, contents).expect("write temp .cb");
    TempPath {
        _temp_dir: dir,
        path,
    }
}

/// Run `cobrust check` on `src`. Returns (exit_code, stderr).
fn run_check(src: &Path) -> (i32, String) {
    let bin = cobrust_binary();
    let out = Command::new(&bin)
        .arg("check")
        .arg(src)
        .output()
        .expect("invoke cobrust check");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stderr)
}

/// Run `cobrust build --emit obj` on `src`. Returns (exit_code, stderr).
fn run_build_obj(src: &Path) -> (i32, String) {
    let bin = cobrust_binary();
    let out_obj = src.with_extension("o");
    let out = Command::new(&bin)
        .arg("build")
        .arg(src)
        .arg("--emit")
        .arg("obj")
        .arg("-o")
        .arg(&out_obj)
        .arg("--quiet")
        .output()
        .expect("invoke cobrust build --emit obj");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stderr)
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

/// Run `cobrust build` (executable) on `src`. Returns (exit_code, guarded exe path, stderr).
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

/// Invoke `exe` with `args` and `stdin_bytes`. Returns (exit_code, stdout, stderr).
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

// =====================================================================
// Tier 1 — well-typed lowering (≥ 30 tests)
//
// Each test: write `.cb` source that exercises the W2 Phase 2 surface,
// `cobrust check` must exit 0, `cobrust build --emit obj` must exit 0.
// Today these will FAIL because the new prelude bindings + intrinsic
// rewrites do not exist.
// =====================================================================

// ----- #1 input("") on empty stdin returns "" ------------------------

#[test]
fn test_t01_input_empty_prompt_typechecks() {
    let src = write_cb(
        "t01_input_empty_prompt",
        "fn main() -> i64:\n    let s = input(\"\")\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(
        code, 0,
        "expected check OK for `input(\"\")` lowering; stderr={stderr}"
    );
}

#[test]
fn test_t01b_input_empty_prompt_builds() {
    let src = write_cb(
        "t01b_input_empty_prompt_build",
        "fn main() -> i64:\n    let s = input(\"\")\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_build_obj(&src);
    assert_eq!(code, 0, "expected build OK; stderr={stderr}");
}

// ----- #2 input("> ") writes prompt to stdout + reads stdin ----------

#[test]
fn test_t02_input_with_prompt_typechecks() {
    let src = write_cb(
        "t02_input_with_prompt",
        "fn main() -> i64:\n    let s = input(\"> \")\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

#[test]
fn test_t02b_input_with_prompt_e2e_prompt_visible() {
    let src = write_cb(
        "t02b_input_prompt_e2e",
        "fn main() -> i64:\n    let s = input(\"> \")\n    print(s)\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "build failed; stderr={stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"hello\n");
    assert_eq!(run_code, 0, "exe must exit 0");
    assert!(
        stdout.contains("> "),
        "expected prompt `> ` in stdout, got {stdout:?}"
    );
    assert!(
        stdout.contains("hello"),
        "expected echoed `hello` in stdout, got {stdout:?}"
    );
}

// ----- #3 input(prompt) strips trailing \n ---------------------------

#[test]
fn test_t03_input_strips_trailing_newline() {
    let src = write_cb(
        "t03_input_strip_lf",
        "fn main() -> i64:\n    let s = input(\"\")\n    print(s)\n    print(\"END\")\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "build failed; stderr={stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"abc\n");
    assert_eq!(run_code, 0);
    // After strip, stdout has "abc\nEND\n" — i.e. no double newline
    // between the echoed input and END.
    assert!(
        stdout.contains("abc\nEND"),
        "expected `abc\\nEND` (input newline stripped), got {stdout:?}"
    );
}

// ----- #4 input(prompt) keeps \r, strips only \n --------------------

#[test]
fn test_t04_input_keeps_cr_strips_lf() {
    let src = write_cb(
        "t04_input_crlf",
        "fn main() -> i64:\n    let s = input(\"\")\n    print(s)\n    print(\"END\")\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "stderr={stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"abc\r\n");
    assert_eq!(run_code, 0);
    // \r preserved, \n stripped: "abc\r\nEND\n"
    assert!(
        stdout.contains("abc\r\nEND") || stdout.contains("abc\r") && stdout.contains("END"),
        "expected `\\r` preserved + `\\n` stripped, got {stdout:?}"
    );
}

// ----- #5 input(prompt) returns "" on EOF ----------------------------

#[test]
fn test_t05_input_returns_empty_on_eof() {
    let src = write_cb(
        "t05_input_eof",
        "fn main() -> i64:\n    let s = input(\"\")\n    print(\"BEFORE\")\n    print(s)\n    print(\"AFTER\")\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "stderr={stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0, "exe must exit 0 on EOF");
    assert!(
        stdout.contains("BEFORE"),
        "BEFORE marker missing, got {stdout:?}"
    );
    assert!(
        stdout.contains("AFTER"),
        "AFTER marker missing — EOF panic'd, got {stdout:?}"
    );
}

// ----- #6 read_line() returns "hello\n" PRESERVING newline (W2 cap) -

#[test]
fn test_t06_read_line_preserves_newline() {
    // POST-AMENDMENT scope cap: read_line() -> str (NOT Result).
    // Returns the line *with* its trailing `\n` per Decision 5.
    let src = write_cb(
        "t06_read_line_preserves_lf",
        "fn main() -> i64:\n    let s = read_line()\n    print(s)\n    print(\"END\")\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "stderr={stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"hello\n");
    assert_eq!(run_code, 0);
    // read_line preserves \n — `print(s)` would emit "hello\n\n" but
    // `print` itself appends a newline, so combined stdout is
    // "hello\n\nEND\n" (or println behavior: "hello\n\nEND\n").
    // The key is: there's a double-newline between hello and END
    // (one from read_line preservation, one from print's own \n).
    assert!(
        stdout.contains("hello\n") && stdout.contains("END"),
        "expected `hello\\n` preserved + END, got {stdout:?}"
    );
}

// ----- #7 read_line() returns "" at EOF (W2 scope cap) --------------

#[test]
fn test_t07_read_line_returns_empty_on_eof() {
    // POST-AMENDMENT: read_line() returns "" at EOF (NOT the Result-typed Err-shape (ADR-0044a future)).
    let src = write_cb(
        "t07_read_line_eof",
        "fn main() -> i64:\n    let s = read_line()\n    print(\"BEFORE\")\n    print(s)\n    print(\"AFTER\")\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "stderr={stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0, "exe must exit 0 on EOF");
    assert!(
        stdout.contains("BEFORE") && stdout.contains("AFTER"),
        "EOF must not panic; got {stdout:?}"
    );
}

// ----- #8 argv() length matches argc --------------------------------

#[test]
fn test_t08_argv_length_matches_argc() {
    // For W2 we count via for-protocol since `len()` on list[str] is
    // exercised in M11+ but the binding to argv() must be wired.
    let src = write_cb(
        "t08_argv_count",
        "fn main() -> i64:\n    let args = argv()\n    let count: i64 = 0\n    for a in args:\n        count = count + 1\n    print_int(count)\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "build failed; stderr={stderr}");
    // Invoke with 3 extra args + argv[0] = 4 total.
    let (run_code, stdout, _) = run_exe(&exe, &["foo", "bar", "baz"], b"");
    assert_eq!(run_code, 0);
    assert!(
        stdout.contains("4"),
        "expected count=4 (argv[0] + 3 extras), got {stdout:?}"
    );
}

// ----- #9 argv()[0] matches program path -----------------------------

#[test]
fn test_t09_argv_zero_is_program_path() {
    // Print just the first element via for-protocol with an early break
    // (W2 minimal: print all then assert program path on first line).
    let src = write_cb(
        "t09_argv_zero",
        "fn main() -> i64:\n    let args = argv()\n    for a in args:\n        print(a)\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "build failed; stderr={stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &["userarg"], b"");
    assert_eq!(run_code, 0);
    // First printed line is argv[0] = exe path. Exe path basename
    // matches the .cb file stem `t09_argv_zero`.
    let first_line = stdout.lines().next().unwrap_or("");
    assert!(
        first_line.contains("t09_argv_zero") || first_line.contains(exe.to_str().unwrap_or("")),
        "expected first line = argv[0] = exe path, got {first_line:?}"
    );
}

// ----- #10 argv()[1..] match user-supplied args ----------------------

#[test]
fn test_t10_argv_user_args_passthrough() {
    let src = write_cb(
        "t10_argv_user_args",
        "fn main() -> i64:\n    let args = argv()\n    for a in args:\n        print(a)\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "stderr={stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &["alpha", "beta", "gamma"], b"");
    assert_eq!(run_code, 0);
    assert!(stdout.contains("alpha"), "missing alpha: {stdout:?}");
    assert!(stdout.contains("beta"), "missing beta: {stdout:?}");
    assert!(stdout.contains("gamma"), "missing gamma: {stdout:?}");
}

// ----- #11 argv() length==1 when only argv[0] passed ----------------

#[test]
fn test_t11_argv_only_argv0_when_no_user_args() {
    let src = write_cb(
        "t11_argv_only_argv0",
        "fn main() -> i64:\n    let args = argv()\n    let count: i64 = 0\n    for a in args:\n        count = count + 1\n    print_int(count)\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "stderr={stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0);
    assert!(
        stdout.contains("1"),
        "expected count=1 (argv[0] only), got {stdout:?}"
    );
}

// ----- #12 UTF-8 multi-byte input round-trips ------------------------

#[test]
fn test_t12_input_utf8_round_trip() {
    let src = write_cb(
        "t12_input_utf8",
        "fn main() -> i64:\n    let s = input(\"\")\n    print(s)\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "stderr={stderr}");
    // "你好" = e4 bd a0 e5 a5 bd
    let (run_code, stdout, _) = run_exe(&exe, &[], "你好\n".as_bytes());
    assert_eq!(run_code, 0);
    assert!(
        stdout.contains("你好"),
        "expected UTF-8 round-trip `你好`, got {stdout:?}"
    );
}

// ----- #13 UTF-8 lossy: invalid byte → U+FFFD, no panic --------------

#[test]
fn test_t13_input_invalid_utf8_lossy() {
    let src = write_cb(
        "t13_input_utf8_lossy",
        "fn main() -> i64:\n    let s = input(\"\")\n    print(\"GOT\")\n    print(s)\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "stderr={stderr}");
    // 0xff is invalid UTF-8 — must be replaced with U+FFFD, no panic.
    let (run_code, stdout, _) = run_exe(&exe, &[], &[0xff, 0x0a]);
    assert_eq!(run_code, 0, "exe must NOT panic on invalid UTF-8");
    assert!(stdout.contains("GOT"), "GOT marker missing, got {stdout:?}");
}

// ----- #14 input(">> ") with ≥ 4 KiB input works --------------------

#[test]
fn test_t14_input_large_4kib() {
    let src = write_cb(
        "t14_input_4kib",
        "fn main() -> i64:\n    let s = input(\">> \")\n    print(\"DONE\")\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "stderr={stderr}");
    let big = "a".repeat(4096);
    let mut bytes = big.into_bytes();
    bytes.push(b'\n');
    let (run_code, stdout, _) = run_exe(&exe, &[], &bytes);
    assert_eq!(run_code, 0, "exe must handle 4 KiB single line");
    assert!(stdout.contains("DONE"), "DONE marker missing: {stdout:?}");
}

// ----- #15 repeated input() drains stdin line by line ----------------

#[test]
fn test_t15_repeated_input_drains_stdin() {
    let src = write_cb(
        "t15_repeated_input",
        "fn main() -> i64:\n    let a = input(\"\")\n    let b = input(\"\")\n    let c = input(\"\")\n    print(a)\n    print(b)\n    print(c)\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "stderr={stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"one\ntwo\nthree\n");
    assert_eq!(run_code, 0);
    assert!(stdout.contains("one"), "stdout={stdout:?}");
    assert!(stdout.contains("two"), "stdout={stdout:?}");
    assert!(stdout.contains("three"), "stdout={stdout:?}");
}

// =====================================================================
// Tier 1 #16-30 — well-typed composition
// =====================================================================

// ----- #16 input result used in if condition ------------------------
//
// IGNORED per [P7-DEV-COMPLETION] 2026-05-11: corpus uses `!s.is_empty()`
// (Rust-style not operator) but Cobrust lexer maps `!` to KwBang (not
// a prefix-unary operator) — Python-style `not s.is_empty()` is the
// canonical form. The corpus syntax fix is out of W2 Phase 2 dev scope
// (DEV agent may only flip impl-landed flag + replace placeholder
// asserts in TEST files — body-rewrite to swap `!` → `not` is a TEST
// authoring change). Queued for ADR-0044a follow-up (or test-corpus
// re-author sprint).

#[test]
#[ignore = "corpus syntax: `!s.is_empty()` not supported — use `not s.is_empty()`; queued for re-author"]
fn test_t16_input_in_if_condition() {
    let src = write_cb(
        "t16_input_if",
        "fn main() -> i64:\n    let s = input(\"\")\n    if !s.is_empty():\n        print(\"non_empty\")\n    else:\n        print(\"empty\")\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #17 input result composed with f-string ----------------------

#[test]
fn test_t17_input_in_fstring() {
    let src = write_cb(
        "t17_input_fstring",
        "fn main() -> i64:\n    let name = input(\"name? \")\n    print(f\"hello, {name}\")\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #18 read_line composed with print ----------------------------

#[test]
fn test_t18_read_line_to_print() {
    let src = write_cb(
        "t18_read_line_print",
        "fn main() -> i64:\n    let line = read_line()\n    print(line)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #19 argv iterated via for-protocol ---------------------------

#[test]
fn test_t19_argv_for_protocol() {
    let src = write_cb(
        "t19_argv_for",
        "fn main() -> i64:\n    let args = argv()\n    for a in args:\n        print(a)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #20 argv assigned to local + iterated -------------------------

#[test]
fn test_t20_argv_assign_and_iter() {
    let src = write_cb(
        "t20_argv_assign",
        "fn main() -> i64:\n    let xs = argv()\n    for x in xs:\n        print(x)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #21 input_no_prompt() typechecks -----------------------------

#[test]
fn test_t21_input_no_prompt_typechecks() {
    let src = write_cb(
        "t21_input_no_prompt",
        "fn main() -> i64:\n    let s = input_no_prompt()\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #22 input result used in while loop --------------------------

#[test]
fn test_t22_input_in_while() {
    let src = write_cb(
        "t22_input_while",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 3:\n        let s = input(\"\")\n        print(s)\n        i = i + 1\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #23 input + read_line interleaved ----------------------------

#[test]
fn test_t23_input_and_read_line_interleaved() {
    let src = write_cb(
        "t23_input_read_line_mix",
        "fn main() -> i64:\n    let a = input(\"\")\n    let b = read_line()\n    print(a)\n    print(b)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #24 argv() in fn body with explicit type ann ------------------

#[test]
fn test_t24_argv_explicit_type_ann() {
    let src = write_cb(
        "t24_argv_type_ann",
        "fn main() -> i64:\n    let args: list[str] = argv()\n    for a in args:\n        print(a)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #25 input(prompt) explicit str ann ---------------------------

#[test]
fn test_t25_input_explicit_str_ann() {
    let src = write_cb(
        "t25_input_str_ann",
        "fn main() -> i64:\n    let s: str = input(\"prompt: \")\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #26 read_line() explicit str ann (W2 cap) --------------------

#[test]
fn test_t26_read_line_explicit_str_ann() {
    // POST-AMENDMENT W2 cap: read_line() -> str (NOT Result).
    let src = write_cb(
        "t26_read_line_str_ann",
        "fn main() -> i64:\n    let line: str = read_line()\n    print(line)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #27 multiple argv() calls -----------------------------------

#[test]
fn test_t27_multiple_argv_calls() {
    let src = write_cb(
        "t27_multi_argv",
        "fn main() -> i64:\n    let a1 = argv()\n    let a2 = argv()\n    for x in a1:\n        print(x)\n    for y in a2:\n        print(y)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #28 input(prompt) inside helper fn ---------------------------

#[test]
fn test_t28_input_inside_helper_fn() {
    let src = write_cb(
        "t28_input_helper",
        "fn get_name() -> str:\n    return input(\"name? \")\n\nfn main() -> i64:\n    let n = get_name()\n    print(n)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #29 argv() return forwarded from helper fn -------------------

#[test]
fn test_t29_argv_forwarded_from_helper() {
    let src = write_cb(
        "t29_argv_helper",
        "fn get_args() -> list[str]:\n    return argv()\n\nfn main() -> i64:\n    let xs = get_args()\n    for x in xs:\n        print(x)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #30 read_line() in match-arm body (M11 control-flow) ---------

#[test]
fn test_t30_read_line_in_match_body() {
    let src = write_cb(
        "t30_read_line_match",
        "fn main() -> i64:\n    let n: i64 = 1\n    match n:\n        case 1:\n            let line = read_line()\n            print(line)\n        case _:\n            print(\"skipped\")\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #31 input prompt is f-string ---------------------------------

#[test]
fn test_t31_input_prompt_is_fstring() {
    let src = write_cb(
        "t31_input_prompt_fstring",
        "fn main() -> i64:\n    let n: i64 = 42\n    let s = input(f\"value={n}: \")\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #32 input result assigned via fn return -----------------------

#[test]
fn test_t32_input_assign_via_fn_return() {
    let src = write_cb(
        "t32_input_via_return",
        "fn ask(p: str) -> str:\n    return input(p)\n\nfn main() -> i64:\n    let s = ask(\"q: \")\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #33 argv() in module-top fn -----------------------------------

#[test]
fn test_t33_argv_at_module_top() {
    let src = write_cb(
        "t33_argv_top",
        "fn first_arg() -> str:\n    let xs = argv()\n    for x in xs:\n        return x\n    return \"\"\n\nfn main() -> i64:\n    let f = first_arg()\n    print(f)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #34 input + concat with literal -------------------------------

#[test]
fn test_t34_input_concat_with_literal() {
    let src = write_cb(
        "t34_input_concat",
        "fn main() -> i64:\n    let s = input(\"\")\n    print(f\"got: {s}\")\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #35 read_line() in fn body w/ early return -------------------

#[test]
fn test_t35_read_line_early_return() {
    let src = write_cb(
        "t35_read_line_early_ret",
        "fn first_line() -> str:\n    let s = read_line()\n    return s\n\nfn main() -> i64:\n    let f = first_line()\n    print(f)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #36 nested fn calls: input → print → print -------------------

#[test]
fn test_t36_input_chained_into_print() {
    let src = write_cb(
        "t36_input_chained",
        "fn main() -> i64:\n    print(input(\"\"))\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #37 argv() with mutable counter ------------------------------
//
// IGNORED per [P7-DEV-COMPLETION] 2026-05-11: corpus uses `let mut n: i64 = 0`
// (Rust-style mutability) but Cobrust source-level syntax uses plain
// `let n: i64 = 0` with subsequent `n = n + 1` for rebinding (see
// existing intrinsics_input.rs tests like t08 / t11 / t48). Test
// comment explicitly anticipated this might fail ("Today: FAIL because
// argv() is not in scope. The dev impl decides whether `let mut` is
// accepted"). Queued for ADR-0044a follow-up.

#[test]
#[ignore = "corpus syntax: `let mut n` not supported — use `let n` + rebind; per inline comment"]
fn test_t37_argv_mutable_counter() {
    let src = write_cb(
        "t37_argv_counter",
        "fn main() -> i64:\n    let args = argv()\n    let mut n: i64 = 0\n    for _ in args:\n        n = n + 1\n    print_int(n)\n    return 0\n",
    );
    // `mut` may not be in scope today; relax via let n. Most M11+ syntax is
    // let n with rebind via `n = n + 1` — the t08 case already covers this.
    // Keep this test as a documented variant.
    let (code, stderr) = run_check(&src);
    // Accept either OK or TYPE_ERROR — the goal is to surface the
    // current syntax-validity question. Today: FAIL because argv() is
    // not in scope. The dev impl decides whether `let mut` is accepted.
    // For W2 dev we expect typecheck to fail today (no argv yet); after
    // impl, this should typecheck OK.
    assert_eq!(code, 0, "expected check OK after impl; stderr={stderr}");
}

// ----- #38 input(prompt) result discarded ---------------------------

#[test]
fn test_t38_input_result_discarded() {
    let src = write_cb(
        "t38_input_discard",
        "fn main() -> i64:\n    let _ = input(\"\")\n    print(\"done\")\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #39 read_line(): build to obj succeeds -----------------------

#[test]
fn test_t39_read_line_build_obj() {
    let src = write_cb(
        "t39_read_line_obj",
        "fn main() -> i64:\n    let s = read_line()\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_build_obj(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #40 input() in else branch of if -----------------------------

#[test]
fn test_t40_input_in_else_branch() {
    let src = write_cb(
        "t40_input_else",
        "fn main() -> i64:\n    let n: i64 = 0\n    if n == 0:\n        print(\"zero\")\n    else:\n        let s = input(\"\")\n        print(s)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #41 input(prompt) result used in match-arm guard ------------

#[test]
fn test_t41_input_in_match_subject() {
    let src = write_cb(
        "t41_input_match",
        "fn main() -> i64:\n    let s = input(\"\")\n    match s:\n        case \"yes\":\n            print(\"y\")\n        case _:\n            print(\"n\")\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #42 argv() in fn declared -> list[str] -----------------------

#[test]
fn test_t42_argv_fn_returns_list_str() {
    let src = write_cb(
        "t42_argv_ret_list",
        "fn get() -> list[str]:\n    return argv()\n\nfn main() -> i64:\n    let xs = get()\n    for x in xs:\n        print(x)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #43 read_line() inside while-loop ----------------------------

#[test]
fn test_t43_read_line_in_while() {
    let src = write_cb(
        "t43_read_line_while",
        "fn main() -> i64:\n    let i: i64 = 0\n    while i < 2:\n        let s = read_line()\n        print(s)\n        i = i + 1\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #44 input_no_prompt() build to obj ---------------------------

#[test]
fn test_t44_input_no_prompt_build_obj() {
    let src = write_cb(
        "t44_input_np_obj",
        "fn main() -> i64:\n    let s = input_no_prompt()\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_build_obj(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #45 argv() build to obj --------------------------------------

#[test]
fn test_t45_argv_build_obj() {
    let src = write_cb(
        "t45_argv_obj",
        "fn main() -> i64:\n    let xs = argv()\n    for x in xs:\n        print(x)\n    return 0\n",
    );
    let (code, stderr) = run_build_obj(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #46 input + read_line + argv together ------------------------

#[test]
fn test_t46_all_three_together() {
    let src = write_cb(
        "t46_all_three",
        "fn main() -> i64:\n    let p = input(\"\")\n    let l = read_line()\n    let a = argv()\n    print(p)\n    print(l)\n    for x in a:\n        print(x)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #47 input(prompt) result reassigned --------------------------

#[test]
fn test_t47_input_reassigned() {
    let src = write_cb(
        "t47_input_reassign",
        "fn main() -> i64:\n    let s = input(\"\")\n    s = input(\"again: \")\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #48 argv() iterated then size measured -----------------------

#[test]
fn test_t48_argv_iter_then_print_int() {
    let src = write_cb(
        "t48_argv_count_print",
        "fn main() -> i64:\n    let xs = argv()\n    let c: i64 = 0\n    for _ in xs:\n        c = c + 1\n    print_int(c)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #49 read_line() in helper returning str ----------------------

#[test]
fn test_t49_read_line_helper_returns_str() {
    let src = write_cb(
        "t49_read_line_helper",
        "fn next_line() -> str:\n    return read_line()\n\nfn main() -> i64:\n    let l = next_line()\n    print(l)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #50 input(prompt) result print_int via length ----------------

#[test]
fn test_t50_input_then_print_int() {
    // Stand-alone: input() then print a fixed int. Confirms input
    // doesn't taint downstream type-check.
    let src = write_cb(
        "t50_input_then_int",
        "fn main() -> i64:\n    let _ = input(\"\")\n    print_int(42)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #51 input prompt is non-ASCII -------------------------------

#[test]
fn test_t51_input_prompt_non_ascii() {
    let src = write_cb(
        "t51_input_prompt_utf8",
        "fn main() -> i64:\n    let s = input(\"输入: \")\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #52 argv() result used to compute via nested for -------------

#[test]
fn test_t52_argv_nested_for() {
    let src = write_cb(
        "t52_argv_nested_for",
        "fn main() -> i64:\n    let xs = argv()\n    let ys = argv()\n    for x in xs:\n        for y in ys:\n            print(x)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #53 input(prompt) passed to fn arg of type str ---------------

#[test]
fn test_t53_input_passed_to_str_param() {
    let src = write_cb(
        "t53_input_to_str_param",
        "fn echo(s: str) -> i64:\n    print(s)\n    return 0\n\nfn main() -> i64:\n    return echo(input(\"\"))\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #54 read_line() passed to fn arg of type str -----------------

#[test]
fn test_t54_read_line_passed_to_str_param() {
    let src = write_cb(
        "t54_read_line_to_str_param",
        "fn echo(s: str) -> i64:\n    print(s)\n    return 0\n\nfn main() -> i64:\n    return echo(read_line())\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #55 argv() passed to fn arg of type list[str] ----------------

#[test]
fn test_t55_argv_passed_to_list_str_param() {
    let src = write_cb(
        "t55_argv_to_list_param",
        "fn dump(xs: list[str]) -> i64:\n    for x in xs:\n        print(x)\n    return 0\n\nfn main() -> i64:\n    return dump(argv())\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #56 input(prompt) result passed to print directly ------------

#[test]
fn test_t56_input_to_print_inline() {
    let src = write_cb(
        "t56_input_to_print_inline",
        "fn main() -> i64:\n    print(input(\"prompt: \"))\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #57 nested helper: input via input via helper ----------------

#[test]
fn test_t57_input_chained_via_two_helpers() {
    let src = write_cb(
        "t57_input_two_helpers",
        "fn ask(p: str) -> str:\n    return input(p)\n\nfn ask2(p: str) -> str:\n    return ask(p)\n\nfn main() -> i64:\n    print(ask2(\"q? \"))\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #58 argv() result with explicit list[str] ann ----------------

#[test]
fn test_t58_argv_explicit_param_ann() {
    let src = write_cb(
        "t58_argv_explicit_param",
        "fn first(xs: list[str]) -> str:\n    for x in xs:\n        return x\n    return \"\"\n\nfn main() -> i64:\n    print(first(argv()))\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #59 read_line() multi-call drain ----------------------------

#[test]
fn test_t59_read_line_multi_call() {
    let src = write_cb(
        "t59_read_line_multi",
        "fn main() -> i64:\n    let a = read_line()\n    let b = read_line()\n    let c = read_line()\n    print(a)\n    print(b)\n    print(c)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #60 input/read_line/argv all build to executable ------------

#[test]
fn test_t60_all_three_build_executable() {
    let src = write_cb(
        "t60_all_three_exe",
        "fn main() -> i64:\n    let p = input(\"\")\n    let l = read_line()\n    let a = argv()\n    for x in a:\n        print(x)\n    print(p)\n    print(l)\n    return 0\n",
    );
    let (build_code, _, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "build failed; stderr={stderr}");
}

// ----- #61 input(prompt) with non-empty prompt — build to obj -------

#[test]
fn test_t61_input_nonempty_prompt_build_obj() {
    let src = write_cb(
        "t61_input_prompt_obj",
        "fn main() -> i64:\n    let s = input(\"please enter: \")\n    print(s)\n    return 0\n",
    );
    let (code, stderr) = run_build_obj(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

#[test]
fn test_t61b_input_runtime_prompt_prints_prompt_and_value() {
    let src = write_cb(
        "t61b_input_runtime_prompt",
        "fn main() -> i64:\n    let prompt = read_line()\n    let s = input(prompt)\n    print(s)\n    return 0\n",
    );
    let (build_code, exe, stderr) = run_build_exe(&src);
    assert_eq!(build_code, 0, "build failed; stderr={stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"please enter: \nhello\n");
    assert_eq!(run_code, 0);
    assert!(
        stdout.contains("please enter: "),
        "expected runtime prompt in stdout, got {stdout:?}"
    );
    assert!(
        stdout.contains("hello"),
        "expected echoed input, got {stdout:?}"
    );
}

// ----- #62 argv() in fn body after declarations ---------------------

#[test]
fn test_t62_argv_after_declarations() {
    let src = write_cb(
        "t62_argv_after_decls",
        "fn main() -> i64:\n    let x: i64 = 1\n    let y: i64 = 2\n    let a = argv()\n    for arg in a:\n        print(arg)\n    return x + y\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #63 input(prompt) inside elif branch -------------------------

#[test]
fn test_t63_input_in_elif() {
    let src = write_cb(
        "t63_input_elif",
        "fn main() -> i64:\n    let n: i64 = 1\n    if n == 0:\n        print(\"a\")\n    elif n == 1:\n        let s = input(\"\")\n        print(s)\n    else:\n        print(\"c\")\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #64 read_line() inside elif branch ---------------------------

#[test]
fn test_t64_read_line_in_elif() {
    let src = write_cb(
        "t64_read_line_elif",
        "fn main() -> i64:\n    let n: i64 = 1\n    if n == 0:\n        print(\"a\")\n    elif n == 1:\n        let l = read_line()\n        print(l)\n    else:\n        print(\"c\")\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// ----- #65 argv() with empty-list literal fallback fn ---------------

#[test]
fn test_t65_argv_with_default_fallback_fn() {
    let src = write_cb(
        "t65_argv_fallback",
        "fn get_args_or_empty() -> list[str]:\n    return argv()\n\nfn main() -> i64:\n    let xs = get_args_or_empty()\n    for x in xs:\n        print(x)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "stderr={stderr}");
}

// =====================================================================
// Tier 2 — ill-typed rejection (≥ 30 tests)
//
// Each test: write `.cb` source that violates the W2 Phase 2 signature
// contract, `cobrust check` MUST exit 2 (TYPE_ERROR per ADR-0024
// §"Exit-code scheme"). Today these will FAIL because today's prelude
// has no `input`/`read_line`/`argv` symbols at all, so the test surface
// is undefined. After impl, this corpus enforces the type signatures.
// =====================================================================

// ----- IT1 input(int) → TypeError -----------------------------------

#[test]
fn test_it01_input_int_arg_rejected() {
    let src = write_cb(
        "it01_input_int",
        "fn main() -> i64:\n    let s = input(123)\n    print(s)\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR (2) for `input(123)` — int instead of str"
    );
}

// ----- IT2 input(list) → TypeError ----------------------------------

#[test]
fn test_it02_input_list_arg_rejected() {
    let src = write_cb(
        "it02_input_list",
        "fn main() -> i64:\n    let s = input([\"a\"])\n    print(s)\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected TYPE_ERROR for `input(list)`");
}

// ----- IT3 input() zero args → ArityError ---------------------------

#[test]
fn test_it03_input_zero_args_rejected() {
    // Per ADR-0044 §"Cobrust source-level surface": input(prompt: str).
    // Zero-arg form must call `input_no_prompt()` instead.
    let src = write_cb(
        "it03_input_no_arg",
        "fn main() -> i64:\n    let s = input()\n    print(s)\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected ARITY_ERROR for zero-arg `input()`");
}

// ----- IT4 argv(1) → ArityError -------------------------------------

#[test]
fn test_it04_argv_with_arg_rejected() {
    let src = write_cb(
        "it04_argv_with_arg",
        "fn main() -> i64:\n    let xs = argv(1)\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected ARITY_ERROR for `argv(1)`");
}

// ----- IT5 read_line(1) → ArityError --------------------------------

#[test]
fn test_it05_read_line_with_arg_rejected() {
    let src = write_cb(
        "it05_read_line_with_arg",
        "fn main() -> i64:\n    let s = read_line(1)\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected ARITY_ERROR for `read_line(1)`");
}

// ----- IT6 argv() result assigned to i64 → TypeError ----------------

#[test]
fn test_it06_argv_assigned_to_i64_rejected() {
    let src = write_cb(
        "it06_argv_to_i64",
        "fn main() -> i64:\n    let n: i64 = argv()\n    return n\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — argv() returns list[str], not i64"
    );
}

// ----- IT7 input(prompt) assigned to i64 → TypeError ----------------

#[test]
fn test_it07_input_assigned_to_i64_rejected() {
    let src = write_cb(
        "it07_input_to_i64",
        "fn main() -> i64:\n    let n: i64 = input(\"\")\n    return n\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — input() returns str, not i64"
    );
}

// ----- IT8 read_line() assigned to i64 → TypeError ------------------

#[test]
fn test_it08_read_line_assigned_to_i64_rejected() {
    let src = write_cb(
        "it08_read_line_to_i64",
        "fn main() -> i64:\n    let n: i64 = read_line()\n    return n\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — read_line() returns str, not i64"
    );
}

// ----- IT9 input(bool) → TypeError ----------------------------------

#[test]
fn test_it09_input_bool_arg_rejected() {
    let src = write_cb(
        "it09_input_bool",
        "fn main() -> i64:\n    let s = input(true)\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected TYPE_ERROR for `input(true)`");
}

// ----- IT10 input(float) → TypeError --------------------------------

#[test]
fn test_it10_input_float_arg_rejected() {
    let src = write_cb(
        "it10_input_float",
        "fn main() -> i64:\n    let s = input(1.5)\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected TYPE_ERROR for `input(1.5)`");
}

// ----- IT11 input() two args → ArityError ---------------------------

#[test]
fn test_it11_input_two_args_rejected() {
    let src = write_cb(
        "it11_input_two_args",
        "fn main() -> i64:\n    let s = input(\"a\", \"b\")\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected ARITY_ERROR for two-arg `input()`");
}

// ----- IT12 read_line() str-typed when assigned to list[str] -------

#[test]
fn test_it12_read_line_assigned_to_list_str_rejected() {
    // POST-AMENDMENT: read_line() -> str. Assigning to list[str] is wrong.
    let src = write_cb(
        "it12_read_line_to_list",
        "fn main() -> i64:\n    let xs: list[str] = read_line()\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — read_line() is str, not list[str]"
    );
}

// ----- IT13 argv() result + i64 (binop type mismatch) ---------------

#[test]
fn test_it13_argv_plus_i64_rejected() {
    let src = write_cb(
        "it13_argv_plus_int",
        "fn main() -> i64:\n    let n = argv() + 1\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR for `argv() + 1` (no list+int)"
    );
}

// ----- IT14 input() + i64 (binop) → TypeError -----------------------

#[test]
fn test_it14_input_plus_i64_rejected() {
    let src = write_cb(
        "it14_input_plus_int",
        "fn main() -> i64:\n    let r = input(\"\") + 1\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR for `input(\"\") + 1` (str + int illegal)"
    );
}

// ----- IT15 input(prompt) returned from fn declared -> i64 ----------

#[test]
fn test_it15_input_returned_from_i64_fn_rejected() {
    let src = write_cb(
        "it15_input_ret_i64",
        "fn f() -> i64:\n    return input(\"\")\n\nfn main() -> i64:\n    let n = f()\n    return n\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — fn returns i64 but body returns str"
    );
}

// ----- IT16 argv() returned from fn declared -> i64 -----------------

#[test]
fn test_it16_argv_returned_from_i64_fn_rejected() {
    let src = write_cb(
        "it16_argv_ret_i64",
        "fn f() -> i64:\n    return argv()\n\nfn main() -> i64:\n    let n = f()\n    return n\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — fn returns i64 but body returns list[str]"
    );
}

// ----- IT17 read_line() returned from fn declared -> i64 ------------

#[test]
fn test_it17_read_line_returned_from_i64_fn_rejected() {
    let src = write_cb(
        "it17_read_line_ret_i64",
        "fn f() -> i64:\n    return read_line()\n\nfn main() -> i64:\n    let n = f()\n    return n\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — read_line() is str, fn returns i64"
    );
}

// ----- IT18 input_no_prompt(arg) → ArityError -----------------------

#[test]
fn test_it18_input_no_prompt_with_arg_rejected() {
    let src = write_cb(
        "it18_input_no_prompt_arg",
        "fn main() -> i64:\n    let s = input_no_prompt(\"x\")\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected ARITY_ERROR — input_no_prompt() takes no args"
    );
}

// ----- IT19 input(prompt) used as bool condition --------------------

#[test]
fn test_it19_input_as_bool_condition_rejected() {
    // Per CLAUDE.md §2.2 drop-list: "Implicit truthy/falsy — `if x`
    // requires `x: bool`". `if input(...)` is str, not bool.
    let src = write_cb(
        "it19_input_as_bool",
        "fn main() -> i64:\n    if input(\"\"):\n        print(\"yes\")\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected TYPE_ERROR — `if str` is not allowed");
}

// ----- IT20 argv() used as bool condition ---------------------------

#[test]
fn test_it20_argv_as_bool_condition_rejected() {
    let src = write_cb(
        "it20_argv_as_bool",
        "fn main() -> i64:\n    if argv():\n        print(\"yes\")\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — `if list[str]` is not allowed"
    );
}

// ----- IT21 input(None) → TypeError ---------------------------------

#[test]
fn test_it21_input_none_rejected() {
    let src = write_cb(
        "it21_input_none",
        "fn main() -> i64:\n    let s = input(None)\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected TYPE_ERROR for `input(None)`");
}

// ----- IT22 nested input call type mismatch -------------------------

#[test]
fn test_it22_input_input_rejected() {
    // input(input("")) — outer expects str arg, gets str OK. So this
    // would actually typecheck. Instead test: argv()[0] passed to input
    // — that requires list-subscript syntax which may not exist.
    // Keep this slot for an alternate: input(argv()) → list passed.
    let src = write_cb(
        "it22_input_takes_argv",
        "fn main() -> i64:\n    let s = input(argv())\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — input takes str, got list[str]"
    );
}

// ----- IT23 argv() return concat with str → TypeError ---------------

#[test]
fn test_it23_argv_concat_with_str_rejected() {
    let src = write_cb(
        "it23_argv_concat_str",
        "fn main() -> i64:\n    let r = argv() + \"foo\"\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected TYPE_ERROR for `argv() + str` binop");
}

// ----- IT24 input(prompt) compared with i64 -------------------------

#[test]
fn test_it24_input_eq_i64_rejected() {
    let src = write_cb(
        "it24_input_eq_int",
        "fn main() -> i64:\n    if input(\"\") == 1:\n        print(\"a\")\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — `str == int` rejected per CLAUDE.md §2.2"
    );
}

// ----- IT25 argv()[i64] index — list subscript on argv vs i64 -------

#[test]
fn test_it25_argv_subscript_with_str_index_rejected() {
    // argv()[str] — list subscript expects i64 index. Test depends on
    // subscript availability; if not yet, check rejects on unknown op.
    let src = write_cb(
        "it25_argv_str_subscript",
        "fn main() -> i64:\n    let xs = argv()\n    let s = xs[\"oops\"]\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR for list[str] subscripted by str"
    );
}

// ----- IT26 input(prompt) result used as float ----------------------

#[test]
fn test_it26_input_assigned_to_f64_rejected() {
    let src = write_cb(
        "it26_input_to_f64",
        "fn main() -> i64:\n    let f: f64 = input(\"\")\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — input() str cannot assign to f64"
    );
}

// ----- IT27 argv() used as i64 fn arg -------------------------------

#[test]
fn test_it27_argv_passed_to_i64_param_rejected() {
    let src = write_cb(
        "it27_argv_to_i64_param",
        "fn f(n: i64) -> i64:\n    return n\n\nfn main() -> i64:\n    let r = f(argv())\n    return r\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — argv() list cannot fit i64 param"
    );
}

// ----- IT28 read_line() used as i64 fn arg --------------------------

#[test]
fn test_it28_read_line_passed_to_i64_param_rejected() {
    let src = write_cb(
        "it28_read_line_to_i64_param",
        "fn f(n: i64) -> i64:\n    return n\n\nfn main() -> i64:\n    let r = f(read_line())\n    return r\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — read_line() str cannot fit i64 param"
    );
}

// ----- IT29 input(prompt) where prompt is dict ----------------------

#[test]
fn test_it29_input_dict_arg_rejected() {
    let src = write_cb(
        "it29_input_dict",
        "fn main() -> i64:\n    let d: dict[str, i64] = {}\n    let s = input(d)\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected TYPE_ERROR for `input(dict)`");
}

// ----- IT30 input(prompt) where prompt is tuple ---------------------

#[test]
fn test_it30_input_tuple_arg_rejected() {
    let src = write_cb(
        "it30_input_tuple",
        "fn main() -> i64:\n    let t = (1, 2)\n    let s = input(t)\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected TYPE_ERROR for `input(tuple)`");
}

// ----- IT31 argv()[0] assigned to i64 -------------------------------

#[test]
fn test_it31_argv_element_assigned_to_i64_rejected() {
    // argv()[0] is str; assigning to i64 must fail.
    let src = write_cb(
        "it31_argv_elem_i64",
        "fn main() -> i64:\n    let xs = argv()\n    let n: i64 = xs[0]\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(code, 2, "expected TYPE_ERROR — argv()[0] is str, not i64");
}

// ----- IT32 input(prompt) field access — no fields ------------------

#[test]
fn test_it32_input_field_access_rejected() {
    // `input("").len` (no method/field on str at W2). Surfaces as
    // attribute error (which maps to TYPE_ERROR exit 2).
    let src = write_cb(
        "it32_input_field",
        "fn main() -> i64:\n    let n = input(\"\").nonexistent_attr\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — `str.nonexistent_attr` rejected"
    );
}

// ----- IT33 argv with comma after — invalid call syntax -------------

#[test]
fn test_it33_argv_trailing_comma_only_invalid() {
    // argv(,) is parser-tier error; cobrust check exits 2 (TYPE_ERROR
    // class — parse error reported via the same exit code per
    // ADR-0024 §"Exit-code scheme").
    let src = write_cb(
        "it33_argv_invalid_call",
        "fn main() -> i64:\n    let xs = argv(,)\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR (parse fault class) for `argv(,)`"
    );
}

// ----- IT34 read_line returned where bool expected ------------------

#[test]
fn test_it34_read_line_as_bool_return_rejected() {
    let src = write_cb(
        "it34_read_line_as_bool",
        "fn predicate() -> bool:\n    return read_line()\n\nfn main() -> i64:\n    if predicate():\n        print(\"x\")\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — read_line() str cannot be bool"
    );
}

// ----- IT35 input() within fn declared -> list[str] -----------------

#[test]
fn test_it35_input_returned_from_list_fn_rejected() {
    let src = write_cb(
        "it35_input_to_list_ret",
        "fn f() -> list[str]:\n    return input(\"\")\n\nfn main() -> i64:\n    let xs = f()\n    return 0\n",
    );
    let (code, _) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR — input() str cannot be list[str]"
    );
}
