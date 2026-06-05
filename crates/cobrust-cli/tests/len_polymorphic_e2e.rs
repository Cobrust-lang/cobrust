// ADR-0088 end-to-end corpus for the Python-canonical free-function
// `len(x)` builtin — the §2.5 LLM-first sized-type fix.
//
// Per ADR-0088 §3/§4: the bare `len(x)` free-function (the spelling a
// Python LLM writes constantly) now accepts ANY sized argument — a
// `str`, a `list[T]`, or a `dict[K, V]` — and returns `i64`, where the
// pre-ADR-0088 type-checker rejected `len("abc")` / `len([1,2,3])` with
// the misleading `type mismatch: expected Dict[?,?]` (the dict-only
// PRELUDE stub leaked). These tests REAL-compile -> link -> spawn a
// `.cb` program and assert the produced executable's stdout / exit code.
//
// Test families:
//   - len_e2e_01 — `print(len("hello"))` -> "5\n" (str literal; byte
//     count, agreeing with the str method-form `s.len()`).
//   - len_e2e_02 — build a `list[i64]` then `print(len(xs))` -> "3\n".
//   - len_e2e_03 — `len("")` -> "0\n" + `len([])` (via list_new(0)) -> "0\n".
//   - len_e2e_04 — `len(d)` on a `dict[i64,i64]` type-checks + links +
//     runs (the dict-len RUNTIME count is pre-existing `#[ignore]`'d
//     debt — f3d08 in dict_e2e.rs — so this asserts only build+exit, not
//     the printed count).
//   - len_e2e_05 — `len(5)` (a non-sized arg) is REJECTED at type-check
//     with exit code 2 (TYPE_ERROR); the stderr names `len` (the §2.5-B
//     fix-naming check) and does NOT say "expected Dict".
//   - len_e2e_06 — runtime str via `input` then `len(s)` -> the byte
//     count (exercises the heap-buffer path, not just literals).
//
// Per `feedback_p9_clippy_stall_pattern.md`: module-level test-only
// lint allow header.

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
fn assert_build_run(name: &str, src: &str, args: &[&str], stdin: &[u8], expected_stdout: &str) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "{name}: build failed; stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    let (run_code, stdout, run_stderr) = run_exe(&exe, args, stdin);
    assert_eq!(run_code, 0, "{name}: run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, expected_stdout,
        "{name}: stdout mismatch\nstderr={run_stderr}"
    );
}

/// Build only; assert build succeeds and the run exits 0 (used where the
/// printed value is pre-existing `#[ignore]`'d debt — dict-len runtime).
fn assert_build_runs_clean(name: &str, src: &str) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "{name}: build failed; stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    let (run_code, _stdout, run_stderr) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0, "{name}: run failed; stderr={run_stderr}");
}

// =====================================================================
// len_e2e_01 — `print(len("hello"))` on a str literal -> "5\n".
// Byte count; agrees with the str method-form `s.len()` (str_len ->
// __cobrust_str_len_src). ADR-0088 §4.
// =====================================================================

#[test]
fn len_e2e_01_str_literal_returns_byte_count() {
    let src = "fn main() -> i64:\n    print(len(\"hello\"))\n    return 0\n";
    assert_build_run("len_e2e_01", src, &[], b"", "5\n");
}

// =====================================================================
// len_e2e_02 — build a `list[i64]` then `print(len(xs))` -> "3\n".
// Routes to __cobrust_list_len (type-erased over the elem type).
// =====================================================================

#[test]
fn len_e2e_02_list_literal_returns_count() {
    let src =
        "fn main() -> i64:\n    let xs: list[i64] = [1, 2, 3]\n    print(len(xs))\n    return 0\n";
    assert_build_run("len_e2e_02", src, &[], b"", "3\n");
}

// =====================================================================
// len_e2e_03 — empty cases: `len("")` -> 0 and `len([])` (via
// list_new(0)) -> 0.
// =====================================================================

#[test]
fn len_e2e_03_empty_str_is_zero() {
    let src = "fn main() -> i64:\n    print(len(\"\"))\n    return 0\n";
    assert_build_run("len_e2e_03_str", src, &[], b"", "0\n");
}

#[test]
fn len_e2e_03_empty_list_is_zero() {
    // An empty `list[i64]` literal; `len(xs)` -> 0. (A bare
    // `list_new(0)` would leave the elem type an unanchored inference
    // var — the pre-existing list-poly AmbiguousType, unrelated to
    // ADR-0088 — so we use an annotated empty literal.)
    let src = "fn main() -> i64:\n    let xs: list[i64] = []\n    print(len(xs))\n    return 0\n";
    assert_build_run("len_e2e_03_list", src, &[], b"", "0\n");
}

// =====================================================================
// len_e2e_04 — `len(d)` on `dict[i64,i64]` type-checks + links + runs.
//
// The dict-len RUNTIME count is pre-existing `#[ignore]`'d debt
// (dict_e2e.rs::f3d08_dict_len_returns_count — the dict-literal /
// __cobrust_dict_len runtime returns 0 today, unrelated to ADR-0088's
// type-checker fix). This test therefore asserts only that the
// `len(dict)` path keeps type-checking + building + running cleanly (the
// ADR-0088 regression guarantee), NOT the printed count.
// =====================================================================

#[test]
fn len_e2e_04_dict_typechecks_and_runs() {
    let src = "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10, 2: 20, 3: 30}\n    let n: i64 = len(d)\n    print(n)\n    return 0\n";
    assert_build_runs_clean("len_e2e_04", src);
}

// =====================================================================
// len_e2e_05 — `len(5)` (non-sized) REJECTED at type-check, exit 2.
// The §2.5-B fix-naming check: stderr NAMES `len` and does NOT carry the
// misleading "expected Dict" the pre-ADR-0088 diagnostic emitted.
// =====================================================================

#[test]
fn len_e2e_05_non_sized_int_rejected() {
    let src = "fn main() -> i64:\n    print(len(5))\n    return 0\n";
    let path = write_cb("len_e2e_05", src);
    let (build_code, _exe, stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 2,
        "len_e2e_05: expected TYPE_ERROR (exit 2) for len(5); stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("LenArgNotSized") || stderr.to_lowercase().contains("sized"),
        "len_e2e_05: stderr should name the sized-type rejection; got:\n{stderr}"
    );
    assert!(
        !stderr.contains("expected Dict"),
        "len_e2e_05: §2.5-B — stderr must NOT carry the misleading \
         'expected Dict' diagnostic; got:\n{stderr}"
    );
}

// =====================================================================
// len_e2e_06 — runtime str via `input` then `len(s)` -> the byte count.
// Exercises the heap-buffer Str path (not the literal path).
// =====================================================================

#[test]
fn len_e2e_06_runtime_str_len() {
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    print(len(s))\n    return 0\n";
    // Input "world\n" -> the trimmed line "world" has 5 bytes.
    assert_build_run("len_e2e_06", src, &[], b"world\n", "5\n");
}
