//! F47 regression — f-string interpolation on user-function-returned `str`.
//!
//! Pre-fix repro (`docs/agent/findings/f47-fstring-user-fn-str-interp-empty.md`):
//! source `fn make_str() -> str: return "hello"` lowered to
//! `_return = Use(Constant::Str("hello"))` against a `Ty::None`-declared
//! return slot; codegen's special-case materialise branch
//! (`cranelift_backend::lower_statement` and `llvm_backend::lower_statement`)
//! required the destination's declared type to be `Ty::Str` and so fell
//! through to `lower_constant(Constant::Str(_))` which returns the M9 stub
//! `iconst(I64, 0)`. The caller's `let s: str = make_str()` therefore bound
//! a null pointer; downstream `__cobrust_str_ptr(s)` / `__cobrust_str_len(s)`
//! both returned zero, and `f"got {s}!"` printed `"got !"` instead of
//! `"got hello!"`.
//!
//! Fix landed 2026-05-25: extend the materialise predicate to also fire when
//! `place.local == body.return_local`, mirrored across both backends.
//!
//! This corpus exercises:
//! - the minimal one-call repro (literal return)
//! - composition with surrounding literal slots
//! - multiple user-fn-returned str interpolations in one f-string
//! - the literal-bound baseline (control — was already correct pre-fix)
//! - branch-dependent returns (mirrors the 99-bottles `line_count` shape)
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09: 18-lint clippy
//! module-level allow header at the top.

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
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn produced exe");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        let _ = stdin.write_all(&[]);
    }
    let out = child.wait_with_output().expect("wait_with_output");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_build_run(name: &str, src: &str, expected_stdout: &str) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "{name}: build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe);
    assert_eq!(run_code, 0, "{name}: run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, expected_stdout,
        "{name}: stdout mismatch\nstderr={run_stderr}"
    );
}

// =====================================================================
// F47 regression suite
// =====================================================================

#[test]
fn fstring_user_fn_str_simple() {
    // Minimal F47 repro shape — single hole, user-fn returns literal `str`.
    //
    // Pre-fix actual : "got !\n"
    // Post-fix expected: "got hello!\n"
    assert_build_run(
        "fstring_user_fn_str_simple",
        "fn make_str() -> str:\n    return \"hello\"\n\nfn main() -> i64:\n    let s: str = make_str()\n    print(f\"got {s}!\")\n    return 0\n",
        "got hello!\n",
    );
}

#[test]
fn fstring_user_fn_str_concat() {
    // Literal slots on both sides of a user-fn `str` interpolation.
    assert_build_run(
        "fstring_user_fn_str_concat",
        "fn pick() -> str:\n    return \"world\"\n\nfn main() -> i64:\n    let s: str = pick()\n    print(f\"prefix {s} suffix\")\n    return 0\n",
        "prefix world suffix\n",
    );
}

#[test]
fn fstring_user_fn_str_multi() {
    // Two user-fn-returned `str` slots in one f-string.
    assert_build_run(
        "fstring_user_fn_str_multi",
        "fn first() -> str:\n    return \"alpha\"\n\nfn second() -> str:\n    return \"beta\"\n\nfn main() -> i64:\n    let a: str = first()\n    let b: str = second()\n    print(f\"{a} and {b}\")\n    return 0\n",
        "alpha and beta\n",
    );
}

#[test]
fn fstring_literal_baseline() {
    // Control — `let s: str = "literal"; f"{s}"` was already correct pre-fix
    // because `let s: str = ...` triggers the original line-1272 materialise
    // branch (dest_ty == Ty::Str). Guards against future regression.
    assert_build_run(
        "fstring_literal_baseline",
        "fn main() -> i64:\n    let s: str = \"baseline\"\n    print(f\"{s}\")\n    return 0\n",
        "baseline\n",
    );
}

#[test]
fn fstring_user_fn_str_branch_returns() {
    // Mirrors the 99-bottles `line_count` shape: multiple early-return
    // branches each returning a Str literal. F47 fix must apply to every
    // `_return = Use(Constant::Str(_))` site regardless of branch.
    assert_build_run(
        "fstring_user_fn_str_branch_returns",
        "fn count_word(n: i64) -> str:\n    if n == 0:\n        return \"none\"\n    if n == 1:\n        return \"one\"\n    return \"many\"\n\nfn main() -> i64:\n    print(f\"{count_word(0)} / {count_word(1)} / {count_word(5)}\")\n    return 0\n",
        "none / one / many\n",
    );
}

#[test]
fn fstring_user_fn_str_with_int_mix() {
    // User-fn `str` slot interleaved with an int slot — verifies the
    // dispatch table (str vs int branch) still picks the right path
    // when the body type is correctly recovered.
    assert_build_run(
        "fstring_user_fn_str_with_int_mix",
        "fn label() -> str:\n    return \"count\"\n\nfn main() -> i64:\n    let l: str = label()\n    let n: i64 = 42\n    print(f\"{l}={n}\")\n    return 0\n",
        "count=42\n",
    );
}
