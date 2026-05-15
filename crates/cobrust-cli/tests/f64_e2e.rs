//! M-F.3.3 — f64 end-to-end corpus.
//!
//! Per ADR-0050 §A1 "remaining gap" items:
//!   (a) `as` cast expression syntax (`x as f64`, `y as i64`)
//!   (b) PRELUDE + intrinsic-rewrite for math functions
//!       (`sqrt`, `floor`, `ceil`, `round`, `sin`, `cos`, `pow`,
//!       `abs`, `min`, `max`, `log`, `exp`)
//!   (c) f-string `{:.Nf}` / `{:e}` / `{:g}` lowering
//!   (d) `inf` / `nan` lexer literals
//!   (e) IEEE 754 strict compliance: NaN ≠ NaN, ±∞ ordering,
//!       0.1 + 0.2 ≠ 0.3 (floating-point representation)
//!
//! Each test writes a `.cb` program, invokes `cobrust build`, runs
//! the produced executable, and asserts stdout. Pattern mirrors
//! `for_range_e2e.rs` exactly (same helpers: `write_cb`,
//! `run_build_exe`, `run_exe`, `assert_build_run`).
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09:
//! 18-lint clippy module-level allow header at the TOP of every test
//! file.

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
// Tier C — `as` cast end-to-end (M-F.3.3 gap item a)
// These tests require the DEV agent to add:
//   1. ExprKind::Cast to the AST
//   2. `x as T` parsing in the Pratt parser
//   3. HIR lowering of Cast expr
//   4. MIR CastKind::IntToFloat / FloatToInt wiring from source
//   5. Type-checker validation (only i64↔f64 allowed)
// =====================================================================

#[test]
fn f64e01_cast_int_literal_to_f64_then_to_i64() {
    // `let x: f64 = 3.14; print_int(x as i64)` → prints "3" (truncating).
    assert_build_run(
        "f64e01_cast_int_lit",
        "fn main() -> i64:\n    let x: f64 = 3.14\n    print_int((x as i64))\n    return 0\n",
        &[],
        b"",
        "3\n",
    );
}

#[test]
fn f64e02_cast_i64_to_f64_then_divide() {
    // `print_int(((10 as f64) / 3.0) as i64)` → "3" (integer division via float).
    assert_build_run(
        "f64e02_cast_div",
        "fn main() -> i64:\n    print_int((((10 as f64) / 3.0) as i64))\n    return 0\n",
        &[],
        b"",
        "3\n",
    );
}

#[test]
fn f64e03_cast_f64_to_i64_floor_semantics() {
    // `3.9 as i64` truncates toward zero (C/IEEE semantics) → 3.
    assert_build_run(
        "f64e03_cast_trunc",
        "fn main() -> i64:\n    let v: f64 = 3.9\n    print_int((v as i64))\n    return 0\n",
        &[],
        b"",
        "3\n",
    );
}

#[test]
fn f64e04_cast_negative_f64_to_i64_truncates_toward_zero() {
    // `-3.9 as i64` truncates toward zero → -3 (not -4).
    assert_build_run(
        "f64e04_cast_neg_trunc",
        "fn main() -> i64:\n    let v: f64 = -3.9\n    print_int((v as i64))\n    return 0\n",
        &[],
        b"",
        "-3\n",
    );
}

#[test]
fn f64e05_cast_i64_zero_to_f64() {
    // `(0 as f64)` should produce 0.0 which cast back to i64 is 0.
    assert_build_run(
        "f64e05_cast_zero",
        "fn main() -> i64:\n    let z: f64 = (0 as f64)\n    print_int((z as i64))\n    return 0\n",
        &[],
        b"",
        "0\n",
    );
}

// =====================================================================
// Tier C — math intrinsics (M-F.3.3 gap item b)
// These tests require the DEV agent to:
//   1. Extend the PRELUDE with `fn sqrt(x: f64) -> f64` etc.
//   2. Add intrinsic-rewrite pass entries for each math function
//      (mirrors ADR-0044 `input`/`argv` pattern)
//   3. Add C-ABI shims in cobrust-stdlib/src/math.rs (Rust side exists)
// =====================================================================

#[test]
fn f64e06_sqrt_of_four_is_two() {
    // `sqrt(4.0)` → 2.0 → `as i64` → 2.
    assert_build_run(
        "f64e06_sqrt",
        "fn main() -> i64:\n    let r: f64 = sqrt(4.0)\n    print_int((r as i64))\n    return 0\n",
        &[],
        b"",
        "2\n",
    );
}

#[test]
fn f64e07_sqrt_of_two_truncated() {
    // `sqrt(2.0)` → ~1.4142 → truncated as i64 → 1.
    assert_build_run(
        "f64e07_sqrt_two",
        "fn main() -> i64:\n    let r: f64 = sqrt(2.0)\n    print_int((r as i64))\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

#[test]
fn f64e08_pow_two_to_ten_is_1024() {
    // `pow(2.0, 10.0) as i64` → 1024.
    assert_build_run(
        "f64e08_pow",
        "fn main() -> i64:\n    let p: f64 = pow(2.0, 10.0)\n    print_int((p as i64))\n    return 0\n",
        &[],
        b"",
        "1024\n",
    );
}

#[test]
fn f64e09_floor_of_3pt7_is_3() {
    // `floor(3.7) as i64` → 3.
    assert_build_run(
        "f64e09_floor",
        "fn main() -> i64:\n    let f: f64 = floor(3.7)\n    print_int((f as i64))\n    return 0\n",
        &[],
        b"",
        "3\n",
    );
}

#[test]
fn f64e10_ceil_of_3pt2_is_4() {
    // `ceil(3.2) as i64` → 4.
    assert_build_run(
        "f64e10_ceil",
        "fn main() -> i64:\n    let c: f64 = ceil(3.2)\n    print_int((c as i64))\n    return 0\n",
        &[],
        b"",
        "4\n",
    );
}

#[test]
fn f64e11_round_of_2pt5_is_3() {
    // `round(2.5) as i64` → 3 (round-half-up).
    // NOTE: IEEE 754 "round half to even" (banker's rounding) would give 2.
    // Cobrust follows Rust's f64::round() which is "round half away from zero" → 3.
    assert_build_run(
        "f64e11_round",
        "fn main() -> i64:\n    let r: f64 = round(2.5)\n    print_int((r as i64))\n    return 0\n",
        &[],
        b"",
        "3\n",
    );
}

#[test]
fn f64e12_abs_of_negative() {
    // `abs(-5.5) as i64` → 5.
    assert_build_run(
        "f64e12_abs",
        "fn main() -> i64:\n    let a: f64 = abs(-5.5)\n    print_int((a as i64))\n    return 0\n",
        &[],
        b"",
        "5\n",
    );
}

// =====================================================================
// Tier C — f-string precision (M-F.3.3 gap item c)
// These tests require DEV to wire format_spec in MIR FormatPart::Hole
// to `__cobrust_fmt_float` with precision / mode arguments.
// =====================================================================

#[test]
fn f64e13_fstring_fixed_2_decimals() {
    // `f"{x:.2f}"` where x = 3.14159 → "3.14".
    assert_build_run(
        "f64e13_fstr_fixed",
        "fn main() -> i64:\n    let x: f64 = 3.14159\n    print(f\"{x:.2f}\")\n    return 0\n",
        &[],
        b"",
        "3.14\n",
    );
}

#[test]
fn f64e14_fstring_fixed_4_decimals() {
    // `f"{y:.4f}"` where y = sqrt(2.0) → "1.4142".
    // NOTE: This test implicitly requires both sqrt() and f-string precision.
    assert_build_run(
        "f64e14_fstr_sqrt",
        "fn main() -> i64:\n    let y: f64 = sqrt(2.0)\n    print(f\"{y:.4f}\")\n    return 0\n",
        &[],
        b"",
        "1.4142\n",
    );
}

#[test]
fn f64e15_fstring_zero_decimals() {
    // `f"{x:.0f}"` → no decimal point, just the integer part.
    assert_build_run(
        "f64e15_fstr_zero_dec",
        "fn main() -> i64:\n    let x: f64 = 3.7\n    print(f\"{x:.0f}\")\n    return 0\n",
        &[],
        b"",
        "4\n",
    );
}

// =====================================================================
// Tier D — IEEE 754 strict compliance corner cases
// (ADR-0050 §A1 audit Lane 1 specific recommendation)
// =====================================================================

#[test]
fn f64e16_nan_not_equal_to_itself() {
    // `nan != nan` is `true` per IEEE 754.
    // Tests that the codegen uses `fcmp` with the NaN-aware `ne` predicate.
    assert_build_run(
        "f64e16_nan_neq",
        "fn main() -> i64:\n    let n: f64 = nan\n    if (n != n):\n        print_int(1)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

#[test]
fn f64e17_nan_equal_to_itself_is_false() {
    // `nan == nan` must be `false` — the else branch prints 0.
    assert_build_run(
        "f64e17_nan_eq_false",
        "fn main() -> i64:\n    let n: f64 = nan\n    if (n == n):\n        print_int(1)\n    else:\n        print_int(0)\n    return 0\n",
        &[],
        b"",
        "0\n",
    );
}

#[test]
fn f64e18_inf_greater_than_any_finite() {
    // `inf > 1e308` must be `true`.
    assert_build_run(
        "f64e18_inf_gt",
        "fn main() -> i64:\n    let i: f64 = inf\n    let big: f64 = 1e308\n    if (i > big):\n        print_int(1)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

#[test]
fn f64e19_neg_inf_less_than_any_finite() {
    // `-inf < -1e308` must be `true`.
    assert_build_run(
        "f64e19_neg_inf_lt",
        "fn main() -> i64:\n    let ni: f64 = -inf\n    let neg_big: f64 = -1e308\n    if (ni < neg_big):\n        print_int(1)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

#[test]
fn f64e20_inf_ordering_chain() {
    // `-inf < 0.0 < inf` is true.
    assert_build_run(
        "f64e20_inf_order",
        "fn main() -> i64:\n    let pos: f64 = inf\n    let neg: f64 = -inf\n    let zero: f64 = 0.0\n    if ((neg < zero) and (zero < pos)):\n        print_int(1)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

#[test]
fn f64e21_one_div_zero_is_inf() {
    // `1.0 / 0.0 == inf` — IEEE 754 defines this.
    // Cobrust must NOT trap on float division by zero (unlike integer div/zero).
    assert_build_run(
        "f64e21_div_zero_inf",
        "fn main() -> i64:\n    let r: f64 = (1.0 / 0.0)\n    let i: f64 = inf\n    if (r == i):\n        print_int(1)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

#[test]
fn f64e22_zero_point_one_plus_zero_point_two_not_eq_zero_point_three() {
    // `(0.1 + 0.2) != 0.3` is `true` — IEEE 754 binary64 representation gap.
    // This is the canonical floating-point gotcha; Cobrust must not paper over it.
    assert_build_run(
        "f64e22_0point1_0point2",
        "fn main() -> i64:\n    let s: f64 = (0.1 + 0.2)\n    let t: f64 = 0.3\n    if (s != t):\n        print_int(1)\n    else:\n        print_int(0)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

#[test]
fn f64e23_sqrt_negative_produces_nan() {
    // `sqrt(-1.0)` produces NaN; NaN != NaN → prints 1.
    // Marks sqrt(-1.0) NaN behavior via identity test.
    // NOTE: Requires sqrt() PRELUDE intrinsic to be wired (gap item b).
    assert_build_run(
        "f64e23_sqrt_neg_nan",
        "fn main() -> i64:\n    let r: f64 = sqrt(-1.0)\n    if (r != r):\n        print_int(1)\n    else:\n        print_int(0)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

// =====================================================================
// Tier E — `inf` / `nan` lexer literal coverage
// =====================================================================

#[test]
fn f64e24_inf_literal_check_accepts() {
    // `inf` accepted as f64 literal in expression position.
    // `cobrust check` must exit 0.
    let src = write_cb(
        "f64e24_inf_check",
        "fn main() -> i64:\n    let x: f64 = inf\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok for `inf` literal; stderr={stderr}");
}

#[test]
fn f64e25_nan_literal_check_accepts() {
    // `nan` accepted as f64 literal.
    let src = write_cb(
        "f64e25_nan_check",
        "fn main() -> i64:\n    let x: f64 = nan\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok for `nan` literal; stderr={stderr}");
}

#[test]
fn f64e26_neg_inf_literal_check_accepts() {
    // `-inf` as unary-neg applied to the `inf` f64 literal.
    let src = write_cb(
        "f64e26_neg_inf_check",
        "fn main() -> i64:\n    let x: f64 = -inf\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok for `-inf`; stderr={stderr}");
}

#[test]
fn f64e27_inf_in_arithmetic_check_accepts() {
    // Arithmetic on `inf` must be well-typed.
    let src = write_cb(
        "f64e27_inf_arith_check",
        "fn main() -> i64:\n    let x: f64 = inf\n    let y: f64 = (x + 1.0)\n    return 0\n",
    );
    let (code, stderr) = run_check(&src);
    assert_eq!(code, 0, "expected check ok; stderr={stderr}");
}

#[test]
fn f64e28_inf_run_prints_int_one() {
    // `inf > 0.0` is true → prints 1.
    assert_build_run(
        "f64e28_inf_run",
        "fn main() -> i64:\n    let x: f64 = inf\n    if (x > 0.0):\n        print_int(1)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

#[test]
fn f64e29_nan_run_not_gt_zero() {
    // `nan > 0.0` is false per IEEE 754 (NaN comparisons are always false).
    assert_build_run(
        "f64e29_nan_not_gt",
        "fn main() -> i64:\n    let x: f64 = nan\n    if (x > 0.0):\n        print_int(1)\n    else:\n        print_int(0)\n    return 0\n",
        &[],
        b"",
        "0\n",
    );
}

// =====================================================================
// Tier D — signaling / bit-pattern NaN preservation (stub / #[ignore])
// Per ADR-0050 mission briefing: "You may stub these as #[ignore] if the
// impl doesn't yet support bit-pattern preservation — the goal is to
// surface the spec gap, not gate impl on it."
// =====================================================================

#[test]
#[ignore = "M-F.3.3 DEV stretch: log(-1.0) NaN bit-pattern preservation not yet specified"]
fn f64e30_log_negative_produces_nan_bit_pattern_preserved() {
    // `log(-1.0)` must produce a NaN whose bit pattern is reproducible
    // (quiet NaN, not signaling). The exact bit pattern is
    // implementation-defined; the property tested is simply "result is NaN".
    // NOTE: Requires log() PRELUDE intrinsic (gap item b).
    assert_build_run(
        "f64e30_log_neg_nan",
        "fn main() -> i64:\n    let r: f64 = log(-1.0)\n    if (r != r):\n        print_int(1)\n    else:\n        print_int(0)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

#[test]
#[ignore = "M-F.3.3 DEV stretch: pow(negative, non-integer) NaN not yet specified"]
fn f64e31_pow_negative_base_non_integer_exp_is_nan() {
    // `pow(-1.0, 0.5)` → NaN (square root of -1).
    // NOTE: Requires pow() PRELUDE intrinsic (gap item b).
    assert_build_run(
        "f64e31_pow_neg_nan",
        "fn main() -> i64:\n    let r: f64 = pow(-1.0, 0.5)\n    if (r != r):\n        print_int(1)\n    else:\n        print_int(0)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

// =====================================================================
// Tier C — additional round-trip / mixed scenarios
// =====================================================================

#[test]
fn f64e32_pow_then_cast_back_to_i64() {
    // pow(2.0, 10.0) = 1024.0 → as i64 → 1024. Mirrors mission brief example.
    assert_build_run(
        "f64e32_pow_cast",
        "fn main() -> i64:\n    let p: f64 = pow(2.0, 10.0)\n    print_int((p as i64))\n    return 0\n",
        &[],
        b"",
        "1024\n",
    );
}

#[test]
fn f64e33_circle_area_print_fixed_2() {
    // Circle area: π × r² where r = 5.0.
    // Area = 3.14159... × 25.0 = 78.539...
    // `f"{area:.2f}"` → "78.54".
    // NOTE: Requires both math PRELUDE (for PI constant) and f-string precision.
    // If PI is not available as a constant, use the literal 3.14159265358979.
    assert_build_run(
        "f64e33_circle",
        "fn main() -> i64:\n    let r: f64 = 5.0\n    let pi: f64 = 3.14159265358979\n    let area: f64 = (pi * (r * r))\n    print(f\"{area:.2f}\")\n    return 0\n",
        &[],
        b"",
        "78.54\n",
    );
}

#[test]
fn f64e34_basic_float_arithmetic_print() {
    // Simple arithmetic and truncation to int.
    // (1.5 + 2.5) * 2.0 = 8.0 → as i64 → 8.
    assert_build_run(
        "f64e34_basic_arith",
        "fn main() -> i64:\n    let a: f64 = 1.5\n    let b: f64 = 2.5\n    let c: f64 = ((a + b) * 2.0)\n    print_int((c as i64))\n    return 0\n",
        &[],
        b"",
        "8\n",
    );
}

#[test]
fn f64e35_cast_in_loop_accumulates_correctly() {
    // Cast i64 loop var to f64 and accumulate; verify result.
    // Sum of (0 as f64 + 1.0) + ... + (9 as f64 + 1.0) = 55.0.
    // 0+1+2+...+9 = 45; plus 10 loop iters × 1.0 = 55.0 → as i64 → 55.
    // NOTE: Requires both `as` cast (gap a) and `range`/for (already shipped).
    assert_build_run(
        "f64e35_cast_loop",
        "fn main() -> i64:\n    let acc: f64 = 0.0\n    let i: i64 = 0\n    while (i < 10):\n        acc = (acc + ((i as f64) + 1.0))\n        i = (i + 1)\n    print_int((acc as i64))\n    return 0\n",
        &[],
        b"",
        "55\n",
    );
}
