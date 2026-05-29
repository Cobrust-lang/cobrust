//! ADR-0079 **Phase 1** — `.cb` end-to-end proof obligation for the FIRST
//! *dotted sub-namespace* under an ecosystem module: `coil.linalg.*`
//! (`coil.linalg.solve(a, b)` / `coil.linalg.det(a)` / `coil.linalg.inv(a)`),
//! mirroring numpy's `np.linalg.*` idiom (ADR-0079 Q4-a — a manifest-
//! namespaced flat symbol `__cobrust_coil_linalg_<fn>`, NOT a bindable
//! handle). The underlying numerical kernels ALREADY EXIST + pass the
//! ADR-0017 `rtol=1e-6` differential gate (`coil::linalg::{solve@464,
//! det@427, inv@503}` in `crates/cobrust-coil/src/linalg.rs`); Phase-1's
//! work is purely the `.cb`-surface + dotted-namespace resolver + cabi
//! wiring (ADR-0079 §8 implementation map). Zero new numerical code.
//!
//! ================================================================
//! DEV PREREQUISITE — THE 2-D MATRIX CONSTRUCTOR GAP (READ FIRST)
//! ================================================================
//! `coil.linalg.{solve,det,inv}` operate on **2-D matrices** (`Array`
//! rank-2). But coil's `.cb` constructor surface today is almost entirely
//! **1-D** (verified at HEAD `9c8f82c` against `cobrust-types/src/
//! ecosystem.rs` + `cobrust-coil/src/cabi.rs`):
//!
//!   - `coil.zeros(n)` / `coil.ones(n)` / `coil.mgrid(a,b)` / `coil.ogrid(a,b)`
//!     all build **1-D** buffers.
//!   - **`coil.eye(n)` is the ONLY `.cb`-constructible 2-D matrix** — and it
//!     builds ONLY the `n x n` identity `I` (`cabi.rs:193` → `coil::eye`).
//!   - There is **NO** `.cb` constructor that takes matrix *element data*:
//!     every coil shim takes scalar `i64`/`f64` args (no `list[f64]`→coil
//!     marshalling, no shape arg, no `reshape`). `coil::array_f64(values,
//!     shape)` exists as a Rust `pub fn` but is **NOT exposed at `.cb`**.
//!   - `a[i] = v` setitem is **1-D** (normalises against `shape()[0]` only),
//!     so a 2-D matrix CANNOT be built by mutating `coil.eye(n)` either.
//!
//! Consequence — this corpus is split into two tiers:
//!
//!   * **Tier A (identity-only positives)** use `coil.eye(n)` ALONE and need
//!     NO new constructor: `det(eye(3)) == 1`, `solve(eye(3), b) == b`,
//!     `inv(eye(2)) == eye(2)`. These pin the CORE `coil.linalg.*` wiring
//!     (dotted resolver + 3 shims) and are turnable-green WITHOUT touching
//!     the constructor surface. They are degenerate (no pivoting / no
//!     non-trivial determinant), so they are necessary-but-not-sufficient.
//!
//!   * **Tier B/C (non-trivial positives + runtime negatives)** REQUIRE a
//!     non-identity 2-D matrix — `det([[1,2],[3,4]]) == -2`, a 2x2 solve
//!     with a known answer, a non-identity inverse, a singular-matrix
//!     solve/inv panic, a non-square det panic. **None of these is
//!     constructible at `.cb` today.** They are written against PROPOSED
//!     minimal data constructors that mirror the existing all-scalar-arg
//!     shim convention (no `list[f64]` marshalling required — the cheapest
//!     path; each delegates to the EXISTING Rust `coil::array_f64(values,
//!     shape)`):
//!       - `coil.array2x2(a, b, c, d) -> Buffer` — row-major `2 x 2`
//!         (→ `array_f64(&[a,b,c,d], &[2,2])`).
//!       - `coil.array2x3(a, b, c, d, e, f) -> Buffer` — row-major `2 x 3`,
//!         for the non-square det negative (→ `array_f64(&[..], &[2,3])`).
//!       - `coil.array1d2(a, b) -> Buffer` — a 2-element 1-D vector with
//!         explicit data, for an arbitrary RHS like `[5,11]` / `[1,1]`
//!         (`coil.ones`/`coil.mgrid` cannot make arbitrary values).
//!     **The DEV MUST either add these constructors (the cheapest path, ~3
//!     trivial shims over the existing `array_f64`) OR pick an equivalent
//!     2-D/1-D data surface (e.g. a list-based `coil.matrix([[..],[..]])` +
//!     `coil.array([..])` once `list[f64]`→coil marshalling lands) and
//!     re-spell these cases against it.** Either way, a real matrix-data
//!     constructor is a genuine Phase-1 prerequisite the DEV cannot skip —
//!     the strong numerical proofs (pivoting, singular detection, shape
//!     checks) are un-exercisable on the identity alone.
//!
//! ================================================================
//! TEST-FIRST status (ADSD) — RED confirmed empirically at HEAD `9c8f82c`
//! ================================================================
//! There is NO `coil.linalg` sub-namespace anywhere (zero matches for
//! `__cobrust_coil_linalg` / `coil_linalg` in `crates/`). The
//! `Attr(Attr(coil, linalg), <fn>)` chain is COMPLETELY UNRESOLVED — it
//! falls through the typecheck Attr arm to `Ok(self.fresh_var())` (the same
//! fall-through `a.shape` had pre-ADR-0077-Phase-1), so:
//!
//!   - `coil.linalg.det(coil.eye(2))` PASSES `cobrust check` (exit 0) AND
//!     BUILDS (exit 0) — but the spawned binary prints **`0`** (det of the
//!     identity is `1`). A FALSE GREEN: the call lowers to garbage / a no-op,
//!     NOT the real determinant. (F37-style silent-rot — builds, runs,
//!     exits 0, WRONG value.)
//!   - `coil.linalg.solve(...)` → Buffer likewise builds+runs exit 0 but
//!     yields an empty/garbage buffer (`mean` reads `0`).
//!   - `coil.linalg.inv(...)` → Buffer builds+runs exit 0 but `print_buffer`
//!     emits NOTHING (garbage handle).
//!   - even a NONEXISTENT member `coil.linalg.solveX(a)` PASSES `cobrust
//!     check` (exit 0) — the unknown-member error is ALSO a false green
//!     today (no sub-namespace resolution to reject it).
//!
//! Therefore every positive below is a FULL build-and-run E2E asserting the
//! CORRECT numerical stdout (RED today: wrong value / empty / garbage). The
//! DEV turns them green by wiring all five layers per ADR-0079 §8 (manifest
//! sub-namespace table + typecheck dotted-namespace rule + MIR retarget +
//! codegen externs + cabi shims wrapping the existing kernels). NONE is
//! `#[ignore]`d — they are the contract the Phase-1 DEV must turn green
//! (corpus + impl land atomically).
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_ops_e2e.rs` /
//! `coil_ops_phase2_e2e.rs`. f64 reads are observed via `(x as i64)` casts
//! to dodge f64 print-format drift (the same robustness trick those corpora
//! use); the cast truncates toward zero and preserves sign, so `det ==
//! -2.0` → `(d as i64)` → `"-2"` (verified) and exact-integer solution
//! elements (`x[0] == 1.0`) → `"1"`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments: the multi-line prose
// continuations after `-`/`coil.*` lines read as "lazy" list items to
// clippy, but they are intentional explanatory prose, not lint targets.
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

/// Build-only helper — returns `(build_succeeded, stderr)`. Used by the
/// runtime-error negatives, which must BUILD then FAIL at run (singular /
/// non-square is invisible to the type, so the build cannot reject them).
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
// TIER A — identity-only positives (need NO new constructor; pin the
// CORE coil.linalg.* wiring). RED today: false-green build+run prints
// the WRONG value (det→"0" not "1") / empty (inv).
// =====================================================================

/// Positive #1 (Tier A — `coil.linalg.det` of the identity) —
/// `coil.linalg.det(coil.eye(3))` is the determinant of the `3 x 3`
/// identity, which is `1.0` → `(d as i64)` → "1". Uses ONLY `coil.eye`
/// (the sole `.cb` 2-D constructor), so it needs NO new constructor.
///
/// PROOF OBLIGATION: at HEAD `coil.linalg.det(...)` passes `cobrust check`
/// + BUILDS but the binary prints "0" (the `Attr(Attr(coil,linalg),det)`
/// chain is unresolved → `fresh_var()` → lowers to garbage). The DEV wires
/// the manifest row `("coil.linalg","det") -> __cobrust_coil_linalg_det`
/// (ret `Ty::Float`), the dotted-namespace typecheck rule, the MIR retarget
/// (ADR-0079 §8 `emit_ecosystem_call`), the codegen extern (`ptr -> f64`),
/// and the cabi shim (borrow 1 Buffer → `coil::linalg::det(&a)` → extract
/// the 0-d scalar via `scalar_array_to_f64`, mirroring `..._buffer_dot`).
/// The shim mirrors ADR-0079 §8 "0-d → f64" honesty (ADR-0077 Q2 precedent).
#[test]
fn test_e2e_linalg_det_identity_is_one() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.eye(3)\n",
        "    let d: f64 = coil.linalg.det(a)\n",
        "    print((d as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn linalg-det-eye");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "1",
        "expected det(eye(3)) == 1.0 → '1' (got '0' means coil.linalg.det is unwired \
         garbage); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #2 (Tier A — `coil.linalg.solve` with the identity) — solving
/// `I · x = b` returns `x == b`. `coil.linalg.solve(coil.eye(3),
/// coil.ones(3))` → `[1, 1, 1]`; observe via `coil.mean(x)` → `1.0` →
/// `(1.0 as i64)` → "1". Uses ONLY `coil.eye` + `coil.ones`, so NO new
/// constructor. Verifies solve returns a real (fresh, droppable) `Buffer`
/// of the solution, not garbage.
///
/// PROOF OBLIGATION: at HEAD this builds+runs exit 0 but `mean(x)` reads "0"
/// (the unresolved chain yields an empty/garbage buffer). The DEV wires
/// `("coil.linalg","solve") -> __cobrust_coil_linalg_solve` (params
/// `[Buffer, Buffer]`, ret `coil_buffer_ty()`), the extern (`ptr,ptr ->
/// ptr`), and the cabi shim (borrow 2 Buffers → `coil::linalg::solve(&a,
/// &b)?` → fresh box; `LinalgShapeError`/`SingularMatrix` → `coil_panic`).
/// The returned Buffer drops once at `.cb` scope exit (existing Buffer drop
/// schedule).
#[test]
fn test_e2e_linalg_solve_identity_returns_rhs() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.eye(3)\n",
        "    let b: coil.Buffer = coil.ones(3)\n",
        "    let x: coil.Buffer = coil.linalg.solve(a, b)\n",
        "    let m: f64 = coil.mean(x)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn linalg-solve-eye");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "1",
        "expected solve(eye(3), ones(3)) == [1,1,1], mean == 1.0 → '1' (got '0' means \
         coil.linalg.solve is unwired); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #3 (Tier A — `coil.linalg.inv` of the identity) — the inverse
/// of `I` is `I`. `coil.linalg.inv(coil.eye(2))` → the `2 x 2` identity;
/// observe via `coil.print_buffer(i)` against coil's exact numpy-style
/// 2-D repr. Uses ONLY `coil.eye`, so NO new constructor. The exact repr is
/// pinned (verified empirically: `coil.print_buffer(coil.eye(2))` emits the
/// single-line `array([[1, 0], [0, 1]], dtype=float64)\n`, and
/// `inv(eye(2)) == eye(2)`).
///
/// PROOF OBLIGATION: at HEAD this builds+runs exit 0 but `print_buffer`
/// emits NOTHING (the unresolved chain yields a garbage handle). The DEV
/// wires `("coil.linalg","inv") -> __cobrust_coil_linalg_inv` (params
/// `[Buffer]`, ret `coil_buffer_ty()`), the extern (`ptr -> ptr`), and the
/// cabi shim (borrow 1 → `coil::linalg::inv(&a)?` → fresh box; `Singular
/// Matrix` → `coil_panic`). This is the strongest Tier-A case — it observes
/// the FULL matrix contents (every element), not just a scalar reduction.
#[test]
fn test_e2e_linalg_inv_identity_is_identity() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.eye(2)\n",
        "    let i: coil.Buffer = coil.linalg.inv(a)\n",
        "    let _ = coil.print_buffer(i)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn linalg-inv-eye");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "array([[1, 0], [0, 1]], dtype=float64)",
        "expected inv(eye(2)) == eye(2) repr (got empty/garbage means coil.linalg.inv is \
         unwired); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// TIER B — non-trivial positives. REQUIRE a non-identity 2-D matrix
// constructor (see the DEV PREREQUISITE banner). Written against the
// PROPOSED `coil.array2x2(a, b, c, d)` (row-major 2x2). The DEV adds
// this constructor (or an equivalent 2-D surface + re-spells these).
// These exercise the REAL math the identity cannot: pivoting (det != 1),
// a genuine linear solve, a non-trivial inverse.
// =====================================================================

/// Positive #4 (Tier B — `coil.linalg.det` of a NON-identity matrix) —
/// `det([[1,2],[3,4]]) == 1*4 - 2*3 == -2.0` → `(d as i64)` → "-2" (the
/// `as i64` cast preserves the sign; verified `(0.0 - 2.0) as i64` → "-2").
/// This is the headline ADR-0079 §7 done-means value: a determinant the
/// identity can NEVER produce, so it forces a real 2-D-data constructor.
///
/// PROOF OBLIGATION (two-part): (1) the DEV must add a 2-D constructor
/// `coil.array2x2(1.0, 2.0, 3.0, 4.0)` building the row-major `[[1,2],[3,4]]`
/// matrix (NEW manifest row + cabi shim → `coil::array_f64(&[1,2,3,4],
/// &[2,2])`); (2) `coil.linalg.det` must compute `-2` via the wired shim. At
/// HEAD BOTH are absent — `coil.array2x2` is an unknown fn (`UnknownMethod`/
/// no manifest row) AND `coil.linalg.det` is unwired. RED on both counts.
#[test]
fn test_e2e_linalg_det_known_2x2_is_minus_two() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let d: f64 = coil.linalg.det(a)\n",
        "    print((d as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn linalg-det-2x2");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "-2",
        "expected det([[1,2],[3,4]]) == -2.0 → '-2'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #5 (Tier B — `coil.linalg.solve` of a known 2x2 system) — solve
/// `[[1,2],[3,4]] · x = [5,11]`. The unique solution is `x == [1, 2]`
/// (check: `1*1 + 2*2 == 5`, `3*1 + 4*2 == 11`). Read BOTH solution
/// elements exactly: `x[0] == 1.0` → "1", `x[1] == 2.0` → "2"; prints
/// "1\n2". Exact integers (no truncation ambiguity) + a genuine
/// off-diagonal system the identity cannot model — this is the real
/// `np.linalg.solve` numeric proof.
///
/// PROOF OBLIGATION: requires the 2-D constructor (`coil.array2x2`) AND the
/// wired `coil.linalg.solve` AND the already-green Buffer-index read
/// (`x[i]` → f64, ADR-0077 Phase 1). At HEAD the constructor is absent +
/// solve is unwired. RED. (numpy idiom: `np.linalg.solve(A, b)` → §2.5
/// overlap 1.0 — `coil` vs `np` is the only difference.)
#[test]
fn test_e2e_linalg_solve_known_2x2_system() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let b: coil.Buffer = coil.array1d2(5.0, 11.0)\n",
        "    let x: coil.Buffer = coil.linalg.solve(a, b)\n",
        "    print((x[0] as i64))\n",
        "    print((x[1] as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn linalg-solve-2x2");
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
        "expected solve([[1,2],[3,4]], [5,11]) == [1,2]: x[0]='1', x[1]='2'; \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #6 (Tier B — `coil.linalg.inv` of a NON-identity matrix,
/// full-matrix observation) — invert the diagonal `A = [[2,0],[0,4]]`;
/// `inv(A) == [[0.5,0],[0,0.25]]` (each diagonal entry is reciprocated).
/// Observe the WHOLE result via `coil.print_buffer` against the exact pinned
/// repr `array([[0.5, 0], [0, 0.25]], dtype=float64)` (verified empirically
/// from `coil::array_repr(inv([[2,0],[0,4]]))`). This is the rigorous
/// inverse proof: it checks every element AND it is unmistakably NON-identity
/// (the `(0,0)` entry is `0.5`, not the `1` of `inv(eye)`), so it cannot
/// pass on an accidental identity-passthrough. It deliberately does NOT
/// depend on 2-D flat-indexing semantics (`a[i]` on a rank-2 Buffer is
/// underspecified — numpy errors; the existing 1-D `getitem` normalises
/// against `shape()[0]` only), reading the full repr instead.
///
/// NOTE — why not `A · inv(A) ≈ I`? `coil.Buffer.dot` ships ONLY the **1-D**
/// dot → f64 scalar today (ADR-0077 Phase 2a; 2-D matmul → Buffer is a
/// Phase-3 follow-up, `cabi.rs:612`), so a 2-D `A.dot(inv(A))` round-trip is
/// not observable yet. The full-repr check is the strongest available proxy.
///
/// PROOF OBLIGATION: requires the 2-D constructor (`coil.array2x2`) + the
/// wired `coil.linalg.inv`. At HEAD both are absent/unwired. RED.
#[test]
fn test_e2e_linalg_inv_nonidentity_full_repr() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(2.0, 0.0, 0.0, 4.0)\n",
        "    let i: coil.Buffer = coil.linalg.inv(a)\n",
        "    let _ = coil.print_buffer(i)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn linalg-inv-2x2");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "array([[0.5, 0], [0, 0.25]], dtype=float64)",
        "expected inv([[2,0],[0,4]]) == [[0.5,0],[0,0.25]] repr (NON-identity — distinguishes \
         from inv(eye)); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// TIER C — runtime negatives. Singular-matrix solve/inv → panic;
// non-square det → panic (ADR-0079 §7 done-means; ADR-0017 runtime
// LinalgShapeError / SingularMatrix). All REQUIRE a non-identity 2-D
// matrix (the identity is never singular + always square), so they also
// depend on the 2-D constructor prerequisite. Written as build-succeeds +
// run-FAILS (singularity / shape is invisible to the static type — a
// coil.Buffer carries no rank/conditioning), NOT as compile errors.
// =====================================================================

/// Negative #1 (Tier C — singular `solve` traps at runtime) —
/// `coil.linalg.solve([[1,2],[2,4]], [1,1])`. The matrix `[[1,2],[2,4]]` is
/// singular (row 2 == 2·row 1; det == 0). Per ADR-0079 §7 / ADR-0017 a
/// singular `solve` is a runtime `SingularMatrix` → `coil_panic` → non-zero
/// exit. Singularity is NOT in the static type (`coil.Buffer` carries no
/// conditioning), so this must BUILD and TRAP at run, NOT compile-error.
///
/// PROOF OBLIGATION (two-part): (1) once the DEV adds `coil.array2x2` + wires
/// `coil.linalg.solve`, the program BUILDS (singularity invisible to the
/// type). At HEAD the build FAILS (constructor absent) — also a legitimate
/// RED state; the build stderr is surfaced. (2) the built binary must EXIT
/// NON-ZERO (the solve shim forwards `linalg::solve`'s `SingularMatrix` err
/// to `coil_panic`). We assert `!success` (not a specific code) for
/// robustness to the abort convention.
#[test]
fn test_runtime_linalg_solve_singular_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 2.0, 4.0)\n",
        "    let b: coil.Buffer = coil.array1d2(1.0, 1.0)\n",
        "    let x: coil.Buffer = coil.linalg.solve(a, b)\n",
        "    let _ = coil.print_buffer(x)\n",
        "    return 0\n",
    );
    let (built, build_stderr) = try_build(source);
    assert!(
        built,
        "Tier C: singular solve must BUILD (singularity is not part of the type — the DEV \
         must add the 2-D constructor + wire coil.linalg.solve); build stderr=\n{build_stderr}",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn solve-singular");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "Tier C: solve of a singular matrix [[1,2],[2,4]] must TRAP at runtime (non-zero exit \
         per SingularMatrix → coil_panic); got success. stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Negative #2 (Tier C — singular `inv` traps at runtime) —
/// `coil.linalg.inv([[1,2],[2,4]])`. A singular matrix has no inverse; per
/// ADR-0079 §7 / ADR-0017 `inv` raises `SingularMatrix` → `coil_panic` →
/// non-zero exit. (Contrast `det` of the SAME singular matrix, which
/// numpy + coil return as `0.0` WITHOUT panicking — verified against the
/// kernel; that is why the non-square *det* below, not a singular det, is
/// the det panic case.) Build-succeeds + run-FAILS.
///
/// PROOF OBLIGATION: as Negative #1 but for `inv` (borrow 1 → `linalg::inv`
/// → `SingularMatrix` → `coil_panic`). At HEAD the constructor is absent +
/// inv is unwired. RED.
#[test]
fn test_runtime_linalg_inv_singular_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 2.0, 4.0)\n",
        "    let i: coil.Buffer = coil.linalg.inv(a)\n",
        "    let _ = coil.print_buffer(i)\n",
        "    return 0\n",
    );
    let (built, build_stderr) = try_build(source);
    assert!(
        built,
        "Tier C: singular inv must BUILD (singularity is not part of the type — the DEV must \
         add the 2-D constructor + wire coil.linalg.inv); build stderr=\n{build_stderr}",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn inv-singular");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "Tier C: inv of a singular matrix [[1,2],[2,4]] must TRAP at runtime (non-zero exit \
         per SingularMatrix → coil_panic); got success. stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Negative #3 (Tier C — non-square `det` traps at runtime) — `det` of a
/// `2 x 3` matrix. Per ADR-0017 `det` requires a square matrix; a non-square
/// input is a runtime `LinalgShapeError` → `coil_panic` → non-zero exit.
/// (This is the det PANIC case — a *singular* det returns `0.0` without
/// panicking, but a *non-square* det DOES panic, verified against the
/// kernel: `det(shape [2,3])` → `Err("det requires a square matrix")`.)
/// Rank/shape is NOT in the static type, so this must BUILD and TRAP at run.
///
/// Requires a NON-SQUARE 2-D constructor. Written against a PROPOSED
/// `coil.array2x3(a,b,c,d,e,f)` (row-major 2x3, six scalar f64 args). The
/// DEV adds it alongside `coil.array2x2` (same shim pattern → `coil::
/// array_f64(&[a..f], &[2,3])`), OR substitutes an equivalent rectangular
/// 2-D surface and re-spells this case.
///
/// PROOF OBLIGATION (two-part): (1) once the DEV adds `coil.array2x3` + wires
/// `coil.linalg.det`, the program BUILDS (shape invisible to the type). At
/// HEAD the build FAILS (constructor absent) — legitimate RED; stderr
/// surfaced. (2) the binary must EXIT NON-ZERO (the det shim forwards
/// `LinalgShapeError` to `coil_panic`).
#[test]
fn test_runtime_linalg_det_nonsquare_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let d: f64 = coil.linalg.det(a)\n",
        "    print((d as i64))\n",
        "    return 0\n",
    );
    let (built, build_stderr) = try_build(source);
    assert!(
        built,
        "Tier C: non-square det must BUILD (rank/shape is not part of the type — the DEV must \
         add a non-square 2-D constructor + wire coil.linalg.det); build stderr=\n{build_stderr}",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn det-nonsquare");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "Tier C: det of a non-square 2x3 matrix must TRAP at runtime (non-zero exit per \
         LinalgShapeError → coil_panic); got success. stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}
