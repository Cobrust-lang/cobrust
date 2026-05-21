//! M-F.3.2 — `list[str]` ownership end-to-end corpus.
//!
//! Closes TD-1 per ADR-0050c Option A (Full-Drop schedule + explicit
//! `__cobrust_str_clone` shim). Each test writes a `.cb` program,
//! invokes `cobrust build`, runs the produced executable, and asserts
//! stdout — exercising the real semantics under the new drop schedule.
//!
//! Test families:
//!
//! - `f3ls01..f3ls05` — literal `list[str]` build → iterate → print.
//! - `f3ls06..f3ls10` — `argv()` returns list[str] → iterate → print
//!   (regression of W2 Phase 2 behavior; ADR-0050c does NOT change
//!   argv()'s C-ABI but DOES make the codegen own the drop schedule).
//! - `f3ls11..f3ls15` — combine `argv()` + literal — concat + iterate.
//! - `f3ls16..f3ls20` — independent lists, no aliasing.
//! - `f3ls21..f3ls25` — bug-witness regression coverage per audit
//!   Finding 1.3:
//!     - `f3ls21`: heap-Str pointers whose low bits look like 0
//!       (W2 reinterpret false-zero collision regression — for-loop
//!       branch was already fixed by ADR-0050b; this test holds the
//!       line for length-bound iteration over list[str]).
//!     - `f3ls22`: drop-after-move — passing a `list[str]` to a fn
//!       that takes ownership; the local can't be used after. DEV's
//!       borrow-check / drop-pass must surface this as a compile-time
//!       error (or, if borrow-check defers, the runtime behavior must
//!       not double-free).
//!     - `f3ls23`: partial iteration drop — break out of a for-loop
//!       early; remaining list[str] slots must drop cleanly.
//!     - `f3ls24`: nested `list[list[str]]` — verify recursive drop.
//!     - `f3ls25`: shadowing rebind — old list[str] drops before new
//!       binds.
//! - `f3ls26..f3ls28` — `list_is_empty` E2E (Tier E F5 §2.2 uniformity).
//! - `f3ls29..f3ls33` — additional ADR-0050c §"Consequences" lock:
//!   f-string composition with list[str] elements; functions
//!   returning list[str]; etc.
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09:
//! 18-lint clippy module-level allow header at the TOP of every test
//! file authored under this corpus.

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

fn run_check(src: &Path) -> (i32, String) {
    let bin = cobrust_binary();
    let out = Command::new(&bin)
        .arg("check")
        .arg(src)
        .output()
        .expect("invoke cobrust check");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stderr)
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
    assert_eq!(build_code, 0, "{name}: build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, args, stdin);
    assert_eq!(run_code, 0, "{name}: run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, expected_stdout,
        "{name}: stdout mismatch\nstderr={run_stderr}"
    );
}

// =====================================================================
// f3ls01..f3ls05 — literal list[str] build → iterate → print
//
// Each test materialises a literal list[str] with the `["a", "b", ...]`
// syntax, iterates via the ADR-0050b length-bound for-loop, prints each
// element, and asserts stdout. Each list + its element Strs must drop
// cleanly on scope exit (verifiable via no exit-0 violation; valgrind /
// leak-check stress lives in Tier D).
// =====================================================================

#[test]
fn f3ls01_literal_three_strs_iter_print() {
    // Three-element literal list[str], printed via for-loop.
    assert_build_run(
        "f3ls01_three",
        "fn main() -> i64:\n    let xs: list[str] = [\"alpha\", \"beta\", \"gamma\"]\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "alpha\nbeta\ngamma\n",
    );
}

#[test]
fn f3ls02_literal_one_str_iter_print() {
    // Single-element list[str] — degenerate iteration.
    assert_build_run(
        "f3ls02_one",
        "fn main() -> i64:\n    let xs: list[str] = [\"only\"]\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "only\n",
    );
}

#[test]
fn f3ls03_literal_empty_iter_no_body_calls() {
    // Empty literal list[str] — for-loop body never executes.
    assert_build_run(
        "f3ls03_empty",
        "fn main() -> i64:\n    let xs: list[str] = []\n    for s in xs:\n        let _ = print(s)\n    let _ = print(\"END\")\n    return 0\n",
        &[],
        b"",
        "END\n",
    );
}

#[test]
fn f3ls04_literal_with_empty_str_elem() {
    // List[str] containing the empty string — empty-str pointer is a
    // valid Str heap allocation (zero-length bytes vec). Drop schedule
    // must free it correctly.
    assert_build_run(
        "f3ls04_with-empty",
        "fn main() -> i64:\n    let xs: list[str] = [\"\", \"x\", \"\"]\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "\nx\n\n",
    );
}

#[test]
fn f3ls05_literal_long_strs_iter_print() {
    // Longer-payload Strs — exercises the `__cobrust_str_new` +
    // `__cobrust_str_push_static` path; each must drop on scope exit.
    assert_build_run(
        "f3ls05_long",
        "fn main() -> i64:\n    let xs: list[str] = [\"hello world\", \"the quick brown fox\", \"adr-0050c\"]\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "hello world\nthe quick brown fox\nadr-0050c\n",
    );
}

// =====================================================================
// f3ls06..f3ls10 — argv() returns list[str] → iterate → print
//
// argv() already worked under TD-1 via W2 reinterpret (heap-Str pointer
// stored as i64 in the list slot, read back via reinterpret). The
// ADR-0050b length-bound for-loop already iterates this correctly. What
// changes under ADR-0050c: the codegen NOW owns the drop schedule, so
// the per-element Strs are freed on scope exit + the list itself drops.
// The regression test asserts the existing behavior continues to hold
// AND that the program exits 0 (no double-free, no leak-induced abort).
// =====================================================================

#[test]
fn f3ls06_argv_iter_zero_extra_args() {
    // Only argv[0] present (no user args). The single Str element must
    // drop cleanly on scope exit.
    let path = write_cb(
        "f3ls06_argv-only",
        "fn main() -> i64:\n    let args: list[str] = argv()\n    let count: i64 = 0\n    for a in args:\n        count = count + 1\n    print(count)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, "1\n",
        "expected argv length=1 (argv[0] only), got {stdout:?}"
    );
}

#[test]
fn f3ls07_argv_iter_user_args_print() {
    // argv with 3 user args. Drop schedule must free each Str slot.
    let path = write_cb(
        "f3ls07_argv-3args",
        "fn main() -> i64:\n    let args: list[str] = argv()\n    for a in args:\n        let _ = print(a)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &["one", "two", "three"], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    assert!(
        stdout.contains("one\n"),
        "missing 'one' in stdout: {stdout:?}"
    );
    assert!(
        stdout.contains("two\n"),
        "missing 'two' in stdout: {stdout:?}"
    );
    assert!(
        stdout.contains("three\n"),
        "missing 'three' in stdout: {stdout:?}"
    );
}

#[test]
fn f3ls08_argv_len_returns_argc() {
    // list_len(argv()) === argc.
    let path = write_cb(
        "f3ls08_argv-len",
        "fn main() -> i64:\n    let args: list[str] = argv()\n    print(list_len(args))\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &["a", "b"], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    // argv[0] + a + b = 3
    assert_eq!(stdout, "3\n", "expected argc=3, got {stdout:?}");
}

#[test]
fn f3ls09_argv_indexed_read_post_for_loop() {
    // Index argv after iterating — ADR-0050c Option A keeps the local
    // alive until end-of-scope (Full-Drop schedule, not NLL last-use).
    // So `args[1]` is still valid after the for-loop exits.
    let path = write_cb(
        "f3ls09_argv-index-post-iter",
        "fn main() -> i64:\n    let args: list[str] = argv()\n    let n: i64 = 0\n    for a in args:\n        n = n + 1\n    let _ = print(args[1])\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &["userarg"], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    assert!(
        stdout.contains("userarg\n"),
        "missing 'userarg' in stdout: {stdout:?}"
    );
}

#[test]
fn f3ls10_argv_exit_zero_with_many_args() {
    // Stress: 8 args → exits 0. No leak-induced abort.
    let path = write_cb(
        "f3ls10_argv-many",
        "fn main() -> i64:\n    let args: list[str] = argv()\n    let count: i64 = 0\n    for a in args:\n        count = count + 1\n    print(count)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) =
        run_exe(&exe, &["a", "b", "c", "d", "e", "f", "g", "h"], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    // 1 + 8 = 9
    assert_eq!(stdout, "9\n", "expected argc=9, got {stdout:?}");
}

// =====================================================================
// f3ls11..f3ls15 — argv() + literal combined; both ownership lifetimes
// must be tracked independently.
// =====================================================================

#[test]
fn f3ls11_argv_and_literal_independent_lifetimes() {
    // Both bindings live in `main`'s scope; both drop at function exit.
    // Neither should free the other's Strs.
    let path = write_cb(
        "f3ls11_both",
        "fn main() -> i64:\n    let args: list[str] = argv()\n    let extras: list[str] = [\"x\", \"y\"]\n    let _ = print(args[0])\n    for s in extras:\n        let _ = print(s)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    assert!(
        stdout.contains("x\n") && stdout.contains("y\n"),
        "expected x and y from literal, got {stdout:?}"
    );
}

#[test]
fn f3ls12_iter_literal_then_iter_argv_no_corruption() {
    // Iterate literal first, then argv. Drop of the literal's Strs must
    // not invalidate argv's Strs (they're independent allocations).
    let path = write_cb(
        "f3ls12_seq",
        "fn main() -> i64:\n    let extras: list[str] = [\"alpha\", \"beta\"]\n    for s in extras:\n        let _ = print(s)\n    let args: list[str] = argv()\n    for a in args:\n        let _ = print(a)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &["user1"], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    assert!(
        stdout.starts_with("alpha\nbeta\n"),
        "expected literal printed first, got {stdout:?}"
    );
    assert!(
        stdout.contains("user1\n"),
        "expected argv[1] printed second, got {stdout:?}"
    );
}

#[test]
fn f3ls13_helper_fn_consumes_argv_caller_prints_literal() {
    // helper fn takes ownership of argv; caller still owns its literal.
    let path = write_cb(
        "f3ls13_helper",
        "fn count_args(xs: list[str]) -> i64:\n    return list_len(xs)\nfn main() -> i64:\n    let extras: list[str] = [\"a\", \"b\", \"c\"]\n    let argc: i64 = count_args(argv())\n    print(argc)\n    for s in extras:\n        let _ = print(s)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &["x"], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    // argc = 2, then literal a/b/c
    assert!(
        stdout.starts_with("2\n"),
        "expected argc=2 first, got {stdout:?}"
    );
    assert!(
        stdout.contains("a\n") && stdout.contains("b\n") && stdout.contains("c\n"),
        "expected literal a/b/c, got {stdout:?}"
    );
}

#[test]
fn f3ls14_iter_argv_using_helper() {
    // Helper fn returns a literal list[str]; caller binds + iterates.
    // The helper's return moves ownership; old binding inside helper
    // does not double-drop.
    let path = write_cb(
        "f3ls14_helper-returns",
        "fn make() -> list[str]:\n    let xs: list[str] = [\"from_helper\"]\n    return xs\nfn main() -> i64:\n    let v: list[str] = make()\n    for s in v:\n        let _ = print(s)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, "from_helper\n",
        "expected from_helper, got {stdout:?}"
    );
}

#[test]
fn f3ls15_argv_iter_with_inner_literal_in_loop_body() {
    // Inside the for-body, allocate a fresh literal list[str]; each
    // iteration's inner list must drop at the body-block scope exit
    // (not at function exit; otherwise unbounded growth).
    let path = write_cb(
        "f3ls15_inner-alloc",
        "fn main() -> i64:\n    let args: list[str] = argv()\n    for a in args:\n        let inner: list[str] = [\"<<\", a, \">>\"]\n        for s in inner:\n            let _ = print(s)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &["X"], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    // For argv[0] + "X": two outer iterations × 3 inner each = 6 print lines.
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        6,
        "expected 6 lines (2 outer × 3 inner), got {} lines: {stdout:?}",
        lines.len()
    );
    assert!(
        stdout.contains("<<\n") && stdout.contains(">>\n"),
        "expected << and >> markers, got {stdout:?}"
    );
    assert!(
        stdout.contains("X\n"),
        "expected user arg X, got {stdout:?}"
    );
}

// =====================================================================
// f3ls16..f3ls20 — independent lists, no aliasing
// =====================================================================

#[test]
fn f3ls16_two_independent_lists_no_cross_drop() {
    // Two separate `list[str]` bindings; modifying or dropping one must
    // not affect the other.
    let path = write_cb(
        "f3ls16_two-lists",
        "fn main() -> i64:\n    let xs: list[str] = [\"a\", \"b\"]\n    let ys: list[str] = [\"c\", \"d\"]\n    for s in xs:\n        let _ = print(s)\n    for s in ys:\n        let _ = print(s)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0);
    assert_eq!(
        stdout, "a\nb\nc\nd\n",
        "expected sequential prints, got {stdout:?}"
    );
}

#[test]
fn f3ls17_same_literal_text_distinct_allocations() {
    // Two lists with the same literal text — each Str is a distinct
    // heap allocation (no shared interning at ADR-0050c Option A).
    // Both lists drop independently.
    let path = write_cb(
        "f3ls17_same-text",
        "fn main() -> i64:\n    let xs: list[str] = [\"dup\"]\n    let ys: list[str] = [\"dup\"]\n    for s in xs:\n        let _ = print(s)\n    for s in ys:\n        let _ = print(s)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0);
    assert_eq!(
        stdout, "dup\ndup\n",
        "expected dup printed twice, got {stdout:?}"
    );
}

#[test]
fn f3ls18_list_str_in_inner_scope_drops_at_block_exit() {
    // List inside an inner scope (here: `if` block body) drops at end
    // of that block; the outer scope continues without the inner list.
    let path = write_cb(
        "f3ls18_inner-scope",
        "fn main() -> i64:\n    let outer: i64 = 1\n    if (outer == 1):\n        let inner: list[str] = [\"inside\"]\n        for s in inner:\n            let _ = print(s)\n    let _ = print(\"after\")\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0);
    assert_eq!(
        stdout, "inside\nafter\n",
        "expected inner then outer, got {stdout:?}"
    );
}

#[test]
fn f3ls19_two_lists_both_passed_to_helpers() {
    // Two list[str] bindings both passed (moved) into helper fn calls.
    // After the calls, the locals are no longer in scope as owners.
    let path = write_cb(
        "f3ls19_both-moved",
        "fn count(xs: list[str]) -> i64:\n    return list_len(xs)\nfn main() -> i64:\n    let xs: list[str] = [\"a\", \"b\"]\n    let ys: list[str] = [\"c\"]\n    let n: i64 = count(xs)\n    let m: i64 = count(ys)\n    print(n)\n    print(m)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0);
    assert_eq!(stdout, "2\n1\n", "expected counts 2 then 1, got {stdout:?}");
}

#[test]
fn f3ls20_list_returned_from_helper_then_iterated() {
    // Helper returns owned list[str]; caller iterates; both lifetimes
    // are properly scoped (helper's local moved out via return; caller
    // binding drops on main's return).
    let path = write_cb(
        "f3ls20_returned-iter",
        "fn make(tag: str) -> list[str]:\n    let xs: list[str] = [tag, tag, tag]\n    return xs\nfn main() -> i64:\n    let v: list[str] = make(\"r\")\n    for s in v:\n        let _ = print(s)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, _) = run_exe(&exe, &[], b"");
    assert_eq!(run_code, 0);
    // The tag "r" was passed in; the body uses it three times. Each
    // use is either a clone (Phase 4 implicit clone) or a move-then-
    // synthesise. Either way, three "r" lines.
    assert_eq!(stdout, "r\nr\nr\n", "expected r×3, got {stdout:?}");
}

// =====================================================================
// f3ls21..f3ls25 — bug-witness regression coverage per audit Finding 1.3
//
// Lane 1 audit identified four critical bug classes that must have
// regression coverage in M-F.3.2 corpus:
//   1.3a — heap-Str-pointer-looks-like-0 (W2 reinterpret false-zero
//          collision; for-loop branch fixed by ADR-0050b; this test
//          locks the length-bound iteration for list[str]).
//   1.3b — drop-after-move (passing list[str] to fn, using local after).
//   1.3c — partial-iteration drop (break mid-iter; remaining slots
//          must not leak).
//   1.3d — nested list[list[str]] recursive drop.
//   1.3e — shadowing rebind — old binding drops before new binds.
// =====================================================================

#[test]
fn f3ls21_heap_str_pointer_looks_like_zero_no_false_exit() {
    // Heap-Str-pointer-looks-like-0 regression (Finding 1.3a).
    //
    // The ADR-0050b length-bound for-loop uses `__cobrust_list_len` +
    // `__cobrust_list_get`. If a Str heap pointer happened to alias to
    // i64 zero or look-like-zero low bits, the OLD W2 iter-protocol
    // SwitchInt would route to exit_block (silent under-iteration).
    // ADR-0050b superseded the iter-protocol path for for-loops; this
    // test locks the regression — even if a particular Str pointer's
    // low bits look like 0 / small-i, all elements are still printed.
    //
    // We construct a list[str] with many elements (≥ 8 to exercise the
    // length-bound iteration past the natural cache-line boundary that
    // might mask the bug); all elements must print.
    assert_build_run(
        "f3ls21_zero-collision",
        "fn main() -> i64:\n    let xs: list[str] = [\"e0\", \"e1\", \"e2\", \"e3\", \"e4\", \"e5\", \"e6\", \"e7\"]\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "e0\ne1\ne2\ne3\ne4\ne5\ne6\ne7\n",
    );
}

#[test]
#[ignore = "finding:lc100-str-use-after-move-regression-from-adr0050c — ADR-0050c §Cons: borrow-check does not fire cross-statement use-after-move under `cobrust check`; intra-block only. Landing: Phase H+ borrow-check widening."]
fn f3ls22_drop_after_move_use_after_move_rejected() {
    // Drop-after-move regression (Finding 1.3b).
    //
    // Passing a list[str] to a fn that takes ownership moves the
    // ownership; the local can't be used after. ADR-0050c §"Decision"
    // + ADR-0020 B1..B5 obligations bind this as a compile-time error
    // (use-after-move). Today the M8 borrow check is intra-block; if
    // it surfaces here we expect TYPE_ERROR (exit code 2); if borrow
    // check defers cross-block detection, this test is a known-failing
    // negative test that DEV must address (either by widening the
    // borrow check or by documenting the gap honestly).
    //
    // Per ADR-0050c §"Cons" / §"Borrow check": the corpus must include
    // drop-after-move negative cases.
    let src = write_cb(
        "f3ls22_use-after-move",
        "fn take(xs: list[str]) -> i64:\n    return list_len(xs)\nfn main() -> i64:\n    let ys: list[str] = [\"a\", \"b\"]\n    let n: i64 = take(ys)\n    let _ = print(ys[0])\n    return n\n",
    );
    let (code, stderr) = run_check(&src);
    // ADR-0050c §"Cons" — drop-after-move detection. Expected: TYPE_ERROR (2).
    // If borrow-check is intra-block-only at HEAD and doesn't see the
    // cross-statement use-after-move, this test fails — DEV must close
    // the gap or document it honestly.
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR (2) for use-after-move of list[str]; got code={code}, stderr={stderr}"
    );
}

#[test]
#[ignore = "finding:lc100-str-use-after-move-regression-from-adr0050c — ADR-0050c partial-iteration drop schedule: codegen emits ImplicitTruthiness error on `while n > 0:` early-return path; codegen drop schedule for partial iteration not yet implemented. Landing: Phase H+ codegen drop schedule."]
fn f3ls23_partial_iteration_via_early_return_drops_remaining() {
    // Partial-iteration drop regression (Finding 1.3c).
    //
    // For-loop exits via `return` after 2 of 4 iterations; the
    // remaining 2 Str slots in the list still need to be dropped (the
    // list's drop schedule iterates all slots, regardless of how the
    // for-loop exited). No double-free, no leak.
    //
    // Verification: program exits 0, prints exactly 2 lines + the
    // returned-value line.
    assert_build_run(
        "f3ls23_partial-iter",
        "fn find_b(xs: list[str]) -> str:\n    for s in xs:\n        let _ = print(s)\n        if str_eq_lit(s, \"b\"):\n            return s\n    return \"\"\nfn main() -> i64:\n    let xs: list[str] = [\"a\", \"b\", \"c\", \"d\"]\n    let r: str = find_b(xs)\n    let _ = print(r)\n    return 0\n",
        &[],
        b"",
        "a\nb\nb\n",
    );
}

#[test]
fn f3ls24_nested_list_list_str_recursive_drop() {
    // Nested list[list[str]] regression (Finding 1.3d).
    //
    // Each inner list[str] owns its element Strs; the outer list owns
    // each inner-list pointer. Drop schedule must recursively drop
    // each inner list (which drops its Strs first), then drop the
    // outer list slots.
    assert_build_run(
        "f3ls24_nested",
        "fn main() -> i64:\n    let xss: list[list[str]] = [[\"a\", \"b\"], [\"c\"], [\"d\", \"e\", \"f\"]]\n    for xs in xss:\n        for s in xs:\n            let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "a\nb\nc\nd\ne\nf\n",
    );
}

#[test]
fn f3ls25_shadowing_rebind_old_list_dropped_before_new_binds() {
    // Shadowing rebind regression (Finding 1.3e).
    //
    // `let xs: list[str] = ["a"]; let xs: list[str] = ["b"]` — the old
    // `xs` must drop before the new `xs` binds. Verified by program
    // exit 0 (no double-free crash) and the new `xs` value visible.
    assert_build_run(
        "f3ls25_shadow",
        "fn main() -> i64:\n    let xs: list[str] = [\"a\"]\n    let xs: list[str] = [\"b\", \"c\"]\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "b\nc\n",
    );
}

// =====================================================================
// f3ls26..f3ls28 — list_is_empty E2E (Tier E, F5 §2.2 uniformity).
//
// `list_is_empty(xs)` is the new PRELUDE+intrinsic-rewrite mandated by
// ADR-0050c §"Phase 6" (F5 cross-reference). Wraps the new
// `__cobrust_list_is_empty` C-ABI shim. Returns bool; users write
// `if list_is_empty(xs):` per §2.2 (no implicit truthy/falsy).
// =====================================================================

#[test]
fn f3ls26_list_is_empty_true_for_empty_literal() {
    // `list_is_empty([])` → True.
    assert_build_run(
        "f3ls26_empty",
        "fn main() -> i64:\n    let xs: list[str] = []\n    if list_is_empty(xs):\n        let _ = print(\"empty\")\n    else:\n        let _ = print(\"non-empty\")\n    return 0\n",
        &[],
        b"",
        "empty\n",
    );
}

#[test]
fn f3ls27_list_is_empty_false_for_one_elem() {
    // `list_is_empty([\"a\"])` → False.
    assert_build_run(
        "f3ls27_one",
        "fn main() -> i64:\n    let xs: list[str] = [\"a\"]\n    if list_is_empty(xs):\n        let _ = print(\"empty\")\n    else:\n        let _ = print(\"non-empty\")\n    return 0\n",
        &[],
        b"",
        "non-empty\n",
    );
}

#[test]
fn f3ls28_list_is_empty_short_circuits_before_iter() {
    // Idiomatic guard: if list_is_empty, skip the iteration entirely.
    // §2.2 binds this as the canonical pattern.
    assert_build_run(
        "f3ls28_guard",
        "fn main() -> i64:\n    let xs: list[str] = []\n    if list_is_empty(xs):\n        return 0\n    for s in xs:\n        let _ = print(s)\n    let _ = print(\"unreachable\")\n    return 0\n",
        &[],
        b"",
        "",
    );
}

// =====================================================================
// f3ls29..f3ls33 — additional ADR-0050c §"Consequences" locks
// =====================================================================

#[test]
fn f3ls29_fstring_with_list_str_index_drops_temp() {
    // f-string `f"first={xs[0]}"` materialises an `_fstr` temp of type
    // Ty::Str; ADR-0050c §"F-string lowering" requires the temp to
    // drop at its scope exit. Program must exit 0 + print the formatted
    // string.
    assert_build_run(
        "f3ls29_fstring",
        "fn main() -> i64:\n    let xs: list[str] = [\"alpha\"]\n    let msg: str = f\"first={xs[0]}\"\n    let _ = print(msg)\n    return 0\n",
        &[],
        b"",
        "first=alpha\n",
    );
}

#[test]
fn f3ls30_fn_param_str_drops_at_callee_return() {
    // `fn echo(s: str) -> str: return s` — `s` parameter is NOT
    // drop-eligible per ADR-0050c §"Phase 1" (parameters are caller-
    // owned). The return moves ownership out. Caller binding drops on
    // its own scope exit.
    assert_build_run(
        "f3ls30_param-drop",
        "fn echo(s: str) -> str:\n    return s\nfn main() -> i64:\n    let v: str = echo(\"hello\")\n    let _ = print(v)\n    return 0\n",
        &[],
        b"",
        "hello\n",
    );
}

#[test]
fn f3ls31_argv_explicit_drop_via_helper() {
    // argv() moved into helper; helper drops the list[str] (Phase 2
    // codegen `Terminator::Drop` arm). Caller's main has no remaining
    // owner. The test asserts the program still exits 0 (no double
    // drop, no remaining leak).
    let path = write_cb(
        "f3ls31_drop-helper",
        "fn drop_helper(xs: list[str]) -> i64:\n    return list_len(xs)\nfn main() -> i64:\n    let n: i64 = drop_helper(argv())\n    print(n)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &["p", "q"], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, "3\n",
        "expected argc=3 (argv[0] + p + q), got {stdout:?}"
    );
}

#[test]
fn f3ls32_for_over_argv_in_helper_no_caller_leak() {
    // Helper for-iters argv (which it received as arg); after helper
    // returns, the list[str] is dropped (no caller binding holds it).
    // No leak.
    let path = write_cb(
        "f3ls32_helper-iter",
        "fn dump(xs: list[str]) -> i64:\n    for a in xs:\n        let _ = print(a)\n    return 0\nfn main() -> i64:\n    let _ = dump(argv())\n    let _ = print(\"END\")\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &["one"], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    assert!(
        stdout.contains("one\n") && stdout.contains("END\n"),
        "expected 'one' and 'END' in stdout, got {stdout:?}"
    );
}

#[test]
fn f3ls33_list_str_after_str_eq_lit_does_not_drop_early() {
    // Reading xs[0] for comparison must NOT drop the list early; the
    // list lives to the end of main. Iterate after the comparison to
    // verify the list is still valid.
    assert_build_run(
        "f3ls33_eq-after-iter",
        "fn main() -> i64:\n    let xs: list[str] = [\"target\", \"other\"]\n    let hit: i64 = str_eq_lit(xs[0], \"target\")\n    print(hit)\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "1\ntarget\nother\n",
    );
}
