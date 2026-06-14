//! F92 / ADR-0104 end-to-end corpus for `str` ORDERING comparison
//! (`<`, `<=`, `>`, `>=`) — lexicographic, like Python.
//!
//! ## Why this exists (§2.5 LLM-first + §5.1 no-panic)
//!
//! Before F92, `"abc" < "abd"` (and `>`, `<=`, `>=`) CRASHED the
//! `cobrust build` compiler with a codegen panic (build exit 101). The
//! type checker ACCEPTS `str < str` (`unify(Str, Str)` succeeds →
//! `Ty::Bool`), exactly as it accepts the already-working `str == str`,
//! so the program type-checks and then ICEs in `lower_binop` — which
//! assumed integer/float operands (`into_int_value()`) and had NO arm for
//! the opaque `Str` POINTER operand (the F85/F87 codegen-panic class, a
//! §5.1 "compiler must not panic on type-checked input" violation).
//!
//! An LLM agent writes `s1 < s2` constantly (sorting, ordering, binary
//! search), and Python performs lexicographic str comparison — so the
//! §2.5 LLM-first fix is to IMPLEMENT it (not reject it). F92 retargets
//! the four ordering ops in MIR lowering to the always-linked
//! `__cobrust_str_cmp(a, b) -> i64` (sign of `a.cmp(b)`: -1/0/+1), then
//! materialises the bool by comparing that i64 against 0 with the SAME
//! ordering op — a direct sibling of the `str == str` retarget.
//!
//! ## Semantics (ADR-0104; CPython 3 str comparison is the oracle)
//!
//! Python compares str lexicographically by CODEPOINT. Rust `str` `Ord`
//! is BYTE-lexicographic over the UTF-8 encoding; UTF-8 is order-
//! preserving, so byte order equals codepoint order for valid UTF-8 —
//! `a.cmp(b)` yields the SAME result as CPython (confirmed in ADR-0104).
//! A shorter string that is a prefix of a longer one is LESS
//! (`"ab" < "abc"`), matching CPython.
//!
//! Each test writes a `.cb` program, invokes `cobrust build`, runs the
//! produced executable, and asserts stdout byte-identical to the CPython-3
//! oracle (or asserts the build exit code for the reject cases).
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
        "{name}: build failed (regression — F92 str comparison must NOT panic); \
         stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    let (run_code, stdout, run_stderr) = run_exe(&exe);
    assert_eq!(run_code, 0, "{name}: run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, expected_stdout,
        "{name}: stdout mismatch (CPython-3 oracle)\nstderr={run_stderr}"
    );
}

// =====================================================================
// str_cmp_e2e_01 — the four ordering ops on literals match CPython 3.
// CPython oracle:
//   "abc" < "abd" == True  ; "abc" > "abd" == False
//   "a"   <= "a"  == True  ; "b"   >= "a"  == True
// This is the EXACT crash repro from the F92 finding — pre-fix each of
// these `cobrust build`-exit-101'd (codegen panic). Now they build, run,
// and print the CPython truth value.
// =====================================================================

#[test]
fn str_cmp_e2e_01_four_ordering_ops_match_cpython() {
    let src = "\
fn main() -> i64:
    print(\"abc\" < \"abd\")
    print(\"abc\" > \"abd\")
    print(\"a\" <= \"a\")
    print(\"b\" >= \"a\")
    return 0
";
    assert_build_run("str_cmp_e2e_01", src, "True\nFalse\nTrue\nTrue\n");
}

// =====================================================================
// str_cmp_e2e_02 — PREFIX ordering: a string that is a prefix of another
// is LESS than it (CPython lexicographic rule). `"ab" < "abc" == True`
// (prefix is less), `"abc" < "ab" == False`, and the empty string is the
// minimum `"" < "a" == True`.
// =====================================================================

#[test]
fn str_cmp_e2e_02_prefix_and_empty_ordering() {
    let src = "\
fn main() -> i64:
    print(\"ab\" < \"abc\")
    print(\"abc\" < \"ab\")
    print(\"\" < \"a\")
    return 0
";
    assert_build_run("str_cmp_e2e_02", src, "True\nFalse\nTrue\n");
}

// =====================================================================
// str_cmp_e2e_03 — EQUAL strings under the inclusive ops: `"x" <= "x"`
// and `"x" >= "x"` are BOTH True (equality satisfies `<=` and `>=`),
// while the strict `"x" < "x"` / `"x" > "x"` are both False. Pins the
// `cmp == 0` boundary (the i64 0 from `__cobrust_str_cmp` flowing into
// SLE/SGE/SLT/SGT against 0).
// =====================================================================

#[test]
fn str_cmp_e2e_03_equal_strings_inclusive_vs_strict() {
    let src = "\
fn main() -> i64:
    print(\"x\" <= \"x\")
    print(\"x\" >= \"x\")
    print(\"x\" < \"x\")
    print(\"x\" > \"x\")
    return 0
";
    assert_build_run("str_cmp_e2e_03", src, "True\nTrue\nFalse\nFalse\n");
}

// =====================================================================
// str_cmp_e2e_04 — UNICODE codepoint ordering. `"é"` (U+00E9) sorts AFTER
// ASCII `"f"` (U+0066) by codepoint, so `"é" < "f" == False` and
// `"é" > "f" == True`. This confirms the Rust `str::cmp` BYTE-lexicographic
// order equals CPython's CODEPOINT order (UTF-8 is order-preserving —
// ADR-0104). Also a multi-codepoint case `"abc" < "abé"` (the differing
// position is non-ASCII).
// =====================================================================

#[test]
fn str_cmp_e2e_04_unicode_codepoint_ordering() {
    let src = "\
fn main() -> i64:
    print(\"é\" < \"f\")
    print(\"é\" > \"f\")
    print(\"abc\" < \"abé\")
    return 0
";
    // CPython: ord('é')==233 > ord('f')==102, so "é" < "f" is False,
    // "é" > "f" is True; "abc" < "abé" compares 'c'(99) < 'é'(233) -> True.
    assert_build_run("str_cmp_e2e_04", src, "False\nTrue\nTrue\n");
}

// =====================================================================
// str_cmp_e2e_05 — usable in an `if` CONDITION and over str VARIABLES
// (not just literals). Exercises the `str` LOCAL operand path (the
// `lower_expr` → `upgrade_move_to_copy_handle` borrow discipline), the
// surface an LLM writes when sorting / branching on names.
// =====================================================================

#[test]
fn str_cmp_e2e_05_str_variables_in_if_condition() {
    let src = "\
fn main() -> i64:
    let a: str = \"apple\"
    let b: str = \"banana\"
    if a < b:
        print(\"a before b\")
    else:
        print(\"b before a\")
    if b > a:
        print(\"b after a\")
    return 0
";
    assert_build_run("str_cmp_e2e_05", src, "a before b\nb after a\n");
}

// =====================================================================
// str_cmp_e2e_06 — REGRESSION: numeric `<`/`>`/`<=`/`>=` are UNCHANGED by
// the F92 str retarget (the str arm guards on `Ty::Str`, so int operands
// fall straight through to the existing integer comparison path).
// `3 < 4 == True`, `5 > 2 == True`, `4 <= 4 == True`, `2 >= 5 == False`.
// =====================================================================

#[test]
fn str_cmp_e2e_06_numeric_comparison_unchanged() {
    let src = "\
fn main() -> i64:
    print(3 < 4)
    print(5 > 2)
    print(4 <= 4)
    print(2 >= 5)
    return 0
";
    assert_build_run("str_cmp_e2e_06", src, "True\nTrue\nTrue\nFalse\n");
}

// =====================================================================
// str_cmp_e2e_07 — REGRESSION: str `==`/`!=` (the already-working ADR-0078
// path) are UNCHANGED. F92 adds the ordering arm BELOW the equality arm in
// MIR lowering, so `==`/`!=` still route through `__cobrust_str_eq`.
// `"abc" == "abc" == True`, `"abc" != "abd" == True`, `"a" == "b" == False`.
// =====================================================================

#[test]
fn str_cmp_e2e_07_str_equality_unchanged() {
    let src = "\
fn main() -> i64:
    print(\"abc\" == \"abc\")
    print(\"abc\" != \"abd\")
    print(\"a\" == \"b\")
    return 0
";
    assert_build_run("str_cmp_e2e_07", src, "True\nTrue\nFalse\n");
}

// =====================================================================
// str_cmp_e2e_08 — NEGATIVE: a MIXED `str < int` comparison REJECTS
// CLEANLY at compile time (exit 2), NOT a codegen panic (exit 101). The
// type checker's comparison arm `unify(Str, Int)` fails and emits a Type
// diagnostic. This is the §5.1 / §2.5 guard: an ill-typed comparison must
// be a clean reject, never a crash.
// =====================================================================

#[test]
fn str_cmp_e2e_08_mixed_str_int_rejects_cleanly() {
    let path = write_cb(
        "str_cmp_e2e_08",
        "\
fn main() -> i64:
    print(\"abc\" < 5)
    return 0
",
    );
    let (build_code, _exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 2,
        "str_cmp_e2e_08: `\"abc\" < 5` (mixed str/int) must REJECT at compile with \
         exit 2 (a clean Type error), NOT panic at codegen (exit 101); \
         stderr=\n{build_stderr}"
    );
    assert!(
        build_stderr.contains("type mismatch") || build_stderr.contains("expected"),
        "str_cmp_e2e_08: reject must be a Type diagnostic; stderr=\n{build_stderr}"
    );
}

// =====================================================================
// str_cmp_e2e_09 — NEGATIVE: `bytes` ordering still REJECTS CLEANLY (exit
// 2) with a fix-printing diagnostic (ADR-0093 deferral), NOT a panic.
// F92 scopes to `str`; `bytes` lexicographic comparison remains an
// ADR-0093 follow-up, but the existing reject (§2.5-B, prints the FIX)
// is confirmed here to NOT be a codegen crash.
// =====================================================================

#[test]
fn str_cmp_e2e_09_bytes_ordering_rejects_cleanly() {
    let path = write_cb(
        "str_cmp_e2e_09",
        "\
fn main() -> i64:
    print(b\"abc\" < b\"abd\")
    return 0
",
    );
    let (build_code, _exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 2,
        "str_cmp_e2e_09: `bytes < bytes` must REJECT at compile with exit 2 (clean \
         diagnostic), NOT panic at codegen (exit 101); stderr=\n{build_stderr}"
    );
    assert!(
        build_stderr.contains("bytes"),
        "str_cmp_e2e_09: reject must name `bytes`; stderr=\n{build_stderr}"
    );
}
