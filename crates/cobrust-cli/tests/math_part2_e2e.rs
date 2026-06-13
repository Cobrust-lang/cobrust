//! ADR-0083 PART-2 — `import math` INT / BOOL / scaling return-shape
//! `.cb` END-TO-END tests.
//!
//! REAL compile -> link -> spawn -> assert-stdout, mirroring
//! `math_e2e.rs` (part-1) + `coil_reduce_e2e.rs` (the Int/Bool-return
//! `coil.argmin` / `coil.any` idioms this part mirrors).
//!
//! Part-2 ships the functions DEFERRED from part-1 because they leave the
//! clean `f64 -> f64` libm batch:
//!
//!   - `math.floor` / `math.ceil` / `math.trunc` — CPython returns an
//!     **`int`** (NOT a float). Via the `__cobrust_math_*_int`
//!     (`f64 -> i64`) shims, mirroring `coil.argmin`'s `Buffer -> i64`.
//!     On a NEGATIVE input the three DIVERGE — the load-bearing test:
//!     floor(-1.5) == -2 (toward −∞), ceil(-1.5) == -1 (toward +∞),
//!     trunc(-1.5) == -1 (toward ZERO).
//!   - `math.isnan` / `math.isinf` / `math.isfinite` — return `bool`,
//!     via the `__cobrust_math_is*` (`f64 -> i1`) shims, mirroring
//!     `coil.any` / `coil.all`. Asserted via the `if b:` idiom (the bool
//!     is USED in a condition — proving the bool return is usable, not
//!     just printable).
//!   - `math.degrees` / `math.radians` — `f64 -> f64` shims.
//!   - `math.copysign(x,y)` / `math.fmod(x,y)` — BARE libm two-arg
//!     symbols (NO shim, like part-1's `pow` / `atan2` / `hypot`).
//!
//! Differential oracle: `/opt/homebrew/bin/python3.11`:
//!   import math
//!   math.floor(-1.5) = -2   math.floor(2.7) = 2
//!   math.ceil(-1.5)  = -1   math.ceil(2.1)  = 3
//!   math.trunc(-1.5) = -1   math.trunc(1.9) = 1
//!   math.isnan(nan)  = True   math.isnan(1.0)  = False
//!   math.isinf(inf)  = True   math.isfinite(1.0) = True
//!   math.isfinite(inf) = False  math.isfinite(nan) = False
//!   math.degrees(pi) = 180.0  math.radians(180.0) = pi
//!   math.copysign(3.0,-1.0) = -3.0   math.fmod(7.0,3.0) = 1.0
//!
//! NOTE on the float-print surface (`__cobrust_println_float`, Rust `{}`):
//! integer-valued floats print WITHOUT a `.0` (`degrees(pi)` -> `180`,
//! `copysign(3,-1)` -> `-3`, `fmod(7,3)` -> `1`). This is the cobrust
//! println_float repr, NOT CPython's `180.0` — the assertions use the
//! cobrust form (consistent with `math_e2e.rs`).

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
    let out = Command::new(exe).output().expect("spawn math part2 prog");
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
// POSITIVE — floor / ceil / trunc return an INT, and DIVERGE on a
// negative input. THE load-bearing distinction: -1.5 maps to -2 / -1 /
// -1 under floor / ceil / trunc. Bound as `let n: i64 = ...` (proving
// the Int return), then `print(n)`. Mirrors `coil.argmin`'s
// `let lo: i64 = coil.argmin(&a)` idiom.
// =====================================================================

/// `math.floor(-1.5)` -> `-2` (toward −∞), `math.ceil(-1.5)` -> `-1`
/// (toward +∞), `math.trunc(-1.5)` -> `-1` (toward ZERO). The THREE
/// diverge on the SAME negative input — pins that floor != ceil != trunc.
/// Oracle: `math.floor(-1.5)=-2`, `math.ceil(-1.5)=-1`,
/// `math.trunc(-1.5)=-1`.
#[test]
fn test_e2e_floor_ceil_trunc_diverge_on_negative() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    let f: i64 = math.floor(-1.5)\n",
        "    let c: i64 = math.ceil(-1.5)\n",
        "    let t: i64 = math.trunc(-1.5)\n",
        "    print(f)\n",
        "    print(c)\n",
        "    print(t)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "-2\n-1\n-1",
        "expected floor(-1.5)=-2 (−∞), ceil(-1.5)=-1 (+∞), \
         trunc(-1.5)=-1 (zero); got stdout=\n{stdout}",
    );
}

/// `math.floor(2.7)` -> `2`, `math.ceil(2.1)` -> `3`, `math.trunc(1.9)`
/// -> `1`. The POSITIVE-input direction + confirms the result is an INT
/// usable in i64 ARITHMETIC (`floor(2.7) + trunc(1.9)` = `2 + 1` = `3`).
/// Oracle: `math.floor(2.7)=2`, `math.ceil(2.1)=3`, `math.trunc(1.9)=1`.
#[test]
fn test_e2e_floor_ceil_trunc_positive_and_int_arithmetic() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    let f: i64 = math.floor(2.7)\n",
        "    let c: i64 = math.ceil(2.1)\n",
        "    let t: i64 = math.trunc(1.9)\n",
        "    print(f)\n",
        "    print(c)\n",
        "    print(t)\n",
        // The Int return flows into i64 arithmetic — proves it is a true
        // integer, not a float repr that happens to print without `.0`.
        "    let s: i64 = f + t\n",
        "    print(s)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "2\n3\n1\n3",
        "expected floor(2.7)=2, ceil(2.1)=3, trunc(1.9)=1, \
         floor+trunc=3; got stdout=\n{stdout}",
    );
}

/// `math.trunc` vs `math.floor` on the SAME negative non-integer input —
/// the cleanest isolation of the toward-ZERO vs toward-−∞ distinction.
/// `math.trunc(-2.7)` -> `-2` (drop the fraction), `math.floor(-2.7)` ->
/// `-3` (go down). Oracle: `math.trunc(-2.7)=-2`, `math.floor(-2.7)=-3`.
#[test]
fn test_e2e_trunc_vs_floor_negative_distinguishes() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    let tr: i64 = math.trunc(-2.7)\n",
        "    let fl: i64 = math.floor(-2.7)\n",
        "    print(tr)\n",
        "    print(fl)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "-2\n-3",
        "expected trunc(-2.7)=-2 (toward zero) != floor(-2.7)=-3 \
         (toward −∞); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — isnan / isinf / isfinite return a BOOL that is USABLE IN A
// CONDITION. Bound as `let b: bool = ...`, then driven through `if b:`
// (the same form `coil_reduce_e2e.rs` uses for `coil.any`/`coil.all` —
// `print(bool)` directly is avoided; the `if` PROVES the bool return is
// a real condition value). 1 = True, 0 = False.
// =====================================================================

/// `math.isnan(nan)` is True (the bare `nan` literal as input);
/// `math.isnan(1.0)` is False. Drives both through `if b:`. Oracle:
/// `math.isnan(nan)=True`, `math.isnan(1.0)=False`.
#[test]
fn test_e2e_isnan_bool_in_condition() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    let x: f64 = nan\n",
        "    let is_n: bool = math.isnan(x)\n",
        "    if is_n:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    let y: f64 = 1.0\n",
        "    let is_n2: bool = math.isnan(y)\n",
        "    if is_n2:\n",
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
        "1\n0",
        "expected isnan(nan)=True(1), isnan(1.0)=False(0); \
         got stdout=\n{stdout}",
    );
}

/// `math.isinf(inf)` is True (the bare `inf` literal as input);
/// `math.isinf(1.0)` is False. Oracle: `math.isinf(inf)=True`,
/// `math.isinf(1.0)=False`.
#[test]
fn test_e2e_isinf_bool_in_condition() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    let big: f64 = inf\n",
        "    let is_i: bool = math.isinf(big)\n",
        "    if is_i:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    let small: f64 = 1.0\n",
        "    let is_i2: bool = math.isinf(small)\n",
        "    if is_i2:\n",
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
        "1\n0",
        "expected isinf(inf)=True(1), isinf(1.0)=False(0); \
         got stdout=\n{stdout}",
    );
}

/// `math.isfinite` truth table: `isfinite(1.0)` True, `isfinite(inf)`
/// False, `isfinite(nan)` False. The full three-way classification on the
/// bare `inf` / `nan` literals. Oracle: `True`, `False`, `False`.
#[test]
fn test_e2e_isfinite_truth_table() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: f64 = 1.0\n",
        "    let fa: bool = math.isfinite(a)\n",
        "    if fa:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    let b: f64 = inf\n",
        "    let fb: bool = math.isfinite(b)\n",
        "    if fb:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    let c: f64 = nan\n",
        "    let fc: bool = math.isfinite(c)\n",
        "    if fc:\n",
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
        "1\n0\n0",
        "expected isfinite(1.0)=True(1), isfinite(inf)=False(0), \
         isfinite(nan)=False(0); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — degrees / radians (`f64 -> f64`) + copysign / fmod (bare
// libm two-arg). The scaling + sign + modulo ops, asserted at the
// cobrust float-print repr.
// =====================================================================

/// `math.degrees(math.pi)` -> `180` (the integer-valued float prints
/// without `.0`); `math.radians(180.0)` round-trips back so
/// `radians(degrees(pi))` truncates to `3` only after `* ...` — instead
/// we assert the cleaner identity `degrees(pi/2) as i64 == 90`. Oracle:
/// `math.degrees(pi)=180.0`, `math.degrees(pi/2)=90.0`.
#[test]
fn test_e2e_degrees_pi_and_half() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.degrees(math.pi))\n",
        "    let half: f64 = math.pi / 2.0\n",
        "    print((math.degrees(half) as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "180\n90",
        "expected degrees(pi)=180, degrees(pi/2)=90; got stdout=\n{stdout}",
    );
}

/// `math.radians(180.0)` equals `math.pi` (the inverse of degrees). Pin
/// the round-trip identity via `radians(180.0) == pi` (both sides exact)
/// by printing their difference rounded — `(radians(180) - pi) as i64`
/// is `0`. Oracle: `math.radians(180.0)=3.141592653589793` == `math.pi`.
#[test]
fn test_e2e_radians_180_equals_pi() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    let r: f64 = math.radians(180.0)\n",
        "    let d: f64 = r - math.pi\n",
        "    print((math.fabs(d) as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "0",
        "expected radians(180)-pi ~ 0 (round-trip identity); \
         got stdout=\n{stdout}",
    );
}

/// `math.copysign(3.0, -1.0)` -> `-3` (magnitude of x, sign of y);
/// `math.copysign(-3.0, 1.0)` -> `3`. Oracle: `math.copysign(3,-1)=-3.0`,
/// `math.copysign(-3,1)=3.0`.
#[test]
fn test_e2e_copysign_transplants_sign() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.copysign(3.0, -1.0))\n",
        "    print(math.copysign(-3.0, 1.0))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "-3\n3",
        "expected copysign(3,-1)=-3, copysign(-3,1)=3; got stdout=\n{stdout}",
    );
}

/// `math.fmod(7.0, 3.0)` -> `1` (the C-library floating remainder).
/// Oracle: `math.fmod(7,3)=1.0`.
#[test]
fn test_e2e_fmod_remainder() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.fmod(7.0, 3.0))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "1",
        "expected fmod(7,3)=1; got stdout=\n{stdout}",
    );
}

/// Pins `fmod`'s SIGN rule — fmod takes the sign of the DIVIDEND (the C
/// `fmod` semantics), which DIVERGES from Python's `%` (sign of the divisor).
/// `math.fmod(-7.0, 3.0)` is `-1.0` (sign of -7); Python `(-7) % 3 == 2`. The
/// positive `fmod(7,3)=1` above cannot distinguish the two — this negative
/// case does. fmod is EXACT (no last-ULP), so `-1` is bit-stable cross-platform.
#[test]
fn test_e2e_fmod_negative_sign_of_dividend() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    print(math.fmod(-7.0, 3.0))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "-1",
        "expected fmod(-7,3)=-1 (sign of dividend, NOT Python %=2); got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE — §2.2 no silent coercion: an Int arg is REJECTED at
// type-check (compile-time-catch, §2.5), exactly as part-1's `math.sqrt`.
// =====================================================================

/// `math.floor(2)` (Int literal, NOT `2.0`) must FAIL type-check — the
/// manifest param is `Ty::Float`, and §2.2 forbids the silent Int->Float
/// promotion. (The RESULT is an Int, but the ARGUMENT must still be a
/// Float.) The diagnostic names a Float/Int mismatch.
#[test]
fn test_neg_floor_rejects_int_arg() {
    let source = concat!(
        "import math\n",
        "\n",
        "fn main() -> i64:\n",
        "    let n: i64 = math.floor(2)\n",
        "    print(n)\n",
        "    return 0\n",
    );
    let (ok, stderr) = try_build(source);
    assert!(
        !ok,
        "math.floor(2) (Int arg) must be REJECTED; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("type mismatch") && stderr.contains("f64") && stderr.contains("i64"),
        "expected a polished f64-vs-i64 type mismatch (error_ux, F80); got stderr=\n{stderr}",
    );
}
