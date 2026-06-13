//! ADR-0097 / §2.5 end-to-end corpus for the `str * int` / `int * str`
//! REPETITION operator: `"ab" * 3 == "ababab"`.
//!
//! ## Why this exists (§2.5 LLM-first, Maximize-training-data-overlap)
//!
//! `"sep" * n` is a Python idiom an LLM writes constantly (padding,
//! dividers, fixed-width fills). This increment is purely ADDITIVE: BEFORE
//! it, `"ab" * 3` was a CLEAN type-mismatch REJECT (`error[Type]: type
//! mismatch: expected str, found i64`) — NOT a silent miscompile. The
//! §2.5 win is making the common idiom WORK on the first try.
//!
//! ## Semantics (CPython 3 `str.__mul__`, the oracle)
//!
//! - `"ab" * 3 == "ababab"`; `3 * "ab" == "ababab"` (SYMMETRIC — Python
//!   allows both operand orders).
//! - `"x" * 0 == ""`; `"x" * 1 == "x"` (a copy); `"x" * -2 == ""`
//!   (a non-positive count yields the empty str — NEVER a trap).
//! - CODEPOINT-faithful: `"é" * 2 == "éé"` (repetition concatenates whole
//!   strings, so a boundary NEVER splits a multi-byte UTF-8 codepoint).
//! - works with a COMPUTED count (`let n: i64 = 2 + 1; "ab" * n`) and the
//!   result is a usable `str` (`len("ab" * 3) == 6`).
//!
//! ## Lowering (the four-piece pipeline this corpus pins)
//!
//! - check.rs `synth_bin` Mul arm: `(Str, Int)` / `(Int, Str)` → `Ty::Str`
//!   (BEFORE `unify`, since `Str` never unifies with `Int`).
//! - lower.rs `lower_bin` Mul guard: NORMALIZE both orders to `(s, n)` →
//!   `__cobrust_str_repeat(s, n)`. The `str` receiver is BORROWED (Move→
//!   Copy upgrade — survives + drops once); the result is a FRESH owned
//!   `str` (Move-out, dropped once), the `str + str` concat discipline.
//! - cobrust-stdlib `__cobrust_str_repeat`: `str::repeat`, `n <= 0 → ""`.
//! - codegen extern decl `(ptr, i64) -> ptr`, the `__cobrust_str_slice`
//!   mirror.
//!
//! Each test writes a `.cb` program, invokes `cobrust build`, runs the
//! produced executable, and asserts stdout byte-identical to the CPython-3
//! oracle.
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

// =====================================================================
// str_mul_e2e_01 — `str * int` basic + the SYMMETRIC `int * str`.
// CPython 3 oracle:  "ab" * 3 == "ababab" ;  3 * "ab" == "ababab".
// Both operand orders normalize to `__cobrust_str_repeat("ab", 3)`.
// =====================================================================

#[test]
fn str_mul_e2e_01_basic_and_symmetric() {
    let src = "\
fn main() -> i64:
    print(\"ab\" * 3)
    print(3 * \"ab\")
    return 0
";
    assert_build_run("str_mul_e2e_01", src, "ababab\nababab\n");
}

// =====================================================================
// str_mul_e2e_02 — the boundary counts. CPython 3:
//   "x" * 0  == ""   (empty)
//   "x" * 1  == "x"  (a copy)
//   "x" * -2 == ""   (Python: a non-positive count → empty, NOT a trap)
// The `n <= 0 → ""` runtime branch — a `.cb` program must build + run +
// exit 0 (no panic) on a zero / negative count.
// =====================================================================

#[test]
fn str_mul_e2e_02_zero_one_negative() {
    let src = "\
fn main() -> i64:
    print(\"x\" * 0)
    print(\"x\" * 1)
    print(\"x\" * -2)
    return 0
";
    // "" (blank), "x", "" (blank) — three newlines, two blank lines.
    assert_build_run("str_mul_e2e_02", src, "\nx\n\n");
}

// =====================================================================
// str_mul_e2e_03 — CODEPOINT faithfulness. CPython 3: "é" * 2 == "éé".
// `é` is U+00E9, a 2-byte UTF-8 codepoint; repetition concatenates whole
// strings so the result is `éé` (4 bytes, 2 codepoints) — a boundary
// never splits the multi-byte codepoint.
// =====================================================================

#[test]
fn str_mul_e2e_03_unicode_codepoint_faithful() {
    let src = "\
fn main() -> i64:
    print(\"é\" * 2)
    print(len(\"é\" * 2))
    return 0
";
    // "éé" then its byte-length. `len` on a `str` is the BYTE length in
    // Cobrust (the runtime `__cobrust_str_len`); `"éé"` is 4 bytes.
    assert_build_run("str_mul_e2e_03", src, "éé\n4\n");
}

// =====================================================================
// str_mul_e2e_04 — a COMPUTED count (not a literal). CPython 3:
//   n = 2 + 1; "ab" * n == "ababab".
// Exercises the `int` operand being an arbitrary `i64` local, not a
// constant — the lowering reads the second operand via `lower_expr`.
// =====================================================================

#[test]
fn str_mul_e2e_04_computed_count() {
    let src = "\
fn main() -> i64:
    let n: i64 = 2 + 1
    print(\"ab\" * n)
    return 0
";
    assert_build_run("str_mul_e2e_04", src, "ababab\n");
}

// =====================================================================
// str_mul_e2e_05 — the result is a USABLE `str`. CPython 3:
//   len("ab" * 3) == 6.
// The fresh repeat buffer flows into `len(...)` (a borrow-not-move
// consumer) — proves the result is a real `str` value, not a one-shot
// print sink.
// =====================================================================

#[test]
fn str_mul_e2e_05_result_is_usable() {
    let src = "\
fn main() -> i64:
    print(len(\"ab\" * 3))
    return 0
";
    assert_build_run("str_mul_e2e_05", src, "6\n");
}

// =====================================================================
// str_mul_e2e_06 — DROP balance. A `str * int` result bound to a local,
// printed, then dropped at scope exit. A fresh repeat buffer is dropped
// EXACTLY ONCE (no leak / double-free); the source `"ab"` literal also
// drops once (the borrow-not-move receiver discipline — `__cobrust_str_
// repeat` reads but does not consume `s`). A clean exit 0 (no allocator
// corruption / hang at scope exit) is the drop-balance signal. NOTE: a
// LOOP minting `"ab" * 3` per iteration is the F82 loop-leak debt, NOT
// this increment's concern — this fn mints exactly once.
// =====================================================================

#[test]
fn str_mul_e2e_06_drop_balance_single_mint() {
    let src = "\
fn main() -> i64:
    let s: str = \"ab\"
    let r: str = s * 3
    print(r)
    print(s)
    return 0
";
    // `r` is "ababab"; `s` survives the borrow-not-move repeat and is
    // still "ab" (used again on the next line) — both drop once.
    assert_build_run("str_mul_e2e_06", src, "ababab\nab\n");
}
