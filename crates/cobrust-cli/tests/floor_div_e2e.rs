//! ADR-0041 §H1 sibling (F86) end-to-end corpus for `//` integer FLOOR
//! division: `//` rounds toward -∞ (Python `floor`), NOT toward zero (C
//! `sdiv`).
//!
//! ## Why this exists (§2.2 silent-miscompile + div/mod invariant)
//!
//! Before F86, integer `//` shared the truncating `build_int_signed_div`
//! arm with `/` in `cobrust-codegen`, so `-7 // 2` SILENTLY produced `-3`
//! (CPython: `-4`) — a clean-compiling WRONG value in a common op (hashing,
//! grid/index math, time arithmetic). Worse, `%` ALREADY floored (Python
//! floor-mod, ADR-0041 §H1), so the two operators were INCONSISTENT and the
//! load-bearing invariant `(a // b) * b + (a % b) == a` was BROKEN for
//! negatives: `(-7 // 2)*2 + (-7 % 2) = (-3)*2 + 1 = -5 != -7`.
//!
//! F86 splits `FloorDiv` out of the shared int arm and applies the standard
//! trunc→floor correction (subtract 1 when the remainder is non-zero AND
//! the operand signs differ) — the SYMMETRIC twin of the `Mod` adjustment.
//! `/` (Cobrust C-like TRUNCATING integer division — `-7 / 2 == -3`, NOT
//! Python true/float division) is UNCHANGED.
//!
//! ## Semantics (CPython 3 `//`, the oracle)
//!
//! - `-7 // 2 == -4`, `7 // -2 == -4`, `-7 // 3 == -3`, `-8 // 3 == -3`.
//! - positives / exact divisions unchanged: `7 // 2 == 3`, `-6 // 2 == -3`
//!   (exact, no adjust), `-6 // -2 == 3`, `0 // 5 == 0`.
//! - `%` STILL floors (`-7 % 2 == 1`, `7 % -2 == -1`, `-7 % 3 == 2`).
//! - the INVARIANT `(a // b) * b + (a % b) == a` holds for every sign
//!   quadrant (the consistency check this corpus pins).
//! - float `//` floors too (`-7.0 // 2.0 == -4.0`).
//! - division-by-zero stays a TRAP (the MIR-level `Assert(rhs != 0)` guard
//!   is unchanged).
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
// floor_div_e2e_01 — the four NEGATIVE-quotient cases that REGRESSED
// before F86. CPython 3 oracle (FLOOR toward -∞):
//   -7 // 2 == -4 ;  7 // -2 == -4 ;  -7 // 3 == -3 ;  -8 // 3 == -3.
// Before F86 these were the truncating -3, -3, -2, -2 (the silent
// miscompile §2.2).
// =====================================================================

#[test]
fn floor_div_e2e_01_negative_quotient_floors() {
    let src = "\
fn main() -> i64:
    print(-7 // 2)
    print(7 // -2)
    print(-7 // 3)
    print(-8 // 3)
    return 0
";
    assert_build_run("floor_div_e2e_01", src, "-4\n-4\n-3\n-3\n");
}

// =====================================================================
// floor_div_e2e_02 — the cases that were ALREADY correct must STAY
// correct (positives, exact divisions, both-negative, zero dividend).
// CPython 3:  7 // 2 == 3 ;  -6 // 2 == -3 (EXACT, no adjust) ;
//   -6 // -2 == 3 ;  0 // 5 == 0 ;  6 // 2 == 3.
// The exact-division cases (`-6 // 2`) confirm the `rem != 0` guard:
// when the remainder is zero, NO `-1` adjustment is applied even though
// the signs differ.
// =====================================================================

#[test]
fn floor_div_e2e_02_positive_exact_unchanged() {
    let src = "\
fn main() -> i64:
    print(7 // 2)
    print(-6 // 2)
    print(-6 // -2)
    print(0 // 5)
    print(6 // 2)
    return 0
";
    assert_build_run("floor_div_e2e_02", src, "3\n-3\n3\n0\n3\n");
}

// =====================================================================
// floor_div_e2e_03 — `%` is UNCHANGED (still Python floor-mod, ADR-0041
// §H1). CPython 3:  -7 % 2 == 1 ;  7 % -2 == -1 ;  -7 % 3 == 2 ;
//   7 % 2 == 1.  The F86 `//` fix MUST NOT touch the `%` arm.
// =====================================================================

#[test]
fn floor_div_e2e_03_mod_unchanged() {
    let src = "\
fn main() -> i64:
    print(-7 % 2)
    print(7 % -2)
    print(-7 % 3)
    print(7 % 2)
    return 0
";
    assert_build_run("floor_div_e2e_03", src, "1\n-1\n2\n1\n");
}

// =====================================================================
// floor_div_e2e_04 — the LOAD-BEARING div/mod invariant
//   (a // b) * b + (a % b) == a
// holds for every sign quadrant. This is the consistency check F86
// restores: before the fix, `(-7 // 2)*2 + (-7 % 2) = -5 != -7`.
// Asserts the reconstruction equals the original dividend exactly.
// =====================================================================

#[test]
fn floor_div_e2e_04_div_mod_invariant() {
    let src = "\
fn main() -> i64:
    let a: i64 = -7
    let b: i64 = 2
    print((a // b) * b + (a % b))
    let c: i64 = 7
    let d: i64 = -2
    print((c // d) * d + (c % d))
    let e: i64 = -8
    let f: i64 = 3
    print((e // f) * f + (e % f))
    let g: i64 = -6
    let h: i64 = -2
    print((g // h) * h + (g % h))
    return 0
";
    // Reconstruction must return the original dividend: -7, 7, -8, -6.
    assert_build_run("floor_div_e2e_04", src, "-7\n7\n-8\n-6\n");
}

// =====================================================================
// floor_div_e2e_05 — `/` (Cobrust C-like TRUNCATING integer division)
// is UNCHANGED. `/` on integers is NOT Python true/float division in
// Cobrust (`int / int -> int`); it TRUNCATES toward zero. CPython
// `math.trunc(-7/2) == -3`. This pins that F86 fixed ONLY `//` and left
// `/` truncating (the pre-existing Cobrust semantics, e.g. `7 / 2 == 3`).
// =====================================================================

#[test]
fn floor_div_e2e_05_slash_stays_truncating() {
    let src = "\
fn main() -> i64:
    print(-7 / 2)
    print(7 / -2)
    print(7 / 2)
    print(-6 / 2)
    return 0
";
    // TRUNCATE toward zero: -3, -3, 3, -3 (NOT the floored -4, -4).
    assert_build_run("floor_div_e2e_05", src, "-3\n-3\n3\n-3\n");
}

// =====================================================================
// floor_div_e2e_06 — float `//` FLOORS too (CPython `-7.0 // 2.0 ==
// -4.0`). The float-print path is a separate limitation, so assert via
// equality comparison: each `1` means the float floor-division matched
// the CPython oracle.
//   -7.0 // 2.0 == -4.0 ;  7.0 // -2.0 == -4.0 ;  7.5 // 2.0 == 3.0 ;
//   -7.5 // 2.0 == -4.0 ;  6.0 // 2.0 == 3.0 (exact).
// =====================================================================

#[test]
fn floor_div_e2e_06_float_floors() {
    let src = "\
fn main() -> i64:
    if (-7.0 // 2.0) == -4.0:
        print(1)
    else:
        print(0)
    if (7.0 // -2.0) == -4.0:
        print(1)
    else:
        print(0)
    if (7.5 // 2.0) == 3.0:
        print(1)
    else:
        print(0)
    if (-7.5 // 2.0) == -4.0:
        print(1)
    else:
        print(0)
    if (6.0 // 2.0) == 3.0:
        print(1)
    else:
        print(0)
    return 0
";
    assert_build_run("floor_div_e2e_06", src, "1\n1\n1\n1\n1\n");
}

// =====================================================================
// floor_div_e2e_07 — `//` by zero still TRAPS (the MIR-level
// `Assert(rhs != 0)` guard at lower.rs is UNCHANGED by F86). A non-zero
// exit code (abort / panic, NOT 0) is the trap signal — F86 must not
// have weakened the div-by-zero guard while adding the floor adjustment.
// =====================================================================

#[test]
fn floor_div_e2e_07_div_by_zero_traps() {
    let src = "\
fn main() -> i64:
    let z: i64 = 0
    print(7 // z)
    return 0
";
    let path = write_cb("floor_div_e2e_07", src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "floor_div_e2e_07: build should succeed (the trap is at RUNTIME); \
         stderr=\n{build_stderr}"
    );
    let (run_code, _stdout, _run_stderr) = run_exe(&exe);
    assert_ne!(
        run_code, 0,
        "floor_div_e2e_07: `7 // 0` must TRAP at runtime (non-zero exit), \
         not silently produce a value"
    );
}
