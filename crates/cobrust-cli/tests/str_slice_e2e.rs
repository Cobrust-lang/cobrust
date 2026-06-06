//! ADR-0094 / F78 end-to-end corpus for the `str` index OPERATOR surface:
//! `s[i]` (scalar) + `s[lo:hi]` (slice), CODEPOINT-addressed.
//!
//! ## The F78 bug this closes
//!
//! Before ADR-0094, a `str` index expression SILENTLY evaluated to the
//! WHOLE base string — a §2.2 silent-miscompile in a core op:
//!
//! - `print("hello"[1:4])` built + ran exit 0 + printed `hello` (CPython
//!   `ell`).
//! - `print("hello"[1])` likewise printed `hello` (CPython `e`).
//! - `len("hello"[1:4])` was a use-of-moved-value compile error.
//!
//! The generic `ExprKind::Index` lowering fell through to the
//! `Projection::Index` no-op for `Ty::Str` (no `__cobrust_str_slice` /
//! `__cobrust_str_char_at` runtime existed). ADR-0094 mirrors the `bytes`
//! Phase-2 slice machinery (ADR-0093 §2) for `str`:
//!
//! - `s[lo:hi]` → a fresh `str` (`__cobrust_str_slice`, the
//!   `__cobrust_bytes_slice` mirror — but **codepoint-addressed**, Python
//!   clamp on OOB).
//! - `s[i]` → a fresh 1-codepoint `str` (`__cobrust_str_char_at`).
//! - the open-ended / stepped / negative shapes REJECT at compile time
//!   (`TypeError::UnsupportedSliceShape`, the ADR-0093 `bytes` reject
//!   EXTENDED to `Ty::Str`) — §2.5-A, NEVER a silent miscompile.
//!
//! ## The CODEPOINT decision (the load-bearing str-vs-bytes difference)
//!
//! Python `str[i]` / `str[i:j]` index by Unicode scalar — a slice NEVER
//! splits a multi-byte UTF-8 codepoint. `bytes` had no such concern
//! (every byte is independent). The `.cb` type contract already declares
//! `str[i] -> str` (a 1-codepoint string), so both forms walk
//! `char_indices()` and address CODEPOINT offsets. A slice boundary
//! always lands on a `char` boundary by construction → the result is
//! ALWAYS valid UTF-8, no trap needed.
//!
//! Each test writes a `.cb` program, invokes `cobrust build`, runs the
//! produced executable, and asserts stdout byte-identical to the
//! CPython-3 oracle (e.g. `"hello"[1:4] == "ell"`, `"héllo"[1:3] ==
//! "él"`).
//!
//! Per `feedback_p9_clippy_stall_pattern.md`: module-level test-only
//! lint allow header.

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
/// not a silent exit-0 miscompile (the F78 hole this closes).
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

// =====================================================================
// str_slice_e2e_01 — `s[lo:hi]` slice (THE F78 FIX). CPython 3 oracle:
//   "hello"[1:4] == "ell"  (len 3)
//   "hello"[1:99] == "ello"  (Python clamps hi to len, NOT an abort)
//   "hello"[3:1] == ""       (hi <= lo → empty)
//   "hello"[0:5] == "hello"  (full span)
// Before the fix, EVERY case silently evaluated to "hello" (the whole
// string) at exit 0. The slice mints a FRESH str the `.cb` scope drops
// once.
// =====================================================================

#[test]
fn str_slice_e2e_01_slice_basic_and_clamp() {
    let src = "\
fn main() -> i64:
    let s: str = \"hello\"
    print(s[1:4])
    print(s[1:99])
    print(s[3:1])
    print(s[0:5])
    print(len(s[1:4]))
    return 0
";
    // "ell", "ello", "" (blank line), "hello", 3
    assert_build_run("str_slice_e2e_01", src, "ell\nello\n\nhello\n3\n");
}

// =====================================================================
// str_slice_e2e_02 — `s[i]` SCALAR index (the F78 sibling — `s[i]` was
// ALSO silently the whole string). CPython 3: "hello"[0]=='h',
// "hello"[1]=='e', "hello"[4]=='o'. A 1-codepoint str, NOT a byte
// (contrast `bytes`' `b[i] -> int`).
// =====================================================================

#[test]
fn str_slice_e2e_02_scalar_index() {
    let src = "\
fn main() -> i64:
    let s: str = \"hello\"
    print(s[0])
    print(s[1])
    print(s[4])
    return 0
";
    assert_build_run("str_slice_e2e_02", src, "h\ne\no\n");
}

// =====================================================================
// str_slice_e2e_03 — the LOAD-BEARING CODEPOINT case. CPython 3:
//   "héllo"[1:3] == "él"   ('é' is 2 UTF-8 bytes; a BYTE slicer would
//                            split it / mis-cut — codepoint slicing does
//                            not)
//   "héllo"[1]   == "é"    (codepoint scalar)
//   "你好世界"[1:3] == "好世"   (all multi-byte)
//   "😀a😀b"[0:3]  == "😀a😀"  (4-byte codepoints, never invalid UTF-8)
// =====================================================================

#[test]
fn str_slice_e2e_03_codepoint_not_byte() {
    let src = "\
fn main() -> i64:
    let u: str = \"héllo\"
    print(u[1:3])
    print(u[1])
    print(u[0:2])
    let z: str = \"你好世界\"
    print(z[1:3])
    let e: str = \"😀a😀b\"
    print(e[0:3])
    return 0
";
    assert_build_run("str_slice_e2e_03", src, "él\né\nhé\n好世\n😀a😀\n");
}

// =====================================================================
// str_slice_e2e_04 — UNSUPPORTED `str` slice shapes (open-ended `s[1:]`/
// `s[:3]`, non-unit step `s[0:4:2]`, negative bound `s[1:-1]`) are
// REJECTED at COMPILE TIME (`TypeError::UnsupportedSliceShape`, §2.5-A,
// the ADR-0093 `bytes` reject EXTENDED to `Ty::Str`) — NOT a silent
// exit-0 whole-string miscompile (§2.2). BEFORE the fix, `"hello"[1:]`
// built + ran exit 0 + printed `hello`. Each shape now fails the build
// with the fix-printing diagnostic naming the supported `s[1:len(s)]`
// form. (The MIR slice guard has the identical defense-in-depth
// `MirError` backstop.)
// =====================================================================

#[test]
fn str_slice_e2e_04_unsupported_slice_shapes_reject() {
    // Open-ended high bound `s[1:]`.
    assert_build_rejects(
        "str_slice_e2e_04a",
        "\
fn main() -> i64:
    let s: str = \"hello\"
    let x: str = s[1:]
    print(len(x))
    return 0
",
        "s[1:len(s)]",
    );
    // Open-ended low bound `s[:3]`.
    assert_build_rejects(
        "str_slice_e2e_04b",
        "\
fn main() -> i64:
    let s: str = \"hello\"
    let x: str = s[:3]
    print(len(x))
    return 0
",
        "s[1:len(s)]",
    );
    // Non-unit step `s[0:4:2]`.
    assert_build_rejects(
        "str_slice_e2e_04c",
        "\
fn main() -> i64:
    let s: str = \"hello\"
    let x: str = s[0:4:2]
    print(len(x))
    return 0
",
        "s[1:len(s)]",
    );
    // Negative bound `s[1:-1]` (CPython "ell", but unsupported → reject).
    assert_build_rejects(
        "str_slice_e2e_04d",
        "\
fn main() -> i64:
    let s: str = \"hello\"
    let x: str = s[1:-1]
    print(len(x))
    return 0
",
        "s[1:len(s)]",
    );
}

// =====================================================================
// str_slice_e2e_05 — DROP-HAMMER loop. 200 iterations each minting a
// fresh slice + a fresh scalar char, accumulating their lengths. A
// double-free / leak in the mint-once / borrow-base discipline would
// crash or diverge. The base `str` is BORROWED (survives all iterations).
// CPython oracle: per iteration "hello"[1:4]="ell" (len 3) + "hello"[1]
// ="e" (len 1) → 4 chars; loop sum printed at the end.
// =====================================================================

#[test]
fn str_slice_e2e_05_drop_hammer_loop() {
    let src = "\
fn main() -> i64:
    let s: str = \"hello\"
    let total: i64 = 0
    let i: i64 = 0
    while i < 200:
        let a: str = s[1:4]
        total = total + len(a)
        let c: str = s[1]
        total = total + len(c)
        i = i + 1
    print(total)
    print(s)
    return 0
";
    // 200 * (3 + 1) = 800; base "hello" still usable after the loop.
    assert_build_run("str_slice_e2e_05", src, "800\nhello\n");
}

// =====================================================================
// str_slice_e2e_06 — the base str is BORROWED, not consumed: a source
// `str` survives a slice AND a scalar index and is STILL usable
// afterward (drops once at scope exit). The Move→Copy upgrade on the
// base handle (mirrors `bytes_ops_e2e_07`'s borrowed-survives pattern:
// the slice/scalar reads do NOT consume the base, so subsequent index
// reads on the SAME base succeed). If the slice consumed its input, the
// second use would use-after-free or fail the borrow check.
//
// NOTE: this exercises the SLICE/SCALAR index borrow specifically — it
// deliberately avoids chaining multiple consuming `len(s)`/`print(s)`
// reads on a bare `str` name, which is an ORTHOGONAL pre-existing
// borrow-checker limitation (a bare `str` name read is `Operand::Move`,
// so two of them trip `UseAfterMove`; that is the Phase-G `&s` explicit-
// borrow surface's job, NOT F78's). The index OPERATOR base read is the
// one this ADR makes a non-consuming `Operand::Copy`.
// =====================================================================

#[test]
fn str_slice_e2e_06_base_borrowed_not_consumed() {
    let src = "\
fn main() -> i64:
    let s: str = \"hello\"
    let mid: str = s[1:4]
    print(mid)
    print(s[0])
    print(s[1:3])
    print(s[4])
    print(s[0:5])
    return 0
";
    // The base `s` survives the slice + 4 further index reads (each a
    // non-consuming `Operand::Copy` base read).
    // "ell", 'h', "el", 'o', "hello".
    assert_build_run("str_slice_e2e_06", src, "ell\nh\nel\no\nhello\n");
}
