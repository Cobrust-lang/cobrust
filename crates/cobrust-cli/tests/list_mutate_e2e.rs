// F96 / ADR-0109 end-to-end corpus for the list MUTABLE methods
// `xs.append(v)` / `xs.pop()` — two of the most-used Python list ops
// (in almost every Python program; previously both REJECTED with
// "method `append`/`pop` not found on `list`").
//
// Semantics (CPython oracle):
//   - `xs.append(v)` mutates `xs` IN PLACE (grows by 1), returns None.
//   - `xs.pop()` removes + RETURNS the LAST element, mutating `xs`
//     (shrinks by 1); `[].pop()` raises IndexError → Cobrust TRAPS
//     (exit 3, §2.2 — NOT a silent sentinel, NOT a raw assert! abort).
//
// The list operand is Copy-at-call (`is_copy_type(Ty::List)`), so the
// receiver `xs` stays the SAME live handle — append/pop mutate through
// the ptr and the `.cb` local sees it; `xs` still drops exactly once at
// scope exit. F96 ships the Copy-scalar element types (`list[int]` /
// `list[float]`); owned-element lists (`list[str]` / `list[list]`) are
// CLEANLY REJECTED at type-check (exit 2) with a §2.5-B fix-printing
// message (the ownership-transfer follow-up — see ADR-0109 + finding F96).
//
// The int/float dispatch keys: append on the VALUE arg's element type,
// pop on the call's DEST (element) type (the type-checker record), the
// ADR-0089 abs-miscompile-proof source of truth — a `list[float]`
// append/pop routes to the `_float` runtime shim (the i64-slot <-> f64
// bit-pattern encode/decode), not the int shim.
//
// These tests REAL-compile -> link -> spawn a `.cb` program and assert
// stdout / exit code, differentially against python3.11 semantics.
// cobrust prints whole floats without the trailing `.0` (`print(3.0)` ->
// "3"), a pre-existing float-format difference unrelated to this ADR.
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

/// Build succeeds, but the produced exe TRAPS at runtime (exit 3) — the
/// empty-`pop` policy (CPython `IndexError` parity, §2.2 → clean exit 3).
fn assert_build_run_traps(name: &str, src: &str) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "{name}: build unexpectedly failed; stderr=\n{build_stderr}"
    );
    let (run_code, _stdout, run_stderr) = run_exe(&exe, &[], b"");
    assert_eq!(
        run_code, 3,
        "{name}: expected the empty-pop TRAP (exit 3), got {run_code}; stderr={run_stderr}"
    );
}

/// Build is EXPECTED to fail at type-check (exit 2). Assert the build
/// exit code AND that stderr names the F96 §2.5-B reject substring.
fn assert_build_rejects(name: &str, src: &str, expect_substr: &str) {
    let path = write_cb(name, src);
    let (build_code, _exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 2,
        "{name}: expected a type-check reject (exit 2), got {build_code}; stderr=\n{build_stderr}"
    );
    assert!(
        build_stderr.contains(expect_substr),
        "{name}: reject stderr missing `{expect_substr}`; got=\n{build_stderr}"
    );
}

// =====================================================================
// lm_e2e_01 — `append` grows a `list[int]` IN PLACE; index + len see it.
// xs=[1,2,3]; xs.append(4) -> len 4, xs[3]==4.
// =====================================================================

#[test]
fn lm_e2e_01_int_append_grows_in_place() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = [1, 2, 3]\n    xs.append(4)\n    print(len(xs))\n    print(xs[3])\n    return 0\n";
    assert_build_run("lm_e2e_01", src, "4\n4\n");
}

// =====================================================================
// lm_e2e_02 — `pop` removes + returns the LAST element, shrinking by 1.
// xs=[1,2,3]; xs.pop()==3, len 2, xs[0..1]==[1,2].
// =====================================================================

#[test]
fn lm_e2e_02_int_pop_returns_last() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = [1, 2, 3]\n    let last: i64 = xs.pop()\n    print(last)\n    print(len(xs))\n    print(xs[0])\n    print(xs[1])\n    return 0\n";
    assert_build_run("lm_e2e_02", src, "3\n2\n1\n2\n");
}

// =====================================================================
// lm_e2e_03 — `append` IN A LOOP builds [0..n) (the canonical Python
// accumulate idiom); sum confirms the contents. n=5 -> [0,1,2,3,4],
// len 5, sum 10.
// =====================================================================

#[test]
fn lm_e2e_03_append_in_loop_builds_range() {
    let src = "fn main() -> i64:\n    let ys: list[i64] = []\n    let i: i64 = 0\n    while i < 5:\n        ys.append(i)\n        i = i + 1\n    print(len(ys))\n    print(sum(ys))\n    print(ys[0])\n    print(ys[4])\n    return 0\n";
    assert_build_run("lm_e2e_03", src, "5\n10\n0\n4\n");
}

// =====================================================================
// lm_e2e_04 — pop down to EMPTY, then len 0 (each pop returns its value).
// xs=[7,8]; pop->8, pop->7, len 0.
// =====================================================================

#[test]
fn lm_e2e_04_pop_down_to_empty() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = [7, 8]\n    print(xs.pop())\n    print(xs.pop())\n    print(len(xs))\n    return 0\n";
    assert_build_run("lm_e2e_04", src, "8\n7\n0\n");
}

// =====================================================================
// lm_e2e_05 — pop on an EMPTY list TRAPS (exit 3, §2.2 — CPython
// IndexError parity; NOT a silent sentinel `0`, NOT a SIGABRT).
// =====================================================================

#[test]
fn lm_e2e_05_pop_empty_traps() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = []\n    let v: i64 = xs.pop()\n    print(v)\n    return 0\n";
    assert_build_run_traps("lm_e2e_05", src);
}

// =====================================================================
// lm_e2e_06 — pop on a list emptied by append+pop ALSO traps (the
// length truly tracks the mutation; pop after the last element is gone).
// =====================================================================

#[test]
fn lm_e2e_06_pop_after_emptying_traps() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = []\n    xs.append(1)\n    let _a: i64 = xs.pop()\n    let _b: i64 = xs.pop()\n    return 0\n";
    assert_build_run_traps("lm_e2e_06", src);
}

// =====================================================================
// lm_e2e_07 — `list[f64]` append/pop (the `_float` slot encode/decode).
// xs=[1.5,2.5]; append(3.5) -> len 3; pop()==3.5; sum([1.5,2.5])==4.
// =====================================================================

#[test]
fn lm_e2e_07_float_append_pop() {
    let src = "fn main() -> i64:\n    let xs: list[f64] = [1.5, 2.5]\n    xs.append(3.5)\n    print(len(xs))\n    let v: f64 = xs.pop()\n    print(v)\n    print(sum(xs))\n    return 0\n";
    assert_build_run("lm_e2e_07", src, "3\n3.5\n4\n");
}

// =====================================================================
// lm_e2e_08 — the popped value is usable in further arithmetic (§2.5
// first-try win: pop returns the element type, so `xs.pop() + 1` stays
// integer). xs=[10,20]; pop()==20; 20+5==25; remaining sum==10.
// =====================================================================

#[test]
fn lm_e2e_08_pop_result_in_arithmetic() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = [10, 20]\n    let p: i64 = xs.pop() + 5\n    print(p)\n    print(sum(xs))\n    return 0\n";
    assert_build_run("lm_e2e_08", src, "25\n10\n");
}

// =====================================================================
// lm_e2e_09 — MUTATION PERSISTS / list still usable after the call: a
// long append+pop interleave leaves a coherent list (drop-once at exit).
// =====================================================================

#[test]
fn lm_e2e_09_interleaved_append_pop_coherent() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = [1]\n    xs.append(2)\n    xs.append(3)\n    let _x: i64 = xs.pop()\n    xs.append(9)\n    print(len(xs))\n    print(xs[0])\n    print(xs[1])\n    print(xs[2])\n    print(sum(xs))\n    return 0\n";
    assert_build_run("lm_e2e_09", src, "3\n1\n2\n9\n12\n");
}

// =====================================================================
// lm_e2e_10 — owned-element `list[str]` append is CLEANLY REJECTED at
// type-check (exit 2) with the §2.5-B fix-printing message (the
// ownership-transfer follow-up). NOT a miscompile.
// =====================================================================

#[test]
fn lm_e2e_10_str_append_rejected() {
    let src = "fn main() -> i64:\n    let xs: list[str] = [\"a\", \"b\"]\n    xs.append(\"c\")\n    return 0\n";
    assert_build_rejects(
        "lm_e2e_10",
        src,
        "on an owned-element list (element type `Str`) is not supported yet",
    );
}

// =====================================================================
// lm_e2e_11 — owned-element `list[str]` pop is likewise CLEANLY REJECTED.
// =====================================================================

#[test]
fn lm_e2e_11_str_pop_rejected() {
    let src = "fn main() -> i64:\n    let xs: list[str] = [\"a\", \"b\"]\n    let v: str = xs.pop()\n    print(v)\n    return 0\n";
    assert_build_rejects(
        "lm_e2e_11",
        src,
        "on an owned-element list (element type `Str`) is not supported yet",
    );
}

// =====================================================================
// lm_e2e_12 — §2.2 type safety: appending a WRONG-typed element to a
// `list[int]` is a type error (no silent coercion).
// =====================================================================

#[test]
fn lm_e2e_12_wrong_type_append_rejected() {
    let src =
        "fn main() -> i64:\n    let xs: list[i64] = [1, 2]\n    xs.append(3.5)\n    return 0\n";
    let path = write_cb("lm_e2e_12", src);
    let (build_code, _exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 2,
        "lm_e2e_12: expected a type-check reject (exit 2); stderr=\n{build_stderr}"
    );
    assert!(
        build_stderr.contains("type mismatch"),
        "lm_e2e_12: expected a `type mismatch` reject; got=\n{build_stderr}"
    );
}
