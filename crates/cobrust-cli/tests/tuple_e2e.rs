//! F83 / ADR-0106 end-to-end corpus for the TUPLE index OPERATOR surface:
//! `t[i]` (scalar, constant-index, Python-negative-indexed) on a
//! HETEROGENEOUS tuple. The TUPLE analogue of the `str`/`bytes`/`list`
//! indexing arc (`str_slice_e2e` F78/F79, `bytes_ops_e2e`,
//! `list_slice_e2e` F81) — this COMPLETES the indexing-correctness arc
//! across all four sequence types.
//!
//! ## The F83 §2.2 bug this closes
//!
//! `(7, "x")[0]` SILENT-0 MISCOMPILE. The program BUILT OK + RAN returning
//! `0` (CPython `7`). Two stub layers caused it:
//!   - MIR `lower_index` returned a `Constant::Int(0)` STUB for a tuple
//!     index, and the `ExprKind::Index` rvalue lowering had NO tuple branch
//!     — the read fell through to the generic `Projection::Index` no-op.
//!   - The LLVM backend lowered a `Ty::Tuple` to an opaque-pointer NULL stub
//!     (both construction AND `Projection::Field` reads were unimplemented).
//!
//! Fixed (mirror the EXISTING `Projection::Field` discipline used by tuple
//! CONSTRUCTION + let-destructure): a tuple now lowers to a REAL LLVM struct
//! VALUE; construction builds it via `build_insert_value`; a `t[i]` read with
//! a CONSTANT index (Python-negative normalised against the static arity)
//! reads `Projection::Field(off)` via `build_extract_value` as the EXACT
//! per-position element type the checker resolved (a tuple is heterogeneous:
//! `(i64, str)[0]` is `i64`, `[1]` is `str`).
//!
//! ## §2.5-A compile-time-catch (NOT a silent miscompile)
//!
//! A tuple's element type is only knowable for a CONSTANT index, so the
//! checker REJECTS (`TypeError::NotIndexable`):
//!   - a NON-LITERAL index (`t[i]`) — a dynamic position has no single static
//!     type for a heterogeneous tuple (the prior head-element fallback was a
//!     §2.2 silent miscompile for a mixed-type tuple).
//!   - a constant OOB index (`(1,2)[5]`, `(1,2)[-5]`) — like the array-OOB
//!     literal reject + the `t.N` tuple-field OOB reject.
//!
//! Each test writes a `.cb` program, invokes `cobrust build`, runs the
//! produced executable, and asserts stdout byte-identical to the CPython-3
//! oracle (e.g. `(7, "x")[0] == 7`, `(1, "a", 2)[2] == 2`, `(1, 2)[-1] == 2`).
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

/// Assert a `.cb` program is REJECTED at compile time (non-zero build exit)
/// and the diagnostic on stderr CONTAINS `needle` (the §2.5-B fix-printing
/// substring). A non-zero exit proves a Cobrust DIAGNOSTIC, not a silent
/// exit-0 miscompile (the F83 SILENT-0 hole this closes).
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
// tuple_e2e_01 — THE F83 FIX: `(7, "x")[0] == 7` (NOT the silent `0`).
// A 2-tuple's first field, read both as a tuple literal AND via a binding.
// CPython 3: (7, "x")[0] == 7.
// =====================================================================

#[test]
fn tuple_e2e_01_silent_zero_fix() {
    let src = "\
fn main() -> i64:
    let t: (i64, str) = (7, \"x\")
    print(t[0])
    print((7, \"x\")[0])
    return 0
";
    // BEFORE the fix both printed `0`; CPython oracle is `7`.
    assert_build_run("tuple_e2e_01", src, "7\n7\n");
}

// =====================================================================
// tuple_e2e_02 — a 3+ element tuple read at EACH position. CPython 3:
// (10, 20, 30)[0]==10, [1]==20, [2]==30.
// =====================================================================

#[test]
fn tuple_e2e_02_each_index() {
    let src = "\
fn main() -> i64:
    let t: (i64, i64, i64) = (10, 20, 30)
    print(t[0])
    print(t[1])
    print(t[2])
    return 0
";
    assert_build_run("tuple_e2e_02", src, "10\n20\n30\n");
}

// =====================================================================
// tuple_e2e_03 — MIXED-TYPE tuple `(1, "a", 2)`. Proves PER-POSITION element
// typing (NOT head-type): position 0/2 are `i64`, position 1 is `str`. The
// prior dynamic-index head-element fallback would mis-type field 2 as the
// head `i64` (fine here) but the SILENT-0 stub mis-COMPILED every field.
// CPython 3: (1, "a", 2)[0]==1, [1]=="a", [2]==2.
// =====================================================================

#[test]
fn tuple_e2e_03_mixed_type_per_position() {
    let src = "\
fn main() -> i64:
    let t: (i64, str, i64) = (1, \"a\", 2)
    print(t[0])
    print(t[1])
    print(t[2])
    return 0
";
    assert_build_run("tuple_e2e_03", src, "1\na\n2\n");
}

// =====================================================================
// tuple_e2e_04 — arithmetic on read fields `t[0] + t[2]`. Proves the
// extracted i64 fields are real integers (not opaque pointers). CPython 3:
// (1, "a", 2)[0] + (1, "a", 2)[2] == 3.
// =====================================================================

#[test]
fn tuple_e2e_04_field_arithmetic() {
    let src = "\
fn main() -> i64:
    let t: (i64, str, i64) = (1, \"a\", 2)
    print(t[0] + t[2])
    return 0
";
    assert_build_run("tuple_e2e_04", src, "3\n");
}

// =====================================================================
// tuple_e2e_05 — Python-NEGATIVE constant index `t[-1]` → LAST element
// (normalised `arity + i` against the static arity, a COMPILE-TIME
// constant-fold). CPython 3: (10, 20, 30)[-1]==30, [-2]==20, [-3]==10.
// =====================================================================

#[test]
fn tuple_e2e_05_negative_index() {
    let src = "\
fn main() -> i64:
    let t: (i64, i64, i64) = (10, 20, 30)
    print(t[-1])
    print(t[-2])
    print(t[-3])
    return 0
";
    assert_build_run("tuple_e2e_05", src, "30\n20\n10\n");
}

// =====================================================================
// tuple_e2e_06 — Python-negative index on a MIXED tuple, last element is a
// `str`. CPython 3: (7, "x")[-1]=="x", [-2]==7.
// =====================================================================

#[test]
fn tuple_e2e_06_negative_index_str_last() {
    let src = "\
fn main() -> i64:
    let t: (i64, str) = (7, \"x\")
    print(t[-1])
    print(t[-2])
    return 0
";
    assert_build_run("tuple_e2e_06", src, "x\n7\n");
}

// =====================================================================
// tuple_e2e_07 — DROP discipline: a tuple containing an OWNED str. Reading
// the OTHER (int) field must NOT corrupt / double-free the owned str field;
// reading the str field repeatedly must not double-free. HONEST SCOPE: the
// tuple drop is currently a codegen NO-OP, so an unread owned field LEAKS
// (never double-frees) — a memory-safe leak-or-free-once gap documented in
// ADR-0106 + finding F83 (and the F82-class loop-body debt). This test
// proves NO double-free + value correctness ONLY; it does NOT verify the
// field is freed (a leak satisfies exit-0). CPython 3:
// (s, 99)[1]==99, [0]=="hello"; reading [0] three times prints "hello" each.
// =====================================================================

#[test]
fn tuple_e2e_07_owned_field_drop_no_double_free() {
    let src = "\
fn main() -> i64:
    let s: str = \"hello\"
    let t: (str, i64) = (s, 99)
    print(t[1])
    print(t[0])
    print(t[0])
    print(t[0])
    return 0
";
    // Exit 0 (no double-free crash) + byte-identical output proves the
    // owned-str field read is a sound borrow.
    assert_build_run("tuple_e2e_07", src, "99\nhello\nhello\nhello\n");
}

// =====================================================================
// tuple_e2e_08 — a CONSTANT OOB index `(1, 2)[5]` REJECTS at compile time
// (§2.5-A compile-time-catch), NOT a silent read. CPython raises IndexError
// at runtime; Cobrust catches it EARLIER (the index is a compile-time
// constant against the static arity).
// =====================================================================

#[test]
fn tuple_e2e_08_positive_oob_rejects() {
    let src = "\
fn main() -> i64:
    let t: (i64, i64) = (1, 2)
    print(t[5])
    return 0
";
    assert_build_rejects("tuple_e2e_08", src, "out of bounds");
}

// =====================================================================
// tuple_e2e_09 — a NEGATIVE constant OOB index `(1, 2)[-5]` REJECTS at
// compile time (normalised `-5 + 2 = -3 < 0`).
// =====================================================================

#[test]
fn tuple_e2e_09_negative_oob_rejects() {
    let src = "\
fn main() -> i64:
    let t: (i64, i64) = (1, 2)
    print(t[-5])
    return 0
";
    assert_build_rejects("tuple_e2e_09", src, "out of bounds");
}

// =====================================================================
// tuple_e2e_10 — a NON-LITERAL (dynamic) tuple index `t[i]` REJECTS at
// compile time: a heterogeneous tuple's element type is unknown for a
// dynamic position (§2.5-A — surface it at check time, not a runtime
// head-element miscompile). The §2.5-B diagnostic names the FIX.
// =====================================================================

#[test]
fn tuple_e2e_10_dynamic_index_rejects() {
    let src = "\
fn main() -> i64:
    let t: (i64, i64) = (1, 2)
    let i: i64 = 1
    print(t[i])
    return 0
";
    assert_build_rejects("tuple_e2e_10", src, "CONSTANT integer index");
}
