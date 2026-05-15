//! M-F.3.1 — for-loop + `range(a, b)` end-to-end corpus.
//!
//! Per ADR-0050b §"Implementation map / M-F.3.1.A — prelude `range`":
//! `range(a, b)` ships as a real Cobrust prelude function body that
//! materialises a `list[i64]` of `b - a` slots. The for-loop iterates
//! the list via the ADR-0050b length-bound index lowering — MIR emits
//! `__cobrust_list_len` + `__cobrust_list_get` calls per iteration,
//! superseding the ADR-0027 iter-protocol path (which had a latent
//! 0-as-None sentinel collision on `list[i64]` elements that are
//! legitimately 0). No new MIR/codegen/runtime surface introduced.
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09: module-level
//! 18-lint test-only allow header at the TOP of every test file.

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

// =====================================================================
// Tier 1 — `cobrust check` accepts `range(a, b)` + `for i in range(...)`.
// =====================================================================

#[test]
fn f3r01_check_simple_for_range() {
    let src = write_cb(
        "f3r01_simple",
        "fn main() -> i64:\n    for i in range(0, 5):\n        print_int(i)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok; stderr={stderr}");
}

#[test]
fn f3r02_check_range_negative_start() {
    let src = write_cb(
        "f3r02_negative",
        "fn main() -> i64:\n    for i in range(-3, 3):\n        print_int(i)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok; stderr={stderr}");
}

#[test]
fn f3r03_check_range_empty() {
    let src = write_cb(
        "f3r03_empty",
        "fn main() -> i64:\n    for i in range(0, 0):\n        print_int(i)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok; stderr={stderr}");
}

#[test]
fn f3r04_check_range_reverse_is_empty() {
    // start > stop is treated as an empty range (Python semantics).
    let src = write_cb(
        "f3r04_reverse",
        "fn main() -> i64:\n    for i in range(5, 0):\n        print_int(i)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok; stderr={stderr}");
}

#[test]
fn f3r05_check_range_bound_to_var() {
    let src = write_cb(
        "f3r05_bound",
        "fn main() -> i64:\n    let r: list[i64] = range(0, 5)\n    for i in r:\n        print_int(i)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok; stderr={stderr}");
}

#[test]
fn f3r06_check_range_nested() {
    let src = write_cb(
        "f3r06_nested",
        "fn main() -> i64:\n    for i in range(0, 3):\n        for j in range(0, 3):\n            print_int((i + j))\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok; stderr={stderr}");
}

#[test]
fn f3r07_check_range_in_helper_fn() {
    let src = write_cb(
        "f3r07_helper",
        "fn loopy() -> i64:\n    for i in range(0, 5):\n        print_int(i)\n    return 0\nfn main() -> i64:\n    return loopy()\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok; stderr={stderr}");
}

#[test]
fn f3r08_check_range_in_arg_expression() {
    let src = write_cb(
        "f3r08_arg-expr",
        "fn main() -> i64:\n    let n: i64 = 5\n    for i in range(0, (n + 0)):\n        print_int(i)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok; stderr={stderr}");
}

// =====================================================================
// Tier 2 — full build + run; assert stdout matches expected.
// =====================================================================

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

#[test]
fn f3r09_run_range_0_to_5_prints_5_ints() {
    assert_build_run(
        "f3r09_basic",
        "fn main() -> i64:\n    for i in range(0, 5):\n        print_int(i)\n    return 0\n",
        &[],
        b"",
        "0\n1\n2\n3\n4\n",
    );
}

#[test]
fn f3r10_run_range_empty_skips_body() {
    assert_build_run(
        "f3r10_empty",
        "fn main() -> i64:\n    for i in range(0, 0):\n        print_int(i)\n    print_int(-1)\n    return 0\n",
        &[],
        b"",
        "-1\n",
    );
}

#[test]
fn f3r11_run_range_reverse_skips_body() {
    assert_build_run(
        "f3r11_reverse",
        "fn main() -> i64:\n    for i in range(5, 0):\n        print_int(i)\n    print_int(-1)\n    return 0\n",
        &[],
        b"",
        "-1\n",
    );
}

#[test]
fn f3r12_run_range_negative_values() {
    assert_build_run(
        "f3r12_negative",
        "fn main() -> i64:\n    for i in range(-3, 3):\n        print_int(i)\n    return 0\n",
        &[],
        b"",
        "-3\n-2\n-1\n0\n1\n2\n",
    );
}

#[test]
fn f3r13_run_range_sum_via_outer_var() {
    assert_build_run(
        "f3r13_sum",
        "fn main() -> i64:\n    let acc: i64 = 0\n    for i in range(0, 10):\n        acc = (acc + i)\n    print_int(acc)\n    return 0\n",
        &[],
        b"",
        "45\n",
    );
}

#[test]
fn f3r14_run_range_nested_cartesian_product_count() {
    assert_build_run(
        "f3r14_nested",
        "fn main() -> i64:\n    let n: i64 = 0\n    for i in range(0, 3):\n        for j in range(0, 4):\n            n = (n + 1)\n    print_int(n)\n    return 0\n",
        &[],
        b"",
        "12\n",
    );
}

#[test]
fn f3r15_run_range_early_return() {
    assert_build_run(
        "f3r15_early-return",
        "fn first_seven() -> i64:\n    for i in range(0, 100):\n        if (i == 7):\n            return i\n    return -1\nfn main() -> i64:\n    let r: i64 = first_seven()\n    print_int(r)\n    return 0\n",
        &[],
        b"",
        "7\n",
    );
}

#[test]
fn f3r16_run_range_zero_to_one_yields_zero() {
    assert_build_run(
        "f3r16_one-elem",
        "fn main() -> i64:\n    for i in range(0, 1):\n        print_int(i)\n    return 0\n",
        &[],
        b"",
        "0\n",
    );
}

#[test]
fn f3r17_run_range_bound_to_var_then_iter() {
    assert_build_run(
        "f3r17_bound",
        "fn main() -> i64:\n    let r: list[i64] = range(2, 5)\n    for v in r:\n        print_int(v)\n    return 0\n",
        &[],
        b"",
        "2\n3\n4\n",
    );
}

#[test]
fn f3r18_run_range_arith_args() {
    assert_build_run(
        "f3r18_arith",
        "fn main() -> i64:\n    let a: i64 = 1\n    let b: i64 = 4\n    for i in range((a - 1), (b + 1)):\n        print_int(i)\n    return 0\n",
        &[],
        b"",
        "0\n1\n2\n3\n4\n",
    );
}

#[test]
fn f3r19_run_range_with_helper_call() {
    assert_build_run(
        "f3r19_helper",
        "fn double(x: i64) -> i64:\n    return (x + x)\nfn main() -> i64:\n    for i in range(0, 4):\n        print_int(double(i))\n    return 0\n",
        &[],
        b"",
        "0\n2\n4\n6\n",
    );
}

// ADR-0050b §"list[str] iter source": runtime works via ADR-0044 W2
// Phase 2 path (heap-Str pointers stored in list[i64] slots).
// M-F.3.1 locks the current behavior. Wave 2 M-F.3.2 (ADR-0050c)
// closes the ownership gap with proper Drop scheduling. Use a
// contains-check rather than exact stdout because argv[0] is the
// runtime-variable program path.
#[test]
fn f3r20_run_for_over_argv_list_str_contains() {
    let path = write_cb(
        "f3r20b_argv",
        "fn main() -> i64:\n    let args = argv()\n    for a in args:\n        print(a)\n    return 0\n",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, &["alpha", "beta", "gamma"], b"");
    assert_eq!(run_code, 0, "run failed; stderr={run_stderr}");
    assert!(
        stdout.contains("alpha\n"),
        "stdout missing alpha:\n{stdout}"
    );
    assert!(stdout.contains("beta\n"), "stdout missing beta:\n{stdout}");
    assert!(
        stdout.contains("gamma\n"),
        "stdout missing gamma:\n{stdout}"
    );
}

#[test]
fn f3r21_run_range_inside_while() {
    assert_build_run(
        "f3r21_for-in-while",
        "fn main() -> i64:\n    let outer: i64 = 0\n    while (outer < 2):\n        for i in range(0, 3):\n            print_int(i)\n        outer = (outer + 1)\n    return 0\n",
        &[],
        b"",
        "0\n1\n2\n0\n1\n2\n",
    );
}

#[test]
fn f3r22_run_while_inside_for_range() {
    assert_build_run(
        "f3r22_while-in-for",
        "fn main() -> i64:\n    for i in range(0, 3):\n        let k: i64 = 0\n        while (k < 2):\n            print_int(i)\n            k = (k + 1)\n    return 0\n",
        &[],
        b"",
        "0\n0\n1\n1\n2\n2\n",
    );
}

#[test]
fn f3r23_run_range_inside_if() {
    assert_build_run(
        "f3r23_if-for",
        "fn main() -> i64:\n    let p: bool = True\n    if p:\n        for i in range(0, 3):\n            print_int(i)\n    return 0\n",
        &[],
        b"",
        "0\n1\n2\n",
    );
}

#[test]
fn f3r24_run_range_one_arg_rejected() {
    // 1-arg range(N) — start defaults to 0 in Python but Cobrust
    // ships only the 2-arg form at M-F.3.1 per ADR-0050b §"`range(a, b, step)`".
    let src = write_cb(
        "f3r24_one-arg",
        "fn main() -> i64:\n    for i in range(5):\n        print_int(i)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR (2) for 1-arg range; got code={code}, stderr={stderr}"
    );
}

#[test]
fn f3r25_run_range_three_arg_rejected() {
    // 3-arg range(a, b, step) — deferred to Phase G per ADR-0050b.
    let src = write_cb(
        "f3r25_three-arg",
        "fn main() -> i64:\n    for i in range(0, 10, 2):\n        print_int(i)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR (2) for 3-arg range; got code={code}, stderr={stderr}"
    );
}

#[test]
fn f3r26_run_range_with_str_arg_rejected() {
    let src = write_cb(
        "f3r26_str-arg",
        "fn main() -> i64:\n    for i in range(\"a\", \"b\"):\n        print_int(i)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR for str arg; got code={code}, stderr={stderr}"
    );
}

#[test]
fn f3r27_run_for_iter_int_rejected() {
    let src = write_cb(
        "f3r27_iter-int",
        "fn main() -> i64:\n    for v in 42:\n        print_int(v)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR (2) for iter-over-int; got code={code}, stderr={stderr}"
    );
}

#[test]
fn f3r28_run_for_iter_str_rejected_phase_g_deferred() {
    // String iteration is deferred to Phase G per ADR-0050b
    // §"Iter source type checking".
    let src = write_cb(
        "f3r28_iter-str",
        "fn main() -> i64:\n    for c in \"hello\":\n        print(c)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(
        code, 2,
        "expected TYPE_ERROR (2) for iter-over-str; got code={code}, stderr={stderr}"
    );
}

#[test]
fn f3r29_run_range_large_count() {
    // O(b - a) memory cost noted in ADR-0050b §"Negative consequences";
    // 1000 elements is well within bounds.
    let mut expected = String::new();
    for v in 0..50 {
        expected.push_str(&format!("{}\n", v));
    }
    assert_build_run(
        "f3r29_large",
        "fn main() -> i64:\n    for i in range(0, 50):\n        print_int(i)\n    return 0\n",
        &[],
        b"",
        &expected,
    );
}

#[test]
fn f3r30_run_range_with_deeply_nested() {
    // 3-deep nesting; tighter bounds to keep test fast.
    assert_build_run(
        "f3r30_triple-nest",
        "fn main() -> i64:\n    let total: i64 = 0\n    for i in range(0, 2):\n        for j in range(0, 2):\n            for k in range(0, 2):\n                total = (total + 1)\n    print_int(total)\n    return 0\n",
        &[],
        b"",
        "8\n",
    );
}
