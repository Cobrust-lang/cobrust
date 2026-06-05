// ADR-0090 end-to-end corpus for the list-reducer builtins `min(xs)` /
// `max(xs)` / `sum(xs)` — three of the most-used Python builtins (all
// previously `unknown name`), and the first builtins that CONSUME
// (borrow-read) a `list[T]` argument.
//
// Per ADR-0090: `min`/`max` return the smallest/largest ELEMENT and
// `sum` the sum — all of the element type (`min(list[int]) -> int`,
// `min(list[float]) -> float`). The list is passed by POINTER
// (`is_copy_type(Ty::List)` so the `.cb` scope keeps ownership) and the
// runtime shim BORROWS it (reads len + each slot) WITHOUT freeing it.
// `min([])`/`max([])` TRAP (CPython `ValueError` parity, §2.2 no
// exceptions → clean non-zero exit); `sum([]) == 0`.
//
// These tests REAL-compile -> link -> spawn a `.cb` program and assert
// the produced executable's stdout / exit code, differentially against
// python3.11 semantics. cobrust prints whole floats without the trailing
// `.0` (`print(3.0)` -> "3"), so `sum([1.5, 2.5]) == 4.0` prints "4"
// (the same value Python prints as "4.0") — a pre-existing float-format
// difference, unrelated to this ADR.
//
// THE ADR-0089 ABS-MISCOMPILE LESSON is locked by the COMPUTED-arg
// fixtures (a list BUILT via a helper / a variable, then passed — NOT
// just a literal): the int/float dispatch keys on the call's DEST type
// (the type-checker's element-type record), so a computed float list
// routes to the float shim, not the int shim.
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

/// Build succeeds, but the produced exe TRAPS at runtime (non-zero
/// exit) — the empty-list `min`/`max` policy (CPython `ValueError`
/// parity, §2.2 → clean non-zero exit).
fn assert_build_run_traps(name: &str, src: &str) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "{name}: build unexpectedly failed; stderr=\n{build_stderr}"
    );
    let (run_code, _stdout, run_stderr) = run_exe(&exe, &[], b"");
    assert_ne!(
        run_code, 0,
        "{name}: expected a runtime trap (non-zero exit), got 0; stderr={run_stderr}"
    );
}

// =====================================================================
// lr_e2e_01 — `min`/`max`/`sum` on a `list[int]` VARIABLE.
// min([3,1,2])=1, max=3, sum=6. ADR-0090 §3.
// =====================================================================

#[test]
fn lr_e2e_01_int_var_min_max_sum() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = [3, 1, 2]\n    print(min(xs))\n    print(max(xs))\n    print(sum(xs))\n    return 0\n";
    assert_build_run("lr_e2e_01", src, "1\n3\n6\n");
}

// =====================================================================
// lr_e2e_02 — `min`/`max`/`sum` on an int-list LITERAL (passed inline,
// not bound first). min([5,9,2])=2, max=9, sum=16.
// =====================================================================

#[test]
fn lr_e2e_02_int_literal_min_max_sum() {
    let src = "fn main() -> i64:\n    print(min([5, 9, 2]))\n    print(max([5, 9, 2]))\n    print(sum([5, 9, 2]))\n    return 0\n";
    assert_build_run("lr_e2e_02", src, "2\n9\n16\n");
}

// =====================================================================
// lr_e2e_03 — negative ints + the int result is usable in int
// arithmetic (the §2.5 first-try win — `sum` returns an int, so
// `sum(xs) + 1` stays integer). sum([-5,-1,-9])=-15, +1 = -14;
// min=-9; max=-1.
// =====================================================================

#[test]
fn lr_e2e_03_int_negatives_and_arithmetic() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = [-5, -1, -9]\n    print(min(xs))\n    print(max(xs))\n    let s: i64 = sum(xs) + 1\n    print(s)\n    return 0\n";
    assert_build_run("lr_e2e_03", src, "-9\n-1\n-14\n");
}

// =====================================================================
// lr_e2e_04 — singleton `min([5])` / `max([5])` / `sum([5])` all == 5.
// =====================================================================

#[test]
fn lr_e2e_04_int_singleton() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = [5]\n    print(min(xs))\n    print(max(xs))\n    print(sum(xs))\n    return 0\n";
    assert_build_run("lr_e2e_04", src, "5\n5\n5\n");
}

// =====================================================================
// lr_e2e_05 — `sum([]) == 0` (CPython parity — NOT a trap). The empty
// int-list annotated `let xs: list[i64] = []`. ADR-0090 §"Empty list".
// =====================================================================

#[test]
fn lr_e2e_05_sum_empty_is_zero() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = []\n    print(sum(xs))\n    return 0\n";
    assert_build_run("lr_e2e_05", src, "0\n");
}

// =====================================================================
// lr_e2e_06 — `min`/`max`/`sum` on a `list[float]` VARIABLE. Each i64
// slot is bitcast to f64 (`__cobrust_{min,max,sum}_float`).
// min([1.5,2.5,3.0])=1.5, max=3.0 (prints "3"), sum=7.0 (prints "7").
// ADR-0090 §3 (the float element-type dispatch).
// =====================================================================

#[test]
fn lr_e2e_06_float_var_min_max_sum() {
    let src = "fn main() -> i64:\n    let fs: list[f64] = [1.5, 2.5, 3.0]\n    print(min(fs))\n    print(max(fs))\n    print(sum(fs))\n    return 0\n";
    assert_build_run("lr_e2e_06", src, "1.5\n3\n7\n");
}

// =====================================================================
// lr_e2e_07 — `sum` on a float LITERAL with a non-whole result:
// sum([1.5, 2.5, 0.25]) == 4.25 (the trailing fraction is preserved).
// =====================================================================

#[test]
fn lr_e2e_07_float_literal_fractional_sum() {
    let src = "fn main() -> i64:\n    print(sum([1.5, 2.5, 0.25]))\n    print(min([1.5, 2.5, 0.25]))\n    return 0\n";
    assert_build_run("lr_e2e_07", src, "4.25\n0.25\n");
}

// =====================================================================
// lr_e2e_08 — THE ADR-0089 COMPUTED-ARG LESSON: a `list[float]` BUILT
// by a helper fn, then passed to `sum`/`min`/`max`. The int/float
// dispatch keys on the call's DEST type (the type-checker's element
// record), NOT the arg operand's MIR temp — so the computed float list
// routes to the FLOAT shim (a pre-fix arg-temp-type dispatch would
// misroute it to the int shim and reinterpret the f64 bits as i64 =
// silent miscompile). sum=8.0 (prints "8"), min=1.5, max=4.0 ("4").
// =====================================================================

#[test]
fn lr_e2e_08_computed_float_list_arg() {
    let src = concat!(
        "fn make_floats() -> list[f64]:\n",
        "    let ys: list[f64] = [1.5, 2.5, 4.0]\n",
        "    return ys\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(sum(make_floats()))\n",
        "    print(min(make_floats()))\n",
        "    print(max(make_floats()))\n",
        "    return 0\n",
    );
    assert_build_run("lr_e2e_08", src, "8\n1.5\n4\n");
}

// =====================================================================
// lr_e2e_09 — THE ADR-0089 COMPUTED-ARG LESSON, int side: a `list[i64]`
// BUILT element-by-element via `list_new` + `list_set` (NOT a literal),
// then reduced. Confirms the int dispatch survives a computed list.
// list = [7, 3, 11]: min=3, max=11, sum=21.
// =====================================================================

#[test]
fn lr_e2e_09_computed_int_list_arg() {
    let src = concat!(
        "fn main() -> i64:\n",
        "    let xs: list[i64] = list_new(3)\n",
        "    let _ = list_set(xs, 0, 7)\n",
        "    let _ = list_set(xs, 1, 3)\n",
        "    let _ = list_set(xs, 2, 11)\n",
        "    print(min(xs))\n",
        "    print(max(xs))\n",
        "    print(sum(xs))\n",
        "    return 0\n",
    );
    assert_build_run("lr_e2e_09", src, "3\n11\n21\n");
}

// =====================================================================
// lr_e2e_10 — THE BORROW LOCK: a list REUSED after `min`/`max`/`sum`.
// The shim BORROWS the list (read-only) and does NOT free it; the `.cb`
// scope drops it exactly once at exit. After all three reducers the
// list is still fully usable (`len`, index, re-`sum`) AND the program
// exits 0 (no double-free / use-after-free). list=[10,20,30,5]:
// min=5, max=30, sum=65, len=4, xs[0]=10, xs[3]=5, sum again=65.
// =====================================================================

#[test]
fn lr_e2e_10_list_reused_after_reduce_borrow() {
    let src = concat!(
        "fn main() -> i64:\n",
        "    let xs: list[i64] = [10, 20, 30, 5]\n",
        "    print(min(xs))\n",
        "    print(max(xs))\n",
        "    print(sum(xs))\n",
        "    print(len(xs))\n",
        "    print(xs[0])\n",
        "    print(xs[3])\n",
        "    print(sum(xs))\n",
        "    return 0\n",
    );
    assert_build_run("lr_e2e_10", src, "5\n30\n65\n4\n10\n5\n65\n");
}

// =====================================================================
// lr_e2e_11 — empty `min([])` TRAPS (non-zero exit). CPython raises
// `ValueError: min() arg is an empty sequence`; Cobrust has no
// exceptions (§2.2) → a clean non-zero-exit trap. ADR-0090 §"Empty
// list". Annotated empty list so the elem type anchors to int.
// =====================================================================

#[test]
fn lr_e2e_11_min_empty_traps() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = []\n    print(min(xs))\n    return 0\n";
    assert_build_run_traps("lr_e2e_11", src);
}

// =====================================================================
// lr_e2e_12 — empty `max([])` TRAPS (non-zero exit). Sibling of
// lr_e2e_11 for the max reducer.
// =====================================================================

#[test]
fn lr_e2e_12_max_empty_traps() {
    let src = "fn main() -> i64:\n    let xs: list[i64] = []\n    print(max(xs))\n    return 0\n";
    assert_build_run_traps("lr_e2e_12", src);
}

// =====================================================================
// lr_e2e_13 — `sum` over a `range(...)` result (composes with the
// ADR-0089 `range` builtin). sum(range(5)) == 0+1+2+3+4 == 10. The
// `range(5)` materialises a `list[i64]`; `sum` borrow-reads it.
// =====================================================================

#[test]
fn lr_e2e_13_sum_of_range() {
    let src = "fn main() -> i64:\n    print(sum(range(5)))\n    print(min(range(2, 7)))\n    print(max(range(2, 7)))\n    return 0\n";
    assert_build_run("lr_e2e_13", src, "10\n2\n6\n");
}

// =====================================================================
// lr_e2e_14 — mixed: int reducers AND float reducers in the same
// program (no cross-contamination of the int/float dispatch). The
// int-list sum (6) and the float-list sum (7.0 -> "7") are independent.
// =====================================================================

#[test]
fn lr_e2e_14_mixed_int_and_float_program() {
    let src = concat!(
        "fn main() -> i64:\n",
        "    let xs: list[i64] = [1, 2, 3]\n",
        "    let fs: list[f64] = [1.5, 2.5, 3.0]\n",
        "    print(sum(xs))\n",
        "    print(sum(fs))\n",
        "    print(min(xs))\n",
        "    print(min(fs))\n",
        "    return 0\n",
    );
    assert_build_run("lr_e2e_14", src, "6\n7\n1\n1.5\n");
}
