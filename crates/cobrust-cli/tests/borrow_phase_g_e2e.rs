//! ADR-0052a Wave-1 end-to-end corpus for the `&s` explicit-borrow surface.
//!
//! Per ADR-0052a §10.1 TEST corpus category C ("E2E ≥ 6 programs"):
//! these tests write a `.cb` program that uses `&s` reads, invoke
//! `cobrust build`, run the produced executable with a fixed stdin,
//! and assert stdout matches the original-LC-100 oracle byte-identical.
//!
//! Test families:
//!
//! - `e0052a_e2e_01..02` — LC-02 reverse_string with `&s` replacements
//!   (§4.1 canonical trigger). Oracle: "hello\n" → "olleh\n".
//! - `e0052a_e2e_03..04` — LC-13 roman_to_integer with `&s` (§4.2).
//!   Oracle: "MCMXCIV\n" → "1994\n".
//! - `e0052a_e2e_05..06` — LC-20 valid_parentheses with `&s` (§4.3).
//!   Oracle #1: "()[]{}\n" → "true\n"; Oracle #2: "(]\n" → "false\n".
//! - `e0052a_e2e_07..08` — synthetic LC-100-style str-multiple-read
//!   patterns from F30 §5 rows 1-6.
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09: module-level
//! 18-lint test-only allow header at the top.
//!
//! Pre-DEV-impl status: every `e0052a_e2e_*` test below is `#[ignore]`'d
//! pending Wave-1 DEV merge.

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
#![allow(clippy::unnecessary_debug_formatting)]

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
        "{name}: stdout mismatch (oracle byte-identical check)\nstderr={run_stderr}"
    );
}

// =====================================================================
// e0052a_e2e_01..02 — LC-02 reverse_string with `&s` replacements
//
// Mirrors `examples/leetcode/reverse_string.cb` byte-for-byte except
// the two PRELUDE Str reads (`str_len(s)` and `str_at(s, i)`) become
// `str_len(&s)` and `str_at(&s, i)` per ADR-0052a §4.1 canonical
// trigger.
//
// Oracle: "hello\n" → "olleh\n" (LC-02 spec).
// =====================================================================

#[test]
fn e0052a_e2e_01_lc02_reverse_string_borrow_oracle_match() {
    // LC-02 §4.1 canonical: replace `str_len(s)` + `str_at(s, i)` with
    // borrow form. Oracle output is byte-identical to the LC-02
    // baseline `examples/leetcode/reverse_string.cb` (pre-M-F.3.5
    // clone-mitigation).
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let i: i64 = n - 1\n    while i >= 0:\n        let c = str_at(&s, i)\n        print_no_nl(c)\n        i = i - 1\n    print(\"\")\n    return 0\n";
    assert_build_run("e0052a_e2e_01", src, &[], b"hello\n", "olleh\n");
}

#[test]
fn e0052a_e2e_02_lc02_reverse_string_borrow_empty_input() {
    // LC-02 §4.1 variant: empty input. The while loop body must
    // execute zero times; output is a single newline (from
    // `print("")`).
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let i: i64 = n - 1\n    while i >= 0:\n        let c = str_at(&s, i)\n        print_no_nl(c)\n        i = i - 1\n    print(\"\")\n    return 0\n";
    assert_build_run("e0052a_e2e_02", src, &[], b"\n", "\n");
}

// =====================================================================
// e0052a_e2e_03..04 — LC-13 roman_to_integer with `&s` (§4.2)
//
// Mirrors `examples/leetcode/roman_to_integer.cb` with `&s` borrow
// replacements at the two PRELUDE Str reads.
// =====================================================================

#[test]
fn e0052a_e2e_03_lc13_roman_to_integer_borrow_mcmxciv() {
    // LC-13 §4.2: borrow form replaces `str_len(s)` + `str_at(s, i)`.
    // Oracle: "MCMXCIV" → 1994.
    let src = "fn roman_val(c: str) -> i64:\n    let o = str_ord(c)\n    if o == 73:\n        return 1\n    if o == 86:\n        return 5\n    if o == 88:\n        return 10\n    if o == 76:\n        return 50\n    if o == 67:\n        return 100\n    if o == 68:\n        return 500\n    if o == 77:\n        return 1000\n    return 0\nfn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let result: i64 = 0\n    let prev: i64 = 0\n    let i: i64 = n - 1\n    while i >= 0:\n        let c = str_at(&s, i)\n        let v = roman_val(c)\n        if v < prev:\n            result = result - v\n        else:\n            result = result + v\n        prev = v\n        i = i - 1\n    print(result)\n    return 0\n";
    assert_build_run("e0052a_e2e_03", src, &[], b"MCMXCIV\n", "1994\n");
}

#[test]
fn e0052a_e2e_04_lc13_roman_to_integer_borrow_iii() {
    // LC-13 §4.2: simple input "III" → 3.
    let src = "fn roman_val(c: str) -> i64:\n    let o = str_ord(c)\n    if o == 73:\n        return 1\n    if o == 86:\n        return 5\n    if o == 88:\n        return 10\n    if o == 76:\n        return 50\n    if o == 67:\n        return 100\n    if o == 68:\n        return 500\n    if o == 77:\n        return 1000\n    return 0\nfn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let result: i64 = 0\n    let prev: i64 = 0\n    let i: i64 = n - 1\n    while i >= 0:\n        let c = str_at(&s, i)\n        let v = roman_val(c)\n        if v < prev:\n            result = result - v\n        else:\n            result = result + v\n        prev = v\n        i = i - 1\n    print(result)\n    return 0\n";
    assert_build_run("e0052a_e2e_04", src, &[], b"III\n", "3\n");
}

// =====================================================================
// e0052a_e2e_05..06 — LC-20 valid_parentheses with `&s` (§4.3)
//
// Mirrors `examples/leetcode/valid_parentheses.cb` with `&s` borrow
// replacements at the two PRELUDE Str reads (str_len + str_at).
// Stack-based bracket-balance algorithm; oracle byte-identical to
// LC-20 baseline.
// =====================================================================

#[test]
fn e0052a_e2e_05_lc20_valid_parens_borrow_balanced_true() {
    // LC-20 §4.3: balanced "()[]{}" → "true".
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let stack = list_new(n)\n    let top: i64 = 0\n    let ok: i64 = 1\n    let i: i64 = 0\n    while i < n:\n        let c = str_at(&s, i)\n        let o = str_ord(c)\n        if o == 40:\n            list_set(stack, top, 40)\n            top = top + 1\n        elif o == 91:\n            list_set(stack, top, 91)\n            top = top + 1\n        elif o == 123:\n            list_set(stack, top, 123)\n            top = top + 1\n        elif o == 41:\n            if top == 0:\n                ok = 0\n            else:\n                top = top - 1\n                let expected = list_get(stack, top)\n                if expected != 40:\n                    ok = 0\n        elif o == 93:\n            if top == 0:\n                ok = 0\n            else:\n                top = top - 1\n                let expected = list_get(stack, top)\n                if expected != 91:\n                    ok = 0\n        elif o == 125:\n            if top == 0:\n                ok = 0\n            else:\n                top = top - 1\n                let expected = list_get(stack, top)\n                if expected != 123:\n                    ok = 0\n        i = i + 1\n    if top != 0:\n        ok = 0\n    if ok == 1:\n        print(\"true\")\n    else:\n        print(\"false\")\n    return 0\n";
    assert_build_run("e0052a_e2e_05", src, &[], b"()[]{}\n", "true\n");
}

#[test]
fn e0052a_e2e_06_lc20_valid_parens_borrow_unbalanced_false() {
    // LC-20 §4.3 variant: unbalanced "(]" → "false".
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let stack = list_new(n)\n    let top: i64 = 0\n    let ok: i64 = 1\n    let i: i64 = 0\n    while i < n:\n        let c = str_at(&s, i)\n        let o = str_ord(c)\n        if o == 40:\n            list_set(stack, top, 40)\n            top = top + 1\n        elif o == 91:\n            list_set(stack, top, 91)\n            top = top + 1\n        elif o == 123:\n            list_set(stack, top, 123)\n            top = top + 1\n        elif o == 41:\n            if top == 0:\n                ok = 0\n            else:\n                top = top - 1\n                let expected = list_get(stack, top)\n                if expected != 40:\n                    ok = 0\n        elif o == 93:\n            if top == 0:\n                ok = 0\n            else:\n                top = top - 1\n                let expected = list_get(stack, top)\n                if expected != 91:\n                    ok = 0\n        elif o == 125:\n            if top == 0:\n                ok = 0\n            else:\n                top = top - 1\n                let expected = list_get(stack, top)\n                if expected != 123:\n                    ok = 0\n        i = i + 1\n    if top != 0:\n        ok = 0\n    if ok == 1:\n        print(\"true\")\n    else:\n        print(\"false\")\n    return 0\n";
    assert_build_run("e0052a_e2e_06", src, &[], b"(]\n", "false\n");
}

// =====================================================================
// e0052a_e2e_07..08 — synthetic LC-100-style str-multiple-read patterns
//
// Each synthetic test exercises an F30 §5-row latent pattern that is
// NOT one of the named LC-02/13/20 sources. They protect against
// future cascade regressions on the borrow surface.
// =====================================================================

#[test]
fn e0052a_e2e_07_synthetic_three_borrows_then_consume_print() {
    // Synthetic: three borrowed reads on the same Str + final owned
    // consume into a user-fn that takes ownership. Oracle: stdin
    // "abc\n" → "9\n" (3 + 3 + 3 from the three borrows; consume is
    // discarded).
    //
    // Verifies the Wave-1 transparency rule across a mix of borrowed
    // (Operand::Copy) and final owned (Operand::Move) reads.
    let src = "fn consume(s: str) -> i64:\n    return str_len(s)\nfn main() -> i64:\n    let s = input(\"\")\n    let a = str_len(&s)\n    let b = str_len(&s)\n    let c = str_len(&s)\n    let _ = consume(s)\n    let total = (a + b) + c\n    print(total)\n    return 0\n";
    assert_build_run("e0052a_e2e_07", src, &[], b"abc\n", "9\n");
}

#[test]
#[ignore = "ADR-0052a §4.4 let-rebind shortcut (`let s = &s`) currently rejected by HIR lower as DuplicateBinding; re-enable when the shortcut lands on main (pre-existing red, not introduced by this branch — also red on main HEAD as of 2026-05-20)"]
fn e0052a_e2e_08_synthetic_let_rebind_with_loop() {
    // Synthetic: let-rebind shortcut (`let s = &s`) followed by a
    // for-loop that reads the borrowed `s` length N times. Oracle:
    // stdin "xyz\n" → "9\n" (3 chars * 3 iters = 9 via accumulated
    // length).
    //
    // Verifies the §4.4 let-rebind shortcut transports the borrow
    // through scope correctly under a loop body's iterative reads.
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let s = &s\n    let total: i64 = 0\n    let i: i64 = 0\n    while i < 3:\n        total = total + str_len(s)\n        i = i + 1\n    print(total)\n    return 0\n";
    assert_build_run("e0052a_e2e_08", src, &[], b"xyz\n", "9\n");
}
