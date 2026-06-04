//! `coil.reshape(a, rows, cols)` — `.cb` end-to-end proof for the #163
//! BATCH-18 addition: the 2-D C / row-major reshape (the ADR-0077 Q5
//! two-scalar-arg honest first proof; the shape-tuple `np.reshape(a, (m,n))`
//! form is a tracked deferral). Wired EXACTLY like `coil.broadcast_to(a, n)`
//! (`[Buffer, Int]`) but with ONE MORE `Int` arg — the generic
//! `try_lower_ecosystem_call` iterates the 3 sig params (Buffer, Int, Int)
//! over the SAME borrow-Buffer-arg → fresh-Buffer-return path, so there is
//! ZERO batch-specific MIR.
//!
//! ## The load-bearing semantics (numpy 2.4.6 confirmed via python3.11)
//!
//! - `coil.reshape(a, rows, cols)` flattens `a` in **C order** then re-lays
//!   it out as `(rows, cols)`: `array2x3([[1,2,3],[4,5,6]])` → reshape(3,2)
//!   → `[[1,2],[3,4],[5,6]]` (NOT column-major). Dtype + values preserved.
//! - `-1` inference: exactly one of `rows` / `cols` may be `-1`, inferred as
//!   `a.size() / (the other)` (`reshape(-1, 2)` on 6 → `(3, 2)`).
//! - A bad shape (size mismatch, both `-1`, a `-1` with a non-divisor other,
//!   a non-`-1` dim `<= 0`) **traps** at runtime — the shim `coil_panic`s
//!   (numpy `ValueError`), aborting NON-zero, NEVER unwinding across the
//!   C-ABI.
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     "reshape")` resolves the `[Buffer, Int, Int] -> Buffer` `EcoSig`;
//!   - MIR `try_lower_ecosystem_call` Case-1 (module free fn) → the GENERIC
//!     `emit_ecosystem_call` borrow-args → fresh-Buffer-return path (the
//!     SAME path as `coil.broadcast_to(a, n)`, +1 Int arg; NO reshape-
//!     specific MIR arm);
//!   - codegen extern (`llvm_backend.rs`) — `(ptr, i64, i64) -> ptr`;
//!   - the cabi shim (`cabi.rs`) — `__cobrust_coil_reshape` returning a fresh
//!     Boxed `Buffer`; a bad shape `coil_panic`s.
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_manipulate_e2e`.
//! Results are observed via `coil.print_buffer`, whose float64 repr renders
//! integer-valued floats WITHOUT a `.0` suffix.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative doc comments read as "lazy" list items to clippy; they
// are intentional explanatory prose, not lint targets.
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
    let out = Command::new(exe).output().expect("spawn coil-reshape prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Compile-only helper for negative typecheck cases — returns (success?,
/// stderr).
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
// POSITIVE — reshape `(2,3) -> (3,2)`, C-order. `array2x3` is a 6-element
// buffer `[[1,2,3],[4,5,6]]`; flatten C-order = `[1..6]`, re-laid (3,2) =
// `[[1,2],[3,4],[5,6]]`. A column-major layout would give a DIFFERENT body.
// =====================================================================

/// `coil.reshape(array2x3(1,2,3,4,5,6), 3, 2)` → `[[1,2],[3,4],[5,6]]`,
/// shape `(3,2)`.
///
/// Oracle (numpy 2.x): `np.array([[1.,2.,3.],[4.,5.,6.]]).reshape(3,2)` →
/// `array([[1., 2.], [3., 4.], [5., 6.]])`.
#[test]
fn test_e2e_reshape_2x3_to_3x2() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let r: coil.Buffer = coil.reshape(a, 3, 2)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[1, 2], [3, 4], [5, 6]]") && stdout.contains("dtype=float64"),
        "expected reshape (3,2) C-order [[1,2],[3,4],[5,6]]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `-1` inference. reshape(-1, 2) on a 6-element buffer infers
// rows = 6 / 2 = 3 → `(3, 2)` (same layout as the explicit form above).
// =====================================================================

/// `coil.reshape(array2x3(...), -1, 2)` → `(3,2)` (rows inferred).
///
/// Oracle (numpy 2.x): `np.arange(6).reshape(3,2)` layout; `-1` resolves to
/// `6 / 2 = 3`.
#[test]
fn test_e2e_reshape_neg_one_inference() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let r: coil.Buffer = coil.reshape(a, -1, 2)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[1, 2], [3, 4], [5, 6]]") && stdout.contains("dtype=float64"),
        "expected reshape(-1,2) inferred (3,2); got stdout=\n{stdout}",
    );
}

// =====================================================================
// CHAIN — reshape feeds a downstream op (proving the fresh result is a
// first-class drop-scheduled Buffer). reshape((2,3),3,2) then transpose ->
// (2,3) with a transposed body.
// =====================================================================

/// `coil.transpose(coil.reshape(a, 3, 2))` → `(2,3)`. The intermediate
/// `(3,2)` reshape is consumed by transpose; both temporaries drop.
#[test]
fn test_e2e_reshape_then_transpose() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let r: coil.Buffer = coil.reshape(a, 3, 2)\n",
        "    let t: coil.Buffer = coil.transpose(r)\n",
        "    let _ = coil.print_buffer(t)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    // r=(3,2)=[[1,2],[3,4],[5,6]]; transpose -> col-read = [[1,3,5],[2,4,6]].
    assert!(
        stdout.contains("[[1, 3, 5], [2, 4, 6]]") && stdout.contains("dtype=float64"),
        "expected transpose∘reshape (2,3) [[1,3,5],[2,4,6]]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE (RUNTIME) — size mismatch aborts cleanly. reshape((2,3),2,4) on a
// 6-element buffer: 2*4=8 != 6 -> numpy ValueError -> the shim `coil_panic`s
// (a clean trap, NON-zero exit, NEVER a C-ABI unwind).
// =====================================================================

/// `coil.reshape(array2x3(...), 2, 4)` traps (size 6 cannot become 8). The
/// binary exits NON-zero (the `__cobrust_panic` abort path) rather than
/// producing a garbage buffer or unwinding across the C-ABI.
#[test]
fn test_e2e_reshape_size_mismatch_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let r: coil.Buffer = coil.reshape(a, 2, 4)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, _stderr, ok) = run(&exe);
    assert!(
        !ok,
        "expected size-mismatch reshape to TRAP (non-zero exit); \
         got success with stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE (TYPECHECK) — a wrong-typed arg is rejected at the manifest-
// driven signature check. `coil.reshape`'s 2nd/3rd params are `i64`, not
// `str`; the typechecker catches the mismatch BEFORE codegen (the §2.5
// compile-time-catch path).
// =====================================================================

/// `coil.reshape(a, "two", 3)` is rejected (i64 expected for `rows`). A
/// type error, NOT a runtime trap — surfaced at the type-check arm.
#[test]
fn test_neg_reshape_rejects_str_dim() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let r: coil.Buffer = coil.reshape(a, \"two\", 3)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.reshape(a, \"two\", 3) must be rejected (i64 expected); stderr=\n{stderr}"
    );
}
