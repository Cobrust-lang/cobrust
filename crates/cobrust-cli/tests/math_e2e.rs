//! ADR-0083 — `import math` (scalar stdlib) `.cb` END-TO-END tests.
//!
//! REAL compile -> link -> spawn -> assert-stdout, mirroring
//! `coil_scalararg_e2e.rs`'s pattern. `math` is the FIRST core-stdlib module
//! (json / re / datetime still absent); it is DISTINCT from `coil` — a
//! `coil.sqrt(a)` is a BUFFER ufunc (`Buffer -> Buffer`), a `math.sqrt(x)` is
//! a SCALAR `f64 -> f64` op lowering to a DIRECT libm call.
//!
//! Differential oracle: `/opt/homebrew/bin/python3.11`:
//!   import math
//!   math.sqrt(2)      = 1.4142135623730951
//!   math.pi           = 3.141592653589793
//!   math.e            = 2.718281828459045
//!   math.tau          = 6.283185307179586
//!   math.pow(2,10)    = 1024.0
//!   math.hypot(3,4)   = 5.0
//!   math.atan2(1,1)   = 0.7853981633974483  (== math.pi/4)
//!   math.fabs(-2.5)   = 2.5
//!   math.log10(1000)  = 3.0
//!   math.log2(8)      = 3.0
//!   math.exp(1)       = 2.718281828459045
//!   math.log(math.e)  = 1.0
//!   math.sin(0)=0  math.cos(0)=1  math.tan(0)=0
//!
//! PLATFORM-LAST-ULP LESSON (per task + macOS-vs-ubuntu libm divergence):
//! `sqrt` is IEEE-correctly-rounded, so its full-precision string is
//! platform-stable — asserted exactly. `pi`/`e`/`tau` are `f64` constants —
//! exact. The TRANSCENDENTALS (sin / cos / atan2 / exp / log) may differ in
//! the LAST ULP between macOS libm and ubuntu glibc, so they are asserted via
//! an `as i64` ROUNDED / IDENTITY form (e.g. `atan2(1,1) == pi/4`), NEVER a
//! full-precision platform-dependent float string.
//!
//! NOTE on the float-print surface (`__cobrust_println_float`, Rust `{}`):
//! integer-valued floats print WITHOUT a `.0` (`hypot(3,4)` -> `5`, not
//! `5.0`; `pow(2,10)` -> `1024`). This is the cobrust println_float repr, NOT
//! CPython's `5.0` — the `.cb` assertions below use the cobrust form.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

fn compile_source(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let build = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}\nstderr: {}",
        build.status,
        String::from_utf8_lossy(&build.stderr)
    );
    (dir, exe)
}

fn run(exe: &PathBuf) -> (String, String, bool) {
    let out = Command::new(exe).output().expect("spawn math prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn try_build(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// =====================================================================
// POSITIVE — sqrt is correctly-rounded: full-precision string is exact.
// =====================================================================

/// `print(math.sqrt(2.0))` -> `1.4142135623730951` (the float-print path,
/// `__cobrust_println_float`). sqrt is IEEE-correctly-rounded so this exact
/// string holds on every platform. Oracle: `math.sqrt(2)`.
#[test]
fn test_e2e_sqrt_two_exact() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.sqrt(2.0))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "1.4142135623730951",
        "expected sqrt(2)=1.4142135623730951; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `math.pi` constant (parens-free module attribute).
// =====================================================================

/// `print(math.pi)` -> `3.141592653589793`. The constant lowers to a pure
/// compile-time `Constant::Float` LLVM literal (NO runtime call). Exact f64.
#[test]
fn test_e2e_pi_constant() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.pi)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "3.141592653589793",
        "expected math.pi=3.141592653589793; got stdout=\n{stdout}",
    );
}

/// `let p: f64 = math.pi` binds + flows through arithmetic. `math.pi / 4.0`
/// equals `math.atan2(1.0, 1.0)` (the IDENTITY assertion — both sides use the
/// SAME platform pi, so the comparison is platform-stable even though the
/// raw atan2 last-ULP is not). Asserted as the `i64` truthy result `1`.
#[test]
fn test_e2e_pi_quarter_equals_atan2_identity() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    let q: f64 = math.pi / 4.0\n",
        "    let a: f64 = math.atan2(1.0, 1.0)\n",
        "    if q == a:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "1",
        "expected math.pi/4 == atan2(1,1); got stdout=\n{stdout}",
    );
}

/// Pins the `atan2(y, x)` ARGUMENT ORDER — the symmetric `atan2(1,1)` above
/// cannot (it is invariant under a y/x swap). `atan2(y=0, x=1)` is `0.0`
/// EXACTLY (a boundary case — no last-ULP platform wobble), whereas a swapped
/// `atan2(x, y)` impl would compute `atan2(1, 0) = pi/2 != 0`. Platform-robust
/// (0.0 is exact), so this is a genuine arg-order mutation probe.
#[test]
fn test_e2e_atan2_arg_order_y_then_x() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    let z: f64 = math.atan2(0.0, 1.0)\n",
        "    if z == 0.0:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "1",
        "expected atan2(0,1)==0.0 (pins atan2 y-then-x arg order); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — two-arg ops `pow` / `hypot` (integer-valued -> no `.0`).
// =====================================================================

/// `print(math.pow(2.0, 10.0))` -> `1024` (the float-print repr drops the
/// `.0` for integer-valued floats). Oracle: `math.pow(2,10)=1024.0`.
#[test]
fn test_e2e_pow_two_ten() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.pow(2.0, 10.0))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "1024",
        "expected pow(2,10)=1024; got stdout=\n{stdout}",
    );
}

/// `print(math.hypot(3.0, 4.0))` -> `5` (the 3-4-5 triangle; correctly-rounded
/// integer-valued result). Oracle: `math.hypot(3,4)=5.0`.
#[test]
fn test_e2e_hypot_three_four() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.hypot(3.0, 4.0))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "5",
        "expected hypot(3,4)=5; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — a CHAIN: nested calls feed each other (the §2.5 LLM-idiom).
// =====================================================================

/// `math.sqrt(math.pow(3.0, 2.0) + math.pow(4.0, 2.0))` -> `5.0` computed as
/// `sqrt(9 + 16) = sqrt(25) = 5`. Proves a scalar return feeds the next
/// scalar op's arg (the f64 `_ecoret` flows). Printed as `5` (integer-valued).
#[test]
fn test_e2e_pythagoras_chain() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    let h: f64 = math.sqrt(math.pow(3.0, 2.0) + math.pow(4.0, 2.0))\n",
        "    print(h)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "5",
        "expected sqrt(3^2 + 4^2)=5; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — transcendentals via `as i64` rounding (platform-ULP-safe).
// =====================================================================

/// The whole-number transcendental surface, each asserted via `as i64` so the
/// last-ULP libm divergence (macOS vs ubuntu) never breaks the assertion:
///   sin(0)=0  cos(0)=1  tan(0)=0  exp(1)=2 (trunc)  log10(1000)=3  log2(8)=3
///   fabs(-2.5)=2 (trunc)  asin(1)=1 (trunc of pi/2)  log(e)=1
/// Oracle: the Python values above (exp(1)=2.718.. truncs to 2; fabs(-2.5)=2.5
/// truncs to 2; asin(1)=1.5707.. truncs to 1).
#[test]
fn test_e2e_transcendentals_rounded() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print((math.sin(0.0) as i64))\n",
        "    print((math.cos(0.0) as i64))\n",
        "    print((math.tan(0.0) as i64))\n",
        "    print((math.exp(1.0) as i64))\n",
        "    print((math.log10(1000.0) as i64))\n",
        "    print((math.log2(8.0) as i64))\n",
        "    print((math.fabs(-2.5) as i64))\n",
        "    print((math.asin(1.0) as i64))\n",
        "    print((math.log(math.e) as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "0\n1\n0\n2\n3\n3\n2\n1\n1",
        "expected sin0/cos0/tan0/exp1/log10_1000/log2_8/fabs/asin1/log_e \
         rounded = 0 1 0 2 3 3 2 1 1; got stdout=\n{stdout}",
    );
}

/// `math.fabs(-2.5)` printed at full precision -> `2.5` (fabs is exact — it
/// only clears the sign bit, no rounding). Confirms the negative-arg + the
/// non-integer float repr both work on the scalar path.
#[test]
fn test_e2e_fabs_exact() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.fabs(-2.5))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "2.5",
        "expected fabs(-2.5)=2.5; got stdout=\n{stdout}",
    );
}

// =====================================================================
// DOMAIN POLICY — libm NaN / -inf (the documented Numerical-tier
// divergence from CPython's ValueError). ADR-0083 §"Domain errors".
// =====================================================================

/// `math.sqrt(-1.0)` -> `NaN` and `math.log(0.0)` -> `-inf` — the libm
/// behaviour Cobrust adopts (CPython would RAISE `ValueError`; we return the
/// IEEE value, NO trap, NO silent wrong-finite value). The Rust
/// `__cobrust_println_float` repr is `NaN` / `-inf`.
#[test]
fn test_e2e_domain_libm_nan_neg_inf() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.sqrt(-1.0))\n",
        "    print(math.log(0.0))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "NaN\n-inf",
        "expected sqrt(-1)=NaN, log(0)=-inf (libm, NOT CPython ValueError); \
         got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE — §2.2 no silent coercion: an Int / str arg is REJECTED at
// type-check (compile-time-catch, §2.5). Distinct module: `math` not
// `coil`.
// =====================================================================

/// `math.sqrt(2)` (Int literal, NOT `2.0`) must FAIL type-check — §2.2 forbids
/// the silent Int->Float promotion (consistent with `coil.power(a, 0.0)`'s
/// float-literal requirement). The diagnostic names a Float/Int mismatch.
#[test]
fn test_neg_sqrt_rejects_int_arg() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.sqrt(2))\n",
        "    return 0\n",
    );
    let (ok, stderr) = try_build(source);
    assert!(
        !ok,
        "math.sqrt(2) (Int arg) must be REJECTED; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("type mismatch") && stderr.contains("f64") && stderr.contains("i64"),
        "expected a polished f64-vs-i64 type mismatch (error_ux, F80); got stderr=\n{stderr}",
    );
}

/// `math.sqrt("x")` (str arg) must FAIL type-check — the manifest signature is
/// `[Float] -> Float`, so a Str arg is a hard TypeMismatch.
#[test]
fn test_neg_sqrt_rejects_str_arg() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.sqrt(\"x\"))\n",
        "    return 0\n",
    );
    let (ok, stderr) = try_build(source);
    assert!(
        !ok,
        "math.sqrt(\"x\") (str arg) must be REJECTED; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("type mismatch") && stderr.contains("f64") && stderr.contains("str"),
        "expected a polished f64-vs-str type mismatch (error_ux, F80); got stderr=\n{stderr}",
    );
}

/// An UNKNOWN module attribute (`math.phi`) must FAIL type-check with an
/// `UnknownName` (§2.5 compile-time-catch — NOT a false-green that unifies
/// with anything). The fix lists the real constants.
#[test]
fn test_neg_unknown_constant_rejected() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.phi)\n",
        "    return 0\n",
    );
    let (ok, stderr) = try_build(source);
    assert!(
        !ok,
        "math.phi (unknown const) must be REJECTED; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("math.phi"),
        "expected an UnknownName naming math.phi; got stderr=\n{stderr}",
    );
}
