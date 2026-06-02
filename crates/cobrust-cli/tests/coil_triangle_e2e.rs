//! coil LINALG-EXTRACT ops (`diag` / `tril` / `triu`) — `.cb` end-to-end
//! proof for the #163 BATCH-14 addition. Each is a 1-arg Buffer-RETURNING
//! op wired EXACTLY like the BATCH-2 reshape ops (`transpose` / `flatten`
//! / `ravel`) + the unary ufuncs: borrow-Buffer-arg → fresh-Buffer-return,
//! riding the `coil_shape_ty` `(ptr) -> ptr` extern + the SAME generic
//! 1-Buffer-arg MIR lowering (ZERO batch-specific MIR code). The ONE
//! batch-specific wrinkle (the cabi shim being FALLIBLE — a disallowed
//! input RANK `coil_panic`s) is invisible at the `.cb` layer.
//!
//! ## The load-bearing semantics (numpy 2.x, oracle `python3.11`, 2.4.6)
//!
//! - `coil.diag(a)` is SHAPE-DEPENDENT (`k=0` main diagonal):
//!   - a 1-D `(n,)` input → the `(n,n)` matrix with `a` on the main
//!     diagonal, zeros elsewhere
//!     (`np.diag([1,2]) == [[1,0],[0,2]]`).
//!   - a 2-D `(r,c)` input → the 1-D main-diagonal extract, length
//!     `min(r,c)` (`np.diag([[1,2],[3,4]]) == [1,4]`).
//! - `coil.tril(a)` — LOWER triangle: keep elements ON and BELOW the main
//!   diagonal, ZERO those ABOVE; SAME shape, 2-D-required
//!   (`np.tril([[1,2],[3,4]]) == [[1,0],[3,4]]`).
//! - `coil.triu(a)` — UPPER triangle: keep ON and ABOVE, ZERO those BELOW;
//!   SAME shape, 2-D-required
//!   (`np.triu([[1,2],[3,4]]) == [[1,2],[0,4]]`).
//!
//! All three are DTYPE-PRESERVING; every `.cb` Buffer constructor builds a
//! Float64 buffer, so the printed repr renders `dtype=float64` (the
//! float64 repr prints integer-valued floats WITHOUT a `.0` suffix). The
//! 2-D repr is coil's flat-bracketed `[[a, b], [c, d]]` form (NOT numpy's
//! column-aligned multi-line layout) per ADR-0013 §4.
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     <op>)` resolves the `Buffer(...) -> Buffer` `EcoSig`;
//!   - MIR `try_lower_ecosystem_call` Case-1 → the GENERIC borrow-arg →
//!     fresh-Buffer-return path (the SAME path `coil.transpose(a)` proves;
//!     NO batch-specific MIR arm, NO `_=>"any"` gap);
//!   - codegen externs (`llvm_backend.rs`) — `(ptr) -> ptr` ≡
//!     `coil_shape_ty` for all three;
//!   - the cabi shims (`cabi.rs`) — `__cobrust_coil_<op>` returning a
//!     fresh Boxed `Buffer` via `buffer_unary_fallible` (a disallowed
//!     RANK is a clean `coil_panic`).
//!
//! Mirrors the compile→spawn→assert-stdout harness of
//! `coil_rearrange_e2e.rs`. Results are observed via `coil.print_buffer`.

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
    let out = Command::new(exe)
        .output()
        .expect("spawn coil-triangle prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — diag, BOTH directions (the shape-dependent op).
// =====================================================================

/// `coil.diag(array1d2(1.0, 2.0))` CONSTRUCTS the 2-D matrix
/// `[[1, 0], [0, 2]]` (1-D → 2-D, main diagonal). Oracle:
/// `np.diag([1., 2.]) == [[1., 0.], [0., 2.]]`. Proves the 1-D→2-D
/// construct path AND the off-diagonal zero-fill.
#[test]
fn test_e2e_diag_constructs_matrix_from_1d() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let r: coil.Buffer = coil.diag(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([[1, 0], [0, 2]], dtype=float64)"),
        "expected diag([1,2])=[[1,0],[0,2]] (1-D->2-D); got stdout=\n{stdout}",
    );
}

/// `coil.diag(array2x2(1.0, 2.0, 3.0, 4.0))` EXTRACTS the main diagonal →
/// `[1, 4]` (2-D → 1-D). Oracle: `np.diag([[1,2],[3,4]]) == [1, 4]`. The
/// OTHER direction of the shape-dependent op from the test above.
#[test]
fn test_e2e_diag_extracts_diagonal_from_2d() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let r: coil.Buffer = coil.diag(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 4], dtype=float64)"),
        "expected diag([[1,2],[3,4]])=[1, 4] (2-D->1-D); got stdout=\n{stdout}",
    );
}

/// `coil.diag(array2x3(...))` on a NON-SQUARE `2 x 3` matrix extracts the
/// length-`min(2,3)=2` main diagonal. `np.diag([[1,2,3],[4,5,6]]) == [1, 5]`.
#[test]
fn test_e2e_diag_extract_non_square_min_rc() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let r: coil.Buffer = coil.diag(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 5], dtype=float64)"),
        "expected diag([[1,2,3],[4,5,6]])=[1, 5] (len min(r,c)=2); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — tril / triu (2-D triangle mask). The DISCRIMINATING pair:
// on the SAME asymmetric matrix they ZERO OPPOSITE corners.
// =====================================================================

/// `coil.tril(array2x2(1.0, 2.0, 3.0, 4.0))` → `[[1, 0], [3, 4]]` (ZERO
/// the upper-right `2`; keep ON+BELOW the diagonal). Oracle:
/// `np.tril([[1,2],[3,4]]) == [[1,0],[3,4]]`.
#[test]
fn test_e2e_tril_zeros_above_diagonal() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let r: coil.Buffer = coil.tril(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([[1, 0], [3, 4]], dtype=float64)"),
        "expected tril([[1,2],[3,4]])=[[1,0],[3,4]] (zeros ABOVE); got stdout=\n{stdout}",
    );
}

/// `coil.triu(array2x2(1.0, 2.0, 3.0, 4.0))` → `[[1, 2], [0, 4]]` (ZERO
/// the lower-left `3`; keep ON+ABOVE the diagonal). Oracle:
/// `np.triu([[1,2],[3,4]]) == [[1,2],[0,4]]`. The COMPLEMENT of `tril` —
/// on the SAME input the two ZERO OPPOSITE corners (they must NOT be
/// swapped: `tril -> [[1,0],[3,4]]`, `triu -> [[1,2],[0,4]]`).
#[test]
fn test_e2e_triu_zeros_below_diagonal() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let r: coil.Buffer = coil.triu(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([[1, 2], [0, 4]], dtype=float64)"),
        "expected triu([[1,2],[3,4]])=[[1,2],[0,4]] (zeros BELOW); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — chain: diag(diag(v)) round-trips a 1-D vector through the
// constructed matrix and back to the SAME 1-D vector. Proves the
// fresh-Buffer-return of one diag feeds DIRECTLY as the borrowed input
// of the next (the ecosystem-call ownership handshake), AND that the
// two shape directions compose (1-D -> (n,n) -> 1-D).
// =====================================================================

/// `coil.diag(coil.diag(array1d2(5.0, 7.0)))` → `[5, 7]`. The inner
/// `diag` builds `[[5,0],[0,7]]` (1-D→2-D); the outer `diag` extracts its
/// main diagonal back to `[5, 7]` (2-D→1-D). Oracle:
/// `np.diag(np.diag([5., 7.])) == [5., 7.]`.
#[test]
fn test_e2e_diag_diag_round_trip() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let v: coil.Buffer = coil.array1d2(5.0, 7.0)\n",
        "    let m: coil.Buffer = coil.diag(v)\n",
        "    let back: coil.Buffer = coil.diag(m)\n",
        "    let _ = coil.print_buffer(back)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([5, 7], dtype=float64)"),
        "expected diag(diag([5,7]))=[5, 7] (round-trip); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — chain: tril composes with the existing reshape op
// `transpose`. `transpose(tril(a))` lifts the kept lower triangle into
// the upper triangle. `np.tril([[1,2],[3,4]]).T == [[1,3],[0,4]]`. Proves
// the BATCH-14 fresh-Buffer-return feeds the BATCH-2 reshape ops.
// =====================================================================

/// `coil.transpose(coil.tril(array2x2(1, 2, 3, 4)))` →
/// `[[1, 3], [0, 4]]`. `tril` gives `[[1,0],[3,4]]`; transpose gives
/// `[[1,3],[0,4]]`. Oracle: `np.tril([[1,2],[3,4]]).T == [[1,3],[0,4]]`.
#[test]
fn test_e2e_transpose_of_tril_chain() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let l: coil.Buffer = coil.tril(a)\n",
        "    let t: coil.Buffer = coil.transpose(l)\n",
        "    let _ = coil.print_buffer(t)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([[1, 3], [0, 4]], dtype=float64)"),
        "expected transpose(tril([[1,2],[3,4]]))=[[1,3],[0,4]]; got stdout=\n{stdout}",
    );
}
