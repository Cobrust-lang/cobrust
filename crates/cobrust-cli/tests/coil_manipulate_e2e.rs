//! coil array-MANIPULATION ops (`transpose` / `flatten` / `ravel` /
//! `concatenate` / `vstack` / `hstack`) — `.cb` end-to-end proof for the
//! #145 BATCH-2 addition: the Buffer-RETURNING combine + reshape surface,
//! wired EXACTLY like the `@` matmul operator (borrow-Buffer-args →
//! fresh-Buffer-return), NOT the scalar-return stats.
//!
//! ## The load-bearing semantics
//!
//! - `coil.transpose(a)` reverses all axes: `(2,3) -> (3,2)`; a 1-D array
//!   is unchanged. Result is a fresh `coil.Buffer`.
//! - `coil.flatten(a)` / `coil.ravel(a)` collapse to a 1-D C-order copy.
//! - `coil.concatenate(a, b)` joins along axis 0 (`(2,3)+(2,3) -> (4,3)`).
//! - `coil.vstack(a, b)` stacks row-wise; a 1-D `(n,)` operand is promoted
//!   to `(1,n)` first.
//! - `coil.hstack(a, b)` stacks column-wise (`(2,3)+(2,3) -> (2,6)` for
//!   2-D; axis-0 concat for 1-D).
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     <op>)` resolves the `Buffer(...) -> Buffer` `EcoSig`;
//!   - MIR `try_lower_ecosystem_call` Case-1 (module free fn) → the GENERIC
//!     `emit_ecosystem_call` borrow-args → fresh-Buffer-return path (the
//!     SAME path as `coil.linalg.solve(a, b)`'s 2-Buffer-arg form; NO
//!     manipulation-specific MIR arm, NO `_=>"any"` gap);
//!   - codegen externs (`llvm_backend.rs`) — `(ptr)->ptr` for the 1-arg
//!     ops, `(ptr,ptr)->ptr` for the 2-array ops;
//!   - the cabi shims (`cabi.rs`) — `__cobrust_coil_<op>` returning a fresh
//!     Boxed `Buffer`; the 2-array shims `coil_panic` on a non-conformable
//!     / dtype-mismatch pair (NEVER unwinding across the C-ABI).
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_matmul_e2e.rs`.
//! Results are observed via `coil.print_buffer`, whose float64 repr renders
//! integer-valued floats WITHOUT a `.0` suffix (e.g.
//! `array([[1, 4], [2, 5], [3, 6]], dtype=float64)`).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_matmul_e2e.rs`.
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
    let out = Command::new(exe).output().expect("spawn coil-manip prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — transpose `(2,3) -> (3,2)`. Asymmetric values pin the axis
// reversal: a no-op / wrong layout yields a DIFFERENT body.
// =====================================================================

/// `coil.transpose(array2x3(1,2,3,4,5,6))` →
/// `[[1,4],[2,5],[3,6]]`, shape `(3,2)`.
///
/// Oracle (numpy 2.x): `np.array([[1.,2.,3.],[4.,5.,6.]]).T` →
/// `array([[1., 4.], [2., 5.], [3., 6.]])`.
#[test]
fn test_e2e_transpose_2x3() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let t: coil.Buffer = coil.transpose(a)\n",
        "    let _ = coil.print_buffer(t)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[1, 4], [2, 5], [3, 6]]") && stdout.contains("dtype=float64"),
        "expected transpose [[1,4],[2,5],[3,6]] (float64); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — flatten / ravel `(2,2) -> (4,)`.
// =====================================================================

/// `coil.flatten(array2x2(1,2,3,4))` → `[1,2,3,4]` (C-order, `(4,)`).
#[test]
fn test_e2e_flatten_2x2() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let f: coil.Buffer = coil.flatten(a)\n",
        "    let _ = coil.print_buffer(f)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 2, 3, 4], dtype=float64)"),
        "expected flatten [1,2,3,4]; got stdout=\n{stdout}",
    );
}

/// `coil.ravel(array2x2(5,6,7,8))` → `[5,6,7,8]` (same value contract as
/// flatten).
#[test]
fn test_e2e_ravel_2x2() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(5.0, 6.0, 7.0, 8.0)\n",
        "    let r: coil.Buffer = coil.ravel(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([5, 6, 7, 8], dtype=float64)"),
        "expected ravel [5,6,7,8]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — concatenate `(2,3)+(2,3) -> (4,3)` along axis 0.
// =====================================================================

/// `coil.concatenate(array2x3(...), array2x3(...))` → a `(4,3)` Buffer.
/// The 2-Buffer-arg → Buffer form proves the `coil.linalg.solve`-analogue
/// path (TWO borrowed handles, fresh return, all three dropped).
#[test]
fn test_e2e_concatenate_axis0() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let b: coil.Buffer = coil.array2x3(7.0, 8.0, 9.0, 10.0, 11.0, 12.0)\n",
        "    let c: coil.Buffer = coil.concatenate(a, b)\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    // The full 4-row body pins BOTH the values AND the (4,3) shape.
    assert!(
        stdout.contains("[[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]")
            && stdout.contains("dtype=float64"),
        "expected concatenate (4,3); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — vstack / hstack on 2-D.
// =====================================================================

/// `coil.vstack(array2x3, array2x3)` → `(4,3)` (row-wise stack; identical
/// to concatenate-axis-0 for 2-D inputs).
#[test]
fn test_e2e_vstack_2x3() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let b: coil.Buffer = coil.array2x3(7.0, 8.0, 9.0, 10.0, 11.0, 12.0)\n",
        "    let v: coil.Buffer = coil.vstack(a, b)\n",
        "    let _ = coil.print_buffer(v)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]")
            && stdout.contains("dtype=float64"),
        "expected vstack (4,3); got stdout=\n{stdout}",
    );
}

/// `coil.hstack(array2x3, array2x3)` → `(2,6)` (column-wise stack; axis 1
/// for 2-D). The row-interleave (`[1,2,3,7,8,9]`) distinguishes hstack
/// from concatenate/vstack — a wrong axis would give `(4,3)` instead.
#[test]
fn test_e2e_hstack_2x3() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let b: coil.Buffer = coil.array2x3(7.0, 8.0, 9.0, 10.0, 11.0, 12.0)\n",
        "    let h: coil.Buffer = coil.hstack(a, b)\n",
        "    let _ = coil.print_buffer(h)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[1, 2, 3, 7, 8, 9], [4, 5, 6, 10, 11, 12]]")
            && stdout.contains("dtype=float64"),
        "expected hstack (2,6); got stdout=\n{stdout}",
    );
}

// =====================================================================
// CHAIN — transpose ∘ concatenate (exercises a fresh-Buffer feeding the
// next op, proving the result handle is a first-class drop-scheduled
// Buffer). concatenate((2,3),(2,3))=(4,3), then transpose -> (3,4).
// =====================================================================

/// `coil.transpose(coil.concatenate(a, b))` → `(3,4)`. The intermediate
/// `(4,3)` concatenation is consumed by transpose; both temporaries drop.
#[test]
fn test_e2e_transpose_of_concatenate() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let b: coil.Buffer = coil.array2x3(7.0, 8.0, 9.0, 10.0, 11.0, 12.0)\n",
        "    let c: coil.Buffer = coil.concatenate(a, b)\n",
        "    let t: coil.Buffer = coil.transpose(c)\n",
        "    let _ = coil.print_buffer(t)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    // c=(4,3) col-major-read transposed: col0=[1,4,7,10], etc.
    assert!(
        stdout.contains("[[1, 4, 7, 10], [2, 5, 8, 11], [3, 6, 9, 12]]")
            && stdout.contains("dtype=float64"),
        "expected transpose∘concatenate (3,4); got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE (RUNTIME) — non-conformable concatenate aborts cleanly (numpy
// raises ValueError; the shim `coil_panic`s = a clean trap, never a
// C-ABI unwind). `(2,3)` concat `(2,2)` along axis 0: non-axis dim
// 3 != 2 -> abort.
// =====================================================================

/// `coil.concatenate((2,3), (2,2))` traps (non-conformable). The binary
/// exits NON-zero (the `__cobrust_panic` abort path) rather than producing
/// a garbage buffer or unwinding across the C-ABI.
#[test]
fn test_e2e_concatenate_nonconformable_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let b: coil.Buffer = coil.array2x2(7.0, 8.0, 9.0, 10.0)\n",
        "    let c: coil.Buffer = coil.concatenate(a, b)\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, _stderr, ok) = run(&exe);
    assert!(
        !ok,
        "expected non-conformable concatenate to TRAP (non-zero exit); \
         got success with stdout=\n{stdout}",
    );
}
