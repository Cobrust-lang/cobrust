//! `coil.astype(a, dtype)` — `.cb` end-to-end proof for the BATCH-19
//! addition: the DTYPE-CONVERSION op (`a.astype('int64')` /
//! `a.astype('float64')`, the op LLMs reach for constantly). This is the
//! SURFACE-COMPLETION proof: coil HAS int-dtype Buffers (`Array::Int64`,
//! `print.rs` emits `dtype=int64`) but had NO `.cb` way to CREATE one —
//! `astype` is it. These tests compile → link → spawn REAL binaries and
//! assert that astyping a FLOAT buffer to `int64` produces a Buffer whose
//! `print_buffer` repr shows `dtype=int64` + TRUNCATED int values, proving
//! int-dtype Buffer creation from `.cb` works END-TO-END.
//!
//! ## The NEW thing — a `Str` ARGUMENT on the coil surface
//!
//! The runtime `dtype` name crosses the C-ABI as a `*mut Str` buffer
//! pointer — the EXACT ABI dora's `event.send_output(output_id: Str,
//! payload: Str)` uses. The codegen materialises the `.cb`-side string
//! literal as a stdlib `Str` buffer + passes its pointer; the
//! `__cobrust_coil_astype` shim reads it via the `__cobrust_str_ptr` /
//! `__cobrust_str_len` stdlib ABI (mirrored VERBATIM from dora). NO new
//! string convention, ZERO new MIR (the generic `try_lower_ecosystem_call`
//! Case-1 path auto-borrows BOTH the Buffer arg and the Str arg).
//!
//! ## The load-bearing semantics (numpy 2.4.6 confirmed via python3.11)
//!
//! - `coil.astype(a, "int64")` TRUNCATES TOWARD ZERO:
//!   `[1.7, -1.7].astype('int64') == [1, -1]` (`-1.7 → -1`, NOT the `-2` a
//!   FLOOR would give — the e2e asserts `-1`, so a floor-mutation FAILS).
//! - `coil.astype(a, "bool")` is `x != 0` (`0.0 → False`, nonzero → True).
//! - An UNKNOWN dtype string `coil_panic`s at runtime — the binary exits
//!   NON-zero (the `__cobrust_panic` abort), NEVER unwinds across the
//!   C-ABI, NEVER a silent wrong cast.
//! - A NON-Str dtype arg (e.g. `astype(a, 5)`) is a COMPILE-TIME reject
//!   (the §2.5 compile-time-catch path — `unify_call_arg` against `Ty::Str`).
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_reshape_e2e`.
//! Results are observed via `coil.print_buffer`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative doc comments read as "lazy" list items to clippy; they
// are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_reshape_e2e.rs`.
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
    let out = Command::new(exe).output().expect("spawn coil-astype prog");
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
// POSITIVE — the SURFACE-COMPLETION proof: float → int64 creates an
// int-dtype Buffer from `.cb`. `array1d2(1.7, -1.7)` is a Float64 `[1.7,
// -1.7]`; `astype("int64")` TRUNCATES TOWARD ZERO → an Int64 `[1, -1]`
// (`print_buffer` renders `dtype=int64`). A FLOOR mutation would give
// `[1, -2]`, so the `-1` assertion FAILS it.
// =====================================================================

/// `coil.astype(coil.array1d2(1.7, -1.7), "int64")` → `array([1, -1],
/// dtype=int64)`. THIS is int-dtype Buffer creation from `.cb`, end-to-end.
///
/// Oracle (numpy 2.x): `np.array([1.7,-1.7]).astype('int64') == [1, -1]`.
#[test]
fn test_e2e_astype_float_to_int64_truncates_toward_zero() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.7, -1.7)\n",
        "    let r: coil.Buffer = coil.astype(a, \"int64\")\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("dtype=int64"),
        "expected an int64-dtype Buffer (the surface-completion); got stdout=\n{stdout}",
    );
    assert!(
        stdout.contains("[1, -1]"),
        "expected TRUNCATE-toward-zero [1, -1] (floor would give [1, -2]); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — float → int64 over a 2-D buffer (astype is shape-agnostic).
// `array2x3(1.7, 2.2, 3.9, -1.7, -2.2, -3.9)` → astype("int64") →
// `[[1, 2, 3], [-1, -2, -3]]` (every lane truncates toward zero).
// =====================================================================

/// `coil.astype(array2x3(...), "int64")` truncates every element toward
/// zero, preserving the `(2,3)` shape, as an int64-dtype Buffer.
///
/// Oracle (numpy 2.x): `np.array([[1.7,2.2,3.9],[-1.7,-2.2,-3.9]])
/// .astype('int64') == [[1,2,3],[-1,-2,-3]]`.
#[test]
fn test_e2e_astype_2d_float_to_int64() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.7, 2.2, 3.9, -1.7, -2.2, -3.9)\n",
        "    let r: coil.Buffer = coil.astype(a, \"int64\")\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[[1, 2, 3], [-1, -2, -3]]") && stdout.contains("dtype=int64"),
        "expected 2-D int64 trunc [[1,2,3],[-1,-2,-3]]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — float → bool (`x != 0`). `array1d2(0.0, 2.0)` → astype("bool")
// → `array([False, True], dtype=bool)` (`0.0 → False`, nonzero → True).
// Proves astype creates a Bool-dtype Buffer too.
// =====================================================================

/// `coil.astype(array1d2(0.0, 2.0), "bool")` → `array([False, True],
/// dtype=bool)`.
///
/// Oracle (numpy 2.x): `np.array([0.0,2.0]).astype(bool) == [False, True]`.
#[test]
fn test_e2e_astype_to_bool() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(0.0, 2.0)\n",
        "    let r: coil.Buffer = coil.astype(a, \"bool\")\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[False, True]") && stdout.contains("dtype=bool"),
        "expected bool cast [False, True]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — same-dtype copy round-trips (numpy's copy=True default).
// `array1d2(1.5, 2.5)` → astype("float64") → a value-identical Float64.
// =====================================================================

/// `coil.astype(array1d2(1.5, 2.5), "float64")` → `array([1.5, 2.5],
/// dtype=float64)` (a copy; values + dtype unchanged).
#[test]
fn test_e2e_astype_same_dtype_copy() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.5, 2.5)\n",
        "    let r: coil.Buffer = coil.astype(a, \"float64\")\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("[1.5, 2.5]") && stdout.contains("dtype=float64"),
        "expected same-dtype copy [1.5, 2.5] float64; got stdout=\n{stdout}",
    );
}

// =====================================================================
// CHAIN — the int64 result is a first-class drop-scheduled Buffer that
// feeds a downstream op. astype→int64 then transpose (shape-only) keeps the
// int64 dtype: `array2x3(...).astype("int64").transpose()`.
// =====================================================================

/// `coil.transpose(coil.astype(array2x3(...), "int64"))` → a `(3,2)`
/// int64 Buffer (the int dtype survives the downstream transpose; both
/// temporaries drop).
#[test]
fn test_e2e_astype_int64_then_transpose() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.7, 2.2, 3.9, -1.7, -2.2, -3.9)\n",
        "    let r: coil.Buffer = coil.astype(a, \"int64\")\n",
        "    let t: coil.Buffer = coil.transpose(r)\n",
        "    let _ = coil.print_buffer(t)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    // r=(2,3)=[[1,2,3],[-1,-2,-3]]; transpose -> (3,2) col-read =
    // [[1,-1],[2,-2],[3,-3]], still int64.
    assert!(
        stdout.contains("[[1, -1], [2, -2], [3, -3]]") && stdout.contains("dtype=int64"),
        "expected transpose∘astype (3,2) int64 [[1,-1],[2,-2],[3,-3]]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE (RUNTIME) — an UNKNOWN dtype string aborts cleanly. The shim
// `coil_panic`s (numpy-style raise); the binary exits NON-zero (the
// `__cobrust_panic` abort), NEVER a silent wrong cast, NEVER a C-ABI
// unwind.
// =====================================================================

/// `coil.astype(a, "not_a_dtype")` traps at runtime — the binary exits
/// NON-zero rather than producing a garbage Buffer.
#[test]
fn test_e2e_astype_unknown_dtype_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let r: coil.Buffer = coil.astype(a, \"not_a_dtype\")\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, _stderr, ok) = run(&exe);
    assert!(
        !ok,
        "expected unknown-dtype astype to TRAP (non-zero exit); \
         got success with stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE (TYPECHECK) — a non-Str dtype arg is rejected at the manifest-
// driven signature check. `coil.astype`'s 2nd param is `str`, NOT `i64`;
// the typechecker catches `astype(a, 5)` BEFORE codegen (the §2.5
// compile-time-catch path — `unify_call_arg` against `Ty::Str`).
// =====================================================================

/// `coil.astype(a, 5)` is rejected (str expected for `dtype`). A type
/// error, NOT a runtime trap — surfaced at the type-check arm.
#[test]
fn test_neg_astype_rejects_non_str_dtype() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let r: coil.Buffer = coil.astype(a, 5)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.astype(a, 5) must be rejected (str expected for dtype); stderr=\n{stderr}"
    );
}
