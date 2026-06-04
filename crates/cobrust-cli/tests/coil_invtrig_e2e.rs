//! coil unary INVERSE trig / hyperbolic ufuncs (`arcsin` / `arccos` /
//! `arctan` / `arcsinh` / `arccosh` / `arctanh`) — `.cb` end-to-end proof
//! for the #145 BATCH-16 addition, COMPLETING the unary transcendental
//! family (the documented BATCH-3 deferral; BATCH 15 shipped the 2-arg
//! `arctan2`). Wired EXACTLY like the BATCH-3 forward transcendentals
//! (`coil_ufunc_e2e.rs`) + BATCH-4 rounding ops (`coil_round_e2e.rs`):
//! borrow-Buffer-arg → fresh-Buffer-return, FLOAT-promoting.
//!
//! ## The load-bearing semantics (numpy-exact, the correctness focus)
//!
//! - **Reference values** (radians): `arcsin(1) = π/2`, `arccos(0) = π/2`,
//!   `arccos(1) = 0`, `arctan(1) = π/4`. These render via the coil repr
//!   (`f64::to_string`) as `1.5707963267948966` (π/2) and
//!   `0.7853981633974483` (π/4).
//! - **DOMAIN -> NaN** (the #1 correctness nuance): these are PARTIAL
//!   functions. An out-of-domain input is a `NaN` VALUE — numpy emits a
//!   RuntimeWarning but the array value IS `NaN`, NOT an error / trap. The
//!   cabi shim NEVER `coil_panic`s on an out-of-domain input. `arcsin` /
//!   `arccos` domain is `[-1, 1]` (`arcsin(2) = NaN`); the coil repr prints
//!   `NaN`. The binary RUNS to exit 0 — the NaN flows through as a value.
//! - **arctanh boundary**: `arctanh(±1) = ±inf` (the repr prints `inf`).
//!
//! ## DTYPE (FLOAT-promoting, the BATCH-3 transcendental rule)
//!
//! int / bool -> `Float64`, `Float32` stays `Float32`, `Float64` stays
//! `Float64`. Every `.cb` coil constructor (`array1d2` / `array2x2`)
//! yields a `Float64` buffer, so these ops return `Float64` here. (The
//! int->f64 promotion + f32-stays-f32 contracts are exhaustively pinned in
//! the `elementwise.rs` Rust unit tests; here we prove the float-DTYPE
//! end-to-end value + domain-NaN contract those rules ultimately serve.)
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     <op>)` resolves the `Buffer(...) -> Buffer` `EcoSig`;
//!   - MIR `try_lower_ecosystem_call` Case-1 (module free fn) → the GENERIC
//!     `emit_ecosystem_call` borrow-arg → fresh-Buffer-return path (the
//!     SAME path `coil.exp(a)` / `coil.transpose(a)` prove; NO BATCH-16-
//!     specific MIR arm, NO `_=>"any"` gap — the 1-Buffer-arg shape is
//!     identical);
//!   - codegen externs (`llvm_backend.rs`) — `(ptr) -> ptr` ≡
//!     `coil_shape_ty`, identical to `exp` / `transpose`;
//!   - the cabi shims (`cabi.rs`) — `__cobrust_coil_<op>` returning a fresh
//!     Boxed `Buffer` via the shared `buffer_unary` body (TOTAL — no
//!     `coil_panic` path; out-of-domain is a NaN value, never unwinds).
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_round_e2e.rs`.
//! Results are observed via `coil.print_buffer`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_round_e2e.rs`.
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

/// Spawn a compiled program; return `(stdout, stderr, success)`.
fn run(exe: &PathBuf) -> (String, String, bool) {
    let out = Command::new(exe).output().expect("spawn coil-invtrig prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — `arcsin` reference values. `arcsin(1) = π/2`, `arcsin(0) = 0`.
// The π/2 renders as `1.5707963267948966` (numpy-exact, IEEE-754).
// =====================================================================

/// `coil.arcsin(array1d2(1.0, 0.0))` → `[π/2, 0]` (Float64). Oracle (numpy
/// 2.x): `np.arcsin([1.0, 0.0])` → `array([1.5707963..., 0.])`.
#[test]
fn test_e2e_arcsin_pi_over_two() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 0.0)\n",
        "    let r: coil.Buffer = coil.arcsin(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1.5707963267948966, 0], dtype=float64)"),
        "expected arcsin([1,0])=[pi/2, 0]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `arccos` reference values. `arccos(0) = π/2`, `arccos(1) = 0`.
// arccos is the COMPLEMENT of arcsin — lane order differs from arcsin.
// =====================================================================

/// `coil.arccos(array1d2(1.0, 0.0))` → `[0, π/2]` (Float64). Oracle:
/// `np.arccos([1.0, 0.0])` → `array([0., 1.5707963...])`. Note the lanes
/// are the MIRROR of arcsin's (`arccos(1)=0` where `arcsin(1)=π/2`).
#[test]
fn test_e2e_arccos_zero_and_pi_over_two() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 0.0)\n",
        "    let r: coil.Buffer = coil.arccos(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([0, 1.5707963267948966], dtype=float64)"),
        "expected arccos([1,0])=[0, pi/2]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `arctan` reference value. `arctan(1) = π/4`, `arctan(0) = 0`.
// UNLIKE 2-arg arctan2, single-arg arctan cannot disambiguate quadrant.
// The π/4 renders as `0.7853981633974483`.
// =====================================================================

/// `coil.arctan(array1d2(1.0, 0.0))` → `[π/4, 0]` (Float64). Oracle:
/// `np.arctan([1.0, 0.0])` → `array([0.7853981..., 0.])`.
#[test]
fn test_e2e_arctan_pi_over_four() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 0.0)\n",
        "    let r: coil.Buffer = coil.arctan(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([0.7853981633974483, 0], dtype=float64)"),
        "expected arctan([1,0])=[pi/4, 0]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — DOMAIN -> NaN (the #1 correctness nuance). `arcsin(2) = NaN`
// (domain is [-1, 1]). The NaN is a VALUE — the binary RUNS to exit 0 and
// the repr prints `NaN`; it is NEVER a trap / error / non-zero exit.
// =====================================================================

/// `coil.arcsin(array1d2(2.0, -2.0))` → `[NaN, NaN]` (Float64, exit 0).
/// Oracle: `np.arcsin([2.0, -2.0])` → `array([nan, nan])` (with a
/// RuntimeWarning; the VALUE is NaN). The load-bearing assertion: the
/// program exits 0 (no domain trap) AND the output contains `NaN`.
#[test]
fn test_e2e_arcsin_out_of_domain_is_nan_value_not_trap() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, -2.0)\n",
        "    let r: coil.Buffer = coil.arcsin(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    // THE key assertion: out-of-domain does NOT crash — exit 0 (NaN is a
    // value, the shim never coil_panics on an out-of-domain input).
    assert!(
        ok,
        "arcsin(2) must NOT trap — it is a NaN VALUE (exit 0); stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert!(
        stdout.contains("NaN") && stdout.contains("dtype=float64"),
        "expected arcsin([2,-2]) to print a NaN value (domain [-1,1]); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `arctanh(1) = +inf` (boundary). `arctanh` domain is (-1, 1);
// the boundary ±1 yields ±inf (a VALUE, not a trap). The repr prints `inf`.
// =====================================================================

/// `coil.arctanh(array1d2(0.0, 1.0))` → `[0, +inf]` (Float64, exit 0).
/// Oracle: `np.arctanh([0.0, 1.0])` → `array([0., inf])`. `arctanh(0)=0`,
/// `arctanh(1)=+inf` (boundary — a value, never an error).
#[test]
fn test_e2e_arctanh_boundary_is_inf() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(0.0, 1.0)\n",
        "    let r: coil.Buffer = coil.arctanh(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([0, inf], dtype=float64)"),
        "expected arctanh([0,1])=[0, inf] (boundary +inf, a value); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `arcsinh` / `arccosh` reference values. `arcsinh(0)=0`,
// `arccosh(1)=0`. The two zero-points pinned together on a 2x2 buffer
// (also proves shape preservation through the inverse-hyperbolic ops).
// =====================================================================

/// `coil.arcsinh(array2x2(0.0, 0.0, 0.0, 0.0))` → `[[0, 0], [0, 0]]`.
/// Oracle: `np.arcsinh([[0,0],[0,0]])` → all-zero. Pins `arcsinh(0)=0` +
/// the `(2,2)` shape preservation.
#[test]
fn test_e2e_arcsinh_zero_2x2_shape() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(0.0, 0.0, 0.0, 0.0)\n",
        "    let r: coil.Buffer = coil.arcsinh(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[0, 0], [0, 0]]") && stdout.contains("dtype=float64"),
        "expected arcsinh(0)=0 on a 2x2 (shape preserved); got stdout=\n{stdout}",
    );
}

/// `coil.arccosh(array1d2(1.0, 1.0))` → `[0, 0]` (Float64). Oracle:
/// `np.arccosh([1.0, 1.0])` → `array([0., 0.])`. `arccosh(1)=0` (the lower
/// boundary of the `[1, inf)` domain).
#[test]
fn test_e2e_arccosh_one_is_zero() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 1.0)\n",
        "    let r: coil.Buffer = coil.arccosh(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([0, 0], dtype=float64)"),
        "expected arccosh([1,1])=[0, 0] (domain boundary); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — CHAIN `sin(arcsin(a)) ~ a` (the classic round-trip). Proves
// the fresh-Buffer return of the inner inverse op feeds the forward op AND
// that the inner temporary is dropped exactly once at scope exit (no leak /
// no double-free). For a in [-1, 1], sin∘arcsin is the identity.
// =====================================================================

/// `coil.sin(coil.arcsin(array1d2(0.5, -0.25)))` → `[0.5, -0.25]`.
///
/// The intermediate `coil.arcsin(...)` Buffer is a fresh handle consumed
/// by `coil.sin`; both the intermediate and the final are drop-scheduled by
/// the `.cb` scope. Oracle: `np.sin(np.arcsin([0.5, -0.25]))` →
/// `array([0.5, -0.25])` (modulo IEEE rounding; `0.5` is exact, `-0.25` is
/// exact). The round-trip identity is the proof the inverse op is correct.
#[test]
fn test_e2e_chain_sin_of_arcsin_round_trips() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(0.5, -0.25)\n",
        "    let r: coil.Buffer = coil.sin(coil.arcsin(a))\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    // sin(arcsin(0.5)) round-trips to ~0.5, but the LAST ULP is
    // PLATFORM-DEPENDENT: macOS libm gives 0.49999999999999994, ubuntu gives
    // exactly 0.5 (the inner arcsin + outer sin each introduce one ULP, and the
    // two platforms' libm round the boundary differently). Accept BOTH forms —
    // the round-trip IDENTITY (~0.5) is the proof, not a platform-specific last
    // digit. sin(arcsin(-0.25)) = -0.25 (exact on both).
    // [transcendental e2e last-ULP lesson — assert the identity, not the exact
    // float string; cf. the batch-13 ±0.0 platform-determinism fix.]
    let first_ok = stdout.contains("0.49999999999999") || stdout.contains("0.5,");
    assert!(
        first_ok && stdout.contains("-0.25"),
        "expected sin(arcsin([0.5,-0.25])) ~ [0.5, -0.25] (round-trip identity); \
         got stdout=\n{stdout}",
    );
}
