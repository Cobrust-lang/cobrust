// ADR-0108 / F95 end-to-end corpus for the `sorted(xs)` builtin — one of
// the most-used Python idioms (`sorted([3,1,2]) == [1,2,3]`), previously a
// clean `unknown name` reject (build-exit 2).
//
// Per ADR-0108: `sorted(xs: list[T]) -> list[T]` returns a NEW ascending-
// sorted list; the SOURCE is NOT mutated (Python copy semantics — distinct
// from the in-place `list.sort()`, a deferred follow-up, as are the
// `reverse=` / `key=` kwargs). int / float sort numerically; str sorts
// LEXICOGRAPHICally (UTF-8 byte order == codepoint order == CPython). The
// source list is passed by POINTER (`is_copy_type(Ty::List)` so the `.cb`
// scope keeps ownership + drops it once) and the runtime shim BORROWS it
// (reads len + each slot) WITHOUT freeing it; the FRESH sorted list is the
// dest local, dropped once by its `Ty::List(T)` drop schedule (a
// `list[str]` dest routes `__cobrust_list_drop_elems` + str_drop over the
// fresh OWNED clones — disjoint from the source's own slots).
//
// These tests REAL-compile -> link -> spawn a `.cb` program and assert the
// produced executable's stdout / exit code, differentially against
// python3.11 semantics. cobrust prints whole floats without the trailing
// `.0` (`print(3.0)` -> "3").
//
// THE ADR-0089/0090 ELEMENT-DISPATCH LESSON is locked by both a literal
// and a VARIABLE-bound fixture per element type: the int/float/str shim
// dispatch keys on the call's DEST list element type (the type-checker's
// `list[T]` record), so a `list[str]` routes to `__cobrust_list_sort_str`,
// not the int shim.
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

/// Assert the source REJECTS at build time (a clean non-zero exit) and the
/// stderr carries `needle` (the §2.5-B fix-suggestion the LLM consumes).
fn assert_build_rejects(name: &str, src: &str, needle: &str) {
    let path = write_cb(name, src);
    let (build_code, _exe, build_stderr) = run_build_exe(&path);
    assert_ne!(
        build_code, 0,
        "{name}: expected a build-time reject (non-zero exit), got 0\n--- source ---\n{src}"
    );
    assert!(
        build_stderr.contains(needle),
        "{name}: stderr missing `{needle}`; got:\n{build_stderr}"
    );
}

// =====================================================================
// sorted_e2e_01 — `sorted` on a `list[int]` VARIABLE; the SOURCE is
// UNMUTATED afterwards (Python copy semantics — proves the shim BORROWS
// + builds a fresh list, never sorts in place). `sorted([3,1,2])` ==
// [1,2,3]; the source still iterates `3 1 2`. ADR-0108 §3.
// =====================================================================

#[test]
fn sorted_e2e_01_int_var_and_source_unmutated() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = [3, 1, 2]\n    let ys: list[i64] = sorted(xs)\n    print(ys[0])\n    print(ys[1])\n    print(ys[2])\n    print(xs[0])\n    print(xs[1])\n    print(xs[2])\n    return 0\n";
    // sorted: 1 2 3 ; source unmutated: 3 1 2
    assert_build_run("sorted_e2e_01", src, "1\n2\n3\n3\n1\n2\n");
}

// =====================================================================
// sorted_e2e_02 — `sorted` on an int LITERAL passed inline + DUPLICATES.
// sorted([5,5,1,3]) == [1,3,5,5].
// =====================================================================

#[test]
fn sorted_e2e_02_int_literal_duplicates() {
    let src = "fn main() -> i64:\n    let ys: list[i64] = sorted([5, 5, 1, 3])\n    print(len(ys))\n    print(ys[0])\n    print(ys[1])\n    print(ys[2])\n    print(ys[3])\n    return 0\n";
    assert_build_run("sorted_e2e_02", src, "4\n1\n3\n5\n5\n");
}

// =====================================================================
// sorted_e2e_03 — negatives + a single-element list. sorted([-1,-9,4,0])
// == [-9,-1,0,4]; sorted([42]) == [42].
// =====================================================================

#[test]
fn sorted_e2e_03_int_negatives_and_singleton() {
    let src = "fn main() -> i64:\n    let ys: list[i64] = sorted([-1, -9, 4, 0])\n    print(ys[0])\n    print(ys[1])\n    print(ys[2])\n    print(ys[3])\n    let one: list[i64] = sorted([42])\n    print(one[0])\n    return 0\n";
    assert_build_run("sorted_e2e_03", src, "-9\n-1\n0\n4\n42\n");
}

// =====================================================================
// sorted_e2e_04 — `sorted([]) == []` (empty list yields a fresh empty
// list — len 0, never a trap). ADR-0108 §"Empty".
// =====================================================================

#[test]
fn sorted_e2e_04_empty_is_empty() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = []\n    let ys: list[i64] = sorted(xs)\n    print(len(ys))\n    return 0\n";
    assert_build_run("sorted_e2e_04", src, "0\n");
}

// =====================================================================
// sorted_e2e_05 — `sorted` on a `list[float]` (the element-dispatch
// lesson — routes to `__cobrust_list_sort_float`, f64-bit slots).
// sorted([3.5,1.5,2.0,1.5]) == [1.5,1.5,2.0,3.5] (cobrust prints whole
// floats without the trailing `.0`, so 2.0 -> "2").
// =====================================================================

#[test]
fn sorted_e2e_05_float_var() {
    let src = "fn main() -> i64:\n    let fs: list[f64] = [3.5, 1.5, 2.0, 1.5]\n    let ys: list[f64] = sorted(fs)\n    print(ys[0])\n    print(ys[1])\n    print(ys[2])\n    print(ys[3])\n    return 0\n";
    assert_build_run("sorted_e2e_05", src, "1.5\n1.5\n2\n3.5\n");
}

// =====================================================================
// sorted_e2e_06 — `sorted` on a `list[str]` (LEXICOGRAPHIC). The fresh
// list OWNS deep-copied clones; the SOURCE is UNMUTATED (still
// "banana"/"apple"/"cherry" in original order) and BOTH lists drop
// cleanly (no double-free, no leak — disjoint Str allocations).
// sorted(["banana","apple","cherry"]) == ["apple","banana","cherry"].
// =====================================================================

#[test]
fn sorted_e2e_06_str_lexicographic_and_source_unmutated() {
    let src = "fn main() -> i64:\n    let ws: list[str] = [\"banana\", \"apple\", \"cherry\"]\n    let sw: list[str] = sorted(ws)\n    print(sw[0])\n    print(sw[1])\n    print(sw[2])\n    print(ws[0])\n    print(ws[1])\n    print(ws[2])\n    return 0\n";
    // sorted: apple banana cherry ; source unmutated: banana apple cherry
    assert_build_run(
        "sorted_e2e_06",
        src,
        "apple\nbanana\ncherry\nbanana\napple\ncherry\n",
    );
}

// =====================================================================
// sorted_e2e_07 — REGRESSION: `min`/`max`/`sum`/`len` on the SAME list
// still work AND the list itself still drops once after a `sorted` of a
// SEPARATE list (no double-free / no leak of the source). Also a
// single-element str sort.
// =====================================================================

#[test]
fn sorted_e2e_07_regression_reducers_unchanged() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = [3, 1, 2]\n    let ys: list[i64] = sorted(xs)\n    print(min(xs))\n    print(max(xs))\n    print(sum(xs))\n    print(len(xs))\n    print(ys[0])\n    let one: list[str] = sorted([\"solo\"])\n    print(one[0])\n    return 0\n";
    assert_build_run("sorted_e2e_07", src, "1\n3\n6\n3\n1\nsolo\n");
}

// =====================================================================
// sorted_e2e_08 — NEGATIVE: a non-list (scalar) argument REJECTS cleanly
// at build time (§2.5-A compile-time-catch) with the §2.5-B fix
// suggestion in stderr. `sorted(5)` is not a list.
// =====================================================================

#[test]
fn sorted_e2e_08_non_list_arg_rejects() {
    let src = "fn main() -> i64:\n    let x: i64 = 5\n    let _ = sorted(x)\n    return 0\n";
    assert_build_rejects(
        "sorted_e2e_08",
        src,
        "`sorted` takes a single list argument",
    );
}
