//! ADR-0105 / F93 end-to-end corpus for the Python CONDITIONAL EXPRESSION
//! (ternary): `<then> if <cond> else <else>`.
//!
//! ## What F93 closes
//!
//! Before ADR-0105, the ternary was entirely UNIMPLEMENTED — `let y = 1 if
//! x < 0 else 2` FAILED at PARSE ("expected end of statement, found `if`").
//! It is the single most ubiquitous Python expression idiom an LLM writes;
//! its absence was a §2.5 (LLM-first / maximize-training-data-overlap)
//! deficit. This was an ADDITIVE gap (a clean parse-reject), NOT a silent
//! miscompile.
//!
//! ## The implemented surface (full pipeline)
//!
//! - PARSE: after a full Pratt expression in EXPRESSION position, a
//!   trailing `if` opens the ternary. It binds more LOOSELY than every
//!   operator (`a or b if c else d` ⇒ `(a or b) if c else d`); the `else`
//!   arm is RIGHT-associative (`a if p else b if q else c` ⇒
//!   `a if p else (b if q else c)`). The statement-level `if cond:` block
//!   is untouched (it is dispatched before any expression is parsed).
//! - TYPE (§2.2): `cond` MUST be `bool` (NO implicit truthiness — a
//!   non-bool cond REJECTS with the §2.5-B `ImplicitTruthiness` fix hint).
//!   The result type is `unify(then, else)`; a branch type mismatch
//!   REJECTS with the canonical `TypeMismatch` (no new error variant).
//! - MIR: lowered as value-producing control flow — `cond` SwitchInts to
//!   then/else blocks, each assigning a fresh result local then `Goto` a
//!   join block; the expression evaluates to that result local. Reuses the
//!   `if`-statement control-flow machinery.
//!
//! Each test writes a `.cb` program, invokes `cobrust build`, runs the
//! produced executable, and asserts stdout byte-identical to the CPython-3
//! oracle. The REJECT tests assert a clean non-zero (exit 2) compile reject
//! with the fix-printing diagnostic — never a crash, never a silent
//! miscompile.
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
        "{name}: build failed; stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    let (run_code, stdout, run_stderr) = run_exe(&exe);
    assert_eq!(run_code, 0, "{name}: run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, expected_stdout,
        "{name}: stdout mismatch (CPython-3 oracle)\nstderr={run_stderr}"
    );
}

/// Assert a `.cb` program is REJECTED at compile time with the clean
/// type-error exit code 2 (NOT a crash / abort), and the diagnostic on
/// stderr CONTAINS `needle` (the §2.5-B fix-printing substring). A clean
/// exit 2 proves a Cobrust DIAGNOSTIC, not a silent miscompile and not a
/// panic.
fn assert_build_rejects_exit2(name: &str, src: &str, needle: &str) {
    let path = write_cb(name, src);
    let (build_code, _exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 2,
        "{name}: build must REJECT with clean exit 2 (type error), got \
         {build_code}; stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    assert!(
        build_stderr.contains(needle),
        "{name}: reject diagnostic must contain {needle:?}; \
         got stderr=\n{build_stderr}"
    );
}

// =====================================================================
// ternary_e2e_01 — the canonical `<a> if <cond> else <b>` in a let-rhs.
// CPython 3 oracle: with x=5, `1 if x<0 else 2 == 2`; with x=-3 it is 1.
// =====================================================================

#[test]
fn ternary_e2e_01_basic_let_rhs() {
    let src = "\
fn main() -> i64:
    let x: i64 = 5
    let y: i64 = 1 if x < 0 else 2
    print(y)
    let w: i64 = -3
    let z: i64 = 1 if w < 0 else 2
    print(z)
    return 0
";
    assert_build_run("ternary_e2e_01", src, "2\n1\n");
}

// =====================================================================
// ternary_e2e_02 — ternary as a CALL ARGUMENT: `print(1 if c else 2)`.
// CPython 3: prints 2 (c False) then 1 (c True).
// =====================================================================

#[test]
fn ternary_e2e_02_in_call_arg() {
    let src = "\
fn main() -> i64:
    let c: bool = False
    print(1 if c else 2)
    let d: bool = True
    print(1 if d else 2)
    return 0
";
    assert_build_run("ternary_e2e_02", src, "2\n1\n");
}

// =====================================================================
// ternary_e2e_03 — ternary as a RETURN value: `return a if c else b`.
// CPython 3: classify(5)==2, classify(-3)==1.
// =====================================================================

#[test]
fn ternary_e2e_03_in_return() {
    let src = "\
fn classify(x: i64) -> i64:
    let a: i64 = 1
    let b: i64 = 2
    return a if x < 0 else b

fn main() -> i64:
    print(classify(5))
    print(classify(-3))
    return 0
";
    assert_build_run("ternary_e2e_03", src, "2\n1\n");
}

// =====================================================================
// ternary_e2e_04 — NESTED / RIGHT-ASSOCIATIVE chain
// `a if p else b if q else c` parses as `a if p else (b if q else c)`.
// CPython 3: with p=False,q=False ⇒ 30; p=False,q=True ⇒ 20;
// p=True ⇒ 10.
// =====================================================================

#[test]
fn ternary_e2e_04_nested_right_assoc() {
    let src = "\
fn pick(p: bool, q: bool) -> i64:
    return 10 if p else 20 if q else 30

fn main() -> i64:
    print(pick(False, False))
    print(pick(False, True))
    print(pick(True, False))
    return 0
";
    assert_build_run("ternary_e2e_04", src, "30\n20\n10\n");
}

// =====================================================================
// ternary_e2e_05 — STR ternary `"yes" if c else "no"` (a non-Copy result
// type — exercises the result local's drop dispatch). CPython 3:
// "yes" when c, else "no".
// =====================================================================

#[test]
fn ternary_e2e_05_str_branches() {
    let src = "\
fn main() -> i64:
    let c: bool = True
    let s: str = \"yes\" if c else \"no\"
    print(s)
    let d: bool = False
    let t: str = \"yes\" if d else \"no\"
    print(t)
    return 0
";
    assert_build_run("ternary_e2e_05", src, "yes\nno\n");
}

// =====================================================================
// ternary_e2e_06 — FLOAT ternary `1.5 if c else 2.5`. CPython 3:
// 1.5 when c, else 2.5.
// =====================================================================

#[test]
fn ternary_e2e_06_float_branches() {
    let src = "\
fn main() -> i64:
    let c: bool = True
    let f: f64 = 1.5 if c else 2.5
    print(f)
    let d: bool = False
    let g: f64 = 1.5 if d else 2.5
    print(g)
    return 0
";
    assert_build_run("ternary_e2e_06", src, "1.5\n2.5\n");
}

// =====================================================================
// ternary_e2e_07 — the ternary binds LOOSER than every operator. The
// THEN arm of `a + 1 if c else a - 1` is the whole `a + 1` (NOT `1`).
// CPython 3: a=10, c=True ⇒ 11; c=False ⇒ 9.
// =====================================================================

#[test]
fn ternary_e2e_07_binds_looser_than_arith() {
    let src = "\
fn main() -> i64:
    let a: i64 = 10
    let c: bool = True
    print(a + 1 if c else a - 1)
    let d: bool = False
    print(a + 1 if d else a - 1)
    return 0
";
    assert_build_run("ternary_e2e_07", src, "11\n9\n");
}

// =====================================================================
// ternary_e2e_08 — REJECT: a NON-BOOL condition `1 if 5 else 2`. §2.2
// (no implicit truthiness): `5` is an `i64`, not a `bool`. Clean exit 2
// + the §2.5-B `ImplicitTruthiness` fix hint — NOT a crash, NOT a silent
// truthy-coercion.
// =====================================================================

#[test]
fn ternary_e2e_08_reject_non_bool_cond() {
    assert_build_rejects_exit2(
        "ternary_e2e_08",
        "\
fn main() -> i64:
    let y: i64 = 1 if 5 else 2
    print(y)
    return 0
",
        "boolean condition",
    );
}

// =====================================================================
// ternary_e2e_09 — REJECT: MISMATCHED branch types `1 if c else \"x\"`
// (int vs str). The result type is `unify(then, else)`; the two arms
// must share a type. Clean exit 2 + the canonical `TypeMismatch` (no new
// error variant) — NOT a crash.
// =====================================================================

#[test]
fn ternary_e2e_09_reject_branch_type_mismatch() {
    assert_build_rejects_exit2(
        "ternary_e2e_09",
        "\
fn main() -> i64:
    let c: bool = True
    let z = 1 if c else \"x\"
    print(z)
    return 0
",
        "type mismatch",
    );
}
