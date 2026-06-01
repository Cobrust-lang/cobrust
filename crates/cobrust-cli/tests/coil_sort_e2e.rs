//! coil FLAT search / order ops (`sort` / `argsort` / `unique` /
//! `flatnonzero`) — `.cb` end-to-end proof for the #145 BATCH-9 addition.
//! Each is a 1-arg Buffer -> Buffer op wired EXACTLY like the BATCH-2
//! reshape ops (`transpose` / `flatten` / `ravel`) and the unary ufuncs:
//! borrow-Buffer-arg → fresh-Buffer-return, riding the shared
//! `cabi::buffer_unary` body + the `coil_shape_ty` `(ptr) -> ptr` extern.
//!
//! ## The load-bearing semantics (numpy 2.x, oracle `python3.11`)
//!
//! - `coil.sort(a)` — ASCENDING sorted 1-D copy (no-axis default flattens
//!   C-order first), DTYPE-PRESERVING; all `NaN` sort LAST.
//! - `coil.argsort(a)` — the `Int64` indices that would sort `a` (STABLE);
//!   the result Buffer is ALWAYS `int64`-dtype regardless of input dtype.
//! - `coil.unique(a)` — SORTED unique values, DTYPE-PRESERVING; multiple
//!   `NaN` collapse to one trailing `NaN`.
//! - `coil.flatnonzero(a)` — the `Int64` flat C-order indices where
//!   `a != 0` (`NaN` counts as nonzero); ALWAYS `int64`-dtype.
//!
//! ## The dtype-flip signal
//!
//! Every `.cb` Buffer constructor (`array1d2` / `array2x2` / `zeros` / …)
//! builds a **Float64** Buffer, so `sort` / `unique` render
//! `dtype=float64` while `argsort` / `flatnonzero` render `dtype=int64` —
//! the dtype literally flips to int64 in the printed repr, a strong
//! observable proof the index-dtype kernel path fired.
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     <op>)` resolves the `Buffer(...) -> Buffer` `EcoSig`;
//!   - MIR `try_lower_ecosystem_call` Case-1 → the GENERIC borrow-arg →
//!     fresh-Buffer-return path (the SAME path `coil.transpose(a)` proves;
//!     NO batch-specific MIR arm, NO `_=>"any"` gap);
//!   - codegen externs (`llvm_backend.rs`) — `(ptr) -> ptr`;
//!   - the cabi shims (`cabi.rs`) — `__cobrust_coil_<op>` returning a fresh
//!     Boxed `Buffer` (TOTAL — a null handle is the only abort).
//!
//! Mirrors the compile→spawn→assert-stdout harness of
//! `coil_manipulate_e2e.rs`. Results are observed via `coil.print_buffer`,
//! whose float64 repr renders integer-valued floats WITHOUT a `.0` suffix
//! (`array([1, 2, 3], dtype=float64)`) and `NaN` as `NaN`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_manipulate_e2e.rs`.
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
    let out = Command::new(exe).output().expect("spawn coil-sort prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — sort a 2x2 → flattened ASCENDING 1-D (float64 preserved).
// The no-axis default FLATTENS the 2x2 first, so the result is a (4,)
// 1-D buffer, NOT a 2x2 (a wrong impl that kept 2-D would print nested).
// =====================================================================

/// `coil.sort(array2x2(3,1,4,2))` → `[1, 2, 3, 4]` (Float64, 1-D).
///
/// Oracle (numpy 2.x): `np.sort([[3,1],[4,2]], axis=None)` → `[1,2,3,4]`.
#[test]
fn test_e2e_sort_2x2_flattens() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(3.0, 1.0, 4.0, 2.0)\n",
        "    let s: coil.Buffer = coil.sort(a)\n",
        "    let _ = coil.print_buffer(s)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 2, 3, 4], dtype=float64)"),
        "expected sort flattened to [1,2,3,4] (float64); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — argsort a 1-D → the int64 INDICES. The result dtype FLIPS
// to int64 (every input Buffer is float64) — the load-bearing index-
// dtype signal. concatenate builds a (4,) 1-D from two array1d2 halves.
// =====================================================================

/// `coil.argsort([3,1,4,2])` → `[1, 3, 0, 2]` (int64 indices).
///
/// Oracle (numpy 2.x): `np.argsort([3.,1.,4.,2.])` → `[1,3,0,2]`,
/// `dtype=int64`. The `int64` repr proves the index-producing kernel arm.
#[test]
fn test_e2e_argsort_indices_are_int64() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let lo: coil.Buffer = coil.array1d2(3.0, 1.0)\n",
        "    let hi: coil.Buffer = coil.array1d2(4.0, 2.0)\n",
        "    let a: coil.Buffer = coil.concatenate(lo, hi)\n",
        "    let s: coil.Buffer = coil.argsort(a)\n",
        "    let _ = coil.print_buffer(s)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 3, 0, 2], dtype=int64)"),
        "expected argsort indices [1,3,0,2] with dtype=int64 (NOT float64); \
         got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — unique a 1-D with DUPLICATES → sorted dedupe (float64).
// concatenate([3,1],[2,1]) = [3,1,2,1]; unique → [1,2,3].
// =====================================================================

/// `coil.unique([3,1,2,1])` → `[1, 2, 3]` (Float64, sorted + deduped).
///
/// Oracle (numpy 2.x): `np.unique([3.,1.,2.,1.])` → `[1.,2.,3.]`.
#[test]
fn test_e2e_unique_sorted_dedupe() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let lo: coil.Buffer = coil.array1d2(3.0, 1.0)\n",
        "    let hi: coil.Buffer = coil.array1d2(2.0, 1.0)\n",
        "    let a: coil.Buffer = coil.concatenate(lo, hi)\n",
        "    let u: coil.Buffer = coil.unique(a)\n",
        "    let _ = coil.print_buffer(u)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 2, 3], dtype=float64)"),
        "expected unique [1,2,3] (float64); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — flatnonzero a 1-D → the int64 INDICES of nonzero elements.
// concatenate([0,5],[0,2]) = [0,5,0,2]; flatnonzero → [1,3] (int64).
// =====================================================================

/// `coil.flatnonzero([0,5,0,2])` → `[1, 3]` (int64 indices).
///
/// Oracle (numpy 2.x): `np.flatnonzero([0.,5.,0.,2.])` → `[1,3]`,
/// `dtype=int64`.
#[test]
fn test_e2e_flatnonzero_indices_are_int64() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let lo: coil.Buffer = coil.array1d2(0.0, 5.0)\n",
        "    let hi: coil.Buffer = coil.array1d2(0.0, 2.0)\n",
        "    let a: coil.Buffer = coil.concatenate(lo, hi)\n",
        "    let nz: coil.Buffer = coil.flatnonzero(a)\n",
        "    let _ = coil.print_buffer(nz)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 3], dtype=int64)"),
        "expected flatnonzero indices [1,3] with dtype=int64; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE (NaN-last) — sort a buffer holding a NaN. The NaN is built via
// IEEE 0.0/0.0 (a `.cb` `a / b` true-divide, which matches numpy: 0/0 ->
// NaN). array1d2(1.0,0.0) / array1d2(1.0,0.0) = [1.0, NaN]; sort -> the
// NaN must land LAST.
// =====================================================================

/// `coil.sort([1.0, NaN])` → `[1, NaN]` (NaN sorts to the END).
/// The NaN is produced by IEEE 0.0/0.0 so no NaN literal is needed.
///
/// Oracle (numpy 2.x): `np.sort([1., nan])` → `[1., nan]`.
#[test]
fn test_e2e_sort_nan_last() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let num: coil.Buffer = coil.array1d2(1.0, 0.0)\n",
        "    let den: coil.Buffer = coil.array1d2(1.0, 0.0)\n",
        // num / den = [1/1, 0/0] = [1.0, NaN]  (IEEE, matches numpy)
        "    let nanbuf: coil.Buffer = num / den\n",
        "    let s: coil.Buffer = coil.sort(nanbuf)\n",
        "    let _ = coil.print_buffer(s)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    // The finite 1 must come first and NaN must be the LAST element.
    assert!(
        stdout.contains("array([1, NaN], dtype=float64)"),
        "expected sort to place NaN LAST: [1, NaN]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// CHAIN — sort ∘ unique (a fresh-Buffer feeding the next op, proving the
// result handle is a first-class drop-scheduled Buffer). unique already
// returns sorted, so the composition is still [1,2,3] — but it exercises
// TWO BATCH-9 ops back-to-back with the intermediate temporary dropped.
// =====================================================================

/// `coil.sort(coil.unique([3,1,2,1]))` → `[1, 2, 3]` (Float64). The
/// intermediate `unique` Buffer is consumed by `sort`; both temporaries
/// drop at scope exit.
#[test]
fn test_e2e_sort_of_unique_chain() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let lo: coil.Buffer = coil.array1d2(3.0, 1.0)\n",
        "    let hi: coil.Buffer = coil.array1d2(2.0, 1.0)\n",
        "    let a: coil.Buffer = coil.concatenate(lo, hi)\n",
        "    let u: coil.Buffer = coil.unique(a)\n",
        "    let s: coil.Buffer = coil.sort(u)\n",
        "    let _ = coil.print_buffer(s)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 2, 3], dtype=float64)"),
        "expected sort(unique(...)) = [1,2,3] (float64); got stdout=\n{stdout}",
    );
}
