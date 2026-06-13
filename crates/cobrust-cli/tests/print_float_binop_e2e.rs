//! ADR-0089 §6 (F87) end-to-end corpus for `print(<inline float binary-op>)`
//! shim dispatch.
//!
//! ## Why this exists (§2.2 compiler-crash + §5.1 never-crash-on-valid-input)
//!
//! Before F87, `print(7.0 / 2.0)` (any INLINE float binary-op arg) CRASHED
//! the `cobrust build` compiler: build exited 3 with an LLVM module-verify
//! error ("Call parameter type does not match function signature") because a
//! FLOAT binop value was handed to `__cobrust_println_int(i64)`.
//!
//! The print-dispatch monomorphizer (cobrust-cli `rewrite_print`) reads the
//! arg local's resolved `Ty` to pick `__cobrust_println_int` vs `_float`. The
//! INLINE binop lowered into a `_bin` temp that was declared with `Ty::None`
//! (cobrust-mir `lower_bin`), which the print rewrite maps to `Ty::Int` →
//! `__cobrust_println_int(i64)` fed the f64 binop value → LLVM verify fail.
//! A DECLARED-f64 var (`let x: f64 = 7.0 / 2.0; print(x)`) already dispatched
//! correctly because the var local carried `Ty::Float`.
//!
//! F87 types the `_bin` temp with the RESOLVED scalar result type
//! (`synth_bin_result_ty`, shared with `synth_expr_ty`'s `Bin` arm — one
//! source of truth), so a Float binop → `Ty::Float` → `__cobrust_println_float`.
//! Mirrors the ADR-0089 `abs`/unary `_un` temp-type fix exactly. The fix is
//! conservative: only scalar Int/Float operand pairs resolve; non-scalar
//! (Buffer/Str/Dict) shapes stay `Ty::None` (unchanged).
//!
//! ## Float-print convention (the oracle, NOT CPython's `9.0`)
//!
//! `__cobrust_println_float` prints via Rust `{v}`, so integer-valued floats
//! print WITHOUT a trailing `.0` (`9.0` -> `9`, `5.0` -> `5`) — the SAME repr
//! `math_e2e` pins. Non-integer floats print in full (`3.5` -> `3.5`).
//!
//! Each test writes a `.cb` program, invokes `cobrust build`, runs the
//! produced executable, and asserts stdout byte-identical to the documented
//! cobrust-float-repr oracle.
//!
//! Per `feedback_p9_clippy_stall_pattern.md`: module-level test-only lint
//! allow header.

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
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::assertions_on_constants)]

use std::path::{Path, PathBuf};
use std::process::Command;

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

fn run_exe(exe: &Path) -> (i32, String, String) {
    let out = Command::new(exe).output().expect("spawn produced exe");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_build_run(name: &str, src: &str, expected_stdout: &str) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "{name}: build failed (F87: the compiler MUST NOT crash on a valid \
         type-checked `print(<float binop>)`); stderr=\n{build_stderr}\n\
         --- source ---\n{src}"
    );
    let (run_code, stdout, run_stderr) = run_exe(&exe);
    assert_eq!(run_code, 0, "{name}: run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, expected_stdout,
        "{name}: stdout mismatch (cobrust-float-repr oracle)\nstderr={run_stderr}"
    );
}

// =====================================================================
// print_float_binop_e2e_01 — the EXACT F87 repro. `print(7.0 / 2.0)` and
// `print(7.0 + 2.0)` previously CRASHED build (LLVM verify: f64 fed to
// `__cobrust_println_int`). Now they dispatch to `__cobrust_println_float`.
//   7.0 / 2.0 == 3.5  (non-integer -> full repr `3.5`)
//   7.0 + 2.0 == 9.0  (integer-valued -> repr `9`, NOT CPython `9.0`)
// =====================================================================

#[test]
fn print_float_binop_e2e_01_div_and_add() {
    let src = "\
fn main() -> i64:
    print(7.0 / 2.0)
    print(7.0 + 2.0)
    return 0
";
    assert_build_run("print_float_binop_e2e_01", src, "3.5\n9\n");
}

// =====================================================================
// print_float_binop_e2e_02 — every arithmetic op `+ - * / //` as an inline
// float binop arg dispatches to `__cobrust_println_float`.
//   7.0 - 2.0 == 5.0   -> `5`
//   2.0 * 3.0 == 6.0   -> `6`
//   7.0 // 2.0 == 3.0  -> `3`   (F86 float floor; integer-valued)
//   1.0 / 4.0 == 0.25  -> `0.25` (non-integer, full repr)
// =====================================================================

#[test]
fn print_float_binop_e2e_02_all_arith_ops() {
    let src = "\
fn main() -> i64:
    print(7.0 - 2.0)
    print(2.0 * 3.0)
    print(7.0 // 2.0)
    print(1.0 / 4.0)
    return 0
";
    assert_build_run("print_float_binop_e2e_02", src, "5\n6\n3\n0.25\n");
}

// =====================================================================
// print_float_binop_e2e_03 — a COMPUTED float binop (two f64 VARS, not
// literals) dispatches correctly: `let a: f64 = 1.5; let b: f64 = 2.5;
// print(a + b)` == 4.0 -> `4`. Confirms the synth-type resolves through
// declared-f64 var operands, not only float literals.
// =====================================================================

#[test]
fn print_float_binop_e2e_03_computed_var_operands() {
    let src = "\
fn main() -> i64:
    let a: f64 = 1.5
    let b: f64 = 2.5
    print(a + b)
    return 0
";
    assert_build_run("print_float_binop_e2e_03", src, "4\n");
}

// =====================================================================
// print_float_binop_e2e_04 — NESTED float binop `print((1.0 + 2.0) * 2.0)`
// == 6.0 -> `6`. The outer `*` temp must also resolve Float from its
// (Float inner-temp, Float literal) operands.
// =====================================================================

#[test]
fn print_float_binop_e2e_04_nested() {
    let src = "\
fn main() -> i64:
    print((1.0 + 2.0) * 2.0)
    return 0
";
    assert_build_run("print_float_binop_e2e_04", src, "6\n");
}

// =====================================================================
// print_float_binop_e2e_05 — a DECLARED-f64 var arg STILL works (the path
// that already dispatched correctly before F87 must not regress).
//   let x: f64 = 7.0 / 2.0; print(x) == 3.5
// =====================================================================

#[test]
fn print_float_binop_e2e_05_declared_var_unchanged() {
    let src = "\
fn main() -> i64:
    let x: f64 = 7.0 / 2.0
    print(x)
    return 0
";
    assert_build_run("print_float_binop_e2e_05", src, "3.5\n");
}

// =====================================================================
// print_float_binop_e2e_06 — REGRESSION: an INT inline binop still
// dispatches to `__cobrust_println_int` (UNCHANGED). `print(3 + 4)` == 7,
// `print(7 // 2)` == 3 (int floor div), `print(2 * 5)` == 10,
// `print(10 - 3)` == 7. These print as bare integers (no float repr).
// =====================================================================

#[test]
fn print_float_binop_e2e_06_int_binop_unchanged() {
    let src = "\
fn main() -> i64:
    print(3 + 4)
    print(7 // 2)
    print(2 * 5)
    print(10 - 3)
    return 0
";
    assert_build_run("print_float_binop_e2e_06", src, "7\n3\n10\n7\n");
}

// =====================================================================
// print_float_binop_e2e_07 — REGRESSION: int VAR print, bool print, and
// str print are all UNCHANGED by the F87 binop-temp-type fix.
//   let n: i64 = 5; print(n) == 5
//   print(2 < 3)   == True   (scalar comparison -> Bool -> println_bool;
//                             Python-style `True`/`False` repr)
//   print("hi")    == hi
// =====================================================================

#[test]
fn print_float_binop_e2e_07_int_bool_str_unchanged() {
    let src = "\
fn main() -> i64:
    let n: i64 = 5
    print(n)
    print(2 < 3)
    print(\"hi\")
    return 0
";
    assert_build_run("print_float_binop_e2e_07", src, "5\nTrue\nhi\n");
}
