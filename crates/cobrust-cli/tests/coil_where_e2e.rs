//! coil `where` op (`coil.where(cond, a, b)`) — `.cb` end-to-end proof for
//! the #145 BATCH-8 addition: the THREE-Buffer elementwise conditional
//! select, the FIRST coil ecosystem fn borrowing THREE handles. It EXTENDS
//! the 2-Buffer combine ops (`concatenate` / `vstack` / `hstack` /
//! `coil.linalg.solve`) to a third borrowed arg — same borrow-Buffer-args →
//! fresh-Buffer-return value-handle ABI, plus one more borrow.
//!
//! ## The load-bearing semantics
//!
//! - `coil.where(cond, a, b)` selects elementwise: `result[i] = cond[i]
//!   truthy ? a[i] : b[i]`. This is the 3-arg `np.where(cond, a, b)` form
//!   (NOT the 1-arg `np.where(cond)` index form, which is a separate
//!   deferral).
//! - `cond` is typically a Bool-dtype Buffer from a `a < b` comparison
//!   (ADR-0077); a numeric cond is truthy on any nonzero element.
//! - The result dtype is `a`'s dtype (`a` and `b` must match). A NaN in
//!   `a`/`b` flows through as a SELECTED value.
//! - All three operands must share one shape (the equal-shape contract;
//!   numpy broadcasts — a tracked follow-up). A non-conformable triple
//!   `coil_panic`s (numpy raises `ValueError`) = a clean trap, NEVER a
//!   C-ABI unwind.
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     "where")` resolves the `(Buffer, Buffer, Buffer) -> Buffer` `EcoSig`;
//!   - MIR `try_lower_ecosystem_call` Case-1 (module free fn) → the GENERIC
//!     `emit_ecosystem_call` borrow-args → fresh-Buffer-return path (the
//!     SAME path as the 2-Buffer `coil.concatenate(a, b)`, iterating
//!     `sig.params` regardless of arity; NO where-specific MIR arm, NO
//!     `_=>"any"` gap — 3 Buffer args is the same path as 2);
//!   - codegen externs (`llvm_backend.rs`) — the FIRST `(ptr,ptr,ptr)->ptr`
//!     coil extern (`coil_select3_ty`);
//!   - the cabi shim (`cabi.rs`) — `__cobrust_coil_where` borrows THREE
//!     handles, returns a fresh Boxed `Buffer`, and `coil_panic`s on a
//!     non-conformable / dtype-mismatch triple (NEVER unwinding across the
//!     C-ABI).
//!
//! Mirrors the compile→spawn→assert-stdout harness of
//! `coil_manipulate_e2e.rs`. Results are observed via `coil.print_buffer`,
//! whose float64 repr renders integer-valued floats WITHOUT a `.0` suffix
//! (e.g. `array([10, 40], dtype=float64)`).

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
    let out = Command::new(exe).output().expect("spawn coil-where prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — the canonical mixed mask. cond=[True,False] (built from a
// REAL `a < b` comparison), where(cond, x, y) picks x[0] then y[1]. The
// mixed (one True, one False) mask rules out a degenerate all-x / all-y
// fill — and proves the bool-mask integration END-TO-END.
// =====================================================================

/// `let cond = a < b` (a Bool-dtype Buffer) feeding `coil.where`. With
/// `a=[1,5]`, `b=[3,2]` → `cond=[True,False]`; `where(cond, [10,20],
/// [30,40])` → `[10, 40]` (lane 0 True → 10, lane 1 False → 40). This is
/// the load-bearing bool-mask integration: the comparison result is a
/// first-class Buffer passed straight as `coil.where`'s first arg.
///
/// Oracle (numpy 2.x): `c = np.array([1.,5.]) < np.array([3.,2.])` →
/// `[True, False]`; `np.where(c, [10.,20.], [30.,40.])` → `[10., 40.]`.
#[test]
fn test_e2e_where_cond_from_comparison() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 5.0)\n",
        "    let b: coil.Buffer = coil.array1d2(3.0, 2.0)\n",
        "    let cond: coil.Buffer = a < b\n",
        "    let x: coil.Buffer = coil.array1d2(10.0, 20.0)\n",
        "    let y: coil.Buffer = coil.array1d2(30.0, 40.0)\n",
        "    let r: coil.Buffer = coil.where(cond, x, y)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([10, 40], dtype=float64)"),
        "expected where(a<b, [10,20], [30,40]) = [10, 40]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — all-true cond returns `a` verbatim. cond = ([1,1] < [2,2]) =
// [True, True]; where(cond, x, y) -> x.
// =====================================================================

/// All-true cond → every lane picks `a`. `cond=[True,True]`;
/// `where(cond, [11,22], [33,44])` → `[11, 22]`.
#[test]
fn test_e2e_where_all_true_picks_a() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let lo: coil.Buffer = coil.array1d2(1.0, 1.0)\n",
        "    let hi: coil.Buffer = coil.array1d2(2.0, 2.0)\n",
        "    let cond: coil.Buffer = lo < hi\n", // [True, True]
        "    let x: coil.Buffer = coil.array1d2(11.0, 22.0)\n",
        "    let y: coil.Buffer = coil.array1d2(33.0, 44.0)\n",
        "    let r: coil.Buffer = coil.where(cond, x, y)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([11, 22], dtype=float64)"),
        "expected all-true where → a = [11, 22]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — all-false cond returns `b` verbatim. cond = ([2,2] < [1,1])
// = [False, False]; where(cond, x, y) -> y.
// =====================================================================

/// All-false cond → every lane picks `b`. `cond=[False,False]`;
/// `where(cond, [11,22], [33,44])` → `[33, 44]`.
#[test]
fn test_e2e_where_all_false_picks_b() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let lo: coil.Buffer = coil.array1d2(2.0, 2.0)\n",
        "    let hi: coil.Buffer = coil.array1d2(1.0, 1.0)\n",
        "    let cond: coil.Buffer = lo < hi\n", // [False, False]
        "    let x: coil.Buffer = coil.array1d2(11.0, 22.0)\n",
        "    let y: coil.Buffer = coil.array1d2(33.0, 44.0)\n",
        "    let r: coil.Buffer = coil.where(cond, x, y)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([33, 44], dtype=float64)"),
        "expected all-false where → b = [33, 44]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — NaN in a/b FLOWS THROUGH as a selected value (it is selected,
// never inspected). cond=[True,False] (from a<b); a has NaN at lane 0
// (picked), b has NaN at lane 1 (picked) → result is [NaN, NaN].
// =====================================================================

/// NaN flows through `coil.where` as a value. With `cond=[True,False]`,
/// `a=[NaN, 2]`, `b=[5, NaN]` → lane 0 picks `a[0]=NaN`, lane 1 picks
/// `b[1]=NaN`, so the result is `[NaN, NaN]`. coil's float64 repr renders
/// `NaN` (Rust `Display`, print.rs). Pins that a NaN VALUE is selectable.
///
/// Oracle (numpy 2.x): `np.where([True,False],[nan,2.],[5.,nan])` →
/// `[nan, nan]` (coil's Rust-Display renders the capital `NaN`).
#[test]
fn test_e2e_where_nan_flows_through() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        // cond = [1,5] < [3,2] = [True, False].
        "    let p: coil.Buffer = coil.array1d2(1.0, 5.0)\n",
        "    let q: coil.Buffer = coil.array1d2(3.0, 2.0)\n",
        "    let cond: coil.Buffer = p < q\n",
        // a = [NaN, 2]  (NaN computed as 0.0/0.0 — no NaN literal needed).
        "    let zero: coil.Buffer = coil.array1d2(0.0, 2.0)\n",
        "    let aden: coil.Buffer = coil.array1d2(0.0, 1.0)\n",
        "    let a: coil.Buffer = zero / aden\n", // [0/0=NaN, 2/1=2]
        // b = [5, NaN].
        "    let bnum: coil.Buffer = coil.array1d2(5.0, 0.0)\n",
        "    let bden: coil.Buffer = coil.array1d2(1.0, 0.0)\n",
        "    let b: coil.Buffer = bnum / bden\n", // [5/1=5, 0/0=NaN]
        "    let r: coil.Buffer = coil.where(cond, a, b)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    // Both lanes are NaN: lane 0 picks a[0]=NaN, lane 1 picks b[1]=NaN.
    // coil renders f64 NaN as the capital `NaN` (Rust Display).
    assert_eq!(
        stdout.matches("NaN").count(),
        2,
        "expected both lanes NaN ([NaN, NaN]); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// CHAIN — a where-result feeds the next op (proving the result handle is a
// first-class drop-scheduled Buffer). where(cond, x, y) then transpose:
// the (2,) select result is consumed by a 1-D transpose (a no-op on rank
// 1, but it proves the fresh Buffer is a valid handle for a follow-on op).
// =====================================================================

/// `coil.transpose(coil.where(cond, x, y))` — the where result is a
/// first-class Buffer consumed by the next op; both temporaries drop.
/// cond=[True,False]; where → [10, 40]; transpose of a 1-D is unchanged.
#[test]
fn test_e2e_transpose_of_where() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 5.0)\n",
        "    let b: coil.Buffer = coil.array1d2(3.0, 2.0)\n",
        "    let cond: coil.Buffer = a < b\n",
        "    let x: coil.Buffer = coil.array1d2(10.0, 20.0)\n",
        "    let y: coil.Buffer = coil.array1d2(30.0, 40.0)\n",
        "    let r: coil.Buffer = coil.where(cond, x, y)\n",
        "    let t: coil.Buffer = coil.transpose(r)\n",
        "    let _ = coil.print_buffer(t)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([10, 40], dtype=float64)"),
        "expected transpose∘where(1-D unchanged) = [10, 40]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE (RUNTIME) — non-conformable triple aborts cleanly (numpy raises
// ValueError; the shim `coil_panic`s = a clean trap, never a C-ABI
// unwind). cond/x are (2,) but y is (2,2): the a/b shape mismatch aborts.
// =====================================================================

/// `coil.where(cond(2,), x(2,), y(2,2))` traps (non-conformable: x and y
/// have different shapes). The binary exits NON-zero (the
/// `__cobrust_panic` abort path) rather than producing a garbage buffer or
/// unwinding across the C-ABI.
#[test]
fn test_e2e_where_nonconformable_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let p: coil.Buffer = coil.array1d2(1.0, 5.0)\n",
        "    let q: coil.Buffer = coil.array1d2(3.0, 2.0)\n",
        "    let cond: coil.Buffer = p < q\n", // (2,)
        "    let x: coil.Buffer = coil.array1d2(10.0, 20.0)\n", // (2,)
        "    let y: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n", // (2,2) — mismatch
        "    let r: coil.Buffer = coil.where(cond, x, y)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, _stderr, ok) = run(&exe);
    assert!(
        !ok,
        "expected non-conformable where to TRAP (non-zero exit); \
         got success with stdout=\n{stdout}",
    );
}
