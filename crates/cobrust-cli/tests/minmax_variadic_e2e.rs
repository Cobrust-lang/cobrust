// ADR-0107 / F94 end-to-end corpus for the VARIADIC scalar form of the
// `min`/`max` builtins — `max(a, b)` / `min(a, b, c)` (>= 2 scalar args).
//
// Python supports BOTH the 1-arg iterable form (`max([3,1,5])`, already
// shipped by ADR-0090 / `list_reduce_e2e.rs`) AND the >=2-arg scalar form
// (`max(3, 5)`). The variadic-scalar form is ubiquitous in Python and was
// a CLEAN REJECT before this change ("wrong number of arguments"), so this
// is an ADDITIVE §2.5 LLM-first win (no prior miscompile to guard).
//
// Per ADR-0107:
//   * `max(3, 5) == 5`, `min(3, 5, 1) == 1` — all-int args, `Int` result.
//   * `max(1.5, 2.5) == 2.5` — all-float args, `Float` result.
//   * a MIXED `max(1, 2.0)` PROMOTES to `Float` (consistent with
//     Cobrust's int+float arithmetic promotion; the `Int` operand is cast
//     i64→f64 at MIR time, NOT a silent value coercion). cobrust prints a
//     whole float without the trailing `.0` (`print(2.0)` -> "2"), a
//     pre-existing float-format difference unrelated to this ADR.
//   * a SINGLE non-list arg (`max(5)`) is REJECTED at type-check (Python:
//     `max(5)` is a `TypeError` — int not iterable). Covered by the
//     `minmax_variadic_neg_e2e.rs` negative corpus.
//   * `sum` does NOT get the variadic form (Python's `sum`'s 2nd
//     positional arg is `start`, not another element).
//
// The lowering MATERIALISES a temp `list[T]` from the N scalar operands
// and reuses the proven ADR-0090 list-consume path
// (`__cobrust_{min,max}_{int,float}`). The scalars are `Int`/`Float`
// (Copy) — no element-drop concern; the temp list drops once.
//
// These tests REAL-compile -> link -> spawn a `.cb` program and assert the
// produced executable's stdout / exit code, differentially against
// python3.11 semantics.
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

/// Build is expected to FAIL with a clean Type diagnostic (exit 2), NOT
/// a codegen panic (exit 101). `needle` must appear in stderr.
fn assert_build_rejects(name: &str, src: &str, needle: &str) {
    let path = write_cb(name, src);
    let (build_code, _exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 2,
        "{name}: expected a clean Type reject (exit 2), got {build_code}; \
         stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    assert!(
        build_stderr.contains(needle),
        "{name}: reject diagnostic must contain `{needle}`; stderr=\n{build_stderr}"
    );
}

// =====================================================================
// mmv_e2e_01 — `max(3, 5) == 5`, `min(3, 5) == 5`->1 — the canonical
// 2-arg scalar form. CPython: max(3,5)=5, min(3,5)=3.
// =====================================================================

#[test]
fn mmv_e2e_01_two_int_args() {
    let src = "fn main() -> i64:\n    print(max(3, 5))\n    print(min(3, 5))\n    return 0\n";
    assert_build_run("mmv_e2e_01", src, "5\n3\n");
}

// =====================================================================
// mmv_e2e_02 — three-arg `min(3, 5, 1) == 1`, `max(3, 5, 1) == 5`.
// =====================================================================

#[test]
fn mmv_e2e_02_three_int_args() {
    let src = "fn main() -> i64:\n    print(min(3, 5, 1))\n    print(max(3, 5, 1))\n    return 0\n";
    assert_build_run("mmv_e2e_02", src, "1\n5\n");
}

// =====================================================================
// mmv_e2e_03 — four-arg `max(2, 8, 4, 1) == 8`, `min(2, 8, 4, 1) == 1`.
// =====================================================================

#[test]
fn mmv_e2e_03_four_int_args() {
    let src =
        "fn main() -> i64:\n    print(max(2, 8, 4, 1))\n    print(min(2, 8, 4, 1))\n    return 0\n";
    assert_build_run("mmv_e2e_03", src, "8\n1\n");
}

// =====================================================================
// mmv_e2e_04 — negative ints: `min(-3, -1, -2) == -3`,
// `max(-3, -1, -2) == -1`.
// =====================================================================

#[test]
fn mmv_e2e_04_negative_ints() {
    let src =
        "fn main() -> i64:\n    print(min(-3, -1, -2))\n    print(max(-3, -1, -2))\n    return 0\n";
    assert_build_run("mmv_e2e_04", src, "-3\n-1\n");
}

// =====================================================================
// mmv_e2e_05 — two-arg FLOAT form: `max(1.5, 2.5) == 2.5`,
// `min(1.5, 2.5) == 1.5`. All-float args → `Float` result.
// =====================================================================

#[test]
fn mmv_e2e_05_two_float_args() {
    let src =
        "fn main() -> i64:\n    print(max(1.5, 2.5))\n    print(min(1.5, 2.5))\n    return 0\n";
    assert_build_run("mmv_e2e_05", src, "2.5\n1.5\n");
}

// =====================================================================
// mmv_e2e_06 — three-arg float with a fractional result preserved:
// `min(2.25, 0.5, 1.75) == 0.5`, `max(...) == 2.25`.
// =====================================================================

#[test]
fn mmv_e2e_06_three_float_args() {
    let src = "fn main() -> i64:\n    print(min(2.25, 0.5, 1.75))\n    print(max(2.25, 0.5, 1.75))\n    return 0\n";
    assert_build_run("mmv_e2e_06", src, "0.5\n2.25\n");
}

// =====================================================================
// mmv_e2e_07 — COMPUTED args (vars, not literals): the §2.5 first-try
// win — `max(a, b)` is the ubiquitous Python idiom. a=7, b=4 →
// max=7, min=4; and the result is usable in int arithmetic.
// =====================================================================

#[test]
fn mmv_e2e_07_computed_int_vars() {
    let src = concat!(
        "fn main() -> i64:\n",
        "    let a: i64 = 7\n",
        "    let b: i64 = 4\n",
        "    print(max(a, b))\n",
        "    print(min(a, b))\n",
        "    let m: i64 = max(a, b) + 1\n",
        "    print(m)\n",
        "    return 0\n",
    );
    assert_build_run("mmv_e2e_07", src, "7\n4\n8\n");
}

// =====================================================================
// mmv_e2e_08 — MIXED int/float PROMOTION (ADR-0107 §"Mixed int/float").
// `max(1, 2.0)` promotes the call to `Float`; the `Int` operand `1` is
// cast i64→f64 at MIR time (NOT a silent value coercion — an explicit
// `CastKind::IntToFloat`). Result 2.0 prints "2" (whole-float format).
// `min(5, 2.5)` → 2.5; `max(1, 2.0, 3)` → 3.0 prints "3".
// =====================================================================

#[test]
fn mmv_e2e_08_mixed_int_float_promotes() {
    let src = concat!(
        "fn main() -> i64:\n",
        "    print(max(1, 2.0))\n",
        "    print(min(5, 2.5))\n",
        "    print(max(1, 2.0, 3))\n",
        "    return 0\n",
    );
    assert_build_run("mmv_e2e_08", src, "2\n2.5\n3\n");
}

// =====================================================================
// mmv_e2e_09 — REGRESSION: the 1-arg LIST form MUST stay green
// alongside the variadic form in the SAME program (no cross-
// contamination of the list-consume vs temp-list-build dispatch).
// max([3,1,5])=5 (list) and max(3, 1, 5)=5 (variadic) agree.
// =====================================================================

#[test]
fn mmv_e2e_09_list_and_variadic_coexist() {
    let src = concat!(
        "fn main() -> i64:\n",
        "    let xs: list[i64] = [3, 1, 5]\n",
        "    print(max(xs))\n",
        "    print(max(3, 1, 5))\n",
        "    print(min(xs))\n",
        "    print(min(3, 1, 5))\n",
        "    print(sum(xs))\n",
        "    return 0\n",
    );
    assert_build_run("mmv_e2e_09", src, "5\n5\n1\n1\n9\n");
}

// =====================================================================
// mmv_e2e_10 — NESTED variadic: `max(min(3, 5), 2) == max(3, 2) == 3`;
// the inner scalar result feeds the outer variadic call. Confirms the
// temp-list build composes with itself.
// =====================================================================

#[test]
fn mmv_e2e_10_nested_variadic() {
    let src = concat!(
        "fn main() -> i64:\n",
        "    print(max(min(3, 5), 2))\n",
        "    print(min(max(1, 4), 2))\n",
        "    return 0\n",
    );
    assert_build_run("mmv_e2e_10", src, "3\n2\n");
}

// =====================================================================
// mmv_e2e_11 — NEGATIVE: a SINGLE non-list arg `max(5)` is REJECTED at
// type-check (Python: `max(5)` is a `TypeError` — int not iterable). The
// 1-arg form is the LIST-consume form (`max([5])`); a bare `i64` falls
// through to the `NotIterable` diagnostic — exit 2 (clean), NOT a codegen
// panic. The variadic form requires >= 2 args.
// =====================================================================

#[test]
fn mmv_e2e_11_single_int_arg_rejects() {
    let src = "fn main() -> i64:\n    print(max(5))\n    return 0\n";
    // The `NotIterable` diagnostic renders as "cannot be used in a `for`
    // loop" (a single `i64` is not an iterable for the 1-arg list form).
    assert_build_rejects("mmv_e2e_11", src, "for");
}

// =====================================================================
// mmv_e2e_12 — NEGATIVE: a single non-list `min(7)` rejects too (sibling
// of mmv_e2e_11 for `min`).
// =====================================================================

#[test]
fn mmv_e2e_12_single_int_min_rejects() {
    let src = "fn main() -> i64:\n    print(min(7))\n    return 0\n";
    assert_build_rejects("mmv_e2e_12", src, "for");
}

// =====================================================================
// mmv_e2e_13 — NEGATIVE: a NON-NUMERIC variadic arg `max("a", "b")` is
// REJECTED with the canonical `TypeMismatch` (exit 2), NOT a panic. The
// variadic form requires numeric (Int/Float) scalars.
// =====================================================================

#[test]
fn mmv_e2e_13_str_variadic_rejects() {
    let src = "fn main() -> i64:\n    print(max(\"a\", \"b\"))\n    return 0\n";
    assert_build_rejects("mmv_e2e_13", src, "type mismatch");
}
