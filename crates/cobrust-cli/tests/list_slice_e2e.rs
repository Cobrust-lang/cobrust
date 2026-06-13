//! F81 / ADR-0096 end-to-end corpus for the `list` index OPERATOR surface:
//! `xs[i]` (scalar, Python-negative-indexed) + `xs[lo:hi]` (slice),
//! ELEMENT-addressed. The LIST analogue of the `str`/`bytes` indexing arc
//! (`str_slice_e2e` F78/F79, `bytes_ops_e2e`).
//!
//! ## The two F81 §2.2 bugs this closes
//!
//! BUG 1 — `xs[-1]` SILENT MISCOMPILE. On `[10,20,30]`, `xs[-1]` USED to
//! print `0` (CPython `30`). The runtime `__cobrust_list_get` returned a
//! silent `0` sentinel for BOTH a negative index AND a positive OOB — an
//! in-band wrong value §2.2 forbids. Fixed (mirror F79 Option B): a
//! negative index Python-normalizes to `len + i` (`[10,20,30][-1] == 30`),
//! and a TRUE OOB (`i >= len`, OR `i < -len`) TRAPS via
//! `crate::panic::panic` → exit 3 (INTERNAL_PANIC), NOT a sentinel nor a
//! raw `assert!` abort (exit 134 + path-leaking backtrace).
//!
//! BUG 2 — `xs[lo:hi]` UB / MEMORY-SAFETY CRASH. `let ys = xs[1:3]` built
//! OK then CRASHED at runtime (`misaligned pointer dereference`) — list
//! slicing was an UNIMPLEMENTED STUB (`lower_index` returned the integer
//! `0`, used as a list handle → UB). Fixed (mirror the str/bytes slice
//! impl): `__cobrust_list_slice(xs, lo, hi) -> list` mints a FRESH owned
//! `list[i64]` for the `[lo, hi)` element range (CPython clamp on OOB,
//! Move-out so the single owner drops it once). The open-ended / stepped /
//! negative shapes REJECT at compile time (`TypeError::UnsupportedSliceShape`,
//! the str/bytes reject EXTENDED to `Ty::List`) — §2.5-A, NEVER a silent
//! miscompile / UB.
//!
//! Each test writes a `.cb` program, invokes `cobrust build`, runs the
//! produced executable, and asserts stdout byte-identical to the CPython-3
//! oracle (e.g. `[10,20,30][1:3] == [20,30]`, `[10,20,30][-1] == 30`).
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

/// Assert a `.cb` program is REJECTED at compile time (non-zero build
/// exit) and the diagnostic on stderr CONTAINS `needle` (the §2.5-B
/// fix-printing substring). A non-zero exit proves a Cobrust DIAGNOSTIC,
/// not a silent exit-0 miscompile / UB (the F81 BUG-2 hole this closes).
fn assert_build_rejects(name: &str, src: &str, needle: &str) {
    let path = write_cb(name, src);
    let (build_code, _exe, build_stderr) = run_build_exe(&path);
    assert_ne!(
        build_code, 0,
        "{name}: build must REJECT (non-zero exit), got 0; \
         stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    assert!(
        build_stderr.contains(needle),
        "{name}: reject diagnostic must contain {needle:?}; \
         got stderr=\n{build_stderr}"
    );
}

/// Assert a `.cb` program BUILDS fine (the trap is RUNTIME, not compile-
/// time) but TRAPS when run: exit 3 (std.panic INTERNAL_PANIC) + a stderr
/// diagnostic CONTAINING `needle`, with NOTHING on stdout. The F81 §2.2
/// guard: a true out-of-range list index is an unrecoverable TRAP, NOT a
/// silent in-band `0` sentinel — AND `== 3`, not merely `!= 0`, so a
/// regression to a raw-abort exit 134 (path-leaking backtrace) can NOT
/// pass (the weak `!= 0` is exactly the drift the F79B audit caught).
fn assert_build_run_traps(name: &str, src: &str, needle: &str) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "{name}: build must SUCCEED (the OOB trap is RUNTIME, not \
         compile-time); stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    let (run_code, stdout, run_stderr) = run_exe(&exe);
    assert_eq!(
        run_code, 3,
        "{name}: an out-of-range list index MUST trap with exit 3 \
         (INTERNAL_PANIC via crate::panic::panic) — NOT a silent sentinel, \
         NOR a raw `assert!` abort (exit 134 + path-leaking backtrace); \
         stdout={stdout:?} stderr={run_stderr:?}"
    );
    assert_eq!(
        stdout, "",
        "{name}: a trapping program must NOT emit any output before \
         trapping; got stdout={stdout:?}"
    );
    assert!(
        run_stderr.contains(needle),
        "{name}: trap diagnostic must contain {needle:?} (§2.5-B names \
         the bad index + length); got stderr={run_stderr:?}"
    );
}

// =====================================================================
// list_slice_e2e_01 — `xs[lo:hi]` slice (THE F81 BUG-2 FIX). CPython 3:
//   [10,20,30,40][1:3]  == [20,30]   (len 2)
//   [10,20,30,40][1:99] == [20,30,40] (Python clamps hi to len)
//   [10,20,30,40][3:1]  == []        (hi <= lo → empty)
//   [10,20,30,40][0:4]  == [10,20,30,40] (full span)
// BEFORE the fix, `xs[1:3]` built OK then CRASHED at runtime with
// `misaligned pointer dereference` (the slice lowered to a stub integer 0
// used as a list handle → UB). The slice mints a FRESH list the `.cb`
// scope drops once.
// =====================================================================

#[test]
fn list_slice_e2e_01_slice_basic_and_clamp() {
    let src = "\
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30, 40]
    let a: list[i64] = xs[1:3]
    print(len(a))
    print(a[0])
    print(a[1])
    let b: list[i64] = xs[1:99]
    print(len(b))
    print(b[2])
    let c: list[i64] = xs[3:1]
    print(len(c))
    let d: list[i64] = xs[0:4]
    print(len(d))
    print(d[0])
    print(d[3])
    return 0
";
    // a: len 2, 20, 30
    // b: len 3, 40
    // c: len 0
    // d: len 4, 10, 40
    assert_build_run("list_slice_e2e_01", src, "2\n20\n30\n3\n40\n0\n4\n10\n40\n");
}

// =====================================================================
// list_slice_e2e_02 — `xs[-i]` NEGATIVE SCALAR index (THE F81 BUG-1 FIX).
// CPython 3: [10,20,30][-1]==30, [-2]==20, [-3]==10. BEFORE the fix, EVERY
// negative index silently returned `0` (the in-band §2.2 sentinel). A
// non-negative scalar `xs[i]` returns the element directly (Copy scalar —
// `list[i64]`, unlike `str`'s fresh 1-codepoint str).
// =====================================================================

#[test]
fn list_slice_e2e_02_negative_and_positive_scalar() {
    let src = "\
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30]
    print(xs[0])
    print(xs[2])
    print(xs[-1])
    print(xs[-2])
    print(xs[-3])
    return 0
";
    assert_build_run("list_slice_e2e_02", src, "10\n30\n30\n20\n10\n");
}

// =====================================================================
// list_slice_e2e_03 — a positive OOB scalar index TRAPS (exit 3), NOT a
// silent `0`. CPython raises IndexError; Cobrust maps a true OOB to an
// unrecoverable runtime TRAP (`crate::panic::panic` → exit 3) naming the
// bad index + length (§2.5-B). The §2.2 guard against the silent-0 hole.
// =====================================================================

#[test]
fn list_slice_e2e_03_positive_oob_traps() {
    let src = "\
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30]
    print(xs[100])
    return 0
";
    assert_build_run_traps(
        "list_slice_e2e_03",
        src,
        "list index out of range: i=100 len=3",
    );
}

// =====================================================================
// list_slice_e2e_04 — a negative-OOB scalar index (`i < -len`) ALSO traps
// (exit 3), NOT a silent `0`. `[10,20,30][-100]` normalizes to
// `3 + (-100) == -97`, still `< 0` → OOB → trap. The OTHER direction of
// the BUG-1 §2.2 hole.
// =====================================================================

#[test]
fn list_slice_e2e_04_negative_oob_traps() {
    let src = "\
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30]
    print(xs[-100])
    return 0
";
    assert_build_run_traps(
        "list_slice_e2e_04",
        src,
        "list index out of range: i=-100 len=3",
    );
}

// =====================================================================
// list_slice_e2e_05 — UNSUPPORTED `list` slice shapes (open-ended
// `xs[1:]`/`xs[:3]`, non-unit step `xs[0:4:2]`, negative bound `xs[1:-1]`)
// are REJECTED at COMPILE TIME (`TypeError::UnsupportedSliceShape`, §2.5-A,
// the str/bytes reject EXTENDED to `Ty::List`) — NOT a silent exit-0
// miscompile / UB (§2.2). Each shape fails the build with the fix-printing
// diagnostic naming the supported `xs[1:len(xs)]` form. (The MIR slice
// guard has the identical defense-in-depth `MirError` backstop.)
// =====================================================================

#[test]
fn list_slice_e2e_05_unsupported_slice_shapes_reject() {
    // Open-ended high bound `xs[1:]`.
    assert_build_rejects(
        "list_slice_e2e_05a",
        "\
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30]
    let ys: list[i64] = xs[1:]
    print(len(ys))
    return 0
",
        "xs[1:len(xs)]",
    );
    // Open-ended low bound `xs[:3]`.
    assert_build_rejects(
        "list_slice_e2e_05b",
        "\
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30]
    let ys: list[i64] = xs[:3]
    print(len(ys))
    return 0
",
        "xs[1:len(xs)]",
    );
    // Non-unit step `xs[0:4:2]`.
    assert_build_rejects(
        "list_slice_e2e_05c",
        "\
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30, 40]
    let ys: list[i64] = xs[0:4:2]
    print(len(ys))
    return 0
",
        "xs[1:len(xs)]",
    );
    // Negative high bound `xs[1:-1]`.
    assert_build_rejects(
        "list_slice_e2e_05d",
        "\
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30]
    let ys: list[i64] = xs[1:-1]
    print(len(ys))
    return 0
",
        "xs[1:len(xs)]",
    );
}

// =====================================================================
// list_slice_e2e_06 — DROP-BALANCE: a slice in a loop mints a fresh list
// each iteration; each must drop exactly once (no leak, no double-free).
// 1000 iterations each slicing + reading. A double-free would abort; a
// leak is invisible to exit code but the run must stay clean exit 0. The
// final print verifies values survive the Move-out drop discipline.
// =====================================================================

#[test]
fn list_slice_e2e_06_slice_drop_balance_in_loop() {
    let src = "\
fn main() -> i64:
    let xs: list[i64] = [10, 20, 30, 40, 50]
    let acc: i64 = 0
    let i: i64 = 0
    while i < 1000:
        let s: list[i64] = xs[1:4]
        acc = acc + s[0] + s[1] + s[2]
        i = i + 1
    print(acc)
    return 0
";
    // each slice is [20,30,40]; sum 90 per iter * 1000 == 90000.
    assert_build_run("list_slice_e2e_06", src, "90000\n");
}
