//! coil elementwise BINARY min/max ufuncs (`coil.maximum` / `coil.minimum`
//! / `coil.fmax` / `coil.fmin`) — `.cb` end-to-end proof for the #163
//! BATCH-13 addition: the FIRST 2-Buffer ELEMENTWISE ufunc family (a value
//! at lane `i` of the result is `pick(a[i], b[i])`). It rides the IDENTICAL
//! borrow-two-Buffers → fresh-Buffer-return value-handle ABI as the
//! `concatenate` / `vstack` / `hstack` combine ops (and `coil.linalg.solve`)
//! — same `(ptr, ptr) -> ptr` `coil_binop_ty` extern, same `buffer_combine`
//! cabi body. The ONLY new behaviour is the elementwise min/max pick + the
//! NaN split (below).
//!
//! ## The load-bearing semantics — the NaN split (numpy 2.4.6 oracle)
//!
//! - `coil.maximum(a, b)` / `coil.minimum(a, b)` — elementwise max/min that
//!   **PROPAGATE NaN**: ANY NaN operand yields a NaN result lane
//!   (`np.maximum([1,nan],[3,7]) = [3, nan]`).
//! - `coil.fmax(a, b)` / `coil.fmin(a, b)` — elementwise max/min that
//!   **IGNORE NaN**: pick the non-NaN operand; the lane is NaN ONLY when
//!   BOTH operands are NaN (`np.fmax([1,nan],[3,7]) = [3, 7]`,
//!   `np.fmax(nan, nan) = nan`).
//!
//! The DISCRIMINATING test (`test_e2e_maximum_vs_fmax_nan_split`) runs
//! `maximum` and `fmax` over the SAME NaN-bearing pair and asserts the two
//! diverge at the NaN lane — `maximum` keeps the NaN, `fmax` substitutes the
//! non-NaN operand. That one case is the whole point of the batch.
//!
//! - Dtype-PRESERVING (`maximum(int,int) -> int`); same-shape + same-dtype
//!   required (the `concatenate` equal-shape / equal-dtype combine contract;
//!   numpy broadcasts + promotes — a tracked follow-up). A non-conformable
//!   pair `coil_panic`s (numpy raises `ValueError`) = a clean trap, NEVER a
//!   C-ABI unwind.
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     "maximum")` resolves the `(Buffer, Buffer) -> Buffer` `EcoSig`;
//!   - MIR `try_lower_ecosystem_call` Case-1 (module free fn) → the GENERIC
//!     `emit_ecosystem_call` borrow-args → fresh-Buffer-return path (the
//!     SAME path as the 2-Buffer `coil.concatenate(a, b)`, iterating
//!     `sig.params` regardless of arity; NO minmax-specific MIR arm, NO
//!     `_=>"any"` gap — ZERO new MIR code);
//!   - codegen externs (`llvm_backend.rs`) — the `(ptr,ptr)->ptr`
//!     `coil_binop_ty` shape, reused verbatim from `concatenate`;
//!   - the cabi shims (`cabi.rs`) — `__cobrust_coil_{maximum,minimum,fmax,
//!     fmin}` borrow BOTH handles via the shared `buffer_combine` body,
//!     return a fresh Boxed `Buffer`, and `coil_panic` on a non-conformable
//!     / dtype-mismatch pair (NEVER unwinding across the C-ABI).
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_where_e2e.rs`.
//! Results are observed via `coil.print_buffer`, whose float64 repr renders
//! integer-valued floats WITHOUT a `.0` suffix (e.g. `array([3, 2],
//! dtype=float64)`) and a NaN lane as the capital `NaN` (Rust `Display`).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_where_e2e.rs`.
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
    let out = Command::new(exe).output().expect("spawn coil-minmax prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — `maximum` / `minimum` (no NaN). maximum([1,2],[3,1]) = [3,2];
// minimum([1,2],[3,1]) = [1,1]. The lane-mixed result (max picks b[0] then
// a[1]) rules out a degenerate all-a / all-b fill.
// =====================================================================

/// `coil.maximum([1,2],[3,1]) = [3, 2]` and `coil.minimum([1,2],[3,1]) =
/// [1, 1]` — the elementwise pick over two real Buffers, both printed.
///
/// Oracle (numpy 2.4.6): `np.maximum([1.,2.],[3.,1.]) = [3., 2.]`,
/// `np.minimum(...) = [1., 1.]`.
#[test]
fn test_e2e_maximum_minimum_basic() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let b: coil.Buffer = coil.array1d2(3.0, 1.0)\n",
        "    let mx: coil.Buffer = coil.maximum(a, b)\n",
        "    let mn: coil.Buffer = coil.minimum(a, b)\n",
        "    let _ = coil.print_buffer(mx)\n",
        "    let _ = coil.print_buffer(mn)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([3, 2], dtype=float64)"),
        "expected maximum([1,2],[3,1]) = [3, 2]; got stdout=\n{stdout}",
    );
    assert!(
        stdout.contains("array([1, 1], dtype=float64)"),
        "expected minimum([1,2],[3,1]) = [1, 1]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `fmin`. fmin([4,5],[2,9]) = [2,5] (lane-mixed: b[0] then a[1]).
// With no NaN, fmin agrees with minimum — the NaN split test below is what
// separates them.
// =====================================================================

/// `coil.fmin([4,5],[2,9]) = [2, 5]` — the NaN-ignoring min agrees with
/// `minimum` when there is no NaN.
///
/// Oracle (numpy 2.4.6): `np.fmin([4.,5.],[2.,9.]) = [2., 5.]`.
#[test]
fn test_e2e_fmin_basic() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(4.0, 5.0)\n",
        "    let b: coil.Buffer = coil.array1d2(2.0, 9.0)\n",
        "    let r: coil.Buffer = coil.fmin(a, b)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([2, 5], dtype=float64)"),
        "expected fmin([4,5],[2,9]) = [2, 5]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — THE discriminating test: maximum-vs-fmax on a NaN input. With
// a=[1, NaN] (NaN built as 0/0), b=[3, 7]:
//   maximum(a, b) = [3, NaN]   (NaN PROPAGATES at lane 1)
//   fmax(a, b)    = [3, 7]      (NaN IGNORED — the non-NaN 7 wins at lane 1)
// The SAME inputs, divergent lane-1 results — the whole point of the batch.
// =====================================================================

/// `coil.maximum` PROPAGATES NaN but `coil.fmax` IGNORES it, on the SAME
/// NaN-bearing pair. `a = [1, 0/0=NaN]`, `b = [3, 7]`:
/// `maximum(a,b) = [3, NaN]` (lane 1 is NaN), `fmax(a,b) = [3, 7]` (lane 1
/// picks the non-NaN 7). The discriminating case for the entire batch.
///
/// Oracle (numpy 2.4.6): `np.maximum([1,nan],[3,7]) = [3, nan]`;
/// `np.fmax([1,nan],[3,7]) = [3, 7]`. coil renders f64 NaN as `NaN`.
#[test]
fn test_e2e_maximum_vs_fmax_nan_split() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        // a = [1, NaN] built via IEEE division: 1/1 = 1, 0/0 = NaN (no NaN
        // literal needed).
        "    let anum: coil.Buffer = coil.array1d2(1.0, 0.0)\n",
        "    let aden: coil.Buffer = coil.array1d2(1.0, 0.0)\n",
        "    let a: coil.Buffer = anum / aden\n", // [1, NaN]
        "    let b: coil.Buffer = coil.array1d2(3.0, 7.0)\n",
        "    let mx: coil.Buffer = coil.maximum(a, b)\n", // [3, NaN]
        "    let fm: coil.Buffer = coil.fmax(a, b)\n",    // [3, 7]
        "    let _ = coil.print_buffer(mx)\n",
        "    let _ = coil.print_buffer(fm)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    // maximum keeps the NaN at lane 1; fmax replaces it with 7. Exactly ONE
    // NaN total across both prints (maximum's lane 1) — proving fmax did NOT
    // emit a NaN.
    assert!(
        stdout.contains("array([3, NaN], dtype=float64)"),
        "expected maximum(a,b) = [3, NaN] (NaN PROPAGATES); got stdout=\n{stdout}",
    );
    assert!(
        stdout.contains("array([3, 7], dtype=float64)"),
        "expected fmax(a,b) = [3, 7] (NaN IGNORED — non-NaN 7 wins); got stdout=\n{stdout}",
    );
    assert_eq!(
        stdout.matches("NaN").count(),
        1,
        "exactly ONE NaN total: maximum keeps it, fmax drops it; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — fmax(NaN, NaN) = NaN: the ONLY NaN case for fmax/fmin (a lane
// is NaN only when BOTH operands are NaN). Built via 0/0 on both operands.
// =====================================================================

/// `coil.fmax(NaN, NaN) = NaN` — the sole NaN-producing case for the
/// NaN-ignoring pair. Both operands are `0/0`, so the only available value
/// at the lane is NaN.
///
/// Oracle (numpy 2.4.6): `np.fmax(nan, nan) = nan`.
#[test]
fn test_e2e_fmax_both_nan_is_nan() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        // Both operands all-NaN, built as TWO distinct bindings from 0/0
        // (`n1` / `n2`). A 2-Buffer eco-call wants two separately-owned
        // Buffer handles — feeding the SAME `/`-result binding to both arg
        // slots is a pre-existing typecheck edge shared with
        // `coil.concatenate(x, x)` (out of this batch's scope), so the two
        // NaN operands are materialized independently.
        "    let znum: coil.Buffer = coil.array1d2(0.0, 0.0)\n",
        "    let zden: coil.Buffer = coil.array1d2(0.0, 0.0)\n",
        "    let n1: coil.Buffer = znum / zden\n", // [NaN, NaN]
        "    let n2: coil.Buffer = znum / zden\n", // [NaN, NaN]
        "    let r: coil.Buffer = coil.fmax(n1, n2)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    // Both lanes NaN (the only NaN case for fmax): two NaN tokens.
    assert_eq!(
        stdout.matches("NaN").count(),
        2,
        "expected fmax(nan,nan) = [NaN, NaN] (BOTH-NaN is the only fmax NaN); got stdout=\n{stdout}",
    );
}

// =====================================================================
// CHAIN — a maximum-result feeds the next op (proving the result handle is a
// first-class drop-scheduled Buffer). maximum(a, b) then transpose: the (2,)
// pick result is consumed by a 1-D transpose (a no-op on rank 1, but it
// proves the fresh Buffer is a valid handle for a follow-on op).
// =====================================================================

/// `coil.transpose(coil.maximum(a, b))` — the maximum result is a
/// first-class Buffer consumed by the next op; both temporaries drop.
/// `maximum([1,8],[5,2]) = [5, 8]`; transpose of a 1-D is unchanged.
#[test]
fn test_e2e_transpose_of_maximum() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 8.0)\n",
        "    let b: coil.Buffer = coil.array1d2(5.0, 2.0)\n",
        "    let r: coil.Buffer = coil.maximum(a, b)\n", // [5, 8]
        "    let t: coil.Buffer = coil.transpose(r)\n",
        "    let _ = coil.print_buffer(t)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([5, 8], dtype=float64)"),
        "expected transpose∘maximum(1-D unchanged) = [5, 8]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE (RUNTIME) — non-conformable pair aborts cleanly (numpy raises
// ValueError; the shim `coil_panic`s = a clean trap, never a C-ABI unwind).
// a is (2,) but b is (2,2): the shape mismatch aborts.
// =====================================================================

/// `coil.maximum(a(2,), b(2,2))` traps (non-conformable: equal-shape
/// combine contract). The binary exits NON-zero (the `__cobrust_panic`
/// abort path) rather than producing a garbage buffer or unwinding across
/// the C-ABI. numpy would broadcast — coil raises (a tracked follow-up).
#[test]
fn test_e2e_maximum_nonconformable_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n", // (2,)
        "    let b: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n", // (2,2) — mismatch
        "    let r: coil.Buffer = coil.maximum(a, b)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, _stderr, ok) = run(&exe);
    assert!(
        !ok,
        "expected non-conformable maximum to TRAP (non-zero exit); \
         got success with stdout=\n{stdout}",
    );
}
