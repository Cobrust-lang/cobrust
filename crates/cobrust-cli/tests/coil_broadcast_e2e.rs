//! ADR-0077 **Phase 3** (broadcasting) — `.cb` end-to-end proof obligation
//! for numpy broadcasting in `coil.Buffer` elementwise ops (`a + b` /
//! `a * b`).
//!
//! Phase 1 (`73c2747`) made `coil.ones(3) + coil.ones(3)` work, but the
//! shared shim body `buffer_binop` (`crates/cobrust-coil/src/cabi.rs:415`)
//! ABORTS on ANY shape difference — line 432:
//! `if lhs.shape() != rhs.shape() { coil_panic("... shape mismatch ...") }`
//! — including numpy-broadcastable ones. Empirically, at HEAD `3aa32ae`:
//!
//! ```text
//! coil.ones(3) + coil.ones(1)   # builds, then ABORTS at run (exit 3):
//!   "coil.Buffer add: shape mismatch [3] vs [1] (Phase 1 requires
//!    same-shape operands; broadcasting is deferred to Phase 2)"
//! ```
//!
//! numpy broadcasts `(3,) + (1,)` to `(3,)` (`[1,1,1] + [10] = [11,11,11]`).
//! Phase 3 relaxes the `cabi.rs:432` guard to only abort when
//! `broadcast_shape(a, b).is_err()`, letting the already-broadcasting
//! `Array::add` kernel (`ufunc.rs:353`) run. The canonical 2-D doc cases
//! (`(3,1)+(1,4)`) need shapes the `.cb` constructors cannot build (all
//! `.cb` ctors are 1-D or `n×n` identity) — those live in the Rust
//! `broadcast_elementwise_corpus.rs` sibling. This file pins the
//! `.cb`-buildable `(N,)+(1,)` broadcast + the same-shape no-regression +
//! the incompatible-traps boundary.
//!
//! TEST-FIRST status (ADSD). The broadcast positives are RED at HEAD
//! (build succeeds — shape is invisible to the type — but the run aborts on
//! the same-shape guard). The same-shape case and the incompatible-traps
//! case are GREEN at HEAD (the baselines). NONE are `#[ignore]`d: corpus +
//! impl land atomically. Mirrors the compile→spawn→assert-stdout harness of
//! `coil_ops_e2e.rs`; reads are observed via `(x as i64)` casts to dodge
//! f64 print-format drift.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional prose, not lint targets.
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

/// Build-only helper — `(build_succeeded, stderr)`. Used by the runtime-
/// error negatives (shape is invisible to the type → they BUILD then FAIL
/// at run).
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
// POSITIVE — broadcasting `(N,) + (1,)` (RED at HEAD; numpy broadcasts).
// =====================================================================

/// Positive #1 — `coil.ones(3) + coil.ones(1)` broadcasts `(3,) + (1,)` to
/// `(3,)`: `[1,1,1] + [1] = [2,2,2]`. Observe via `coil.mean(c)` →
/// `(2+2+2)/3 = 2.0` → `(2.0 as i64)` → "2".
///
/// Oracle (numpy 2.0.2): `np.ones(3) + np.ones(1)` → `array([2.,2.,2.])`.
///
/// PROOF OBLIGATION: at HEAD this BUILDS (both are `coil.Buffer` — shape is
/// not in the type) but ABORTS at run (exit 3) on the `cabi.rs:432`
/// same-shape guard. RED. Phase 3 makes the guard broadcast-aware so this
/// runs and prints "2".
#[test]
fn test_e2e_broadcast_3_plus_1_mean_is_two() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let b: coil.Buffer = coil.ones(1)\n",
        "    let c: coil.Buffer = a + b\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn broadcast-3-1");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); broadcasting (3,)+(1,) must NOT trap — numpy \
         broadcasts to (3,). stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "2",
        "expected ones(3)+ones(1) → [2,2,2], mean 2.0 → '2'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #2 — broadcasting with a NON-uniform LHS pins the broadcast
/// VALUES, not just a uniform mean. `coil.mgrid(0, 4)` → `[0,1,2,3]` (shape
/// `(4,)`); `coil.ones(1)` → `[1]` (shape `(1,)`); `a + b` broadcasts to
/// `[1,2,3,4]`. Read the LAST element `c[3]` → `f64` `4.0` →
/// `(4.0 as i64)` → "4". A broadcast that wrongly added element-0 of `b`
/// only to element-0 of `a` (no broadcast) would leave `c[3]` undefined /
/// trap; a correct broadcast adds the single `b` element to EVERY `a`
/// element.
///
/// Oracle (numpy 2.0.2): `np.arange(4.) + np.ones(1)` →
/// `array([1.,2.,3.,4.])`; element `[3]` == `4.0`.
#[test]
fn test_e2e_broadcast_nonuniform_value_at_index() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 4)\n",
        "    let b: coil.Buffer = coil.ones(1)\n",
        "    let c: coil.Buffer = a + b\n",
        "    let x: f64 = c[3]\n",
        "    print((x as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe)
        .output()
        .expect("spawn broadcast-nonuniform");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); mgrid(0,4)+ones(1) must broadcast to [1,2,3,4]. \
         stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "4",
        "expected (mgrid(0,4)+ones(1))[3] == 4.0 → '4'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #3 — broadcasting also works for `*` (proves the relaxed guard
/// lives in the shared `buffer_binop` body, not bolted onto `add`).
/// `coil.mgrid(0, 4)` → `[0,1,2,3]`; `coil.ones(1)` → `[1]` is the
/// multiplicative identity, so `a * b` broadcasts to `[0,1,2,3]` unchanged;
/// observe `coil.mean(c)` → `(0+1+2+3)/4 = 1.5` → `(1.5 as i64)` → "1".
/// (Distinct op + a `*` that wrongly trapped on the `(4,)` vs `(1,)` shape
/// difference would fail to even run.)
///
/// Oracle (numpy 2.0.2): `np.arange(4.) * np.ones(1)` →
/// `array([0.,1.,2.,3.])`; mean `1.5`.
#[test]
fn test_e2e_broadcast_mul_4_by_1_mean_is_one() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 4)\n",
        "    let b: coil.Buffer = coil.ones(1)\n",
        "    let c: coil.Buffer = a * b\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn broadcast-mul");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); mgrid(0,4)*ones(1) must broadcast (not trap). \
         stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "1",
        "expected mean(mgrid(0,4)*ones(1)) == 1.5 → '1'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// NO-REGRESSION — same-shape add must STILL work (GREEN at HEAD).
// =====================================================================

/// No-regression — `coil.ones(3) + coil.ones(3)` (equal shapes, the Phase-1
/// path) must STAY green after the guard relaxation. `[1,1,1]+[1,1,1] =
/// [2,2,2]`; `coil.mean(c)` → `2.0` → "2". (Verbatim the Phase-1
/// `test_e2e_buffer_add_then_mean` shape — duplicated here so this file's
/// regression baseline is self-contained.)
#[test]
fn test_e2e_same_shape_add_no_regression() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let c: coil.Buffer = coil.ones(3) + coil.ones(3)\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn same-shape-add");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); same-shape add must stay green. \
         stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "2",
        "expected ones(3)+ones(3) → [2,2,2], mean 2.0 → '2'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// RUNTIME error — genuinely INCOMPATIBLE shapes must STILL trap (handles
// carry no shape in the type → runtime, not compile, error). numpy also
// rejects these. Build-succeeds + run-FAILS.
// =====================================================================

/// Negative #1 — `coil.ones(3) + coil.ones(4)` is incompatible: `3` vs `4`
/// (neither equal nor 1) is NOT broadcastable. numpy raises
/// `operands could not be broadcast together with shapes (3,) (4,)`. The
/// shim MUST abort (non-zero exit) — both at HEAD (today's blanket
/// same-shape guard) and after Phase 3 (now via `broadcast_shape.is_err()`).
/// The Phase-3 DEV must keep this RED-trapping: the relaxed guard must
/// reject `[3]` vs `[4]`, not silently produce a wrong-shaped buffer.
///
/// Build-succeeds + run-FAILS (asserts `!success`, not a specific exit code,
/// for robustness to the abort convention).
#[test]
fn test_runtime_incompatible_3_plus_4_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let b: coil.Buffer = coil.ones(4)\n",
        "    let c: coil.Buffer = a + b\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (built, build_stderr) = try_build(source);
    assert!(
        built,
        "incompatible-shape add must BUILD (shape is not part of the type); \
         build stderr=\n{build_stderr}",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn incompat-3-4");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "incompatible `ones(3)+ones(4)` (3 vs 4, not broadcastable) must TRAP at runtime \
         (non-zero exit); got success. A relaxed broadcast guard must STILL reject this. \
         stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Negative #2 — a different incompatible 1-D pair `coil.mgrid(0, 5)` (shape
/// `(5,)`) `+ coil.ones(2)` (shape `(2,)`): `5` vs `2`, not broadcastable.
/// numpy raises. Must abort. Pins that the trap is "non-broadcastable in
/// general", not a one-off for `(3,)+(4,)`. Build-succeeds + run-FAILS.
#[test]
fn test_runtime_incompatible_5_plus_2_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 5)\n",
        "    let b: coil.Buffer = coil.ones(2)\n",
        "    let c: coil.Buffer = a + b\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (built, build_stderr) = try_build(source);
    assert!(
        built,
        "incompatible-shape add must BUILD (shape is not part of the type); \
         build stderr=\n{build_stderr}",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn incompat-5-2");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "incompatible `mgrid(0,5)+ones(2)` (5 vs 2, not broadcastable) must TRAP at runtime \
         (non-zero exit); got success. stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}
