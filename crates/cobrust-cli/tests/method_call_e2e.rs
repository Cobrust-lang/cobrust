//! ADR-0052d-prereq Wave-2 end-to-end corpus for the method-call sugar
//! surface.
//!
//! Per ADR-0052d-prereq §"TEST+DEV PAIR" deliverable: these tests write
//! a `.cb` program that uses the new method-form (e.g. `s.split(",")`,
//! `xs.len()`, `f.is_nan()`), invoke `cobrust build`, run the produced
//! executable with a fixed stdin, and assert stdout matches the
//! original PRELUDE-fn-form oracle byte-identical. The method-form is
//! purely syntactic sugar over the PRELUDE-fn call per ADR-0052d-prereq
//! §"Decision"; the E2E identity is the strongest behavioural guarantee
//! the corpus can encode.
//!
//! Test families (5 programs covering LC-style idioms):
//!
//! - `e0052dpre_e2e_01` — CSV parser using `s.split(",")` to count fields.
//! - `e0052dpre_e2e_02` — Substring search using `s.find("...")`.
//! - `e0052dpre_e2e_03` — Prefix check using `s.starts_with(prefix)`.
//! - `e0052dpre_e2e_04` — List iteration using `xs.len()` + `xs.get(i)`.
//! - `e0052dpre_e2e_05` — Numerical clamp using `f.is_nan()` + `f.floor()`.
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09: module-level
//! 18-lint test-only allow header at the top.
//!
//! Pre-DEV-impl status: every `e0052dpre_e2e_*` test below is
//! `#[ignore]`'d pending Wave-2 DEV merge per F28 strict-separation
//! PAIR pattern (`findings/adsd-pair-pattern-impl-gap.md`). DEV's
//! responsibility is to land the four `try_synth_*_method` fns + the
//! chain dispatcher, then unmark the tests and verify oracle-identical
//! stdout against the PRELUDE-fn-form baseline.

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
// e0052dpre_e2e_01 — CSV parser using `s.split(",")` to count fields
//
// Mirrors ADR-0052d-prereq §"Latent-consumer enumeration" item 1
// ("`s.split(",")` in CSV parsers"). Counts the number of fields in a
// comma-separated input line; oracle byte-identical to the PRELUDE-fn
// form `split(s, ",")`. Method-form is pure sugar over the PRELUDE-fn
// rewrite per §"Decision".
// =====================================================================

#[test]
#[ignore = "ADR-0052d-prereq DEV impl pending"]
fn e0052dpre_e2e_01_csv_split_field_count() {
    // CSV parsing via the method-form `s.split(",")`. Input "a,b,c\n"
    // splits into 3 fields; oracle "3\n".
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let xs: list[str] = s.split(\",\")\n    let n: i64 = xs.len()\n    print_int(n)\n    return 0\n";
    assert_build_run("e0052dpre_e2e_01", src, &[], b"a,b,c\n", "3\n");
}

// =====================================================================
// e0052dpre_e2e_02 — Substring search using `s.find("...")`
//
// Mirrors ADR-0052d-prereq §"Latent-consumer enumeration" item 6
// inversion ("substring search"). Locates the first occurrence of
// "ll" in "hello"; oracle "2\n" (0-indexed).
// =====================================================================

#[test]
#[ignore = "ADR-0052d-prereq DEV impl pending"]
fn e0052dpre_e2e_02_substring_find_first_occurrence() {
    // `s.find("ll")` on "hello" returns 2 (the index of the first 'l'
    // in "ll"). Method-form rewrite to `find(s, "ll")`.
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let i: i64 = s.find(\"ll\")\n    print_int(i)\n    return 0\n";
    assert_build_run("e0052dpre_e2e_02", src, &[], b"hello\n", "2\n");
}

// =====================================================================
// e0052dpre_e2e_03 — Prefix check using `s.starts_with(prefix)`
//
// Mirrors ADR-0052d-prereq §"Latent-consumer enumeration" item 7
// ("tokenizers"). Tests whether the input line starts with "Hello";
// oracle "1\n" for true, "0\n" for false. Method-form rewrite to
// `starts_with(s, "Hello")`.
// =====================================================================

#[test]
#[ignore = "ADR-0052d-prereq DEV impl pending"]
fn e0052dpre_e2e_03_prefix_check_starts_with() {
    // `s.starts_with("Hello")` on "Hello, world\n" returns true (1).
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let b: bool = s.starts_with(\"Hello\")\n    if b:\n        print_int(1)\n    else:\n        print_int(0)\n    return 0\n";
    assert_build_run("e0052dpre_e2e_03", src, &[], b"Hello, world\n", "1\n");
}

// =====================================================================
// e0052dpre_e2e_04 — List iteration using `xs.len()` + `xs.get(i)`
//
// Mirrors ADR-0052d-prereq §"Latent-consumer enumeration" item 3
// ("accumulator-loops"). Builds a list, iterates via `xs.len()` +
// `xs.get(i)` to compute a sum; oracle "60\n" (10+20+30).
// =====================================================================

#[test]
#[ignore = "ADR-0052d-prereq DEV impl pending"]
fn e0052dpre_e2e_04_list_len_get_iteration() {
    // `xs.len()` and `xs.get(i)` method-forms inside a while loop.
    // List literal [10, 20, 30] sums to 60.
    let src = "fn main() -> i64:\n    let xs: list[i64] = [10, 20, 30]\n    let n: i64 = xs.len()\n    let i: i64 = 0\n    let total: i64 = 0\n    while i < n:\n        total = total + xs.get(i)\n        i = i + 1\n    print_int(total)\n    return 0\n";
    assert_build_run("e0052dpre_e2e_04", src, &[], b"", "60\n");
}

// =====================================================================
// e0052dpre_e2e_05 — Numerical clamp using `f.is_nan()` + `f.floor()`
//
// Mirrors ADR-0052d-prereq §"Latent-consumer enumeration" item 4
// ("numerical-rounding paths"). Combines `f.is_nan()` predicate with
// `f.floor()` rewrite; oracle "3\n" (the integer floor of 3.7).
// =====================================================================

#[test]
#[ignore = "ADR-0052d-prereq DEV impl pending"]
fn e0052dpre_e2e_05_float_is_nan_floor_clamp() {
    // Conditional clamp: if NaN, print 0; else print floor as i64.
    // Input 3.7 → not NaN → floor → 3.
    let src = "fn main() -> i64:\n    let x: f64 = 3.7\n    let b: bool = x.is_nan()\n    if b:\n        print_int(0)\n    else:\n        let y: f64 = x.floor()\n        let n: i64 = y as i64\n        print_int(n)\n    return 0\n";
    assert_build_run("e0052dpre_e2e_05", src, &[], b"", "3\n");
}
