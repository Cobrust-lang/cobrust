//! M-F.3.6 — File IO completion end-to-end corpus (ADR-0050f Tier C+D).
//!
//! Locks the source-level surface for the 7 PRELUDE fns added by
//! M-F.3.6: `read_file` / `read_file_lines` / `write_file` /
//! `append_file` / `stdin_read_all` / `stdout_write` / `stderr_write`.
//!
//! Pre-impl status (TEST corpus baseline at branch
//! `feature/f3-file-io-test` off `main@f4a90ae`; ADR-0050f accepted
//! at `c524738`):
//!
//!   - The PRELUDE in `crates/cobrust-cli/src/build.rs:51` does NOT
//!     yet declare the 7 new fns. Source-level calls to e.g.
//!     `read_file("/tmp/x.txt")` produce `UnknownName` errors.
//!   - The C-ABI shims (`__cobrust_read_file`, `__cobrust_write_file`,
//!     `__cobrust_read_file_lines`, `__cobrust_append_file`,
//!     `__cobrust_stdin_read_all`, `__cobrust_stdout_write`,
//!     `__cobrust_stderr_write`) do NOT exist in
//!     `crates/cobrust-stdlib/src/io.rs`.
//!   - Therefore every test in this file SHOULD FAIL pre-impl and is
//!     marked `#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV"]`.
//!     The DEV PAIR removes the ignore markers as each sub-sprint closes.
//!
//! Test families:
//!
//! - `f3fio01..f3fio05` — `write_file` + `read_file` round-trip.
//! - `f3fio06..f3fio10` — `read_file_lines` over a known 3-line file.
//! - `f3fio11..f3fio12` — `append_file` extends existing file.
//! - `f3fio13..f3fio15` — `stdin_read_all` / `stdout_write` /
//!   `stderr_write` via Cobrust run subprocess + captured stdio.
//! - `f3fio_bug01..f3fio_bug03` — F30 bug-witness regression per
//!   ADR-0050f §"F30 §Consequences" + Q2 newline resolution.
//!
//! All filesystem operations use `tempfile::TempDir` for isolation;
//! no test leaves files on disk after completion.
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09:
//! 18-lint clippy module-level allow header at the TOP of the file.

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
#![allow(clippy::too_many_lines)]

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

struct TempCbSource {
    _temp_dir: tempfile::TempDir,
    path: PathBuf,
}

impl std::ops::Deref for TempCbSource {
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

fn write_cb(name: &str, contents: &str) -> TempCbSource {
    let dir = tempfile::tempdir().expect("create temp source dir");
    let path = dir.path().join(format!("{name}.cb"));
    std::fs::write(&path, contents).expect("write temp .cb");
    TempCbSource {
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

/// Build and run a Cobrust source program with an optional data file path
/// injected via argv[1]. Returns (run_exit_code, stdout, stderr).
fn build_run_with_args(
    name: &str,
    src: &str,
    args: &[&str],
    stdin_bytes: &[u8],
) -> (i32, String, String) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "{name}: build failed\nstderr={build_stderr}");
    run_exe(&exe, args, stdin_bytes)
}

/// Build and run; assert exit 0 and stdout matches expected.
fn assert_build_run(name: &str, src: &str, args: &[&str], stdin: &[u8], expected_stdout: &str) {
    let (run_code, stdout, run_stderr) = build_run_with_args(name, src, args, stdin);
    assert_eq!(run_code, 0, "{name}: run failed\nstderr={run_stderr}");
    assert_eq!(
        stdout, expected_stdout,
        "{name}: stdout mismatch\nstderr={run_stderr}"
    );
}

// =====================================================================
// f3fio01..f3fio05 — write_file + read_file round-trip.
//
// Each test writes a file to a tempdir, then reads it back and prints
// the contents. The program receives the file path via argv[1] so
// the test can inject a fresh tempfile path without hardcoding /tmp.
// =====================================================================

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio01_write_then_read_hello_roundtrip() {
    // write_file("hello") then read_file and print confirms the round-trip
    // identity. write_file returns 0 on success (i64-sentinel Q1).
    // Path comes from argv[1] so test passes a real tempfile path.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    assert_build_run(
        "f3fio01_write_read",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let rc: i64 = write_file(p, \"hello\")\n    if rc != 0:\n        return rc\n    let contents: str = read_file(p)\n    let _ = print(contents)\n    return 0\n",
        &[&path],
        b"",
        "hello",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio02_write_empty_string_then_read_back() {
    // write_file("") to an empty file, then read_file returns "".
    // Tests that empty-string write + empty-string read are consistent.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    assert_build_run(
        "f3fio02_write_empty",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let rc: i64 = write_file(p, \"\")\n    if rc != 0:\n        return rc\n    let contents: str = read_file(p)\n    let n: i64 = str_len(contents)\n    print_int(n)\n    return 0\n",
        &[&path],
        b"",
        "0\n",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio03_write_multiline_then_read_back_full() {
    // write_file with a multi-line string; read_file returns all bytes
    // including embedded newlines as a single str.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    assert_build_run(
        "f3fio03_write_multiline",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let rc: i64 = write_file(p, \"line1\\nline2\\nline3\\n\")\n    if rc != 0:\n        return rc\n    let contents: str = read_file(p)\n    let _ = print(contents)\n    return 0\n",
        &[&path],
        b"",
        "line1\nline2\nline3\n",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio04_write_file_returns_zero_sentinel_on_success() {
    // write_file success path returns exactly 0.
    // Locks the i64-sentinel Q1 resolution: 0 = success.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    assert_build_run(
        "f3fio04_write_sentinel_zero",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let rc: i64 = write_file(p, \"x\")\n    print_int(rc)\n    return 0\n",
        &[&path],
        b"",
        "0\n",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio05_write_truncates_existing_contents() {
    // write_file on an existing file truncates; a second write replaces
    // the first. The final read returns only the second content.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    // Pre-populate via Rust so we can test truncation.
    std::fs::write(&path, "old content").expect("setup pre-populate");
    assert_build_run(
        "f3fio05_write_truncate",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let rc: i64 = write_file(p, \"new\")\n    if rc != 0:\n        return rc\n    let contents: str = read_file(p)\n    let _ = print(contents)\n    return 0\n",
        &[&path],
        b"",
        "new",
    );
}

// =====================================================================
// f3fio06..f3fio10 — read_file_lines over a known file.
//
// Tests write a known 3-line file (via Rust's std::fs::write for
// setup purity, not via Cobrust write_file), then call
// read_file_lines and iterate the resulting list[str].
//
// ADR-0050f Q2: \n and \r\n both stripped per line;
// trailing empty line preserved per s.split('\n') semantics.
// =====================================================================

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio06_read_file_lines_three_lf_lines_each_printed() {
    // File with 3 LF-terminated lines: "a\nb\nc\n".
    // read_file_lines returns ["a", "b", "c", ""] (4 elements; trailing
    // empty per Q2 trailing-empty-line-preserved resolution).
    // for-loop print prints each element; empty string prints nothing.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    std::fs::write(&path, "alpha\nbeta\ngamma\n").expect("setup");
    assert_build_run(
        "f3fio06_lines_lf",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let xs: list[str] = read_file_lines(p)\n    for s in xs:\n        let _ = print(s)\n        let _ = print(\"\\n\")\n    return 0\n",
        &[&path],
        b"",
        "alpha\nbeta\ngamma\n\n",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio07_read_file_lines_three_crlf_lines_cr_stripped() {
    // File with CRLF line endings: "a\r\nb\r\nc\r\n".
    // Q2: \r stripped, so lines are ["a", "b", "c", ""].
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    std::fs::write(&path, "a\r\nb\r\nc\r\n").expect("setup");
    assert_build_run(
        "f3fio07_lines_crlf",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let xs: list[str] = read_file_lines(p)\n    let n: i64 = list_len(xs)\n    print_int(n)\n    return 0\n",
        &[&path],
        b"",
        "4\n",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio08_read_file_lines_count_equals_newlines_plus_one() {
    // Round-trip identity check: for a file with N embedded newlines,
    // read_file_lines returns N+1 elements. Tests the Q2 resolution
    // "count matches s.count('\\n') + 1".
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    // 2 newlines → 3 elements ("x", "y", "").
    std::fs::write(&path, "x\ny\n").expect("setup");
    assert_build_run(
        "f3fio08_lines_count",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let xs: list[str] = read_file_lines(p)\n    let n: i64 = list_len(xs)\n    print_int(n)\n    return 0\n",
        &[&path],
        b"",
        "3\n",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio09_read_file_lines_elements_have_no_trailing_newline() {
    // Each element from read_file_lines has its \n stripped.
    // Print the first element and assert no trailing newline in element.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    std::fs::write(&path, "hello\nworld\n").expect("setup");
    assert_build_run(
        "f3fio09_lines_no_newline",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let xs: list[str] = read_file_lines(p)\n    let s: str = xs[0]\n    let n: i64 = str_len(s)\n    print_int(n)\n    return 0\n",
        &[&path],
        b"",
        "5\n",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio10_read_file_lines_second_element_correct_content() {
    // Index into read_file_lines result and assert line content.
    // Locks that list[str] indexing returns the right line.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    std::fs::write(&path, "first\nsecond\nthird\n").expect("setup");
    assert_build_run(
        "f3fio10_lines_index",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let xs: list[str] = read_file_lines(p)\n    let s: str = xs[1]\n    let _ = print(s)\n    return 0\n",
        &[&path],
        b"",
        "second",
    );
}

// =====================================================================
// f3fio11..f3fio12 — append_file extends existing file.
//
// append_file creates if absent and appends if present (per ADR-0050f
// §"Decision" row 4 + Q3 "always create if absent").
// =====================================================================

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio11_append_file_extends_existing_content() {
    // File pre-populated with "hello"; append_file adds " world".
    // read_file returns "hello world" — confirming accumulation.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    std::fs::write(&path, "hello").expect("setup");
    assert_build_run(
        "f3fio11_append_extend",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let rc: i64 = append_file(p, \" world\")\n    if rc != 0:\n        return rc\n    let contents: str = read_file(p)\n    let _ = print(contents)\n    return 0\n",
        &[&path],
        b"",
        "hello world",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio12_append_file_creates_new_file_if_absent() {
    // append_file on a non-existent path creates it.
    // ADR-0050f §"Decision" row 4: "creating if absent".
    let tmp_dir = tempfile::tempdir().expect("tempdir");
    let path = tmp_dir.path().join("newfile.txt");
    let path_str = path.to_str().expect("utf8 path").to_owned();
    // Assert file does not exist before test.
    assert!(!path.exists(), "file should not exist before append");
    assert_build_run(
        "f3fio12_append_create",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let rc: i64 = append_file(p, \"created\")\n    if rc != 0:\n        return rc\n    let contents: str = read_file(p)\n    let _ = print(contents)\n    return 0\n",
        &[&path_str],
        b"",
        "created",
    );
}

// =====================================================================
// f3fio13..f3fio15 — stdin_read_all / stdout_write / stderr_write
// via Cobrust run subprocess + captured stdio.
// =====================================================================

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio13_stdin_read_all_captures_full_input() {
    // stdin_read_all() reads until EOF; the test pipes input via stdin.
    // Prints the byte length of the captured string to confirm full read.
    assert_build_run(
        "f3fio13_stdin_read_all",
        "fn main() -> i64:\n    let s: str = stdin_read_all()\n    let n: i64 = str_len(s)\n    print_int(n)\n    return 0\n",
        &[],
        b"hello world",
        "11\n",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio14_stdout_write_no_trailing_newline() {
    // stdout_write(s) writes s without a trailing newline.
    // ADR-0050f §"Cross-surface dispatch table": stdout_write
    // differs from print(s) which appends \n.
    // We assert the exact bytes captured on stdout.
    assert_build_run(
        "f3fio14_stdout_write_no_nl",
        "fn main() -> i64:\n    let rc: i64 = stdout_write(\"no newline\")\n    return 0\n",
        &[],
        b"",
        "no newline",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio15_stderr_write_goes_to_stderr_not_stdout() {
    // stderr_write(s) writes to stderr only; stdout remains empty.
    // Tests the surface-separation between stdout_write and stderr_write.
    let path = write_cb(
        "f3fio15_stderr_write",
        "fn main() -> i64:\n    let rc: i64 = stderr_write(\"err msg\")\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed\nstderr={build_stderr}");
    let (run_code, stdout, stderr) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0, "run failed\nstderr={stderr}");
    // stdout must be empty — stderr_write does NOT go to stdout.
    assert_eq!(stdout, "", "stdout must be empty; stderr_write goes to stderr");
    // stderr must contain the message.
    assert!(
        stderr.contains("err msg"),
        "stderr must contain 'err msg'; got: {stderr:?}"
    );
}

// =====================================================================
// f3fio_bug01..f3fio_bug03 — F30 bug-witness regression tests.
//
// Per ADR-0050f mission §"Tier D — F30 bug-witness regression":
// these 3 tests lock edge-case behaviors that DEV must implement
// correctly, preventing future predicate-flip regressions.
// =====================================================================

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio_bug01_read_file_lines_empty_file_returns_empty_list() {
    // F30 bug-witness: read_file_lines on an empty file returns a
    // list with exactly 1 element: the empty string "".
    //
    // Rationale: empty file has 0 newlines; s.split('\n') on "" returns
    // [""] — one element. list_len = 1, not 0.
    //
    // A naive impl that returns [] for an empty file is WRONG.
    // This test locks the correct ADR-0050f Q2 behavior.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    std::fs::write(&path, "").expect("setup empty file");
    assert_build_run(
        "f3fio_bug01_empty_lines",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let xs: list[str] = read_file_lines(p)\n    let n: i64 = list_len(xs)\n    print_int(n)\n    return 0\n",
        &[&path],
        b"",
        "1\n",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio_bug02_read_file_lines_trailing_newline_preserved_as_empty_elem() {
    // F30 bug-witness: file ending with \n has a trailing empty string
    // element in the result of read_file_lines.
    //
    // "a\nb\n" → split('\n') = ["a", "b", ""] → 3 elements.
    //
    // This diverges from Python's readlines() but matches s.split('\n')
    // per ADR-0050f Q2 decision. A DEV implementation that strips the
    // trailing empty element is WRONG.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    std::fs::write(&path, "a\nb\n").expect("setup");
    assert_build_run(
        "f3fio_bug02_trailing_newline_elem",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let xs: list[str] = read_file_lines(p)\n    let n: i64 = list_len(xs)\n    print_int(n)\n    return 0\n",
        &[&path],
        b"",
        "3\n",
    );
}

#[test]
#[ignore = "M-F.3.6 pre-impl — remove ignore post-DEV (Sub-sprint 1+2)"]
fn f3fio_bug03_fstring_with_read_file_str_hole_correct_dispatch() {
    // F30 bug-witness: f-string with read_file(path) result slotted
    // into a Str hole. Locks the f-string Str-hole dispatch fix
    // (Wave 2 commit 9c8b1d2 per ADR-0050f §"F30 §Consequences —
    // f-string Str hole dispatch") against file-IO regression.
    //
    // A predicate-flip in the f-string Str-hole dispatch would cause
    // the f-string to skip the Str hole, producing incorrect output.
    // This test is the runtime-level lock on top of the type-level
    // lock in w193_fstring_with_read_file_return_str_hole.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let path = tmp.path().to_str().expect("utf8 path").to_owned();
    std::fs::write(&path, "world").expect("setup");
    assert_build_run(
        "f3fio_bug03_fstring_str_hole",
        "fn main() -> i64:\n    let p: str = argv()[1]\n    let contents: str = read_file(p)\n    let result: str = f\"hello {contents}\"\n    let _ = print(result)\n    return 0\n",
        &[&path],
        b"",
        "hello world",
    );
}
