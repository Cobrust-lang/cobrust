//! F90 / ADR-0102 end-to-end corpus for the `**` POWER operator.
//!
//! ## Why this exists (§2.5 LLM-first — `**` is ubiquitous in Python)
//!
//! Before F90, `2 ** 3` REJECTED at codegen with an "unimplemented"
//! diagnostic (ADR-0041 §H3): `**` is one of the most common Python
//! operators an LLM agent writes, so its absence was a constant first-try
//! failure (§2.5 maximize-training-data-overlap). This was an ADDITIVE gap
//! (a clean reject, NOT a silent miscompile), so F90 simply wires the
//! operator through.
//!
//! ## Semantics (ADR-0102; CPython 3 `**` is the oracle)
//!
//! Python's `**` result type depends on the exponent SIGN at RUNTIME
//! (`2 ** 3 == 8` an int, `2 ** -1 == 0.5` a float). A static type system
//! cannot make `int ** int` be both, so Cobrust PINS the typed result by
//! the operand types:
//!
//! - `int ** int -> int` (i64). `base ** 0 == 1` (incl. `0 ** 0 == 1`),
//!   `base ** 1 == base`, matching CPython.
//!   - A NEGATIVE-LITERAL exponent (`2 ** -1`) is REJECTED at compile time
//!     (§2.5-A — a negative power is a non-integer; mirrors F79's
//!     negative-literal scalar-index reject), exit 2.
//!   - Integer OVERFLOW (`2 ** 63`) TRAPS (`checked_pow` → panic → exit 3),
//!     NOT a silent wrap (Constitution §2.2).
//!   - A runtime-DYNAMIC negative exponent (a variable) TRAPS at runtime
//!     (exit 3) — the type checker cannot catch the non-literal case.
//! - ANY float operand `-> f64` (`float ** float`, `int ** float`,
//!   `float ** int`), via libm `pow` (`__cobrust_math_pow`). This is the
//!   ONE arithmetic op that PROMOTES a mixed int/float pair (the float
//!   exponent makes the result a float unambiguously); `+`/`-`/`*`/`/` do
//!   NOT promote (Cobrust has no implicit numeric coercion, §2.2).
//!
//! NOTE on the float-print surface (`__cobrust_println_float`): integer-
//! valued floats print WITHOUT a `.0` (`2.0 ** 3.0` -> `8`, not `8.0`);
//! this is the cobrust println_float repr, NOT CPython's `8.0`.
//!
//! Each test writes a `.cb` program, invokes `cobrust build`, runs the
//! produced executable, and asserts stdout byte-identical to the CPython-3
//! oracle (or asserts the build/run exit code for the reject/trap cases).
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
// power_e2e_01 — integer `**` matches CPython 3 across the canonical
// cases. CPython oracle:
//   2 ** 10 == 1024 ; 2 ** 0 == 1 ; 0 ** 0 == 1 ; 5 ** 1 == 5 ;
//   3 ** 3 == 27 ; 10 ** 3 == 1000.
// `base ** 0 == 1` (incl. `0 ** 0 == 1`) and `base ** 1 == base` are the
// CPython identities `checked_pow` preserves (ADR-0102).
// =====================================================================

#[test]
fn power_e2e_01_integer_pow_matches_cpython() {
    let src = "\
fn main() -> i64:
    print(2 ** 10)
    print(2 ** 0)
    print(0 ** 0)
    print(5 ** 1)
    print(3 ** 3)
    print(10 ** 3)
    return 0
";
    assert_build_run("power_e2e_01", src, "1024\n1\n1\n5\n27\n1000\n");
}

// =====================================================================
// power_e2e_02 — `*` is NOT confused with `**` (the REGRESSION guard the
// spec calls out). `2 * 3 == 6` (multiply), NOT `8` (`2 ** 3`). Also pins
// the other numeric ops `+`/`-`/`/`/`//` UNCHANGED alongside.
// =====================================================================

#[test]
fn power_e2e_02_star_not_confused_with_starstar() {
    let src = "\
fn main() -> i64:
    print(2 * 3)
    print(2 + 3)
    print(7 - 3)
    print(7 / 2)
    print(7 // 2)
    return 0
";
    // multiply=6 (NOT pow 8), add=5, sub=4, truncdiv=3, floordiv=3.
    assert_build_run("power_e2e_02", src, "6\n5\n4\n3\n3\n");
}

// =====================================================================
// power_e2e_03 — float `**` promotes to f64 (libm `pow`). All four
// operand shapes that yield a float:
//   2.0 ** 3.0 -> 8        (float ** float; cobrust float repr drops .0)
//   2.0 ** 0.5 -> 1.4142135623730951  (fractional exponent — sqrt(2))
//   2 ** 3.0   -> 8        (int ** float PROMOTES — the mixed shape)
//   2.0 ** 3   -> 8        (float ** int PROMOTES)
// CPython: 8.0 / 1.4142135623730951 / 8.0 / 8.0 (cobrust drops the .0 on
// the integer-valued results).
// =====================================================================

#[test]
fn power_e2e_03_float_pow_promotes() {
    let src = "\
fn main() -> i64:
    print(2.0 ** 3.0)
    print(2.0 ** 0.5)
    print(2 ** 3.0)
    print(2.0 ** 3)
    return 0
";
    assert_build_run("power_e2e_03", src, "8\n1.4142135623730951\n8\n8\n");
}

// =====================================================================
// power_e2e_04 — float base with a NEGATIVE exponent is DEFINED (the
// float path has no negative-exponent reject — the result is a float).
// `2.0 ** -1 == 0.5` (CPython). Contrast the int-base reject in
// power_e2e_06.
// =====================================================================

#[test]
fn power_e2e_04_float_base_negative_exponent_ok() {
    let src = "\
fn main() -> i64:
    print(2.0 ** -1)
    return 0
";
    assert_build_run("power_e2e_04", src, "0.5\n");
}

// =====================================================================
// power_e2e_05 — integer OVERFLOW TRAPS (exit 3), NOT a silent wrap
// (Constitution §2.2). `2 ** 63` overflows i64 (`i64::MAX` is `2**63 -
// 1`), so `checked_pow` returns `None` and `__cobrust_ipow` panics →
// exit 3. CPython promotes to bignum; Cobrust's i64 has no bignum, so a
// trap is the honest surface. The BUILD succeeds (a clean-typing program)
// — it is the RUN that traps.
// =====================================================================

#[test]
fn power_e2e_05_integer_overflow_traps() {
    let path = write_cb(
        "power_e2e_05",
        "\
fn main() -> i64:
    print(2 ** 63)
    return 0
",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "power_e2e_05: build should SUCCEED (overflow is a runtime trap, not a \
         compile error); stderr=\n{build_stderr}"
    );
    let (run_code, stdout, run_stderr) = run_exe(&exe);
    assert_eq!(
        run_code, 3,
        "power_e2e_05: `2 ** 63` must TRAP at runtime with exit 3 (not silently \
         wrap); stdout={stdout:?} stderr={run_stderr}"
    );
    assert!(
        run_stderr.contains("overflow"),
        "power_e2e_05: trap message should name the overflow; stderr={run_stderr}"
    );
}

// =====================================================================
// power_e2e_06 — a NEGATIVE-LITERAL exponent on an INT base REJECTS at
// COMPILE time (§2.5-A), exit 2. `2 ** -1` yields a non-integer (0.5),
// impossible for the pinned `int ** int -> int` result. The diagnostic
// PRINTS THE FIX (§2.5-B): use a float base. Mirrors F79's negative-
// literal scalar-index reject.
// =====================================================================

#[test]
fn power_e2e_06_negative_literal_exponent_rejected() {
    let path = write_cb(
        "power_e2e_06",
        "\
fn main() -> i64:
    print(2 ** -1)
    return 0
",
    );
    let (build_code, _exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 2,
        "power_e2e_06: `2 ** -1` (int base, negative literal exponent) must \
         REJECT at compile with exit 2; stderr=\n{build_stderr}"
    );
    // §2.5-B — the diagnostic must PRINT THE FIX (float base) so the LLM
    // agent rewrites it in one step.
    assert!(
        build_stderr.contains("negative exponent") && build_stderr.contains("float"),
        "power_e2e_06: reject message must name the negative exponent AND the \
         float-base fix; stderr=\n{build_stderr}"
    );
}

// =====================================================================
// power_e2e_07 — a runtime-DYNAMIC negative exponent (a VARIABLE, not a
// literal) TRAPS at runtime with exit 3. The type checker cannot see the
// sign of a non-literal exponent, so the compile-time reject (power_e2e_06)
// does not fire; `__cobrust_ipow` traps instead of returning a wrong-typed
// truncated value (§2.2 — no silent wrong value). The BUILD succeeds.
// =====================================================================

#[test]
fn power_e2e_07_runtime_negative_exponent_traps() {
    let path = write_cb(
        "power_e2e_07",
        "\
fn main() -> i64:
    let e: i64 = 0 - 1
    print(2 ** e)
    return 0
",
    );
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "power_e2e_07: build should SUCCEED (a non-literal negative exponent is a \
         runtime trap, not a compile error); stderr=\n{build_stderr}"
    );
    let (run_code, stdout, run_stderr) = run_exe(&exe);
    assert_eq!(
        run_code, 3,
        "power_e2e_07: a runtime-negative exponent must TRAP with exit 3; \
         stdout={stdout:?} stderr={run_stderr}"
    );
}

// =====================================================================
// power_e2e_08 — negative BASE, non-negative exponent stays an integer,
// sign tracking parity (CPython): `(-2) ** 3 == -8`, `(-2) ** 2 == 4`,
// `(-3) ** 0 == 1`.
// =====================================================================

#[test]
fn power_e2e_08_negative_base_integer() {
    let src = "\
fn main() -> i64:
    print((0 - 2) ** 3)
    print((0 - 2) ** 2)
    print((0 - 3) ** 0)
    return 0
";
    assert_build_run("power_e2e_08", src, "-8\n4\n1\n");
}
