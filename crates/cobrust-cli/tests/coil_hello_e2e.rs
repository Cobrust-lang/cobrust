//! ADR-0072 8/8 first proof — end-to-end `.cb` source → compile → link →
//! run → stdout-assert for the `coil` ecosystem-import wiring (numpy
//! ndarray, ndarray foundation — the EIGHTH and final cobra-batch
//! ecosystem module; completes the workspace-vendored ecosystem chain).
//!
//! Generalizes the proven flat-intrinsic value-handle chain (the same
//! pattern den/molt/strike use) to the numpy surface: the `.cb` source
//! constructs a `coil.Buffer` via `coil.zeros(3)`, prints it via
//! `coil.print_buffer(b)`, and lets scope-exit drop it via
//! `__cobrust_coil_buffer_drop`. The handle wraps a Boxed `coil::Array`
//! which in turn owns its `ndarray::ArrayD<f64>` + its `Vec<f64>`.
//!
//! ```text
//! `import coil` + `coil.zeros(3)` + `coil.print_buffer(a)`
//!   → cobrust-types ecosystem manifest (typecheck)
//!   → cobrust-mir lowering (Str retarget → __cobrust_coil_*)
//!   → cobrust-codegen externs + handle drop
//!   → cobrust-coil C-ABI shims (libcoil.a)
//!   → cobrust-cli build.rs per-import static link (after libcobrust_stdlib.a)
//!   → binary prints the array_repr to stdout + returns exit 0
//!   → test asserts stdout contains `array(` + `dtype=` + exit 0
//! ```
//!
//! Pattern (mirrors `hood_cmd_e2e.rs` + `ecosystem_den_e2e.rs`): compile
//! a `.cb` program to an exe, run it with `std::process::Command`,
//! assert stdout + exit.
//!
//! Out-of-scope (per ADR-0072 §"coil deep operator/index"):
//! - `a + b` (BinOp dispatch for Buffer — deep operator work).
//! - `a[i]` (IndexExpr dispatch for Buffer — deep index work).
//! - `a.shape` (Attr access on handle — handle-attr sub-ADR).
//! - `Buffer.dot(other)` (multi-handle methods — chain extension).
//!
//! These all want their own sub-ADRs; this first proof scopes to
//! constructors + repr only.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable and return its path. The
/// caller spawns + asserts.
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

/// Compile the `.cb` "coil hello" program, run it, assert stdout +
/// exit code. The example mirrors `examples/coil_hello/main.cb`.
#[test]
fn test_e2e_coil_hello_zeros_round_trip() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(3)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);

    let out = Command::new(&exe).output().expect("spawn coil example");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    assert!(
        out.status.success(),
        "binary exit non-zero ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // coil's `array_repr` produces an `array(..., dtype=float64)`-style
    // string (numpy-compatible per ADR-0013 §4). We assert both the
    // shape markers + the dtype tag rather than an exact byte-string
    // (the inner-data formatter can drift; the shape + dtype are
    // contract-stable).
    assert!(
        stdout.contains("array("),
        "stdout must contain numpy-style `array(`; got:\n{stdout}\nstderr:\n{stderr}",
    );
    assert!(
        stdout.contains("dtype=float64"),
        "stdout must contain `dtype=float64`; got:\n{stdout}\nstderr:\n{stderr}",
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "exit code must be 0; got {:?}",
        out.status.code(),
    );
}

// =====================================================================
// Negative type-check cases — coil's `.cb` surface is pure value-
// handle (no callbacks), so the negative corpus targets the surface
// constraints that today's first-proof scope rejects.
// =====================================================================

/// Compile-only helper — returns (success?, stderr).
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

/// Unknown coil function — rejected at the type-check arm. Uses a
/// PERMANENTLY-fake name (`coil.no_such_function`) so the test stays valid
/// as the manifest grows: the original `coil.flatten` placeholder BECAME a
/// real op when the array-manipulation batch landed, silently flipping this
/// reject-path test green-to-red (F36 fixture-name-vs-behaviour). A name no
/// real numpy surface will ever claim keeps the reject path under test
/// regardless of future additions.
#[test]
fn test_neg_coil_rejects_unknown_function() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(3)\n",
        "    let b = coil.no_such_function(a)\n",
        "    let _ = coil.print_buffer(b)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "unknown function coil.no_such_function must be rejected; stderr=\n{stderr}"
    );
}

/// Wrong-typed argument — `coil.zeros` expects `i64`, not `str`. The
/// typechecker catches the mismatch at the manifest-driven signature
/// check.
#[test]
fn test_neg_coil_zeros_rejects_str_argument() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(\"three\")\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.zeros(\"three\") must be rejected (i64 expected); stderr=\n{stderr}"
    );
}
