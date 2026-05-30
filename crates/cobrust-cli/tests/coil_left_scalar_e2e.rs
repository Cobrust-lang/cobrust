//! coil LEFT-scalar `k ⊕ a` — `.cb` end-to-end proof for ADR-0077
//! Phase-2/3 addition (A): the scalar on the LEFT of a `coil.Buffer`
//! arithmetic op (`2 * a`, `6 / a`, `1 + a`, `2 - a`). The RIGHT-scalar
//! form `a ⊕ k` shipped in the Phase-1 completion (`coil_div_scalar_e2e`);
//! this is its MIRROR.
//!
//! ## The commute / reverse split (the load-bearing semantics)
//!
//! - `+` / `*` COMMUTE: `k + a == a + k`, `k * a == a * k`. The left-scalar
//!   form reuses the EXISTING right-scalar shims
//!   `__cobrust_coil_buffer_{add,mul}_scalar(a, k)` — the MIR retarget
//!   passes the BUFFER as the handle arg and the SCALAR as `k: f64`.
//! - `-` / `/` do NOT commute: `k - a != a - k`, `k / a != a / k`. The
//!   left-scalar form needs REVERSED shims
//!   `__cobrust_coil_buffer_{rsub,rdiv}_scalar(a, k)`, which compute
//!   `k - a[i]` / `k / a[i]` (the cabi `buffer_binop_scalar_rev` puts `k`
//!   on the LEFT). `2 - a` is `2 - a[i]` (NOT `a[i] - 2`); `6 / a` is
//!   `6 / a[i]` (NOT `a[i] / 6`).
//!
//! Each `-` / `/` test is constructed so the REVERSED result is
//! DISTINGUISHABLE from the (wrong) right-scalar result — otherwise the
//! test could not catch an operand-order bug.
//!
//! Where this sits in the chain (the SAME five touch-points as the
//! right-scalar completion):
//!   - typecheck `synth_bin` arithmetic arm — a left-scalar block admitting
//!     `Int/Float ⊕ Buffer` via `lookup_buffer_left_scalar_binop`;
//!   - MIR retarget (`lower.rs`) — `k ⊕ a` → a `Terminator::Call` onto the
//!     commutative / reversed scalar shim (buffer = handle, scalar = `k`);
//!   - codegen externs (`llvm_backend.rs`) — `rsub_scalar` / `rdiv_scalar`
//!     join the existing `*_scalar` rows (same `(ptr, f64) -> ptr` shape);
//!   - the cabi shim bodies (`cabi.rs`) — `buffer_binop_scalar_rev` flips
//!     the operand order, forwarding to the SAME array-array kernel;
//!   - manifest `lookup_buffer_left_scalar_binop` (`ecosystem.rs`).
//!
//! Mirrors the compile→spawn→assert-stdout harness of
//! `coil_div_scalar_e2e.rs`. f64 reads are observed via `(x as i64)` casts
//! (which truncate toward zero) where the result is an exact integer.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. The caller spawns + asserts. Mirrors `coil_div_scalar_e2e.rs`.
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

/// `cobrust check` helper — `(ok, exit_code, stderr)`. For the negative
/// (out-of-scope) assertions that must reject at typecheck.
fn try_check(source: &str) -> (bool, Option<i32>, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("check")
        .arg(&src_path)
        .output()
        .unwrap();
    (
        out.status.success(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// =====================================================================
// POSITIVE — left-scalar COMMUTATIVE `+` / `*` (reuse right-scalar shims)
// =====================================================================

/// Positive #1 (left-scalar `+`, commutes) — `1 + [1,2,3]` → `[2,3,4]`.
/// `+` commutes, so `1 + a == a + 1`; the left-scalar form reuses
/// `__cobrust_coil_buffer_add_scalar`. Observe `c[0]` → `2.0` → "2" and
/// `coil.mean(c)` → `(2+3+4)/3 = 3.0` → "3".
///
/// Oracle (numpy 2.0.2): `1 + np.array([1.,2.,3.])` → `array([2.,3.,4.])`.
#[test]
fn test_e2e_left_scalar_add_one() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(1, 4)\n", // [1,2,3]
        "    let c: coil.Buffer = 1 + a\n",
        "    let x0: f64 = c[0]\n",
        "    print((x0 as i64))\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn left-scalar-add");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // 1 + [1,2,3] = [2,3,4]; c[0]=2, mean=3.
    assert_eq!(
        stdout.trim(),
        "2\n3".trim_end(),
        "expected 1+[1,2,3] = [2,3,4]: c[0]='2', mean='3'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #2 (left-scalar `*`, commutes) — `3 * [1,2,3]` → `[3,6,9]`.
/// `*` commutes, so `3 * a == a * 3`; reuses
/// `__cobrust_coil_buffer_mul_scalar`. A non-identity scalar (`3`) over a
/// non-uniform array rules out an add/no-op masquerading as multiply.
/// Observe `coil.mean(c)` → `(3+6+9)/3 = 6.0` → "6".
///
/// This is the canonical §2.5 "numpy users write `2 * a`" form.
///
/// Oracle (numpy 2.0.2): `3 * np.array([1.,2.,3.])` → `array([3.,6.,9.])`;
/// mean `6.0`.
#[test]
fn test_e2e_left_scalar_mul_three() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(1, 4)\n", // [1,2,3]
        "    let c: coil.Buffer = 3 * a\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn left-scalar-mul");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "6",
        "expected mean(3*[1,2,3]) = mean([3,6,9]) = 6 → '6'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — left-scalar REVERSED `-` / `/` (the load-bearing cases).
// Each value chosen so the REVERSED result differs from the WRONG
// (right-scalar) result — an operand-order bug would FAIL these.
// =====================================================================

/// Positive #3 (left-scalar `-`, REVERSED) — `10 - [2,4]` → `[8,6]`.
/// THE reversed-subtract discriminator: `10 - a[i]`, NOT `a[i] - 10`
/// (which would be `[-8,-6]`). Observe `c[0]` → `8.0` → "8", `c[1]` →
/// `6.0` → "6". A bug that reused the right-scalar `_sub_scalar` (computing
/// `a[i] - 10`) would print "-8\n-6" and FAIL here.
///
/// Oracle (numpy 2.0.2): `10 - np.array([2.,4.])` → `array([8.,6.])`.
#[test]
fn test_e2e_left_scalar_sub_is_reversed() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 4.0)\n", // [2,4]
        "    let c: coil.Buffer = 10 - a\n",                  // [8,6] (reversed)
        "    let x0: f64 = c[0]\n",
        "    let x1: f64 = c[1]\n",
        "    print((x0 as i64))\n",
        "    print((x1 as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn left-scalar-sub");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // 10 - [2,4] = [8,6] (REVERSED). A wrong `a - 10` would give [-8,-6].
    assert_eq!(
        stdout.trim(),
        "8\n6".trim_end(),
        "expected REVERSED 10-[2,4] = [8,6] (NOT [2,4]-10 = [-8,-6]): c[0]='8', c[1]='6'; \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #4 (left-scalar `/`, REVERSED true-division) — `8 / [2,4]` →
/// `[4,2]`. THE reversed-divide discriminator: `8 / a[i]`, NOT `a[i] / 8`
/// (which would be `[0.25, 0.5]`). Observe `c[0]` → `4.0` → "4", `c[1]` →
/// `2.0` → "2". A bug reusing the right-scalar `_div_scalar` (`a[i] / 8`)
/// would print "0\n0" (both `< 1` truncate to 0) and FAIL here.
///
/// Oracle (numpy 2.0.2): `8 / np.array([2.,4.])` → `array([4.,2.])`.
#[test]
fn test_e2e_left_scalar_div_is_reversed() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 4.0)\n", // [2,4]
        "    let c: coil.Buffer = 8 / a\n",                   // [4,2] (reversed)
        "    let x0: f64 = c[0]\n",
        "    let x1: f64 = c[1]\n",
        "    print((x0 as i64))\n",
        "    print((x1 as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn left-scalar-div");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // 8 / [2,4] = [4,2] (REVERSED). A wrong `a / 8` would give [0.25,0.5] → "0\n0".
    assert_eq!(
        stdout.trim(),
        "4\n2".trim_end(),
        "expected REVERSED 8/[2,4] = [4,2] (NOT [2,4]/8 = [0.25,0.5]): c[0]='4', c[1]='2'; \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #5 (left-scalar `/`, REVERSED yields a FRACTION) — `3 / [2,4]`
/// → `[1.5, 0.75]`. Pins that the reversed `/` is numpy TRUE-division
/// (fractional float), not floor-division. Observe the WHOLE buffer via
/// `coil.print_buffer`, whose f64 repr renders `1.5` and `0.75` literally.
/// (A `(as i64)` truncation would collapse both to integers and miss the
/// fraction.)
///
/// Oracle (numpy 2.0.2): `3 / np.array([2.,4.])` → `array([1.5 , 0.75])`.
#[test]
fn test_e2e_left_scalar_div_true_division_fraction() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 4.0)\n", // [2,4]
        "    let c: coil.Buffer = 3 / a\n",                   // [1.5, 0.75]
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe)
        .output()
        .expect("spawn left-scalar-div-frac");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // 3 / [2,4] = [1.5, 0.75] — TRUE division, fractional. A floor/integer
    // division would print integers (no `.5` / `.75`).
    assert!(
        stdout.contains("1.5") && stdout.contains("0.75"),
        "expected REVERSED TRUE-division 3/[2,4] = [1.5, 0.75] (fractional floats); \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert!(
        !stdout.contains("dtype=int"),
        "true-division result must be a FLOAT array, not int; got stdout=\n{stdout}",
    );
}

/// Positive #6 (left-scalar `/` by zero → IEEE inf, NOT a trap) — `1 /
/// [0,2]` → `[inf, 0.5]`. Per IEEE 754, float `1.0/0.0 = +inf`; the reversed
/// `/` must produce `inf` (numpy RuntimeWarning, not an exception) and run
/// to completion (exit 0), NOT abort. Observe via `coil.print_buffer`,
/// whose f64 repr renders `inf` literally.
///
/// This pins that the REVERSED shim routes f64/0.0 to IEEE, not
/// `coil_panic` — the same div-by-zero contract as the right-scalar `/`.
///
/// Oracle (numpy 2.0.2): `1 / np.array([0.,2.])` → `array([inf, 0.5])`.
#[test]
fn test_e2e_left_scalar_div_by_zero_is_inf() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(0.0, 2.0)\n", // [0,2]
        "    let c: coil.Buffer = 1 / a\n",                   // [inf, 0.5]
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe)
        .output()
        .expect("spawn left-scalar-div-zero");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "reversed float div-by-zero must NOT trap (IEEE 754: 1.0/0.0 = inf); got non-zero exit. \
         stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert!(
        stdout.contains("inf"),
        "expected 1/0.0 → IEEE `inf` in the printed buffer; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — float scalar on the LEFT (the scalar need not be an int).
// =====================================================================

/// Positive #7 (left-scalar FLOAT `*`) — `0.5 * [2,4]` → `[1,2]`. The
/// left-scalar `k` may be a `Float` literal (passed straight as `f64`, no
/// i64→f64 cast). Observe `c[0]` → `1.0` → "1", `c[1]` → `2.0` → "2".
///
/// Oracle (numpy 2.0.2): `0.5 * np.array([2.,4.])` → `array([1.,2.])`.
#[test]
fn test_e2e_left_scalar_float_mul() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 4.0)\n", // [2,4]
        "    let c: coil.Buffer = 0.5 * a\n",                 // [1,2]
        "    let x0: f64 = c[0]\n",
        "    let x1: f64 = c[1]\n",
        "    print((x0 as i64))\n",
        "    print((x1 as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe)
        .output()
        .expect("spawn left-scalar-float-mul");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "1\n2".trim_end(),
        "expected 0.5*[2,4] = [1,2]: c[0]='1', c[1]='2'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// NEGATIVE / OUT-OF-SCOPE — `1 + s` (str) must still reject. The
// left-scalar guard must require the RHS to be a Buffer (it must not
// over-accept any `Int ⊕ X`).
// =====================================================================

/// Negative #1 — `1 + s` where `s: str` must be REJECTED at typecheck.
/// The left-scalar Buffer block keys on the RHS being a `coil.Buffer`; a
/// scalar LHS with a non-Buffer RHS must NOT match (it falls through to
/// `unify`, which rejects `Int + Str`). Expect exit 2. Guards that the new
/// left-scalar path did not loosen `Int ⊕ anything`.
#[test]
fn test_neg_left_scalar_int_plus_str_rejected() {
    let (ok, code, stderr) = try_check(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let s: str = \"x\"\n",
        "    let c: i64 = 1 + s\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "`1 + str` must be rejected (the left-scalar Buffer path requires a Buffer RHS); \
         stderr=\n{stderr}",
    );
    assert_eq!(
        code,
        Some(2),
        "expected TYPE_ERROR exit 2 for `1 + s`; got {code:?}; stderr=\n{stderr}",
    );
}
