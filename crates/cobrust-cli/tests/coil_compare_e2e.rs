//! coil buffer-buffer COMPARISON `a cmp b` — `.cb` end-to-end proof for
//! ADR-0077 Phase-2/3 addition (B): the six element-wise comparison
//! operators `<` / `<=` / `>` / `>=` / `==` / `!=` on two `coil.Buffer`s.
//!
//! ## The load-bearing semantic: NumPy mask, NOT a Cobrust bool scalar
//!
//! `a < b` on two arrays is an ELEMENT-WISE comparison yielding a
//! `coil.Buffer` of dtype **Bool** (a NumPy mask) — NOT a single Cobrust
//! `bool`. `np.array([1,5]) < np.array([3,2])` is `array([True, False])`,
//! not `False`. The result is bindable as `let m: coil.Buffer = a < b` and
//! prints as `array([True, False], dtype=bool)` (the `dtype=bool` token is
//! the discriminator vs. an int / float array).
//!
//! This is why the typecheck guard lives in the COMPARISON arm of
//! `synth_bin` (not the arithmetic arm), returning `coil_buffer_ty()`
//! instead of the usual `Ty::Bool`: a Buffer DOES unify with a Buffer, so
//! without the guard the comparison would mis-type as a scalar bool and
//! mis-compile (codegen's comparison arm assumes int operands).
//!
//! ## Where this sits in the chain (reuses the `+`/`-`/`*`/`/` machinery)
//!
//!   - typecheck `synth_bin` COMPARISON arm — a Buffer-vs-Buffer guard
//!     returning `coil_buffer_ty()` (a Buffer-vs-scalar `a < 1` is a §12
//!     deferral that prints a §2.5 FIX instead);
//!   - MIR retarget (`lower.rs`) — the SAME `lookup_buffer_binop` path as
//!     the arithmetic ops; comparison ops reach it unintercepted and
//!     retarget to `__cobrust_coil_buffer_{lt,le,gt,ge,eq,ne}`;
//!   - codegen externs (`llvm_backend.rs`) — six `(ptr, ptr) -> ptr` rows;
//!   - the cabi shims (`cabi.rs`) — each forwards through the shared
//!     broadcast-aware `buffer_binop` body onto `Array::{lt,le,gt,ge,eq_,
//!     ne_}`, which ALWAYS return a `Dtype::Bool` array;
//!   - manifest `lookup_buffer_binop` (`ecosystem.rs`) — six new arms.
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_ops_e2e.rs`.
//! The bool mask is observed via `coil.print_buffer`, whose Bool repr
//! renders `True` / `False` + `dtype=bool` (Rust `Display`, print.rs:37).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. The caller spawns + asserts. Mirrors `coil_ops_e2e.rs`.
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
// POSITIVE — each of the six comparison ops yields a Bool-dtype mask.
// `a = [1,5]`, `b = [3,2]` (one element each way) so EVERY op produces a
// MIXED mask (one True, one False) — a uniform/swapped result would FAIL.
// =====================================================================

/// Positive #1 (`<`) — `[1,5] < [3,2]` → `[True, False]` (1<3 True, 5<2
/// False). The mixed mask (NOT all-True / all-False) rules out a degenerate
/// fill. Bound as a `coil.Buffer` and printed: must render `True, False`
/// AND `dtype=bool` (the "it's a mask, not an int array" discriminator).
///
/// Oracle (numpy 2.0.2): `np.array([1.,5.]) < np.array([3.,2.])` →
/// `array([ True, False])`.
#[test]
fn test_e2e_buffer_lt_mask() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 5.0)\n",
        "    let b: coil.Buffer = coil.array1d2(3.0, 2.0)\n",
        "    let m: coil.Buffer = a < b\n",
        "    let _ = coil.print_buffer(m)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn lt-mask");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert!(
        stdout.contains("True") && stdout.contains("False") && stdout.contains("dtype=bool"),
        "expected [1,5]<[3,2] = [True, False] as a bool-dtype mask; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    // Pin the exact order: True BEFORE False (1<3 True, 5<2 False) — a
    // swapped/reversed comparison would print `[False, True]`.
    let t = stdout.find("True");
    let f = stdout.find("False");
    assert!(
        t < f,
        "expected mask order [True, False] (1<3 then 5<2); got stdout=\n{stdout}",
    );
}

/// Positive #2 (`>`) — `[1,5] > [3,2]` → `[False, True]` (1>3 False, 5>2
/// True). The MIRROR of `<`: the mask order flips. Pins `>` is not aliased
/// to `<`.
///
/// Oracle (numpy 2.0.2): `np.array([1.,5.]) > np.array([3.,2.])` →
/// `array([False,  True])`.
#[test]
fn test_e2e_buffer_gt_mask() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 5.0)\n",
        "    let b: coil.Buffer = coil.array1d2(3.0, 2.0)\n",
        "    let m: coil.Buffer = a > b\n",
        "    let _ = coil.print_buffer(m)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn gt-mask");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert!(
        stdout.contains("dtype=bool"),
        "expected a bool-dtype mask; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    // [False, True]: False BEFORE True (1>3 False, 5>2 True).
    let f = stdout.find("False");
    let t = stdout.find("True");
    assert!(
        f < t,
        "expected mask order [False, True] (1>3 then 5>2); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #3 (`<=`) — `[2,2] <= [2,1]` → `[True, False]` (2<=2 True at
/// the EQUAL boundary, 2<=1 False). Pins `<=` includes equality (a plain
/// `<` would give `[False, False]` at the `2<=2` element).
///
/// Oracle (numpy 2.0.2): `np.array([2.,2.]) <= np.array([2.,1.])` →
/// `array([ True, False])`.
#[test]
fn test_e2e_buffer_le_includes_equal() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 2.0)\n",
        "    let b: coil.Buffer = coil.array1d2(2.0, 1.0)\n",
        "    let m: coil.Buffer = a <= b\n",
        "    let _ = coil.print_buffer(m)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn le-mask");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert!(
        stdout.contains("dtype=bool"),
        "expected a bool-dtype mask; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    // [True, False]: the 2<=2 element is True (equality included).
    let t = stdout.find("True");
    let f = stdout.find("False");
    assert!(
        t < f,
        "expected `<=` to include the equal boundary → [True, False] (2<=2 then 2<=1); \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #4 (`>=`) — `[2,0] >= [2,1]` → `[True, False]` (2>=2 True at the
/// EQUAL boundary, 0>=1 False). Pins `>=` includes equality.
///
/// Oracle (numpy 2.0.2): `np.array([2.,0.]) >= np.array([2.,1.])` →
/// `array([ True, False])`.
#[test]
fn test_e2e_buffer_ge_includes_equal() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 0.0)\n",
        "    let b: coil.Buffer = coil.array1d2(2.0, 1.0)\n",
        "    let m: coil.Buffer = a >= b\n",
        "    let _ = coil.print_buffer(m)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn ge-mask");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert!(
        stdout.contains("dtype=bool"),
        "expected a bool-dtype mask; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    let t = stdout.find("True");
    let f = stdout.find("False");
    assert!(
        t < f,
        "expected `>=` to include the equal boundary → [True, False] (2>=2 then 0>=1); \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #5 (`==`) — `[1,5] == [1,2]` → `[True, False]`. THE "==-on-
/// arrays-is-a-mask-not-a-scalar-bool" case: numpy's `==` is element-wise,
/// so the result is `[True, False]` (NOT a single `False`). A naive
/// scalar-bool lowering would be a type error here (a `coil.Buffer`
/// binding cannot hold a `bool`).
///
/// Oracle (numpy 2.0.2): `np.array([1.,5.]) == np.array([1.,2.])` →
/// `array([ True, False])`.
#[test]
fn test_e2e_buffer_eq_is_elementwise_mask() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 5.0)\n",
        "    let b: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let m: coil.Buffer = a == b\n",
        "    let _ = coil.print_buffer(m)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn eq-mask");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert!(
        stdout.contains("dtype=bool"),
        "expected `==` to yield a bool-dtype MASK (element-wise), not a scalar bool; \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    // [True, False]: 1==1 True, 5==2 False.
    let t = stdout.find("True");
    let f = stdout.find("False");
    assert!(
        t < f,
        "expected [1,5]==[1,2] = [True, False]; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #6 (`!=`) — `[1,5] != [1,2]` → `[False, True]` (the exact
/// inverse of `==` above). Pins `!=` is the negation mask, not aliased to
/// `==`.
///
/// Oracle (numpy 2.0.2): `np.array([1.,5.]) != np.array([1.,2.])` →
/// `array([False,  True])`.
#[test]
fn test_e2e_buffer_ne_is_inverse_of_eq() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 5.0)\n",
        "    let b: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let m: coil.Buffer = a != b\n",
        "    let _ = coil.print_buffer(m)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn ne-mask");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert!(
        stdout.contains("dtype=bool"),
        "expected a bool-dtype mask; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    // [False, True]: 1!=1 False, 5!=2 True (inverse of the `==` case).
    let f = stdout.find("False");
    let t = stdout.find("True");
    assert!(
        f < t,
        "expected [1,5]!=[1,2] = [False, True] (inverse of ==); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — comparison broadcasts like the arithmetic ops (shared body).
// =====================================================================

/// Positive #7 (`<` broadcasts `(3,)` vs `(1,)`) — `[0,1,2] < [1]` →
/// `[True, False, False]` (0<1 True, 1<1 False, 2<1 False). Pins that
/// comparison flows through the SAME broadcast-aware `buffer_binop` body
/// as `+`/`-`/`*`/`/` (a length-1 RHS stretches across the (3,) LHS).
///
/// Oracle (numpy 2.0.2): `np.array([0.,1.,2.]) < np.ones(1)` →
/// `array([ True, False, False])`.
#[test]
fn test_e2e_buffer_lt_broadcast() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 3)\n", // [0,1,2], shape (3,)
        "    let b: coil.Buffer = coil.ones(1)\n",     // [1], shape (1,)
        "    let m: coil.Buffer = a < b\n",            // broadcast -> [True, False, False]
        "    let _ = coil.print_buffer(m)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn lt-broadcast");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); comparison broadcast (3,)<(1,) must not trap. \
         stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert!(
        stdout.contains("dtype=bool"),
        "expected a bool-dtype mask; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    // [True, False, False]: exactly one True (the `0 < 1` element), first.
    assert_eq!(
        stdout.matches("True").count(),
        1,
        "expected exactly one True in [0,1,2]<[1] = [True, False, False]; \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert_eq!(
        stdout.matches("False").count(),
        2,
        "expected exactly two False in [0,1,2]<[1]; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// CHECK-only POSITIVE — comparison via the EXPLICIT-borrow form `&a < &b`
// (the LLM-idiomatic non-Copy reuse pattern) typechecks identically.
// =====================================================================

/// Positive #8 (borrow form) — `&a < &b` typechecks and binds to a
/// `coil.Buffer` (the explicit-borrow form resolves through the same
/// `lookup_buffer_binop` Ref-unwrap as the bare `a < b`). A `check`-only
/// assertion (the runtime values are pinned by #1).
#[test]
fn test_check_buffer_compare_borrow_form() {
    let (ok, code, stderr) = try_check(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let b: coil.Buffer = coil.ones(3)\n",
        "    let m: coil.Buffer = &a < &b\n",
        "    let _ = coil.print_buffer(m)\n",
        "    return 0\n",
    ));
    assert!(
        ok,
        "`&a < &b` (explicit-borrow comparison) must typecheck to a coil.Buffer; \
         code={code:?}; stderr=\n{stderr}",
    );
}

// =====================================================================
// NEGATIVE / OUT-OF-SCOPE — buffer-vs-SCALAR comparison `a < 1` is a §12
// deferral and must reject with a §2.5 FIX (not a generic unify error).
// =====================================================================

/// Negative #1 (out-of-scope) — `a < 1` (Buffer vs scalar) must be
/// REJECTED at typecheck. Buffer-vs-scalar comparison is an explicit
/// follow-up deferral; the comparison guard requires BOTH operands to be a
/// Buffer. Expect exit 2 AND a §2.5 fix-printing diagnostic that names the
/// requirement (so the LLM knows the FIX, not just the diagnosis).
#[test]
fn test_neg_buffer_vs_scalar_compare_rejected_with_fix() {
    let (ok, code, stderr) = try_check(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let m: coil.Buffer = a < 1\n",
        "    let _ = coil.print_buffer(m)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "`a < 1` (buffer vs scalar comparison) must be rejected (out-of-scope deferral); \
         stderr=\n{stderr}",
    );
    assert_eq!(
        code,
        Some(2),
        "expected TYPE_ERROR exit 2 for `a < 1`; got {code:?}; stderr=\n{stderr}",
    );
    // §2.5-B — the diagnostic must PRINT THE FIX, not just the diagnosis.
    assert!(
        stderr.contains("coil.Buffer") && stderr.contains("scalar"),
        "expected a §2.5 fix naming the Buffer-vs-scalar restriction; got stderr=\n{stderr}",
    );
}

/// Negative #2 (out-of-scope, mirror) — `1 < a` (scalar vs Buffer) is the
/// LEFT-operand mirror and must ALSO reject with a §2.5 fix. Pins that the
/// guard catches a Buffer on EITHER side, not only the LHS.
#[test]
fn test_neg_scalar_vs_buffer_compare_rejected() {
    let (ok, code, stderr) = try_check(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let m: coil.Buffer = 1 < a\n",
        "    let _ = coil.print_buffer(m)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "`1 < a` (scalar vs buffer comparison) must be rejected (out-of-scope deferral); \
         stderr=\n{stderr}",
    );
    assert_eq!(
        code,
        Some(2),
        "expected TYPE_ERROR exit 2 for `1 < a`; got {code:?}; stderr=\n{stderr}",
    );
}
